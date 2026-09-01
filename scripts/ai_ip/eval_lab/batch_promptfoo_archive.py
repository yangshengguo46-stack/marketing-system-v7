"""Descriptor-bound streaming archive acquisition for the Promptfoo adapter."""

import gzip
import hashlib
import json
import os
import stat
import struct
from dataclasses import dataclass
from pathlib import Path


MAX_CHUNK_BYTES = 16 * 1024 * 1024
MAX_CHUNKS = 29
MAX_FILES = 100_000
MAX_ENTRIES = 200_000
MAX_DEPTH = 128
MAX_FILE_BYTES = 384 * 1024 * 1024
MAX_UNPACKED_BYTES = 2 * 1024 * 1024 * 1024
MAX_PATH_BYTES = 4096
MAX_HEADER_BYTES = 4096
MAX_PACKAGE_BYTES = 1024 * 1024
_READ_CHUNK_BYTES = 1024 * 1024
_PACKAGE_PATH = "node_modules/promptfoo/package.json"
_ENTRYPOINT = "node_modules/promptfoo/dist/src/entrypoint.js"
_HAS_DESCRIPTOR_RELATIVE_OPEN = os.open in os.supports_dir_fd
_HAS_DESCRIPTOR_RELATIVE_STAT = os.stat in os.supports_dir_fd
_HAS_DESCRIPTOR_SCANDIR = os.scandir in os.supports_fd


class PromptfooFilesystemError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class ArchiveBuild:
    chunks: tuple[bytes, ...]
    tree_sha256: str
    file_count: int
    unpacked_bytes: int


def _state(metadata: os.stat_result) -> tuple[int, int, int, int, int, int, int]:
    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_mode,
        metadata.st_nlink,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def _require_backend() -> None:
    required_flags = ("O_NOFOLLOW", "O_DIRECTORY", "O_NONBLOCK")
    if (
        os.name != "posix"
        or any(not getattr(os, name, 0) for name in required_flags)
        or not _HAS_DESCRIPTOR_RELATIVE_OPEN
        or not _HAS_DESCRIPTOR_RELATIVE_STAT
        or not _HAS_DESCRIPTOR_SCANDIR
        or not callable(getattr(os, "pread", None))
    ):
        raise PromptfooFilesystemError(
            "descriptor-bound Promptfoo filesystem access is unavailable"
        )


def _strict_component(name: str) -> bytes:
    if (
        type(name) is not str
        or not name
        or name in {".", ".."}
        or "/" in name
        or "\\" in name
        or any(ord(character) < 0x20 or ord(character) == 0x7F for character in name)
    ):
        raise PromptfooFilesystemError("Promptfoo path name is not canonical")
    try:
        return name.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise PromptfooFilesystemError("Promptfoo path name is not strict UTF-8") from error


def _absolute_parts(path: Path) -> tuple[str, ...]:
    raw = os.fspath(path)
    if type(raw) is not str or not os.path.isabs(raw) or os.path.normpath(raw) != raw:
        raise PromptfooFilesystemError("Promptfoo path must be absolute and canonical")
    parts = Path(raw).parts
    if not parts or parts[0] != os.path.sep:
        raise PromptfooFilesystemError("Promptfoo path must have a filesystem anchor")
    for part in parts[1:]:
        _strict_component(part)
    return tuple(parts[1:])


def _relative_path(parts: tuple[str, ...]) -> tuple[str, bytes]:
    if not parts or len(parts) > MAX_DEPTH:
        raise PromptfooFilesystemError("Promptfoo relative path exceeds its depth bound")
    for part in parts:
        _strict_component(part)
    value = "/".join(parts)
    encoded = value.encode("utf-8", errors="strict")
    if len(encoded) > MAX_PATH_BYTES:
        raise PromptfooFilesystemError("Promptfoo relative path exceeds its byte bound")
    return value, encoded


_DIR_FLAGS = (
    os.O_RDONLY
    | getattr(os, "O_DIRECTORY", 0)
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_CLOEXEC", 0)
)
_FILE_FLAGS = (
    os.O_RDONLY
    | getattr(os, "O_NOFOLLOW", 0)
    | getattr(os, "O_NONBLOCK", 0)
    | getattr(os, "O_CLOEXEC", 0)
)


