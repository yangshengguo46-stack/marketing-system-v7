"""Preparation, capture, normalization, and sealing for one candidate arm."""

from datetime import datetime, timezone

try:
    from .batch_controller_artifacts import materialize_neutral_binary_bytes
    from .batch_controller_capture import (
        capture,
        capture_start_failure,
        normalize,
        start,
    )
    from .batch_controller_support import (
        ArmBinding,
        ValidatedBindings,
        classify_attempt_result,
        freeze_json,
    )
    from .batch_controller_types import (
        AttemptRequest,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from .batch_isolation import AttemptCell, mark_receipts_sealed
    from .batch_plan import sha256_tree
    from .batch_receipts import seal_arm_attempt_receipt
    from .batch_receipt_storage import write_entry
    from .contracts import (
        CompiledContract,
        LabContractError,
        canonical_json_bytes,
        sha256_json,
    )
except ImportError:
    from batch_controller_artifacts import materialize_neutral_binary_bytes
    from batch_controller_capture import (
        capture,
        capture_start_failure,
        normalize,
        start,
    )
    from batch_controller_support import (
        ArmBinding,
        ValidatedBindings,
        classify_attempt_result,
        freeze_json,
    )
    from batch_controller_types import (
        AttemptRequest,
        MemoryAttemptByteSource,
        RawAttemptResult,
    )
    from batch_isolation import AttemptCell, mark_receipts_sealed
    from batch_plan import sha256_tree
    from batch_receipts import seal_arm_attempt_receipt
    from batch_receipt_storage import write_entry
    from contracts import (
        CompiledContract,
        LabContractError,
        canonical_json_bytes,
        sha256_json,
    )


def _failure_result(error: BaseException) -> RawAttemptResult:
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    metadata = {
        "costEvidence": {
            "costCny": 0,
            "sourceSha256": sha256_json(type(error).__name__),
        },
        "executorError": type(error).__name__,
        "requestCount": 0,
        "threadId": None,
        "timedOut": isinstance(error, TimeoutError),
        "supervisorTimedOut": isinstance(error, TimeoutError),
        "trajectory": None,
        "turnId": None,
        "usage": {"inputTokens": 0, "outputTokens": 0, "totalTokens": 0},
    }
    return RawAttemptResult(
        -1,
        now,
        now,
        MemoryAttemptByteSource(b"null"),
        MemoryAttemptByteSource(canonical_json_bytes(metadata)),
        MemoryAttemptByteSource(b""),
        MemoryAttemptByteSource(str(error).encode()),
    )


def prepare_arm(
    arm: ArmBinding,
    cell: AttemptCell,
    plan: dict[str, object],
    bindings: ValidatedBindings,
) -> tuple[str, AttemptRequest]:
    before = sha256_tree(cell.workspace)
    binary_path = materialize_neutral_binary_bytes(
        cell.home, arm.binary.payload, str(arm.binary_manifest["binarySha256"])
    )
    request = AttemptRequest(
        cell,
        binary_path,
        canonical_json_bytes(bindings.case_bundle),
        canonical_json_bytes(bindings.case_answer_schema),
        canonical_json_bytes(bindings.model_route),
        canonical_json_bytes(bindings.execution_profile),
        arm.effective_config.payload,
        int(plan["timeoutBudget"]),
        int(plan["tokenBudget"]),
        int(plan["requestBudget"]),
        int(plan["costBudgetCny"]),
        arm.binary.sha256,
        arm.codex_home_seed.digest,
        arm.effective_config_sha256,
        bindings.workspace_seed.digest,
        sha256_json(bindings.execution_profile),
        sha256_json(bindings.model_route),
        bindings.protocol.payload,
        bindings.protocol_sha256,
        bindings.promptfoo_config.payload,
        bindings.promptfoo_config_sha256,
        arm.codex_home_seed,
        bindings.workspace_seed,
    )
    return before, request


def start_arm(executor: object, request: AttemptRequest) -> object:
    return start(executor, request)


def capture_arm(handle: object, request: AttemptRequest) -> object:
    return capture(handle, request, _failure_result)


def capture_arm_start_failure(error: BaseException) -> object:
    return capture_start_failure(error, _failure_result)


def seal_arm(
    arm: ArmBinding,
    cell: AttemptCell,
    directory: object,
    plan: dict[str, object],
    bindings: ValidatedBindings,
    before: str,
    captured: object,
    schema: CompiledContract,
) -> tuple[dict[str, object], object, bytes]:
    raw = normalize(
        captured,
        RawAttemptResult,
        min(int(bindings.execution_profile["maxOutputBytes"]), 16 * 1024 * 1024),
    )
    try:
        output = freeze_json(raw.output)
    except (LabContractError, TypeError, ValueError):
        output = {"unserializableOutputType": type(raw.output).__name__}
    try:
        metadata = freeze_json(raw.metadata)
    except (LabContractError, TypeError, ValueError):
        metadata = {"unserializableMetadataType": type(raw.metadata).__name__}
    after = sha256_tree(cell.workspace)
    byte_fields = ("outputBytes", "metadataBytes", "stdoutBytes", "stderrBytes")
    classification, details = classify_attempt_result(
        raw.exit_code,
        output,
        metadata,
        plan,
        schema,
        started_at=raw.raw_evidence["startedAt"],
        finished_at=raw.raw_evidence["finishedAt"],
        request_count=raw.raw_evidence["requestCount"],
        artifact_sizes=tuple(
            int(raw.raw_evidence[field]["rawSize"]) for field in byte_fields
        ),
        max_output_bytes=int(bindings.execution_profile["maxOutputBytes"]),
        forced_evidence_failure=(
            raw.forced_evidence_failure
            or (
                raw.exit_code == 0
                and raw.attestation
                != {
                    "appServerProtocolSchemaSha256": bindings.protocol_sha256,
                    "binarySha256": arm.binary.sha256,
                    "codexHomeSeedSha256": arm.codex_home_seed.digest,
                    "effectiveConfigSha256": arm.effective_config_sha256,
                    "executionProfileSha256": sha256_json(bindings.execution_profile),
                    "modelRouteSha256": sha256_json(bindings.model_route),
                    "promptfooConfigSha256": bindings.promptfoo_config_sha256,
                    "workspaceSeedSha256": bindings.workspace_seed.cell_digest(),
                }
            )
        ),
    )
    raw_evidence = {
        **raw.raw_evidence,
        "normalizationFailure": raw.forced_evidence_failure,
        "normalizedFinishedAt": raw.finished_at,
        "normalizedStartedAt": raw.started_at,
    }
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
        app_server_protocol_schema_sha256=bindings.protocol_sha256,
        promptfoo_config_sha256=bindings.promptfoo_config_sha256,
        started_at=raw.started_at,
        finished_at=raw.finished_at,
        output=output,
        metadata=metadata,
        stdout=raw.stdout,
        stderr=raw.stderr,
        exit_classification=classification,
        failure_details=details,
        raw_evidence=raw_evidence,
    )
    mark_receipts_sealed(cell)
    return receipt, output, canonical_json_bytes(output)


