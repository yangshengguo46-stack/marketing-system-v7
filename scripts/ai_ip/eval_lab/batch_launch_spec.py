"""Immutable launch specifications admitted by the production controller."""

import hashlib
import os
import stat
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

try:
    from .batch_controller_support import ValidatedBindings
    from .contracts import canonical_json_bytes, sha256_json
except ImportError:
    from batch_controller_support import ValidatedBindings
    from contracts import canonical_json_bytes, sha256_json


_MAX_EXECUTABLE_BYTES = 128 * 1024 * 1024
_MAX_ARTIFACT_BYTES = 16 * 1024 * 1024
_MAX_ARTIFACT_COUNT = 32
_RESERVED_ENVIRONMENT = {
    "AI_IP_ATTEMPT_ID",
    "AI_IP_CASE_PATH",
    "AI_IP_CODEX_PATH",
    "AI_IP_CONFIG_PATH",
    "AI_IP_PROFILE_PATH",
    "AI_IP_PROTOCOL_PATH",
    "AI_IP_PROMPTFOO_PATH",
    "AI_IP_RESULT_FD",
    "AI_IP_ROUTE_PATH",
    "AI_IP_SCHEMA_PATH",
    "AI_IP_TELEMETRY_FD",
}


class LaunchSpecError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class LaunchArtifact:
    relative_path: str
    payload: bytes
    sha256: str


@dataclass(frozen=True, slots=True)
class LaunchSpec:
    """Exact executable, argv, environment, and bytes for one candidate process."""

    executable_path: Path
    executable_sha256: str
    argv: tuple[str, ...]
    environment: tuple[tuple[str, str], ...]
    artifacts: tuple[LaunchArtifact, ...]
    effective_config: bytes
    execution_profile_json: bytes
    model_route_json: bytes
    app_server_protocol_schema: bytes
    promptfoo_config: bytes


@dataclass(frozen=True, slots=True)
class CandidateLaunchSet:
    stock: LaunchSpec
    modified: LaunchSpec


@dataclass(frozen=True, slots=True)
class FrozenLaunchSpec:
    executable_payload: bytes
    executable_sha256: str
    argv: tuple[str, ...]
    environment: tuple[tuple[str, str], ...]
    artifacts: tuple[LaunchArtifact, ...]
    effective_config: bytes
    execution_profile_json: bytes
    model_route_json: bytes
    app_server_protocol_schema: bytes
    promptfoo_config: bytes


@dataclass(frozen=True, slots=True)
class ValidatedLaunchSet:
    stock: FrozenLaunchSpec
    modified: FrozenLaunchSpec


def launch_spec_identity(spec: FrozenLaunchSpec) -> tuple[dict[str, object], str]:
    """Return the exact immutable adapter declaration and its commitment."""
    value = {
        "appServerProtocolSchemaSha256": hashlib.sha256(
            spec.app_server_protocol_schema
        ).hexdigest(),
        "argv": list(spec.argv),
        "artifacts": {
            artifact.relative_path: artifact.sha256 for artifact in spec.artifacts
        },
        "effectiveConfigSha256": hashlib.sha256(spec.effective_config).hexdigest(),
        "environment": [list(item) for item in spec.environment],
        "executableSha256": spec.executable_sha256,
        "executionProfileSha256": hashlib.sha256(
            spec.execution_profile_json
        ).hexdigest(),
        "modelRouteSha256": hashlib.sha256(spec.model_route_json).hexdigest(),
        "promptfooConfigSha256": hashlib.sha256(spec.promptfoo_config).hexdigest(),
    }
    return value, sha256_json(value)


def _digest(value: object, label: str) -> str:
    if (
        type(value) is not str
        or len(value) != 64
        or any(character not in "0123456789abcdef" for character in value)
    ):
        raise LaunchSpecError(f"{label} must be a lowercase SHA-256")
    return value


