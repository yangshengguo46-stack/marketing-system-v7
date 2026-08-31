import copy
import importlib
import json
import os
import sys
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_controller  # noqa: E402
import batch_plan_authority  # noqa: E402
from batch_contracts import seal_self_commitment  # noqa: E402
from batch_controller import (  # noqa: E402
    BatchControllerError,
    MemoryAttemptByteSource,
    RawAttemptResult,
    run_candidate_batch,
    run_candidate_pair,
)
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_file  # noqa: E402
from batch_receipts import BatchReceiptError, verify_paired_run_receipt  # noqa: E402
from contracts import canonical_json_bytes, load_exact_json, sha256_json  # noqa: E402
from test_batch_controller import (  # noqa: E402
    ScriptedExecutor,
    World,
    _metadata,
    _result,
    world,
)


def _bindings(world: World) -> dict[str, object]:
    return copy.deepcopy(world.bindings)


def _source(value: object) -> MemoryAttemptByteSource:
    payload = value if type(value) is bytes else canonical_json_bytes(value)
    return MemoryAttemptByteSource(payload)


def _plan_with_count(
    world: World, count: int
) -> tuple[dict[str, object], dict[str, object]]:
    plan = dict(world.plan)
    plan["replicationCount"] = count
    plan["planSha256"] = "0" * 64
    plan = seal_candidate_run_plan(
        plan,
        stock_treatment=world.bindings["stock"]["treatmentManifest"],
        modified_treatment=world.bindings["modified"]["treatmentManifest"],
    )
    bindings = _bindings(world)
    parity = {field: plan[field] for field in PARITY_FIELDS}
    bindings["stock"]["effectiveConditions"] = parity
    bindings["modified"]["effectiveConditions"] = parity
    return plan, bindings


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _reseal(
    value: dict[str, object], field: str = "receiptSha256"
) -> dict[str, object]:
    value[field] = "0" * 64
    return seal_self_commitment(value, field)


@pytest.mark.parametrize(
    "substitution",
    ["binary", "seed", "effective-config", "model-route", "protocol", "promptfoo"],
)
def test_global_preflight_verifies_actual_sealed_execution_identity(
    world: World, tmp_path: Path, substitution: str
) -> None:
    bindings = _bindings(world)
    if substitution == "binary":
        bindings["stock"]["binaryPath"] = tmp_path / "missing-codex"
    elif substitution == "seed":
        seed = tmp_path / "substitute-seed"
        seed.mkdir()
        bindings["stock"]["codexHomeSeed"] = seed
    elif substitution == "effective-config":
        config = tmp_path / "substitute.toml"
        config.write_text("model = 'substitute'\n", encoding="utf-8")
        bindings["stock"]["effectiveConfig"] = config
        bindings["stock"]["effectiveConfigSha256"] = sha256_file(config)
    elif substitution == "model-route":
        bindings["modelRoute"] = {
            "model": "substitute",
            "baseUrl": "http://127.0.0.1:1",
        }
    elif substitution == "protocol":
        protocol = tmp_path / "substitute-protocol.json"
        protocol.write_text('{"version":999}\n', encoding="utf-8")
        bindings["appServerProtocolSchemaPath"] = protocol
    else:
        promptfoo = tmp_path / "substitute-promptfoo.json"
        promptfoo.write_text('{"reuse_server":true}\n', encoding="utf-8")
        bindings["promptfooConfigPath"] = promptfoo
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError):
        run_candidate_pair(
            world.plan, bindings, executor, world.private_root, seed=b"i" * 32
        )

    assert executor.calls == 0
    assert not world.private_root.exists()


def test_candidate_observes_same_neutral_binary_path_for_both_arms(
    world: World,
) -> None:
    executor = ScriptedExecutor([_result("left"), _result("right")])

    run_candidate_pair(
        world.plan, world.bindings, executor, world.private_root, seed=b"n" * 32
    )

    observed = []
    for request in executor.requests:
        relative = request.binary_path.relative_to(request.cell.root).as_posix()
        observed.append(relative)
        assert not any(
            label in str(request.binary_path).casefold()
            for label in ("stock", "modified", "candidate-a", "candidate-b")
        )
    assert observed == ["home/.runtime/codex", "home/.runtime/codex"]


