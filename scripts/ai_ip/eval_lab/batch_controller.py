"""Randomized two-arm controller for sealed 07B candidate plans."""

from dataclasses import dataclass
from pathlib import Path

try:
    from .batch_controller_attempt import (
        capture_arm,
        prepare_arm,
        seal_arm,
        seal_arm_failure,
    )
    from .batch_controller_types import AttemptRequest, CandidateExecutor, RawAttemptResult
    from .batch_receipt_storage import seal_failure_tombstone
    from .batch_controller_support import (
        ControllerSupportError,
        ValidatedBindings,
        derive,
        identities,
        pair_seed,
        require_mapping,
        require_seed,
        validate_bindings,
    )
    from .batch_isolation import (
        create_attempt_cell,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from .batch_plan import verify_effective_condition_parity
    from .batch_plan_authority import (
        PlanAuthorityError,
        release_unstarted,
        reserve_plan,
    )
    from .batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_pair_context,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
except ImportError:
    from batch_controller_attempt import (
        capture_arm,
        prepare_arm,
        seal_arm,
        seal_arm_failure,
    )
    from batch_controller_types import AttemptRequest, CandidateExecutor, RawAttemptResult
    from batch_receipt_storage import seal_failure_tombstone
    from batch_controller_support import (
        ControllerSupportError,
        ValidatedBindings,
        derive,
        identities,
        pair_seed,
        require_mapping,
        require_seed,
        validate_bindings,
    )
    from batch_isolation import (
        create_attempt_cell,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from batch_plan import verify_effective_condition_parity
    from batch_plan_authority import (
        PlanAuthorityError,
        release_unstarted,
        reserve_plan,
    )
    from batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_pair_context,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )


class BatchControllerError(ValueError):
    pass


@dataclass
class _CountingExecutor:
    executor: CandidateExecutor
    calls: int = 0

    def execute(self, request: AttemptRequest) -> RawAttemptResult:
        self.calls += 1
        return self.executor.execute(request)


def _run_candidate_pair(
    plan_value: dict[str, object],
    validated: ValidatedBindings,
    executor: CandidateExecutor,
    private_root: Path,
    *,
    active_seed: bytes,
) -> dict[str, object]:
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
    try:
        seal_pair_context(
            pair_directory,
            plan=plan_value,
            execution_profile=validated.execution_profile,
            case_answer_schema_path=validated.case_answer_schema_path,
        )
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
    except BaseException as error:
        seal_failure_tombstone(pair_directory, pair_id, "cellCreation", error)
        raise BatchControllerError("pair cell creation or context sealing failed") from error
    verify_attempt_cells_disjoint(cells["stock"], cells["modified"])
    order_secret = derive(active_seed, b"execution-order")
    order = ("stock", "modified") if order_secret[0] % 2 == 0 else ("modified", "stock")
    arms = {"stock": validated.stock, "modified": validated.modified}
    directories = {"stock": attempt_directories[0], "modified": attempt_directories[1]}
    arm_receipts: dict[str, dict[str, object]] = {}
    outputs: dict[str, tuple[object, bytes]] = {}
    prepared = {
        name: prepare_arm(
            arms[name],
            cells[name],
            plan_value,
            validated,
        )
        for name in ("stock", "modified")
    }
    captured = {
        name: capture_arm(executor, prepared[name][1]) for name in order
    }
    sealing_errors: list[BaseException] = []
    for name in order:
        try:
            arm_receipt, output, output_bytes = seal_arm(
                arms[name],
                cells[name],
                directories[name],
                plan_value,
                validated,
                prepared[name][0],
                captured[name],
            )
            arm_receipts[name] = arm_receipt
            outputs[name] = (output, output_bytes)
        except BaseException as error:
            sealing_errors.append(error)
            try:
                seal_arm_failure(
                    directories[name],
                    cells[name],
                    captured[name],
                    int(validated.execution_profile["maxOutputBytes"]),
                    error,
                )
            except BaseException:
                pass
    if sealing_errors:
        seal_failure_tombstone(
            pair_directory, pair_id, "armEvidenceSealing", sealing_errors[0]
        )
        raise BatchControllerError("one or more arm evidence records failed to seal")
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


def run_candidate_pair(
    plan: object,
    bindings: object,
    executor: CandidateExecutor,
    private_root: Path,
    *,
    seed: bytes | None = None,
) -> dict[str, object]:
    """Execute the sole replication in a sealed plan."""
    require_supported_isolation_platform()
    try:
        plan_value = require_mapping(plan, "candidate run plan")
        validated = validate_bindings(plan_value, bindings)
        if plan_value.get("replicationCount") != 1:
            raise ControllerSupportError(
                "standalone pair requires sealed replication count 1"
            )
        active_seed = pair_seed(
            require_seed(seed), Path(private_root), plan_value["planSha256"]
        )
        identity = identities(active_seed)
        pair_id = identity[0]
        assert_private_layout_available(Path(private_root), (identity,))
        reservation = reserve_plan(
            Path(private_root), str(plan_value["planSha256"]), 1, (pair_id,)
        )
    except (BatchReceiptError, ControllerSupportError, PlanAuthorityError) as error:
        raise BatchControllerError(str(error)) from error
    counted = _CountingExecutor(executor)
    try:
        return _run_candidate_pair(
            plan_value,
            validated,
            counted,
            Path(private_root),
            active_seed=active_seed,
        )
    except BatchReceiptError as error:
        if counted.calls == 0:
            try:
                release_unstarted(reservation)
            except PlanAuthorityError:
                pass
        raise BatchControllerError(str(error)) from error
    except BaseException:
        if counted.calls == 0:
            try:
                release_unstarted(reservation)
            except PlanAuthorityError:
                pass
        raise


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
        validated = validate_bindings(plan_value, bindings)
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
        reservation = reserve_plan(
            Path(private_root),
            str(plan_value["planSha256"]),
            replication_count,
            tuple(value[0] for value in batch_identities),
        )
    except (BatchReceiptError, PlanAuthorityError) as error:
        raise BatchControllerError(str(error)) from error
    counted = _CountingExecutor(executor)
    try:
        return [
            _run_candidate_pair(
                plan_value,
                validated,
                counted,
                Path(private_root),
                active_seed=active_seed,
            )
            for active_seed in pair_seeds
        ]
    except BatchReceiptError as error:
        if counted.calls == 0:
            try:
                release_unstarted(reservation)
            except PlanAuthorityError:
                pass
        raise BatchControllerError(str(error)) from error
    except BaseException:
        if counted.calls == 0:
            try:
                release_unstarted(reservation)
            except PlanAuthorityError:
                pass
        raise
