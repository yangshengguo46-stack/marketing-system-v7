import copy
import json
import shutil
import sys
import time
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_isolation  # noqa: E402
from batch_contracts import seal_self_commitment  # noqa: E402
from batch_controller import (  # noqa: E402
    AttemptAttestation,
    AttemptTelemetry,
    BatchControllerError,
    MemoryAttemptByteSource,
    RunningAttempt,
)
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_file, sha256_tree  # noqa: E402
from batch_receipts import BatchReceiptError, verify_paired_run_receipt  # noqa: E402
from contracts import canonical_json_bytes, load_exact_json, sha256_json  # noqa: E402
from test_batch_controller import (  # noqa: E402
    ScriptedExecutor,
    World,
    _metadata,
    _result,
    world,
)
from batch_controller_test_support import run_candidate_pair  # noqa: E402


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _reseal(value: dict[str, object]) -> dict[str, object]:
    value["receiptSha256"] = "0" * 64
    return seal_self_commitment(value, "receiptSha256")


def _localize_mutable_inputs(world: World, tmp_path: Path) -> tuple[dict, dict]:
    bindings = copy.deepcopy(world.bindings)
    stock_seed = tmp_path / "stock-seed"
    modified_seed = tmp_path / "modified-seed"
    workspace = tmp_path / "workspace"
    shutil.copytree(bindings["stock"]["codexHomeSeed"], stock_seed)
    shutil.copytree(bindings["modified"]["codexHomeSeed"], modified_seed)
    shutil.copytree(bindings["workspaceSeed"], workspace)
    bindings["stock"]["codexHomeSeed"] = stock_seed
    bindings["modified"]["codexHomeSeed"] = modified_seed
    bindings["workspaceSeed"] = workspace
    for name in ("stock", "modified"):
        treatment = bindings[name]["treatmentManifest"]
        treatment["codexHomeSeedSha256"] = sha256_tree(bindings[name]["codexHomeSeed"])
        treatment["treatmentManifestSha256"] = "0" * 64
        treatment["treatmentManifestSha256"] = sha256_json(
            {
                key: value
                for key, value in treatment.items()
                if key != "treatmentManifestSha256"
            }
        )
    plan = dict(world.plan)
    plan["stockTreatmentRef"] = bindings["stock"]["treatmentManifest"]["treatmentId"]
    plan["modifiedTreatmentRef"] = bindings["modified"]["treatmentManifest"][
        "treatmentId"
    ]
    plan["workspaceTemplateSha256"] = sha256_tree(workspace)
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


