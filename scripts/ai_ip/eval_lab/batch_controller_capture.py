"""Start, supervise, and incrementally normalize bounded executor evidence."""

import hashlib
import json
import time
from dataclasses import dataclass
from datetime import datetime, timezone

try:
    from .batch_controller_identity import measure_request_identity
    from .batch_controller_types import (
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
        validate_handle,
        validate_source,
    )
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_controller_identity import measure_request_identity
    from batch_controller_types import (
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
        validate_handle,
        validate_source,
    )
    from contracts import canonical_json_bytes


class FatalSupervisorError(RuntimeError):
    """The controller cannot prove that candidate work stopped."""


@dataclass(frozen=True)
class CapturedResult:
    raw: object
    telemetry: AttemptTelemetry
    forced_evidence_failure: bool = False
    launch_record: dict[str, object] | None = None


@dataclass(frozen=True)
class NormalizedResult:
    exit_code: int
    started_at: str
    finished_at: str
    output: object
    metadata: object
    stdout: bytes
    stderr: bytes
    raw_evidence: dict[str, object]
    attestation: object
    forced_evidence_failure: bool


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def start(executor: object, request: object) -> object:
    """Remeasure the cell, then perform the adapter's one nonblocking start."""
    measure_request_identity(request)
    return validate_handle(executor.start(request))


def _zero_telemetry() -> AttemptTelemetry:
    return AttemptTelemetry(0, 0, 0, 0)


def capture(handle: object, request: object, failure_factory: object) -> CapturedResult:
    """Supervise one already-started handle without blocking past its deadline."""
    try:
        handle = validate_handle(handle)
        deadline = time.monotonic() + request.timeout_seconds
        raw = None
        while raw is None:
            raw = handle.poll()
            if raw is not None:
                break
            if time.monotonic() >= deadline:
                stop_deadline = time.monotonic() + min(1.0, request.timeout_seconds)
                if handle.terminate_and_wait(stop_deadline) is not True:
                    raise FatalSupervisorError(
                        "fatal supervisor orphan: stop was not confirmed"
                    )
                raise TimeoutError("candidate exceeded the supervisor deadline")
            time.sleep(0.005)
        telemetry = handle.telemetry()
        if not isinstance(telemetry, AttemptTelemetry):
            raise ValueError("trusted executor telemetry is missing")
        return CapturedResult(raw, telemetry)
    except FatalSupervisorError:
        raise
    except BaseException as error:
        return CapturedResult(failure_factory(error), _zero_telemetry())


def capture_start_failure(
    error: BaseException, failure_factory: object
) -> CapturedResult:
    return CapturedResult(failure_factory(error), _zero_telemetry())


def _collect(source: object, limit: int) -> tuple[bytes, dict[str, object], bool]:
    try:
        source = validate_source(source)
    except ValueError:
        return b"", _byte_evidence(b"", False), True
    chunks: list[bytes] = []
    size = 0
    oversized = False
    digest = hashlib.sha256()
    while True:
        remaining = limit + 1 - size
        if remaining <= 0:
            oversized = True
            break
        chunk = source.read(min(64 * 1024, remaining))
        if type(chunk) is not bytes or len(chunk) > min(64 * 1024, remaining):
            return b"".join(chunks), _byte_evidence(b"".join(chunks), False), True
        if not chunk:
            break
        chunks.append(chunk)
        digest.update(chunk)
        size += len(chunk)
    payload = b"".join(chunks)
    if len(payload) > limit:
        oversized = True
    evidence = {
        "rawSha256": digest.hexdigest(),
        "rawSize": size,
        "storedSha256": hashlib.sha256(payload[:limit]).hexdigest(),
        "storedSize": min(size, limit),
        "truncated": oversized,
    }
    return payload[:limit], evidence, oversized


def _byte_evidence(payload: bytes, truncated: bool) -> dict[str, object]:
    return {
        "rawSha256": hashlib.sha256(payload).hexdigest(),
        "rawSize": len(payload),
        "storedSha256": hashlib.sha256(payload).hexdigest(),
        "storedSize": len(payload),
        "truncated": truncated,
    }


def _json_shape(payload: bytes) -> bool:
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


def _bounded_json(
    source: object, limit: int, label: str
) -> tuple[object, dict[str, object], bool]:
    payload, evidence, malformed = _collect(source, limit)
    if not malformed and _json_shape(payload):
        try:
            value = json.loads(payload)
            stored = canonical_json_bytes(value) + b"\n"
            evidence["storedSha256"] = hashlib.sha256(stored).hexdigest()
            evidence["storedSize"] = len(stored)
            return value, evidence, False
        except (UnicodeDecodeError, TypeError, ValueError):
            malformed = True
    else:
        malformed = True
    normalized = {
        f"{label}Evidence": "oversized" if evidence["truncated"] else "malformed",
        "rawSha256": evidence["rawSha256"],
        "rawSize": evidence["rawSize"],
    }
    stored = canonical_json_bytes(normalized) + b"\n"
    evidence["storedSha256"] = hashlib.sha256(stored).hexdigest()
    evidence["storedSize"] = len(stored)
    return normalized, evidence, malformed


