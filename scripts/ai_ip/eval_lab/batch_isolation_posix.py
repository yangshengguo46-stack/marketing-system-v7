import os
import secrets
import stat
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

try:
    from . import batch_isolation_posix_terminal as terminal
except ImportError:
    import batch_isolation_posix_terminal as terminal


_DIR_FLAGS = (
    os.O_RDONLY
    | getattr(os, "O_DIRECTORY", 0)
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_CLOEXEC", 0)
)
_FILE_FLAGS = (
    os.O_WRONLY
    | os.O_CREAT
    | os.O_EXCL
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_CLOEXEC", 0)
)
_MAX_DELETE_ENTRIES = 100_000
_MAX_DELETE_DEPTH = 2_048
_MAX_SCAN_BYTES = 1024 * 1024 * 1024
_MAX_SCAN_SECONDS = 10.0
_MAX_TERMINAL_SCAN_ENTRIES = 100_000
_MAX_TERMINAL_SCAN_SECONDS = 10.0


class SecureFilesystemError(OSError):
    pass


identity = terminal.identity
handle_identity = terminal.handle_identity


def close_handle(fd: int) -> None:
    os.close(fd)


def _require_directory(
    fd: int, expected: tuple[int, int] | None = None
) -> tuple[int, int]:
    metadata = os.fstat(fd)
    actual = identity(metadata)
    if not stat.S_ISDIR(metadata.st_mode):
        raise SecureFilesystemError("secure directory handle required")
    if expected is not None and actual != expected:
        raise SecureFilesystemError("directory identity changed while opening")
    return actual


def _require_private_directory(
    fd: int, expected: tuple[int, int], description: str
) -> None:
    metadata = os.fstat(fd)
    if (
        identity(metadata) != expected
        or not stat.S_ISDIR(metadata.st_mode)
        or metadata.st_uid != os.getuid()
        or stat.S_IMODE(metadata.st_mode) != 0o700
    ):
        raise SecureFilesystemError(f"required layout {description} is not private")


def open_directory(path: Path) -> int:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise SecureFilesystemError("absolute directory path required")
    current = -1
    try:
        current = os.open(candidate.anchor, _DIR_FLAGS)
        _require_directory(current)
        for part in candidate.parts[1:]:
            child = _open_child(current, part)
            os.close(current)
            current = child
        result = current
        current = -1
        return result
    except SecureFilesystemError:
        raise
    except OSError as error:
        raise SecureFilesystemError("cannot securely open directory") from error
    finally:
        if current >= 0:
            os.close(current)


def ancestor_identities(fd: int) -> frozenset[tuple[int, int]]:
    current = os.dup(fd)
    found: set[tuple[int, int]] = set()
    try:
        for _ in range(2_048):
            child = _require_directory(current)
            found.add(child)
            parent = os.open("..", _DIR_FLAGS, dir_fd=current)
            parent_identity = _require_directory(parent)
            if parent_identity == child:
                os.close(parent)
                return frozenset(found)
            os.close(current)
            current = parent
        raise SecureFilesystemError("filesystem ancestry exceeds safety bound")
    finally:
        os.close(current)


def _open_child(parent_fd: int, name: str) -> int:
    before = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    if stat.S_ISLNK(before.st_mode) or not stat.S_ISDIR(before.st_mode):
        raise SecureFilesystemError("directory entry is not a safe directory")
    fd = os.open(name, _DIR_FLAGS, dir_fd=parent_fd)
    _require_directory(fd, identity(before))
    return fd


def _mkdir_child(parent_fd: int, name: str) -> int:
    os.mkdir(name, 0o700, dir_fd=parent_fd)
    fd = -1
    try:
        fd = _open_child(parent_fd, name)
        os.fchmod(fd, 0o700)
        result = fd
        fd = -1
        return result
    except BaseException:
        if fd >= 0:
            os.close(fd)
        try:
            os.rmdir(name, dir_fd=parent_fd)
        except OSError:
            pass
        raise