class _DirectoryCapability:
    def __init__(self, path: Path) -> None:
        self.path = Path(path)
        self.descriptors: list[int] = []
        self.states: list[tuple[int, int, int, int, int, int, int]] = []
        self.edges: list[tuple[int, str, tuple[int, int, int, int, int, int, int]]] = []

    def __enter__(self) -> "_DirectoryCapability":
        _require_backend()
        parts = _absolute_parts(self.path)
        try:
            root_state = os.stat(os.path.sep, follow_symlinks=False)
            descriptor = os.open(os.path.sep, _DIR_FLAGS)
            self.descriptors.append(descriptor)
            if not stat.S_ISDIR(root_state.st_mode) or _state(root_state) != _state(os.fstat(descriptor)):
                raise PromptfooFilesystemError("filesystem anchor changed while opened")
            self.states.append(_state(root_state))
            for name in parts:
                parent = descriptor
                before = os.stat(name, dir_fd=parent, follow_symlinks=False)
                if stat.S_ISLNK(before.st_mode) or not stat.S_ISDIR(before.st_mode):
                    raise PromptfooFilesystemError("Promptfoo path contains a directory link")
                descriptor = os.open(name, _DIR_FLAGS, dir_fd=parent)
                self.descriptors.append(descriptor)
                opened = os.fstat(descriptor)
                if _state(before) != _state(opened):
                    raise PromptfooFilesystemError("Promptfoo directory changed while opened")
                self.states.append(_state(before))
                self.edges.append((parent, name, _state(before)))
            return self
        except PromptfooFilesystemError:
            self._close()
            raise
        except OSError as error:
            self._close()
            raise PromptfooFilesystemError("Promptfoo directory capability is unavailable") from error

    @property
    def descriptor(self) -> int:
        return self.descriptors[-1]

    def verify(self) -> None:
        try:
            for descriptor, expected in zip(self.descriptors, self.states, strict=True):
                if _state(os.fstat(descriptor)) != expected:
                    raise PromptfooFilesystemError("Promptfoo directory changed during access")
            for parent, name, expected in self.edges:
                actual = os.stat(name, dir_fd=parent, follow_symlinks=False)
                if _state(actual) != expected:
                    raise PromptfooFilesystemError("Promptfoo directory edge changed during access")
        except PromptfooFilesystemError:
            raise
        except OSError as error:
            raise PromptfooFilesystemError("Promptfoo directory changed during access") from error

    def _close(self) -> None:
        error: OSError | None = None
        while self.descriptors:
            descriptor = self.descriptors.pop()
            try:
                os.close(descriptor)
            except OSError as close_error:
                if error is None:
                    error = close_error
        self.states.clear()
        self.edges.clear()
        if error is not None:
            raise PromptfooFilesystemError("Promptfoo descriptor close failed") from error

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> bool:
        self._close()
        return False


def _require_regular(metadata: os.stat_result, maximum: int, label: str) -> None:
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
        raise PromptfooFilesystemError(f"{label} is not a bounded regular file")
    if metadata.st_nlink != 1:
        raise PromptfooFilesystemError(f"{label} hard link is not allowed")
    if metadata.st_size < 0 or metadata.st_size > maximum:
        raise PromptfooFilesystemError(f"{label} exceeds its byte bound")


def _open_regular(parent: int, name: str, maximum: int, label: str) -> tuple[int, os.stat_result]:
    descriptor = -1
    try:
        before = os.stat(name, dir_fd=parent, follow_symlinks=False)
        _require_regular(before, maximum, label)
        descriptor = os.open(name, _FILE_FLAGS, dir_fd=parent)
        opened = os.fstat(descriptor)
        _require_regular(opened, maximum, label)
        if _state(before) != _state(opened):
            raise PromptfooFilesystemError(f"{label} changed while opened")
        return descriptor, before
    except (PromptfooFilesystemError, OSError) as error:
        close_error: OSError | None = None
        if descriptor >= 0:
            try:
                os.close(descriptor)
            except OSError as caught:
                close_error = caught
        if close_error is not None:
            raise PromptfooFilesystemError(f"{label} descriptor close failed") from close_error
        if isinstance(error, PromptfooFilesystemError):
            raise
        raise PromptfooFilesystemError(f"{label} is unavailable or changed") from error


def _pread_exact(descriptor: int, size: int, *, collect: bool) -> tuple[str, bytes | None]:
    digest = hashlib.sha256()
    chunks: list[bytes] | None = [] if collect else None
    offset = 0
    while offset < size:
        payload = os.pread(descriptor, min(_READ_CHUNK_BYTES, size - offset), offset)
        if not payload or len(payload) > size - offset:
            raise PromptfooFilesystemError("Promptfoo file changed while read")
        digest.update(payload)
        if chunks is not None:
            chunks.append(payload)
        offset += len(payload)
    if os.pread(descriptor, 1, size):
        raise PromptfooFilesystemError("Promptfoo file grew while read")
    return digest.hexdigest(), b"".join(chunks) if chunks is not None else None


