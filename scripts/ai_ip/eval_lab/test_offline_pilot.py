import hashlib
import os
import socket
import sys
import tempfile
import urllib.request
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from cli import main
from contracts import canonical_json_bytes, load_exact_json, sha256_json


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
CASE_ID = "golden-gift-li-culture-v1"
BATCH_ID = "golden-gift-fixture-batch-1"
COMPILED_AT = "2026-08-21T00:00:00Z"
ANALYSIS_FROZEN_AT = "2026-09-01T00:00:00Z"
SEALED_AT = "2026-09-01T00:00:01Z"
UNLOCKED_AT = "2026-09-01T00:00:02Z"
SOURCE_NAMES = {
    "A113": "douyin-community-mcp-a113-2026-08-20.json",
    "A115": "a115-golden-gift-real-e2e-2026-08-20.json",
    "A116": "a116-vertical-incubation-skill-live-2026-08-20.json",
}
SOURCE_IDS = {
    "A113": "v6-a113-community-acceptance",
    "A115": "v6-a115-known-failure",
    "A116": "v6-a116-pollution-diagnostic",
}
SOURCE_DIGESTS = {
    "douyin-community-mcp-a113-2026-08-20.json": (
        "f3171fbbad16cd5fc2d3907d619132fcf873a16225fa29eb78e59be0986ff3f9"
    ),
    "a115-golden-gift-real-e2e-2026-08-20.json": (
        "4fb16c9fe54a313f51a479d60e51e7361726a36c8dd2a1a7a6a5cabcf1398c7e"
    ),
    "a116-vertical-incubation-skill-live-2026-08-20.json": (
        "6b304738309195216b203dc846ee7491e430a396d69fb4001cef93f870af889c"
    ),
}
DIMENSIONS = (
    "businessSubjectClarity",
    "audienceActionFit",
    "directionBreadthAndTradeoffs",
    "evidenceAndUnknownDiscipline",
    "sustainableIpPotential",
    "commercialConnectionWithoutForcedSelling",
)


def _write_json(path: Path, value: object) -> None:
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _source_values() -> dict[str, dict[str, object]]:
    return {
        "A113": {
            "audit_id": "A113",
            "recorded_at": "2026-08-20",
            "provider": {
                "revision": "fixture-community-revision",
                "license_status": "local experiment only",
            },
            "credentials": {"credential_values_recorded": False},
            "deerflow_acceptance": {
                "query": "黄金礼品 人情世故",
                "results": 3,
                "tool_payload_bytes": 2048,
                "status": "passed",
            },
            "production_status": "diagnostic fixture only",
        },
        "A115": {
            "audit": "A115",
            "date": "2026-08-20",
            "status": "known failure retained",
            "semantic_result": {
                "failure": "content-root-abstraction-stopped-too-early"
            },
            "post_run_defect": {"symptom": "unsupported-existing-experience-claim"},
            "secrets_included": False,
        },
        "A116": {
            "audit": "A116",
            "date": "2026-08-20",
            "status": "pollution diagnostic retained",
            "accepted_full_run": {
                "remaining_failure": "unsupported-existing-experience-claim"
            },
            "failed_runs_retained": [
                {"failure": "invented-existing-insight"},
                {"failure": "generic-root-replaced-vertical-candidate"},
            ],
            "secrets_included": False,
        },
    }


def _manifest(source_root: Path) -> dict[str, object]:
    roles = {
        "A113": ["contentEvidence", "referenceEvidence"],
        "A115": ["outcomeEvidence", "referenceEvidence"],
        "A116": ["outcomeEvidence", "referenceEvidence"],
    }
    return {
        "schemaVersion": 1,
        "objectKind": "SourceImportManifest",
        "caseFamilyId": CASE_ID,
        "sources": [
            {
                "sourceId": SOURCE_IDS[audit],
                "relativePath": SOURCE_NAMES[audit],
                "sha256": hashlib.sha256(
                    (source_root / SOURCE_NAMES[audit]).read_bytes()
                ).hexdigest(),
                "packetRoles": roles[audit],
            }
            for audit in ("A113", "A115", "A116")
        ],
    }