def seal_arm_failure(
    directory: object,
    cell: AttemptCell,
    captured: object,
    max_output_bytes: int,
    error: BaseException,
) -> None:
    """Retain bounded immutable raw evidence when normal arm sealing fails."""
    raw = normalize(captured, RawAttemptResult, min(max_output_bytes, 16 * 1024 * 1024))
    try:
        output = canonical_json_bytes(raw.output) + b"\n"
    except LabContractError:
        output = (
            canonical_json_bytes({"unserializable": type(raw.output).__name__}) + b"\n"
        )
    try:
        metadata = canonical_json_bytes(raw.metadata) + b"\n"
    except LabContractError:
        metadata = (
            canonical_json_bytes({"unserializable": type(raw.metadata).__name__})
            + b"\n"
        )
    failure = (
        canonical_json_bytes(
            {
                "attemptId": cell.attempt_id,
                "errorType": type(error).__name__,
                "pairId": cell.pair_id,
                "rawEvidence": raw.raw_evidence,
                "status": "failed",
            }
        )
        + b"\n"
    )
    write_entry(directory, "failure-output.json", output)
    write_entry(directory, "failure-metadata.json", metadata)
    write_entry(directory, "failure-stdout.bin", raw.stdout)
    write_entry(directory, "failure-stderr.bin", raw.stderr)
    write_entry(directory, "failure.json", failure)
