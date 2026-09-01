from datetime import datetime, timezone
from pathlib import Path

try:
    from .batch_contracts import seal_self_commitment, verify_self_commitment
    from .batch_controller_process_lease import OrphanedProcess
    from .batch_receipt_offline import OfflineEvidence
    from .batch_receipt_storage import BoundDirectory, identifier, write_entry
    from .contracts import canonical_json_bytes
except ImportError:
    from batch_contracts import seal_self_commitment, verify_self_commitment
    from batch_controller_process_lease import OrphanedProcess
    from batch_receipt_offline import OfflineEvidence
    from batch_receipt_storage import BoundDirectory, identifier, write_entry
    from contracts import canonical_json_bytes


_FIELDS = set(
    "schemaVersion pairId armClass attemptId processId processGroupId "
    "launchSpecSha256 status stopDisposition errorType recordedAt orphanSha256".split()
)
_TEXT_FIELDS = _FIELDS - {"schemaVersion", "processId", "processGroupId"}


class OrphanAuthorityError(ValueError): ...


def _timestamp(value: object) -> None:
    try:
        if type(value) is not str or not value.endswith("Z"):
            raise ValueError
        datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError as error:
        raise OrphanAuthorityError("invalid orphan authority timestamp") from error


def orphan_authority_name(pair_id: str, attempt_id: str) -> str:
    try:
        pair = identifier(pair_id, "pair ID")
        attempt = identifier(attempt_id, "attempt ID")
    except ValueError as error:
        raise OrphanAuthorityError("orphan authority identifier is invalid") from error
    return f"orphan-{pair}-{attempt}.json"


def seal_orphan_authority(
    root: BoundDirectory, orphan: OrphanedProcess, *, recorded_at: str | None = None
) -> dict[str, object]:
    recorded = recorded_at
    if recorded is None:
        recorded = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    _timestamp(recorded)
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
        name = orphan_authority_name(orphan.pair_id, orphan.attempt_id)
        write_entry(root, name, canonical_json_bytes(sealed) + b"\n")
    except ValueError as error:
        raise OrphanAuthorityError("orphan authority sealing failed") from error
    return sealed


def verify_orphan_authority(
    private_root: Path, pair_id: str, attempt_id: str
) -> dict[str, object]:
    name = orphan_authority_name(pair_id, attempt_id)
    evidence = None
    try:
        evidence = OfflineEvidence(Path(private_root), pair_id)
        evidence.attempt(attempt_id)
        record = evidence.load_root(name)
        invalid_types = type(record) is not dict or set(record) != _FIELDS
        if not invalid_types:
            invalid_types = any(
                type(record[field]) is not int for field in _FIELDS - _TEXT_FIELDS
            )
            invalid_types |= any(
                type(record[field]) is not str for field in _TEXT_FIELDS
            )
        if invalid_types:
            raise OrphanAuthorityError("orphan authority fields or types are invalid")
        verify_self_commitment(record, "orphanSha256")
        if record["pairId"] != pair_id or record["attemptId"] != attempt_id:
            raise OrphanAuthorityError("orphan authority filename identity differs")
        identifier(record["pairId"], "pair ID")
        identifier(record["attemptId"], "attempt ID")
        arm = record["armClass"]
        identity = evidence.load_pair("identity-context.json")
        if (
            arm not in {"stock", "modified"}
            or type(identity) is not dict
            or type(identity.get(arm)) is not dict
            or identity[arm].get("launchSpecSha256") != record["launchSpecSha256"]
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
    except (OSError, ValueError) as error:
        raise OrphanAuthorityError("orphan authority verification failed") from error
    finally:
        if evidence is not None:
            evidence.close()
