"""Private evidence sealing and public 07B receipt verification."""

import hashlib
from pathlib import Path

try:
    from .batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
    )
    from .batch_receipt_verification import (
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from .batch_receipt_storage import (
        MAX_CONTEXT_BYTES as _MAX_CONTEXT_BYTES,
        SecureStorageError as BatchReceiptError,
        child_directory as _child_directory,
        entry_exists as _entry_exists,
        identifier as _identifier,
        private_directory as _private_directory,
        write_exclusive as _write_exclusive,
    )
    from .contracts import (
        LabContractError,
        canonical_json_bytes,
        sha256_json,
    )
except ImportError:
    from batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
    )
    from batch_receipt_verification import (
        verify_arm_attempt_receipt,
        verify_paired_run_receipt,
    )
    from batch_receipt_storage import (
        MAX_CONTEXT_BYTES as _MAX_CONTEXT_BYTES,
        SecureStorageError as BatchReceiptError,
        child_directory as _child_directory,
        entry_exists as _entry_exists,
        identifier as _identifier,
        private_directory as _private_directory,
        write_exclusive as _write_exclusive,
    )
    from contracts import (
        LabContractError,
        canonical_json_bytes,
        sha256_json,
    )


def prepare_private_layout(
    private_root: Path, pair_id: str, attempt_ids: tuple[str, str]
) -> tuple[Path, tuple[Path, Path]]:
    """Reserve collision-free pair and attempt evidence directories before execution."""
    pair = _identifier(pair_id, "pair ID")
    attempts = tuple(_identifier(value, "attempt ID") for value in attempt_ids)
    root = _private_directory(Path(private_root), create=True)
    pairs_root = _child_directory(root, "pairs", exclusive=False)
    attempts_root = _child_directory(root, "attempts", exclusive=False)
    pair_path = pairs_root / pair
    if _entry_exists(pairs_root, pair):
        raise BatchReceiptError("pair already exists")
    for attempt in attempts:
        if _entry_exists(attempts_root, attempt):
            raise BatchReceiptError("attempt already exists")
    pair_path = _child_directory(pairs_root, pair, exclusive=True)
    created: list[Path] = []
    try:
        for attempt in attempts:
            created.append(_child_directory(attempts_root, attempt, exclusive=True))
    except BaseException:
        for target in reversed(created):
            target.rmdir()
        pair_path.rmdir()
        raise
    return pair_path, (created[0], created[1])


def assert_private_layout_available(
    private_root: Path, identities: tuple[tuple[str, str, str], ...]
) -> None:
    """Preflight all batch identities without creating any evidence directory."""
    root = Path(private_root)
    if root.exists():
        _private_directory(root)
    for pair_id, left_id, right_id in identities:
        pair = _identifier(pair_id, "pair ID")
        attempts = (
            _identifier(left_id, "attempt ID"),
            _identifier(right_id, "attempt ID"),
        )
        pairs = root / "pairs"
        attempts_root = root / "attempts"
        if pairs.exists() and _entry_exists(pairs, pair):
            raise BatchReceiptError("pair already exists")
        if attempts_root.exists() and any(
            _entry_exists(attempts_root, attempt) for attempt in attempts
        ):
            raise BatchReceiptError("attempt already exists")


def _json_bytes(value: object) -> bytes:
    try:
        return canonical_json_bytes(value) + b"\n"
    except LabContractError as error:
        raise BatchReceiptError("private evidence is not canonical JSON") from error


def _sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _metadata_parts(metadata: object) -> tuple[object, object, object, object, object]:
    if type(metadata) is not dict:
        return None, None, None, None, None
    return (
        metadata.get("threadId"),
        metadata.get("turnId"),
        metadata.get("trajectory"),
        metadata.get("usage"),
        metadata.get("costEvidence"),
    )


def seal_pair_context(
    pair_directory: Path,
    *,
    plan: dict[str, object],
    execution_profile: dict[str, object],
    case_answer_schema_path: Path,
) -> None:
    """Seal the immutable semantic inputs needed for offline receipt verification."""
    directory = _private_directory(Path(pair_directory))
    try:
        schema_bytes = Path(case_answer_schema_path).read_bytes()
    except OSError as error:
        raise BatchReceiptError("CaseAnswer schema is unavailable") from error
    if len(schema_bytes) > _MAX_CONTEXT_BYTES:
        raise BatchReceiptError("CaseAnswer schema exceeds the context bound")
    _write_exclusive(directory / "plan.json", _json_bytes(plan))
    _write_exclusive(directory / "execution-profile.json", _json_bytes(execution_profile))
    _write_exclusive(directory / "case-answer-schema.json", schema_bytes)


