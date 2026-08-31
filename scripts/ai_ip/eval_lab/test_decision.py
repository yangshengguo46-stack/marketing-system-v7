import inspect
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import decision
from blind_controller import prepare_blind_batch
from contracts import canonical_json_bytes, load_exact_json, sha256_json, validate_contract
from decision import ArbitrationRequiredError, DecisionError, seal_blind_statistics
from private_fs import PrivateRoot
from test_blind_controller import (
    BATCH_ID,
    LAB_ROOT,
    SEEDS,
    _fixture as controller_fixture,
    _qualification,
    _seed_source,
    _write_json,
)

SEALED_AT = "2026-09-01T08:00:00+08:00"
SUBMITTED_AT = "2026-09-01T00:00:00Z"
DIMENSIONS = (
    "businessSubjectClarity",
    "audienceActionFit",
    "directionBreadthAndTradeoffs",
    "evidenceAndUnknownDiscipline",
    "sustainableIpPotential",
    "commercialConnectionWithoutForcedSelling",
)
SEVERE = ("fabricatedExistingExperience", "notActuallyUsable")


def _scores(score):
    return {dimension: score for dimension in DIMENSIONS}


def _prepared_batch(tmp_path):
    fixture = controller_fixture(tmp_path)
    arbitrator_path = tmp_path / "arbitrator.json"
    _write_json(arbitrator_path, _qualification("reviewer-arbitrator-1"))
    fixture["arbitrator_qualification_receipt_path"] = arbitrator_path
    prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS))
    private = fixture["private_root"]
    receipt_path = private.path / f"batches/{BATCH_ID}/blind-pack-receipt.json"
    reviewers = ("reviewer-business-1", "reviewer-business-2", "reviewer-arbitrator-1")
    assignments = {}
    for reviewer in reviewers:
        for role in ("primary", "swap"):
            label = f"{reviewer}-{role}"
            assignments[label] = private.read_json(
                f"batches/{BATCH_ID}/reviewer/{reviewer}/{label}/assignment.json"
            )
    return {
        "private_root": private,
        "receipt_path": receipt_path,
        "base_qualifications": fixture["base_qualification_receipt_paths"],
        "arbitrator_qualification": arbitrator_path,
        "assignments": assignments,
    }


def _arm_for_output(assignment, output_sha):
    for arm in ("A", "B"):
        if assignment[f"arm{arm}OutputSha256"] == output_sha:
            return arm
    raise AssertionError("fixture output is not present in assignment")


def _submission(
    assignment,
    *,
    preferred_output,
    severe_by_output=None,
    dimensions_by_output=None,
    eligibility="both",
    preference=None,
):
    output_hashes = {
        arm: assignment[f"arm{arm}OutputSha256"] for arm in ("A", "B")
    }
    if preference is None:
        preference = (
            "abstain"
            if preferred_output is None
            else _arm_for_output(assignment, preferred_output)
        )
    severe_by_output = severe_by_output or {}
    dimensions_by_output = dimensions_by_output or {
        output_hashes["A"]: _scores(3),
        output_hashes["B"]: _scores(4),
    }
    return {
        "schemaVersion": 1,
        "objectKind": "ReviewSubmission",
        "assignmentId": assignment["assignmentId"],
        "reviewerId": assignment["reviewerId"],
        "eligibility": eligibility,
        "preference": preference,
        "dimensionsByArm": {
            arm: dimensions_by_output[output_hashes[arm]] for arm in ("A", "B")
        },
        "severeFlagsByArm": {
            arm: severe_by_output.get(output_hashes[arm], []) for arm in ("A", "B")
        },
        "reasons": ["Opaque fixture judgment."],
        "evidenceRefs": ["fixture-evidence"],
        "submittedAt": SUBMITTED_AT,
    }


def _submission_paths(tmp_path, batch, preferences, *, severe=None, dimensions=None):
    if dimensions is None:
        output_a, output_b = _base_preferences(batch)
        dimensions = {output_a: _scores(3), output_b: _scores(4)}
    paths = []
    for reviewer, preferred_output in preferences.items():
        for role in ("primary", "swap"):
            label = f"{reviewer}-{role}"
            path = tmp_path / f"submission-{label}.json"
            _write_json(
                path,
                _submission(
                    batch["assignments"][label],
                    preferred_output=preferred_output,
                    severe_by_output=severe,
                    dimensions_by_output=dimensions,
                ),
            )
            paths.append(path)
    return tuple(paths)


def _args(batch, submissions, **overrides):
    values = {
        "private_root": batch["private_root"],
        "blind_pack_receipt_path": batch["receipt_path"],
        "base_qualification_receipt_paths": batch["base_qualifications"],
        "arbitrator_qualification_receipt_path": batch["arbitrator_qualification"],
        "submission_paths": submissions,
        "sealed_at": SEALED_AT,
    }
    values.update(overrides)
    return values


