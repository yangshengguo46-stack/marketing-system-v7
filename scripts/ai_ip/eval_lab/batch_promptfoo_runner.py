"""Self-contained sealed Promptfoo runner and bounded result decoder."""

import hashlib
import json
import os
import stat
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


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
    usage = _mapping(response.get("tokenUsage"), "Promptfoo token usage")
    input_tokens = _nonnegative_integer(usage.get("prompt"), "token usage prompt")
    output_tokens = _nonnegative_integer(
        usage.get("completion"), "token usage completion"
    )
    total_tokens = _nonnegative_integer(usage.get("total"), "token usage total")
    if total_tokens != input_tokens + output_tokens:
        raise PromptfooAdapterError(
            "token usage total must equal prompt plus completion"
        )
    row_usage = _mapping(row.get("tokenUsage"), "Promptfoo row token usage")
    request_count = _nonnegative_integer(row_usage.get("numRequests"), "request count")
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


def _read_bounded(path: Path, maximum: int) -> bytes:
    try:
        with path.open("rb") as stream:
            payload = stream.read(maximum + 1)
    except OSError as error:
        raise PromptfooAdapterError("Promptfoo input is unavailable") from error
    if len(payload) > maximum:
        raise PromptfooAdapterError("Promptfoo input exceeds the byte bound")
    return payload


def _write_all(descriptor: int, payload: bytes) -> None:
    view = memoryview(payload)
    while view:
        written = os.write(descriptor, view)
        if written < 1:
            raise PromptfooAdapterError("controller descriptor write made no progress")
        view = view[written:]


def _write_file_at(directory: int, name: str, payload: bytes) -> None:
    descriptor = os.open(
        name,
        os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
        0o600,
        dir_fd=directory,
    )
    try:
        _write_all(descriptor, payload)
        os.fchmod(descriptor, 0o600)
    finally:
        os.close(descriptor)


def _same_file(left: os.stat_result, right: os.stat_result) -> bool:
    return (left.st_dev, left.st_ino) == (right.st_dev, right.st_ino)


def _read_result_at(directory: int, directory_path: Path) -> bytes:
    retained = os.fstat(directory)
    current = directory_path.lstat()
    if (
        not stat.S_ISDIR(current.st_mode)
        or stat.S_ISLNK(current.st_mode)
        or not _same_file(retained, current)
    ):
        raise PromptfooAdapterError("Promptfoo result path escaped its directory")
    descriptor = os.open(
        "result.json",
        os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0),
        dir_fd=directory,
    )
    try:
        before = os.fstat(descriptor)
        if (
            not stat.S_ISREG(before.st_mode)
            or before.st_size > MAX_PROMPTFOO_RESULT_BYTES
        ):
            raise PromptfooAdapterError("Promptfoo result path is not a bounded file")
        payload = os.read(descriptor, MAX_PROMPTFOO_RESULT_BYTES + 1)
        after = os.fstat(descriptor)
        if not _same_file(before, after) or len(payload) != before.st_size:
            raise PromptfooAdapterError("Promptfoo result changed while read")
        return payload
    except OSError as error:
        raise PromptfooAdapterError("Promptfoo result path is unsafe") from error
    finally:
        os.close(descriptor)


def _canonical(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def main() -> int:
    """Run only when this source has been sealed as a controller launch artifact."""
    directory = -1
    try:
        cli = os.environ["AI_IP_PROMPTFOO_CLI"]
        config_source = Path(os.environ["AI_IP_PROMPTFOO_PATH"])
        result_fd = int(os.environ["AI_IP_RESULT_FD"])
        telemetry_fd = int(os.environ["AI_IP_TELEMETRY_FD"])
        cell = Path.cwd() / "cell"
        promptfoo = cell / "promptfoo"
        cell.mkdir(mode=0o700)
        promptfoo.mkdir(mode=0o700)
        directory = os.open(
            promptfoo,
            os.O_RDONLY | getattr(os, "O_DIRECTORY", 0) | getattr(os, "O_NOFOLLOW", 0),
        )
        _write_file_at(
            directory, "config.json", _read_bounded(config_source, _MAX_CONFIG_BYTES)
        )
        completed = subprocess.run((cli, *_COMMAND), check=False)
        if completed.returncode != 0:
            return completed.returncode
        parsed = parse_promptfoo_result(_read_result_at(directory, promptfoo))
        _write_all(
            result_fd,
            _canonical({"metadata": parsed.metadata, "output": parsed.output}),
        )
        _write_all(telemetry_fd, _canonical(parsed.telemetry))
        return 0
    except (KeyError, OSError, TypeError, ValueError) as error:
        message = f"Promptfoo sealed runner failed: {type(error).__name__}: {error}\n"
        _write_all(2, message.encode()[:4096])
        return 2
    finally:
        if directory >= 0:
            os.close(directory)


if __name__ == "__main__":
    sys.exit(main())
