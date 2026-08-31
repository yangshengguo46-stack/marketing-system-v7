"""Bounded decoding of controller-acquired process streams."""

import hashlib
import json

try:
    from .batch_controller_capture import CapturedResult
    from .batch_controller_identity import measure_request_identity
    from .batch_controller_types import (
        AttemptAttestation,
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_controller_capture import CapturedResult
    from batch_controller_identity import measure_request_identity
    from batch_controller_types import (
        AttemptAttestation,
        AttemptTelemetry,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from contracts import canonical_json_bytes


_MAX_EVENTS = 1_024


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


def captured_result(owned: object, finished_at: str) -> CapturedResult:
    forced = owned.budget.exhausted or any(
        buffer.truncated for buffer in owned.buffers.values()
    )
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
        metadata_bytes = canonical_json_bytes(
            {
                "costEvidence": {
                    "costCny": 0,
                    "sourceSha256": hashlib.sha256(
                        b"invalid-result-envelope"
                    ).hexdigest(),
                },
                "requestCount": 0,
                "threadId": None,
                "trajectory": None,
                "turnId": None,
                "usage": {"inputTokens": 0, "outputTokens": 0, "totalTokens": 0},
            }
        )
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
        finished_at,
        MemoryAttemptByteSource(output_bytes),
        MemoryAttemptByteSource(metadata_bytes),
        MemoryAttemptByteSource(bytes(owned.buffers["stdout"].payload)),
        MemoryAttemptByteSource(bytes(owned.buffers["stderr"].payload)),
        attestation,
    )
    evidence = {name: buffer.evidence() for name, buffer in owned.buffers.items()}
    evidence["aggregate"] = {
        "acquiredSize": owned.budget.acquired,
        "limit": owned.budget.limit,
        "storedSize": sum(len(buffer.payload) for buffer in owned.buffers.values()),
        "truncated": owned.budget.exhausted,
    }
    return CapturedResult(raw, telemetry, forced, owned.launch_record, evidence)