def read_bounded_regular(path: Path, maximum_bytes: int, label: str) -> bytes:
    """Read one stable single-link file through retained absolute descriptors."""
    if type(maximum_bytes) is not int or maximum_bytes < 0 or type(label) is not str:
        raise PromptfooFilesystemError("Promptfoo control read bound is invalid")
    target = Path(path)
    parts = _absolute_parts(target)
    if not parts:
        raise PromptfooFilesystemError(f"{label} must name a regular file")
    leaf = parts[-1]
    with _DirectoryCapability(target.parent) as parent:
        descriptor = -1
        try:
            descriptor, before = _open_regular(parent.descriptor, leaf, maximum_bytes, label)
            _, payload = _pread_exact(descriptor, before.st_size, collect=True)
            after = os.fstat(descriptor)
            final = os.stat(leaf, dir_fd=parent.descriptor, follow_symlinks=False)
            if _state(before) != _state(after) or _state(before) != _state(final):
                raise PromptfooFilesystemError(f"{label} changed while read")
            parent.verify()
            assert payload is not None
            return payload
        except PromptfooFilesystemError:
            raise
        except OSError as error:
            raise PromptfooFilesystemError(f"{label} is unavailable or changed") from error
        finally:
            if descriptor >= 0:
                try:
                    os.close(descriptor)
                except OSError as error:
                    raise PromptfooFilesystemError(f"{label} descriptor close failed") from error


class _ChunkWriter:
    def __init__(self) -> None:
        self.pending = bytearray()
        self.chunks: list[bytes] = []

    def write(self, payload: bytes) -> int:
        view = memoryview(payload)
        while view:
            amount = min(MAX_CHUNK_BYTES - len(self.pending), len(view))
            self.pending.extend(view[:amount])
            view = view[amount:]
            if len(self.pending) == MAX_CHUNK_BYTES:
                if len(self.chunks) >= MAX_CHUNKS:
                    raise PromptfooFilesystemError("Promptfoo archive exceeds its chunk bound")
                self.chunks.append(bytes(self.pending))
                self.pending.clear()
        return len(payload)

    def flush(self) -> None:
        pass

    def finish(self) -> tuple[bytes, ...]:
        if self.pending:
            if len(self.chunks) >= MAX_CHUNKS:
                raise PromptfooFilesystemError("Promptfoo archive exceeds its chunk bound")
            self.chunks.append(bytes(self.pending))
            self.pending.clear()
        return tuple(self.chunks)


