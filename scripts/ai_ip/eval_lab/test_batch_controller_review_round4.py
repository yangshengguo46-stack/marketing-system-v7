import copy
import dataclasses
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_isolation  # noqa: E402
import batch_receipt_verification  # noqa: E402
from batch_controller import BatchControllerError  # noqa: E402
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_file  # noqa: E402
from batch_receipts import BatchReceiptError, verify_paired_run_receipt  # noqa: E402
from contracts import canonical_json_bytes, sha256_json  # noqa: E402
from test_batch_controller import World, world  # noqa: E402


_CHILD = b"""\
import json
import os
import sys
import time

def answer(label):
    return {
        "schemaVersion": 1,
        "objectKind": "CaseAnswer",
        "caseId": "case-1",
        "taskLevels": ["L1", "L2"],
        "subject": "sealed subject",
        "audiences": ["audience"],
        "desiredActions": ["act"],
        "evidenceGaps": [],
        "directionOptions": [{
            "directionId": label,
            "directionFamily": "family",
            "rationale": "because",
            "tradeoffs": ["tradeoff"],
            "conditions": ["condition"],
            "evidenceRefs": [],
        }],
        "recommendedDirectionId": label,
        "claims": [],
        "readiness": "readyForHumanReview",
    }

mode = os.environ.get("AI_IP_FIXTURE_MODE", "normal")
binary = open(os.environ["AI_IP_CODEX_PATH"], "rb").read()
label = "left" if b"stock" in binary else "right"
if mode == "block":
    time.sleep(10)
if mode == "excessive":
    block = b"x" * 65536
    for _ in range(64):
        os.write(1, block)
output = answer(label)
if mode == "deep":
    result = b"[" * 2000 + b"0" + b"]" * 2000
else:
    metadata = {
        "threadId": "thread-" + label,
        "turnId": "turn-" + label,
        "trajectory": [{"type": "turn.completed"}],
        "usage": {"inputTokens": 1, "outputTokens": 2, "totalTokens": 3},
        "costEvidence": {"costCny": 0, "sourceSha256": "e" * 64},
        "requestCount": 1,
    }
    result = json.dumps(
        {"output": output, "metadata": metadata},
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
telemetry = json.dumps(
    {"requestCount": 1, "inputTokens": 1, "outputTokens": 2, "costCny": 0},
    sort_keys=True,
    separators=(",", ":"),
).encode()
os.write(int(os.environ["AI_IP_RESULT_FD"]), result)
os.write(int(os.environ["AI_IP_TELEMETRY_FD"]), telemetry)
os.write(2, ("stderr-" + label).encode())
"""


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _one_second(world: World) -> tuple[dict[str, object], dict[str, object]]:
    bindings = copy.deepcopy(world.bindings)
    profile = dict(bindings["executionProfile"])
    profile["maxWallClockSeconds"] = 1
    bindings["executionProfile"] = profile
    _write_json(bindings["executionProfilePath"], profile)
    plan = dict(world.plan)
    plan["timeoutBudget"] = 1
    plan["executionProfileRef"] = sha256_json(profile)
    plan["planSha256"] = "0" * 64
    plan = seal_candidate_run_plan(
        plan,
        stock_treatment=bindings["stock"]["treatmentManifest"],
        modified_treatment=bindings["modified"]["treatmentManifest"],
    )
    parity = {field: plan[field] for field in PARITY_FIELDS}
    bindings["stock"]["effectiveConditions"] = parity
    bindings["modified"]["effectiveConditions"] = parity
    return plan, bindings


def _launches(world: World, *, mode: str = "normal"):
    artifact_type = getattr(batch_controller, "LaunchArtifact")
    spec_type = getattr(batch_controller, "LaunchSpec")
    launch_set_type = getattr(batch_controller, "CandidateLaunchSet")
    script = artifact_type("runner.py", _CHILD, hashlib.sha256(_CHILD).hexdigest())

    def spec(arm: str):
        binding = world.bindings[arm]
        executable = Path(sys.executable).resolve()
        return spec_type(
            executable_path=executable,
            executable_sha256=sha256_file(executable),
            argv=("{artifact:runner.py}",),
            environment=(("AI_IP_FIXTURE_MODE", mode),),
            artifacts=(script,),
            effective_config=Path(binding["effectiveConfig"]).read_bytes(),
            execution_profile_json=canonical_json_bytes(
                world.bindings["executionProfile"]
            ),
            model_route_json=canonical_json_bytes(world.bindings["modelRoute"]),
            app_server_protocol_schema=Path(
                world.bindings["appServerProtocolSchemaPath"]
            ).read_bytes(),
            promptfoo_config=Path(world.bindings["promptfooConfigPath"]).read_bytes(),
        )

    return launch_set_type(stock=spec("stock"), modified=spec("modified"))


