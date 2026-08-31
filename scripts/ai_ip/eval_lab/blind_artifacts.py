"""Internal exact contracts and atomic storage for blind batch artifacts."""

import os
import re
import stat
from pathlib import Path

if __package__:
    from .contracts import LabContractError, canonical_json_bytes
    from .private_fs import _DIRECTORY_FLAGS, PrivateRoot, _open_checked_at
else:
    from contracts import LabContractError, canonical_json_bytes
    from private_fs import _DIRECTORY_FLAGS, PrivateRoot, _open_checked_at


class BlindArtifactError(ValueError):
    """Raised when a private artifact or publication transaction is invalid."""


_SHA256 = re.compile(r"[0-9a-f]{64}\Z")
_ASSIGNMENT_ID = re.compile(r"assignment-[0-9a-f]{32}\Z")
_ARM_NONCE = re.compile(r"arm-[0-9a-f]{64}\Z")
_ARM_KEY_FIELDS = {
    "schemaVersion",
    "objectKind",
    "batchId",
    "stockOutputSha256",
    "modifiedOutputSha256",
    "diagnosticOnly",
}
_MAPPING_FIELDS = {
    "schemaVersion",
    "objectKind",
    "assignmentId",
    "arms",
    "diagnosticOnly",
}
_ARM_FIELDS = {"opaqueNonce", "outputSha256"}
_KEY_MARKERS = (
    "stock",
    "modified",
    "treatment",
    "provider",
    "model",
    "binary",
    "executable",
    "skill",
    "cost",
    "sourceroot",
)
_LABEL_PATTERN = re.compile(
    r"(?i)(?:\b(?:stock|modified)\s+arm\b|\barm\s+(?:output|provenance|source|id|label|name)\b|"
    r"\b(?:treatment|provider|model|executable|skill)\s*(?::|=)|"
    r"\bbinary(?:\s+path)?\s*(?::|=)|\bcost(?:\s+(?:usd|cny|rmb|yuan|dollars?))?\s*(?::|=)|"
    r"\bsource[-_ ]?root\s*(?::|=))"
)
_VENDORS = r"openai|gpt(?:-[0-9.]+)?|codex|anthropic|claude|gemini|glm(?:-[0-9.]+)?|doubao|deepseek|qwen"
_VENDOR_PATTERN = re.compile(rf"(?i)(?<![a-z0-9])(?:{_VENDORS})(?![a-z0-9])")
_GENERATION_PATTERN = re.compile(
    rf"(?i)\b(?:generated|created|authored|written|produced)\s+(?:by|with|using)\s+(?:[a-z0-9.-]+\s+)?(?:{_VENDORS})\b"
)
_CHINESE_GENERATION_PATTERN = re.compile(
    rf"(?i)(?:由|使用)\s*(?:{_VENDORS})\s*(?:生成|创作|编写)"
)
_POSIX_PATH = re.compile(r"(?:^|[\s\"'=:])/(?!/)[^\s\"'<>]+")
_WINDOWS_PATH = re.compile(r"(?:^|[\s\"'=])[A-Za-z]:[\\/]")


def _exact_object(value: object, fields: set[str], label: str) -> dict[str, object]:
    if type(value) is not dict or set(value) != fields:
        raise BlindArtifactError(f"{label} must have exact fields")
    try:
        canonical_json_bytes(value)
    except LabContractError as error:
        raise BlindArtifactError(f"{label} is not exact wire JSON") from error
    return value


def _hash(value: object, label: str) -> str:
    if type(value) is not str or not _SHA256.fullmatch(value):
        raise BlindArtifactError(f"{label} must be a SHA-256")
    return value


def _validate_arm_key(
    value: object, *, batch_id: str, stock_hash: str, modified_hash: str
) -> dict[str, object]:
    item = _exact_object(value, _ARM_KEY_FIELDS, "ArmKey")
    if type(item["schemaVersion"]) is not int or item["schemaVersion"] != 1:
        raise BlindArtifactError("ArmKey schemaVersion must be integer 1")
    if item["objectKind"] != "ArmKey" or type(item["objectKind"]) is not str:
        raise BlindArtifactError("ArmKey objectKind mismatch")
    if type(item["batchId"]) is not str or item["batchId"] != batch_id:
        raise BlindArtifactError("ArmKey batchId mismatch")
    if item["diagnosticOnly"] is not True:
        raise BlindArtifactError("ArmKey must be diagnosticOnly")
    if _hash(item["stockOutputSha256"], "stock output") != stock_hash:
        raise BlindArtifactError("ArmKey stock hash mismatch")
    if _hash(item["modifiedOutputSha256"], "modified output") != modified_hash:
        raise BlindArtifactError("ArmKey modified hash mismatch")
    if stock_hash == modified_hash:
        raise BlindArtifactError("ArmKey output hashes must differ")
    return item


