"""Semantic reverification of sealed arm and paired-run receipts."""

import hashlib
import json
from pathlib import Path

try:
    from .batch_contracts import (
        BatchContractError,
        validate_named_contract,
        verify_self_commitment,
    )
    from .batch_controller_support import classify_attempt_result
    from .batch_receipt_offline import OfflineEvidence
    from .batch_plan import (
        BatchPlanError,
        sha256_tree,
        verify_effective_condition_parity,
    )
    from .batch_receipt_storage import (
        MAX_CONTEXT_BYTES,
        MAX_PRIVATE_FILE_BYTES,
        SecureStorageError as BatchReceiptError,
        entry_exists,
        identifier,
        load_canonical,
        private_directory,
        read_bounded,
    )
    from .contracts import (
        canonical_json_bytes,
        sha256_json,
    )
    from .private_fs import PrivateRoot
except ImportError:
    from batch_contracts import (
        BatchContractError,
        validate_named_contract,
        verify_self_commitment,
    )
    from batch_controller_support import classify_attempt_result
    from batch_receipt_offline import OfflineEvidence
    from batch_plan import BatchPlanError, sha256_tree, verify_effective_condition_parity
    from batch_receipt_storage import (
        MAX_CONTEXT_BYTES,
        MAX_PRIVATE_FILE_BYTES,
        SecureStorageError as BatchReceiptError,
        entry_exists,
        identifier,
        load_canonical,
        private_directory,
        read_bounded,
    )
    from contracts import (
        canonical_json_bytes,
        sha256_json,
    )
    from private_fs import PrivateRoot


def _json_bytes(value: object) -> bytes:
    return canonical_json_bytes(value) + b"\n"


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


def _load_pair_context(
    private_root: Path, pair_id: str, evidence: OfflineEvidence
) -> tuple[dict[str, object], dict[str, object], Path, dict[str, object]]:
    root = Path(private_root)
    plan = evidence.load_pair("plan.json", MAX_CONTEXT_BYTES)
    profile = evidence.load_pair("execution-profile.json", MAX_CONTEXT_BYTES)
    schema_path = evidence.pair.path / "case-answer-schema.json"
    identity = evidence.load_pair("identity-context.json", MAX_CONTEXT_BYTES)
    if (
        type(plan) is not dict
        or type(profile) is not dict
        or type(identity) is not dict
    ):
        raise BatchReceiptError("sealed pair context is invalid")
    try:
        validate_named_contract("candidate-run-plan", plan)
        verify_self_commitment(plan, "planSha256")
        validate_named_contract("execution-profile", profile)
        schema_bytes = evidence.read_pair("case-answer-schema.json", MAX_CONTEXT_BYTES)
        if type(json.loads(schema_bytes)) is not dict:
            raise BatchReceiptError("sealed CaseAnswer schema is invalid")
    except (
        BatchContractError,
        json.JSONDecodeError,
        OSError,
    ) as error:
        raise BatchReceiptError("sealed pair context is invalid") from error
    if _sha256_bytes(schema_bytes) != plan["caseAnswerSchemaSha256"]:
        raise BatchReceiptError("sealed CaseAnswer schema commitment mismatch")
    if sha256_json(profile) != plan["executionProfileRef"]:
        raise BatchReceiptError("sealed execution profile commitment mismatch")
    if profile["maxWallClockSeconds"] != plan["timeoutBudget"]:
        raise BatchReceiptError("sealed execution timeout mismatch")
    return plan, profile, schema_path, identity