def test_preflight_snapshot_survives_nested_and_artifact_mutation(
    world: World, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan, bindings = _localize_mutable_inputs(world, tmp_path)
    original_create = batch_controller.create_attempt_cell
    original_profile = copy.deepcopy(bindings["executionProfile"])
    original_route = copy.deepcopy(bindings["modelRoute"])
    expected_seed = sha256_tree(bindings["stock"]["codexHomeSeed"])
    expected_workspace = sha256_tree(bindings["workspaceSeed"])
    expected_protocol = sha256_file(bindings["appServerProtocolSchemaPath"])
    expected_promptfoo = sha256_file(bindings["promptfooConfigPath"])
    calls = 0

    def mutate_then_create(*args, **kwargs):
        nonlocal calls
        calls += 1
        if calls == 1:
            bindings["executionProfile"]["sandboxMode"] = "danger-full-access"
            bindings["modelRoute"]["model"] = "substituted-model"
            Path(bindings["stock"]["codexHomeSeed"], "config.toml").write_text(
                "model = 'substituted'\n", encoding="utf-8"
            )
            Path(bindings["workspaceSeed"], "case.txt").write_text(
                "substituted workspace\n", encoding="utf-8"
            )
            Path(bindings["appServerProtocolSchemaPath"]).write_text(
                '{"version":999}\n', encoding="utf-8"
            )
            Path(bindings["promptfooConfigPath"]).write_text(
                '{"reuse_server":true}\n', encoding="utf-8"
            )
        return original_create(*args, **kwargs)

    monkeypatch.setattr(batch_controller, "create_attempt_cell", mutate_then_create)
    executor = ScriptedExecutor([_result("left"), _result("right")])

    run_candidate_pair(plan, bindings, executor, world.private_root, seed=b"1" * 32)

    assert executor.calls == 2
    for request in executor.requests:
        assert json.loads(request.execution_profile_json) == original_profile
        assert json.loads(request.model_route_json) == original_route
        assert request.workspace_seed_sha256 == expected_workspace
        assert request.app_server_protocol_schema_sha256 == expected_protocol
        assert request.promptfoo_config_sha256 == expected_promptfoo
    assert {request.codex_home_seed_sha256 for request in executor.requests} >= {
        expected_seed
    }


def test_replacing_real_authority_directory_does_not_reset_plan(
    world: World,
) -> None:
    run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("first"), _result("second")]),
        world.private_root,
        seed=b"2" * 32,
    )
    authority = world.private_root / "plan-authority"
    authority.rename(world.private_root / "plan-authority-held")
    authority.mkdir(mode=0o700)
    executor = ScriptedExecutor([_result("replay-a"), _result("replay-b")])

    with pytest.raises(BatchControllerError, match="authority|replaced|reserved"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"3" * 32
        )

    assert executor.calls == 0


def test_resealed_pair_cannot_reverse_stock_and_modified_membership(
    world: World,
) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"4" * 32,
    )
    pair["stockArmAttemptReceiptSha256"], pair["modifiedArmAttemptReceiptSha256"] = (
        pair["modifiedArmAttemptReceiptSha256"],
        pair["stockArmAttemptReceiptSha256"],
    )
    pair = _reseal(pair)
    _write_json(
        world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json", pair
    )

    with pytest.raises(BatchReceiptError, match="identity|membership|stock|modified"):
        verify_paired_run_receipt(pair, world.private_root)


@pytest.mark.parametrize(
    "field",
    [
        "treatmentManifestSha256",
        "binaryManifestSha256",
        "effectiveConfigSha256",
        "inputSha256",
        "workspaceBeforeSha256",
        "appServerProtocolSchemaSha256",
        "promptfooConfigSha256",
    ],
)
def test_resealed_arm_identity_substitution_is_rejected(
    world: World, field: str
) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"9" * 32,
    )
    arm_path = next((world.private_root / "attempts").glob("*/receipt.json"))
    arm = load_exact_json(arm_path)
    old = arm["receiptSha256"]
    arm[field] = "f" * 64 if arm[field] != "f" * 64 else "e" * 64
    arm = _reseal(arm)
    _write_json(arm_path, arm)
    commitment_field = (
        "stockArmAttemptReceiptSha256"
        if pair["stockArmAttemptReceiptSha256"] == old
        else "modifiedArmAttemptReceiptSha256"
    )
    pair[commitment_field] = arm["receiptSha256"]
    pair = _reseal(pair)
    _write_json(
        world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json", pair
    )

    with pytest.raises(BatchReceiptError, match="identity|commitment|context"):
        verify_paired_run_receipt(pair, world.private_root)


def test_real_attempt_parent_replacement_cannot_redirect_evidence(
    world: World,
) -> None:
    class ReplacingExecutor(ScriptedExecutor):
        def execute(self, request):
            result = super().execute(request)
            if self.calls == 1:
                attempts = world.private_root / "attempts"
                held = world.private_root / "attempts-held"
                attempts.rename(held)
                attempts.mkdir(mode=0o700)
                for directory in held.iterdir():
                    (attempts / directory.name).mkdir(mode=0o700)
            return result

    executor = ReplacingExecutor([_result("left"), _result("right")])

    with pytest.raises(
        BatchControllerError, match="evidence|identity|replaced|private|launch|lifecycle"
    ):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"5" * 32
        )

    assert executor.calls == 2
    assert not list((world.private_root / "attempts").glob("*/*"))


