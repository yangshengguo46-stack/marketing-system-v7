"""Self-contained sealed Promptfoo runner and bounded result decoder."""

import hashlib
import json
from dataclasses import dataclass


MAX_PROMPTFOO_RESULT_BYTES = 8 * 1024 * 1024
_MAX_CONFIG_BYTES = 1024 * 1024
_MAX_EVENTS = 1024
_MAX_JSON_DEPTH = 64
_MAX_JSON_NODES = 10_000
_COMMAND = (
    "eval",
    "--config",
    "cell/promptfoo/config.json",
    "--output",
    "cell/promptfoo/result.json",
    "--no-cache",
    "--no-progress-bar",
    "--no-table",
    "--no-share",
    "--no-write",
    "--max-concurrency",
    "1",
)


class PromptfooAdapterError(ValueError):
    pass


@dataclass(frozen=True, slots=True)
class PromptfooResult:
    output: object
    metadata: dict[str, object]
    telemetry: dict[str, int]


def _bounded_shape(payload: bytes) -> bool:
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
            if depth > _MAX_JSON_DEPTH:
                return False
        elif byte in (0x5D, 0x7D):
            depth -= 1
            if depth < 0:
                return False
        elif byte in (0x2C, 0x3A):
            nodes += 1
            if nodes > _MAX_JSON_NODES:
                return False
    return not in_string and depth == 0


def _decode_json(payload: bytes, label: str) -> object:
    if not _bounded_shape(payload):
        raise PromptfooAdapterError(f"{label} has an invalid bounded JSON shape")
    try:
        return json.loads(payload)
    except (UnicodeDecodeError, json.JSONDecodeError, RecursionError) as error:
        raise PromptfooAdapterError(f"{label} is not valid JSON") from error


def _mapping(value: object, label: str) -> dict[str, object]:
    if type(value) is not dict:
        raise PromptfooAdapterError(f"{label} must be an object")
    return value


def _has_provider_error(value: object) -> bool:
    return value is not None and value != ""


def _nonnegative_integer(value: object, label: str) -> int:
    if type(value) is not int or value < 0:
        raise PromptfooAdapterError(f"{label} must be a nonnegative integer")
    return value


def _usage_layer(
    value: object, label: str, *, require_requests: bool
) -> dict[str, int]:
    usage = _mapping(value, label)
    required = ("prompt", "completion", "total")
    fields = required + (("numRequests",) if require_requests else ())
    normalized = {
        field: _nonnegative_integer(usage.get(field), f"{label} {field}")
        for field in fields
    }
    if "numRequests" in usage and "numRequests" not in normalized:
        normalized["numRequests"] = _nonnegative_integer(
            usage["numRequests"], f"{label} numRequests"
        )
    if normalized["total"] != normalized["prompt"] + normalized["completion"]:
        raise PromptfooAdapterError(
            f"{label} total must equal prompt plus completion"
        )
    return normalized


def parse_promptfoo_result(
    payload: bytes, *, maximum_bytes: int = MAX_PROMPTFOO_RESULT_BYTES
) -> PromptfooResult:
    """Decode one untrusted Promptfoo output only after enforcing a byte cap."""
    if type(payload) is not bytes:
        raise PromptfooAdapterError("Promptfoo result must be bytes")
    if type(maximum_bytes) is not int or maximum_bytes < 1:
        raise PromptfooAdapterError("Promptfoo result byte bound is invalid")
    if len(payload) > maximum_bytes:
        raise PromptfooAdapterError("Promptfoo result exceeds the byte bound")
    root = _mapping(_decode_json(payload, "Promptfoo result"), "Promptfoo result")
    results = _mapping(root.get("results"), "Promptfoo results")
    rows = results.get("results")
    if type(rows) is not list or len(rows) != 1:
        raise PromptfooAdapterError("Promptfoo must contain exactly one result row")
    row = _mapping(rows[0], "Promptfoo result row")
    response = _mapping(row.get("response"), "Promptfoo provider response")
    if _has_provider_error(row.get("error")) or _has_provider_error(
        response.get("error")
    ):
        raise PromptfooAdapterError("Promptfoo reported a provider error")
    output_text = response.get("output")
    if type(output_text) is not str:
        raise PromptfooAdapterError("Promptfoo provider must return string output")
    output = _decode_json(output_text.encode(), "Promptfoo provider output")
    metadata = _mapping(response.get("metadata"), "Promptfoo provider metadata")
    codex = _mapping(metadata.get("codexAppServer"), "Codex App Server metadata")
    thread_id = codex.get("threadId")
    turn_id = codex.get("turnId")
    if (
        type(thread_id) is not str
        or not thread_id.strip()
        or type(turn_id) is not str
        or not turn_id.strip()
    ):
        raise PromptfooAdapterError("Codex App Server thread and turn IDs are required")
    items = codex.get("items")
    raw_text = response.get("raw")
    if type(raw_text) is not str:
        raise PromptfooAdapterError("Codex trajectory and raw events are required")
    raw = _mapping(
        _decode_json(raw_text.encode(), "Codex raw response"), "Codex raw response"
    )
    trajectory = raw.get("notifications")
    if (
        type(items) is not list
        or not items
        or len(items) > _MAX_EVENTS
        or type(trajectory) is not list
        or not trajectory
        or len(trajectory) > _MAX_EVENTS
    ):
        raise PromptfooAdapterError("Codex trajectory and raw events are required")
    response_usage = _usage_layer(
        response.get("tokenUsage"),
        "Promptfoo response token usage",
        require_requests=False,
    )
    row_usage = _usage_layer(
        row.get("tokenUsage"), "Promptfoo row token usage", require_requests=True
    )
    for field, response_value in response_usage.items():
        if row_usage.get(field) != response_value:
            raise PromptfooAdapterError(
                f"Promptfoo token usage layers disagree for {field}"
            )
    input_tokens = response_usage["prompt"]
    output_tokens = response_usage["completion"]
    total_tokens = response_usage["total"]
    request_count = row_usage["numRequests"]
    if request_count < 1:
        raise PromptfooAdapterError("request count must be positive")
    normalized_usage = {
        "inputTokens": input_tokens,
        "outputTokens": output_tokens,
        "totalTokens": total_tokens,
    }
    normalized_metadata = {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": hashlib.sha256(payload).hexdigest(),
        },
        "requestCount": request_count,
        "threadId": thread_id,
        "trajectory": trajectory,
        "turnId": turn_id,
        "usage": normalized_usage,
    }
    telemetry = {
        "costCny": 0,
        "inputTokens": input_tokens,
        "outputTokens": output_tokens,
        "requestCount": request_count,
    }
    return PromptfooResult(output, normalized_metadata, telemetry)