def _verify_arm_details(
    receipt: object, private_root: Path, evidence: OfflineEvidence
) -> tuple[dict[str, object], object, str]:
    try:
        validate_named_contract("arm-attempt-receipt", receipt)
        verify_self_commitment(receipt, "receiptSha256")
    except BatchContractError as error:
        raise BatchReceiptError(f"arm receipt verification failed: {error}") from error
    assert type(receipt) is dict
    attempt_id = identifier(receipt["attemptId"], "attempt ID")
    pair_id = identifier(receipt["pairId"], "pair ID")
    plan, profile, schema_path, identity = _load_pair_context(
        private_root, pair_id, evidence
    )
    if evidence.load_attempt(attempt_id, "receipt.json") != receipt:
        raise BatchReceiptError("arm receipt differs from sealed private receipt")
    output = evidence.load_attempt(attempt_id, "output.json")
    metadata = evidence.load_attempt(attempt_id, "metadata.json")
    envelope = evidence.load_attempt(attempt_id, "evidence.json")
    if type(envelope) is not dict:
        raise BatchReceiptError("private evidence envelope is invalid")
    limit = min(int(profile["maxOutputBytes"]), MAX_PRIVATE_FILE_BYTES)
    stdout = evidence.read_attempt(attempt_id, "stdout.bin", limit)
    stderr = evidence.read_attempt(attempt_id, "stderr.bin", limit)
    expected = {
        "metadataSha256": _sha256_bytes(_json_bytes(metadata)),
        "stderrSha256": _sha256_bytes(stderr),
        "stdoutSha256": _sha256_bytes(stdout),
        "trajectory": _metadata_parts(metadata)[2],
        "workspacePath": envelope.get("workspacePath"),
        "rawEvidence": envelope.get("rawEvidence"),
    }
    if envelope != expected or sha256_json(envelope) != receipt["trajectorySha256"]:
        raise BatchReceiptError("evidence commitment mismatch")
    if sha256_json(output) != receipt["outputSha256"]:
        raise BatchReceiptError("evidence commitment mismatch")
    thread_id, turn_id, _, usage, cost = _metadata_parts(metadata)
    if (
        sha256_json(thread_id) != receipt["threadIdCommitment"]
        or sha256_json(turn_id) != receipt["turnIdCommitment"]
        or usage != receipt["usage"]
        or cost != receipt["costEvidence"]
    ):
        raise BatchReceiptError("arm receipt semantic evidence mismatch")
    raw = envelope.get("rawEvidence")
    byte_fields = ("outputBytes", "metadataBytes", "stdoutBytes", "stderrBytes")
    if type(raw) is not dict or any(
        type(raw.get(field)) is not dict for field in byte_fields
    ):
        raise BatchReceiptError("raw attempt byte evidence is invalid")
    if (
        raw["outputBytes"].get("storedSha256") != _sha256_bytes(_json_bytes(output))
        or raw["metadataBytes"].get("storedSha256")
        != _sha256_bytes(_json_bytes(metadata))
        or raw["stdoutBytes"].get("storedSha256") != _sha256_bytes(stdout)
        or raw["stderrBytes"].get("storedSha256") != _sha256_bytes(stderr)
    ):
        raise BatchReceiptError("raw attempt byte evidence mismatch")
    arms = {name: identity.get(name) for name in ("stock", "modified")}
    arm_class = next(
        (
            name
            for name, expected_arm in arms.items()
            if type(expected_arm) is dict
            and expected_arm.get("privateArmId") == receipt["privateArmId"]
        ),
        None,
    )
    if arm_class is None:
        raise BatchReceiptError("arm identity is absent from sealed context")
    expected_arm = arms[arm_class]
    assert type(expected_arm) is dict
    expected_fields = {
        "treatmentManifestSha256": expected_arm.get("treatmentManifestSha256"),
        "binaryManifestSha256": expected_arm.get("binaryManifestSha256"),
        "effectiveConfigSha256": expected_arm.get("effectiveConfigSha256"),
        "inputSha256": identity.get("inputSha256"),
        "workspaceBeforeSha256": identity.get("workspaceBeforeSha256"),
        "appServerProtocolSchemaSha256": identity.get("appServerProtocolSchemaSha256"),
        "promptfooConfigSha256": identity.get("promptfooConfigSha256"),
    }
    if any(
        receipt.get(field) != expected for field, expected in expected_fields.items()
    ):
        raise BatchReceiptError("arm identity differs from sealed context")
    expected_attestation = {
        "appServerProtocolSchemaSha256": identity.get("appServerProtocolSchemaSha256"),
        "binarySha256": expected_arm.get("binarySha256"),
        "codexHomeSeedSha256": expected_arm.get("codexHomeSeedSha256"),
        "effectiveConfigSha256": expected_arm.get("effectiveConfigSha256"),
        "executionProfileSha256": identity.get("executionProfileSha256"),
        "modelRouteSha256": identity.get("modelRouteSha256"),
        "promptfooConfigSha256": identity.get("promptfooConfigSha256"),
        "workspaceSeedSha256": identity.get("workspaceBeforeSha256"),
    }
    if (
        receipt["exitClassification"] == "completed"
        and raw.get("attestation") != expected_attestation
    ) or (
        raw.get("attestation") is not None
        and raw.get("attestation") != expected_attestation
    ):
        raise BatchReceiptError("executor attestation differs from sealed context")
    try:
        artifact_sizes = tuple(
            int(raw[field].get("rawSize", -1)) for field in byte_fields
        )
    except (TypeError, ValueError) as error:
        raise BatchReceiptError("raw attempt byte sizes are invalid") from error
    classification, details = classify_attempt_result(
        raw.get("exitCode"),
        output,
        metadata,
        plan,
        schema_path,
        started_at=raw.get("startedAt"),
        finished_at=raw.get("finishedAt"),
        request_count=raw.get("requestCount"),
        artifact_sizes=artifact_sizes,
        max_output_bytes=int(profile["maxOutputBytes"]),
        forced_evidence_failure=raw.get("normalizationFailure") is True,
    )
    if (
        receipt["startedAt"] != raw.get("normalizedStartedAt", raw.get("startedAt"))
        or receipt["finishedAt"]
        != raw.get("normalizedFinishedAt", raw.get("finishedAt"))
        or receipt["exitClassification"] != classification
        or receipt["failureDetails"] != details
    ):
        raise BatchReceiptError("arm receipt classification is false")
    try:
        workspace_digest = sha256_tree(Path(str(envelope["workspacePath"])))
    except BatchPlanError as error:
        raise BatchReceiptError("sealed workspace cannot be verified") from error
    if workspace_digest != receipt["workspaceAfterSha256"]:
        raise BatchReceiptError("evidence commitment mismatch")
    return receipt, output, arm_class


