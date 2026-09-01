"""Deterministic Promptfoo runtime sealing for Task 4 launch artifacts."""

import gzip
import hashlib
import json
import os
import platform
import re
import stat
import struct
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

try:
    from .batch_launch_spec import LaunchArtifact
except ImportError:
    from batch_launch_spec import LaunchArtifact


PROMPTFOO_VERSION = "0.122.0"
ARCHIVE_FORMAT = "ai-ip-promptfoo-records-v1"
MAX_CHUNK_BYTES = 16 * 1024 * 1024
MAX_CHUNKS = 29
MAX_FILES = 100_000
MAX_FILE_BYTES = 384 * 1024 * 1024
MAX_UNPACKED_BYTES = 2 * 1024 * 1024 * 1024
MAX_NODE_BYTES = 128 * 1024 * 1024
_ENTRYPOINT = "node_modules/promptfoo/dist/src/entrypoint.js"
_REPO_ROOT = Path(__file__).resolve().parents[3]
_WORKSPACE_PACKAGE = _REPO_ROOT / "ai-ip-evals/lab/promptfoo/package.json"
_WORKSPACE_LOCK = _REPO_ROOT / "pnpm-lock.yaml"
_COMMITTED_SEALS = _REPO_ROOT / "ai-ip-evals/lab/promptfoo/runtime-manifests"
_NODE_VERSION = re.compile(r"v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)\Z")


class PromptfooBundleError(ValueError):
    pass


def _supported_node_version(value: object) -> bool:
    if type(value) is not str:
        return False
    matched = _NODE_VERSION.fullmatch(value)
    return matched is not None and tuple(map(int, matched.groups())) >= (22, 22, 0)


@dataclass(frozen=True, slots=True)
class RuntimeSeal:
    archive_sha256: str
    chunk_sha256: tuple[str, ...]
    entrypoint: str
    file_count: int
    node_sha256: str
    node_version: str
    package_json_sha256: str
    platform_arch: str
    platform_os: str
    pnpm_lock_sha256: str
    promptfoo_version: str
    tree_sha256: str
    unpacked_bytes: int

    def json_value(self) -> dict[str, object]:
        return {
            "archiveSha256": self.archive_sha256,
            "chunkSha256": list(self.chunk_sha256),
            "entrypoint": self.entrypoint,
            "fileCount": self.file_count,
            "format": ARCHIVE_FORMAT,
            "maximumFileBytes": MAX_FILE_BYTES,
            "maximumUnpackedBytes": MAX_UNPACKED_BYTES,
            "nodeSha256": self.node_sha256,
            "nodeVersion": self.node_version,
            "packageJsonSha256": self.package_json_sha256,
            "platform": {"arch": self.platform_arch, "os": self.platform_os},
            "pnpmLockSha256": self.pnpm_lock_sha256,
            "promptfooVersion": self.promptfoo_version,
            "schemaVersion": 1,
            "treeSha256": self.tree_sha256,
            "unpackedBytes": self.unpacked_bytes,
        }


@dataclass(frozen=True, slots=True)
class SealedPromptfooRuntime:
    node_path: Path
    node_sha256: str
    artifacts: tuple[LaunchArtifact, ...]
    artifact_arguments: tuple[str, ...]