def _profile(reviewer_id: str) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "objectKind": "ReviewerProfile",
        "reviewerId": reviewer_id,
        "capabilityDomains": ["businessIpJudgment", "evidenceIntegrity"],
        "conflictDisclosure": "accepted",
        "requestedQualifications": [
            "businessIpJudgment",
            "evidenceIntegrity",
        ],
    }


def _attempt(reviewer_id: str) -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "objectKind": "CalibrationAttempt",
        "reviewerId": reviewer_id,
        "policyId": "fixture-reviewer-qualification-v1",
        "anchorCorrect": 4,
        "severeMisses": 0,
        "repeatAgreement": 1000,
        "swapAgreement": 1000,
        "completedAt": "2026-08-31T00:00:00Z",
    }


def _scores(value: int) -> dict[str, int]:
    return {dimension: value for dimension in DIMENSIONS}


def _arm_for_output(assignment: dict[str, object], output_sha: str) -> str:
    for arm in ("A", "B"):
        if assignment[f"arm{arm}OutputSha256"] == output_sha:
            return arm
    raise AssertionError("expected output is missing from blind assignment")


def _submission(
    assignment: dict[str, object],
    *,
    preferred_output: str,
    known_failure_sha: str,
) -> dict[str, object]:
    output_by_arm = {arm: assignment[f"arm{arm}OutputSha256"] for arm in ("A", "B")}
    scores_by_output = {
        known_failure_sha: _scores(4),
        next(
            value for value in output_by_arm.values() if value != known_failure_sha
        ): _scores(3),
    }
    severe_by_output = {
        known_failure_sha: [
            "fabricatedExistingExperience",
            "notActuallyUsable",
        ]
    }
    return {
        "schemaVersion": 1,
        "objectKind": "ReviewSubmission",
        "assignmentId": assignment["assignmentId"],
        "reviewerId": assignment["reviewerId"],
        "eligibility": "both",
        "preference": _arm_for_output(assignment, preferred_output),
        "dimensionsByArm": {
            arm: scores_by_output[output_by_arm[arm]] for arm in ("A", "B")
        },
        "severeFlagsByArm": {
            arm: severe_by_output.get(output_by_arm[arm], []) for arm in ("A", "B")
        },
        "reasons": ["Opaque fixture judgment."],
        "evidenceRefs": ["v6-a113-community-acceptance"],
        "submittedAt": ANALYSIS_FROZEN_AT,
    }


def _run(capsys, arguments: list[str]) -> dict[str, object]:
    assert main(arguments) == 0
    captured = capsys.readouterr()
    assert captured.err == ""
    assert captured.out.endswith("\n")
    assert captured.out.count("\n") == 1
    return load_exact_json_from_text(captured.out)


def load_exact_json_from_text(payload: str) -> dict[str, object]:
    import json

    value = json.loads(payload)
    assert type(value) is dict
    assert canonical_json_bytes(value).decode("utf-8") + "\n" == payload
    return value


def _qualification_roots(
    tmp_path: Path, capsys
) -> tuple[list[Path], list[dict[str, object]]]:
    reviewer_ids = [
        "reviewer-business-1",
        "reviewer-business-2",
        "reviewer-arbitrator-1",
    ]
    roots = []
    projections = []
    for reviewer_id in reviewer_ids:
        profile = tmp_path / f"{reviewer_id}-profile.json"
        attempt = tmp_path / f"{reviewer_id}-attempt.json"
        root = tmp_path / f"{reviewer_id}-qualification"
        _write_json(profile, _profile(reviewer_id))
        _write_json(attempt, _attempt(reviewer_id))
        projections.append(
            _run(
                capsys,
                [
                    "qualify-reviewer",
                    "--profile",
                    str(profile),
                    "--attempt",
                    str(attempt),
                    "--policy",
                    str(LAB_ROOT / "rubrics/fixture-qualification-policy.json"),
                    "--evaluated-at",
                    "2026-08-31T00:00:00Z",
                    "--output-root",
                    str(root),
                ],
            )
        )
        roots.append(root)
    return roots, projections


