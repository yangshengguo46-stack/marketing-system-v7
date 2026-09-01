"""Post-seal reveal for the provider-free diagnostic fixture pilot."""

import re
from datetime import datetime, timezone
from pathlib import Path

if __package__:
    from .contracts import (
        BatchDecision,
        LabContractError,
        sha256_json,
        validate_contract,
    )
    from .private_fs import PrivateRoot
else:
    from contracts import (
        BatchDecision,
        LabContractError,
        sha256_json,
        validate_contract,
    )
    from private_fs import PrivateRoot


class UnlockError(ValueError):
    """Raised when sealed evidence cannot authorize a diagnostic unlock."""


_LAB_ROOT = Path(__file__).resolve().parents[3] / "ai-ip-evals" / "lab"
_BATCH_SCHEMA = _LAB_ROOT / "schemas" / "batch.schema.json"
_CASE_SCHEMA = _LAB_ROOT / "schemas" / "case-bundle.schema.json"
_COMPONENT = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}\Z")
_SHA256 = re.compile(r"[0-9a-f]{64}\Z")
_RFC3339 = re.compile(
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?"
    r"(?:Z|[+-](?:0\d|1\d|2[0-3]):[0-5]\d)\Z"
)
_ARM_KEY_FIELDS = {
    "schemaVersion",
    "objectKind",
    "batchId",
    "stockOutputSha256",
    "modifiedOutputSha256",
    "diagnosticOnly",
}


def _time(value: object, label: str) -> datetime:
    if type(value) is not str or _RFC3339.fullmatch(value) is None:
        raise UnlockError(f"{label} must be a strict RFC3339 timestamp")
    try:
        parsed = datetime.fromisoformat(
            value[:-1] + "+00:00" if value.endswith("Z") else value
        )
    except ValueError as error:
        raise UnlockError(f"{label} must be a strict RFC3339 timestamp") from error
    return parsed.astimezone(timezone.utc)


def _component(value: object, label: str) -> str:
    if type(value) is not str or _COMPONENT.fullmatch(value) is None:
        raise UnlockError(f"invalid {label}")
    return value


def _authoritative_path(
    private_root: PrivateRoot,
    path: Path,
    *,
    prefix: str,
    directory: str,
    filename: str,
    label: str,
) -> tuple[str, str]:
    candidate = Path(path)
    if not candidate.is_absolute():
        raise UnlockError(f"{label} path must be absolute")
    try:
        relative = candidate.relative_to(private_root.path)
    except ValueError as error:
        raise UnlockError(f"{label} is outside the trusted private root") from error
    parts = relative.parts
    if (
        len(parts) != 4
        or parts[0] != prefix
        or parts[2] != directory
        or parts[3] != filename
    ):
        raise UnlockError(f"{label} path is not authoritative")
    identifier = _component(parts[1], f"{prefix} identifier")
    expected = private_root.path / prefix / identifier / directory / filename
    if candidate != expected:
        raise UnlockError(f"{label} path is not exact")
    return relative.as_posix(), identifier


def _private_contract(
    private_root: PrivateRoot, relative: str, schema: Path, label: str
) -> dict[str, object]:
    try:
        value = private_root.read_json(relative)
        validate_contract(value, schema)
    except (LabContractError, OSError) as error:
        raise UnlockError(f"cannot validate {label}") from error
    if type(value) is not dict:
        raise UnlockError(f"invalid {label}")
    return value


def _validate_statistics(value: dict[str, object], batch_id: str) -> set[str]:
    if value.get("objectKind") != "BlindStatistics" or value.get("batchId") != batch_id:
        raise UnlockError("blind statistics batch binding mismatch")
    qualified = value["qualifiedReviewerCount"]
    review_count = value["reviewCount"]
    consistent = value["swapConsistentCount"]
    counts = value["preferenceCounts"]
    if sum(counts.values()) != qualified or consistent > qualified:
        raise UnlockError("blind statistics counts are incomplete")
    required = value["arbitrationRequired"]
    completed = value["arbitrationCompleted"]
    if required:
        complete_shape = completed is True and review_count == 6 and qualified == 3
    else:
        complete_shape = completed is False and review_count == 4 and qualified == 2
    if not complete_shape:
        raise UnlockError("blind statistics arbitration state is incomplete")
    severe = value["severeByOutputSha256"]
    if len(severe) != 2:
        raise UnlockError("blind statistics must bind exactly two outputs")
    return set(severe)


