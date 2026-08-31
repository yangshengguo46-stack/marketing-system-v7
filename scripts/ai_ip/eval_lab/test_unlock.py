import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from contracts import canonical_json_bytes, sha256_json, validate_contract
from private_fs import PrivateRoot


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
BATCH_SCHEMA = LAB_ROOT / "schemas" / "batch.schema.json"
CASE_ID = "golden-gift-li-culture-v1"
BATCH_ID = "golden-gift-fixture-batch-1"
STOCK_SHA = "1" * 64
MODIFIED_SHA = "2" * 64
SEALED_AT = "2026-09-01T00:00:00Z"
REVEAL_AFTER = "2026-09-01T08:00:00+08:00"
UNLOCKED_AT = "2026-09-01T00:00:01Z"


def _unlock(**arguments):
    from unlock import unlock_fixture_pilot

    return unlock_fixture_pilot(**arguments)


def _unlock_error():
    from unlock import UnlockError

    return UnlockError


def _outcome(*, known_failures=None, outcome_case_id=CASE_ID):
    return {
        "schemaVersion": 1,
        "objectKind": "OutcomePacket",
        "caseId": outcome_case_id,
        "revealAfter": REVEAL_AFTER,
        "observations": ["The fixture retained its bounded historical outcome."],
        "knownFailures": (
            ["The known-failure arm collapses conditional directions."]
            if known_failures is None
            else known_failures
        ),
        "sourceRefs": ["v6-a115-known-failure"],
    }


def _statistics(**overrides):
    value = {
        "schemaVersion": 1,
        "objectKind": "BlindStatistics",
        "batchId": BATCH_ID,
        "reviewCount": 6,
        "qualifiedReviewerCount": 3,
        "swapConsistentCount": 3,
        "preferenceCounts": {"A": 2, "B": 1, "nearTie": 0, "abstain": 0},
        "severeByOutputSha256": {
            STOCK_SHA: [],
            MODIFIED_SHA: ["notActuallyUsable"],
        },
        "arbitrationRequired": True,
        "arbitrationCompleted": True,
        "sealedAt": SEALED_AT,
    }
    value.update(overrides)
    return value


def _fixture(
    tmp_path,
    *,
    statistics=None,
    outcome=None,
    arm_key=None,
    write_statistics=True,
):
    private_root = PrivateRoot.create_new(tmp_path / "private")
    for relative in (
        "cases",
        f"cases/{CASE_ID}",
        f"cases/{CASE_ID}/coordinator",
        f"cases/{CASE_ID}/outcome",
        "batches",
        f"batches/{BATCH_ID}",
        f"batches/{BATCH_ID}/coordinator",
    ):
        private_root.create_dir(relative)

    outcome = _outcome() if outcome is None else outcome
    receipt = {
        "schemaVersion": 1,
        "objectKind": "CaseCompilationReceipt",
        "caseId": CASE_ID,
        "contentPacketSha256": "3" * 64,
        "outcomePacketSha256": sha256_json(outcome),
        "referenceDossierSha256": "4" * 64,
        "sourceManifestSha256": "5" * 64,
    }
    arm_key = arm_key or {
        "schemaVersion": 1,
        "objectKind": "ArmKey",
        "batchId": BATCH_ID,
        "stockOutputSha256": STOCK_SHA,
        "modifiedOutputSha256": MODIFIED_SHA,
        "diagnosticOnly": True,
    }
    statistics = _statistics() if statistics is None else statistics
    if write_statistics:
        private_root.write_new_json(
            f"batches/{BATCH_ID}/coordinator/blind-statistics.json", statistics
        )
    private_root.write_new_json(
        f"batches/{BATCH_ID}/coordinator/arm-key.json", arm_key
    )
    private_root.write_new_json(
        f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json", receipt
    )
    private_root.write_new_json(
        f"cases/{CASE_ID}/outcome/outcome-packet.json", outcome
    )
    return {
        "private_root": private_root,
        "blind_statistics_path": private_root.path
        / f"batches/{BATCH_ID}/coordinator/blind-statistics.json",
        "arm_key_path": private_root.path
        / f"batches/{BATCH_ID}/coordinator/arm-key.json",
        "case_receipt_path": private_root.path
        / f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json",
        "outcome_packet_path": private_root.path
        / f"cases/{CASE_ID}/outcome/outcome-packet.json",
        "unlocked_at": UNLOCKED_AT,
        "statistics": statistics,
        "outcome": outcome,
    }