def test_sealed_replication_authority_rejects_append_and_repeated_batch(
    world: World,
) -> None:
    plan, bindings = _plan_with_count(world, 3)
    first = ScriptedExecutor([_result(f"first-{index}") for index in range(6)])
    assert (
        len(
            run_candidate_batch(
                plan, bindings, first, world.private_root, seed=b"a" * 32
            )
        )
        == 3
    )

    append = ScriptedExecutor([_result("append-a"), _result("append-b")])
    with pytest.raises(BatchControllerError, match="replication|reserved|complete"):
        run_candidate_pair(plan, bindings, append, world.private_root, seed=b"b" * 32)
    assert append.calls == 0

    repeated = ScriptedExecutor([_result(f"repeat-{index}") for index in range(6)])
    with pytest.raises(BatchControllerError, match="replication|reserved|complete"):
        run_candidate_batch(
            plan, bindings, repeated, world.private_root, seed=b"c" * 32
        )
    assert repeated.calls == 0


def test_reopened_replication_authority_rejects_repeated_batch_without_calls(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan, bindings = _plan_with_count(world, 3)
    run_candidate_batch(
        plan,
        bindings,
        ScriptedExecutor([_result(f"initial-{index}") for index in range(6)]),
        world.private_root,
        seed=b"d" * 32,
    )
    reopened = importlib.reload(batch_plan_authority)
    monkeypatch.setattr(batch_controller, "reserve_plan", reopened.reserve_plan)
    monkeypatch.setattr(
        batch_controller, "PlanAuthorityError", reopened.PlanAuthorityError
    )
    executor = ScriptedExecutor([_result(f"repeat-{index}") for index in range(6)])

    with pytest.raises(BatchControllerError, match="reserved|complete"):
        run_candidate_batch(
            plan, bindings, executor, world.private_root, seed=b"e" * 32
        )
    assert executor.calls == 0


def test_standalone_pair_rejects_multi_replication_plan_before_execution(
    world: World,
) -> None:
    plan, bindings = _plan_with_count(world, 2)
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="replication"):
        run_candidate_pair(plan, bindings, executor, world.private_root, seed=b"s" * 32)
    assert executor.calls == 0


def test_malformed_first_result_still_calls_and_retains_both_arms(world: World) -> None:
    malformed = RawAttemptResult(
        exit_code=0,
        started_at="not-a-timestamp",
        finished_at="also-not-a-timestamp",
        output=_result().output,
        metadata=_source(_metadata("malformed")),
        stdout=_source(b"first"),
        stderr=_source(b""),
    )
    executor = ScriptedExecutor([malformed, _result("second")])

    receipt = run_candidate_pair(
        world.plan, world.bindings, executor, world.private_root, seed=b"m" * 32
    )

    assert executor.calls == 2
    assert receipt["pairValidity"] == "invalid"
    assert len(list((world.private_root / "attempts").glob("*/receipt.json"))) == 2


@pytest.mark.parametrize(
    ("result", "classification"),
    [
        (
            _result(metadata={**_metadata("requests"), "requestCount": 3}),
            "budgetFailure",
        ),
        (
            RawAttemptResult(
                0,
                "2026-08-31T12:00:00Z",
                "2026-08-31T12:06:00Z",
                _result().output,
                _source(_metadata("elapsed")),
                _source(b""),
                _source(b""),
            ),
            "budgetFailure",
        ),
        (
            RawAttemptResult(
                0,
                "2026-08-31T12:00:02Z",
                "2026-08-31T12:00:01Z",
                _result().output,
                _source(_metadata("reversed")),
                _source(b""),
                _source(b""),
            ),
            "evidenceFailure",
        ),
    ],
)
def test_request_and_elapsed_budgets_are_enforced(
    world: World, result: RawAttemptResult, classification: str
) -> None:
    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([result, _result("other")]),
        world.private_root,
        seed=b"u" * 32,
    )

    assert receipt == {
        **receipt,
        "pairValidity": "invalid",
        "invalidReason": classification,
    }


def test_output_stream_budget_is_enforced_without_unbounded_evidence_write(
    world: World,
) -> None:
    oversized = _result("oversized")
    oversized = RawAttemptResult(
        oversized.exit_code,
        oversized.started_at,
        oversized.finished_at,
        oversized.output,
        oversized.metadata,
        _source(b"x" * (1_048_576 + 1)),
        _source(b""),
    )

    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([oversized, _result("other")]),
        world.private_root,
        seed=b"o" * 32,
    )

    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] == "budgetFailure"
    assert (
        max(
            path.stat().st_size
            for path in (world.private_root / "attempts").glob("*/stdout.bin")
        )
        <= 1_048_576
    )


def test_private_root_rejects_wrong_mode_before_executor_calls(world: World) -> None:
    world.private_root.mkdir(mode=0o755)
    os.chmod(world.private_root, 0o755)
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="private|mode|permission"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"p" * 32
        )
    assert executor.calls == 0


