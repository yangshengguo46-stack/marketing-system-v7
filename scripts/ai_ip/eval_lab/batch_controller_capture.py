"""Bound and normalize executor evidence without suppressing the paired call."""

import hashlib
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
    forced_evidence_failure: bool


def _now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def capture(executor: object, request: object, failure_factory: object) -> CapturedResult:
    try:
        raw = executor.execute(request)
    except BaseException as error:
        raw = failure_factory(error)
    return CapturedResult(raw)


def _bounded_json(value: object, limit: int, label: str) -> tuple[object, dict[str, object], bool]:
    malformed = False
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


def normalize(captured: CapturedResult, result_type: type, limit: int) -> NormalizedResult:
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
        invalid_result
        or bad_output
        or bad_metadata
        or bad_stdout
        or bad_stderr
        or not timestamps_valid
        or type(raw.exit_code) is not int,
    )