def _validate_arm_key(
    value: object, batch_id: str, sealed_outputs: set[str]
) -> tuple[str, str]:
    if type(value) is not dict or set(value) != _ARM_KEY_FIELDS:
        raise UnlockError("ArmKey must have exact fields")
    if (
        type(value["schemaVersion"]) is not int
        or value["schemaVersion"] != 1
        or type(value["objectKind"]) is not str
        or value["objectKind"] != "ArmKey"
        or type(value["batchId"]) is not str
        or value["batchId"] != batch_id
        or value["diagnosticOnly"] is not True
    ):
        raise UnlockError("ArmKey authority mismatch")
    stock = value["stockOutputSha256"]
    modified = value["modifiedOutputSha256"]
    if (
        type(stock) is not str
        or _SHA256.fullmatch(stock) is None
        or type(modified) is not str
        or _SHA256.fullmatch(modified) is None
        or stock == modified
        or {stock, modified} != sealed_outputs
    ):
        raise UnlockError("ArmKey output binding mismatch")
    return stock, modified


def unlock_fixture_pilot(
    *,
    private_root: PrivateRoot,
    blind_statistics_path: Path,
    arm_key_path: Path,
    case_receipt_path: Path,
    outcome_packet_path: Path,
    unlocked_at: str,
) -> BatchDecision:
    """Reveal only the identities needed for one immutable diagnostic decision."""
    statistics_relative, batch_id = _authoritative_path(
        private_root,
        blind_statistics_path,
        prefix="batches",
        directory="coordinator",
        filename="blind-statistics.json",
        label="blind statistics",
    )
    arm_relative, arm_batch_id = _authoritative_path(
        private_root,
        arm_key_path,
        prefix="batches",
        directory="coordinator",
        filename="arm-key.json",
        label="arm key",
    )
    receipt_relative, case_id = _authoritative_path(
        private_root,
        case_receipt_path,
        prefix="cases",
        directory="coordinator",
        filename="case-compilation-receipt.json",
        label="case receipt",
    )
    outcome_relative, outcome_case_id = _authoritative_path(
        private_root,
        outcome_packet_path,
        prefix="cases",
        directory="outcome",
        filename="outcome-packet.json",
        label="outcome packet",
    )
    if arm_batch_id != batch_id or outcome_case_id != case_id:
        raise UnlockError("unlock artifact paths do not share one authority")

    statistics = _private_contract(
        private_root, statistics_relative, _BATCH_SCHEMA, "blind statistics"
    )
    sealed_outputs = _validate_statistics(statistics, batch_id)
    statistics_sha = sha256_json(statistics)

    receipt = _private_contract(
        private_root, receipt_relative, _CASE_SCHEMA, "case receipt"
    )
    if (
        receipt.get("objectKind") != "CaseCompilationReceipt"
        or receipt.get("caseId") != case_id
    ):
        raise UnlockError("case receipt authority mismatch")
    outcome = _private_contract(
        private_root, outcome_relative, _CASE_SCHEMA, "outcome packet"
    )
    outcome_sha = sha256_json(outcome)
    if (
        outcome.get("objectKind") != "OutcomePacket"
        or outcome.get("caseId") != case_id
        or receipt.get("outcomePacketSha256") != outcome_sha
    ):
        raise UnlockError("case receipt does not bind the OutcomePacket")

    unlocked = _time(unlocked_at, "unlockedAt")
    if unlocked < _time(statistics.get("sealedAt"), "sealedAt") or unlocked < _time(
        outcome.get("revealAfter"), "revealAfter"
    ):
        raise UnlockError("unlock occurs before sealed evidence may be revealed")

    try:
        arm_key = private_root.read_json(arm_relative)
    except (LabContractError, OSError) as error:
        raise UnlockError("cannot validate arm key") from error
    stock_sha, modified_sha = _validate_arm_key(arm_key, batch_id, sealed_outputs)
    severe_detected = any(statistics["severeByOutputSha256"].values())
    known_failure = bool(outcome["knownFailures"])
    decision: BatchDecision = {
        "schemaVersion": 1,
        "objectKind": "BatchDecision",
        "batchId": batch_id,
        "stockOutputSha256": stock_sha,
        "modifiedOutputSha256": modified_sha,
        "disposition": "fixtureValid"
        if severe_detected and known_failure
        else "iterate",
        "diagnosticOnly": True,
        "providerMode": "not-run",
        "promotionEligible": False,
        "blindStatisticsSha256": statistics_sha,
        "outcomePacketSha256": outcome_sha,
        "unlockedAt": unlocked_at,
    }
    try:
        validate_contract(decision, _BATCH_SCHEMA)
    except LabContractError as error:
        raise UnlockError("cannot validate diagnostic decision") from error

    try:
        stable_statistics = private_root.read_json(statistics_relative)
        if sha256_json(stable_statistics) != statistics_sha:
            raise UnlockError("blind statistics changed during unlock")
        private_root.write_new_json(
            f"batches/{batch_id}/coordinator/batch-decision.json", decision
        )
    except UnlockError:
        raise
    except (LabContractError, OSError) as error:
        raise UnlockError("cannot create diagnostic decision") from error
    return decision
