"""Deterministic Promptfoo runtime sealing for Task 4 launch artifacts."""

import hashlib
import json
import platform
import re
from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_promptfoo_archive import (
        MAX_CHUNK_BYTES,
        MAX_CHUNKS,
        MAX_FILES,
        MAX_FILE_BYTES,
        MAX_UNPACKED_BYTES,
        PromptfooFilesystemError,
        build_runtime_archive,
        read_bounded_regular,
    )
    from .batch_launch_spec import LaunchArtifact
except ImportError:
    from batch_promptfoo_archive import (
        MAX_CHUNK_BYTES,
        MAX_CHUNKS,
        MAX_FILES,
        MAX_FILE_BYTES,
        MAX_UNPACKED_BYTES,
        PromptfooFilesystemError,
        build_runtime_archive,
        read_bounded_regular,
    )
    from batch_launch_spec import LaunchArtifact


PROMPTFOO_VERSION = "0.122.0"
ARCHIVE_FORMAT = "ai-ip-promptfoo-records-v1"
MAX_NODE_BYTES = 128 * 1024 * 1024
MAX_WORKSPACE_PACKAGE_BYTES = 64 * 1024
MAX_WORKSPACE_LOCK_BYTES = 8 * 1024 * 1024
MAX_RUNTIME_MANIFEST_BYTES = 1024 * 1024
MAX_SOURCE_BYTES = 1024 * 1024
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
    try:
        return read_bounded_regular(path, maximum, label)
    except PromptfooFilesystemError as error:
        raise PromptfooBundleError(str(error)) from error


def _build_archive(root: Path) -> tuple[tuple[bytes, ...], str, int, int]:
    try:
        built = build_runtime_archive(root)
    except PromptfooFilesystemError as error:
        raise PromptfooBundleError(str(error)) from error
    return built.chunks, built.tree_sha256, built.file_count, built.unpacked_bytes


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
        hashlib.sha256(
            _bounded_regular(
                _WORKSPACE_PACKAGE, MAX_WORKSPACE_PACKAGE_BYTES, "workspace package"
            )
        ).hexdigest(),
        platform_arch,
        platform_os,
        hashlib.sha256(
            _bounded_regular(_WORKSPACE_LOCK, MAX_WORKSPACE_LOCK_BYTES, "workspace lock")
        ).hexdigest(),
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
        value = json.loads(
            _bounded_regular(
                manifest_path, MAX_RUNTIME_MANIFEST_BYTES, "runtime manifest"
            )
        )
    except (PromptfooBundleError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PromptfooBundleError("committed Promptfoo runtime seal is unavailable") from error
    seal = _seal_from_json(value)
    if (seal.platform_os, seal.platform_arch) != (platform_os, platform_arch):
        raise PromptfooBundleError("committed Promptfoo runtime targets another platform")
    if (
        hashlib.sha256(
            _bounded_regular(
                _WORKSPACE_PACKAGE, MAX_WORKSPACE_PACKAGE_BYTES, "workspace package"
            )
        ).hexdigest()
        != seal.package_json_sha256
        or hashlib.sha256(
            _bounded_regular(_WORKSPACE_LOCK, MAX_WORKSPACE_LOCK_BYTES, "workspace lock")
        ).hexdigest()
        != seal.pnpm_lock_sha256
    ):
        raise PromptfooBundleError(
            "workspace package or lock differs from the committed Promptfoo runtime seal"
        )
    return seal_promptfoo_runtime(runtime_root, node_path, seal)
