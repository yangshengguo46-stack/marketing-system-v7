"""Descriptor-bound sealing and offline verification for orphaned processes."""

from datetime import datetime, timezone
from pathlib import Path

try:
    from .batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        verify_self_commitment,
    )
    from .batch_controller_process_lease import OrphanedProcess
    from .batch_receipt_offline import OfflineEvidence
    from .batch_receipt_storage import (
        BoundDirectory,
        SecureStorageError,
        identifier,
        write_entry,
    )
    from .contracts import LabContractError, canonical_json_bytes
except ImportError:
    from batch_contracts import (
        BatchContractError,
        seal_self_commitment,
        verify_self_commitment,
    )
    from batch_controller_process_lease import OrphanedProcess
    from batch_receipt_offline import OfflineEvidence
    from batch_receipt_storage import (
        BoundDirectory,
        SecureStorageError,
        identifier,
        write_entry,
    )
    from contracts import LabContractError, canonical_json_bytes


_FIELDS = frozenset(
    {
        "schemaVersion",
        "pairId",
        "armClass",
        "attemptId",
        "processId",
        "processGroupId",
        "launchSpecSha256",
        "status",
        "stopDisposition",
        "errorType",
        "recordedAt",
        "orphanSha256",
    }
)
_ARM_CLASSES = frozenset({"stock", "modified"})


class OrphanAuthorityError(ValueError):
    pass


def _timestamp(value: object) -> str:
    if type(value) is not str or not value.endswith("Z"):
        raise OrphanAuthorityError("orphan authority timestamp must be UTC ISO-8601")
    try:
        parsed = datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise OrphanAuthorityError(
            "orphan authority timestamp must be UTC ISO-8601"
        ) from error
    if parsed.utcoffset() != timezone.utc.utcoffset(parsed):
        raise OrphanAuthorityError("orphan authority timestamp must be UTC ISO-8601")
    return value


def orphan_authority_name(pair_id: str, attempt_id: str) -> str:
    try:
        pair = identifier(pair_id, "pair ID")
        attempt = identifier(attempt_id, "attempt ID")
    except SecureStorageError as error:
        raise OrphanAuthorityError("orphan authority identifier is invalid") from error
    return f"orphan-{pair}-{attempt}.json"


def seal_orphan_authority(
    root: BoundDirectory,
    orphan: OrphanedProcess,
    *,
    recorded_at: str | None = None,
) -> dict[str, object]:
    if not isinstance(root, BoundDirectory) or not isinstance(orphan, OrphanedProcess):
        raise OrphanAuthorityError("orphan authority inputs are invalid")
    recorded = _timestamp(
        datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        if recorded_at is None
        else recorded_at
    )
    name = orphan_authority_name(orphan.pair_id, orphan.attempt_id)
    record: dict[str, object] = {
        "schemaVersion": 1,
        "pairId": orphan.pair_id,
        "armClass": orphan.arm_class,
        "attemptId": orphan.attempt_id,
        "processId": orphan.process_id,
        "processGroupId": orphan.process_group_id,
        "launchSpecSha256": orphan.launch_spec_sha256,
        "status": "orphaned",
        "stopDisposition": "unconfirmed",
        "errorType": orphan.error_type,
        "recordedAt": recorded,
        "orphanSha256": "0" * 64,
    }
    try:
        sealed = seal_self_commitment(record, "orphanSha256")
        write_entry(root, name, canonical_json_bytes(sealed) + b"\n")
    except (BatchContractError, LabContractError, SecureStorageError) as error:
        raise OrphanAuthorityError("orphan authority sealing failed") from error
    return sealed


def _require_record(record: object) -> dict[str, object]:
    if type(record) is not dict or set(record) != _FIELDS:
        raise OrphanAuthorityError("orphan authority fields are invalid")
    expected_types = {
        "schemaVersion": int,
        "pairId": str,
        "armClass": str,
        "attemptId": str,
        "processId": int,
        "processGroupId": int,
        "launchSpecSha256": str,
        "status": str,
        "stopDisposition": str,
        "errorType": str,
        "recordedAt": str,
        "orphanSha256": str,
    }
    if any(
        type(record[name]) is not field_type
        for name, field_type in expected_types.items()
    ):
        raise OrphanAuthorityError("orphan authority field types are invalid")
    return record


def verify_orphan_authority(
    private_root: Path, pair_id: str, attempt_id: str
) -> dict[str, object]:
    name = orphan_authority_name(pair_id, attempt_id)
    evidence = None
    try:
        evidence = OfflineEvidence(Path(private_root), pair_id)
        evidence.attempt(attempt_id)
        record = _require_record(evidence.load_root(name))
        verify_self_commitment(record, "orphanSha256")
        if record["pairId"] != pair_id or record["attemptId"] != attempt_id:
            raise OrphanAuthorityError("orphan authority filename identity differs")
        identifier(record["pairId"], "pair ID")
        identifier(record["attemptId"], "attempt ID")
        arm_class = record["armClass"]
        identity = evidence.load_pair("identity-context.json")
        if (
            arm_class not in _ARM_CLASSES
            or type(identity) is not dict
            or type(identity.get(arm_class)) is not dict
            or identity[arm_class].get("launchSpecSha256") != record["launchSpecSha256"]
        ):
            raise OrphanAuthorityError("orphan authority arm commitment differs")
        identifier(record["launchSpecSha256"], "launch specification SHA-256")
        if (
            record["schemaVersion"] != 1
            or record["processId"] <= 0
            or record["processGroupId"] <= 0
            or record["status"] != "orphaned"
            or record["stopDisposition"] != "unconfirmed"
            or not record["errorType"]
        ):
            raise OrphanAuthorityError("orphan authority disposition is invalid")
        _timestamp(record["recordedAt"])
        evidence.load_pair("failure.json")
        if evidence.has_pair("receipt.json"):
            raise OrphanAuthorityError("completed pair cannot claim orphan authority")
        evidence.verify()
        return record
    except OrphanAuthorityError:
        raise
    except (
        BatchContractError,
        LabContractError,
        OSError,
        SecureStorageError,
        ValueError,
    ) as error:
        raise OrphanAuthorityError("orphan authority verification failed") from error
    finally:
        if evidence is not None:
            evidence.close()
