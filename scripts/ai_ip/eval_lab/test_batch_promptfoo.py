import copy
import gzip
import hashlib
import importlib
import json
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest


MODULE_ROOT = Path(__file__).resolve().parent
REPO_ROOT = Path(__file__).resolve().parents[3]
FROZEN_RESULT = (
    REPO_ROOT / "ai-ip-evals/lab/fixtures/batch-runner/promptfoo-result.json"
)
BASELINE_ANSWER = '{"caseId":"case-1","objectKind":"CaseAnswer","schemaVersion":1}'
ANSWER_EVIDENCE_CORPUS = (
    pytest.param(
        '{"\ue000":"bmp","\U00010000":"non-bmp"}',
        True,
        {"\ue000": "bmp", "\U00010000": "non-bmp"},
        id="bmp-and-non-bmp-keys",
    ),
    pytest.param(
        '{"gift":"\u793c\\ud83c\\udf81"}',
        True,
        {"gift": "礼🎁"},
        id="escaped-equivalent",
    ),
    pytest.param('{"value":-0}', True, {"value": 0}, id="negative-zero"),
    pytest.param('{"value":"\\ud800"}', False, None, id="unpaired-high-surrogate"),
    pytest.param('{"value":"\\udc00"}', False, None, id="unpaired-low-surrogate"),
    pytest.param('{"value":1.0}', False, None, id="decimal"),
    pytest.param('{"value":1e0}', False, None, id="lowercase-exponent"),
    pytest.param('{"value":1E+1}', False, None, id="uppercase-exponent"),
    pytest.param('{"value":1,"value":2}', False, None, id="duplicate-key"),
    pytest.param('{"value":NaN}', False, None, id="nan"),
    pytest.param('{"value":Infinity}', False, None, id="infinity"),
    pytest.param(
        '{"value":9007199254740992}', False, None, id="positive-unsafe-integer"
    ),
    pytest.param(
        '{"value":-9007199254740992}', False, None, id="negative-unsafe-integer"
    ),
)
sys.path.insert(0, str(MODULE_ROOT))


def _adapter():
    return importlib.import_module("batch_promptfoo")