def verify_arm_attempt_receipt(receipt: object, private_root: Path) -> None:
    """Reverify an arm receipt and every private artifact it commits."""
    if type(receipt) is not dict:
        raise BatchReceiptError("arm receipt must be an object")
    evidence = OfflineEvidence(private_root, identifier(receipt.get("pairId"), "pair ID"))
    try:
        _verify_arm_details(receipt, private_root, evidence)
        evidence.verify()
    finally:
        evidence.close()


def _verify_pair_with_evidence(
    receipt: object, private_root: Path, evidence: OfflineEvidence
) -> None:
    """Reverify pair semantics, its two distinct arms, and their raw evidence."""
    try:
        validate_named_contract("paired-run-receipt", receipt)
        verify_self_commitment(receipt, "receiptSha256")
    except BatchContractError as error:
        raise BatchReceiptError(f"pair receipt verification failed: {error}") from error
    assert type(receipt) is dict
    pair_id = identifier(receipt["pairId"], "pair ID")
    plan, _, _, identity = _load_pair_context(private_root, pair_id, evidence)
    if receipt["planSha256"] != plan["planSha256"]:
        raise BatchReceiptError("pair plan commitment mismatch")
    if evidence.load_pair("receipt.json") != receipt:
        raise BatchReceiptError("pair receipt differs from sealed private receipt")
    order = evidence.load_pair("order.json")
    layout_authority = evidence.layout_authority
    if (
        type(order) is not dict
        or order.get("pairId") != pair_id
        or type(order.get("attemptOrder")) is not list
        or len(order["attemptOrder"]) != 2
        or len(set(order["attemptOrder"])) != 2
        or order.get("layoutIdentitySha256")
        != layout_authority.get("layoutSha256")
        or sha256_json(order) != receipt["orderRandomizationCommitment"]
    ):
        raise BatchReceiptError("pair order commitment mismatch")
    try:
        parity = verify_effective_condition_parity(
            identity["stock"]["effectiveConditions"],
            identity["modified"]["effectiveConditions"],
        )
    except (BatchPlanError, KeyError, TypeError) as error:
        raise BatchReceiptError("sealed condition parity is invalid") from error
    if receipt["conditionParitySha256"] != parity:
        raise BatchReceiptError("pair condition parity commitment mismatch")
    has_mapping = evidence.has_pair("mapping.json")
    if "anonymousMappingCommitment" in receipt:
        mapping = evidence.load_pair("mapping.json")
        if sha256_json(mapping) != receipt["anonymousMappingCommitment"]:
            raise BatchReceiptError("anonymous mapping commitment mismatch")
    elif has_mapping:
        raise BatchReceiptError("identical or invalid pair has a mapping artifact")
    arm_values = []
    for attempt_id in order["attemptOrder"]:
        attempt = identifier(attempt_id, "attempt ID")
        arm = evidence.load_attempt(attempt, "receipt.json")
        if type(arm) is not dict or arm.get("attemptId") != attempt:
            raise BatchReceiptError("pair arm receipt is invalid")
        arm_values.append(_verify_arm_details(arm, private_root, evidence))
    by_commitment = {
        arm["receiptSha256"]: (arm, output, arm_class)
        for arm, output, arm_class in arm_values
    }
    stock_hash = receipt["stockArmAttemptReceiptSha256"]
    modified_hash = receipt["modifiedArmAttemptReceiptSha256"]
    if stock_hash == modified_hash or set(by_commitment) != {stock_hash, modified_hash}:
        raise BatchReceiptError("pair must commit two distinct arm receipts")
    stock, stock_output, stock_class = by_commitment[stock_hash]
    modified, modified_output, modified_class = by_commitment[modified_hash]
    if (
        stock["pairId"] != pair_id
        or modified["pairId"] != pair_id
        or stock["privateArmId"] == modified["privateArmId"]
        or stock_class != "stock"
        or modified_class != "modified"
    ):
        raise BatchReceiptError("pair arm identity is false")
    failures = [
        arm["exitClassification"]
        for arm in (stock, modified)
        if arm["exitClassification"] != "completed"
    ]
    if receipt["pairValidity"] != ("invalid" if failures else "valid") or receipt[
        "invalidReason"
    ] != (failures[0] if failures else None):
        raise BatchReceiptError("pair validity is false")
    if not failures:
        relation = (
            "canonicallyIdentical"
            if canonical_json_bytes(stock_output)
            == canonical_json_bytes(modified_output)
            else "distinct"
        )
        if receipt.get("outputRelation") != relation:
            raise BatchReceiptError("pair output relation is false")
        if relation == "distinct":
            mapping = evidence.load_pair("mapping.json")
            labels = (
                mapping.get("labelToPrivateArmId") if type(mapping) is dict else None
            )
            if type(labels) is not dict or set(labels.values()) != {
                stock["privateArmId"],
                modified["privateArmId"],
            }:
                raise BatchReceiptError("anonymous arm mapping is false")


def verify_paired_run_receipt(receipt: object, private_root: Path) -> None:
    """Reverify a pair through one descriptor-retained offline layout."""
    if type(receipt) is not dict:
        raise BatchReceiptError("pair receipt must be an object")
    evidence = OfflineEvidence(private_root, identifier(receipt.get("pairId"), "pair ID"))
    try:
        _verify_pair_with_evidence(receipt, private_root, evidence)
        evidence.verify()
    finally:
        evidence.close()