def _decision_path(fixture):
    return (
        fixture["private_root"].path
        / f"batches/{BATCH_ID}/coordinator/batch-decision.json"
    )


def test_unlock_writes_the_exact_diagnostic_known_failure_decision(tmp_path):
    fixture = _fixture(tmp_path)

    decision = _unlock(
        **{key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}}
    )

    assert decision == {
        "schemaVersion": 1,
        "objectKind": "BatchDecision",
        "batchId": BATCH_ID,
        "stockOutputSha256": STOCK_SHA,
        "modifiedOutputSha256": MODIFIED_SHA,
        "disposition": "fixtureValid",
        "diagnosticOnly": True,
        "providerMode": "not-run",
        "promotionEligible": False,
        "blindStatisticsSha256": sha256_json(fixture["statistics"]),
        "outcomePacketSha256": sha256_json(fixture["outcome"]),
        "unlockedAt": UNLOCKED_AT,
    }
    validate_contract(decision, BATCH_SCHEMA)
    assert fixture["private_root"].read_json(
        f"batches/{BATCH_ID}/coordinator/batch-decision.json"
    ) == decision
    encoded = canonical_json_bytes(decision).decode("utf-8")
    assert "PASS_TO_PHASE_0B" not in encoded
    assert "G2" not in encoded


def test_unlock_emits_iterate_without_both_seeded_failure_signals(tmp_path):
    for index, (statistics, outcome) in enumerate(
        (
            (_statistics(severeByOutputSha256={STOCK_SHA: [], MODIFIED_SHA: []}), _outcome()),
            (_statistics(), _outcome(known_failures=[])),
        )
    ):
        fixture_parent = tmp_path / str(index)
        fixture_parent.mkdir(mode=0o700)
        fixture = _fixture(fixture_parent, statistics=statistics, outcome=outcome)
        arguments = {
            key: value
            for key, value in fixture.items()
            if key not in {"statistics", "outcome"}
        }

        assert _unlock(**arguments)["disposition"] == "iterate"


@pytest.mark.parametrize(
    "statistics",
    (
        _statistics(arbitrationCompleted=False),
        _statistics(reviewCount=5),
        _statistics(qualifiedReviewerCount=2),
        _statistics(swapConsistentCount=4),
        _statistics(
            arbitrationRequired=False,
            arbitrationCompleted=True,
        ),
        _statistics(
            arbitrationRequired=False,
            arbitrationCompleted=False,
            reviewCount=6,
            qualifiedReviewerCount=3,
        ),
    ),
)
def test_unlock_rejects_incomplete_or_impossible_statistics(tmp_path, statistics):
    fixture = _fixture(tmp_path, statistics=statistics)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert not _decision_path(fixture).exists()


def test_unlock_rejects_absent_statistics(tmp_path):
    fixture = _fixture(tmp_path, write_statistics=False)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert not _decision_path(fixture).exists()


def test_unlock_rejects_cross_batch_and_non_authoritative_paths(tmp_path):
    fixture = _fixture(tmp_path, statistics=_statistics(batchId="another-batch"))
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }
    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    alias_parent = tmp_path / "alias"
    alias_parent.mkdir(mode=0o700)
    valid = _fixture(alias_parent)
    copied = valid["private_root"].path / "batches" / BATCH_ID / "copied-statistics.json"
    copied.write_bytes(valid["blind_statistics_path"].read_bytes())
    copied.chmod(0o600)
    arguments = {
        key: value for key, value in valid.items() if key not in {"statistics", "outcome"}
    }
    arguments["blind_statistics_path"] = copied
    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert not _decision_path(valid).exists()


