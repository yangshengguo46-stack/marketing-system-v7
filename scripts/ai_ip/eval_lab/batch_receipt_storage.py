"""Descriptor-bound, no-follow POSIX storage for private batch evidence."""

import json
import os
import re
import stat
from dataclasses import dataclass
from pathlib import Path

try:
    from .contracts import LabContractError, canonical_json_bytes
except ImportError:
    from contracts import LabContractError, canonical_json_bytes


MAX_CONTEXT_BYTES = 2 * 1024 * 1024
MAX_PRIVATE_FILE_BYTES = 16 * 1024 * 1024
_IDENTIFIER = re.compile(r"[0-9a-f]{64}\Z")


class SecureStorageError(ValueError):
    pass


@dataclass
class BoundDirectory:
    """A retained owner-only directory identity for one evidence lifecycle."""

    path: Path
    descriptor: int
    identity: tuple[int, int]
    closed: bool = False

    @classmethod
    def open(cls, path: Path) -> "BoundDirectory":
        descriptor = open_private_directory(path)
        state = os.fstat(descriptor)
        return cls(Path(path), descriptor, (state.st_dev, state.st_ino))

    def verify(self) -> None:
        if self.closed:
            raise SecureStorageError("private evidence capability is closed")
        try:
            state = self.path.lstat()
            bound = os.fstat(self.descriptor)
        except OSError as error:
            raise SecureStorageError(
                "private evidence capability is unavailable"
            ) from error
        if (state.st_dev, state.st_ino) != self.identity or (
            bound.st_dev,
            bound.st_ino,
        ) != self.identity:
            raise SecureStorageError("private evidence directory identity was replaced")

    def close(self) -> None:
        if not self.closed:
            os.close(self.descriptor)
            self.closed = True


def directory_path(directory: Path | BoundDirectory) -> Path:
    return directory.path if isinstance(directory, BoundDirectory) else Path(directory)


def bind_directory(path: Path) -> BoundDirectory:
    return BoundDirectory.open(Path(path))


def identifier(value: object, name: str) -> str:
    if type(value) is not str or _IDENTIFIER.fullmatch(value) is None:
        raise SecureStorageError(f"invalid {name}")
    return value


def _open_directory(path: Path, *, private: bool) -> int:
    target = Path(path)
    if not target.is_absolute():
        raise SecureStorageError("private root must be absolute")
    flags = os.O_RDONLY | os.O_DIRECTORY | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(target.anchor, flags)
    try:
        for name in target.parts[1:]:
            child = os.open(name, flags, dir_fd=descriptor)
            os.close(descriptor)
            descriptor = child
        state = os.fstat(descriptor)
        if not stat.S_ISDIR(state.st_mode):
            raise SecureStorageError("private evidence entry is not a directory")
        if private and (
            stat.S_IMODE(state.st_mode) != 0o700 or state.st_uid != os.getuid()
        ):
            raise SecureStorageError(
                "private evidence directory ownership or mode is unsafe"
            )
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def open_private_directory(path: Path) -> int:
    """Return a bound descriptor for an owner-only private directory."""
    return _open_directory(Path(path), private=True)


def private_directory(path: Path, *, create: bool = False) -> Path:
    target = Path(path)
    if create:
        parent_fd = -1
        try:
            parent_fd = _open_directory(target.parent, private=False)
            try:
                os.mkdir(target.name, mode=0o700, dir_fd=parent_fd)
            except FileExistsError:
                pass
        except OSError as error:
            raise SecureStorageError(
                "private evidence directory is unavailable"
            ) from error
        finally:
            if parent_fd >= 0:
                os.close(parent_fd)
    try:
        descriptor = _open_directory(target, private=True)
    except (OSError, SecureStorageError) as error:
        raise SecureStorageError(
            "private evidence directory is unavailable, linked, or unsafe"
        ) from error
    os.close(descriptor)
    return target