def _write_file(parent_fd: int, name: str, payload: bytes) -> None:
    fd = os.open(name, _FILE_FLAGS, 0o600, dir_fd=parent_fd)
    try:
        os.fchmod(fd, 0o600)
        view = memoryview(payload)
        while view:
            written = os.write(fd, view)
            if written <= 0:
                raise SecureFilesystemError("short secure write")
            view = view[written:]
        os.fsync(fd)
        metadata = os.fstat(fd)
        if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
            raise SecureFilesystemError("written file is not independent")
    finally:
        os.close(fd)


@dataclass
class PosixCellFilesystem:
    base_path: Path
    root_name: str
    base_fd: int
    root_fd: int
    base_identity: tuple[int, int]
    root_identity: tuple[int, int]
    directories: dict[str, int]
    directory_identities: dict[str, tuple[int, int]]
    creator_pid: int
    quarantine_name: str | None = None
    terminal_unlinked: bool = False
    terminal_proof: terminal.TerminalProof | None = None

    @classmethod
    def create(cls, base: Path, root_name: str, base_fd: int) -> "PosixCellFilesystem":
        if not all(
            function in os.supports_dir_fd
            for function in (os.open, os.mkdir, os.stat, os.unlink, os.rmdir)
        ) or not getattr(os, "O_NOFOLLOW", 0):
            raise SecureFilesystemError("secure POSIX directory primitives unavailable")
        base_identity = handle_identity(base_fd)
        root_fd = -1
        directories: dict[str, int] = {}
        try:
            os.mkdir(root_name, 0o700, dir_fd=base_fd)
            root_fd = _open_child(base_fd, root_name)
            os.fchmod(root_fd, 0o700)
            for name in ("home", "workspace", "cache", "temp", "logs", "promptfoo"):
                directories[name] = _mkdir_child(root_fd, name)
            promptfoo_cache = _mkdir_child(directories["cache"], "promptfoo")
            directories["cache/promptfoo"] = promptfoo_cache
            identities = {
                name: identity(os.fstat(fd)) for name, fd in directories.items()
            }
            return cls(
                base,
                root_name,
                base_fd,
                root_fd,
                base_identity,
                identity(os.fstat(root_fd)),
                directories,
                identities,
                os.getpid(),
            )
        except BaseException:
            if "cache" in directories:
                try:
                    os.rmdir("promptfoo", dir_fd=directories["cache"])
                except OSError:
                    pass
            if root_fd >= 0:
                for name in ("home", "workspace", "cache", "temp", "logs", "promptfoo"):
                    try:
                        os.rmdir(name, dir_fd=root_fd)
                    except OSError:
                        pass
                try:
                    os.rmdir(root_name, dir_fd=base_fd)
                except OSError:
                    pass
            for fd in directories.values():
                close_handle(fd)
            if root_fd >= 0:
                close_handle(root_fd)
            close_handle(base_fd)
            raise

    def _require_owner(self) -> None:
        if self.creator_pid != os.getpid():
            raise SecureFilesystemError("POSIX cell belongs to another process")

    def validate(self, *, cleanup: bool = False) -> None:
        self._require_owner()
        _require_directory(self.base_fd, self.base_identity)
        _require_private_directory(self.root_fd, self.root_identity, "root")
        if cleanup and self.terminal_unlinked:
            try:
                terminal.require_parent(self.root_fd, self.base_identity, _DIR_FLAGS)
            except terminal.TerminalProofError as error:
                raise SecureFilesystemError(str(error)) from error
            return
        entry_name = (
            self.quarantine_name if cleanup and self.quarantine_name else self.root_name
        )
        entry = os.stat(entry_name, dir_fd=self.base_fd, follow_symlinks=False)
        if identity(entry) != self.root_identity or not stat.S_ISDIR(entry.st_mode):
            raise SecureFilesystemError("attempt root identity was substituted")
        if cleanup and self.quarantine_name:
            return
        for name in ("home", "workspace", "cache", "temp", "logs", "promptfoo"):
            expected = self.directory_identities[name]
            _require_private_directory(self.directories[name], expected, name)
            live = os.stat(name, dir_fd=self.root_fd, follow_symlinks=False)
            if identity(live) != expected or not stat.S_ISDIR(live.st_mode):
                raise SecureFilesystemError(f"required layout {name} identity changed")
        _require_private_directory(
            self.directories["cache/promptfoo"],
            self.directory_identities["cache/promptfoo"],
            "cache/promptfoo",
        )
        try:
            cache_entry = os.stat(
                "promptfoo", dir_fd=self.directories["cache"], follow_symlinks=False
            )
        except OSError as error:
            raise SecureFilesystemError(
                "required layout cache/promptfoo is unavailable"
            ) from error
        if identity(cache_entry) != self.directory_identities["cache/promptfoo"]:
            raise SecureFilesystemError("required layout cache/promptfoo changed")

    def base_is_still_bound(self) -> bool:
        try:
            return identity(self.base_path.lstat()) == self.base_identity
        except OSError:
            return False

    def write_snapshot(self, files: Iterable[tuple[str, bytes]], target: str) -> None:
        self._require_owner()
        target_fd = self.directories[target]
        created: dict[tuple[str, ...], int] = {(): target_fd}
        owned: list[int] = []
        try:
            for relative_text, payload in sorted(files):
                parts = Path(relative_text).parts
                if not parts or any(part in {"", ".", ".."} for part in parts):
                    raise SecureFilesystemError("snapshot contains unsafe path")
                parent: tuple[str, ...] = ()
                for part in parts[:-1]:
                    child = (*parent, part)
                    if child not in created:
                        try:
                            fd = _mkdir_child(created[parent], part)
                        except FileExistsError:
                            fd = _open_child(created[parent], part)
                        created[child] = fd
                        owned.append(fd)
                    parent = child
                _write_file(created[parent], parts[-1], payload)
        finally:
            for fd in reversed(owned):
                os.close(fd)

    def create_file(self, name: str, payload: bytes) -> None:
        self._require_owner()
        _write_file(self.root_fd, name, payload)

    def read_file(self, name: str, limit: int) -> bytes:
        self._require_owner()
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_CLOEXEC", 0)
        fd = os.open(name, flags, dir_fd=self.root_fd)
        try:
            metadata = os.fstat(fd)
            if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
                raise SecureFilesystemError("receipt seal is not an independent file")
            payload = os.read(fd, limit + 1)
            if len(payload) > limit or os.read(fd, 1):
                raise SecureFilesystemError("receipt seal exceeds safety bound")
            return payload
        finally:
            os.close(fd)

    def scan_regular_identities(self) -> frozenset[tuple[int, int]]:
        self._require_owner()
        pending: list[tuple[str, ...]] = [()]
        files: set[tuple[int, int]] = set()
        entries = 0
        total_bytes = 0
        started = time.monotonic()
        while pending:
            relative = pending.pop()
            if len(relative) > _MAX_DELETE_DEPTH:
                raise SecureFilesystemError("cell traversal exceeds depth bound")
            directory_fd = self._open_relative(relative)
            try:
                with os.scandir(directory_fd) as children:
                    for child in children:
                        name = child.name
                        entries += 1
                        if entries > _MAX_DELETE_ENTRIES:
                            raise SecureFilesystemError(
                                "cell traversal exceeds entry bound"
                            )
                        if time.monotonic() - started > _MAX_SCAN_SECONDS:
                            raise SecureFilesystemError(
                                "cell traversal exceeds time bound"
                            )
                        metadata = os.stat(
                            name, dir_fd=directory_fd, follow_symlinks=False
                        )
                        total_bytes += metadata.st_size
                        if total_bytes > _MAX_SCAN_BYTES:
                            raise SecureFilesystemError(
                                "cell traversal exceeds byte bound"
                            )
                        if stat.S_ISLNK(metadata.st_mode):
                            raise SecureFilesystemError("cell contains a symbolic link")
                        if stat.S_ISDIR(metadata.st_mode):
                            pending.append((*relative, name))
                        elif stat.S_ISREG(metadata.st_mode):
                            if metadata.st_nlink != 1:
                                raise SecureFilesystemError("cell contains a hard link")
                            files.add(identity(metadata))
                        else:
                            raise SecureFilesystemError("cell contains an unsafe entry")
            finally:
                os.close(directory_fd)
        return frozenset(files)

    def _open_relative(self, parts: tuple[str, ...]) -> int:
        current = os.dup(self.root_fd)
        try:
            for part in parts:
                child = _open_child(current, part)
                os.close(current)
                current = child
            return current
        except BaseException:
            os.close(current)
            raise

    def delete_exact(self) -> None:
        if not self.terminal_unlinked:
            self.validate(cleanup=True)
        if self.quarantine_name is None and not self.terminal_unlinked:
            quarantine = f".deleting-{secrets.token_hex(16)}"
            os.rename(
                self.root_name,
                quarantine,
                src_dir_fd=self.base_fd,
                dst_dir_fd=self.base_fd,
            )
            renamed = os.stat(quarantine, dir_fd=self.base_fd, follow_symlinks=False)
            if identity(renamed) != self.root_identity:
                raise SecureFilesystemError("cleanup quarantine identity changed")
            self.quarantine_name = quarantine
            for fd in self.directories.values():
                os.close(fd)
            self.directories.clear()
        if not self.terminal_unlinked:
            self._delete_contents()
            before = os.stat(
                self.quarantine_name, dir_fd=self.base_fd, follow_symlinks=False
            )
            if identity(before) != self.root_identity:
                raise SecureFilesystemError("cleanup quarantine identity changed")
            os.rmdir(self.quarantine_name, dir_fd=self.base_fd)
            self.terminal_unlinked = True
        try:
            if self.terminal_proof is None:
                self.terminal_proof = terminal.TerminalProof.start(self, _DIR_FLAGS)
            self.terminal_proof.advance(
                self.root_fd,
                _MAX_TERMINAL_SCAN_ENTRIES,
                _MAX_TERMINAL_SCAN_SECONDS,
            )
            self.terminal_proof = None
        except terminal.TerminalProofError as error:
            if self.terminal_proof is not None and self.terminal_proof.scan_fd < 0:
                self.terminal_proof = None
            raise SecureFilesystemError(str(error)) from error
        os.close(self.root_fd)
        os.close(self.base_fd)
        self.root_fd = -1
        self.base_fd = -1

    def _delete_contents(self) -> None:
        pending: list[tuple[str, ...]] = [()]
        entries = 0
        total_bytes = 0
        started = time.monotonic()
        while pending:
            relative = pending.pop()
            if len(relative) > _MAX_DELETE_DEPTH:
                raise SecureFilesystemError("cleanup exceeds depth bound")
            directory_fd = self._open_relative(relative)
            child_directory: tuple[str, ...] | None = None
            try:
                with os.scandir(directory_fd) as children:
                    for child_entry in children:
                        name = child_entry.name
                        entries += 1
                        metadata = os.stat(
                            name, dir_fd=directory_fd, follow_symlinks=False
                        )
                        total_bytes += metadata.st_size
                        if (
                            entries > _MAX_DELETE_ENTRIES
                            or total_bytes > _MAX_SCAN_BYTES
                            or time.monotonic() - started > _MAX_SCAN_SECONDS
                        ):
                            raise SecureFilesystemError(
                                "cleanup reached its bounded progress limit; retry is required"
                            )
                        if stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(
                            metadata.st_mode
                        ):
                            child_directory = (*relative, name)
                            break
                        else:
                            os.unlink(name, dir_fd=directory_fd)
            finally:
                os.close(directory_fd)
            if child_directory is not None:
                pending.append(relative)
                pending.append(child_directory)
            elif relative:
                parent_fd = self._open_relative(relative[:-1])
                try:
                    os.rmdir(relative[-1], dir_fd=parent_fd)
                finally:
                    os.close(parent_fd)

    def close(self) -> None:
        if self.terminal_proof is not None:
            self.terminal_proof.close()
            self.terminal_proof = None
        for fd in self.directories.values():
            try:
                os.close(fd)
            except OSError:
                pass
        self.directories.clear()
        root_fd, base_fd = self.root_fd, self.base_fd
        self.root_fd = -1
        self.base_fd = -1
        for fd in (root_fd, base_fd):
            if fd >= 0:
                try:
                    os.close(fd)
                except OSError:
                    pass
