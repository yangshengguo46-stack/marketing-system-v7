import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from blind_controller import prepare_blind_batch
from contracts import (
    canonical_json_bytes,
    load_exact_json,
    sha256_json,
    validate_contract,
)
from private_fs import PrivateRoot

REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
CASE_ID = "golden-gift-li-culture-v1"
BATCH_ID = "golden-gift-fixture-batch-1"
FROZEN_AT = "2026-09-01T00:00:00Z"
SEEDS = tuple(bytes([index]) * 32 for index in range(1, 7))


def _write_json(path, value):
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _content_packet():
    return {
        "schemaVersion": 1,
        "objectKind": "ContentPacket",
        "caseId": CASE_ID,
        "predictionTime": "2026-08-21T00:00:00Z",
        "mission": "Evaluate bounded L1-L2 marketing judgment.",
        "businessSubject": "A gift-culture business",
        "knownEvidence": {
            "sourceId": "fixture-evidence",
            "providerRevision": "fixture-revision",
            "localExperimentLicenseStatus": "local-only",
            "query": "gift culture",
            "resultCount": 2,
            "payloadSizeBytes": 128,
            "limitations": ["diagnostic fixture only"],
        },
        "unknowns": ["real audience response"],
        "requestedOutput": ["L1", "L2"],
        "sourceRefs": ["fixture-evidence"],
    }


def _qualification(reviewer_id, **overrides):
    value = {
        "schemaVersion": 1,
        "objectKind": "QualificationReceipt",
        "reviewerId": reviewer_id,
        "policyId": "fixture-reviewer-qualification-v1",
        "qualifiedDomains": ["businessIpJudgment", "evidenceIntegrity"],
        "status": "qualified",
        "attemptSha256": "a" * 64,
        "expiresAt": "2026-09-07T00:00:00Z",
        "diagnosticOnly": True,
    }
    value.update(overrides)
    return value


def _fixture(tmp_path):
    private_parent = tmp_path / "private-parent"
    private_parent.mkdir(mode=0o700)
    private_parent.chmod(0o700)
    private = PrivateRoot.create_new(private_parent / "root")
    content = _content_packet()
    case_base = f"cases/{CASE_ID}"
    for relative in (
        "cases",
        case_base,
        f"{case_base}/content",
        f"{case_base}/coordinator",
    ):
        private.create_dir(relative)
    private.write_new_json(f"{case_base}/content/content-packet.json", content)
    receipt = {
        "schemaVersion": 1,
        "objectKind": "CaseCompilationReceipt",
        "caseId": CASE_ID,
        "contentPacketSha256": sha256_json(content),
        "outcomePacketSha256": "b" * 64,
        "referenceDossierSha256": "c" * 64,
        "sourceManifestSha256": "d" * 64,
    }
    receipt_path = (
        private.path / case_base / "coordinator/case-compilation-receipt.json"
    )
    private.write_new_json(receipt_path.relative_to(private.path), receipt)

    stock_path = tmp_path / "stock.json"
    modified_path = tmp_path / "modified.json"
    rubric_path = tmp_path / "rubric.json"
    _write_json(
        stock_path, load_exact_json(LAB_ROOT / "fixtures/synthetic/arm-strong.json")
    )
    _write_json(
        modified_path,
        load_exact_json(LAB_ROOT / "fixtures/synthetic/arm-known-failure.json"),
    )
    _write_json(
        rubric_path, load_exact_json(LAB_ROOT / "rubrics/golden-gift-l1-l2-rubric.json")
    )
    qualification_paths = []
    for index in (1, 2):
        path = tmp_path / f"reviewer-{index}.json"
        _write_json(path, _qualification(f"reviewer-business-{index}"))
        qualification_paths.append(path)
    return {
        "private_root": private,
        "case_receipt_path": receipt_path,
        "stock_answer_path": stock_path,
        "modified_answer_path": modified_path,
        "rubric_path": rubric_path,
        "base_qualification_receipt_paths": tuple(qualification_paths),
        "arbitrator_qualification_receipt_path": None,
        "batch_id": BATCH_ID,
        "analysis_frozen_at": FROZEN_AT,
    }


def _seed_source(values, calls=None):
    iterator = iter(values)

    def supply(count):
        if calls is not None:
            calls.append(count)
        return next(iterator)

    return supply


def _batch_files(private):
    root = private.path / "batches" / BATCH_ID
    return sorted(
        path.relative_to(root).as_posix() for path in root.rglob("*") if path.is_file()
    )


def _json_strings(value):
    if type(value) is dict:
        return [item for child in value.values() for item in _json_strings(child)]
    if type(value) is list:
        return [item for child in value for item in _json_strings(child)]
    return [value] if type(value) is str else []