def _install_external_operation_traps(monkeypatch) -> None:
    def reject_external(*_args, **_kwargs):
        raise AssertionError("offline pilot attempted an external operation")

    original_getenv = os.getenv

    def reject_credential_lookup(name, *defaults):
        if any(
            marker in name.casefold()
            for marker in ("key", "token", "secret", "credential")
        ):
            raise AssertionError(
                "offline pilot inspected a credential environment variable"
            )
        return original_getenv(name, *defaults)

    monkeypatch.setattr(socket, "create_connection", reject_external)
    monkeypatch.setattr(socket.socket, "connect", reject_external)
    monkeypatch.setattr(urllib.request, "urlopen", reject_external)
    monkeypatch.setattr(os, "getenv", reject_credential_lookup)
    monkeypatch.setattr(
        Path,
        "home",
        classmethod(lambda _cls: reject_external()),
    )


def test_provider_free_golden_gift_pilot_runs_all_five_cli_commands(
    tmp_path, monkeypatch, capsys
):
    _install_external_operation_traps(monkeypatch)
    source_root = tmp_path / "source"
    source_root.mkdir()
    for audit, value in _source_values().items():
        _write_json(source_root / SOURCE_NAMES[audit], value)
    manifest_path = tmp_path / "source-import.json"
    _write_json(manifest_path, _manifest(source_root))
    private_root = tmp_path / "private-root"

    compile_projection = _run(
        capsys,
        [
            "compile-golden-gift",
            "--source-root",
            str(source_root),
            "--source-manifest",
            str(manifest_path),
            "--blueprint",
            str(LAB_ROOT / f"fixtures/{CASE_ID}/case-blueprint.json"),
            "--private-root",
            str(private_root),
            "--compiled-at",
            COMPILED_AT,
        ],
    )
    qualification_roots, qualification_projections = _qualification_roots(
        tmp_path, capsys
    )
    qualification_receipts = [
        root / "qualification-receipt.json" for root in qualification_roots
    ]
    case_receipt = private_root / (
        f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json"
    )
    prepare_projection = _run(
        capsys,
        [
            "prepare-blind-batch",
            "--private-root",
            str(private_root),
            "--case-receipt",
            str(case_receipt),
            "--stock-answer",
            str(LAB_ROOT / "fixtures/synthetic/arm-known-failure.json"),
            "--modified-answer",
            str(LAB_ROOT / "fixtures/synthetic/arm-strong.json"),
            "--rubric",
            str(LAB_ROOT / "rubrics/golden-gift-l1-l2-rubric.json"),
            "--base-qualification-receipt",
            str(qualification_receipts[0]),
            "--base-qualification-receipt",
            str(qualification_receipts[1]),
            "--arbitrator-qualification-receipt",
            str(qualification_receipts[2]),
            "--batch-id",
            BATCH_ID,
            "--analysis-frozen-at",
            ANALYSIS_FROZEN_AT,
        ],
    )

    known_failure_sha = sha256_json(
        load_exact_json(LAB_ROOT / "fixtures/synthetic/arm-known-failure.json")
    )
    strong_sha = sha256_json(
        load_exact_json(LAB_ROOT / "fixtures/synthetic/arm-strong.json")
    )
    reviewer_preferences = {
        "reviewer-business-1": strong_sha,
        "reviewer-business-2": known_failure_sha,
        "reviewer-arbitrator-1": strong_sha,
    }
    submission_paths = []
    for reviewer_id, preferred_output in reviewer_preferences.items():
        for role in ("primary", "swap"):
            label = f"{reviewer_id}-{role}"
            assignment = load_exact_json(
                private_root
                / f"batches/{BATCH_ID}/reviewer/{reviewer_id}/{label}/assignment.json"
            )
            submission_path = tmp_path / f"submission-{label}.json"
            _write_json(
                submission_path,
                _submission(
                    assignment,
                    preferred_output=preferred_output,
                    known_failure_sha=known_failure_sha,
                ),
            )
            submission_paths.append(submission_path)

    seal_arguments = [
        "seal-blind-statistics",
        "--private-root",
        str(private_root),
        "--blind-pack-receipt",
        str(private_root / f"batches/{BATCH_ID}/blind-pack-receipt.json"),
        "--base-qualification-receipt",
        str(qualification_receipts[0]),
        "--base-qualification-receipt",
        str(qualification_receipts[1]),
        "--arbitrator-qualification-receipt",
        str(qualification_receipts[2]),
        "--sealed-at",
        SEALED_AT,
    ]
    for submission_path in submission_paths:
        seal_arguments.extend(("--submission", str(submission_path)))
    seal_projection = _run(capsys, seal_arguments)
    statistics_path = private_root / (
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    )
    statistics = load_exact_json(statistics_path)
    assert statistics["severeByOutputSha256"][known_failure_sha] == [
        "fabricatedExistingExperience",
        "notActuallyUsable",
    ]
    assert statistics["preferenceCounts"] == {
        "A": 1,
        "B": 2,
        "nearTie": 0,
        "abstain": 0,
    }

    result = _run(
        capsys,
        [
            "unlock-fixture-pilot",
            "--private-root",
            str(private_root),
            "--blind-statistics",
            str(statistics_path),
            "--arm-key",
            str(private_root / f"batches/{BATCH_ID}/coordinator/arm-key.json"),
            "--case-receipt",
            str(case_receipt),
            "--outcome-packet",
            str(private_root / f"cases/{CASE_ID}/outcome/outcome-packet.json"),
            "--unlocked-at",
            UNLOCKED_AT,
        ],
    )

    assert compile_projection == {
        "caseId": CASE_ID,
        "objectKind": "CaseCompilationReceipt",
    }
    assert (
        qualification_projections
        == [
            {
                "diagnosticOnly": True,
                "objectKind": "QualificationReceipt",
                "status": "qualified",
            }
        ]
        * 3
    )
    assert prepare_projection == {
        "batchId": BATCH_ID,
        "objectKind": "BlindPackReceipt",
    }
    assert seal_projection == {
        "arbitrationCompleted": True,
        "arbitrationRequired": True,
        "batchId": BATCH_ID,
        "objectKind": "BlindStatistics",
    }
    assert result == {
        "batchId": BATCH_ID,
        "diagnosticOnly": True,
        "disposition": "fixtureValid",
        "promotionEligible": False,
        "providerMode": "not-run",
    }
    assert set(result) == {
        "batchId",
        "diagnosticOnly",
        "disposition",
        "promotionEligible",
        "providerMode",
    }