def test_second_cell_failure_cleans_first_cell_and_consumes_plan(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    original_create = batch_controller.create_attempt_cell
    created = []

    def fail_second(*args, **kwargs):
        if created:
            raise RuntimeError("second-cell failure")
        cell = original_create(*args, **kwargs)
        created.append(cell)
        return cell

    monkeypatch.setattr(batch_controller, "create_attempt_cell", fail_second)
    executor = ScriptedExecutor([_result("unused"), _result("unused")])

    with pytest.raises(BatchControllerError, match="cell|pair|isolation"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"6" * 32
        )

    assert executor.calls == 0
    assert created[0]._capability not in batch_isolation._CELLS
    assert not created[0].root.exists()
    monkeypatch.setattr(batch_controller, "create_attempt_cell", original_create)
    replay = ScriptedExecutor([_result("a"), _result("b")])
    with pytest.raises(BatchControllerError, match="reserved|complete|authority"):
        run_candidate_pair(
            world.plan, world.bindings, replay, world.private_root, seed=b"7" * 32
        )
    assert replay.calls == 0


def test_deep_structured_output_is_bounded_before_canonicalization(
    world: World,
) -> None:
    first = _result("deep")
    first = type(first)(
        first.exit_code,
        first.started_at,
        first.finished_at,
        MemoryAttemptByteSource(b"[" * 2_000 + b"0" + b"]" * 2_000),
        first.metadata,
        first.stdout,
        first.stderr,
    )

    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([first, _result("other")]),
        world.private_root,
        seed=b"8" * 32,
    )

    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] in {"budgetFailure", "evidenceFailure"}


def test_callback_attestation_is_not_used_as_execution_identity(world: World) -> None:
    wrong = AttemptAttestation(*(["f" * 64] * 8))
    first = _result("substituted")
    first = type(first)(
        first.exit_code,
        first.started_at,
        first.finished_at,
        first.output,
        first.metadata,
        first.stdout,
        first.stderr,
        wrong,
    )

    executor = ScriptedExecutor([first, _result("other")])
    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        executor,
        world.private_root,
        seed=b"a" * 32,
    )

    evidence = [
        load_exact_json(path)
        for path in (world.private_root / "attempts").glob("*/evidence.json")
    ]
    assert receipt["pairValidity"] == "valid"
    assert all(
        item["rawEvidence"]["attestation"]["binarySha256"] != "f" * 64
        for item in evidence
    )
    assert executor.calls == 2


def test_candidate_metadata_cannot_forge_supervisor_telemetry(world: World) -> None:
    metadata = _metadata("bounded")
    metadata["requestCount"] = 999
    metadata["usage"] = {
        "inputTokens": 999_999,
        "outputTokens": 999_999,
        "totalTokens": 1_999_998,
    }
    result = _result("bounded", metadata=metadata)

    executor = ScriptedExecutor([result, _result("other")])
    executor.test_telemetry = [
        {"requestCount": 1, "inputTokens": 1, "outputTokens": 2, "costCny": 0},
        {"requestCount": 1, "inputTokens": 1, "outputTokens": 2, "costCny": 0},
    ]

    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        executor,
        world.private_root,
        seed=b"b" * 32,
    )

    assert receipt["pairValidity"] == "valid"


def test_supervisor_deadline_terminates_sleeping_execution(
    world: World,
) -> None:
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

    executor = ScriptedExecutor([_result("late"), _result("other")])
    executor.test_sleep = [10, 0]
    started = time.monotonic()
    receipt = run_candidate_pair(
        plan, bindings, executor, world.private_root, seed=b"0" * 32
    )

    assert time.monotonic() - started < 2.5
    assert time.monotonic() - started >= 1
    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] == "budgetFailure"
