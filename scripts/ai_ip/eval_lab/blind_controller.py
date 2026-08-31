"""Create diagnostic-only, position-swapped blind reviewer bundles."""

import hashlib
import os
import re
import secrets
import stat
from collections.abc import Callable
from datetime import datetime, timezone
from pathlib import Path

if __package__:
    from .blind_artifacts import (
        BlindArtifactError,
        _reject_candidate_provenance,
        _stage_and_publish,
        _validate_arm_key,
        _validate_assignment_mapping,
    )
    from .contracts import (
        BlindPackReceipt,
        LabContractError,
        _load_exact_json_bytes,
        canonical_json_bytes,
        sha256_json,
        validate_contract,
    )
    from .private_fs import _DIRECTORY_FLAGS, PrivateRoot, _open_checked_at
else:
    from blind_artifacts import (
        BlindArtifactError,
        _reject_candidate_provenance,
        _stage_and_publish,
        _validate_arm_key,
        _validate_assignment_mapping,
    )
    from contracts import (
        BlindPackReceipt,
        LabContractError,
        _load_exact_json_bytes,
        canonical_json_bytes,
        sha256_json,
        validate_contract,
    )
    from private_fs import _DIRECTORY_FLAGS, PrivateRoot, _open_checked_at


class BlindControllerError(ValueError):
    """Raised when a blind batch cannot be authorized or published."""


_LAB_ROOT = Path(__file__).resolve().parents[3] / "ai-ip-evals" / "lab"
_CASE_SCHEMA = _LAB_ROOT / "schemas" / "case-bundle.schema.json"
_ANSWER_SCHEMA = _LAB_ROOT / "schemas" / "case-answer.schema.json"
_REVIEWER_SCHEMA = _LAB_ROOT / "schemas" / "reviewer.schema.json"
_BATCH_SCHEMA = _LAB_ROOT / "schemas" / "batch.schema.json"
_BLIND_SCHEMA = _LAB_ROOT / "schemas" / "blind-review.schema.json"
_FROZEN_RUBRIC = _LAB_ROOT / "rubrics" / "golden-gift-l1-l2-rubric.json"
_REQUIRED_DOMAINS = {"businessIpJudgment", "evidenceIntegrity"}
_COMPONENT = re.compile(r"[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?\Z")
_RFC3339 = re.compile(
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?"
    r"(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)\Z"
)
_NOFOLLOW = getattr(os, "O_NOFOLLOW", 0)
_CLOEXEC = getattr(os, "O_CLOEXEC", 0)
_POSITION_DOMAIN = b"ai-ip-eval-public-position-v1\0"
_MAPPING_DOMAIN = b"ai-ip-eval-hidden-arm-mapping-v1\0"


def _component(value: object, label: str) -> str:
    if type(value) is not str or not _COMPONENT.fullmatch(value):
        raise BlindControllerError(f"{label} must be a safe non-empty path component")
    return value


def _load_regular_json(path: Path, label: str) -> object:
    candidate = Path(path)
    descriptor = None
    try:
        before = candidate.lstat()
        if stat.S_ISLNK(before.st_mode) or not stat.S_ISREG(before.st_mode):
            raise BlindControllerError(f"{label} must be a regular non-symlink file")
        descriptor = os.open(candidate, os.O_RDONLY | _NOFOLLOW | _CLOEXEC)
        opened = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino) != (before.st_dev, before.st_ino):
            raise BlindControllerError(f"{label} changed while opening")
        chunks = []
        while chunk := os.read(descriptor, 1024 * 1024):
            chunks.append(chunk)
        after = os.fstat(descriptor)
        if (opened.st_dev, opened.st_ino, opened.st_size) != (
            after.st_dev,
            after.st_ino,
            after.st_size,
        ):
            raise BlindControllerError(f"{label} changed while reading")
        payload = b"".join(chunks)
        return _load_exact_json_bytes(payload)
    except BlindControllerError:
        raise
    except (LabContractError, OSError) as error:
        raise BlindControllerError(f"cannot safely load {label}") from error
    finally:
        if descriptor is not None:
            os.close(descriptor)


def _validated(value: object, schema: Path, label: str) -> dict[str, object]:
    try:
        validate_contract(value, schema)
    except LabContractError as error:
        raise BlindControllerError(f"invalid {label}") from error
    if type(value) is not dict:
        raise BlindControllerError(f"invalid {label}")
    return value


def _private_case_receipt(
    private_root: PrivateRoot, path: Path
) -> tuple[dict[str, object], str]:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise BlindControllerError("case receipt path must be absolute")
    try:
        relative = candidate.relative_to(private_root.path)
        value = private_root.read_json(relative)
    except (LabContractError, OSError, ValueError) as error:
        raise BlindControllerError(
            "case receipt is outside the trusted private root"
        ) from error
    return _validated(value, _CASE_SCHEMA, "case receipt"), relative.as_posix()


