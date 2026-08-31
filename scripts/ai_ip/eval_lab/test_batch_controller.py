import copy
import json
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path

import pytest


sys.path.insert(0, str(Path(__file__).resolve().parent))
import batch_isolation  # noqa: E402
from batch_controller import (  # noqa: E402
    AttemptRequest,
    BatchControllerError,
    RawAttemptResult,
    run_candidate_batch,
    run_candidate_pair,
)
from batch_isolation import UnsupportedIsolationPlatformError  # noqa: E402
from batch_plan import PARITY_FIELDS, seal_candidate_run_plan, sha256_file, sha256_tree  # noqa: E402
from batch_receipts import (  # noqa: E402
    BatchReceiptError,
    verify_arm_attempt_receipt,
    verify_paired_run_receipt,
)
from contracts import load_exact_json, sha256_json  # noqa: E402


REPO_ROOT = Path(__file__).resolve().parents[3]
FIXTURE_ROOT = REPO_ROOT / "ai-ip-evals" / "lab" / "fixtures" / "batch-runner"
CASE_ANSWER_SCHEMA = REPO_ROOT / "ai-ip-evals" / "lab" / "schemas" / "case-answer.schema.json"


def _sealed_treatment(treatment_id: str) -> dict[str, object]:
    value: dict[str, object] = {
        "schemaVersion": 1,
        "treatmentId": treatment_id,
        "binaryManifestRef": f"{treatment_id}-binary",
        "codexHomeSeedSha256": "a" * 64,
        "systemInstructionSha256": "b" * 64,
        "capabilityBundleSha256": "c" * 64,
        "effectiveCodexConfigSha256": "d" * 64,
        "declaredCapabilities": [],
        "prohibitedCaseSpecificMaterial": ["case answers", "review rubrics"],
        "treatmentManifestSha256": "0" * 64,
    }
    value["treatmentManifestSha256"] = sha256_json(
        {key: item for key, item in value.items() if key != "treatmentManifestSha256"}
    )
    return value


def _answer(label: str = "direction", *, case_id: str = "case-1") -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "objectKind": "CaseAnswer",
        "caseId": case_id,
        "taskLevels": ["L1", "L2"],
        "subject": "sealed subject",
        "audiences": ["audience"],
        "desiredActions": ["act"],
        "evidenceGaps": [],
        "directionOptions": [
            {
                "directionId": label,
                "directionFamily": "family",
                "rationale": "because",
                "tradeoffs": ["tradeoff"],
                "conditions": ["condition"],
                "evidenceRefs": [],
            }
        ],
        "recommendedDirectionId": label,
        "claims": [],
        "readiness": "readyForHumanReview",
    }


def _metadata(marker: str = "ok") -> dict[str, object]:
    return {
        "threadId": f"thread-{marker}",
        "turnId": f"turn-{marker}",
        "trajectory": [{"type": "turn.completed", "marker": marker}],
        "usage": {"inputTokens": 1, "outputTokens": 2, "totalTokens": 3},
        "costEvidence": {"costCny": 0, "sourceSha256": "e" * 64},
    }


def _result(
    label: str = "direction",
    *,
    output: object | None = None,
    metadata: object | None = None,
    exit_code: int = 0,
) -> RawAttemptResult:
    return RawAttemptResult(
        exit_code=exit_code,
        started_at="2026-08-31T12:00:00Z",
        finished_at="2026-08-31T12:00:01Z",
        output=_answer(label) if output is None else output,
        metadata=_metadata(label) if metadata is None else metadata,
        stdout=f"stdout-{label}".encode(),
        stderr=f"stderr-{label}".encode(),
    )


@dataclass
class ScriptedExecutor:
    results: list[RawAttemptResult]

    def __post_init__(self) -> None:
        self.requests: list[AttemptRequest] = []

    @property
    def calls(self) -> int:
        return len(self.requests)

    def execute(self, request: AttemptRequest) -> RawAttemptResult:
        self.requests.append(request)
        return self.results[len(self.requests) - 1]


@dataclass(frozen=True)
class World:
    plan: dict[str, object]
    bindings: dict[str, object]
    private_root: Path