def _validate_assignment_mapping(
    value: object,
    *,
    assignment_id: str,
    arm_a_hash: str,
    arm_b_hash: str,
) -> dict[str, object]:
    item = _exact_object(value, _MAPPING_FIELDS, "BlindAssignmentMapping")
    if type(item["schemaVersion"]) is not int or item["schemaVersion"] != 1:
        raise BlindArtifactError("mapping schemaVersion must be integer 1")
    if (
        type(item["objectKind"]) is not str
        or item["objectKind"] != "BlindAssignmentMapping"
    ):
        raise BlindArtifactError("mapping objectKind mismatch")
    if (
        type(item["assignmentId"]) is not str
        or not _ASSIGNMENT_ID.fullmatch(item["assignmentId"])
        or item["assignmentId"] != assignment_id
    ):
        raise BlindArtifactError("mapping assignmentId mismatch")
    if item["diagnosticOnly"] is not True:
        raise BlindArtifactError("mapping must be diagnosticOnly")
    arms = item["arms"]
    if type(arms) is not dict or set(arms) != {"A", "B"}:
        raise BlindArtifactError("mapping arms must be exact A/B")
    expected_hashes = {"A": arm_a_hash, "B": arm_b_hash}
    nonces = []
    for arm in ("A", "B"):
        arm_value = _exact_object(arms[arm], _ARM_FIELDS, f"mapping arm {arm}")
        nonce = arm_value["opaqueNonce"]
        if type(nonce) is not str or not _ARM_NONCE.fullmatch(nonce):
            raise BlindArtifactError(f"mapping arm {arm} nonce is invalid")
        nonces.append(nonce)
        if (
            _hash(arm_value["outputSha256"], f"mapping arm {arm} output")
            != expected_hashes[arm]
        ):
            raise BlindArtifactError(f"mapping arm {arm} hash mismatch")
    if len(set(nonces)) != 2:
        raise BlindArtifactError("mapping arm nonces must differ")
    return item


def _forbidden_string(value: str) -> bool:
    lowered = value.casefold()
    return bool(
        _LABEL_PATTERN.search(value)
        or _VENDOR_PATTERN.search(value)
        or _GENERATION_PATTERN.search(value)
        or _CHINESE_GENERATION_PATTERN.search(value)
        or _POSIX_PATH.search(value)
        or _WINDOWS_PATH.search(value)
        or "file://" in lowered
    )


def _reject_candidate_provenance(value: object) -> None:
    if type(value) is dict:
        for key, child in value.items():
            normalized = re.sub(r"[^a-z0-9]", "", key.casefold())
            if (
                any(marker in normalized for marker in _KEY_MARKERS)
                or normalized.startswith("arm")
                or _forbidden_string(key)
            ):
                raise BlindArtifactError("candidate contains provenance metadata key")
            _reject_candidate_provenance(child)
    elif type(value) is list:
        for child in value:
            _reject_candidate_provenance(child)
    elif type(value) is str and _forbidden_string(value):
        raise BlindArtifactError("candidate contains provenance metadata value")


def _open_batches(private_root: PrivateRoot) -> int:
    root_fd = private_root._open_root()
    try:
        return _open_checked_at(root_fd, "batches", _DIRECTORY_FLAGS, directory=True)
    except (LabContractError, OSError) as error:
        raise BlindArtifactError("trusted batches directory is unavailable") from error
    finally:
        os.close(root_fd)


def _entry_exists(parent_fd: int, name: str) -> bool:
    try:
        os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    except FileNotFoundError:
        return False
    return True


def _remove_tree_at(
    parent_fd: int, name: str, expected_identity: tuple[int, int]
) -> None:
    metadata = os.stat(name, dir_fd=parent_fd, follow_symlinks=False)
    if (metadata.st_dev, metadata.st_ino) != expected_identity or not stat.S_ISDIR(
        metadata.st_mode
    ):
        raise BlindArtifactError("refusing to remove replaced staging directory")
    descriptor = _open_checked_at(
        parent_fd, name, _DIRECTORY_FLAGS, directory=True, expected=metadata
    )
    try:
        with os.scandir(descriptor) as entries:
            children = [entry.name for entry in entries]
        for child in children:
            child_metadata = os.stat(child, dir_fd=descriptor, follow_symlinks=False)
            if stat.S_ISDIR(child_metadata.st_mode):
                _remove_tree_at(
                    descriptor,
                    child,
                    (child_metadata.st_dev, child_metadata.st_ino),
                )
            else:
                os.unlink(child, dir_fd=descriptor)
    finally:
        os.close(descriptor)
    os.rmdir(name, dir_fd=parent_fd)