def _attestation(value: object) -> dict[str, object] | None:
    fields = (
        "app_server_protocol_schema_sha256",
        "binary_sha256",
        "codex_home_seed_sha256",
        "effective_config_sha256",
        "execution_profile_sha256",
        "model_route_sha256",
        "promptfoo_config_sha256",
        "workspace_seed_sha256",
    )
    if not all(hasattr(value, field) for field in fields):
        return None
    return {
        "appServerProtocolSchemaSha256": value.app_server_protocol_schema_sha256,
        "binarySha256": value.binary_sha256,
        "codexHomeSeedSha256": value.codex_home_seed_sha256,
        "effectiveConfigSha256": value.effective_config_sha256,
        "executionProfileSha256": value.execution_profile_sha256,
        "modelRouteSha256": value.model_route_sha256,
        "promptfooConfigSha256": value.promptfoo_config_sha256,
        "workspaceSeedSha256": value.workspace_seed_sha256,
    }


def normalize(
    captured: CapturedResult, result_type: type, limit: int
) -> NormalizedResult:
    raw = captured.raw
    invalid_result = not isinstance(raw, result_type)
    if invalid_result:
        now = _now()
        empty = lambda value: MemoryAttemptByteSource(value)
        raw = result_type(
            -1, now, now, empty(b"null"), empty(b"null"), empty(b""), empty(b"")
        )
    output, output_evidence, bad_output = _bounded_json(raw.output, limit, "output")
    metadata, metadata_evidence, bad_metadata = _bounded_json(
        raw.metadata, limit, "metadata"
    )
    stdout, stdout_evidence, bad_stdout = _collect(raw.stdout, limit)
    stderr, stderr_evidence, bad_stderr = _collect(raw.stderr, limit)
    if type(metadata) is dict:
        metadata = dict(metadata)
        telemetry = captured.telemetry
        metadata["requestCount"] = telemetry.request_count
        metadata["usage"] = {
            "inputTokens": telemetry.input_tokens,
            "outputTokens": telemetry.output_tokens,
            "totalTokens": telemetry.input_tokens + telemetry.output_tokens,
        }
        cost = metadata.get("costEvidence")
        metadata["costEvidence"] = {
            **(cost if type(cost) is dict else {}),
            "costCny": telemetry.cost_cny,
            "sourceSha256": (
                cost.get("sourceSha256")
                if type(cost) is dict and type(cost.get("sourceSha256")) is str
                else hashlib.sha256(b"controller-owned-telemetry").hexdigest()
            ),
        }
        stored_metadata = canonical_json_bytes(metadata) + b"\n"
        metadata_evidence["storedSha256"] = hashlib.sha256(stored_metadata).hexdigest()
        metadata_evidence["storedSize"] = len(stored_metadata)
    timestamps_valid = (
        type(raw.started_at) is str
        and type(raw.finished_at) is str
        and len(raw.started_at) <= 128
        and len(raw.finished_at) <= 128
    )
    if timestamps_valid:
        try:
            parsed = (
                datetime.fromisoformat(raw.started_at.replace("Z", "+00:00")),
                datetime.fromisoformat(raw.finished_at.replace("Z", "+00:00")),
            )
            timestamps_valid = all(value.tzinfo is not None for value in parsed)
        except ValueError:
            timestamps_valid = False
    now = _now()
    started_at = raw.started_at if timestamps_valid else now
    finished_at = raw.finished_at if timestamps_valid else now
    exit_code = raw.exit_code if type(raw.exit_code) is int else -1
    attestation = _attestation(raw.attestation)
    raw_evidence = {
        "attestation": attestation,
        "exitCode": exit_code,
        "finishedAt": raw.finished_at if type(raw.finished_at) is str else None,
        "metadataBytes": metadata_evidence,
        "outputBytes": output_evidence,
        "requestCount": captured.telemetry.request_count,
        "startedAt": raw.started_at if type(raw.started_at) is str else None,
        "stderrBytes": stderr_evidence,
        "stdoutBytes": stdout_evidence,
    }
    if captured.launch_record is not None:
        raw_evidence["launchRecordSha256"] = hashlib.sha256(
            canonical_json_bytes(captured.launch_record) + b"\n"
        ).hexdigest()
    return NormalizedResult(
        exit_code,
        started_at,
        finished_at,
        output,
        metadata,
        stdout,
        stderr,
        raw_evidence,
        attestation,
        invalid_result
        or captured.forced_evidence_failure
        or bad_output
        or bad_metadata
        or bad_stdout
        or bad_stderr
        or not timestamps_valid
        or type(raw.exit_code) is not int,
    )