def _request(**changes: object) -> SimpleNamespace:
    values = {
        "app_server_protocol_schema": b'{"version":2}',
        "binary_path": Path("/private/arm-specific/codex"),
        "case_answer_schema_json": b'{"type":"object"}',
        "cell": SimpleNamespace(root=Path("/private/arm-specific/cell")),
        "effective_config": b"arm-specific-config",
        "execution_profile_json": json.dumps(
            {
                "approvalPolicy": "never",
                "maxWallClockSeconds": 30,
                "sandboxMode": "workspace-write",
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode(),
        "model_route_json": b'{"baseUrl":"http://127.0.0.1:1/v1","model":"loopback-model","reasoningEffort":"medium"}',
        "promptfoo_config": b"",
    }
    values.update(changes)
    return SimpleNamespace(**values)


def _json_bytes(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def _canonical_bytes(value: object) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode()


def _result_value() -> dict[str, object]:
    value = json.loads(FROZEN_RESULT.read_bytes())
    assert type(value) is dict
    _set_answer_fields(value, BASELINE_ANSWER)
    return value


def _result_bytes(value: dict[str, object]) -> bytes:
    return _json_bytes(value)


def _row(value: dict[str, object]) -> dict[str, object]:
    results = value["results"]
    assert type(results) is dict
    rows = results["results"]
    assert type(rows) is list
    row = rows[0]
    assert type(row) is dict
    return row


def _response(value: dict[str, object]) -> dict[str, object]:
    response = _row(value)["response"]
    assert type(response) is dict
    return response


def _set_answer_fields(value: dict[str, object], answer_text: str) -> None:
    response = _response(value)
    raw = json.loads(response["raw"])
    assert type(raw) is dict
    response["output"] = answer_text
    raw["output"] = answer_text
    raw["finalResponse"] = answer_text
    response["raw"] = json.dumps(raw, ensure_ascii=False, separators=(",", ":"))


def _mutate_one_answer_field(
    value: dict[str, object], field: str
) -> dict[str, object]:
    mutated = copy.deepcopy(value)
    response = _response(mutated)
    raw = json.loads(response["raw"])
    assert type(raw) is dict
    replacement = "not valid JSON"
    if field == "response":
        response["output"] = replacement
    elif field == "rawOutput":
        raw["output"] = replacement
    else:
        assert field == "rawFinalResponse"
        raw["finalResponse"] = replacement
    response["raw"] = json.dumps(raw, ensure_ascii=False, separators=(",", ":"))
    return mutated


def _run_js_parser(payload: bytes) -> subprocess.CompletedProcess[bytes]:
    source = """
const fs = require('node:fs');
const parser = require(process.argv[1]);
try {
  const parsed = parser.parseResult(fs.readFileSync(0));
  process.stdout.write(parser.canonical(parsed));
} catch (error) {
  process.stderr.write(`${error.name}: ${error.message}`);
  process.exitCode = 2;
}
"""
    return subprocess.run(
        [
            shutil.which("node") or "node",
            "-e",
            source,
            str(MODULE_ROOT / "batch_promptfoo_result.js"),
        ],
        input=payload,
        capture_output=True,
        check=False,
    )


def test_config_is_path_neutral_and_forces_one_isolated_app_server() -> None:
    adapter = _adapter()
    request = _request()

    config = adapter.render_promptfoo_config(request)

    assert set(config) == {"prompts", "providers", "tests"}
    assert len(config["prompts"]) == 1
    assert config["tests"] == [{"vars": {}}]
    provider = config["providers"][0]
    assert provider == {
        "id": "openai:codex-app-server",
        "config": {
            "approval_policy": "never",
            "apiKey": "ai-ip-public-loopback-dummy",
            "base_url": "http://127.0.0.1:1/v1",
            "cli_env": {
                "AI_IP_CASE_PATH": "{{ env.AI_IP_CASE_PATH }}",
                "AI_IP_CONFIG_PATH": "{{ env.AI_IP_CONFIG_PATH }}",
                "AI_IP_PROFILE_PATH": "{{ env.AI_IP_PROFILE_PATH }}",
                "AI_IP_PROTOCOL_PATH": "{{ env.AI_IP_PROTOCOL_PATH }}",
                "AI_IP_ROUTE_PATH": "{{ env.AI_IP_ROUTE_PATH }}",
                "AI_IP_SCHEMA_PATH": "{{ env.AI_IP_SCHEMA_PATH }}",
                "CODEX_HOME": "{{ env.CODEX_HOME }}",
                "HOME": "{{ env.HOME }}",
                "OPENAI_API_KEY": "ai-ip-public-loopback-dummy",
                "TMPDIR": "{{ env.TMPDIR }}",
            },
            "codex_path_override": "{{ env.AI_IP_CODEX_PATH }}",
            "ephemeral": True,
            "include_raw_events": True,
            "inherit_process_env": False,
            "model": "loopback-model",
            "model_provider": "openai",
            "model_reasoning_effort": "medium",
            "output_schema": {"type": "object"},
            "request_timeout_ms": 30000,
            "reuse_server": False,
            "sandbox_mode": "workspace-write",
            "startup_timeout_ms": 30000,
            "turn_timeout_ms": 30000,
            "working_dir": "{{ env.AI_IP_WORKSPACE }}",
        },
    }
    rendered = _json_bytes(config)
    assert b"/private/arm-specific" not in rendered
    assert all(
        token not in rendered
        for token in (b'"assert"', b"rubric", b"reference", b"javascript")
    )


@pytest.mark.parametrize(
    "base_url",
    [
        "https://127.0.0.1:8443/v1",
        "http://localhost:8000/v1",
        "http://example.com:8000/v1",
        "http://10.0.0.1:8000/v1",
        "http://user:pass@127.0.0.1:8000/v1",
        "http://127.0.0.1/v1",
        "http://127.0.0.1:8000/../admin",
        "http://127.0.0.1:8000/v1?key=value",
        "file:///tmp/socket",
    ],
)
def test_config_rejects_non_loopback_or_dangerous_model_routes(base_url: str) -> None:
    adapter = _adapter()
    route = _json_bytes({"baseUrl": base_url, "model": "loopback-model"})

    with pytest.raises(adapter.PromptfooAdapterError, match="loopback"):
        adapter.render_promptfoo_config(_request(model_route_json=route))


def test_config_accepts_explicit_ipv6_loopback_port() -> None:
    adapter = _adapter()
    route = b'{"baseUrl":"http://[::1]:8000/v1","model":"loopback-model"}'

    config = adapter.render_promptfoo_config(_request(model_route_json=route))

    provider = config["providers"][0]
    assert provider["config"]["base_url"] == "http://[::1]:8000/v1"


def test_config_bytes_do_not_change_with_arm_local_material() -> None:
    adapter = _adapter()
    stock = _request()
    modified = _request(
        binary_path=Path("/other/modified/codex"),
        cell=SimpleNamespace(root=Path("/other/modified/cell")),
        effective_config=b"modified-config",
    )

    assert _json_bytes(adapter.render_promptfoo_config(stock)) == _json_bytes(
        adapter.render_promptfoo_config(modified)
    )


def test_config_resolves_candidate_workspace_only_from_runtime_environment() -> None:
    adapter = _adapter()

    rendered = _json_bytes(adapter.render_promptfoo_config(_request()))

    assert b'"working_dir":"{{ env.AI_IP_WORKSPACE }}"' in rendered
    assert b"/private/arm-specific" not in rendered


def test_parser_normalizes_only_provider_output_and_auditable_evidence() -> None:
    adapter = _adapter()
    payload = _result_bytes(_result_value())

    parsed = adapter.parse_promptfoo_result(payload)

    assert parsed.output == {
        "caseId": "case-1",
        "objectKind": "CaseAnswer",
        "schemaVersion": 1,
    }
    expected_trajectory = {
        "finalResponse": BASELINE_ANSWER,
        "items": [
            {"id": "item-1", "status": "completed", "type": "agentMessage"}
        ],
        "notifications": [
            {"method": "item/completed", "params": {"itemId": "item-1"}},
            {"method": "turn/completed", "params": {"turnId": "turn-1"}},
        ],
        "rawItems": [
            {"id": "item-1", "status": "completed", "type": "agentMessage"}
        ],
        "responseUsage": {
            "cached": 0,
            "completion": 11,
            "prompt": 17,
            "total": 28,
        },
        "rowUsage": {
            "cached": 0,
            "completion": 11,
            "numRequests": 1,
            "prompt": 17,
            "total": 28,
        },
    }
    assert parsed.metadata == {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": hashlib.sha256(
                _canonical_bytes(expected_trajectory)
            ).hexdigest(),
        },
        "requestCount": 1,
        "threadId": "thread-1",
        "trajectory": expected_trajectory,
        "turnId": "turn-1",
        "usage": {"inputTokens": 17, "outputTokens": 11, "totalTokens": 28},
    }
    assert parsed.telemetry == {
        "costCny": 0,
        "inputTokens": 17,
        "outputTokens": 11,
        "requestCount": 1,
    }
    assert parsed.metadata["costEvidence"]["sourceSha256"] == hashlib.sha256(
        _canonical_bytes(parsed.metadata["trajectory"])
    ).hexdigest()


def test_parser_rejects_multiple_rows() -> None:
    adapter = _adapter()
    value = _result_value()
    results = value["results"]
    assert type(results) is dict
    rows = results["results"]
    assert type(rows) is list
    rows.append(copy.deepcopy(rows[0]))

    with pytest.raises(adapter.PromptfooAdapterError, match="exactly one result row"):
        adapter.parse_promptfoo_result(_result_bytes(value))


def test_parser_enforces_its_own_byte_bound_before_decoding() -> None:
    adapter = _adapter()
    payload = _result_bytes(_result_value())

    with pytest.raises(adapter.PromptfooAdapterError, match="byte bound"):
        adapter.parse_promptfoo_result(payload, maximum_bytes=len(payload) - 1)


@pytest.mark.parametrize("location", ["row", "response"])
def test_parser_rejects_provider_errors(location: str) -> None:
    adapter = _adapter()
    value = _result_value()
    target = _row(value) if location == "row" else _response(value)
    target["error"] = "provider failed"

    with pytest.raises(adapter.PromptfooAdapterError, match="provider error"):
        adapter.parse_promptfoo_result(_result_bytes(value))


def test_parser_requires_a_string_output_containing_json() -> None:
    adapter = _adapter()
    value = _result_value()
    _response(value)["output"] = {"not": "a provider string"}

    with pytest.raises(adapter.PromptfooAdapterError, match="string output"):
        adapter.parse_promptfoo_result(_result_bytes(value))


@pytest.mark.parametrize("field", ["threadId", "turnId"])
def test_parser_requires_nonempty_app_server_ids(field: str) -> None:
    adapter = _adapter()
    value = _result_value()
    metadata = _response(value)["metadata"]
    assert type(metadata) is dict
    codex = metadata["codexAppServer"]
    assert type(codex) is dict
    codex[field] = "  "

    with pytest.raises(adapter.PromptfooAdapterError, match="thread and turn IDs"):
        adapter.parse_promptfoo_result(_result_bytes(value))


@pytest.mark.parametrize("missing", ["items", "notifications"])
def test_parser_requires_nonempty_trajectory_and_raw_events(missing: str) -> None:
    adapter = _adapter()
    value = _result_value()
    response = _response(value)
    if missing == "items":
        metadata = response["metadata"]
        assert type(metadata) is dict
        codex = metadata["codexAppServer"]
        assert type(codex) is dict
        codex["items"] = []
    else:
        raw = json.loads(response["raw"])
        raw["notifications"] = []
        response["raw"] = json.dumps(raw, sort_keys=True, separators=(",", ":"))

    with pytest.raises(
        adapter.PromptfooAdapterError, match="trajectory and raw events"
    ):
        adapter.parse_promptfoo_result(_result_bytes(value))


@pytest.mark.parametrize(
    ("field", "replacement"),
    [("prompt", True), ("completion", -1), ("total", 29)],
)
def test_parser_rejects_nonintegral_negative_or_inconsistent_usage(
    field: str, replacement: object
) -> None:
    adapter = _adapter()
    value = _result_value()
    usage = _response(value)["tokenUsage"]
    assert type(usage) is dict
    usage[field] = replacement

    with pytest.raises(adapter.PromptfooAdapterError, match="token usage"):
        adapter.parse_promptfoo_result(_result_bytes(value))


@pytest.mark.parametrize(
    ("location", "field", "replacement"),
    [
        ("row", "prompt", 18),
        ("row", "completion", True),
        ("row", "total", 29),
        ("row", "numRequests", -1),
        ("response", "numRequests", 2),
    ],
)
def test_parser_rejects_invalid_or_conflicting_usage_layers(
    location: str, field: str, replacement: object
) -> None:
    adapter = _adapter()
    value = _result_value()
    row = _row(value)
    response = _response(value)
    response_usage = response["tokenUsage"]
    row_usage = row["tokenUsage"]
    assert type(response_usage) is dict
    assert type(row_usage) is dict
    target = row_usage if location == "row" else response_usage
    target[field] = replacement

    with pytest.raises(adapter.PromptfooAdapterError, match="token usage"):
        adapter.parse_promptfoo_result(_result_bytes(value))


def test_parser_ignores_only_assertion_usage_subtree() -> None:
    adapter = _adapter()
    value = _result_value()
    row_usage = _row(value)["tokenUsage"]
    response_usage = _response(value)["tokenUsage"]
    assert type(row_usage) is dict
    assert type(response_usage) is dict
    row_usage["assertions"] = {
        "completion": False,
        "numRequests": -10,
        "prompt": "untrusted",
        "total": None,
    }

    parsed = adapter.parse_promptfoo_result(_result_bytes(value))

    assert parsed.telemetry["requestCount"] == 1


@pytest.mark.parametrize(
    ("location", "replacement"),
    [
        ("row", 1),
        ("response", 1),
        ("row", True),
        ("response", -1),
    ],
)
def test_both_parsers_reject_cached_mismatch_or_invalid_value(
    location: str, replacement: object
) -> None:
    adapter = _adapter()
    value = _result_value()
    usage = (
        _row(value)["tokenUsage"]
        if location == "row"
        else _response(value)["tokenUsage"]
    )
    assert type(usage) is dict
    usage["cached"] = replacement
    payload = _result_bytes(value)

    with pytest.raises(adapter.PromptfooAdapterError, match="token usage"):
        adapter.parse_promptfoo_result(payload)
    assert _run_js_parser(payload).returncode == 2


@pytest.mark.parametrize("location", ["row", "response"])
def test_both_parsers_reject_unexpected_nonassertion_usage_field(
    location: str,
) -> None:
    adapter = _adapter()
    value = _result_value()
    usage = (
        _row(value)["tokenUsage"]
        if location == "row"
        else _response(value)["tokenUsage"]
    )
    assert type(usage) is dict
    usage["completionDetails"] = {"reasoning": 7}
    payload = _result_bytes(value)

    with pytest.raises(adapter.PromptfooAdapterError, match="token usage"):
        adapter.parse_promptfoo_result(payload)
    assert _run_js_parser(payload).returncode == 2


@pytest.mark.parametrize(
    "output_text",
    [
        '{"value":NaN}',
        '{"value":Infinity}',
        '{"value":9007199254740993}',
        '{"value":1,"value":2}',
    ],
)
def test_python_and_js_reject_the_same_nonstandard_or_lossy_json(
    output_text: str,
) -> None:
    adapter = _adapter()
    value = _result_value()
    _set_answer_fields(value, output_text)
    payload = _result_bytes(value)

    with pytest.raises(adapter.PromptfooAdapterError, match="JSON"):
        adapter.parse_promptfoo_result(payload)
    completed = _run_js_parser(payload)
    assert completed.returncode == 2
    assert b"JSON" in completed.stderr


@pytest.mark.parametrize("field", ["response", "rawOutput", "rawFinalResponse"])
def test_answer_must_equal_retained_final_response(field: str) -> None:
    adapter = _adapter()
    payload = _result_bytes(_mutate_one_answer_field(_result_value(), field))

    with pytest.raises(adapter.PromptfooAdapterError, match="final response differs"):
        adapter.parse_promptfoo_result(payload)
    completed = _run_js_parser(payload)
    assert completed.returncode == 2
    assert b"final response differs" in completed.stderr


@pytest.mark.parametrize(
    ("answer_text", "accepted", "expected_output"), ANSWER_EVIDENCE_CORPUS
)
def test_python_and_js_share_the_answer_evidence_corpus(
    answer_text: str, accepted: bool, expected_output: object
) -> None:
    adapter = _adapter()
    value = _result_value()
    _set_answer_fields(value, answer_text)
    payload = _result_bytes(value)

    if not accepted:
        with pytest.raises(adapter.PromptfooAdapterError):
            adapter.parse_promptfoo_result(payload)
        assert _run_js_parser(payload).returncode == 2
        return

    parsed = adapter.parse_promptfoo_result(payload)
    completed = _run_js_parser(payload)

    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    js_value = json.loads(completed.stdout)
    assert parsed.output == expected_output
    assert js_value["result"]["output"] == expected_output
    assert (
        js_value["result"]["metadata"]["costEvidence"]["sourceSha256"]
        == parsed.metadata["costEvidence"]["sourceSha256"]
    )


def test_python_and_js_sort_canonical_trajectory_keys_by_utf8_bytes() -> None:
    adapter = _adapter()
    value = _result_value()
    response = _response(value)
    metadata = response["metadata"]
    assert type(metadata) is dict
    codex = metadata["codexAppServer"]
    assert type(codex) is dict
    items = codex["items"]
    assert type(items) is list and type(items[0]) is dict
    items[0]["\ue000"] = "bmp"
    items[0]["\U00010000"] = "non-bmp"
    raw = json.loads(response["raw"])
    raw["items"][0]["\ue000"] = "bmp"
    raw["items"][0]["\U00010000"] = "non-bmp"
    response["raw"] = json.dumps(raw, ensure_ascii=False, separators=(",", ":"))
    payload = _result_bytes(value)

    parsed = adapter.parse_promptfoo_result(payload)
    completed = _run_js_parser(payload)

    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    expected_digest = "d49ba0ea646d93e2efd101e7c5744d92cae72dc13cbbb9f5c605bcc1cd7c80ae"
    assert parsed.metadata["costEvidence"]["sourceSha256"] == expected_digest
    assert (
        json.loads(completed.stdout)["result"]["metadata"]["costEvidence"][
            "sourceSha256"
        ]
        == expected_digest
    )


def test_retained_trajectory_excludes_scores_and_assertions_but_keeps_raw_evidence() -> None:
    adapter = _adapter()
    value = _result_value()
    row = _row(value)
    row["score"] = 123456
    row["success"] = True
    usage = row["tokenUsage"]
    assert type(usage) is dict
    usage["assertions"] = {"prompt": 999999, "total": 999999}
    payload = _result_bytes(value)

    parsed = adapter.parse_promptfoo_result(payload)
    js = _run_js_parser(payload)

    assert js.returncode == 0, js.stderr.decode(errors="replace")
    js_value = json.loads(js.stdout)
    assert js_value["result"]["metadata"] == parsed.metadata
    trajectory = parsed.metadata["trajectory"]
    assert type(trajectory) is dict
    assert trajectory["items"]
    assert trajectory["rawItems"]
    assert trajectory["notifications"]
    assert trajectory["finalResponse"] == BASELINE_ANSWER
    assert "assertions" not in _canonical_bytes(trajectory).decode()
    assert parsed.metadata["costEvidence"]["sourceSha256"] == hashlib.sha256(
        _canonical_bytes(trajectory)
    ).hexdigest()


def test_python_and_js_retain_the_same_unicode_trajectory() -> None:
    adapter = _adapter()
    value = _result_value()
    response = _response(value)
    metadata = response["metadata"]
    assert type(metadata) is dict
    codex = metadata["codexAppServer"]
    assert type(codex) is dict
    items = codex["items"]
    assert type(items) is list and type(items[0]) is dict
    items[0]["text"] = "礼🎁"
    _set_answer_fields(value, '{"gift":"礼🎁"}')
    response = _response(value)
    raw = json.loads(response["raw"])
    raw["items"][0]["text"] = "礼🎁"
    response["raw"] = json.dumps(raw, ensure_ascii=True, separators=(",", ":"))
    payload = _result_bytes(value)

    parsed = adapter.parse_promptfoo_result(payload)
    js = _run_js_parser(payload)

    assert js.returncode == 0, js.stderr.decode(errors="replace")
    assert json.loads(js.stdout)["result"]["metadata"] == parsed.metadata


def test_python_and_js_reject_oversized_retained_evidence() -> None:
    adapter = _adapter()
    value = _result_value()
    _set_answer_fields(value, json.dumps("x" * (1024 * 1024)))
    payload = _result_bytes(value)

    with pytest.raises(adapter.PromptfooAdapterError, match="evidence"):
        adapter.parse_promptfoo_result(payload)
    assert _run_js_parser(payload).returncode == 2


def _archive_record(path: str, payload: bytes, *, declared_size: int | None = None) -> bytes:
    header = _json_bytes(
        {
            "mode": 0o600,
            "path": path,
            "sha256": hashlib.sha256(payload).hexdigest(),
            "size": len(payload) if declared_size is None else declared_size,
        }
    )
    return struct.pack(">I", len(header)) + header + payload + struct.pack(">I", 0)


def _malicious_archive_manifest(payload: bytes) -> bytes:
    compressed = gzip.compress(payload, mtime=0)
    return _json_bytes(
        {
            "archiveSha256": hashlib.sha256(compressed).hexdigest(),
            "chunkSha256": [hashlib.sha256(compressed).hexdigest()],
            "entrypoint": "node_modules/promptfoo/dist/src/entrypoint.js",
            "fileCount": 1,
            "format": "ai-ip-promptfoo-records-v1",
            "maximumFileBytes": 384 * 1024 * 1024,
            "maximumUnpackedBytes": 2 * 1024 * 1024 * 1024,
            "nodeSha256": "a" * 64,
            "nodeVersion": subprocess.check_output(
                [shutil.which("node") or "node", "--version"], text=True
            ).strip(),
            "packageJsonSha256": "b" * 64,
            "platform": {
                "arch": "x86_64" if sys.platform == "darwin" else "unsupported",
                "os": "darwin" if sys.platform == "darwin" else sys.platform,
            },
            "pnpmLockSha256": "c" * 64,
            "promptfooVersion": "0.122.0",
            "schemaVersion": 1,
            "treeSha256": hashlib.sha256(payload).hexdigest(),
            "unpackedBytes": len(payload),
        }
    )


def _runner_isolation(tmp_path: Path) -> dict[str, str]:
    temporary = tmp_path / "isolated-temp"
    promptfoo = tmp_path / "isolated-promptfoo"
    temporary.mkdir()
    promptfoo.mkdir()
    return {
        **os.environ,
        "PROMPTFOO_CONFIG_DIR": str(promptfoo),
        "PROMPTFOO_OUTPUT_PATH": str(promptfoo / "output.json"),
        "TMPDIR": str(temporary),
    }


@pytest.mark.parametrize(
    "record",
    [
        _archive_record("../escape", b"payload"),
        _archive_record("/absolute", b"payload"),
        _archive_record(
            "node_modules/promptfoo/dist/src/entrypoint.js",
            b"payload",
            declared_size=512 * 1024 * 1024,
        ),
    ],
)
def test_sealed_bootstrap_rejects_archive_traversal_or_oversize(
    tmp_path: Path, record: bytes
) -> None:
    runner = MODULE_ROOT / "batch_promptfoo_runner.js"
    compressed = gzip.compress(record, mtime=0)
    manifest = tmp_path / "manifest.json"
    chunk = tmp_path / "chunk.bin"
    manifest.write_bytes(_malicious_archive_manifest(record))
    chunk.write_bytes(compressed)

    completed = subprocess.run(
        [
            shutil.which("node") or "node",
            str(runner),
            str(MODULE_ROOT / "batch_promptfoo_result.js"),
            str(manifest),
            str(chunk),
        ],
        cwd=tmp_path,
        env=_runner_isolation(tmp_path),
        capture_output=True,
        check=False,
    )

    assert completed.returncode == 2
    assert any(token in completed.stderr.lower() for token in (b"path", b"bound"))
    assert not (tmp_path.parent / "escape").exists()


@pytest.mark.parametrize(
    ("header_length", "message"),
    [(4097, b"header"), (4096, b"inflated")],
)
def test_sealed_bootstrap_bounds_incomplete_header_before_buffering_tail(
    tmp_path: Path, header_length: int, message: bytes
) -> None:
    record = struct.pack(">I", header_length) + b"x" * (2 * 1024 * 1024)
    compressed = gzip.compress(record, mtime=0)
    manifest_value = json.loads(_malicious_archive_manifest(record))
    manifest_value["unpackedBytes"] = 1
    manifest = tmp_path / "manifest.json"
    chunk = tmp_path / "chunk.bin"
    manifest.write_bytes(_json_bytes(manifest_value))
    chunk.write_bytes(compressed)

    completed = subprocess.run(
        [
            shutil.which("node") or "node",
            str(MODULE_ROOT / "batch_promptfoo_runner.js"),
            str(MODULE_ROOT / "batch_promptfoo_result.js"),
            str(manifest),
            str(chunk),
        ],
        cwd=tmp_path,
        env=_runner_isolation(tmp_path),
        capture_output=True,
        check=False,
    )

    assert completed.returncode == 2
    assert message in completed.stderr.lower()


def test_runtime_bundle_rejects_replaced_node_or_module_bytes(tmp_path: Path) -> None:
    bundle = importlib.import_module("batch_promptfoo_bundle")
    runtime = tmp_path / "runtime"
    entrypoint = runtime / "node_modules/promptfoo/dist/src/entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_bytes(b"console.log('0.122.0')\n")
    package = entrypoint.parents[2] / "package.json"
    package.write_bytes(
        _json_bytes(
            {
                "bin": {"promptfoo": "dist/src/entrypoint.js"},
                "name": "promptfoo",
                "version": "0.122.0",
            }
        )
    )
    node = tmp_path / "node"
    node.write_bytes(b"sealed portable node")
    seal = bundle.measure_promptfoo_runtime(runtime, node)

    node.write_bytes(b"replaced portable node")
    with pytest.raises(bundle.PromptfooBundleError, match="Node identity"):
        bundle.seal_promptfoo_runtime(runtime, node, seal)

    node.write_bytes(b"sealed portable node")
    entrypoint.write_bytes(b"replaced entrypoint")
    with pytest.raises(bundle.PromptfooBundleError, match="runtime tree identity"):
        bundle.seal_promptfoo_runtime(runtime, node, seal)


@pytest.mark.parametrize(
    ("version", "accepted"),
    [
        ("v22.21.9", False),
        ("v22.22.0", True),
        ("v24.19.0", True),
        ("22.22.0", False),
        ("v22.22", False),
        ("v022.22.0", False),
        ("v22.22.0-rc.1", False),
    ],
)
def test_runtime_seal_loader_enforces_strict_node_engine_floor(
    version: str, accepted: bool
) -> None:
    bundle = importlib.import_module("batch_promptfoo_bundle")
    manifest_path = (
        REPO_ROOT
        / "ai-ip-evals/lab/promptfoo/runtime-manifests/darwin-x86_64.json"
    )
    value = json.loads(manifest_path.read_bytes())
    value["nodeVersion"] = version

    if accepted:
        assert bundle._seal_from_json(value).node_version == version
    else:
        with pytest.raises(bundle.PromptfooBundleError, match="incompatible"):
            bundle._seal_from_json(value)


@pytest.mark.parametrize(
    ("version", "accepted"),
    [("v22.21.9", False), ("v22.22.0", True), ("v24.19.0", True), ("v22.22", False)],
)
def test_sealed_runner_mirrors_node_engine_floor(version: str, accepted: bool) -> None:
    source = """
const runner = require(process.argv[1]);
process.stdout.write(JSON.stringify(runner.nodeVersionSupported(process.argv[2])));
"""
    completed = subprocess.run(
        [
            shutil.which("node") or "node",
            "-e",
            source,
            str(MODULE_ROOT / "batch_promptfoo_runner.js"),
            version,
        ],
        capture_output=True,
        check=False,
    )

    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    assert json.loads(completed.stdout) is accepted


def test_production_compile_rejects_forged_exact_version_stub(tmp_path: Path) -> None:
    adapter = _adapter()
    runtime = tmp_path / "runtime"
    entrypoint = runtime / "node_modules/promptfoo/dist/src/entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_bytes(b"console.log('forged 0.122.0')\n")
    (entrypoint.parents[2] / "package.json").write_bytes(
        _json_bytes(
            {
                "bin": {"promptfoo": "dist/src/entrypoint.js"},
                "name": "promptfoo",
                "version": "0.122.0",
            }
        )
    )
    node = tmp_path / "node"
    node.write_bytes(b"forged node")
    stock = _request(effective_config=b"stock-config")
    config_bytes = _json_bytes(adapter.render_promptfoo_config(stock))
    stock.promptfoo_config = config_bytes
    modified = _request(
        effective_config=b"modified-config", promptfoo_config=config_bytes
    )

    with pytest.raises(
        adapter.PromptfooAdapterError, match="committed Promptfoo runtime"
    ):
        adapter.compile_promptfoo_launch_set(
            stock,
            modified,
            promptfoo_runtime_root=runtime,
            portable_node_path=node,
        )


def _test_runtime(tmp_path: Path, entrypoint_source: bytes) -> tuple[object, Path, Path]:
    bundle = importlib.import_module("batch_promptfoo_bundle")
    runtime = tmp_path / "runtime"
    entrypoint = runtime / "node_modules/promptfoo/dist/src/entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_bytes(entrypoint_source)
    (entrypoint.parents[2] / "package.json").write_bytes(
        _json_bytes(
            {
                "bin": {"promptfoo": "dist/src/entrypoint.js"},
                "name": "promptfoo",
                "version": "0.122.0",
            }
        )
    )
    node = Path(shutil.which("node") or "node").resolve()
    node_version = subprocess.check_output([str(node), "--version"], text=True).strip()
    seal = bundle.measure_promptfoo_runtime(runtime, node, node_version=node_version)
    return bundle.seal_promptfoo_runtime(runtime, node, seal), runtime, node


def test_launch_compiler_returns_controller_owned_parity_material(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    adapter = _adapter()
    runtime, runtime_root, node = _test_runtime(
        tmp_path, b"console.log('sealed Promptfoo')\n"
    )
    monkeypatch.setattr(
        adapter._bundle,
        "seal_committed_promptfoo_runtime",
        lambda runtime_root, node_path: runtime,
    )
    stock = _request(effective_config=b"stock-config")
    config_bytes = _json_bytes(adapter.render_promptfoo_config(stock))
    stock.promptfoo_config = config_bytes
    modified = _request(
        binary_path=Path("/modified/codex"),
        effective_config=b"modified-config",
        promptfoo_config=config_bytes,
    )

    launches = adapter.compile_promptfoo_launch_set(
        stock,
        modified,
        promptfoo_runtime_root=runtime_root,
        portable_node_path=node,
    )

    launch_types = importlib.import_module("batch_launch_spec")
    assert type(launches) is launch_types.CandidateLaunchSet
    assert launches.stock.effective_config == b"stock-config"
    assert launches.modified.effective_config == b"modified-config"
    assert launches.stock.executable_path == node
    assert launches.stock.executable_sha256 == runtime.node_sha256
    assert launches.stock.argv == (
        "{artifact:batch_promptfoo_runner.js}",
        "{artifact:batch_promptfoo_result.js}",
        *runtime.artifact_arguments,
    )
    assert launches.stock.environment == (
        ("PROMPTFOO_DISABLE_SHARING", "1"),
        ("PROMPTFOO_DISABLE_TELEMETRY", "1"),
        ("PROMPTFOO_DISABLE_UPDATE", "1"),
    )
    assert launches.stock.artifacts == launches.modified.artifacts
    assert launches.stock.promptfoo_config == launches.modified.promptfoo_config
    assert (
        launches.stock.execution_profile_json
        == launches.modified.execution_profile_json
    )
    assert launches.stock.model_route_json == launches.modified.model_route_json
    assert launches.stock.app_server_protocol_schema == (
        launches.modified.app_server_protocol_schema
    )
    runner = launches.stock.artifacts[0]
    assert runner.relative_path == "batch_promptfoo_runner.js"
    assert runner.sha256 == hashlib.sha256(runner.payload).hexdigest()
    assert launches.stock.artifacts[2:] == runtime.artifacts
    assert all(
        token not in runner.payload.lower()
        for token in (b"rubric", b"reference", b"outcome", b"arm mapping")
    )


def test_sealed_runner_invokes_exact_cli_and_writes_controller_envelopes(
    tmp_path: Path,
) -> None:
    adapter = _adapter()
    frozen_result_literal = json.dumps(_result_bytes(_result_value()).decode())
    runtime, _, node = _test_runtime(
        tmp_path,
        f"""const fs = require('node:fs');
const argvPath = require('node:path').join(process.env.PROMPTFOO_CONFIG_DIR, 'argv.json');
fs.writeFileSync(argvPath, JSON.stringify(process.argv.slice(2)));
const envPath = require('node:path').join(process.env.PROMPTFOO_CONFIG_DIR, 'child-env.json');
fs.writeFileSync(envPath, JSON.stringify(process.env));
fs.writeFileSync(process.argv[6], {frozen_result_literal});
""".encode(),
    )
    stock = _request(effective_config=b"stock-config")
    config_bytes = _json_bytes(adapter.render_promptfoo_config(stock))
    runner_path = MODULE_ROOT / "batch_promptfoo_runner.js"
    artifact_paths: list[Path] = []
    for artifact in runtime.artifacts:
        artifact_path = tmp_path / artifact.relative_path
        artifact_path.parent.mkdir(parents=True, exist_ok=True)
        artifact_path.write_bytes(artifact.payload)
        artifact_paths.append(artifact_path)
    config_path = tmp_path / "promptfoo-config.json"
    config_path.write_bytes(config_bytes)
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    isolated = tmp_path / "isolated"
    isolated_temp = isolated / "temp"
    isolated_promptfoo = isolated / "promptfoo"
    isolated_temp.mkdir(parents=True)
    isolated_promptfoo.mkdir()
    result_read, result_write = os.pipe()
    telemetry_read, telemetry_write = os.pipe()
    environment = {
        **os.environ,
        "AI_IP_PROMPTFOO_PATH": str(config_path),
        "AI_IP_RESULT_FD": str(result_write),
        "AI_IP_TELEMETRY_FD": str(telemetry_write),
        "REAL_PROVIDER_API_KEY": "must-not-reach-promptfoo",
        "PROMPTFOO_DISABLE_SHARING": "1",
        "PROMPTFOO_DISABLE_TELEMETRY": "1",
        "PROMPTFOO_DISABLE_UPDATE": "1",
        "PROMPTFOO_CONFIG_DIR": str(isolated_promptfoo),
        "PROMPTFOO_OUTPUT_PATH": str(isolated_promptfoo / "output.json"),
        "TMPDIR": str(isolated_temp),
    }
    try:
        completed = subprocess.run(
            [
                str(node),
                str(runner_path),
                str(MODULE_ROOT / "batch_promptfoo_result.js"),
                *(str(path) for path in artifact_paths),
            ],
            cwd=workspace,
            env=environment,
            pass_fds=(result_write, telemetry_write),
            capture_output=True,
            check=False,
        )
    finally:
        os.close(result_write)
        os.close(telemetry_write)
    result = os.read(result_read, 64 * 1024)
    telemetry = os.read(telemetry_read, 64 * 1024)
    os.close(result_read)
    os.close(telemetry_read)

    assert not list(workspace.iterdir()), "sealed runner polluted candidate workspace"
    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    assert json.loads((isolated_promptfoo / "argv.json").read_bytes()) == [
        "eval",
        "--config",
        str(isolated_promptfoo / "eval-config.json"),
        "--output",
        str(isolated_promptfoo / "output.json"),
        "--no-cache",
        "--no-progress-bar",
        "--no-table",
        "--no-share",
        "--no-write",
        "--max-concurrency",
        "1",
    ]
    child_environment = json.loads(
        (isolated_promptfoo / "child-env.json").read_bytes()
    )
    assert child_environment["AI_IP_WORKSPACE"] == str(workspace)
    assert child_environment["PROMPTFOO_DISABLE_SHARING"] == "1"
    assert child_environment["PROMPTFOO_DISABLE_TELEMETRY"] == "1"
    assert child_environment["PROMPTFOO_DISABLE_UPDATE"] == "1"
    assert "REAL_PROVIDER_API_KEY" not in child_environment
    assert json.loads(result) == {
        "metadata": adapter.parse_promptfoo_result(
            _result_bytes(_result_value())
        ).metadata,
        "output": {
            "caseId": "case-1",
            "objectKind": "CaseAnswer",
            "schemaVersion": 1,
        },
    }
    assert json.loads(telemetry) == {
        "costCny": 0,
        "inputTokens": 17,
        "outputTokens": 11,
        "requestCount": 1,
    }