def _statistics_path(batch):
    return batch["private_root"].path / (
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    )


def _base_preferences(batch):
    primary = batch["assignments"]["reviewer-business-1-primary"]
    return primary["armAOutputSha256"], primary["armBOutputSha256"]


def _arm_key_path(batch):
    return batch["private_root"].path / (
        f"batches/{BATCH_ID}/coordinator/arm-key.json"
    )


def test_seals_output_hash_preferences_without_reading_arm_key(tmp_path, monkeypatch):
    batch = _prepared_batch(tmp_path)
    output_a, output_b = _base_preferences(batch)
    submissions = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
    )
    original_read = PrivateRoot.read_json

    def reject_arm_key(self, relative):
        if Path(relative).name == "arm-key.json":
            raise AssertionError("decision gate attempted to read arm-key.json")
        return original_read(self, relative)

    monkeypatch.setattr(PrivateRoot, "read_json", reject_arm_key)

    result = seal_blind_statistics(**_args(batch, submissions))

    assert result == {
        "schemaVersion": 1,
        "objectKind": "BlindStatistics",
        "batchId": BATCH_ID,
        "reviewCount": 4,
        "qualifiedReviewerCount": 2,
        "swapConsistentCount": 2,
        "preferenceCounts": {"A": 2, "B": 0, "nearTie": 0, "abstain": 0},
        "severeByOutputSha256": {output_a: [], output_b: []},
        "arbitrationRequired": False,
        "arbitrationCompleted": False,
        "sealedAt": SEALED_AT,
    }
    assert batch["private_root"].read_json(
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    ) == result
    validate_contract(result, LAB_ROOT / "schemas/batch.schema.json")
    assert len(sha256_json(result)) == 64
    assert "total" not in json.dumps(result).lower()
    assert "arm_key" not in inspect.signature(seal_blind_statistics).parameters
    assert set(result) == {
        "schemaVersion",
        "objectKind",
        "batchId",
        "reviewCount",
        "qualifiedReviewerCount",
        "swapConsistentCount",
        "preferenceCounts",
        "severeByOutputSha256",
        "arbitrationRequired",
        "arbitrationCompleted",
        "sealedAt",
    }


def test_requires_then_completes_arbitration_and_retains_known_failures(tmp_path):
    batch = _prepared_batch(tmp_path)
    output_a, known_failure = _base_preferences(batch)
    maximum_scores = {output_a: _scores(2), known_failure: _scores(4)}
    severe = {known_failure: list(SEVERE)}
    base = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": known_failure},
        severe=severe,
        dimensions=maximum_scores,
    )

    with pytest.raises(ArbitrationRequiredError):
        seal_blind_statistics(**_args(batch, base))
    assert not _statistics_path(batch).exists()

    arbitrator = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-arbitrator-1": output_a},
        severe=severe,
        dimensions=maximum_scores,
    )
    result = seal_blind_statistics(**_args(batch, base + arbitrator))

    assert result["reviewCount"] == 6
    assert result["qualifiedReviewerCount"] == 3
    assert result["swapConsistentCount"] == 3
    assert result["preferenceCounts"] == {
        "A": 2,
        "B": 1,
        "nearTie": 0,
        "abstain": 0,
    }
    assert result["severeByOutputSha256"][known_failure] == list(SEVERE)
    assert result["arbitrationRequired"] is True
    assert result["arbitrationCompleted"] is True
    assert "totalQualityScore" not in result
    assert batch["private_root"].read_json(
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    ) == result


def test_severe_flags_alone_require_arbitration_and_survive_the_final_seal(tmp_path):
    batch = _prepared_batch(tmp_path)
    preferred_output, severe_output = _base_preferences(batch)
    maximum_scores = {preferred_output: _scores(3), severe_output: _scores(4)}
    severe = {severe_output: list(SEVERE)}
    base = _submission_paths(
        tmp_path,
        batch,
        {
            "reviewer-business-1": preferred_output,
            "reviewer-business-2": preferred_output,
        },
        severe=severe,
        dimensions=maximum_scores,
    )

    with pytest.raises(ArbitrationRequiredError):
        seal_blind_statistics(**_args(batch, base))
    assert not _statistics_path(batch).exists()

    arbitrator = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-arbitrator-1": preferred_output},
        severe=severe,
        dimensions=maximum_scores,
    )
    result = seal_blind_statistics(**_args(batch, base + arbitrator))

    assert result["preferenceCounts"] == {
        "A": 3,
        "B": 0,
        "nearTie": 0,
        "abstain": 0,
    }
    assert result["arbitrationRequired"] is True
    assert result["arbitrationCompleted"] is True
    assert result["severeByOutputSha256"][severe_output] == list(SEVERE)