def seal_arm_attempt_receipt(
    attempt_directory: Path,
    *,
    attempt_id: str,
    pair_id: str,
    private_arm_id: str,
    treatment_manifest_sha256: str,
    binary_manifest_sha256: str,
    effective_config_sha256: str,
    input_sha256: str,
    workspace_before_sha256: str,
    workspace_after_sha256: str,
    workspace_path: Path,
    app_server_protocol_schema_sha256: str,
    promptfoo_config_sha256: str,
    started_at: str,
    finished_at: str,
    output: object,
    metadata: object,
    stdout: bytes,
    stderr: bytes,
    exit_classification: str,
    failure_details: str | None,
    raw_evidence: dict[str, object],
) -> dict[str, object]:
    """Seal raw private evidence, then seal its schema-valid arm receipt."""
    directory = _private_directory(Path(attempt_directory))
    output_bytes, metadata_bytes = _json_bytes(output), _json_bytes(metadata)
    _write_exclusive(directory / "output.json", output_bytes)
    _write_exclusive(directory / "metadata.json", metadata_bytes)
    _write_exclusive(directory / "stdout.bin", stdout)
    _write_exclusive(directory / "stderr.bin", stderr)
    thread_id, turn_id, trajectory, usage, cost_evidence = _metadata_parts(metadata)
    evidence = {
        "metadataSha256": _sha256_bytes(metadata_bytes),
        "stderrSha256": _sha256_bytes(stderr),
        "stdoutSha256": _sha256_bytes(stdout),
        "trajectory": trajectory,
        "workspacePath": str(Path(workspace_path)),
        "rawEvidence": raw_evidence,
    }
    evidence_bytes = _json_bytes(evidence)
    _write_exclusive(directory / "evidence.json", evidence_bytes)
    if type(usage) is not dict:
        usage = {"inputTokens": 0, "outputTokens": 0, "totalTokens": 0}
    if type(cost_evidence) is not dict:
        cost_evidence = {"costCny": 0, "sourceSha256": sha256_json(metadata)}
    receipt: dict[str, object] = {
        "schemaVersion": 1,
        "attemptId": attempt_id,
        "pairId": pair_id,
        "privateArmId": private_arm_id,
        "treatmentManifestSha256": treatment_manifest_sha256,
        "binaryManifestSha256": binary_manifest_sha256,
        "effectiveConfigSha256": effective_config_sha256,
        "inputSha256": input_sha256,
        "workspaceBeforeSha256": workspace_before_sha256,
        "workspaceAfterSha256": workspace_after_sha256,
        "appServerProtocolSchemaSha256": app_server_protocol_schema_sha256,
        "promptfooConfigSha256": promptfoo_config_sha256,
        "startedAt": started_at,
        "finishedAt": finished_at,
        "threadIdCommitment": sha256_json(thread_id),
        "turnIdCommitment": sha256_json(turn_id),
        "trajectorySha256": sha256_json(evidence),
        "outputSha256": sha256_json(output),
        "usage": usage,
        "costEvidence": cost_evidence,
        "exitClassification": exit_classification,
        "failureDetails": failure_details,
        "receiptSha256": "0" * 64,
    }
    try:
        receipt = seal_self_commitment(receipt, "receiptSha256")
        validate_named_contract("arm-attempt-receipt", receipt)
    except BatchContractError as error:
        raise BatchReceiptError(f"arm receipt is invalid: {error}") from error
    _write_exclusive(directory / "receipt.json", _json_bytes(receipt))
    return receipt

def seal_paired_run_receipt(
    pair_directory: Path,
    *,
    pair_id: str,
    plan_sha256: str,
    stock_receipt: dict[str, object],
    modified_receipt: dict[str, object],
    condition_parity_sha256: str,
    order_record: dict[str, object],
    pair_validity: str,
    invalid_reason: str | None,
    output_relation: str | None,
    mapping_record: dict[str, object] | None,
) -> dict[str, object]:
    """Seal private order/mapping evidence, then the public pair receipt."""
    directory = _private_directory(Path(pair_directory))
    _write_exclusive(directory / "order.json", _json_bytes(order_record))
    receipt: dict[str, object] = {
        "schemaVersion": 1,
        "pairId": pair_id,
        "planSha256": plan_sha256,
        "stockArmAttemptReceiptSha256": stock_receipt["receiptSha256"],
        "modifiedArmAttemptReceiptSha256": modified_receipt["receiptSha256"],
        "conditionParitySha256": condition_parity_sha256,
        "orderRandomizationCommitment": sha256_json(order_record),
        "pairValidity": pair_validity,
        "invalidReason": invalid_reason,
        "receiptSha256": "0" * 64,
    }
    if output_relation is not None:
        receipt["outputRelation"] = output_relation
    if mapping_record is not None:
        _write_exclusive(directory / "mapping.json", _json_bytes(mapping_record))
        receipt["anonymousMappingCommitment"] = sha256_json(mapping_record)
    try:
        receipt = seal_self_commitment(receipt, "receiptSha256")
        validate_named_contract("paired-run-receipt", receipt)
    except BatchContractError as error:
        raise BatchReceiptError(f"pair receipt is invalid: {error}") from error
    _write_exclusive(directory / "receipt.json", _json_bytes(receipt))
    return receipt
