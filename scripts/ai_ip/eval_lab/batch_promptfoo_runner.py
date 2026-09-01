"""Self-contained sealed Promptfoo runner and bounded result decoder."""

import hashlib
import json
import math
from dataclasses import dataclass


MAX_PROMPTFOO_RESULT_BYTES = 8 * 1024 * 1024
_MAX_CONFIG_BYTES = 1024 * 1024
_MAX_EVENTS = 1024
_MAX_EVIDENCE_ITEM_BYTES = 256 * 1024
_MAX_FINAL_RESPONSE_BYTES = 1024 * 1024
_MAX_TRAJECTORY_BYTES = 4 * 1024 * 1024
_MAX_JSON_DEPTH = 64
_MAX_JSON_NODES = 10_000
_MAX_SAFE_INTEGER = (1 << 53) - 1
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

    def reject_pairs(pairs: list[tuple[str, object]]) -> dict[str, object]:
        value: dict[str, object] = {}
        for key, item in pairs:
            if key in value:
                raise ValueError("duplicate JSON key")
            value[key] = item
        return value

    def safe_integer(token: str) -> int:
        value = int(token)
        if abs(value) > _MAX_SAFE_INTEGER:
            raise ValueError("JSON integer exceeds the safe range")
        return value

    def finite_number(token: str) -> float:
        value = float(token)
        if not math.isfinite(value) or (
            value.is_integer() and abs(value) > _MAX_SAFE_INTEGER
        ):
            raise ValueError("JSON number must be finite")
        return value

    def reject_constant(token: str) -> object:
        raise ValueError(f"non-standard JSON constant: {token}")

    try:
        return json.loads(
            payload.decode("utf-8", errors="strict"),
            object_pairs_hook=reject_pairs,
            parse_constant=reject_constant,
            parse_float=finite_number,
            parse_int=safe_integer,
        )
    except (
        UnicodeDecodeError,
        json.JSONDecodeError,
        RecursionError,
        ValueError,
    ) as error:
        raise PromptfooAdapterError(f"{label} is not valid strict JSON") from error


def _mapping(value: object, label: str) -> dict[str, object]:
    if type(value) is not dict:
        raise PromptfooAdapterError(f"{label} must be an object")
    return value


def _strict_utf8(value: str, label: str) -> bytes:
    try:
        return value.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise PromptfooAdapterError(f"{label} contains invalid Unicode") from error


def _has_provider_error(value: object) -> bool:
    return value is not None and value != ""


def _nonnegative_integer(value: object, label: str) -> int:
    if type(value) is not int or not 0 <= value <= _MAX_SAFE_INTEGER:
        raise PromptfooAdapterError(f"{label} must be a nonnegative integer")
    return value


def _usage_layer(
    value: object, label: str, *, require_requests: bool
) -> dict[str, int]:
    usage = _mapping(value, label)
    required = ("prompt", "completion", "cached", "total")
    fields = required + (("numRequests",) if require_requests else ())
    allowed = set(fields) | ({"assertions"} if require_requests else set())
    if set(usage) != allowed and not (
        require_requests and set(usage) == set(fields)
    ):
        raise PromptfooAdapterError(f"{label} has invalid token usage fields")
    normalized = {
        field: _nonnegative_integer(usage.get(field), f"{label} {field}")
        for field in fields
    }
    if normalized["total"] != normalized["prompt"] + normalized["completion"]:
        raise PromptfooAdapterError(
            f"{label} total must equal prompt plus completion"
        )
    return normalized


def _canonical(value: object) -> bytes:
    if type(value) is dict:
        return b"{" + b",".join(
            _canonical(key) + b":" + _canonical(value[key])
            for key in sorted(value, key=_utf8_key)
        ) + b"}"
    if type(value) is list:
        return b"[" + b",".join(_canonical(item) for item in value) + b"]"
    if value is None or type(value) in (bool, int, str):
        try:
            return json.dumps(
                value,
                ensure_ascii=False,
                separators=(",", ":"),
            ).encode("utf-8", errors="strict")
        except UnicodeEncodeError as error:
            raise PromptfooAdapterError("evidence contains invalid Unicode") from error
    raise PromptfooAdapterError("evidence contains unsupported values")


def _utf8_key(value: str) -> bytes:
    try:
        return value.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise PromptfooAdapterError("evidence contains invalid Unicode") from error


def _bounded_evidence(value: object, maximum: int, label: str) -> None:
    def validate(item: object) -> None:
        if item is None or type(item) in (bool, str):
            if type(item) is str and any(0xD800 <= ord(char) <= 0xDFFF for char in item):
                raise PromptfooAdapterError(f"{label} contains invalid Unicode")
            return
        if type(item) is int and abs(item) <= _MAX_SAFE_INTEGER:
            return
        if type(item) is list:
            for child in item:
                validate(child)
            return
        if type(item) is dict and all(type(key) is str for key in item):
            for key, child in item.items():
                validate(key)
                validate(child)
            return
        raise PromptfooAdapterError(f"{label} contains unsupported evidence values")

    validate(value)
    if len(_canonical(value)) > maximum:
        raise PromptfooAdapterError(f"{label} exceeds its evidence byte bound")


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
        _decode_json(
            _strict_utf8(raw_text, "Codex raw response"), "Codex raw response"
        ),
        "Codex raw response",
    )
    notifications = raw.get("notifications")
    raw_items = raw.get("items")
    raw_output = raw.get("output")
    final_response = raw.get("finalResponse")
    if (
        type(items) is not list
        or not items
        or len(items) > _MAX_EVENTS
        or type(raw_items) is not list
        or not raw_items
        or len(raw_items) > _MAX_EVENTS
        or type(notifications) is not list
        or not notifications
        or len(notifications) > _MAX_EVENTS
        or type(raw_output) is not str
        or type(final_response) is not str
    ):
        raise PromptfooAdapterError("Codex trajectory and raw events are required")
    if output_text != raw_output or output_text != final_response:
        raise PromptfooAdapterError(
            "Promptfoo final response differs across retained output fields"
        )
    output = _decode_json(
        _strict_utf8(output_text, "Promptfoo final response"),
        "Promptfoo provider output",
    )
    _bounded_evidence(output, _MAX_FINAL_RESPONSE_BYTES, "Promptfoo final response")
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
    for label, values in (
        ("Codex trajectory item", items),
        ("Codex raw item", raw_items),
        ("Codex notification", notifications),
    ):
        for value in values:
            _bounded_evidence(value, _MAX_EVIDENCE_ITEM_BYTES, label)
    _bounded_evidence(
        final_response, _MAX_FINAL_RESPONSE_BYTES, "Codex final response"
    )
    trajectory = {
        "finalResponse": final_response,
        "items": items,
        "notifications": notifications,
        "rawItems": raw_items,
        "responseUsage": response_usage,
        "rowUsage": row_usage,
    }
    _bounded_evidence(trajectory, _MAX_TRAJECTORY_BYTES, "Codex retained evidence")
    trajectory_bytes = _canonical(trajectory)
    normalized_usage = {
        "inputTokens": input_tokens,
        "outputTokens": output_tokens,
        "totalTokens": total_tokens,
    }
    normalized_metadata = {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": hashlib.sha256(trajectory_bytes).hexdigest(),
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
