from pathlib import Path
import subprocess
import sys

import pytest

REPO_ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO_ROOT))
sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import LabContractError, sha256_json, validate_contract
from reviewer_academy import ReviewerAcademyError, evaluate_calibration


LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
REVIEWER_SCHEMA = LAB_ROOT / "schemas" / "reviewer.schema.json"


def test_supports_package_import():
    result = subprocess.run(
        [
            sys.executable,
            "-c",
            "from scripts.ai_ip.eval_lab.reviewer_academy import evaluate_calibration; "
            "assert callable(evaluate_calibration)",
        ],
        cwd=REPO_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr


def _profile(**overrides):
    value = {
        "schemaVersion": 1,
        "objectKind": "ReviewerProfile",
        "reviewerId": "reviewer-business-1",
        "capabilityDomains": ["businessIpJudgment", "evidenceIntegrity"],
        "conflictDisclosure": "accepted",
        "requestedQualifications": ["businessIpJudgment", "evidenceIntegrity"],
    }
    value.update(overrides)
    return value


def _attempt(**overrides):
    value = {
        "schemaVersion": 1,
        "objectKind": "CalibrationAttempt",
        "reviewerId": "reviewer-business-1",
        "policyId": "fixture-reviewer-qualification-v1",
        "anchorCorrect": 4,
        "severeMisses": 0,
        "repeatAgreement": 1000,
        "swapAgreement": 1000,
        "completedAt": "2026-08-31T00:00:00Z",
    }
    value.update(overrides)
    return value


def _policy(**overrides):
    value = {
        "schemaVersion": 1,
        "policyId": "fixture-reviewer-qualification-v1",
        "diagnosticOnly": True,
        "minimumAnchorCorrect": 4,
        "maximumSevereMisses": 0,
        "minimumRepeatAgreementPermille": 1000,
        "minimumSwapAgreementPermille": 1000,
        "qualificationLifetimeDays": 7,
    }
    value.update(overrides)
    return value


def _evaluate(profile=None, attempt=None, policy=None, evaluated_at="2026-08-31T00:00:00Z"):
    return evaluate_calibration(
        profile or _profile(), attempt or _attempt(), policy or _policy(), evaluated_at
    )


def test_qualifies_fixture_reviewer_with_exact_diagnostic_receipt():
    attempt = _attempt()

    receipt = _evaluate(attempt=attempt)

    assert receipt == {
        "schemaVersion": 1,
        "objectKind": "QualificationReceipt",
        "reviewerId": "reviewer-business-1",
        "policyId": "fixture-reviewer-qualification-v1",
        "qualifiedDomains": ["businessIpJudgment", "evidenceIntegrity"],
        "status": "qualified",
        "attemptSha256": sha256_json(attempt),
        "expiresAt": "2026-09-07T00:00:00Z",
        "diagnosticOnly": True,
    }
    validate_contract(receipt, REVIEWER_SCHEMA)


@pytest.mark.parametrize(
    ("attempt_overrides", "expected_status"),
    [
        ({"anchorCorrect": 3}, "notQualified"),
        ({"severeMisses": 1}, "notQualified"),
        ({"repeatAgreement": 999}, "notQualified"),
        ({"swapAgreement": 999}, "notQualified"),
    ],
)
def test_threshold_failures_return_validated_not_qualified_receipts(
    attempt_overrides, expected_status
):
    attempt = _attempt(**attempt_overrides)

    receipt = _evaluate(attempt=attempt)

    assert receipt["status"] == expected_status
    assert receipt["qualifiedDomains"] == []
    assert receipt["attemptSha256"] == sha256_json(attempt)
    assert receipt["diagnosticOnly"] is True
    validate_contract(receipt, REVIEWER_SCHEMA)


def test_disclosed_conflict_returns_not_qualified_diagnostic_receipt():
    attempt = _attempt()

    receipt = _evaluate(profile=_profile(conflictDisclosure="disclosedConflict"), attempt=attempt)

    assert receipt["status"] == "notQualified"
    assert receipt["qualifiedDomains"] == []
    validate_contract(receipt, REVIEWER_SCHEMA)


def test_expired_passing_attempt_returns_expired_not_qualified_receipt():
    receipt = _evaluate(evaluated_at="2026-09-07T00:00:00.000001Z")

    assert receipt["status"] == "expired"
    assert receipt["qualifiedDomains"] == []
    assert receipt["expiresAt"] == "2026-09-07T00:00:00Z"
    validate_contract(receipt, REVIEWER_SCHEMA)


def test_rejects_calibration_attempt_for_another_reviewer():
    with pytest.raises(ReviewerAcademyError, match="reviewerId"):
        _evaluate(attempt=_attempt(reviewerId="reviewer-business-2"))


def test_rejects_calibration_attempt_for_another_policy():
    with pytest.raises(ReviewerAcademyError, match="policyId"):
        _evaluate(attempt=_attempt(policyId="fixture-reviewer-qualification-v2"))


def test_rejects_requested_domain_absent_from_reviewer_capabilities():
    with pytest.raises(ReviewerAcademyError, match="requestedQualifications"):
        _evaluate(
            profile=_profile(
                requestedQualifications=["businessIpJudgment", "safetyJudgment"]
            )
        )


@pytest.mark.parametrize(
    ("profile", "attempt", "policy"),
    [
        (_profile(unexpected="rejected"), _attempt(), _policy()),
        (_profile(), _attempt(unexpected="rejected"), _policy()),
        (_profile(), _attempt(), _policy(unexpected="rejected")),
    ],
)
def test_rejects_unknown_fields_from_profiles_attempts_and_policies(profile, attempt, policy):
    with pytest.raises((ReviewerAcademyError, LabContractError)):
        _evaluate(profile=profile, attempt=attempt, policy=policy)


@pytest.mark.parametrize(
    "timestamp",
    ["2026-08-31T00:00:00", "2026-08-31T08:00:00+08:00"],
)
def test_rejects_non_utc_or_naive_timestamps(timestamp):
    with pytest.raises(ReviewerAcademyError, match="UTC"):
        _evaluate(attempt=_attempt(completedAt=timestamp))
