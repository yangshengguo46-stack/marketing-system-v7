"""Preparation, capture, normalization, and sealing for one candidate arm."""

from datetime import datetime, timezone
from pathlib import Path

try:
    from .batch_controller_artifacts import materialize_neutral_binary
    from .batch_controller_capture import capture, normalize
    from .batch_controller_support import (
        ArmBinding,
        ValidatedBindings,
        classify_attempt_result,
        freeze_json,
    )
    from .batch_controller_types import AttemptRequest, RawAttemptResult
    from .batch_isolation import AttemptCell, mark_receipts_sealed
    from .batch_plan import sha256_tree
    from .batch_receipts import seal_arm_attempt_receipt
    from .batch_receipt_storage import write_exclusive
    from .contracts import LabContractError, canonical_json_bytes, sha256_json
except ImportError:
    from batch_controller_artifacts import materialize_neutral_binary
    from batch_controller_capture import capture, normalize
    from batch_controller_support import (
        ArmBinding,
        ValidatedBindings,
        classify_attempt_result,
        freeze_json,
    )
    from batch_controller_types import AttemptRequest, RawAttemptResult
    from batch_isolation import AttemptCell, mark_receipts_sealed
    from batch_plan import sha256_tree
    from batch_receipts import seal_arm_attempt_receipt
    from batch_receipt_storage import write_exclusive
    from contracts import LabContractError, canonical_json_bytes, sha256_json


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
        "trajectory": None,
        "turnId": None,
        "usage": {"inputTokens": 0, "outputTokens": 0, "totalTokens": 0},
    }
    return RawAttemptResult(-1, now, now, None, metadata, b"", str(error).encode())


def prepare_arm(
    arm: ArmBinding,
    cell: AttemptCell,
    plan: dict[str, object],
    bindings: ValidatedBindings,
) -> tuple[str, AttemptRequest]:
    before = sha256_tree(cell.workspace)
    binary_path = materialize_neutral_binary(
        cell.home, arm.binary_path, str(arm.binary_manifest["binarySha256"])
    )
    request = AttemptRequest(
        cell,
        binary_path,
        freeze_json(bindings.case_bundle),
        freeze_json(bindings.case_answer_schema),
        dict(bindings.model_route),
        freeze_json(bindings.execution_profile),
        int(plan["timeoutBudget"]),
        int(plan["tokenBudget"]),
        int(plan["requestBudget"]),
        int(plan["costBudgetCny"]),
    )
    return before, request


def capture_arm(executor: object, request: AttemptRequest) -> object:
    return capture(executor, request, _failure_result)


def seal_arm(
    arm: ArmBinding,
    cell: AttemptCell,
    directory: Path,
    plan: dict[str, object],
    bindings: ValidatedBindings,
    before: str,
    captured: object,
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
        bindings.case_answer_schema_path,
        started_at=raw.raw_evidence["startedAt"],
        finished_at=raw.raw_evidence["finishedAt"],
        request_count=raw.raw_evidence["requestCount"],
        artifact_sizes=tuple(
            int(raw.raw_evidence[field]["rawSize"]) for field in byte_fields
        ),
        max_output_bytes=int(bindings.execution_profile["maxOutputBytes"]),
        forced_evidence_failure=raw.forced_evidence_failure,
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
        workspace_path=cell.workspace,
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
    directory: Path,
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
        output = canonical_json_bytes({"unserializable": type(raw.output).__name__}) + b"\n"
    try:
        metadata = canonical_json_bytes(raw.metadata) + b"\n"
    except LabContractError:
        metadata = canonical_json_bytes({"unserializable": type(raw.metadata).__name__}) + b"\n"
    failure = canonical_json_bytes(
        {
            "attemptId": cell.attempt_id,
            "errorType": type(error).__name__,
            "pairId": cell.pair_id,
            "rawEvidence": raw.raw_evidence,
            "status": "failed",
        }
    ) + b"\n"
    write_exclusive(directory / "failure-output.json", output)
    write_exclusive(directory / "failure-metadata.json", metadata)
    write_exclusive(directory / "failure-stdout.bin", raw.stdout)
    write_exclusive(directory / "failure-stderr.bin", raw.stderr)
    write_exclusive(directory / "failure.json", failure)
