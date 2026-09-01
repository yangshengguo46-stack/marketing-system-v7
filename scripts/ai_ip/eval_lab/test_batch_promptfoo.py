import copy
import hashlib
import importlib
import json
import sys
from pathlib import Path
from types import SimpleNamespace

import pytest


MODULE_ROOT = Path(__file__).resolve().parent
REPO_ROOT = Path(__file__).resolve().parents[3]
FROZEN_RESULT = (
    REPO_ROOT / "ai-ip-evals/lab/fixtures/batch-runner/promptfoo-result.json"
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


def _result_value() -> dict[str, object]:
    value = json.loads(FROZEN_RESULT.read_bytes())
    assert type(value) is dict
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


def test_config_is_path_neutral_and_forces_one_isolated_app_server() -> None:
    adapter = _adapter()
    request = _request()

    config = adapter.render_promptfoo_config(request)

    assert set(config) == {"prompts", "providers", "tests"}
    assert len(config["prompts"]) == 1
    assert config["tests"] == [{}]
    provider = config["providers"][0]
    assert provider == {
        "id": "openai:codex-app-server",
        "config": {
            "approval_policy": "never",
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
            "working_dir": ".",
        },
    }
    rendered = _json_bytes(config)
    assert b"/private/arm-specific" not in rendered
    assert all(
        token not in rendered
        for token in (b'"assert"', b"rubric", b"reference", b"javascript")
    )


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


def test_parser_normalizes_only_provider_output_and_auditable_evidence() -> None:
    adapter = _adapter()
    payload = FROZEN_RESULT.read_bytes()

    parsed = adapter.parse_promptfoo_result(payload)

    assert parsed.output == {
        "caseId": "case-1",
        "objectKind": "CaseAnswer",
        "schemaVersion": 1,
    }
    assert parsed.metadata == {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": hashlib.sha256(payload).hexdigest(),
        },
        "requestCount": 1,
        "threadId": "thread-1",
        "trajectory": [
            {"method": "item/completed", "params": {"itemId": "item-1"}},
            {"method": "turn/completed", "params": {"turnId": "turn-1"}},
        ],
        "turnId": "turn-1",
        "usage": {"inputTokens": 17, "outputTokens": 11, "totalTokens": 28},
    }
    assert parsed.telemetry == {
        "costCny": 0,
        "inputTokens": 17,
        "outputTokens": 11,
        "requestCount": 1,
    }


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
    payload = FROZEN_RESULT.read_bytes()

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


def test_launch_compiler_returns_controller_owned_parity_material(
    tmp_path: Path,
) -> None:
    adapter = _adapter()
    package_root = tmp_path / "node_modules" / "promptfoo"
    entrypoint = package_root / "dist" / "src" / "entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_text("#!/usr/bin/env node\n", encoding="utf-8")
    (package_root / "package.json").write_text(
        json.dumps(
            {
                "bin": {"promptfoo": "dist/src/entrypoint.js"},
                "name": "promptfoo",
                "version": "0.122.0",
            }
        ),
        encoding="utf-8",
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
        promptfoo_cli_path=entrypoint,
        runner_python_path=Path(sys.executable).resolve(),
    )

    launch_types = importlib.import_module("batch_launch_spec")
    assert type(launches) is launch_types.CandidateLaunchSet
    assert launches.stock.effective_config == b"stock-config"
    assert launches.modified.effective_config == b"modified-config"
    assert launches.stock.argv == ("{artifact:promptfoo_runner.py}",)
    assert launches.stock.environment == (("AI_IP_PROMPTFOO_CLI", str(entrypoint)),)
    assert launches.stock.artifacts == launches.modified.artifacts
    assert launches.stock.promptfoo_config == launches.modified.promptfoo_config
    assert launches.stock.execution_profile_json == launches.modified.execution_profile_json
    assert launches.stock.model_route_json == launches.modified.model_route_json
    assert launches.stock.app_server_protocol_schema == (
        launches.modified.app_server_protocol_schema
    )
    runner = launches.stock.artifacts[0]
    assert runner.relative_path == "promptfoo_runner.py"
    assert runner.sha256 == hashlib.sha256(runner.payload).hexdigest()
    assert all(
        token not in runner.payload.lower()
        for token in (b"rubric", b"reference", b"outcome", b"arm mapping")
    )
