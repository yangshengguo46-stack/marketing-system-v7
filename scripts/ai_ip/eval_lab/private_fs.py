import os
import stat
import subprocess
from contextlib import contextmanager
from dataclasses import dataclass, field
from pathlib import Path
from typing import Iterator

if __package__:
    from .contracts import (
        LabContractError,
        _load_exact_json_bytes,
        canonical_json_bytes,
        sha256_json,
    )
else:
    from contracts import (
        LabContractError,
        _load_exact_json_bytes,
        canonical_json_bytes,
        sha256_json,
    )


_MODULE_REPO_ROOT = Path(__file__).resolve().parents[3]
_DIRECTORY_FLAGS = os.O_RDONLY | getattr(os, "O_DIRECTORY", 0)
_READ_FLAGS = os.O_RDONLY
_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
_CLOEXEC = getattr(os, "O_CLOEXEC", 0)
_RECEIPT_NAME = ".ai-ip-private-root-v1.json"


def _git_bytes(repo: Path, *arguments: str) -> bytes:
    try:
        completed = subprocess.run(
            ["git", "-C", str(repo), *arguments],
            check=False,
            capture_output=True,
        )
    except OSError as error:
        raise LabContractError("unable to execute Git worktree query") from error
    if completed.returncode != 0:
        raise LabContractError("unable to resolve Git worktrees")
    return completed.stdout


def _repository_context() -> tuple[Path, tuple[Path, ...]]:
    try:
        expected_repo = _MODULE_REPO_ROOT.resolve(strict=True)
    except OSError as error:
        raise LabContractError("repository root is unavailable") from error
    top_level_raw = _git_bytes(expected_repo, "rev-parse", "--show-toplevel")
    if b"\0" in top_level_raw or top_level_raw.count(b"\n") != 1:
        raise LabContractError("ambiguous Git repository root")
    top_level = Path(os.fsdecode(top_level_raw[:-1]))
    if not top_level.is_absolute() or top_level.resolve(strict=True) != expected_repo:
        raise LabContractError("module is not in its expected Git repository")

    payload = _git_bytes(expected_repo, "worktree", "list", "--porcelain", "-z")
    if not payload.endswith(b"\0\0"):
        raise LabContractError("ambiguous Git worktree listing")
    worktrees: list[Path] = []
    for record in payload[:-2].split(b"\0\0"):
        fields = record.split(b"\0")
        if not fields or not fields[0].startswith(b"worktree "):
            raise LabContractError("invalid Git worktree listing")
        raw_path = fields[0][len(b"worktree ") :]
        path = Path(os.fsdecode(raw_path))
        if not raw_path or not path.is_absolute() or ".." in path.parts:
            raise LabContractError("invalid Git worktree path")
        normalized = Path(os.path.abspath(path)).resolve(strict=False)
        if normalized in worktrees:
            raise LabContractError("duplicate Git worktree path")
        worktrees.append(normalized)
    if expected_repo not in worktrees:
        raise LabContractError("current Git worktree is missing from listing")
    return expected_repo, tuple(worktrees)


def _reject_symlink_ancestors(path: Path) -> None:
    current = Path(path.anchor)
    for part in path.parts[1:]:
        current /= part
        try:
            metadata = current.lstat()
        except FileNotFoundError:
            continue
        except OSError as error:
            raise LabContractError(
                f"cannot inspect path ancestor: {current}"
            ) from error
        if stat.S_ISLNK(metadata.st_mode):
            raise LabContractError(f"symlink path component is forbidden: {current}")


def _is_within(path: Path, directory: Path) -> bool:
    return path == directory or directory in path.parents


def _prepare_root_path(path: Path) -> tuple[Path, Path, tuple[Path, ...]]:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise LabContractError("private root must be absolute")
    _reject_symlink_ancestors(candidate)
    candidate = candidate.resolve(strict=False)
    repo_root, worktrees = _repository_context()
    if any(_is_within(candidate, worktree) for worktree in worktrees):
        raise LabContractError("private root must be outside every Git worktree")
    return candidate, repo_root, worktrees


def _owner_uid() -> int:
    get_euid = getattr(os, "geteuid", None)
    if get_euid is None:
        raise LabContractError("owner checks are unavailable on this platform")
    return get_euid()