class _ChunkWriter:
    def __init__(self) -> None:
        self._pending = bytearray()
        self.chunks: list[bytes] = []

    def write(self, payload: bytes) -> int:
        view = memoryview(payload)
        while view:
            amount = min(MAX_CHUNK_BYTES - len(self._pending), len(view))
            self._pending.extend(view[:amount])
            view = view[amount:]
            if len(self._pending) == MAX_CHUNK_BYTES:
                self.chunks.append(bytes(self._pending))
                self._pending.clear()
        return len(payload)

    def flush(self) -> None:
        pass

    def finish(self) -> tuple[bytes, ...]:
        if self._pending:
            self.chunks.append(bytes(self._pending))
            self._pending.clear()
        return tuple(self.chunks)


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def _digest(value: object, label: str) -> str:
    if (
        type(value) is not str
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise PromptfooBundleError(f"{label} must be a lowercase SHA-256")
    return value


def _bounded_regular(path: Path, maximum: int, label: str) -> bytes:
    descriptor = -1
    try:
        before = path.lstat()
        if (
            not stat.S_ISREG(before.st_mode)
            or stat.S_ISLNK(before.st_mode)
            or before.st_size > maximum
        ):
            raise PromptfooBundleError(f"{label} is not a bounded regular file")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise PromptfooBundleError(f"{label} changed while opened")
        chunks: list[bytes] = []
        remaining = maximum + 1
        while remaining:
            chunk = os.read(descriptor, min(64 * 1024, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        after = os.fstat(descriptor)
        if (
            opened.st_size,
            opened.st_mtime_ns,
            opened.st_ino,
        ) != (after.st_size, after.st_mtime_ns, after.st_ino):
            raise PromptfooBundleError(f"{label} changed while measured")
        payload = b"".join(chunks)
        if len(payload) > maximum:
            raise PromptfooBundleError(f"{label} exceeds its byte bound")
        return payload
    except OSError as error:
        raise PromptfooBundleError(f"{label} is unavailable") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _runtime_files(root: Path) -> tuple[tuple[PurePosixPath, Path], ...]:
    if not root.is_absolute() or not root.is_dir() or root.is_symlink():
        raise PromptfooBundleError("Promptfoo runtime root is unsafe")
    files: list[tuple[PurePosixPath, Path]] = []
    try:
        for directory, names, filenames in os.walk(root, followlinks=False):
            names.sort()
            filenames.sort()
            directory_path = Path(directory)
            for name in names:
                candidate = directory_path / name
                if candidate.is_symlink():
                    raise PromptfooBundleError("Promptfoo runtime contains a symlink")
            for name in filenames:
                candidate = directory_path / name
                relative = PurePosixPath(candidate.relative_to(root).as_posix())
                if candidate.is_symlink() and ".bin" in relative.parts:
                    continue
                if candidate.is_symlink() or not candidate.is_file():
                    raise PromptfooBundleError(
                        "Promptfoo runtime contains a non-regular entry"
                    )
                files.append((relative, candidate))
    except OSError as error:
        raise PromptfooBundleError("Promptfoo runtime tree is unavailable") from error
    if not files or len(files) > MAX_FILES:
        raise PromptfooBundleError("Promptfoo runtime file count exceeds its bound")
    return tuple(files)


def _validate_promptfoo_package(root: Path) -> None:
    package_path = root / "node_modules/promptfoo/package.json"
    try:
        package = json.loads(_bounded_regular(package_path, 1024 * 1024, "package"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromptfooBundleError("Promptfoo package manifest is invalid") from error
    if (
        type(package) is not dict
        or package.get("name") != "promptfoo"
        or package.get("version") != PROMPTFOO_VERSION
        or type(package.get("bin")) is not dict
        or package["bin"].get("promptfoo") != "dist/src/entrypoint.js"
        or not (root / _ENTRYPOINT).is_file()
    ):
        raise PromptfooBundleError("Promptfoo runtime package is not exact 0.122.0")


def _build_archive(root: Path) -> tuple[tuple[bytes, ...], str, int, int]:
    _validate_promptfoo_package(root)
    writer = _ChunkWriter()
    tree = hashlib.sha256()
    file_count = 0
    unpacked_bytes = 0
    with gzip.GzipFile(fileobj=writer, mode="wb", compresslevel=9, mtime=0) as archive:
        for relative, path in _runtime_files(root):
            payload = _bounded_regular(path, MAX_FILE_BYTES, "Promptfoo runtime file")
            unpacked_bytes += len(payload)
            file_count += 1
            if unpacked_bytes > MAX_UNPACKED_BYTES:
                raise PromptfooBundleError(
                    "Promptfoo runtime expanded bytes exceed the bound"
                )
            mode = 0o700 if path.stat().st_mode & 0o111 else 0o600
            header = _canonical(
                {
                    "mode": mode,
                    "path": relative.as_posix(),
                    "sha256": hashlib.sha256(payload).hexdigest(),
                    "size": len(payload),
                }
            )
            record = struct.pack(">I", len(header)) + header + payload
            tree.update(record)
            archive.write(record)
        terminator = struct.pack(">I", 0)
        tree.update(terminator)
        archive.write(terminator)
    chunks = writer.finish()
    if not chunks or len(chunks) > MAX_CHUNKS:
        raise PromptfooBundleError("Promptfoo runtime archive exceeds its chunk bound")
    return chunks, tree.hexdigest(), file_count, unpacked_bytes


def _platform_identity() -> tuple[str, str]:
    os_name = platform.system().lower()
    machine = platform.machine().lower()
    architectures = {
        "amd64": "x86_64",
        "x64": "x86_64",
        "x86_64": "x86_64",
        "aarch64": "arm64",
        "arm64": "arm64",
    }
    return os_name, architectures.get(machine, machine)


def measure_promptfoo_runtime(
    runtime_root: Path, node_path: Path, *, node_version: str = "test-node"
) -> RuntimeSeal:
    """Measure a tree for builder tests; production still requires the committed seal."""
    node = _bounded_regular(node_path, MAX_NODE_BYTES, "portable Node")
    chunks, tree_sha256, file_count, unpacked_bytes = _build_archive(runtime_root)
    platform_os, platform_arch = _platform_identity()
    return RuntimeSeal(
        hashlib.sha256(b"".join(chunks)).hexdigest(),
        tuple(hashlib.sha256(chunk).hexdigest() for chunk in chunks),
        _ENTRYPOINT,
        file_count,
        hashlib.sha256(node).hexdigest(),
        node_version,
        hashlib.sha256(_WORKSPACE_PACKAGE.read_bytes()).hexdigest(),
        platform_arch,
        platform_os,
        hashlib.sha256(_WORKSPACE_LOCK.read_bytes()).hexdigest(),
        PROMPTFOO_VERSION,
        tree_sha256,
        unpacked_bytes,
    )


def seal_promptfoo_runtime(
    runtime_root: Path, node_path: Path, expected: RuntimeSeal
) -> SealedPromptfooRuntime:
    """Seal only bytes that reproduce an already-authorized runtime identity."""
    node = _bounded_regular(node_path, MAX_NODE_BYTES, "portable Node")
    if hashlib.sha256(node).hexdigest() != expected.node_sha256:
        raise PromptfooBundleError("portable Node identity differs from the runtime seal")
    chunks, tree_sha256, file_count, unpacked_bytes = _build_archive(runtime_root)
    archive_sha256 = hashlib.sha256(b"".join(chunks)).hexdigest()
    chunk_sha256 = tuple(hashlib.sha256(chunk).hexdigest() for chunk in chunks)
    if (
        tree_sha256 != expected.tree_sha256
        or archive_sha256 != expected.archive_sha256
        or chunk_sha256 != expected.chunk_sha256
        or file_count != expected.file_count
        or unpacked_bytes != expected.unpacked_bytes
    ):
        raise PromptfooBundleError("Promptfoo runtime tree identity differs from the seal")
    manifest_payload = _canonical(expected.json_value())
    artifacts = [
        LaunchArtifact(
            "promptfoo-runtime-manifest.json",
            manifest_payload,
            hashlib.sha256(manifest_payload).hexdigest(),
        )
    ]
    for index, payload in enumerate(chunks):
        relative = f"promptfoo-runtime/chunk-{index:02d}.bin"
        artifacts.append(
            LaunchArtifact(relative, payload, hashlib.sha256(payload).hexdigest())
        )
    arguments = tuple(f"{{artifact:{item.relative_path}}}" for item in artifacts)
    return SealedPromptfooRuntime(
        node_path, expected.node_sha256, tuple(artifacts), arguments
    )


def _seal_from_json(value: object) -> RuntimeSeal:
    fields = {
        "archiveSha256",
        "chunkSha256",
        "entrypoint",
        "fileCount",
        "format",
        "maximumFileBytes",
        "maximumUnpackedBytes",
        "nodeSha256",
        "nodeVersion",
        "packageJsonSha256",
        "platform",
        "pnpmLockSha256",
        "promptfooVersion",
        "schemaVersion",
        "treeSha256",
        "unpackedBytes",
    }
    if type(value) is not dict or set(value) != fields:
        raise PromptfooBundleError("committed Promptfoo runtime seal has invalid fields")
    chunks = value["chunkSha256"]
    if type(chunks) is not list or not chunks or len(chunks) > MAX_CHUNKS:
        raise PromptfooBundleError("committed Promptfoo chunk declaration is invalid")
    target = value["platform"]
    if (
        value["schemaVersion"] != 1
        or value["format"] != ARCHIVE_FORMAT
        or value["maximumFileBytes"] != MAX_FILE_BYTES
        or value["maximumUnpackedBytes"] != MAX_UNPACKED_BYTES
        or value["entrypoint"] != _ENTRYPOINT
        or value["promptfooVersion"] != PROMPTFOO_VERSION
        or type(value["fileCount"]) is not int
        or not 1 <= value["fileCount"] <= MAX_FILES
        or type(value["unpackedBytes"]) is not int
        or not 1 <= value["unpackedBytes"] <= MAX_UNPACKED_BYTES
        or not _supported_node_version(value["nodeVersion"])
        or type(target) is not dict
        or set(target) != {"arch", "os"}
        or type(target["arch"]) is not str
        or type(target["os"]) is not str
    ):
        raise PromptfooBundleError("committed Promptfoo runtime seal is incompatible")
    return RuntimeSeal(
        _digest(value["archiveSha256"], "archive SHA-256"),
        tuple(_digest(item, "chunk SHA-256") for item in chunks),
        value["entrypoint"],
        value["fileCount"],
        _digest(value["nodeSha256"], "Node SHA-256"),
        value["nodeVersion"],
        _digest(value["packageJsonSha256"], "package SHA-256"),
        target["arch"],
        target["os"],
        _digest(value["pnpmLockSha256"], "lock SHA-256"),
        value["promptfooVersion"],
        _digest(value["treeSha256"], "tree SHA-256"),
        value["unpackedBytes"],
    )


def seal_committed_promptfoo_runtime(
    runtime_root: Path, node_path: Path
) -> SealedPromptfooRuntime:
    """Bind exact workspace lock/package bytes and the reviewed deployment tree seal."""
    platform_os, platform_arch = _platform_identity()
    manifest_path = _COMMITTED_SEALS / f"{platform_os}-{platform_arch}.json"
    try:
        value = json.loads(manifest_path.read_bytes())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromptfooBundleError("committed Promptfoo runtime seal is unavailable") from error
    seal = _seal_from_json(value)
    if (seal.platform_os, seal.platform_arch) != (platform_os, platform_arch):
        raise PromptfooBundleError("committed Promptfoo runtime targets another platform")
    if (
        hashlib.sha256(_WORKSPACE_PACKAGE.read_bytes()).hexdigest()
        != seal.package_json_sha256
        or hashlib.sha256(_WORKSPACE_LOCK.read_bytes()).hexdigest()
        != seal.pnpm_lock_sha256
    ):
        raise PromptfooBundleError(
            "workspace package or lock differs from the committed Promptfoo runtime seal"
        )
    return seal_promptfoo_runtime(runtime_root, node_path, seal)
