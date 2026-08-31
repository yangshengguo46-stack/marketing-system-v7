"""Randomized two-arm controller for sealed 07B candidate plans."""

from contextlib import ExitStack
from pathlib import Path

try:
    from .batch_controller_attempt import (
        prepare_arm,
        seal_arm,
        seal_arm_failure,
    )
    from . import batch_controller_process as _process
    from .batch_controller_identity import ExecutionIdentityError
    from .batch_controller_lifecycle import PairLifecycle, PairLifecycleError
    from .batch_controller_reservation import reserve_batch, reserve_single
    from . import batch_controller_types as _types
    from .batch_launch_spec import (
        CandidateLaunchSet,
        LaunchArtifact,
        LaunchSpec,
        LaunchSpecError,
        ValidatedLaunchSet,
        launch_spec_identity,
        validate_launch_set,
    )
    from .batch_receipt_storage import seal_failure_tombstone
    from .batch_controller_staging import stage_inputs
    from .batch_controller_support import (
        ControllerSupportError,
        ValidatedBindings,
        derive,
        identities,
        require_mapping,
        validate_bindings,
    )
    from .batch_isolation import (
        create_attempt_cell,
        cleanup_attempt_cell,
        mark_receipts_sealed,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from .batch_plan import verify_effective_condition_parity
    from .batch_plan_authority import PlanAuthorityError, reserve_plan
    from .batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_pair_context,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from .contracts import compile_contract, sha256_json
except ImportError:
    from batch_controller_attempt import (
        prepare_arm,
        seal_arm,
        seal_arm_failure,
    )
    import batch_controller_process as _process
    from batch_controller_identity import ExecutionIdentityError
    from batch_controller_lifecycle import PairLifecycle, PairLifecycleError
    from batch_controller_reservation import reserve_batch, reserve_single
    import batch_controller_types as _types
    from batch_launch_spec import (
        CandidateLaunchSet,
        LaunchArtifact,
        LaunchSpec,
        LaunchSpecError,
        ValidatedLaunchSet,
        launch_spec_identity,
        validate_launch_set,
    )
    from batch_receipt_storage import seal_failure_tombstone
    from batch_controller_staging import stage_inputs
    from batch_controller_support import (
        ControllerSupportError,
        ValidatedBindings,
        derive,
        identities,
        require_mapping,
        validate_bindings,
    )
    from batch_isolation import (
        create_attempt_cell,
        cleanup_attempt_cell,
        mark_receipts_sealed,
        require_supported_isolation_platform,
        verify_attempt_cells_disjoint,
    )
    from batch_plan import verify_effective_condition_parity
    from batch_plan_authority import PlanAuthorityError, reserve_plan
    from batch_receipts import (
        BatchReceiptError,
        assert_private_layout_available,
        prepare_private_layout,
        seal_pair_context,
        seal_paired_run_receipt,
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from contracts import compile_contract, sha256_json


AttemptAttestation = _types.AttemptAttestation
AttemptByteSource = _types.AttemptByteSource
AttemptRequest = _types.AttemptRequest
AttemptTelemetry = _types.AttemptTelemetry
CandidateExecutor = _types.CandidateExecutor
MemoryAttemptByteSource = _types.MemoryAttemptByteSource
RawAttemptResult = _types.RawAttemptResult
RunningAttempt = _types.RunningAttempt


class BatchControllerError(ValueError):
    pass


def _run_candidate_pair(
    plan_value: dict[str, object],
    validated: ValidatedBindings,
    launches: ValidatedLaunchSet,
    private_root: Path,
    *,
    active_seed: bytes,
) -> dict[str, object]:
    pair_id = identities(active_seed)[0]
    with ExitStack() as resources, PairLifecycle(private_root, pair_id) as lifecycle:
        return _run_candidate_pair_scoped(
            plan_value,
            validated,
            launches,
            private_root,
            active_seed=active_seed,
            resources=resources,
            lifecycle=lifecycle,
        )


def _run_candidate_pair_scoped(
    plan_value: dict[str, object],
    validated: ValidatedBindings,
    launches: ValidatedLaunchSet,
    private_root: Path,
    *,
    active_seed: bytes,
    resources: ExitStack,
    lifecycle: PairLifecycle,
) -> dict[str, object]:
    pair_id, stock_attempt_id, modified_attempt_id = identities(active_seed)
    try:
        assert_private_layout_available(
            Path(private_root), ((pair_id, stock_attempt_id, modified_attempt_id),)
        )
        layout = prepare_private_layout(
            Path(private_root), pair_id, (stock_attempt_id, modified_attempt_id)
        )
        pair_directory = layout.pair_directory
        attempt_directories = layout.attempt_directories
        resources.callback(layout.close)
        lifecycle.bind_layout(layout)
    except (BatchReceiptError, ExecutionIdentityError, PairLifecycleError) as error:
        raise BatchControllerError(str(error)) from error
    cells: dict[str, object] = {}
    staged = None
    try:
        layout.verify()
        staged = stage_inputs(validated, pair_id)
        resources.callback(staged.cleanup)
        seal_pair_context(
            pair_directory,
            plan=plan_value,
            execution_profile=validated.execution_profile,
            case_answer_schema_path=staged.schema_path,
            identity_context={
                "stock": {
                    "privateArmId": validated.stock.private_arm_id,
                    "treatmentManifestSha256": validated.stock.treatment_manifest[
                        "treatmentManifestSha256"
                    ],
                    "binaryManifestSha256": validated.stock.binary_manifest_sha256,
                    "binarySha256": validated.stock.binary.sha256,
                    "codexHomeSeedSha256": validated.stock.codex_home_seed.digest,
                    "effectiveConfigSha256": validated.stock.effective_config_sha256,
                    "effectiveConditions": validated.stock.effective_conditions,
                    "launchSpecSha256": launch_spec_identity(launches.stock)[1],
                },
                "modified": {
                    "privateArmId": validated.modified.private_arm_id,
                    "treatmentManifestSha256": validated.modified.treatment_manifest[
                        "treatmentManifestSha256"
                    ],
                    "binaryManifestSha256": validated.modified.binary_manifest_sha256,
                    "binarySha256": validated.modified.binary.sha256,
                    "codexHomeSeedSha256": validated.modified.codex_home_seed.digest,
                    "effectiveConfigSha256": validated.modified.effective_config_sha256,
                    "effectiveConditions": validated.modified.effective_conditions,
                    "launchSpecSha256": launch_spec_identity(launches.modified)[1],
                },
                "inputSha256": plan_value["caseBundleSha256"],
                "caseAnswerRuntimeSha256": sha256_json(validated.case_answer_schema),
                "workspaceBeforeSha256": validated.workspace_seed.cell_digest(),
                "appServerProtocolSchemaSha256": validated.protocol_sha256,
                "promptfooConfigSha256": validated.promptfoo_config_sha256,
                "executionProfileSha256": plan_value["executionProfileRef"],
                "modelRouteSha256": plan_value["modelRouteRef"],
            },
        )
        cells["stock"] = create_attempt_cell(
            validated.attempt_base,
            pair_id,
            stock_attempt_id,
            staged.stock_seed,
            staged.workspace_seed,
            validated.execution_profile,
            validated.source_environment,
        )
        lifecycle.bind_cell(cells["stock"])
        cells["modified"] = create_attempt_cell(
            validated.attempt_base,
            pair_id,
            modified_attempt_id,
            staged.modified_seed,
            staged.workspace_seed,
            validated.execution_profile,
            validated.source_environment,
        )
        lifecycle.bind_cell(cells["modified"])
    except BaseException as error:
        seal_failure_tombstone(pair_directory, pair_id, "cellCreation", error)
        for directory in attempt_directories:
            try:
                seal_failure_tombstone(directory, pair_id, "cellCreation", error)
            except BaseException:
                pass
        for cell in cells.values():
            try:
                mark_receipts_sealed(cell)
                cleanup_attempt_cell(cell)
            except BaseException:
                pass
        raise BatchControllerError(
            "pair cell creation or context sealing failed"
        ) from error
    verify_attempt_cells_disjoint(cells["stock"], cells["modified"])
    order_secret = derive(active_seed, b"execution-order")
    order = ("stock", "modified") if order_secret[0] % 2 == 0 else ("modified", "stock")
    arms = {"stock": validated.stock, "modified": validated.modified}
    directories = {"stock": attempt_directories[0], "modified": attempt_directories[1]}
    arm_receipts: dict[str, dict[str, object]] = {}
    outputs: dict[str, tuple[object, bytes]] = {}
    case_answer_validator = compile_contract(validated.case_answer_schema_file.payload)
    prepared = {
        name: prepare_arm(
            arms[name],
            cells[name],
            plan_value,
            validated,
        )
        for name in ("stock", "modified")
    }
    process_specs = {"stock": launches.stock, "modified": launches.modified}
    processes: dict[str, object] = {}
    launch_errors: list[BaseException] = []
    for name in order:
        try:
            candidate = _process.prepare_process(prepared[name][1], process_specs[name])
            owned = _process.spawn(
                candidate,
                min(
                    int(validated.execution_profile["maxOutputBytes"]),
                    16 * 1024 * 1024,
                ),
                lifecycle,
            )
            processes[name] = owned
            _process.seal_launch_record(owned, directories[name])
        except BaseException as error:
            launch_errors.append(error)
    if launch_errors:
        raise BatchControllerError(
            "candidate process launch failed"
        ) from launch_errors[0]
    captured = _process.supervise_pair(processes)
    try:
        layout.verify()
    except BaseException as error:
        seal_failure_tombstone(pair_directory, pair_id, "directoryIdentity", error)
        raise BatchControllerError(
            "private evidence directory identity was replaced"
        ) from error
    sealing_errors: list[BaseException] = []
    for name in order:
        try:
            layout.verify()
            arm_receipt, output, output_bytes = seal_arm(
                arms[name],
                cells[name],
                directories[name],
                plan_value,
                validated,
                prepared[name][0],
                captured[name],
                case_answer_validator,
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
        "layoutIdentitySha256": layout.identity_sha256,
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
    launches: CandidateLaunchSet,
    private_root: Path,
    *,
    seed: bytes | None = None,
) -> dict[str, object]:
    require_supported_isolation_platform()
    try:
        plan_value = require_mapping(plan, "candidate run plan")
        validated = validate_bindings(plan_value, bindings)
        validated_launches = validate_launch_set(launches, validated)
        plan_value = validated.plan
        if plan_value.get("replicationCount") != 1:
            raise ControllerSupportError(
                "standalone pair requires sealed replication count 1"
            )
        active_seed = reserve_single(plan_value, Path(private_root), seed, reserve_plan)
    except ValueError as error:
        raise BatchControllerError(str(error)) from error
    try:
        return _run_candidate_pair(
            plan_value,
            validated,
            validated_launches,
            Path(private_root),
            active_seed=active_seed,
        )
    except (
        BatchReceiptError,
        ExecutionIdentityError,
        LaunchSpecError,
        PairLifecycleError,
    ) as error:
        raise BatchControllerError(str(error)) from error
    except BaseException:
        raise


def run_candidate_batch(
    plan: object,
    bindings: object,
    launches: CandidateLaunchSet,
    private_root: Path,
    *,
    seed: bytes | None = None,
) -> list[dict[str, object]]:
    require_supported_isolation_platform()
    try:
        plan_value = require_mapping(plan, "candidate run plan")
        validated = validate_bindings(plan_value, bindings)
        validated_launches = validate_launch_set(launches, validated)
        plan_value = validated.plan
    except ControllerSupportError as error:
        raise BatchControllerError(str(error)) from error
    try:
        pair_seeds = reserve_batch(plan_value, Path(private_root), seed, reserve_plan)
    except ValueError as error:
        raise BatchControllerError(str(error)) from error
    try:
        return [
            _run_candidate_pair(
                plan_value,
                validated,
                validated_launches,
                Path(private_root),
                active_seed=active_seed,
            )
            for active_seed in pair_seeds
        ]
    except BatchReceiptError as error:
        raise BatchControllerError(str(error)) from error
    except BaseException:
        raise