@pytest.fixture
def world(tmp_path: Path) -> World:
    stock_treatment = _sealed_treatment("stock-treatment")
    modified_treatment = _sealed_treatment("modified-treatment")
    case_bundle = {"caseId": "case-1", "prompt": "Return one CaseAnswer."}
    plan: dict[str, object] = {
        "schemaVersion": 1,
        "planId": "plan-1",
        "batchId": "batch-1",
        "batchKind": "smoke",
        "caseBundleId": "case-1",
        "caseBundleSha256": sha256_json(case_bundle),
        "caseAnswerSchemaId": "case-answer-v1",
        "caseAnswerSchemaSha256": sha256_file(CASE_ANSWER_SCHEMA),
        "stockTreatmentRef": "stock-treatment",
        "modifiedTreatmentRef": "modified-treatment",
        "modelRouteRef": "loopback-model",
        "executionProfileRef": "offline-profile",
        "workspaceTemplateSha256": sha256_tree(FIXTURE_ROOT / "workspace"),
        "promptfooPackageVersion": "0.122.0",
        "promptfooLockSha256": "f" * 64,
        "replicationCount": 1,
        "retryPolicy": {"maxRetries": 0},
        "timeoutBudget": 300,
        "tokenBudget": 4096,
        "requestBudget": 2,
        "costBudgetCny": 0,
        "networkPolicy": "offline",
        "permissionPolicy": "never",
        "toolPolicy": "minimal",
        "createdAt": "2026-08-31T12:00:00Z",
        "sealedAt": "2026-08-31T12:01:00Z",
        "planSha256": "0" * 64,
    }
    plan = seal_candidate_run_plan(
        plan,
        stock_treatment=stock_treatment,
        modified_treatment=modified_treatment,
    )
    conditions = {field: plan[field] for field in PARITY_FIELDS}
    attempt_base = tmp_path / "cells"
    attempt_base.mkdir()
    bindings: dict[str, object] = {
        "stock": {
            "privateArmId": "opaque-arm-a",
            "treatmentManifest": stock_treatment,
            "binaryManifestSha256": "1" * 64,
            "binaryPath": tmp_path / "stock-codex",
            "codexHomeSeed": FIXTURE_ROOT / "stock-seed",
            "effectiveConfigSha256": "2" * 64,
            "effectiveConditions": conditions,
        },
        "modified": {
            "privateArmId": "opaque-arm-b",
            "treatmentManifest": modified_treatment,
            "binaryManifestSha256": "3" * 64,
            "binaryPath": tmp_path / "modified-codex",
            "codexHomeSeed": FIXTURE_ROOT / "modified-seed",
            "effectiveConfigSha256": "4" * 64,
            "effectiveConditions": conditions,
        },
        "attemptBase": attempt_base,
        "workspaceSeed": FIXTURE_ROOT / "workspace",
        "executionProfile": {
            "schemaVersion": 1,
            "sandboxMode": "workspace-write",
            "approvalPolicy": "never",
            "networkMode": "offline",
            "networkAllowlist": [],
            "writablePaths": ["workspace"],
            "externalTools": [],
            "environmentAllowlist": ["LANG"],
            "maxWallClockSeconds": 300,
            "maxOutputBytes": 1_048_576,
        },
        "sourceEnvironment": {"PATH": "/safe/bin", "LANG": "C.UTF-8"},
        "caseBundle": case_bundle,
        "caseAnswerSchema": load_exact_json(CASE_ANSWER_SCHEMA),
        "caseAnswerSchemaPath": CASE_ANSWER_SCHEMA,
        "modelRoute": {"model": "loopback-model", "baseUrl": "http://127.0.0.1:1/v1"},
        "appServerProtocolSchemaSha256": "5" * 64,
        "promptfooConfigSha256": "6" * 64,
        "retryPolicy": {"maxAttemptsPerArm": 1},
    }
    return World(plan, bindings, tmp_path / "private")


def _run(world: World, executor: ScriptedExecutor, seed: bytes = b"x" * 32):
    return run_candidate_pair(
        world.plan, world.bindings, executor, world.private_root, seed=seed
    )


def test_one_arm_failure_invalidates_pair_but_retains_two_attempts(world: World) -> None:
    executor = ScriptedExecutor([_result("stock"), _result("modified", exit_code=17)])

    receipt = _run(world, executor)

    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] == "executionFailure"
    assert executor.calls == 2
    assert len(list((world.private_root / "attempts").iterdir())) == 2


def test_identical_outputs_are_a_valid_tie_and_seal_both_arms(world: World) -> None:
    executor = ScriptedExecutor([_result("same"), _result("same")])

    receipt = _run(world, executor)

    assert receipt["pairValidity"] == "valid"
    assert receipt["outputRelation"] == "canonicallyIdentical"
    assert "anonymousMappingCommitment" not in receipt
    verify_paired_run_receipt(receipt, world.private_root)
    arm_receipts = [
        load_exact_json(path)
        for path in sorted((world.private_root / "attempts").glob("*/receipt.json"))
    ]
    assert len(arm_receipts) == 2
    for arm_receipt in arm_receipts:
        verify_arm_attempt_receipt(arm_receipt, world.private_root)


def test_distinct_outputs_create_only_an_opaque_mapping_commitment(
    world: World, capsys: pytest.CaptureFixture[str]
) -> None:
    receipt = _run(world, ScriptedExecutor([_result("left"), _result("right")]))

    assert receipt["outputRelation"] == "distinct"
    assert len(receipt["anonymousMappingCommitment"]) == 64
    public = json.dumps(receipt)
    assert "opaque-arm" not in public
    assert str(world.private_root) not in public
    assert capsys.readouterr() == ("", "")


def test_seeded_pairs_execute_both_orders(world: World) -> None:
    observed: set[tuple[str, str]] = set()
    for index in range(256):
        private_root = world.private_root.parent / f"private-{index}"
        executor = ScriptedExecutor([_result(f"{index}-1"), _result(f"{index}-2")])
        run_candidate_pair(
            world.plan,
            world.bindings,
            executor,
            private_root,
            seed=index.to_bytes(32, "big"),
        )
        observed.add(tuple(request.binary_path.name for request in executor.requests))
        if len(observed) == 2:
            break

    assert observed == {("stock-codex", "modified-codex"), ("modified-codex", "stock-codex")}