def test_legacy_sources_compile_with_exact_approved_digests():
    source_root_value = os.environ.get("AI_IP_V6_EVIDENCE_ROOT")
    if source_root_value is None:
        pytest.skip("AI_IP_V6_EVIDENCE_ROOT is not configured")
    source_root = Path(source_root_value).resolve(strict=True)
    source_payloads = {
        name: (source_root / name).read_bytes() for name in SOURCE_DIGESTS
    }
    assert {
        name: hashlib.sha256(payload).hexdigest()
        for name, payload in source_payloads.items()
    } == SOURCE_DIGESTS

    with tempfile.TemporaryDirectory() as temporary:
        temporary_path = Path(temporary)
        source_copy = temporary_path / "source"
        source_copy.mkdir()
        for name, payload in source_payloads.items():
            (source_copy / name).write_bytes(payload)
        private_root = temporary_path / "private-root"
        assert (
            main(
                [
                    "compile-golden-gift",
                    "--source-root",
                    str(source_copy),
                    "--source-manifest",
                    str(LAB_ROOT / f"fixtures/{CASE_ID}/source-import.json"),
                    "--blueprint",
                    str(LAB_ROOT / f"fixtures/{CASE_ID}/case-blueprint.json"),
                    "--private-root",
                    str(private_root),
                    "--compiled-at",
                    COMPILED_AT,
                ]
            )
            == 0
        )
        receipt = load_exact_json(
            private_root / f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json"
        )
        assert (
            receipt["sourceManifestSha256"]
            == hashlib.sha256(
                (LAB_ROOT / f"fixtures/{CASE_ID}/source-import.json").read_bytes()
            ).hexdigest()
        )