def _parse_time(value: object, label: str) -> datetime:
    if type(value) is not str or not _RFC3339.fullmatch(value):
        raise BlindControllerError(f"{label} must be an RFC3339 timestamp")
    try:
        wire_value = value[:-1] + "+00:00" if value.endswith("Z") else value
        parsed = datetime.fromisoformat(wire_value)
    except ValueError as error:
        raise BlindControllerError(f"{label} must be an RFC3339 timestamp") from error
    return parsed.astimezone(timezone.utc)


def _qualification(path: Path, analysis_time: datetime) -> dict[str, object]:
    value = _load_regular_json(path, "qualification receipt")
    receipt = _validated(value, _REVIEWER_SCHEMA, "qualification receipt")
    if receipt.get("objectKind") != "QualificationReceipt":
        raise BlindControllerError("reviewer authority must be a qualification receipt")
    if (
        receipt.get("diagnosticOnly") is not True
        or receipt.get("status") != "qualified"
    ):
        raise BlindControllerError("reviewer is not qualified")
    domains = receipt.get("qualifiedDomains")
    if type(domains) is not list or not _REQUIRED_DOMAINS.issubset(set(domains)):
        raise BlindControllerError("reviewer lacks required qualification domains")
    if analysis_time >= _parse_time(receipt.get("expiresAt"), "expiresAt"):
        raise BlindControllerError("reviewer qualification is expired")
    _component(receipt.get("reviewerId"), "reviewerId")
    return receipt


def _batch_exists(private_root: PrivateRoot, batch_id: str) -> bool:
    root_fd = private_root._open_root()
    batches_fd = None
    try:
        try:
            batches_fd = _open_checked_at(
                root_fd, "batches", _DIRECTORY_FLAGS, directory=True
            )
        except FileNotFoundError:
            return False
        try:
            os.stat(batch_id, dir_fd=batches_fd, follow_symlinks=False)
        except FileNotFoundError:
            return False
        return True
    except (LabContractError, OSError) as error:
        raise BlindControllerError(
            "cannot inspect private batch destination"
        ) from error
    finally:
        if batches_fd is not None:
            os.close(batches_fd)
        os.close(root_fd)


def _derived(seed: bytes) -> tuple[str, str, str]:
    def digest(domain: bytes) -> str:
        return hashlib.sha256(domain + seed).hexdigest()

    assignment_id = "assignment-" + digest(b"ai-ip-eval-assignment-v1\0")[:32]
    arm_a_nonce = "arm-" + digest(b"ai-ip-eval-arm-a-v1\0")
    arm_b_nonce = "arm-" + digest(b"ai-ip-eval-arm-b-v1\0")
    return assignment_id, arm_a_nonce, arm_b_nonce


def _seed_bit(seed: bytes, domain: bytes) -> bool:
    return bool(hashlib.sha256(domain + seed).digest()[0] & 1)


def _assignment_values(
    *,
    batch_id: str,
    reviewer_ids: list[str],
    release_conditions: list[str],
    content_hash: str,
    stock_hash: str,
    modified_hash: str,
    seeds: list[bytes],
) -> list[tuple[str, dict[str, object], dict[str, object]]]:
    count = len(reviewer_ids)
    derived = [_derived(seed) for seed in seeds]
    flat_tokens = [token for values in derived for token in values]
    if len(flat_tokens) != len(set(flat_tokens)):
        raise BlindControllerError("opaque identifier collision")
    values = []
    primary_ids = [derived[index][0] for index in range(count)]
    for sequence_index in range(count * 2):
        reviewer_index = sequence_index % count
        is_swap = sequence_index >= count
        role = "swap" if is_swap else "primary"
        reviewer_id = reviewer_ids[reviewer_index]
        label = f"{reviewer_id}-{role}"
        assignment_id, arm_a_nonce, arm_b_nonce = derived[sequence_index]
        primary_seed = seeds[reviewer_index]
        position_ab = _seed_bit(primary_seed, _POSITION_DOMAIN) != is_swap
        stock_is_a = (not _seed_bit(primary_seed, _MAPPING_DOMAIN)) != is_swap
        arm_a_hash, arm_b_hash = (
            (stock_hash, modified_hash) if stock_is_a else (modified_hash, stock_hash)
        )
        assignment = {
            "schemaVersion": 1,
            "objectKind": "BlindAssignment",
            "assignmentId": assignment_id,
            "batchId": batch_id,
            "reviewerId": reviewer_id,
            "position": "AB" if position_ab else "BA",
            "sequenceSlot": sequence_index + 1,
            "releaseCondition": release_conditions[reviewer_index],
            "contentPacketSha256": content_hash,
            "armAOutputSha256": arm_a_hash,
            "armBOutputSha256": arm_b_hash,
            "swapOf": primary_ids[reviewer_index] if is_swap else None,
        }
        mapping = {
            "schemaVersion": 1,
            "objectKind": "BlindAssignmentMapping",
            "assignmentId": assignment_id,
            "arms": {
                "A": {"opaqueNonce": arm_a_nonce, "outputSha256": arm_a_hash},
                "B": {"opaqueNonce": arm_b_nonce, "outputSha256": arm_b_hash},
            },
            "diagnosticOnly": True,
        }
        values.append((label, assignment, mapping))
    return values


