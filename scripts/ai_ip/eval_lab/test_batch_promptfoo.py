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
            "working_dir": ".",
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
    response_usage["numRequests"] = 1
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
    response_usage["numRequests"] = 1
    row_usage["assertions"] = {
        "completion": False,
        "numRequests": -10,
        "prompt": "untrusted",
        "total": None,
    }

    parsed = adapter.parse_promptfoo_result(_result_bytes(value))

    assert parsed.telemetry["requestCount"] == 1


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
            "promptfooVersion": "0.122.0",
            "schemaVersion": 1,
            "treeSha256": hashlib.sha256(payload).hexdigest(),
            "unpackedBytes": len(payload),
        }
    )


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
        [shutil.which("node") or "node", str(runner), str(manifest), str(chunk)],
        cwd=tmp_path,
        capture_output=True,
        check=False,
    )

    assert completed.returncode == 2
    assert not (tmp_path.parent / "escape").exists()


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
    assert (
        launches.stock.execution_profile_json
        == launches.modified.execution_profile_json
    )
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


def test_sealed_runner_invokes_exact_cli_and_writes_controller_envelopes(
    tmp_path: Path,
) -> None:
    adapter = _adapter()
    package_root = tmp_path / "node_modules" / "promptfoo"
    entrypoint = package_root / "dist" / "src" / "entrypoint.js"
    entrypoint.parent.mkdir(parents=True)
    entrypoint.write_text(
        """#!/usr/bin/env python3
import json
import os
import shutil
import sys

open("argv.json", "w", encoding="utf-8").write(json.dumps(sys.argv[1:]))
shutil.copyfile(os.environ["FAKE_PROMPTFOO_RESULT"], sys.argv[5])
""",
        encoding="utf-8",
    )
    entrypoint.chmod(0o700)
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
        effective_config=b"modified-config", promptfoo_config=config_bytes
    )
    launches = adapter.compile_promptfoo_launch_set(
        stock,
        modified,
        promptfoo_cli_path=entrypoint,
        runner_python_path=Path(sys.executable).resolve(),
    )
    runner_path = tmp_path / "sealed-runner.py"
    runner_path.write_bytes(launches.stock.artifacts[0].payload)
    config_path = tmp_path / "promptfoo-config.json"
    config_path.write_bytes(config_bytes)
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    result_read, result_write = os.pipe()
    telemetry_read, telemetry_write = os.pipe()
    environment = {
        **os.environ,
        "AI_IP_PROMPTFOO_CLI": str(entrypoint),
        "AI_IP_PROMPTFOO_PATH": str(config_path),
        "AI_IP_RESULT_FD": str(result_write),
        "AI_IP_TELEMETRY_FD": str(telemetry_write),
        "FAKE_PROMPTFOO_RESULT": str(FROZEN_RESULT),
    }
    try:
        completed = subprocess.run(
            [sys.executable, str(runner_path)],
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

    assert completed.returncode == 0, completed.stderr.decode(errors="replace")
    assert json.loads((workspace / "argv.json").read_bytes()) == [
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
    ]
    assert json.loads(result) == {
        "metadata": adapter.parse_promptfoo_result(FROZEN_RESULT.read_bytes()).metadata,
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