def child_directory(parent: Path, name: str, *, exclusive: bool) -> Path:
    if not name or "/" in name or "\\" in name:
        raise SecureStorageError("invalid private evidence directory name")
    descriptor = _open_directory(parent, private=True)
    try:
        try:
            os.mkdir(name, mode=0o700, dir_fd=descriptor)
        except FileExistsError:
            if exclusive:
                raise SecureStorageError(f"{name} already exists")
    except OSError as error:
        raise SecureStorageError(
            "private evidence directory creation failed"
        ) from error
    finally:
        os.close(descriptor)
    return private_directory(Path(parent) / name)


def entry_exists(parent: Path, name: str) -> bool:
    descriptor = _open_directory(parent, private=True)
    try:
        try:
            os.stat(name, dir_fd=descriptor, follow_symlinks=False)
        except FileNotFoundError:
            return False
        return True
    finally:
        os.close(descriptor)


def write_exclusive(path: Path, payload: bytes) -> None:
    if len(payload) > MAX_PRIVATE_FILE_BYTES:
        raise SecureStorageError("private evidence exceeds the write bound")
    parent_fd = _open_directory(Path(path).parent, private=True)
    descriptor = -1
    try:
        descriptor = os.open(
            Path(path).name,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=parent_fd,
        )
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise SecureStorageError("private evidence write failed")
            view = view[written:]
        os.fsync(descriptor)
    except OSError as error:
        raise SecureStorageError(
            f"private evidence write collision: {Path(path).name}"
        ) from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        os.close(parent_fd)


def write_entry(directory: Path | BoundDirectory, name: str, payload: bytes) -> None:
    if not name or "/" in name or "\\" in name:
        raise SecureStorageError("invalid private evidence file name")
    if not isinstance(directory, BoundDirectory):
        write_exclusive(Path(directory) / name, payload)
        return
    if len(payload) > MAX_PRIVATE_FILE_BYTES:
        raise SecureStorageError("private evidence exceeds the write bound")
    directory.verify()
    descriptor = -1
    try:
        descriptor = os.open(
            name,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
            dir_fd=directory.descriptor,
        )
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise SecureStorageError("private evidence write failed")
            view = view[written:]
        os.fsync(descriptor)
        directory.verify()
    except OSError as error:
        raise SecureStorageError(f"private evidence write collision: {name}") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def seal_failure_tombstone(
    pair_directory: Path | BoundDirectory,
    pair_id: str,
    phase: str,
    error: BaseException,
) -> None:
    payload = (
        canonical_json_bytes(
            {
                "errorType": type(error).__name__,
                "pairId": pair_id,
                "phase": phase,
                "status": "failed",
            }
        )
        + b"\n"
    )
    write_entry(pair_directory, "failure.json", payload)


def read_bounded(path: Path, limit: int) -> bytes:
    parent_fd = _open_directory(Path(path).parent, private=True)
    descriptor = -1
    try:
        descriptor = os.open(
            Path(path).name,
            os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0),
            dir_fd=parent_fd,
        )
        before = os.fstat(descriptor)
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
            raise SecureStorageError("private evidence exceeds the read bound")
        chunks: list[bytes] = []
        remaining = before.st_size
        while remaining:
            chunk = os.read(descriptor, min(1024 * 1024, remaining))
            if not chunk:
                raise SecureStorageError("private evidence changed while read")
            chunks.append(chunk)
            remaining -= len(chunk)
        if os.read(descriptor, 1):
            raise SecureStorageError("private evidence changed while read")
        after = os.fstat(descriptor)
        if (before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
            after.st_mtime_ns,
        ):
            raise SecureStorageError("private evidence changed while read")
        return b"".join(chunks)
    except OSError as error:
        raise SecureStorageError("private evidence is unavailable") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        os.close(parent_fd)


def load_canonical(path: Path, limit: int = MAX_PRIVATE_FILE_BYTES) -> object:
    payload = read_bounded(path, limit)
    try:
        value = json.loads(payload)
        expected = canonical_json_bytes(value) + b"\n"
    except (json.JSONDecodeError, UnicodeDecodeError, LabContractError) as error:
        raise SecureStorageError("private evidence is invalid") from error
    if payload != expected:
        raise SecureStorageError("private evidence bytes are not canonical")
    return value