@pytest.mark.parametrize(
    "case",
    [
        "receipt_is_arm_key",
        "qualification_is_arm_key",
        "submission_is_arm_key",
        "qualification_symlink_to_arm_key",
        "submission_hardlink_to_arm_key",
    ],
)
def test_rejects_arm_key_paths_and_aliases_before_content_read(
    tmp_path, monkeypatch, case
):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    submissions = list(
        _submission_paths(
            tmp_path,
            batch,
            {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
        )
    )
    arm_key = _arm_key_path(batch)
    arguments = _args(batch, tuple(submissions))
    if case == "receipt_is_arm_key":
        arguments["blind_pack_receipt_path"] = arm_key
    elif case == "qualification_is_arm_key":
        arguments["base_qualification_receipt_paths"] = (
            arm_key,
            batch["base_qualifications"][1],
        )
    elif case == "submission_is_arm_key":
        submissions[0] = arm_key
        arguments["submission_paths"] = tuple(submissions)
    elif case == "qualification_symlink_to_arm_key":
        alias = tmp_path / "qualification-alias.json"
        alias.symlink_to(arm_key)
        arguments["base_qualification_receipt_paths"] = (
            alias,
            batch["base_qualifications"][1],
        )
    else:
        alias = tmp_path / "submission-alias.json"
        alias.hardlink_to(arm_key)
        submissions[0] = alias
        arguments["submission_paths"] = tuple(submissions)

    arm_identity = (arm_key.stat().st_dev, arm_key.stat().st_ino)
    attempted_arm_reads = []
    original_private_read = PrivateRoot.read_json
    original_external_read = decision.load_exact_json

    def guarded_private_read(self, relative):
        if Path(relative).name == "arm-key.json":
            attempted_arm_reads.append(("private", str(relative)))
            raise AssertionError("opened arm key through PrivateRoot.read_json")
        return original_private_read(self, relative)

    def guarded_external_read(path):
        metadata = Path(path).stat()
        if (metadata.st_dev, metadata.st_ino) == arm_identity:
            attempted_arm_reads.append(("external", str(path)))
            raise AssertionError("opened arm key through load_exact_json")
        return original_external_read(path)

    monkeypatch.setattr(PrivateRoot, "read_json", guarded_private_read)
    monkeypatch.setattr(decision, "load_exact_json", guarded_external_read)

    with pytest.raises(DecisionError):
        seal_blind_statistics(**arguments)

    assert attempted_arm_reads == []
    assert not _statistics_path(batch).exists()


def test_position_disagreement_becomes_near_tie_and_requires_arbitration(tmp_path):
    batch = _prepared_batch(tmp_path)
    output_a, output_b = _base_preferences(batch)
    paths = list(
        _submission_paths(
            tmp_path,
            batch,
            {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
        )
    )
    swap = load_exact_json(paths[1])
    swap["preference"] = _arm_for_output(
        batch["assignments"]["reviewer-business-1-swap"], output_b
    )
    _write_json(paths[1], swap)

    with pytest.raises(ArbitrationRequiredError) as caught:
        seal_blind_statistics(**_args(batch, tuple(paths)))

    assert caught.value.preference_counts == {
        "A": 1,
        "B": 0,
        "nearTie": 1,
        "abstain": 0,
    }
    assert not _statistics_path(batch).exists()


def test_neither_is_both_output_ineligibility_not_an_ordinary_tie(tmp_path):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    paths = list(
        _submission_paths(
            tmp_path,
            batch,
            {"reviewer-business-1": None, "reviewer-business-2": output_a},
        )
    )
    for index in (0, 1):
        value = load_exact_json(paths[index])
        value["eligibility"] = "neither"
        value["preference"] = "abstain"
        _write_json(paths[index], value)

    with pytest.raises(ArbitrationRequiredError) as caught:
        seal_blind_statistics(**_args(batch, tuple(paths)))

    assert caught.value.preference_counts == {
        "A": 1,
        "B": 0,
        "nearTie": 0,
        "abstain": 1,
    }


@pytest.mark.parametrize(
    "case",
    [
        "extra_submission_field",
        "wrong_reviewer",
        "wrong_assignment",
        "duplicate_assignment",
        "missing_swap",
        "unknown_evidence",
        "dimension_swap_mismatch",
        "severe_swap_mismatch",
        "expired_qualification",
        "unqualified_status",
        "missing_domain",
        "duplicate_reviewer",
        "receipt_assignment_hash",
    ],
)
def test_rejects_invalid_review_sets_before_writing_statistics(tmp_path, case):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    paths = list(
        _submission_paths(
            tmp_path,
            batch,
            {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
        )
    )
    if case in {
        "extra_submission_field",
        "wrong_reviewer",
        "wrong_assignment",
        "unknown_evidence",
        "dimension_swap_mismatch",
        "severe_swap_mismatch",
    }:
        index = 0 if case not in {"dimension_swap_mismatch", "severe_swap_mismatch"} else 1
        value = load_exact_json(paths[index])
        if case == "extra_submission_field":
            value["treatmentId"] = "forbidden"
        elif case == "wrong_reviewer":
            value["reviewerId"] = "reviewer-business-2"
        elif case == "wrong_assignment":
            value["assignmentId"] = "assignment-" + "f" * 32
        elif case == "unknown_evidence":
            value["evidenceRefs"] = ["not-in-content-packet"]
        elif case == "dimension_swap_mismatch":
            value["dimensionsByArm"]["A"][DIMENSIONS[0]] -= 1
        else:
            value["severeFlagsByArm"]["A"] = [SEVERE[0]]
        _write_json(paths[index], value)
    elif case == "duplicate_assignment":
        _write_json(paths[1], load_exact_json(paths[0]))
    elif case == "missing_swap":
        paths.pop(1)
    elif case in {
        "expired_qualification",
        "unqualified_status",
        "missing_domain",
        "duplicate_reviewer",
    }:
        qualification = load_exact_json(batch["base_qualifications"][1])
        if case == "expired_qualification":
            qualification["expiresAt"] = "2026-09-01T08:00:00+08:00"
        elif case == "unqualified_status":
            qualification["status"] = "notQualified"
        elif case == "missing_domain":
            qualification["qualifiedDomains"] = ["businessIpJudgment"]
        else:
            qualification["reviewerId"] = "reviewer-business-1"
        _write_json(batch["base_qualifications"][1], qualification)
    else:
        receipt = load_exact_json(batch["receipt_path"])
        receipt["assignmentSha256s"][0] = "0" * 64
        _write_json(batch["receipt_path"], receipt)

    with pytest.raises(DecisionError):
        seal_blind_statistics(**_args(batch, tuple(paths)))

    assert not _statistics_path(batch).exists()


@pytest.mark.parametrize("arbitrator_count", [1, 2])
def test_rejects_incomplete_or_wrong_release_arbitration_pair(tmp_path, arbitrator_count):
    batch = _prepared_batch(tmp_path)
    output_a, output_b = _base_preferences(batch)
    base = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": output_b},
    )
    arbitrator = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-arbitrator-1": output_a},
    )
    if arbitrator_count == 2:
        private = batch["private_root"]
        label = "reviewer-arbitrator-1-primary"
        relative = f"batches/{BATCH_ID}/reviewer/reviewer-arbitrator-1/{label}/assignment.json"
        assignment = private.read_json(relative)
        assignment["releaseCondition"] = "immediate"
        _write_json(private.path / relative, assignment)
        receipt = load_exact_json(batch["receipt_path"])
        receipt["assignmentSha256s"][assignment["sequenceSlot"] - 1] = sha256_json(
            assignment
        )
        _write_json(batch["receipt_path"], receipt)

    error = ArbitrationRequiredError if arbitrator_count == 1 else DecisionError
    with pytest.raises(error):
        seal_blind_statistics(
            **_args(batch, base + arbitrator[:arbitrator_count])
        )
    assert not _statistics_path(batch).exists()


def test_does_not_count_unneeded_arbitrator_submissions(tmp_path):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    base = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
    )
    arbitrator = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-arbitrator-1": output_a},
    )

    with pytest.raises(DecisionError):
        seal_blind_statistics(**_args(batch, base + arbitrator))

    assert not _statistics_path(batch).exists()


@pytest.mark.parametrize(
    "timestamp",
    [
        "2026-09-01T00:00:00",
        "2026-09-01 00:00:00Z",
        "2026-09-01T00:00:00+0800",
        "2026-09-01T00:00:00+24:00",
    ],
)
def test_rejects_non_rfc3339_seal_times_without_output(tmp_path, timestamp):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    submissions = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
    )

    with pytest.raises(DecisionError):
        seal_blind_statistics(**_args(batch, submissions, sealed_at=timestamp))

    assert not _statistics_path(batch).exists()


def test_create_new_statistics_never_overwrites_existing_file(tmp_path):
    batch = _prepared_batch(tmp_path)
    output_a, _ = _base_preferences(batch)
    submissions = _submission_paths(
        tmp_path,
        batch,
        {"reviewer-business-1": output_a, "reviewer-business-2": output_a},
    )
    sentinel = {"owner": "existing"}
    batch["private_root"].write_new_json(
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json", sentinel
    )

    with pytest.raises(DecisionError):
        seal_blind_statistics(**_args(batch, submissions))

    assert batch["private_root"].read_json(
        f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    ) == sentinel