def test_profile_and_plan_timeout_mismatch_is_rejected_before_execution(
    world: World,
) -> None:
    bindings = _bindings(world)
    profile = dict(bindings["executionProfile"])
    profile["maxWallClockSeconds"] = 299
    profile_path = Path(bindings["executionProfilePath"])
    profile_path.write_text(json.dumps(profile, sort_keys=True), encoding="utf-8")
    bindings["executionProfile"] = profile
    plan = dict(world.plan)
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
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="timeout|wall clock"):
        run_candidate_pair(plan, bindings, executor, world.private_root, seed=b"t" * 32)
    assert executor.calls == 0


def test_attempt_parent_replacement_after_first_call_fails_closed_and_calls_both(
    world: World,
) -> None:
    class ReplacingExecutor(ScriptedExecutor):
        def execute(self, request):
            result = super().execute(request)
            if self.calls == 1:
                attempts = world.private_root / "attempts"
                held = world.private_root / "attempts-held"
                attempts.rename(held)
                attacker = world.private_root.parent / "attacker-attempts"
                attacker.mkdir(mode=0o700)
                for directory in held.iterdir():
                    (attacker / directory.name).mkdir(mode=0o700)
                attempts.symlink_to(attacker, target_is_directory=True)
            return result

    executor = ReplacingExecutor([_result("first"), _result("second")])

    with pytest.raises(
        BatchControllerError, match="private|evidence|replaced|identity"
    ):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"r" * 32
        )
    assert executor.calls == 2
    assert not list((world.private_root.parent / "attacker-attempts").glob("*/*"))


def test_unrelated_partial_attempt_directory_does_not_poison_pair_verification(
    world: World,
) -> None:
    receipt = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"z" * 32,
    )
    poison = world.private_root / "attempts" / ("f" * 64)
    poison.mkdir(mode=0o700)

    verify_paired_run_receipt(receipt, world.private_root)


def test_cell_creation_failure_writes_tombstone_and_consumes_plan(
    world: World, monkeypatch: pytest.MonkeyPatch
) -> None:
    real_create = batch_controller.create_attempt_cell
    calls = 0

    def fail_second(*args, **kwargs):
        nonlocal calls
        calls += 1
        if calls == 2:
            raise RuntimeError("injected second-cell failure")
        return real_create(*args, **kwargs)

    monkeypatch.setattr(batch_controller, "create_attempt_cell", fail_second)
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="cell|isolation|pair"):
        run_candidate_pair(
            world.plan, world.bindings, executor, world.private_root, seed=b"k" * 32
        )
    assert executor.calls == 0
    assert len(list((world.private_root / "pairs").glob("*/failure.json"))) == 1

    monkeypatch.setattr(batch_controller, "create_attempt_cell", real_create)
    recovery = ScriptedExecutor([_result("recovery-a"), _result("recovery-b")])
    with pytest.raises(BatchControllerError, match="reserved|complete"):
        run_candidate_pair(
            world.plan, world.bindings, recovery, world.private_root, seed=b"l" * 32
        )
    assert recovery.calls == 0


def test_resealed_usage_inflation_is_rejected(world: World) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("same"), _result("same")]),
        world.private_root,
        seed=b"v" * 32,
    )
    arm_path = next((world.private_root / "attempts").glob("*/receipt.json"))
    arm_dir = arm_path.parent
    arm = load_exact_json(arm_path)
    metadata = load_exact_json(arm_dir / "metadata.json")
    metadata["usage"] = {
        "inputTokens": 1,
        "outputTokens": 999_998,
        "totalTokens": 999_999,
    }
    _write_json(arm_dir / "metadata.json", metadata)
    evidence = load_exact_json(arm_dir / "evidence.json")
    evidence["metadataSha256"] = sha256_file(arm_dir / "metadata.json")
    _write_json(arm_dir / "evidence.json", evidence)
    old_commitment = arm["receiptSha256"]
    arm["usage"] = metadata["usage"]
    arm["trajectorySha256"] = sha256_json(evidence)
    arm = _reseal(arm)
    _write_json(arm_path, arm)
    field = (
        "stockArmAttemptReceiptSha256"
        if pair["stockArmAttemptReceiptSha256"] == old_commitment
        else "modifiedArmAttemptReceiptSha256"
    )
    pair[field] = arm["receiptSha256"]
    pair = _reseal(pair)
    pair_path = world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json"
    _write_json(pair_path, pair)

    with pytest.raises(BatchReceiptError, match="budget|semantic|evidence"):
        verify_paired_run_receipt(pair, world.private_root)


