"""Randomized two-arm controller for sealed 07B candidate plans."""

from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Mapping, Protocol

try:
    from .batch_controller_support import (
        ArmBinding,
        ControllerSupportError,
        ValidatedBindings,
        classify_attempt_result,
        derive,
        freeze_json,
        identities,
        pair_seed,
        require_mapping,
        require_seed,
        validate_bindings,
    )
    from .batch_isolation import (
        AttemptCell,
        create_attempt_cell,
        mark_receipts_sealed,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from .batch_plan import sha256_tree, verify_effective_condition_parity
    from .batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_arm_attempt_receipt,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from .contracts import LabContractError, canonical_json_bytes, sha256_json
except ImportError:
    from batch_controller_support import (
        ArmBinding,
        ControllerSupportError,
        ValidatedBindings,
        classify_attempt_result,
        derive,
        freeze_json,
        identities,
        pair_seed,
        require_mapping,
        require_seed,
        validate_bindings,
    )
    from batch_isolation import (
        AttemptCell,
        create_attempt_cell,
        mark_receipts_sealed,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from batch_plan import sha256_tree, verify_effective_condition_parity
    from batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_arm_attempt_receipt,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from contracts import LabContractError, canonical_json_bytes, sha256_json


class BatchControllerError(ValueError):
    pass


@dataclass(frozen=True)
class AttemptRequest:
    cell: AttemptCell
    binary_path: Path
    case_bundle: object
    case_answer_schema: object
    model_route: Mapping[str, object]
    execution_profile: object
    timeout_seconds: int
    token_budget: int
    request_budget: int
    cost_budget_cny: int


@dataclass(frozen=True)
class RawAttemptResult:
    exit_code: int
    started_at: str
    finished_at: str
    output: object | None
    metadata: object | None
    stdout: bytes
    stderr: bytes


class CandidateExecutor(Protocol):
    """Execute one isolated request exactly once and return its complete raw evidence."""

    def execute(self, request: AttemptRequest) -> RawAttemptResult: ...


def _failure_result(error: BaseException) -> RawAttemptResult:
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    timed_out = isinstance(error, TimeoutError)
    metadata = {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": sha256_json(type(error).__name__),
        },
        "executorError": type(error).__name__,
        "threadId": None,
        "timedOut": timed_out,
        "trajectory": None,
        "turnId": None,
        "usage": {"inputTokens": 0, "outputTokens": 0, "totalTokens": 0},
    }
    return RawAttemptResult(-1, now, now, None, metadata, b"", str(error).encode())


def _execute_arm(
    *,
    arm: ArmBinding,
    cell: AttemptCell,
    directory: Path,
    plan: dict[str, object],
    bindings: ValidatedBindings,
    executor: CandidateExecutor,
) -> tuple[dict[str, object], object, bytes]:
    before = sha256_tree(cell.workspace)
    request = AttemptRequest(
        cell,
        arm.binary_path,
        freeze_json(bindings.case_bundle),
        freeze_json(bindings.case_answer_schema),
        dict(bindings.model_route),
        freeze_json(bindings.execution_profile),
        int(plan["timeoutBudget"]),
        int(plan["tokenBudget"]),
        int(plan["requestBudget"]),
        int(plan["costBudgetCny"]),
    )
    try:
        raw = executor.execute(request)
        if not isinstance(raw, RawAttemptResult):
            raise TypeError("executor returned an invalid result")
    except BaseException as error:
        raw = _failure_result(error)
    try:
        output = freeze_json(raw.output)
    except (LabContractError, TypeError, ValueError):
        output = {"unserializableOutputType": type(raw.output).__name__}
    try:
        metadata = freeze_json(raw.metadata)
    except (LabContractError, TypeError, ValueError):
        metadata = {"unserializableMetadataType": type(raw.metadata).__name__}
    after = sha256_tree(cell.workspace)
    classification, details = classify_attempt_result(
        raw.exit_code, output, metadata, plan, bindings.case_answer_schema_path
    )
    receipt = seal_arm_attempt_receipt(
        directory,
        attempt_id=cell.attempt_id,
        pair_id=cell.pair_id,
        private_arm_id=arm.private_arm_id,
        treatment_manifest_sha256=str(
            arm.treatment_manifest["treatmentManifestSha256"]
        ),
        binary_manifest_sha256=arm.binary_manifest_sha256,
        effective_config_sha256=arm.effective_config_sha256,
        input_sha256=str(plan["caseBundleSha256"]),
        workspace_before_sha256=before,
        workspace_after_sha256=after,
        workspace_path=cell.workspace,
        app_server_protocol_schema_sha256=bindings.protocol_sha256,
        promptfoo_config_sha256=bindings.promptfoo_config_sha256,
        started_at=raw.started_at,
        finished_at=raw.finished_at,
        output=output,
        metadata=metadata,
        stdout=bytes(raw.stdout),
        stderr=bytes(raw.stderr),
        exit_classification=classification,
        failure_details=details,
    )
    mark_receipts_sealed(cell)
    return receipt, output, canonical_json_bytes(output)


def run_candidate_pair(
    plan: object,
    bindings: object,
    executor: CandidateExecutor,
    private_root: Path,
    *,
    seed: bytes | None = None,
) -> dict[str, object]:
    """Execute exactly one sealed, randomized two-arm pair and return its public receipt."""
    require_supported_isolation_platform()
    try:
        plan_value = require_mapping(plan, "candidate run plan")
        validated = validate_bindings(plan_value, bindings)
        active_seed = pair_seed(
            require_seed(seed), Path(private_root), plan_value["planSha256"]
        )
    except ControllerSupportError as error:
        raise BatchControllerError(str(error)) from error
    pair_id, stock_attempt_id, modified_attempt_id = identities(active_seed)
    try:
        assert_private_layout_available(
            Path(private_root), ((pair_id, stock_attempt_id, modified_attempt_id),)
        )
        pair_directory, attempt_directories = prepare_private_layout(
            Path(private_root), pair_id, (stock_attempt_id, modified_attempt_id)
        )
    except BatchReceiptError as error:
        raise BatchControllerError(str(error)) from error
    cells = {
        "stock": create_attempt_cell(
            validated.attempt_base,
            pair_id,
            stock_attempt_id,
            validated.stock.codex_home_seed,
            validated.workspace_seed,
            validated.execution_profile,
            validated.source_environment,
        ),
        "modified": create_attempt_cell(
            validated.attempt_base,
            pair_id,
            modified_attempt_id,
            validated.modified.codex_home_seed,
            validated.workspace_seed,
            validated.execution_profile,
            validated.source_environment,
        ),
    }
    verify_attempt_cells_disjoint(cells["stock"], cells["modified"])
    order_secret = derive(active_seed, b"execution-order")
    order = ("stock", "modified") if order_secret[0] % 2 == 0 else ("modified", "stock")
    arms = {"stock": validated.stock, "modified": validated.modified}
    directories = {"stock": attempt_directories[0], "modified": attempt_directories[1]}
    arm_receipts: dict[str, dict[str, object]] = {}
    outputs: dict[str, tuple[object, bytes]] = {}
    for name in order:
        arm_receipt, output, output_bytes = _execute_arm(
            arm=arms[name],
            cell=cells[name],
            directory=directories[name],
            plan=plan_value,
            bindings=validated,
            executor=executor,
        )
        arm_receipts[name] = arm_receipt
        outputs[name] = (output, output_bytes)
    for receipt in arm_receipts.values():
        verify_arm_attempt_receipt(receipt, private_root)
    failures = [
        str(arm_receipts[name]["exitClassification"])
        for name in ("stock", "modified")
        if arm_receipts[name]["exitClassification"] != "completed"
    ]
    valid = not failures
    relation: str | None = None
    mapping: dict[str, object] | None = None
    if valid:
        same_bytes = outputs["stock"][1] == outputs["modified"][1]
        same_hash = (
            arm_receipts["stock"]["outputSha256"]
            == arm_receipts["modified"]["outputSha256"]
        )
        relation = "canonicallyIdentical" if same_bytes and same_hash else "distinct"
        if relation == "distinct":
            mapping_secret = derive(active_seed, b"anonymous-mapping")
            labels = ("A", "B") if mapping_secret[0] % 2 == 0 else ("B", "A")
            mapping = {
                "labelToPrivateArmId": {
                    labels[0]: validated.stock.private_arm_id,
                    labels[1]: validated.modified.private_arm_id,
                },
                "mappingNonce": mapping_secret.hex(),
                "pairId": pair_id,
            }
    order_record = {
        "attemptOrder": [cells[name].attempt_id for name in order],
        "orderNonce": order_secret.hex(),
        "pairId": pair_id,
    }
    parity = verify_effective_condition_parity(
        validated.stock.effective_conditions, validated.modified.effective_conditions
    )
    pair_receipt = seal_paired_run_receipt(
        pair_directory,
        pair_id=pair_id,
        plan_sha256=str(plan_value["planSha256"]),
        stock_receipt=arm_receipts["stock"],
        modified_receipt=arm_receipts["modified"],
        condition_parity_sha256=parity,
        order_record=order_record,
        pair_validity="valid" if valid else "invalid",
        invalid_reason=None if valid else failures[0],
        output_relation=relation,
        mapping_record=mapping,
    )
    verify_paired_run_receipt(pair_receipt, private_root)
    return pair_receipt


def run_candidate_batch(
    plan: object,
    bindings: object,
    executor: CandidateExecutor,
    private_root: Path,
    *,
    seed: bytes | None = None,
) -> list[dict[str, object]]:
    """Execute exactly the pre-sealed replication count without adaptive stopping."""
    require_supported_isolation_platform()
    try:
        plan_value = require_mapping(plan, "candidate run plan")
        validate_bindings(plan_value, bindings)
        master_seed = require_seed(seed)
    except ControllerSupportError as error:
        raise BatchControllerError(str(error)) from error
    replication_count = plan_value["replicationCount"]
    if type(replication_count) is not int or replication_count < 1:
        raise BatchControllerError("replication count must be a positive integer")
    raw_pair_seeds = tuple(
        derive(master_seed, b"replication:" + index.to_bytes(8, "big"))
        for index in range(replication_count)
    )
    pair_seeds = tuple(
        pair_seed(raw_seed, Path(private_root), plan_value["planSha256"])
        for raw_seed in raw_pair_seeds
    )
    batch_identities = tuple(identities(active_seed) for active_seed in pair_seeds)
    try:
        assert_private_layout_available(Path(private_root), batch_identities)
    except BatchReceiptError as error:
        raise BatchControllerError(str(error)) from error
    return [
        run_candidate_pair(
            plan_value, bindings, executor, private_root, seed=raw_pair_seed
        )
        for raw_pair_seed in raw_pair_seeds
    ]