def test_production_entrypoint_requires_exact_immutable_launch_specs(
    world: World,
) -> None:
    assert hasattr(batch_controller, "LaunchSpec")
    assert hasattr(batch_controller, "CandidateLaunchSet")
    legacy = object()

    with pytest.raises(BatchControllerError, match="launch specification"):
        batch_controller.run_candidate_pair(
            world.plan, world.bindings, legacy, world.private_root, seed=b"a" * 32
        )

    assert not world.private_root.exists()


def test_controller_launch_record_comes_from_opened_executable_and_exact_bytes(
    world: World,
) -> None:
    receipt = batch_controller.run_candidate_pair(
        world.plan,
        world.bindings,
        _launches(world),
        world.private_root,
        seed=b"b" * 32,
    )

    assert receipt["pairValidity"] == "valid"
    assert receipt["outputRelation"] == "distinct"
    records = [
        json.loads(path.read_bytes())
        for path in world.private_root.glob("attempts/*/launch-record.json")
    ]
    assert len(records) == 2
    assert all(
        record["executableSha256"] == sha256_file(Path(sys.executable).resolve())
        for record in records
    )
    assert all(record["copiedExecutableSha256"] == record["executableSha256"] for record in records)
    assert all(record["argv"] and record["environmentSha256"] for record in records)


@pytest.mark.parametrize(
    "field",
    [
        "effective_config",
        "execution_profile_json",
        "model_route_json",
        "app_server_protocol_schema",
        "promptfoo_config",
    ],
)
def test_divergent_launch_artifact_cannot_yield_a_valid_pair(
    world: World, field: str
) -> None:
    launches = _launches(world)
    spec = launches.stock
    replacement = dataclasses.replace(spec, **{field: b"substituted"})
    launch_set = type(launches)(stock=replacement, modified=launches.modified)

    with pytest.raises(BatchControllerError, match="launch|artifact|identity"):
        batch_controller.run_candidate_pair(
            world.plan,
            world.bindings,
            launch_set,
            world.private_root,
            seed=b"c" * 32,
        )

    assert not world.private_root.exists()


@pytest.mark.parametrize("mode", ["block", "excessive", "deep"])
def test_real_child_is_deadline_and_read_bounded(world: World, mode: str) -> None:
    plan, bindings = _one_second(world)
    child_world = World(plan, bindings, world.private_root)
    started = time.monotonic()

    receipt = batch_controller.run_candidate_pair(
        plan,
        bindings,
        _launches(child_world, mode=mode),
        world.private_root,
        seed=b"d" * 32,
    )

    assert time.monotonic() - started < 3
    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] in {"budgetFailure", "evidenceFailure"}
    assert not list(world.private_root.glob("orphan-*.json"))
    assert all(
        path.stat().st_size <= 1_048_576
        for path in world.private_root.glob("attempts/*/*.bin")
    )


def test_second_launch_failure_stops_and_waits_first_owned_process(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan, bindings = _one_second(world)
    child_world = World(plan, bindings, world.private_root)
    process_module = getattr(batch_controller, "_process")
    cells_before = set(batch_isolation._CELLS)
    real_popen = process_module._Popen
    first = None
    calls = 0

    def fail_second(*args, **kwargs):
        nonlocal calls, first
        calls += 1
        if calls == 2:
            raise OSError("injected second launch failure")
        first = real_popen(*args, **kwargs)
        return first

    monkeypatch.setattr(process_module, "_Popen", fail_second)

    with pytest.raises(BatchControllerError, match="launch|process|lifecycle"):
        batch_controller.run_candidate_pair(
            plan,
            bindings,
            _launches(child_world, mode="block"),
            world.private_root,
            seed=b"e" * 32,
        )

    assert calls == 2
    assert first is not None and first.poll() is not None
    assert not list(Path(bindings["attemptBase"]).glob("*"))
    assert set(batch_isolation._CELLS) == cells_before
    assert not list(world.private_root.glob("orphan-*.json"))
    assert list(world.private_root.glob("pairs/*/failure.json"))


def test_offline_classification_never_reopens_schema_path(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    pair = batch_controller.run_candidate_pair(
        world.plan,
        world.bindings,
        _launches(world),
        world.private_root,
        seed=b"f" * 32,
    )
    schema_path = world.private_root / "pairs" / str(pair["pairId"]) / "case-answer-schema.json"
    original = batch_receipt_verification.classify_attempt_result
    calls = 0

    def replace_after_acquisition(*args, **kwargs):
        nonlocal calls
        calls += 1
        assert not isinstance(args[4], Path)
        schema_path.write_text('{"type":"null"}\n', encoding="utf-8")
        return original(*args, **kwargs)

    monkeypatch.setattr(
        batch_receipt_verification,
        "classify_attempt_result",
        replace_after_acquisition,
    )

    verify_paired_run_receipt(pair, world.private_root)

    assert calls == 2