@pytest.mark.parametrize("tamper", ["relation", "duplicate-arm"])
def test_resealed_false_pair_semantics_are_rejected(world: World, tamper: str) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("left"), _result("right")]),
        world.private_root,
        seed=b"q" * 32,
    )
    pair_dir = world.private_root / "pairs" / str(pair["pairId"])
    if tamper == "relation":
        pair["outputRelation"] = "canonicallyIdentical"
        pair.pop("anonymousMappingCommitment")
        (pair_dir / "mapping.json").unlink()
    else:
        pair["modifiedArmAttemptReceiptSha256"] = pair["stockArmAttemptReceiptSha256"]
    pair = _reseal(pair)
    _write_json(pair_dir / "receipt.json", pair)

    with pytest.raises(BatchReceiptError, match="distinct|relation|semantic|arm"):
        verify_paired_run_receipt(pair, world.private_root)


def test_resealed_false_arm_classification_is_rejected(world: World) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("same", exit_code=9), _result("same")]),
        world.private_root,
        seed=b"c" * 32,
    )
    arm_paths = list((world.private_root / "attempts").glob("*/receipt.json"))
    failed_path = next(
        path
        for path in arm_paths
        if load_exact_json(path)["exitClassification"] == "executionFailure"
    )
    arm = load_exact_json(failed_path)
    old_commitment = arm["receiptSha256"]
    arm["exitClassification"] = "completed"
    arm["failureDetails"] = None
    arm = _reseal(arm)
    _write_json(failed_path, arm)
    field = (
        "stockArmAttemptReceiptSha256"
        if pair["stockArmAttemptReceiptSha256"] == old_commitment
        else "modifiedArmAttemptReceiptSha256"
    )
    pair[field] = arm["receiptSha256"]
    pair["pairValidity"] = "valid"
    pair["invalidReason"] = None
    pair["outputRelation"] = "canonicallyIdentical"
    pair = _reseal(pair)
    pair_path = world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json"
    _write_json(pair_path, pair)

    with pytest.raises(BatchReceiptError, match="classification|semantic|exit"):
        verify_paired_run_receipt(pair, world.private_root)


def test_resealed_raw_exit_code_tamper_is_rejected(world: World) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("same"), _result("same")]),
        world.private_root,
        seed=b"h" * 32,
    )
    arm_path = next((world.private_root / "attempts").glob("*/receipt.json"))
    arm_dir = arm_path.parent
    arm = load_exact_json(arm_path)
    evidence = load_exact_json(arm_dir / "evidence.json")
    evidence["rawEvidence"]["exitCode"] = 9
    _write_json(arm_dir / "evidence.json", evidence)
    old_commitment = arm["receiptSha256"]
    arm["trajectorySha256"] = sha256_json(evidence)
    arm = _reseal(arm)
    _write_json(arm_path, arm)
    field = (
        "stockArmAttemptReceiptSha256"
        if pair["stockArmAttemptReceiptSha256"] == old_commitment
        else "modifiedArmAttemptReceiptSha256"
    )
    pair[field] = arm["receiptSha256"]
    pair = _reseal(pair)
    pair_path = world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json"
    _write_json(pair_path, pair)

    with pytest.raises(BatchReceiptError, match="classification|exit"):
        verify_paired_run_receipt(pair, world.private_root)


def test_resealed_request_count_is_rejected(world: World) -> None:
    pair = run_candidate_pair(
        world.plan,
        world.bindings,
        ScriptedExecutor([_result("same"), _result("same")]),
        world.private_root,
        seed=b"j" * 32,
    )
    arm_path = next((world.private_root / "attempts").glob("*/receipt.json"))
    arm_dir = arm_path.parent
    arm = load_exact_json(arm_path)
    metadata = load_exact_json(arm_dir / "metadata.json")
    metadata["requestCount"] = 999
    _write_json(arm_dir / "metadata.json", metadata)
    evidence = load_exact_json(arm_dir / "evidence.json")
    evidence["metadataSha256"] = sha256_file(arm_dir / "metadata.json")
    _write_json(arm_dir / "evidence.json", evidence)
    old_commitment = arm["receiptSha256"]
    arm["trajectorySha256"] = sha256_json(evidence)
    arm = _reseal(arm)
    _write_json(arm_path, arm)
    field = (
        "stockArmAttemptReceiptSha256"
        if pair["stockArmAttemptReceiptSha256"] == old_commitment
        else "modifiedArmAttemptReceiptSha256"
    )
    pair[field] = arm["receiptSha256"]
    pair = _reseal(pair)
    pair_path = world.private_root / "pairs" / str(pair["pairId"]) / "receipt.json"
    _write_json(pair_path, pair)

    with pytest.raises(BatchReceiptError, match="request|budget|semantic|evidence"):
        verify_paired_run_receipt(pair, world.private_root)