def _regular_bytes(path: Path, expected: str) -> bytes:
    descriptor = -1
    try:
        before = Path(path).lstat()
        if not stat.S_ISREG(before.st_mode) or stat.S_ISLNK(before.st_mode):
            raise LaunchSpecError("launch executable must be a regular file")
        if before.st_size > _MAX_EXECUTABLE_BYTES:
            raise LaunchSpecError("launch executable exceeds the byte bound")
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise LaunchSpecError("launch executable was replaced")
        chunks: list[bytes] = []
        remaining = _MAX_EXECUTABLE_BYTES + 1
        while remaining:
            chunk = os.read(descriptor, min(64 * 1024, remaining))
            if not chunk:
                break
            chunks.append(chunk)
            remaining -= len(chunk)
        payload = b"".join(chunks)
        after = os.fstat(descriptor)
        if len(payload) > _MAX_EXECUTABLE_BYTES:
            raise LaunchSpecError("launch executable exceeds the byte bound")
        if (
            opened.st_dev,
            opened.st_ino,
            opened.st_size,
            opened.st_mtime_ns,
        ) != (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns):
            raise LaunchSpecError("launch executable changed while opened")
        if hashlib.sha256(payload).hexdigest() != expected:
            raise LaunchSpecError("launch executable identity differs from spec")
        return payload
    except OSError as error:
        raise LaunchSpecError("launch executable is unavailable") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def _artifact(value: object) -> LaunchArtifact:
    if type(value) is not LaunchArtifact:
        raise LaunchSpecError("launch artifacts must use the exact immutable type")
    relative = PurePosixPath(value.relative_path)
    if (
        not value.relative_path
        or relative.is_absolute()
        or any(part in ("", ".", "..") for part in relative.parts)
    ):
        raise LaunchSpecError("launch artifact path is unsafe")
    if type(value.payload) is not bytes or len(value.payload) > _MAX_ARTIFACT_BYTES:
        raise LaunchSpecError("launch artifact exceeds the byte bound")
    digest = _digest(value.sha256, "launch artifact SHA-256")
    if hashlib.sha256(value.payload).hexdigest() != digest:
        raise LaunchSpecError("launch artifact identity differs from bytes")
    return value


def _spec(value: object, expected: dict[str, bytes]) -> FrozenLaunchSpec:
    if type(value) is not LaunchSpec:
        raise LaunchSpecError(
            "controller requires an exact immutable launch specification"
        )
    if not isinstance(value.executable_path, Path):
        raise LaunchSpecError("launch executable path must be a Path")
    executable_sha256 = _digest(value.executable_sha256, "launch executable SHA-256")
    if type(value.argv) is not tuple or not all(
        type(item) is str for item in value.argv
    ):
        raise LaunchSpecError("launch argv must be an immutable string tuple")
    if len(value.argv) > 64 or any(len(item) > 4096 for item in value.argv):
        raise LaunchSpecError("launch argv exceeds its bound")
    if type(value.environment) is not tuple or not all(
        type(item) is tuple
        and len(item) == 2
        and all(type(part) is str for part in item)
        for item in value.environment
    ):
        raise LaunchSpecError("launch environment must be immutable string pairs")
    names = [name for name, _ in value.environment]
    if len(names) != len(set(names)) or any(
        name in _RESERVED_ENVIRONMENT for name in names
    ):
        raise LaunchSpecError("launch environment collides with controller authority")
    if any("KEY" in name or "TOKEN" in name or "SECRET" in name for name in names):
        raise LaunchSpecError("launch environment contains a prohibited secret name")
    if len(value.artifacts) > _MAX_ARTIFACT_COUNT:
        raise LaunchSpecError("launch artifact count exceeds its bound")
    artifacts = tuple(_artifact(item) for item in value.artifacts)
    paths = [item.relative_path for item in artifacts]
    if len(paths) != len(set(paths)):
        raise LaunchSpecError("launch artifact paths must be unique")
    for field, payload in expected.items():
        actual = getattr(value, field)
        if type(actual) is not bytes or actual != payload:
            raise LaunchSpecError(f"launch artifact identity mismatch: {field}")
    return FrozenLaunchSpec(
        _regular_bytes(value.executable_path, executable_sha256),
        executable_sha256,
        value.argv,
        value.environment,
        artifacts,
        value.effective_config,
        value.execution_profile_json,
        value.model_route_json,
        value.app_server_protocol_schema,
        value.promptfoo_config,
    )


def validate_launch_set(
    value: object, bindings: ValidatedBindings
) -> ValidatedLaunchSet:
    """Freeze a closed launch set and reject caller-claimed emitted-byte drift."""
    if type(value) is not CandidateLaunchSet:
        raise LaunchSpecError(
            "controller requires an exact immutable launch specification set"
        )
    shared = {
        "execution_profile_json": canonical_json_bytes(bindings.execution_profile),
        "model_route_json": canonical_json_bytes(bindings.model_route),
        "app_server_protocol_schema": bindings.protocol.payload,
        "promptfoo_config": bindings.promptfoo_config.payload,
    }
    stock = _spec(
        value.stock,
        {**shared, "effective_config": bindings.stock.effective_config.payload},
    )
    modified = _spec(
        value.modified,
        {**shared, "effective_config": bindings.modified.effective_config.payload},
    )
    parity = (
        "executable_payload",
        "executable_sha256",
        "argv",
        "environment",
        "artifacts",
        "execution_profile_json",
        "model_route_json",
        "app_server_protocol_schema",
        "promptfoo_config",
    )
    if any(getattr(stock, field) != getattr(modified, field) for field in parity):
        raise LaunchSpecError("stock and modified launch conditions differ")
    return ValidatedLaunchSet(stock, modified)