@pytest.mark.parametrize(
    ("result", "classification"),
    [
        (_result(output=_answer(case_id="wrong-case")), "schemaFailure"),
        (_result(output={"schemaVersion": 1, "objectKind": "CaseAnswer"}), "schemaFailure"),
        (_result(metadata={**_metadata(), "trajectory": None}), "evidenceFailure"),
        (_result(metadata={**_metadata(), "timedOut": True}), "executionFailure"),
        (_result(exit_code=9), "executionFailure"),
    ],
)
def test_mechanical_failures_invalidate_pair_and_preserve_raw_evidence(
    world: World,
    result: RawAttemptResult,
    classification: str,
) -> None:
    receipt = _run(world, ScriptedExecutor([result, _result("other")]))

    assert receipt["pairValidity"] == "invalid"
    assert receipt["invalidReason"] == classification
    assert len(list((world.private_root / "attempts").glob("*/stdout.bin"))) == 2
    assert len(list((world.private_root / "attempts").glob("*/stderr.bin"))) == 2
    assert len(list((world.private_root / "attempts").glob("*/metadata.json"))) == 2


def test_post_receipt_private_evidence_mutation_is_rejected(world: World) -> None:
    receipt = _run(world, ScriptedExecutor([_result("left"), _result("right")]))
    output_path = next((world.private_root / "attempts").glob("*/output.json"))
    output_path.write_text("{}\n", encoding="utf-8")

    with pytest.raises(BatchReceiptError, match="evidence commitment mismatch"):
        verify_paired_run_receipt(receipt, world.private_root)


def test_duplicate_attempt_and_existing_pair_are_rejected_without_new_calls(world: World) -> None:
    first = ScriptedExecutor([_result("left"), _result("right")])
    receipt = _run(world, first)
    second = ScriptedExecutor([_result("third"), _result("fourth")])

    with pytest.raises(BatchControllerError, match="pair already exists"):
        _run(world, second)
    assert second.calls == 0

    pair_dir = world.private_root / "pairs" / receipt["pairId"]
    shutil.rmtree(pair_dir)
    with pytest.raises(BatchControllerError, match="attempt already exists"):
        _run(world, second)
    assert second.calls == 0


def test_batch_executes_exactly_the_sealed_replication_count(world: World) -> None:
    plan = dict(world.plan)
    plan["replicationCount"] = 3
    plan["planSha256"] = "0" * 64
    plan = seal_candidate_run_plan(
        plan,
        stock_treatment=world.bindings["stock"]["treatmentManifest"],
        modified_treatment=world.bindings["modified"]["treatmentManifest"],
    )
    conditions = {field: plan[field] for field in PARITY_FIELDS}
    bindings = copy.deepcopy(world.bindings)
    bindings["attemptBase"] = world.bindings["attemptBase"]
    bindings["workspaceSeed"] = world.bindings["workspaceSeed"]
    bindings["caseAnswerSchemaPath"] = world.bindings["caseAnswerSchemaPath"]
    bindings["stock"]["codexHomeSeed"] = world.bindings["stock"]["codexHomeSeed"]
    bindings["modified"]["codexHomeSeed"] = world.bindings["modified"]["codexHomeSeed"]
    bindings["stock"]["binaryPath"] = world.bindings["stock"]["binaryPath"]
    bindings["modified"]["binaryPath"] = world.bindings["modified"]["binaryPath"]
    bindings["stock"]["effectiveConditions"] = conditions
    bindings["modified"]["effectiveConditions"] = conditions
    executor = ScriptedExecutor(
        [_result("failed", exit_code=1)] + [_result(f"result-{index}") for index in range(5)]
    )

    receipts = run_candidate_batch(
        plan, bindings, executor, world.private_root, seed=b"b" * 32
    )

    assert len(receipts) == 3
    assert len({receipt["pairId"] for receipt in receipts}) == 3
    assert executor.calls == 6
    assert len(list((world.private_root / "pairs").iterdir())) == 3


def test_controller_windows_gate_prevents_every_executor_call(
    world: World,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(batch_isolation, "_platform_name", lambda: "Windows")
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(
        UnsupportedIsolationPlatformError,
        match="separate Windows redesign and native CI",
    ):
        _run(world, executor)
    assert executor.calls == 0
    assert not world.private_root.exists()


def test_only_one_presealed_attempt_per_arm_is_accepted(world: World) -> None:
    bad_bindings = {**world.bindings, "retryPolicy": {"maxAttemptsPerArm": 2}}
    executor = ScriptedExecutor([_result("left"), _result("right")])

    with pytest.raises(BatchControllerError, match="maxAttemptsPerArm=1"):
        run_candidate_pair(
            world.plan, bad_bindings, executor, world.private_root, seed=b"r" * 32
        )
    assert executor.calls == 0