def _cleanup_stage(
    private_root: PrivateRoot, stage_name: str, identity: tuple[int, int]
) -> None:
    batches_fd = _open_batches(private_root)
    try:
        if _entry_exists(batches_fd, stage_name):
            _remove_tree_at(batches_fd, stage_name, identity)
            os.fsync(batches_fd)
    except (LabContractError, OSError) as error:
        raise BlindArtifactError("cannot safely clean staging directory") from error
    finally:
        os.close(batches_fd)


def _atomic_publish(private_root: PrivateRoot, stage_name: str, batch_id: str) -> None:
    batches_fd = _open_batches(private_root)
    committed = False
    try:
        if _entry_exists(batches_fd, batch_id):
            raise BlindArtifactError("batch destination appeared before publish")
        stage = os.stat(stage_name, dir_fd=batches_fd, follow_symlinks=False)
        if not stat.S_ISDIR(stage.st_mode):
            raise BlindArtifactError("staging artifact is not a directory")
        os.rename(
            stage_name,
            batch_id,
            src_dir_fd=batches_fd,
            dst_dir_fd=batches_fd,
        )
        committed = True
        os.fsync(batches_fd)
    except BlindArtifactError:
        raise
    except (LabContractError, OSError) as error:
        if not committed:
            raise BlindArtifactError("atomic blind batch publish failed") from error
    finally:
        try:
            os.close(batches_fd)
        except OSError as error:
            if not committed:
                raise BlindArtifactError("atomic blind batch publish failed") from error


def _stage_identity(
    private_root: PrivateRoot, stage_name: str
) -> tuple[int, int] | None:
    batches_fd = _open_batches(private_root)
    try:
        try:
            metadata = os.stat(stage_name, dir_fd=batches_fd, follow_symlinks=False)
        except FileNotFoundError:
            return None
        if not stat.S_ISDIR(metadata.st_mode):
            raise BlindArtifactError("staging artifact is not a directory")
        return metadata.st_dev, metadata.st_ino
    finally:
        os.close(batches_fd)


def _stage_and_publish(
    *,
    private_root: PrivateRoot,
    batch_id: str,
    stage_name: str,
    artifacts: dict[str, object],
    receipt: object,
) -> None:
    try:
        private_root.create_dir("batches")
    except LabContractError:
        with private_root._parent("batches/.probe"):
            pass
    batches_fd = _open_batches(private_root)
    try:
        if _entry_exists(batches_fd, batch_id):
            raise BlindArtifactError("batch destination already exists")
        if _entry_exists(batches_fd, stage_name):
            raise BlindArtifactError("blind staging destination already exists")
    finally:
        os.close(batches_fd)

    stage_base = f"batches/{stage_name}"
    cleanup_allowed = False
    identity = None
    try:
        cleanup_allowed = True
        private_root.create_dir(stage_base)
        identity = _stage_identity(private_root, stage_name)
        if identity is None:
            raise BlindArtifactError("created staging directory is unavailable")
        directories = set()
        for relative in artifacts:
            parent = Path(relative).parent
            directories.update(
                str(item) for item in (parent, *parent.parents) if str(item) != "."
            )
        for directory in sorted(directories, key=lambda item: (item.count("/"), item)):
            private_root.create_dir(f"{stage_base}/{directory}")
        for relative, value in artifacts.items():
            private_root.write_new_json(f"{stage_base}/{relative}", value)
        for relative, value in artifacts.items():
            if private_root.read_json(f"{stage_base}/{relative}") != value:
                raise BlindArtifactError("staged blind artifact verification failed")
        private_root.write_new_json(f"{stage_base}/blind-pack-receipt.json", receipt)
        if private_root.read_json(f"{stage_base}/blind-pack-receipt.json") != receipt:
            raise BlindArtifactError("staged blind receipt verification failed")
        _atomic_publish(private_root, stage_name, batch_id)
        cleanup_allowed = False
    except BlindArtifactError:
        raise
    except (LabContractError, OSError) as error:
        raise BlindArtifactError("blind batch staging failed") from error
    finally:
        if cleanup_allowed and identity is None:
            identity = _stage_identity(private_root, stage_name)
        if cleanup_allowed and identity is not None:
            _cleanup_stage(private_root, stage_name, identity)
