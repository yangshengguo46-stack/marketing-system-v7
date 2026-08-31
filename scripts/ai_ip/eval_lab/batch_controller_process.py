"""Concrete controller-owned process, descriptor, and deadline supervision."""

import hashlib
import json
import os
import selectors
import shutil
import signal
import stat
import subprocess
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

try:
    from .batch_controller_capture import CapturedResult, FatalSupervisorError
    from .batch_controller_identity import measure_request_identity
    from .batch_controller_types import (
        AttemptAttestation,
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from .batch_launch_spec import FrozenLaunchSpec
    from .batch_receipt_storage import write_entry
    from .contracts import canonical_json_bytes, sha256_json
except ImportError:
    from batch_controller_capture import CapturedResult, FatalSupervisorError
    from batch_controller_identity import measure_request_identity
    from batch_controller_types import (
        AttemptAttestation,
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from batch_launch_spec import FrozenLaunchSpec
    from batch_receipt_storage import write_entry
    from contracts import canonical_json_bytes, sha256_json


_CHUNK_BYTES = 64 * 1024
_MAX_EVENTS = 1_024


class ProcessLaunchError(RuntimeError):
    pass


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def _write_exact(path: Path, payload: bytes, mode: int) -> str:
    descriptor = -1
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    try:
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        descriptor = os.open(path, flags, mode)
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written < 1:
                raise ProcessLaunchError("launch artifact write made no progress")
            view = view[written:]
        os.fchmod(descriptor, mode)
        state = os.fstat(descriptor)
        if not stat.S_ISREG(state.st_mode) or state.st_size != len(payload):
            raise ProcessLaunchError("launch artifact was not written exactly")
        return hashlib.sha256(payload).hexdigest()
    except OSError as error:
        raise ProcessLaunchError("launch artifact could not be materialized") from error
    finally:
        if descriptor >= 0:
            os.close(descriptor)


@dataclass(frozen=True)
class PreparedProcess:
    request: object
    spec: FrozenLaunchSpec
    launcher_path: Path
    artifact_paths: dict[str, Path]
    written_sha256: dict[str, str]


def prepare_process(request: object, spec: FrozenLaunchSpec) -> PreparedProcess:
    """Write every admitted byte into the exact controller-owned cell."""
    runtime = request.cell.home / ".runtime"
    launch_root = runtime / "launch"
    launch_root.mkdir(mode=0o700)
    launcher = runtime / "launcher"
    written = {"executable": _write_exact(launcher, spec.executable_payload, 0o700)}
    fixed = {
        "config": ("effective-config.bin", spec.effective_config),
        "profile": ("execution-profile.json", spec.execution_profile_json),
        "route": ("model-route.json", spec.model_route_json),
        "protocol": ("app-server-protocol.json", spec.app_server_protocol_schema),
        "promptfoo": ("promptfoo-config.json", spec.promptfoo_config),
        "case": ("case-bundle.json", request.case_bundle_json),
        "schema": ("case-answer-schema.json", request.case_answer_schema_json),
    }
    paths: dict[str, Path] = {}
    for name, (relative, payload) in fixed.items():
        path = launch_root / relative
        paths[name] = path
        written[name] = _write_exact(path, payload, 0o600)
    for artifact in spec.artifacts:
        path = launch_root / "artifacts" / artifact.relative_path
        paths[f"artifact:{artifact.relative_path}"] = path
        written[f"artifact:{artifact.relative_path}"] = _write_exact(
            path, artifact.payload, 0o600
        )
    if written["executable"] != spec.executable_sha256:
        raise ProcessLaunchError("copied launch executable identity differs")
    return PreparedProcess(request, spec, launcher, paths, written)


def _expand(argument: str, prepared: PreparedProcess) -> str:
    values = {
        "{codex}": prepared.request.binary_path,
        "{config}": prepared.artifact_paths["config"],
        "{profile}": prepared.artifact_paths["profile"],
        "{route}": prepared.artifact_paths["route"],
        "{protocol}": prepared.artifact_paths["protocol"],
        "{promptfoo}": prepared.artifact_paths["promptfoo"],
        "{case}": prepared.artifact_paths["case"],
        "{schema}": prepared.artifact_paths["schema"],
    }
    values.update(
        {
            "{" + name + "}": path
            for name, path in prepared.artifact_paths.items()
            if name.startswith("artifact:")
        }
    )
    if argument in values:
        return str(values[argument])
    if "{" in argument or "}" in argument:
        raise ProcessLaunchError("launch argv contains an unknown placeholder")
    return argument


@dataclass
class _BoundedBytes:
    limit: int
    payload: bytearray = field(default_factory=bytearray)
    digest: object = field(default_factory=hashlib.sha256)
    size: int = 0
    truncated: bool = False

    def add(self, chunk: bytes) -> None:
        self.digest.update(chunk)
        self.size += len(chunk)
        room = self.limit + 1 - len(self.payload)
        if room > 0:
            self.payload.extend(chunk[:room])
        if self.size > self.limit:
            self.truncated = True


@dataclass
class OwnedProcess:
    process: subprocess.Popen
    prepared: PreparedProcess
    read_descriptors: dict[str, int]
    started_at: str
    started_monotonic: float
    deadline: float
    launch_record: dict[str, object]
    buffers: dict[str, _BoundedBytes]
    timed_out: bool = False
    stopped: bool = False


def _pipes() -> tuple[int, int]:
    read_descriptor, write_descriptor = os.pipe()
    os.set_blocking(read_descriptor, False)
    return read_descriptor, write_descriptor


def spawn(prepared: PreparedProcess, limit: int) -> OwnedProcess:
    """Launch one neutral executable and return only concrete owned state."""
    result_read, result_write = _pipes()
    telemetry_read, telemetry_write = _pipes()
    started_at = _now()
    started_monotonic = time.monotonic()
    argv = [str(prepared.launcher_path)] + [
        _expand(argument, prepared) for argument in prepared.spec.argv
    ]
    environment = dict(prepared.request.cell.environment)
    environment.update(dict(prepared.spec.environment))
    environment.update(
        {
            "AI_IP_CODEX_PATH": str(prepared.request.binary_path),
            "AI_IP_CONFIG_PATH": str(prepared.artifact_paths["config"]),
            "AI_IP_PROFILE_PATH": str(prepared.artifact_paths["profile"]),
            "AI_IP_ROUTE_PATH": str(prepared.artifact_paths["route"]),
            "AI_IP_PROTOCOL_PATH": str(prepared.artifact_paths["protocol"]),
            "AI_IP_PROMPTFOO_PATH": str(prepared.artifact_paths["promptfoo"]),
            "AI_IP_SCHEMA_PATH": str(prepared.artifact_paths["schema"]),
            "AI_IP_RESULT_FD": str(result_write),
            "AI_IP_TELEMETRY_FD": str(telemetry_write),
        }
    )
    try:
        process = subprocess.Popen(
            argv,
            executable=str(prepared.launcher_path),
            cwd=prepared.request.cell.workspace,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            pass_fds=(result_write, telemetry_write),
            start_new_session=True,
        )
    except BaseException:
        for descriptor in (result_read, result_write, telemetry_read, telemetry_write):
            os.close(descriptor)
        raise
    os.close(result_write)
    os.close(telemetry_write)
    assert process.stdout is not None and process.stderr is not None
    stdout = process.stdout.fileno()
    stderr = process.stderr.fileno()
    os.set_blocking(stdout, False)
    os.set_blocking(stderr, False)
    record = {
        "argv": argv,
        "artifactSha256": dict(sorted(prepared.written_sha256.items())),
        "candidateBinarySha256": measure_request_identity(prepared.request)[
            "binary_sha256"
        ],
        "copiedExecutableSha256": prepared.written_sha256["executable"],
        "environment": dict(sorted(environment.items())),
        "environmentSha256": sha256_json(dict(sorted(environment.items()))),
        "executableSha256": prepared.spec.executable_sha256,
        "pid": process.pid,
        "startedAt": started_at,
    }
    return OwnedProcess(
        process,
        prepared,
        {"result": result_read, "telemetry": telemetry_read, "stdout": stdout, "stderr": stderr},
        started_at,
        started_monotonic,
        started_monotonic + prepared.request.timeout_seconds,
        record,
        {name: _BoundedBytes(limit) for name in ("result", "telemetry", "stdout", "stderr")},
    )


def seal_launch_record(owned: OwnedProcess, directory: object) -> None:
    write_entry(
        directory,
        "launch-record.json",
        canonical_json_bytes(owned.launch_record) + b"\n",
    )


def terminate_and_wait(owned: OwnedProcess, deadline: float) -> bool:
    """Controller-owned terminate, kill fallback, wait, and positive observation."""
    process = owned.process
    if process.poll() is None:
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
    remaining = max(0.0, deadline - time.monotonic())
    try:
        process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        remaining = max(0.0, deadline - time.monotonic())
        try:
            process.wait(timeout=remaining)
        except subprocess.TimeoutExpired:
            return False
    owned.stopped = process.poll() is not None
    return owned.stopped


def _shape(payload: bytes) -> bool:
    depth = 0
    nodes = 1
    in_string = False
    escaped = False
    for byte in payload:
        if in_string:
            if escaped:
                escaped = False
            elif byte == 0x5C:
                escaped = True
            elif byte == 0x22:
                in_string = False
            continue
        if byte == 0x22:
            in_string = True
        elif byte in (0x5B, 0x7B):
            depth += 1
            nodes += 1
            if depth > 64:
                return False
        elif byte in (0x5D, 0x7D):
            depth -= 1
            if depth < 0:
                return False
        elif byte in (0x2C, 0x3A):
            nodes += 1
            if nodes > 10_000:
                return False
    return not in_string and depth == 0


def _json(payload: bytes) -> object:
    if not _shape(payload):
        raise ValueError("bounded JSON shape is invalid")
    return json.loads(payload)


def _captured(owned: OwnedProcess) -> CapturedResult:
    forced = any(buffer.truncated for buffer in owned.buffers.values())
    result_payload = bytes(owned.buffers["result"].payload)
    telemetry_payload = bytes(owned.buffers["telemetry"].payload)
    try:
        result = _json(result_payload)
        if type(result) is not dict or set(result) != {"output", "metadata"}:
            raise ValueError("result envelope is invalid")
        metadata = result["metadata"]
        if (
            type(metadata) is dict
            and type(metadata.get("trajectory")) is list
            and len(metadata["trajectory"]) > _MAX_EVENTS
        ):
            raise ValueError("event count exceeds the bound")
        output_bytes = canonical_json_bytes(result["output"])
        metadata_bytes = canonical_json_bytes(metadata)
    except (TypeError, UnicodeDecodeError, ValueError, json.JSONDecodeError):
        forced = True
        output_bytes = result_payload
        metadata_bytes = b"null"
    try:
        telemetry_value = _json(telemetry_payload)
        if type(telemetry_value) is not dict:
            raise ValueError("telemetry must be an object")
        telemetry = AttemptTelemetry(
            int(telemetry_value["requestCount"]),
            int(telemetry_value["inputTokens"]),
            int(telemetry_value["outputTokens"]),
            int(telemetry_value["costCny"]),
        )
        if any(value < 0 for value in telemetry.__dict__.values()):
            raise ValueError("telemetry values must be nonnegative")
    except (KeyError, TypeError, ValueError, json.JSONDecodeError):
        telemetry = AttemptTelemetry(0, 0, 0, 0)
        forced = True
    if owned.timed_out:
        try:
            metadata = json.loads(metadata_bytes)
        except (json.JSONDecodeError, UnicodeDecodeError):
            metadata = {}
        if type(metadata) is not dict:
            metadata = {}
        metadata["supervisorTimedOut"] = True
        metadata["timedOut"] = True
        metadata_bytes = canonical_json_bytes(metadata)
    actual = measure_request_identity(owned.prepared.request)
    written = owned.prepared.written_sha256
    attestation = AttemptAttestation(
        actual["binary_sha256"],
        actual["codex_home_seed_sha256"],
        written["config"],
        actual["workspace_seed_sha256"],
        written["profile"],
        written["route"],
        written["protocol"],
        written["promptfoo"],
    )
    raw = RawAttemptResult(
        owned.process.returncode if type(owned.process.returncode) is int else -1,
        owned.started_at,
        _now(),
        MemoryAttemptByteSource(output_bytes),
        MemoryAttemptByteSource(metadata_bytes),
        MemoryAttemptByteSource(bytes(owned.buffers["stdout"].payload)),
        MemoryAttemptByteSource(bytes(owned.buffers["stderr"].payload)),
        attestation,
    )
    return CapturedResult(raw, telemetry, forced, owned.launch_record)


def supervise_pair(owned: dict[str, OwnedProcess]) -> dict[str, CapturedResult]:
    """Select over all controller-owned descriptors until both attempts terminalize."""
    selector = selectors.DefaultSelector()
    descriptor_owner: dict[int, tuple[OwnedProcess, str]] = {}
    for attempt in owned.values():
        for name, descriptor in attempt.read_descriptors.items():
            selector.register(descriptor, selectors.EVENT_READ)
            descriptor_owner[descriptor] = (attempt, name)
    try:
        while descriptor_owner or any(item.process.poll() is None for item in owned.values()):
            now = time.monotonic()
            for item in owned.values():
                if item.process.poll() is None and now >= item.deadline:
                    item.timed_out = True
                    if not terminate_and_wait(item, now + 1.0):
                        raise FatalSupervisorError("fatal supervisor orphan: stop was not confirmed")
            timeout = min(
                [0.05]
                + [max(0.0, item.deadline - now) for item in owned.values() if item.process.poll() is None]
            )
            for key, _ in selector.select(timeout):
                descriptor = key.fd
                item, name = descriptor_owner[descriptor]
                try:
                    chunk = os.read(descriptor, _CHUNK_BYTES)
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(descriptor)
                    os.close(descriptor)
                    descriptor_owner.pop(descriptor)
                    continue
                item.buffers[name].add(chunk)
                if item.buffers[name].truncated and item.process.poll() is None:
                    if not terminate_and_wait(item, time.monotonic() + 1.0):
                        raise FatalSupervisorError("fatal supervisor orphan: stop was not confirmed")
            for item in owned.values():
                if item.process.poll() is not None:
                    item.stopped = True
        return {name: _captured(item) for name, item in owned.items()}
    finally:
        selector.close()
        for descriptor in list(descriptor_owner):
            try:
                os.close(descriptor)
            except OSError:
                pass