def test_unlock_validates_outcome_binding_before_opening_arm_key(tmp_path, monkeypatch):
    outcome = _outcome()
    fixture = _fixture(tmp_path, outcome=outcome)
    receipt_relative = f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json"
    receipt = fixture["private_root"].read_json(receipt_relative)
    receipt["outcomePacketSha256"] = "9" * 64
    receipt_path = fixture["case_receipt_path"]
    receipt_path.write_bytes(canonical_json_bytes(receipt) + b"\n")
    reads = []
    original = PrivateRoot.read_json

    def recording_read(self, relative):
        reads.append(Path(relative).as_posix())
        return original(self, relative)

    monkeypatch.setattr(PrivateRoot, "read_json", recording_read)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert f"batches/{BATCH_ID}/coordinator/arm-key.json" not in reads
    assert not _decision_path(fixture).exists()


def test_unlock_rejects_arm_key_output_set_mismatch(tmp_path):
    arm_key = {
        "schemaVersion": 1,
        "objectKind": "ArmKey",
        "batchId": BATCH_ID,
        "stockOutputSha256": STOCK_SHA,
        "modifiedOutputSha256": "8" * 64,
        "diagnosticOnly": True,
    }
    fixture = _fixture(tmp_path, arm_key=arm_key)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert not _decision_path(fixture).exists()


@pytest.mark.parametrize(
    "unlocked_at",
    (
        "2026-09-01 00:00:01Z",
        "2026-08-31T23:59:59Z",
        "2026-09-01T07:59:59+08:00",
    ),
)
def test_unlock_rejects_invalid_or_early_unlock_time(tmp_path, unlocked_at):
    fixture = _fixture(tmp_path)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }
    arguments["unlocked_at"] = unlocked_at

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert not _decision_path(fixture).exists()


def test_unlock_rejects_statistics_mutation_after_arm_reveal(tmp_path, monkeypatch):
    fixture = _fixture(tmp_path)
    statistics_relative = f"batches/{BATCH_ID}/coordinator/blind-statistics.json"
    statistics_path = fixture["blind_statistics_path"]
    original = PrivateRoot.read_json
    statistics_reads = 0

    def mutating_read(self, relative):
        nonlocal statistics_reads
        if Path(relative).as_posix() == statistics_relative:
            statistics_reads += 1
            if statistics_reads == 2:
                changed = dict(fixture["statistics"])
                changed["swapConsistentCount"] = 2
                statistics_path.write_bytes(canonical_json_bytes(changed) + b"\n")
        return original(self, relative)

    monkeypatch.setattr(PrivateRoot, "read_json", mutating_read)
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert statistics_reads == 2
    assert not _decision_path(fixture).exists()


def test_unlock_never_rewrites_statistics_and_preserves_offset_text(tmp_path):
    fixture = _fixture(tmp_path)
    before = fixture["blind_statistics_path"].read_bytes()
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }
    arguments["unlocked_at"] = "2026-09-01T08:00:01+08:00"

    decision = _unlock(**arguments)

    assert decision["unlockedAt"] == "2026-09-01T08:00:01+08:00"
    assert fixture["blind_statistics_path"].read_bytes() == before


def test_unlock_collision_preserves_existing_decision_and_statistics(tmp_path):
    fixture = _fixture(tmp_path)
    existing = {
        "schemaVersion": 1,
        "objectKind": "BatchDecision",
        "batchId": BATCH_ID,
        "stockOutputSha256": STOCK_SHA,
        "modifiedOutputSha256": MODIFIED_SHA,
        "disposition": "iterate",
        "diagnosticOnly": True,
        "providerMode": "not-run",
        "promotionEligible": False,
        "blindStatisticsSha256": "6" * 64,
        "outcomePacketSha256": "7" * 64,
        "unlockedAt": "2026-09-01T00:00:00Z",
    }
    fixture["private_root"].write_new_json(
        f"batches/{BATCH_ID}/coordinator/batch-decision.json", existing
    )
    statistics_before = fixture["blind_statistics_path"].read_bytes()
    arguments = {
        key: value for key, value in fixture.items() if key not in {"statistics", "outcome"}
    }

    with pytest.raises(_unlock_error()):
        _unlock(**arguments)

    assert fixture["private_root"].read_json(
        f"batches/{BATCH_ID}/coordinator/batch-decision.json"
    ) == existing
    assert fixture["blind_statistics_path"].read_bytes() == statistics_before
