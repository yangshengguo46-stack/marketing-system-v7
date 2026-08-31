"""Bound and normalize executor evidence without suppressing the paired call."""

import hashlib
import time
from dataclasses import dataclass
from datetime import datetime, timezone

try:
    from .contracts import LabContractError, canonical_json_bytes
except ImportError:
    from contracts import LabContractError, canonical_json_bytes


@dataclass(frozen=True)
class CapturedResult:
    raw: object


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


def capture(
    executor: object, request: object, failure_factory: object
) -> CapturedResult:
    try:
        handle = executor.start(request)
        deadline = time.monotonic() + request.timeout_seconds
        raw = None
        timed_out = False
        while raw is None:
            raw = handle.poll()
            if raw is not None:
                break
            if time.monotonic() >= deadline:
                timed_out = True
                if handle.terminate() is not True:
                    raise RuntimeError("executor termination was not confirmed")
                raise TimeoutError("candidate exceeded the supervisor deadline")
            time.sleep(0.005)
        telemetry = handle.telemetry()
        if timed_out:
            raise TimeoutError("candidate exceeded the supervisor deadline")
        raw = _bind_telemetry(raw, telemetry)
    except BaseException as error:
        raw = failure_factory(error)
    return CapturedResult(raw)


def _bind_telemetry(raw: object, telemetry: object) -> object:
    from dataclasses import replace

    if not all(
        hasattr(telemetry, name)
        for name in ("request_count", "input_tokens", "output_tokens", "cost_cny")
    ):
        raise ValueError("trusted executor telemetry is missing")
    if not hasattr(raw, "metadata") or type(raw.metadata) is not dict:
        return raw
    metadata = dict(raw.metadata)
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
    }
    return replace(raw, metadata=metadata)


def _bounded_json(
    value: object, limit: int, label: str
) -> tuple[object, dict[str, object], bool]:
    malformed = not _bounded_shape(value, limit)
    if malformed:
        payload = b""
    else:
        try:
            payload = canonical_json_bytes(value)
        except (LabContractError, TypeError, ValueError):
            payload = b""
            malformed = True
    oversized = len(payload) > limit
    if malformed or oversized:
        normalized: object = {
            f"{label}Evidence": "malformed" if malformed else "oversized",
            "rawSha256": hashlib.sha256(payload).hexdigest(),
            "rawSize": len(payload),
        }
    else:
        normalized = value
    stored = canonical_json_bytes(normalized)
    evidence = {
        "rawSha256": hashlib.sha256(payload).hexdigest(),
        "rawSize": len(payload),
        "storedSha256": hashlib.sha256(stored + b"\n").hexdigest(),
        "storedSize": len(stored) + 1,
    }
    return normalized, evidence, malformed


def _bounded_shape(value: object, limit: int) -> bool:
    """Reject deep/large structures before recursive canonical serialization."""
    pending = [(value, 0)]
    nodes = 0
    scalar_bytes = 0
    while pending:
        item, depth = pending.pop()
        nodes += 1
        if depth > 64 or nodes > 10_000:
            return False
        if item is None or type(item) in (bool, int, float):
            scalar_bytes += 16
        elif type(item) is str:
            scalar_bytes += len(item.encode("utf-8", errors="replace"))
        elif type(item) is list:
            pending.extend((child, depth + 1) for child in item)
        elif type(item) is dict:
            for key, child in item.items():
                if type(key) is not str:
                    return False
                scalar_bytes += len(key.encode("utf-8", errors="replace"))
                pending.append((child, depth + 1))
        else:
            return False
        if scalar_bytes > limit:
            return False
    return True


def _bounded_stream(value: object, limit: int) -> tuple[bytes, dict[str, object], bool]:
    malformed = type(value) is not bytes
    raw = value if type(value) is bytes else b""
    stored = raw[:limit]
    return (
        stored,
        {
            "rawSha256": hashlib.sha256(raw).hexdigest(),
            "rawSize": len(raw),
            "storedSha256": hashlib.sha256(stored).hexdigest(),
            "storedSize": len(stored),
        },
        malformed,
    )


def normalize(
    captured: CapturedResult, result_type: type, limit: int
) -> NormalizedResult:
    raw = captured.raw
    if not isinstance(raw, result_type):
        now = _now()
        raw = result_type(-1, now, now, None, None, b"", b"")
        invalid_result = True
    else:
        invalid_result = False
    output, output_evidence, bad_output = _bounded_json(raw.output, limit, "output")
    metadata, metadata_evidence, bad_metadata = _bounded_json(
        raw.metadata, limit, "metadata"
    )
    stdout, stdout_evidence, bad_stdout = _bounded_stream(raw.stdout, limit)
    stderr, stderr_evidence, bad_stderr = _bounded_stream(raw.stderr, limit)
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
    raw_evidence = {
        "exitCode": exit_code,
        "finishedAt": raw.finished_at if type(raw.finished_at) is str else None,
        "metadataBytes": metadata_evidence,
        "outputBytes": output_evidence,
        "requestCount": (
            raw.metadata.get("requestCount") if type(raw.metadata) is dict else None
        ),
        "startedAt": raw.started_at if type(raw.started_at) is str else None,
        "stderrBytes": stderr_evidence,
        "stdoutBytes": stdout_evidence,
        "attestation": (
            {
                "appServerProtocolSchemaSha256": raw.attestation.app_server_protocol_schema_sha256,
                "binarySha256": raw.attestation.binary_sha256,
                "codexHomeSeedSha256": raw.attestation.codex_home_seed_sha256,
                "effectiveConfigSha256": raw.attestation.effective_config_sha256,
                "executionProfileSha256": raw.attestation.execution_profile_sha256,
                "modelRouteSha256": raw.attestation.model_route_sha256,
                "promptfooConfigSha256": raw.attestation.promptfoo_config_sha256,
            }
            if hasattr(raw.attestation, "binary_sha256")
            else None
        ),
    }
    return NormalizedResult(
        exit_code,
        started_at,
        finished_at,
        output,
        metadata,
        stdout,
        stderr,
        raw_evidence,
        raw_evidence["attestation"],
        invalid_result
        or bad_output
        or bad_metadata
        or bad_stdout
        or bad_stderr
        or not timestamps_valid
        or type(raw.exit_code) is not int,
    )
