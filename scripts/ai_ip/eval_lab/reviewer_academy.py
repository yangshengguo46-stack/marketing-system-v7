"""Fixture-only calibration qualification for diagnostic reviewers."""

from datetime import datetime, timedelta, timezone
from pathlib import Path

if __package__:
    from .contracts import (
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )
else:
    from contracts import (
        LabContractError,
        load_exact_json,
        sha256_json,
        validate_contract,
    )


class ReviewerAcademyError(ValueError):
    """Raised when calibration inputs do not meet the lab's binding contract."""


_LAB_ROOT = Path(__file__).resolve().parents[3] / "ai-ip-evals" / "lab"
_REVIEWER_SCHEMA = _LAB_ROOT / "schemas" / "reviewer.schema.json"
_FIXTURE_POLICY_PATH = _LAB_ROOT / "rubrics" / "fixture-qualification-policy.json"
_POLICY_FIELDS = {
    "schemaVersion",
    "policyId",
    "diagnosticOnly",
    "minimumAnchorCorrect",
    "maximumSevereMisses",
    "minimumRepeatAgreementPermille",
    "minimumSwapAgreementPermille",
    "qualificationLifetimeDays",
}
_PERMILLE_FIELDS = {
    "minimumRepeatAgreementPermille",
    "minimumSwapAgreementPermille",
}


def _validate_contract(value: object, label: str) -> dict[str, object]:
    try:
        validate_contract(value, _REVIEWER_SCHEMA)
    except LabContractError as error:
        raise ReviewerAcademyError(f"invalid {label}: {error}") from error
    if type(value) is not dict:
        raise ReviewerAcademyError(f"invalid {label}")
    return value


def _validate_policy(policy: object) -> dict[str, object]:
    if type(policy) is not dict or set(policy) != _POLICY_FIELDS:
        raise ReviewerAcademyError("policy must be an exact fixture policy object")
    if policy["schemaVersion"] != 1 or type(policy["schemaVersion"]) is not int:
        raise ReviewerAcademyError("policy schemaVersion must be 1")
    if type(policy["policyId"]) is not str or not policy["policyId"]:
        raise ReviewerAcademyError("policyId must be a non-empty string")
    if policy["diagnosticOnly"] is not True:
        raise ReviewerAcademyError("policy must be diagnosticOnly")
    for field in _POLICY_FIELDS - {"schemaVersion", "policyId", "diagnosticOnly"}:
        value = policy[field]
        if type(value) is not int or value < 0:
            raise ReviewerAcademyError(f"policy {field} must be a non-negative integer")
        if field in _PERMILLE_FIELDS and value > 1000:
            raise ReviewerAcademyError(f"policy {field} must not exceed 1000")
    return policy


def _load_frozen_policy() -> dict[str, object]:
    try:
        return _validate_policy(load_exact_json(_FIXTURE_POLICY_PATH))
    except (LabContractError, OSError, ReviewerAcademyError) as error:
        raise ReviewerAcademyError(
            "frozen qualification policy is unavailable"
        ) from error


def _require_frozen_policy(policy: dict[str, object]) -> None:
    frozen_policy = _load_frozen_policy()
    if policy != frozen_policy or sha256_json(policy) != sha256_json(frozen_policy):
        raise ReviewerAcademyError(
            "policy must exactly match the frozen fixture policy"
        )


def _parse_utc(value: object, field: str) -> datetime:
    if type(value) is not str:
        raise ReviewerAcademyError(f"{field} must be an RFC3339 UTC timestamp")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as error:
        raise ReviewerAcademyError(
            f"{field} must be an RFC3339 UTC timestamp"
        ) from error
    if parsed.tzinfo is None or parsed.utcoffset() != timedelta(0):
        raise ReviewerAcademyError(f"{field} must be an RFC3339 UTC timestamp")
    return parsed.astimezone(timezone.utc)


def _format_utc(value: datetime) -> str:
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def _receipt(
    profile: dict[str, object],
    attempt: dict[str, object],
    expires_at: datetime,
    status: str,
) -> dict[str, object]:
    domains = profile["requestedQualifications"] if status == "qualified" else []
    receipt = {
        "schemaVersion": 1,
        "objectKind": "QualificationReceipt",
        "reviewerId": profile["reviewerId"],
        "policyId": attempt["policyId"],
        "qualifiedDomains": domains,
        "status": status,
        "attemptSha256": sha256_json(attempt),
        "expiresAt": _format_utc(expires_at),
        "diagnosticOnly": True,
    }
    try:
        validate_contract(receipt, _REVIEWER_SCHEMA)
    except LabContractError as error:
        raise ReviewerAcademyError(f"invalid qualification receipt: {error}") from error
    return receipt


def evaluate_calibration(
    profile: object,
    attempt: object,
    policy: object,
    evaluated_at: object,
) -> dict[str, object]:
    """Return a fixture-only qualification receipt from explicit calibration evidence."""
    profile_value = _validate_contract(profile, "ReviewerProfile")
    attempt_value = _validate_contract(attempt, "CalibrationAttempt")
    policy_value = _validate_policy(policy)
    _require_frozen_policy(policy_value)

    if profile_value["objectKind"] != "ReviewerProfile":
        raise ReviewerAcademyError("profile must be a ReviewerProfile")
    if attempt_value["objectKind"] != "CalibrationAttempt":
        raise ReviewerAcademyError("attempt must be a CalibrationAttempt")
    if attempt_value["reviewerId"] != profile_value["reviewerId"]:
        raise ReviewerAcademyError("attempt reviewerId must match profile reviewerId")
    if attempt_value["policyId"] != policy_value["policyId"]:
        raise ReviewerAcademyError("attempt policyId must match policyId")

    capabilities = profile_value["capabilityDomains"]
    requested = profile_value["requestedQualifications"]
    if not set(requested).issubset(set(capabilities)):
        raise ReviewerAcademyError(
            "requestedQualifications must be declared in capabilityDomains"
        )

    completed_at = _parse_utc(attempt_value["completedAt"], "completedAt")
    evaluation_time = _parse_utc(evaluated_at, "evaluated_at")
    expires_at = completed_at + timedelta(
        days=policy_value["qualificationLifetimeDays"]
    )

    if evaluation_time >= expires_at:
        return _receipt(profile_value, attempt_value, expires_at, "expired")

    thresholds_met = (
        profile_value["conflictDisclosure"] == "accepted"
        and attempt_value["anchorCorrect"] >= policy_value["minimumAnchorCorrect"]
        and attempt_value["severeMisses"] <= policy_value["maximumSevereMisses"]
        and attempt_value["repeatAgreement"]
        >= policy_value["minimumRepeatAgreementPermille"]
        and attempt_value["swapAgreement"]
        >= policy_value["minimumSwapAgreementPermille"]
    )
    status = "qualified" if thresholds_met else "notQualified"
    return _receipt(profile_value, attempt_value, expires_at, status)
