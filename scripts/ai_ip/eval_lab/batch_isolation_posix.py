import os
import secrets
import stat
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


_DIRECTORY_FLAGS = (
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


class SecureFilesystemError(OSError):
    pass


def identity(metadata: os.stat_result) -> tuple[int, int]:
    return metadata.st_dev, metadata.st_ino


def handle_identity(fd: int) -> tuple[int, int]:
    return identity(os.fstat(fd))


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


def open_directory(path: Path) -> int:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise SecureFilesystemError("absolute directory path required")
    current = -1
    try:
        current = os.open(candidate.anchor, _DIRECTORY_FLAGS)
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
            parent = os.open("..", _DIRECTORY_FLAGS, dir_fd=current)
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
    fd = os.open(name, _DIRECTORY_FLAGS, dir_fd=parent_fd)
    _require_directory(fd, identity(before))
    return fd


def _mkdir_child(parent_fd: int, name: str) -> int:
    os.mkdir(name, 0o700, dir_fd=parent_fd)
    fd = _open_child(parent_fd, name)
    os.fchmod(fd, 0o700)
    return fd


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
    quarantine_name: str | None = None

    @classmethod
    def create(cls, base: Path, root_name: str) -> "PosixCellFilesystem":
        if not all(
            function in os.supports_dir_fd
            for function in (os.open, os.mkdir, os.stat, os.unlink, os.rmdir)
        ) or not getattr(os, "O_NOFOLLOW", 0):
            raise SecureFilesystemError("secure POSIX directory primitives unavailable")
        base_fd = open_directory(base)
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
                identity(os.fstat(base_fd)),
                identity(os.fstat(root_fd)),
                directories,
                identities,
            )
        except BaseException:
            for fd in directories.values():
                os.close(fd)
            if root_fd >= 0:
                os.close(root_fd)
            os.close(base_fd)
            raise

    def validate(self, *, cleanup: bool = False) -> None:
        _require_directory(self.base_fd, self.base_identity)
        _require_directory(self.root_fd, self.root_identity)
        entry_name = (
            self.quarantine_name if cleanup and self.quarantine_name else self.root_name
        )
        entry = os.stat(entry_name, dir_fd=self.base_fd, follow_symlinks=False)
        if identity(entry) != self.root_identity or not stat.S_ISDIR(entry.st_mode):
            raise SecureFilesystemError("attempt root identity was substituted")
        if cleanup and self.quarantine_name:
            return
        for name, expected in self.directory_identities.items():
            _require_directory(self.directories[name], expected)
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
        _write_file(self.root_fd, name, payload)

    def read_file(self, name: str, limit: int) -> bytes:
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
        pending: list[tuple[str, ...]] = [()]
        files: set[tuple[int, int]] = set()
        entries = 0
        while pending:
            relative = pending.pop()
            if len(relative) > _MAX_DELETE_DEPTH:
                raise SecureFilesystemError("cell traversal exceeds depth bound")
            directory_fd = self._open_relative(relative)
            try:
                for name in os.listdir(directory_fd):
                    entries += 1
                    if entries > _MAX_DELETE_ENTRIES:
                        raise SecureFilesystemError(
                            "cell traversal exceeds entry bound"
                        )
                    metadata = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
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
        self.validate(cleanup=True)
        if self.quarantine_name is None:
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
        self._delete_contents()
        os.rmdir(self.quarantine_name, dir_fd=self.base_fd)
        os.close(self.root_fd)
        os.close(self.base_fd)
        self.root_fd = -1
        self.base_fd = -1

    def _delete_contents(self) -> None:
        pending: list[tuple[str, ...]] = [()]
        directories: list[tuple[str, ...]] = []
        entries = 0
        exhausted = False
        while pending:
            relative = pending.pop()
            if len(relative) > _MAX_DELETE_DEPTH:
                raise SecureFilesystemError("cleanup exceeds depth bound")
            directory_fd = self._open_relative(relative)
            try:
                for name in os.listdir(directory_fd):
                    entries += 1
                    if entries > _MAX_DELETE_ENTRIES:
                        exhausted = True
                        break
                    metadata = os.stat(name, dir_fd=directory_fd, follow_symlinks=False)
                    if stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(
                        metadata.st_mode
                    ):
                        child = (*relative, name)
                        pending.append(child)
                        directories.append(child)
                    else:
                        os.unlink(name, dir_fd=directory_fd)
            finally:
                os.close(directory_fd)
            if exhausted:
                break
        for relative in sorted(directories, key=len, reverse=True):
            parent_fd = self._open_relative(relative[:-1])
            try:
                try:
                    os.rmdir(relative[-1], dir_fd=parent_fd)
                except OSError:
                    if not exhausted:
                        raise
            finally:
                os.close(parent_fd)
        if exhausted:
            raise SecureFilesystemError(
                "cleanup reached its bounded progress limit; retry is required"
            )

    def close(self) -> None:
        for fd in self.directories.values():
            try:
                os.close(fd)
            except OSError:
                pass
        self.directories.clear()
        for fd in (self.root_fd, self.base_fd):
            if fd >= 0:
                try:
                    os.close(fd)
                except OSError:
                    pass