def test_prepares_deterministic_physically_separated_swapped_assignments(tmp_path):
    fixture = _fixture(tmp_path)
    calls = []
    fixture["seed_source"] = _seed_source(SEEDS[:4], calls)

    receipt = prepare_blind_batch(**fixture)

    expected = [
        "blind-pack-receipt.json",
        "coordinator/arm-key.json",
        "coordinator/batch-manifest.json",
    ]
    for reviewer in ("reviewer-business-1", "reviewer-business-2"):
        for role in ("primary", "swap"):
            label = f"{reviewer}-{role}"
            expected.append(f"coordinator/mappings/{label}.json")
            expected.extend(
                f"reviewer/{reviewer}/{label}/{name}"
                for name in (
                    "A.json",
                    "B.json",
                    "assignment.json",
                    "content-packet.json",
                    "rubric.json",
                )
            )
    assert _batch_files(fixture["private_root"]) == sorted(expected)
    assert calls == [32, 32, 32, 32]

    private = fixture["private_root"]
    base = f"batches/{BATCH_ID}"
    assignments = {}
    for reviewer in ("reviewer-business-1", "reviewer-business-2"):
        for role in ("primary", "swap"):
            label = f"{reviewer}-{role}"
            assignments[label] = private.read_json(
                f"{base}/reviewer/{reviewer}/{label}/assignment.json"
            )
    assert [
        (value["sequenceSlot"], value["position"]) for value in assignments.values()
    ] == [
        (1, "AB"),
        (3, "BA"),
        (2, "BA"),
        (4, "AB"),
    ]
    assert (
        assignments["reviewer-business-1-swap"]["swapOf"]
        == assignments["reviewer-business-1-primary"]["assignmentId"]
    )
    assert (
        assignments["reviewer-business-2-swap"]["swapOf"]
        == assignments["reviewer-business-2-primary"]["assignmentId"]
    )
    assert all(
        value["releaseCondition"] == "immediate" for value in assignments.values()
    )
    assert len({value["assignmentId"] for value in assignments.values()}) == 4
    assert all(
        value["assignmentId"].startswith("assignment-")
        for value in assignments.values()
    )

    stock = load_exact_json(fixture["stock_answer_path"])
    modified = load_exact_json(fixture["modified_answer_path"])
    first = f"{base}/reviewer/reviewer-business-1/reviewer-business-1-primary"
    swapped = f"{base}/reviewer/reviewer-business-1/reviewer-business-1-swap"
    assert private.read_json(f"{first}/A.json") == stock
    assert private.read_json(f"{first}/B.json") == modified
    assert private.read_json(f"{swapped}/A.json") == modified
    assert private.read_json(f"{swapped}/B.json") == stock

    manifest = private.read_json(f"{base}/coordinator/batch-manifest.json")
    arm_key = private.read_json(f"{base}/coordinator/arm-key.json")
    assert manifest["diagnosticOnly"] is True
    assert arm_key == {
        "schemaVersion": 1,
        "objectKind": "ArmKey",
        "batchId": BATCH_ID,
        "stockOutputSha256": sha256_json(stock),
        "modifiedOutputSha256": sha256_json(modified),
        "diagnosticOnly": True,
    }
    validate_contract(manifest, LAB_ROOT / "schemas/batch.schema.json")
    validate_contract(receipt, LAB_ROOT / "schemas/blind-review.schema.json")
    assert private.read_json(f"{base}/blind-pack-receipt.json") == receipt

    forbidden_keys = {
        "stockOutputSha256",
        "modifiedOutputSha256",
        "treatmentId",
        "mapping",
    }
    forbidden_values = {"stock", "modified", "provider", "model", "Skill", "cost"}
    reviewer_root = private.path / "batches" / BATCH_ID / "reviewer"
    for path in reviewer_root.rglob("*.json"):
        value = json.loads(path.read_text())
        assert not (forbidden_keys & (set(value) if type(value) is dict else set()))
        assert not (forbidden_values & set(_json_strings(value)))
        encoded = canonical_json_bytes(value)
        assert b"arm-strong" not in encoded
        assert b"arm-known-failure" not in encoded
        assert str(tmp_path).encode() not in encoded
        assert all(seed.hex().encode() not in encoded for seed in SEEDS[:4])
    assert not any(path.name == "arm-key.json" for path in reviewer_root.rglob("*"))
    assert not any(path.name == "mappings" for path in reviewer_root.rglob("*"))


def test_prepares_unreleased_arbitrator_primary_and_nonadjacent_swap(tmp_path):
    fixture = _fixture(tmp_path)
    arbitrator_path = tmp_path / "arbitrator.json"
    _write_json(arbitrator_path, _qualification("reviewer-arbitrator-1"))
    fixture["arbitrator_qualification_receipt_path"] = arbitrator_path
    calls = []
    fixture["seed_source"] = _seed_source(SEEDS, calls)

    prepare_blind_batch(**fixture)

    private = fixture["private_root"]
    prefix = f"batches/{BATCH_ID}/reviewer/reviewer-arbitrator-1"
    primary = private.read_json(
        f"{prefix}/reviewer-arbitrator-1-primary/assignment.json"
    )
    swap = private.read_json(f"{prefix}/reviewer-arbitrator-1-swap/assignment.json")
    assert (primary["sequenceSlot"], swap["sequenceSlot"]) == (3, 6)
    assert (primary["position"], swap["position"]) == ("AB", "BA")
    assert (
        primary["releaseCondition"] == swap["releaseCondition"] == "arbitrationRequired"
    )
    assert swap["swapOf"] == primary["assignmentId"]
    assert calls == [32] * 6
