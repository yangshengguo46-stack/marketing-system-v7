"""Private evidence sealing and public 07B receipt verification."""

import hashlib
import os
import re
import stat
from pathlib import Path

try:
    from .batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
        verify_self_commitment,
    )
    from .batch_plan import BatchPlanError, sha256_tree
    from .contracts import LabContractError, canonical_json_bytes, load_exact_json, sha256_json
except ImportError:
    from batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        validate_named_contract,
        verify_self_commitment,
    )
    from batch_plan import BatchPlanError, sha256_tree
    from contracts import LabContractError, canonical_json_bytes, load_exact_json, sha256_json


_IDENTIFIER = re.compile(r"[0-9a-f]{64}\Z")


class BatchReceiptError(ValueError):
    pass


def _identifier(value: object, name: str) -> str:
    if type(value) is not str or _IDENTIFIER.fullmatch(value) is None:
        raise BatchReceiptError(f"invalid {name}")
    return value


def _private_directory(path: Path, *, create: bool = False) -> Path:
    target = Path(path)
    if not target.is_absolute():
        raise BatchReceiptError("private root must be absolute")
    if create:
        target.mkdir(parents=True, mode=0o700, exist_ok=True)
    try:
        metadata = target.lstat()
    except OSError as error:
        raise BatchReceiptError("private evidence directory is unavailable") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        raise BatchReceiptError("private evidence directory must not be a link")
    return target


def _child_directory(parent: Path, name: str, *, exclusive: bool) -> Path:
    target = parent / name
    try:
        target.mkdir(mode=0o700, exist_ok=not exclusive)
    except FileExistsError as error:
        raise BatchReceiptError(f"{name} already exists") from error
    return _private_directory(target)


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
    if pair_path.exists():
        raise BatchReceiptError("pair already exists")
    for attempt in attempts:
        if (attempts_root / attempt).exists():
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
        attempts = (_identifier(left_id, "attempt ID"), _identifier(right_id, "attempt ID"))
        if (root / "pairs" / pair).exists():
            raise BatchReceiptError("pair already exists")
        if any((root / "attempts" / attempt).exists() for attempt in attempts):
            raise BatchReceiptError("attempt already exists")


def _write_exclusive(path: Path, payload: bytes) -> None:
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0)
    try:
        descriptor = os.open(path, flags, 0o600)
    except OSError as error:
        raise BatchReceiptError(f"private evidence write collision: {path.name}") from error
    try:
        view = memoryview(payload)
        while view:
            written = os.write(descriptor, view)
            if written <= 0:
                raise BatchReceiptError("private evidence write failed")
            view = view[written:]
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


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


def _load_canonical(path: Path) -> object:
    try:
        payload = path.read_bytes()
        value = load_exact_json(path)
    except (OSError, LabContractError) as error:
        raise BatchReceiptError("private evidence is unavailable or invalid") from error
    if payload != _json_bytes(value):
        raise BatchReceiptError("private evidence bytes are not canonical")
    return value


def verify_arm_attempt_receipt(receipt: object, private_root: Path) -> None:
    """Reverify an arm receipt and every private artifact it commits."""
    try:
        validate_named_contract("arm-attempt-receipt", receipt)
        verify_self_commitment(receipt, "receiptSha256")
    except BatchContractError as error:
        raise BatchReceiptError(f"arm receipt verification failed: {error}") from error
    assert type(receipt) is dict
    attempt_id = _identifier(receipt["attemptId"], "attempt ID")
    directory = _private_directory(Path(private_root) / "attempts" / attempt_id)
    stored = _load_canonical(directory / "receipt.json")
    if stored != receipt:
        raise BatchReceiptError("arm receipt differs from sealed private receipt")
    output = _load_canonical(directory / "output.json")
    metadata = _load_canonical(directory / "metadata.json")
    evidence = _load_canonical(directory / "evidence.json")
    if type(evidence) is not dict:
        raise BatchReceiptError("private evidence envelope is invalid")
    try:
        stdout = (directory / "stdout.bin").read_bytes()
        stderr = (directory / "stderr.bin").read_bytes()
    except OSError as error:
        raise BatchReceiptError("private attempt stream is unavailable") from error
    expected = {
        "metadataSha256": _sha256_bytes(_json_bytes(metadata)),
        "stderrSha256": _sha256_bytes(stderr),
        "stdoutSha256": _sha256_bytes(stdout),
        "trajectory": _metadata_parts(metadata)[2],
        "workspacePath": evidence.get("workspacePath"),
    }
    if evidence != expected or sha256_json(evidence) != receipt["trajectorySha256"]:
        raise BatchReceiptError("evidence commitment mismatch")
    if sha256_json(output) != receipt["outputSha256"]:
        raise BatchReceiptError("evidence commitment mismatch")
    workspace = Path(str(evidence["workspacePath"]))
    try:
        workspace_digest = sha256_tree(workspace)
    except BatchPlanError as error:
        raise BatchReceiptError("sealed workspace cannot be verified") from error
    if workspace_digest != receipt["workspaceAfterSha256"]:
        raise BatchReceiptError("evidence commitment mismatch")


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


def _receipt_by_commitment(private_root: Path, pair_id: str, commitment: object) -> dict[str, object]:
    matches: list[dict[str, object]] = []
    attempts_root = _private_directory(Path(private_root) / "attempts")
    for directory in attempts_root.iterdir():
        if not directory.is_dir() or directory.is_symlink():
            raise BatchReceiptError("unexpected private attempt entry")
        value = _load_canonical(directory / "receipt.json")
        if type(value) is dict and value.get("receiptSha256") == commitment:
            matches.append(value)
    if len(matches) != 1 or matches[0].get("pairId") != pair_id:
        raise BatchReceiptError("pair arm receipt commitment is unavailable or ambiguous")
    return matches[0]


def verify_paired_run_receipt(receipt: object, private_root: Path) -> None:
    """Reverify a pair receipt, both arm receipts, and all committed private evidence."""
    try:
        validate_named_contract("paired-run-receipt", receipt)
        verify_self_commitment(receipt, "receiptSha256")
    except BatchContractError as error:
        raise BatchReceiptError(f"pair receipt verification failed: {error}") from error
    assert type(receipt) is dict
    pair_id = _identifier(receipt["pairId"], "pair ID")
    directory = _private_directory(Path(private_root) / "pairs" / pair_id)
    if _load_canonical(directory / "receipt.json") != receipt:
        raise BatchReceiptError("pair receipt differs from sealed private receipt")
    order = _load_canonical(directory / "order.json")
    if sha256_json(order) != receipt["orderRandomizationCommitment"]:
        raise BatchReceiptError("pair order commitment mismatch")
    mapping_path = directory / "mapping.json"
    if "anonymousMappingCommitment" in receipt:
        mapping = _load_canonical(mapping_path)
        if sha256_json(mapping) != receipt["anonymousMappingCommitment"]:
            raise BatchReceiptError("anonymous mapping commitment mismatch")
    elif mapping_path.exists():
        raise BatchReceiptError("identical or invalid pair has a mapping artifact")
    for field in ("stockArmAttemptReceiptSha256", "modifiedArmAttemptReceiptSha256"):
        arm_receipt = _receipt_by_commitment(private_root, pair_id, receipt[field])
        verify_arm_attempt_receipt(arm_receipt, private_root)