def _validate_directory(metadata: os.stat_result, description: str) -> None:
    if not stat.S_ISDIR(metadata.st_mode):
        raise LabContractError(f"{description} must be a directory")
    if metadata.st_uid != _owner_uid():
        raise LabContractError(f"{description} must be owned by the current user")
    if stat.S_IMODE(metadata.st_mode) != 0o700:
        raise LabContractError(f"{description} mode must be 0700")


def _validate_file(metadata: os.stat_result, description: str) -> None:
    if not stat.S_ISREG(metadata.st_mode):
        raise LabContractError(f"{description} must be a regular file")
    if metadata.st_uid != _owner_uid():
        raise LabContractError(f"{description} must be owned by the current user")
    if metadata.st_nlink != 1:
        raise LabContractError(f"{description} must have exactly one hard link")
    if stat.S_IMODE(metadata.st_mode) != 0o600:
        raise LabContractError(f"{description} mode must be 0600")


def _created_file_metadata(descriptor: int, description: str) -> os.stat_result:
    metadata = os.fstat(descriptor)
    _validate_file(metadata, description)
    return metadata


def _write_and_fsync(descriptor: int, payload: bytes, description: str) -> None:
    view = memoryview(payload)
    while view:
        written = os.write(descriptor, view)
        if written <= 0:
            raise LabContractError(f"short write for {description}")
        view = view[written:]
    os.fsync(descriptor)


def _open_trusted_parent(root: Path) -> int:
    parent = root.parent
    _reject_symlink_ancestors(parent)
    try:
        before = parent.lstat()
        if stat.S_ISLNK(before.st_mode):
            raise LabContractError("private root parent may not be a symlink")
        descriptor = os.open(parent, _DIRECTORY_FLAGS | _NOFOLLOW | _CLOEXEC)
    except OSError as error:
        raise LabContractError("cannot safely open private root parent") from error
    try:
        after = os.fstat(descriptor)
        if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
            raise LabContractError("private root parent changed while opening")
        if not stat.S_ISDIR(after.st_mode) or after.st_uid != _owner_uid():
            raise LabContractError("private root parent must be an owner directory")
        if stat.S_IMODE(after.st_mode) & 0o022:
            raise LabContractError(
                "private root parent must not be group/world writable"
            )
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _lstat_at(parent_fd: int, name: str) -> os.stat_result:
    try:
        return os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except (NotImplementedError, TypeError) as error:
        raise LabContractError(
            "no safe no-follow stat operation is available"
        ) from error


def _open_checked_at(
    parent_fd: int,
    name: str,
    flags: int,
    *,
    directory: bool,
    expected: os.stat_result | None = None,
) -> int:
    before = _lstat_at(parent_fd, name)
    if stat.S_ISLNK(before.st_mode):
        raise LabContractError(f"symlink path component is forbidden: {name}")
    try:
        descriptor = os.open(name, flags | _NOFOLLOW | _CLOEXEC, dir_fd=parent_fd)
    except OSError as error:
        raise LabContractError(f"cannot safely open private path: {name}") from error
    try:
        after = os.fstat(descriptor)
        if (before.st_dev, before.st_ino) != (after.st_dev, after.st_ino):
            raise LabContractError(f"private path changed while opening: {name}")
        if expected is not None and (expected.st_dev, expected.st_ino) != (
            after.st_dev,
            after.st_ino,
        ):
            raise LabContractError(f"created private path was replaced: {name}")
        if directory:
            _validate_directory(after, name)
        else:
            _validate_file(after, name)
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _relative_parts(relative: str | Path) -> tuple[str, ...]:
    path = Path(relative)
    if path.is_absolute() or path.anchor or not path.parts:
        raise LabContractError("private artifact path must be non-empty and relative")
    if any(part in ("", ".", "..") for part in path.parts):
        raise LabContractError("private artifact path may not traverse")
    if _RECEIPT_NAME in path.parts:
        raise LabContractError("private root completion receipt name is reserved")
    return path.parts