class _ArchiveBuilder:
    def __init__(self, archive: gzip.GzipFile, tree: object) -> None:
        self.archive = archive
        self.tree = tree
        self.file_count = 0
        self.entry_count = 0
        self.unpacked_bytes = 0
        self.package_bytes: bytes | None = None
        self.has_entrypoint = False

    def _file(self, directory: int, name: str, parts: tuple[str, ...], before: os.stat_result) -> None:
        relative, _ = _relative_path(parts)
        descriptor = -1
        try:
            descriptor, opened_before = _open_regular(
                directory, name, MAX_FILE_BYTES, "Promptfoo runtime file"
            )
            if _state(before) != _state(opened_before):
                raise PromptfooFilesystemError("Promptfoo runtime file changed before open")
            is_package = relative == _PACKAGE_PATH
            if is_package and before.st_size > MAX_PACKAGE_BYTES:
                raise PromptfooFilesystemError("Promptfoo package exceeds its byte bound")
            first_digest, captured = _pread_exact(
                descriptor, before.st_size, collect=is_package
            )
            if _state(os.fstat(descriptor)) != _state(before):
                raise PromptfooFilesystemError("Promptfoo runtime file changed after digest")
            mode = 0o700 if before.st_mode & 0o111 else 0o600
            header = json.dumps(
                {"mode": mode, "path": relative, "sha256": first_digest, "size": before.st_size},
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
            ).encode("utf-8", errors="strict")
            if len(header) > MAX_HEADER_BYTES:
                raise PromptfooFilesystemError("Promptfoo archive header exceeds its byte bound")
            prefix = struct.pack(">I", len(header)) + header
            self.tree.update(prefix)
            self.archive.write(prefix)
            second = hashlib.sha256()
            offset = 0
            while offset < before.st_size:
                payload = os.pread(
                    descriptor, min(_READ_CHUNK_BYTES, before.st_size - offset), offset
                )
                if not payload or len(payload) > before.st_size - offset:
                    raise PromptfooFilesystemError("Promptfoo runtime file changed during copy")
                second.update(payload)
                self.tree.update(payload)
                self.archive.write(payload)
                offset += len(payload)
            if os.pread(descriptor, 1, before.st_size) or second.hexdigest() != first_digest:
                raise PromptfooFilesystemError("Promptfoo runtime file digest changed during copy")
            final = os.stat(name, dir_fd=directory, follow_symlinks=False)
            if _state(os.fstat(descriptor)) != _state(before) or _state(final) != _state(before):
                raise PromptfooFilesystemError("Promptfoo runtime file changed during copy")
            self.file_count += 1
            self.unpacked_bytes += before.st_size
            if self.file_count > MAX_FILES or self.unpacked_bytes > MAX_UNPACKED_BYTES:
                raise PromptfooFilesystemError("Promptfoo runtime content exceeds its bound")
            if is_package:
                self.package_bytes = captured
            if relative == _ENTRYPOINT:
                self.has_entrypoint = True
        finally:
            if descriptor >= 0:
                os.close(descriptor)

    def walk(self, directory: int, prefix: tuple[str, ...], expected: os.stat_result) -> None:
        scanner = None
        try:
            scanner = os.scandir(directory)
            names = [entry.name for entry in scanner]
        except OSError as error:
            raise PromptfooFilesystemError("Promptfoo runtime tree is unavailable") from error
        finally:
            close = getattr(scanner, "close", None)
            if close is not None:
                close()
        self.entry_count += len(names)
        if self.entry_count > MAX_ENTRIES:
            raise PromptfooFilesystemError("Promptfoo runtime entry count exceeds its bound")
        ordered = sorted(names, key=_strict_component)
        files: list[tuple[str, os.stat_result]] = []
        directories: list[tuple[str, os.stat_result]] = []
        for name in ordered:
            parts = (*prefix, name)
            _relative_path(parts)
            try:
                metadata = os.stat(name, dir_fd=directory, follow_symlinks=False)
            except OSError as error:
                raise PromptfooFilesystemError("Promptfoo runtime entry changed") from error
            if stat.S_ISLNK(metadata.st_mode):
                if ".bin" not in prefix:
                    raise PromptfooFilesystemError("Promptfoo runtime contains a symbolic link")
                continue
            if stat.S_ISDIR(metadata.st_mode):
                directories.append((name, metadata))
            elif stat.S_ISREG(metadata.st_mode):
                _require_regular(metadata, MAX_FILE_BYTES, "Promptfoo runtime file")
                files.append((name, metadata))
            else:
                raise PromptfooFilesystemError("Promptfoo runtime contains a special entry")
        for name, metadata in files:
            self._file(directory, name, (*prefix, name), metadata)
        for name, metadata in directories:
            child = -1
            try:
                child = os.open(name, _DIR_FLAGS, dir_fd=directory)
                if _state(os.fstat(child)) != _state(metadata):
                    raise PromptfooFilesystemError("Promptfoo directory changed while opened")
                self.walk(child, (*prefix, name), metadata)
                final = os.stat(name, dir_fd=directory, follow_symlinks=False)
                if _state(final) != _state(metadata):
                    raise PromptfooFilesystemError("Promptfoo directory changed during traversal")
            except PromptfooFilesystemError:
                raise
            except OSError as error:
                raise PromptfooFilesystemError("Promptfoo directory changed while opened") from error
            finally:
                if child >= 0:
                    os.close(child)
        if _state(os.fstat(directory)) != _state(expected):
            raise PromptfooFilesystemError("Promptfoo directory changed during traversal")


def _validate_package(payload: bytes | None, has_entrypoint: bool) -> None:
    try:
        package = json.loads((payload or b"").decode("utf-8", errors="strict"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromptfooFilesystemError("Promptfoo package manifest is invalid") from error
    if (
        type(package) is not dict
        or package.get("name") != "promptfoo"
        or package.get("version") != "0.122.0"
        or type(package.get("bin")) is not dict
        or package["bin"].get("promptfoo") != "dist/src/entrypoint.js"
        or not has_entrypoint
    ):
        raise PromptfooFilesystemError("Promptfoo runtime package is not exact 0.122.0")


def build_runtime_archive(runtime_root: Path) -> ArchiveBuild:
    """Build the deterministic runtime archive from retained directory capabilities."""
    writer = _ChunkWriter()
    tree = hashlib.sha256()
    with _DirectoryCapability(Path(runtime_root)) as root:
        expected = os.fstat(root.descriptor)
        builder: _ArchiveBuilder | None = None
        try:
            with gzip.GzipFile(fileobj=writer, mode="wb", compresslevel=9, mtime=0) as stream:
                builder = _ArchiveBuilder(stream, tree)
                builder.walk(root.descriptor, (), expected)
                terminator = struct.pack(">I", 0)
                tree.update(terminator)
                stream.write(terminator)
        except PromptfooFilesystemError:
            raise
        except OSError as error:
            raise PromptfooFilesystemError("Promptfoo runtime archive could not be built") from error
        root.verify()
    assert builder is not None
    _validate_package(builder.package_bytes, builder.has_entrypoint)
    chunks = writer.finish()
    if not chunks:
        raise PromptfooFilesystemError("Promptfoo runtime archive is empty")
    return ArchiveBuild(chunks, tree.hexdigest(), builder.file_count, builder.unpacked_bytes)