def prepare_blind_batch(
    *,
    private_root: PrivateRoot,
    case_receipt_path: Path,
    stock_answer_path: Path,
    modified_answer_path: Path,
    rubric_path: Path,
    base_qualification_receipt_paths: tuple[Path, Path],
    arbitrator_qualification_receipt_path: Path | None,
    batch_id: str,
    analysis_frozen_at: str,
    seed_source: Callable[[int], bytes] = secrets.token_bytes,
) -> BlindPackReceipt:
    """Prepare and atomically expose anonymous primary/swap reviewer trees."""
    batch_id = _component(batch_id, "batchId")
    if _batch_exists(private_root, batch_id):
        raise BlindControllerError("batch destination already exists")
    if (
        type(base_qualification_receipt_paths) is not tuple
        or len(base_qualification_receipt_paths) != 2
    ):
        raise BlindControllerError("exactly two base reviewers are required")
    analysis_time = _parse_time(analysis_frozen_at, "analysisFrozenAt")

    case_receipt, receipt_relative = _private_case_receipt(
        private_root, case_receipt_path
    )
    if case_receipt.get("objectKind") != "CaseCompilationReceipt":
        raise BlindControllerError("case receipt has the wrong object kind")
    case_id = _component(case_receipt.get("caseId"), "caseId")
    expected_receipt = f"cases/{case_id}/coordinator/case-compilation-receipt.json"
    if receipt_relative != expected_receipt:
        raise BlindControllerError("case receipt is outside its authoritative location")
    try:
        content = private_root.read_json(f"cases/{case_id}/content/content-packet.json")
    except (LabContractError, OSError) as error:
        raise BlindControllerError("compiled content packet is unavailable") from error
    content = _validated(content, _CASE_SCHEMA, "content packet")
    if content.get("objectKind") != "ContentPacket":
        raise BlindControllerError("compiled input is not a ContentPacket")
    content_hash = sha256_json(content)
    if (
        content.get("caseId") != case_id
        or case_receipt.get("contentPacketSha256") != content_hash
    ):
        raise BlindControllerError("case receipt does not bind the content packet")

    stock_raw = _load_regular_json(stock_answer_path, "stock answer")
    modified_raw = _load_regular_json(modified_answer_path, "modified answer")
    stock = _validated(stock_raw, _ANSWER_SCHEMA, "stock answer")
    modified = _validated(modified_raw, _ANSWER_SCHEMA, "modified answer")
    if stock.get("caseId") != case_id or modified.get("caseId") != case_id:
        raise BlindControllerError("candidate answer caseId mismatch")
    stock_hash, modified_hash = sha256_json(stock), sha256_json(modified)
    if stock_hash == modified_hash:
        raise BlindControllerError("candidate outputs are canonically identical")
    try:
        _reject_candidate_provenance(stock)
        _reject_candidate_provenance(modified)
    except BlindArtifactError as error:
        raise BlindControllerError("candidate output exposes provenance") from error

    rubric_raw = _load_regular_json(rubric_path, "rubric")
    frozen_rubric = _load_regular_json(_FROZEN_RUBRIC, "frozen rubric")
    if rubric_raw != frozen_rubric or sha256_json(rubric_raw) != sha256_json(
        frozen_rubric
    ):
        raise BlindControllerError("rubric does not match the frozen lab rubric")
    if type(rubric_raw) is not dict:
        raise BlindControllerError("rubric must be a JSON object")
    rubric = rubric_raw

    base_receipts = [
        _qualification(path, analysis_time) for path in base_qualification_receipt_paths
    ]
    receipts = list(base_receipts)
    release_conditions = ["immediate", "immediate"]
    if arbitrator_qualification_receipt_path is not None:
        receipts.append(
            _qualification(arbitrator_qualification_receipt_path, analysis_time)
        )
        release_conditions.append("arbitrationRequired")
    reviewer_ids = [receipt["reviewerId"] for receipt in receipts]
    if len(reviewer_ids) != len(set(reviewer_ids)):
        raise BlindControllerError("reviewer identities must be distinct")

    assignment_count = len(reviewer_ids) * 2
    try:
        seeds = [seed_source(32) for _ in range(assignment_count)]
    except Exception as error:
        raise BlindControllerError("seed source failed") from error
    if any(type(seed) is not bytes or len(seed) != 32 for seed in seeds):
        raise BlindControllerError("every assignment seed must be exactly 32 bytes")
    if len(seeds) != len(set(seeds)):
        raise BlindControllerError("assignment seeds must be unique")

    assignments = _assignment_values(
        batch_id=batch_id,
        reviewer_ids=reviewer_ids,
        release_conditions=release_conditions,
        content_hash=content_hash,
        stock_hash=stock_hash,
        modified_hash=modified_hash,
        seeds=seeds,
    )
    reserved = {
        token
        for _, assignment, mapping in assignments
        for token in (
            assignment["assignmentId"],
            mapping["arms"]["A"]["opaqueNonce"],
            mapping["arms"]["B"]["opaqueNonce"],
        )
    }
    candidate_bytes = canonical_json_bytes(stock) + canonical_json_bytes(modified)
    if any(token.encode("ascii") in candidate_bytes for token in reserved):
        raise BlindControllerError("candidate output contains a reserved opaque nonce")

    manifest = {
        "schemaVersion": 1,
        "objectKind": "EvaluationBatch",
        "batchId": batch_id,
        "caseId": case_id,
        "rubricSha256": sha256_json(rubric),
        "armOutputSha256s": sorted((stock_hash, modified_hash)),
        "reviewerIds": reviewer_ids,
        "analysisFrozenAt": analysis_frozen_at,
        "diagnosticOnly": True,
    }
    arm_key = {
        "schemaVersion": 1,
        "objectKind": "ArmKey",
        "batchId": batch_id,
        "stockOutputSha256": stock_hash,
        "modifiedOutputSha256": modified_hash,
        "diagnosticOnly": True,
    }
    try:
        _validate_arm_key(
            arm_key,
            batch_id=batch_id,
            stock_hash=stock_hash,
            modified_hash=modified_hash,
        )
    except BlindArtifactError as error:
        raise BlindControllerError("invalid private arm key") from error
    artifacts: dict[str, object] = {
        "coordinator/batch-manifest.json": manifest,
        "coordinator/arm-key.json": arm_key,
    }
    assignment_hashes = []
    mapping_hashes = []
    for label, assignment, mapping in assignments:
        try:
            _validate_assignment_mapping(
                mapping,
                assignment_id=assignment["assignmentId"],
                arm_a_hash=assignment["armAOutputSha256"],
                arm_b_hash=assignment["armBOutputSha256"],
            )
        except BlindArtifactError as error:
            raise BlindControllerError("invalid private assignment mapping") from error
        answer_by_hash = {stock_hash: stock, modified_hash: modified}
        answer_a = answer_by_hash[assignment["armAOutputSha256"]]
        answer_b = answer_by_hash[assignment["armBOutputSha256"]]
        reviewer_base = f"reviewer/{assignment['reviewerId']}/{label}"
        artifacts[f"coordinator/mappings/{label}.json"] = mapping
        artifacts[f"{reviewer_base}/assignment.json"] = assignment
        artifacts[f"{reviewer_base}/A.json"] = answer_a
        artifacts[f"{reviewer_base}/B.json"] = answer_b
        artifacts[f"{reviewer_base}/content-packet.json"] = content
        artifacts[f"{reviewer_base}/rubric.json"] = rubric
        assignment_hashes.append(sha256_json(assignment))
        mapping_hashes.append(sha256_json(mapping))
    receipt: BlindPackReceipt = {
        "schemaVersion": 1,
        "objectKind": "BlindPackReceipt",
        "batchId": batch_id,
        "batchManifestSha256": sha256_json(manifest),
        "armKeySha256": sha256_json(arm_key),
        "assignmentSha256s": assignment_hashes,
        "mappingSha256s": mapping_hashes,
        "createdAt": analysis_frozen_at,
    }

    _validated(manifest, _BATCH_SCHEMA, "batch manifest")
    for _, assignment, _ in assignments:
        _validated(assignment, _BLIND_SCHEMA, "blind assignment")
    _validated(receipt, _BLIND_SCHEMA, "blind pack receipt")
    for value in (*artifacts.values(), receipt):
        canonical_json_bytes(value)

    stage_token = hashlib.sha256(
        "\0".join(sorted(reserved)).encode("ascii")
    ).hexdigest()[:24]
    stage_name = f".{batch_id}-{stage_token}.staging"
    try:
        _stage_and_publish(
            private_root=private_root,
            batch_id=batch_id,
            stage_name=stage_name,
            artifacts=artifacts,
            receipt=receipt,
        )
    except BlindArtifactError as error:
        raise BlindControllerError("blind artifact transaction failed") from error
    return receipt