@dataclass(frozen=True)
class PrivateRoot:
    path: Path
    repo_root: Path
    _worktrees: tuple[Path, ...] = field(repr=False)

    @classmethod
    def create_new(cls, path: Path) -> "PrivateRoot":
        root, repo_root, worktrees = _prepare_root_path(path)
        parent_fd = _open_trusted_parent(root)
        try:
            try:
                os.mkdir(root.name, 0o700, dir_fd=parent_fd)
            except OSError as error:
                raise LabContractError("cannot create private root") from error
            instance = cls(root, repo_root, worktrees)
            created: os.stat_result | None = None
            descriptor: int | None = None
            try:
                created = _lstat_at(parent_fd, root.name)
                descriptor = _open_checked_at(
                    parent_fd,
                    root.name,
                    _DIRECTORY_FLAGS,
                    directory=True,
                    expected=created,
                )
                instance._create_completion_receipt(descriptor, created)
            except BaseException:
                if descriptor is not None:
                    os.close(descriptor)
                cls._remove_created_dir(parent_fd, root.name, created)
                raise
            os.close(descriptor)
        finally:
            os.close(parent_fd)
        return instance

    @classmethod
    def open_existing(cls, path: Path) -> "PrivateRoot":
        root, repo_root, worktrees = _prepare_root_path(path)
        instance = cls(root, repo_root, worktrees)
        descriptor = instance._open_root()
        os.close(descriptor)
        return instance

    def _open_root(self, expected: os.stat_result | None = None) -> int:
        parent_fd = _open_trusted_parent(self.path)
        try:
            descriptor = _open_checked_at(
                parent_fd,
                self.path.name,
                _DIRECTORY_FLAGS,
                directory=True,
                expected=expected,
            )
        finally:
            os.close(parent_fd)
        try:
            self._validate_completion_receipt(descriptor, os.fstat(descriptor))
            return descriptor
        except BaseException:
            os.close(descriptor)
            raise

    @staticmethod
    def _receipt_value(
        root: os.stat_result, receipt: os.stat_result
    ) -> dict[str, object]:
        return {
            "formatVersion": 1,
            "receiptDevice": receipt.st_dev,
            "receiptInode": receipt.st_ino,
            "rootDevice": root.st_dev,
            "rootInode": root.st_ino,
        }

    def _create_completion_receipt(self, root_fd: int, root: os.stat_result) -> None:
        flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | _NOFOLLOW | _CLOEXEC
        try:
            descriptor = os.open(_RECEIPT_NAME, flags, 0o600, dir_fd=root_fd)
        except OSError as error:
            raise LabContractError("cannot create private root receipt") from error
        created: os.stat_result | None = None
        try:
            created = _created_file_metadata(descriptor, "private root receipt")
            payload = canonical_json_bytes(self._receipt_value(root, created)) + b"\n"
            _write_and_fsync(descriptor, payload, "private root receipt")
        except BaseException:
            try:
                os.close(descriptor)
            except OSError:
                pass
            if created is not None:
                self._unlink_if_same(root_fd, _RECEIPT_NAME, created)
            raise
        os.close(descriptor)
        try:
            self._validate_completion_receipt(root_fd, root)
        except BaseException:
            self._unlink_if_same(root_fd, _RECEIPT_NAME, created)
            raise

    def _validate_completion_receipt(self, root_fd: int, root: os.stat_result) -> None:
        try:
            payload, receipt = self._read_checked_file(root_fd, _RECEIPT_NAME)
        except OSError as error:
            raise LabContractError("private root receipt is unavailable") from error
        if not payload.endswith(b"\n"):
            raise LabContractError("private root receipt must end with one LF")
        value = _load_exact_json_bytes(payload[:-1])
        if value != self._receipt_value(root, receipt):
            raise LabContractError("private root receipt identity mismatch")
        if canonical_json_bytes(value) + b"\n" != payload:
            raise LabContractError("private root receipt is not canonical")

    @contextmanager
    def _parent(self, relative: str | Path) -> Iterator[tuple[int, str]]:
        parts = _relative_parts(relative)
        descriptor = self._open_root()
        try:
            for part in parts[:-1]:
                next_descriptor = _open_checked_at(
                    descriptor, part, _DIRECTORY_FLAGS, directory=True
                )
                os.close(descriptor)
                descriptor = next_descriptor
            yield descriptor, parts[-1]
        finally:
            os.close(descriptor)

    def create_dir(self, relative: str | Path) -> Path:
        with self._parent(relative) as (parent_fd, name):
            try:
                os.mkdir(name, 0o700, dir_fd=parent_fd)
            except OSError as error:
                raise LabContractError(
                    f"cannot create private directory: {relative}"
                ) from error
            created: os.stat_result | None = None
            try:
                created = _lstat_at(parent_fd, name)
                descriptor = _open_checked_at(
                    parent_fd,
                    name,
                    _DIRECTORY_FLAGS,
                    directory=True,
                    expected=created,
                )
                os.close(descriptor)
            except BaseException:
                self._remove_created_dir(parent_fd, name, created)
                raise
        return self.path.joinpath(*_relative_parts(relative))

    def write_new_json(self, relative: str | Path, value: object) -> None:
        canonical = canonical_json_bytes(value)
        payload = canonical + b"\n"
        expected_sha = sha256_json(value)
        with self._parent(relative) as (parent_fd, name):
            flags = os.O_CREAT | os.O_EXCL | os.O_WRONLY | _NOFOLLOW | _CLOEXEC
            try:
                descriptor = os.open(name, flags, 0o600, dir_fd=parent_fd)
            except OSError as error:
                raise LabContractError(
                    f"cannot create private JSON: {relative}"
                ) from error
            created: os.stat_result | None = None
            try:
                created = _created_file_metadata(descriptor, str(relative))
                _write_and_fsync(descriptor, payload, "private JSON")
            except BaseException:
                try:
                    os.close(descriptor)
                except OSError:
                    pass
                if created is not None:
                    self._unlink_if_same(parent_fd, name, created)
                raise
            try:
                os.close(descriptor)
            except OSError as error:
                self._unlink_if_same(parent_fd, name, created)
                raise LabContractError(
                    "cannot close private JSON after fsync"
                ) from error

            try:
                retained, _ = self._read_checked_file(parent_fd, name, expected=created)
                if retained != payload:
                    raise LabContractError("retained private JSON differs from write")
                parsed = _load_exact_json_bytes(retained[:-1])
                if sha256_json(parsed) != expected_sha:
                    raise LabContractError("retained private JSON SHA-256 mismatch")
            except BaseException:
                self._unlink_if_same(parent_fd, name, created)
                raise

    def _read_checked_file(
        self,
        parent_fd: int,
        name: str,
        *,
        expected: os.stat_result | None = None,
    ) -> tuple[bytes, os.stat_result]:
        descriptor = _open_checked_at(
            parent_fd,
            name,
            _READ_FLAGS,
            directory=False,
            expected=expected,
        )
        metadata = os.fstat(descriptor)
        try:
            chunks: list[bytes] = []
            while True:
                chunk = os.read(descriptor, 1024 * 1024)
                if not chunk:
                    return b"".join(chunks), metadata
                chunks.append(chunk)
        finally:
            os.close(descriptor)

    @staticmethod
    def _unlink_if_same(parent_fd: int, name: str, expected: os.stat_result) -> None:
        try:
            current = _lstat_at(parent_fd, name)
            if (current.st_dev, current.st_ino) == (expected.st_dev, expected.st_ino):
                os.unlink(name, dir_fd=parent_fd)
        except (FileNotFoundError, OSError):
            pass

    @staticmethod
    def _remove_created_dir(
        parent_fd: int, name: str, expected: os.stat_result | None
    ) -> None:
        if expected is None:
            return
        try:
            current = _lstat_at(parent_fd, name)
            same_inode = (current.st_dev, current.st_ino) == (
                expected.st_dev,
                expected.st_ino,
            )
            if same_inode and stat.S_ISDIR(current.st_mode):
                os.rmdir(name, dir_fd=parent_fd)
        except OSError:
            pass

    def read_json(self, relative: str | Path) -> object:
        with self._parent(relative) as (parent_fd, name):
            payload, _ = self._read_checked_file(parent_fd, name)
        if not payload.endswith(b"\n"):
            raise LabContractError("private JSON must end with one LF")
        value = _load_exact_json_bytes(payload[:-1])
        if canonical_json_bytes(value) + b"\n" != payload:
            raise LabContractError("private JSON is not canonical")
        return value
