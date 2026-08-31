import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from blind_controller import BlindControllerError, prepare_blind_batch
from contracts import (
    LabContractError,
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


def _candidate_text(fixture, text):
    value = load_exact_json(fixture["stock_answer_path"])
    value["subject"] = text
    _write_json(fixture["stock_answer_path"], value)


def _staging_names(private):
    batches = private.path / "batches"
    return (
        []
        if not batches.exists()
        else sorted(
            path.name for path in batches.iterdir() if path.name.endswith(".staging")
        )
    )


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
    ordered_labels = [
        label
        for label, _ in sorted(
            assignments.items(), key=lambda item: item[1]["sequenceSlot"]
        )
    ]
    mappings = [
        private.read_json(f"{base}/coordinator/mappings/{label}.json")
        for label in ordered_labels
    ]
    for label, mapping in zip(ordered_labels, mappings, strict=True):
        assignment = assignments[label]
        assert set(mapping) == {
            "schemaVersion",
            "objectKind",
            "assignmentId",
            "arms",
            "diagnosticOnly",
        }
        assert set(mapping["arms"]) == {"A", "B"}
        assert mapping["assignmentId"] == assignment["assignmentId"]
        for arm in ("A", "B"):
            assert set(mapping["arms"][arm]) == {"opaqueNonce", "outputSha256"}
            assert (
                mapping["arms"][arm]["outputSha256"]
                == assignment[f"arm{arm}OutputSha256"]
            )
            assert mapping["arms"][arm]["opaqueNonce"].startswith("arm-")
    assert receipt == {
        "schemaVersion": 1,
        "objectKind": "BlindPackReceipt",
        "batchId": BATCH_ID,
        "batchManifestSha256": sha256_json(manifest),
        "armKeySha256": sha256_json(arm_key),
        "assignmentSha256s": [
            sha256_json(assignments[label]) for label in ordered_labels
        ],
        "mappingSha256s": [sha256_json(mapping) for mapping in mappings],
        "createdAt": FROZEN_AT,
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


@pytest.mark.parametrize(
    "case",
    [
        "identical_outputs",
        "answer_case_mismatch",
        "unqualified",
        "expired",
        "missing_domains",
        "diagnostic_false",
        "duplicate_reviewer",
        "wrong_reviewer_count",
        "duplicate_arbitrator",
        "wrong_rubric",
        "extra_answer_field",
        "reused_seed",
        "wrong_seed_length",
        "reserved_nonce",
        "case_binding_mismatch",
    ],
)
def test_rejects_invalid_authority_without_destination_batch(tmp_path, case):
    fixture = _fixture(tmp_path)
    seeds = list(SEEDS[:4])
    if case == "identical_outputs":
        fixture["modified_answer_path"].write_text(
            "  " + fixture["stock_answer_path"].read_text(), encoding="utf-8"
        )
    elif case == "answer_case_mismatch":
        value = load_exact_json(fixture["modified_answer_path"])
        value["caseId"] = "another-case"
        _write_json(fixture["modified_answer_path"], value)
    elif case in ("unqualified", "expired"):
        value = load_exact_json(fixture["base_qualification_receipt_paths"][0])
        value["status"] = "notQualified" if case == "unqualified" else "qualified"
        if case == "expired":
            value["expiresAt"] = FROZEN_AT
        _write_json(fixture["base_qualification_receipt_paths"][0], value)
    elif case in ("missing_domains", "diagnostic_false"):
        value = load_exact_json(fixture["base_qualification_receipt_paths"][0])
        if case == "missing_domains":
            value["qualifiedDomains"] = ["businessIpJudgment"]
        else:
            value["diagnosticOnly"] = False
        _write_json(fixture["base_qualification_receipt_paths"][0], value)
    elif case == "duplicate_reviewer":
        duplicate = tmp_path / "duplicate.json"
        _write_json(duplicate, _qualification("reviewer-business-1"))
        fixture["base_qualification_receipt_paths"] = (
            fixture["base_qualification_receipt_paths"][0],
            duplicate,
        )
    elif case == "wrong_reviewer_count":
        fixture["base_qualification_receipt_paths"] = fixture[
            "base_qualification_receipt_paths"
        ][:1]
        seeds = SEEDS[:2]
    elif case == "duplicate_arbitrator":
        duplicate = tmp_path / "arbitrator.json"
        _write_json(duplicate, _qualification("reviewer-business-1"))
        fixture["arbitrator_qualification_receipt_path"] = duplicate
        seeds = SEEDS
    elif case == "wrong_rubric":
        value = load_exact_json(fixture["rubric_path"])
        value["rubricId"] = "weakened-rubric"
        _write_json(fixture["rubric_path"], value)
    elif case == "extra_answer_field":
        value = load_exact_json(fixture["stock_answer_path"])
        value["model"] = "leaking-model"
        _write_json(fixture["stock_answer_path"], value)
    elif case == "reused_seed":
        seeds[3] = seeds[0]
    elif case == "wrong_seed_length":
        seeds[2] = b"short"
    elif case == "reserved_nonce":
        value = load_exact_json(fixture["stock_answer_path"])
        value["subject"] += (
            " arm-37ec8e9fe861cd6f751b9b3a7e10fe4d4eb9aaa7f05d805d9578aeb554ac8b37"
        )
        _write_json(fixture["stock_answer_path"], value)
    elif case == "case_binding_mismatch":
        value = load_exact_json(fixture["case_receipt_path"])
        value["contentPacketSha256"] = "0" * 64
        _write_json(fixture["case_receipt_path"], value)
    fixture["seed_source"] = _seed_source(seeds)

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture)

    assert not (fixture["private_root"].path / "batches" / BATCH_ID).exists()


@pytest.mark.parametrize(
    ("timestamp", "normalized"),
    [
        ("2026-09-01T00:00:00+00:00", "2026-09-01T00:00:00+00:00"),
        ("2026-09-01T08:00:00+08:00", "2026-09-01T00:00:00+00:00"),
        ("2026-08-31T19:00:00-05:00", "2026-09-01T00:00:00+00:00"),
        ("2026-09-01T00:00:00.125Z", "2026-09-01T00:00:00.125000+00:00"),
    ],
)
def test_accepts_strict_rfc3339_offsets_and_normalizes_to_utc(
    tmp_path, timestamp, normalized
):
    from blind_controller import _parse_time

    fixture = _fixture(tmp_path)
    fixture["analysis_frozen_at"] = timestamp

    receipt = prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert receipt["createdAt"] == timestamp
    assert _parse_time(timestamp, "test").isoformat() == normalized


@pytest.mark.parametrize(
    "timestamp",
    [
        "2026-09-01T00:00:00",
        "2026-09-01",
        "2026-09-01 00:00:00+00:00",
        "20260901T000000+00:00",
        "2026-09-01T00:00:00+8:00",
        "2026-09-01T00:00:00+0800",
        "2026-09-01T00:00:00+24:00",
        "2026-09-01T00:00:00+00:60",
        "2026-09-01T00:00:00,5Z",
        "not-a-timestamp",
    ],
)
def test_rejects_naive_malformed_and_permissive_timestamp_spellings(
    tmp_path, timestamp
):
    fixture = _fixture(tmp_path)
    fixture["analysis_frozen_at"] = timestamp

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))


def test_compares_offset_expiry_after_normalizing_to_utc(tmp_path):
    fixture = _fixture(tmp_path)
    path = fixture["base_qualification_receipt_paths"][0]
    value = load_exact_json(path)
    value["expiresAt"] = "2026-09-01T08:00:01+08:00"
    _write_json(path, value)

    receipt = prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert receipt["objectKind"] == "BlindPackReceipt"


@pytest.mark.parametrize("kind", ["non_bytes", "short", "throwing"])
def test_rejects_invalid_seed_sources_without_staging(tmp_path, kind):
    fixture = _fixture(tmp_path)
    if kind == "throwing":

        def source(_count):
            raise RuntimeError("injected seed failure")
    else:
        bad = bytearray(32) if kind == "non_bytes" else b"short"
        source = _seed_source((SEEDS[0], SEEDS[1], bad, SEEDS[3]))

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=source)

    assert not (fixture["private_root"].path / "batches" / BATCH_ID).exists()
    assert _staging_names(fixture["private_root"]) == []


@pytest.mark.parametrize(
    "marker",
    [
        "Treatment: stock arm",
        "Modified arm output",
        "Provider: ExampleVendor",
        "Model: private-model-v1",
        "Binary path: answer.bin",
        "Executable: hidden-runner",
        "Skill: internal-writer",
        "Cost USD: 1.25",
        "source-root=/tmp/private-project",
        "Generated by OpenAI GPT-5",
        "Generated by Anthropic Claude 4",
        "Generated by Google Gemini 2",
        "Generated by Zhipu GLM-4",
        "Generated by Doubao",
        "Generated by DeepSeek",
        "Generated by Qwen",
        "OpenAI",
        "GPT-5",
        "Codex",
        "Anthropic",
        "Claude",
        "Gemini",
        "GLM",
        "Doubao",
        "DeepSeek",
        "Qwen",
        "model GPT-5",
        "/tmp",
        "/用户/私有",
        "/Users/alice/private/answer.json",
        r"C:\private\answer.json",
        "file:///tmp/private/answer.json",
    ],
)
def test_rejects_schema_valid_candidate_provenance_leaks(tmp_path, marker):
    fixture = _fixture(tmp_path)
    _candidate_text(fixture, f"A valid business subject. {marker}")

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert not (fixture["private_root"].path / "batches" / BATCH_ID).exists()
    assert _staging_names(fixture["private_root"]) == []


def test_provenance_guard_recurses_through_candidate_values_and_keys(tmp_path):
    from blind_artifacts import BlindArtifactError, _reject_candidate_provenance

    fixture = _fixture(tmp_path)
    value = load_exact_json(fixture["stock_answer_path"])
    value["directionOptions"][0]["rationale"] = "Generated by OpenAI GPT-5"
    _write_json(fixture["stock_answer_path"], value)

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))
    with pytest.raises(BlindArtifactError):
        _reject_candidate_provenance({"outer": {"MODEL_ID": "hidden"}})


@pytest.mark.parametrize(
    "marker",
    [
        "OpenAI",
        "GPT-5",
        "Codex",
        "Anthropic",
        "Claude",
        "Gemini",
        "GLM",
        "Doubao",
        "DeepSeek",
        "Qwen",
    ],
)
def test_provenance_guard_rejects_bare_vendor_markers_in_nested_keys(marker):
    from blind_artifacts import BlindArtifactError, _reject_candidate_provenance

    with pytest.raises(BlindArtifactError):
        _reject_candidate_provenance({"outer": {marker: "hidden"}})


def test_allows_ordinary_chinese_business_cost_language(tmp_path):
    fixture = _fixture(tmp_path)
    _candidate_text(fixture, "讨论顾客的时间成本、选择风险与赠礼关系")

    receipt = prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert receipt["objectKind"] == "BlindPackReceipt"


@pytest.mark.parametrize(
    ("contract", "mutation"),
    [
        ("arm_key", "extra"),
        ("arm_key", "type"),
        ("arm_key", "kind"),
        ("arm_key", "binding"),
        ("mapping", "extra"),
        ("mapping", "type"),
        ("mapping", "kind"),
        ("mapping", "assignment"),
        ("mapping", "binding"),
    ],
)
def test_private_contracts_reject_exact_shape_type_kind_and_binding_mutations(
    contract, mutation
):
    from blind_artifacts import (
        BlindArtifactError,
        _validate_arm_key,
        _validate_assignment_mapping,
    )

    stock_hash, modified_hash = "a" * 64, "b" * 64
    arm_key = {
        "schemaVersion": 1,
        "objectKind": "ArmKey",
        "batchId": BATCH_ID,
        "stockOutputSha256": stock_hash,
        "modifiedOutputSha256": modified_hash,
        "diagnosticOnly": True,
    }
    mapping = {
        "schemaVersion": 1,
        "objectKind": "BlindAssignmentMapping",
        "assignmentId": "assignment-" + "e" * 32,
        "arms": {
            "A": {"opaqueNonce": "arm-" + "c" * 64, "outputSha256": stock_hash},
            "B": {"opaqueNonce": "arm-" + "d" * 64, "outputSha256": modified_hash},
        },
        "diagnosticOnly": True,
    }
    value = json.loads(json.dumps(arm_key if contract == "arm_key" else mapping))
    if mutation == "extra":
        value["extra"] = "forbidden"
    elif mutation == "type":
        value["schemaVersion"] = True
    elif mutation == "kind":
        value["objectKind"] = "WrongKind"
    elif mutation == "assignment":
        value["assignmentId"] = "assignment-" + "f" * 32
    elif mutation == "binding":
        key = "stockOutputSha256" if contract == "arm_key" else "arms"
        if key == "stockOutputSha256":
            value[key] = "0" * 64
        else:
            value[key]["A"]["outputSha256"] = "0" * 64

    with pytest.raises(BlindArtifactError):
        if contract == "arm_key":
            _validate_arm_key(
                value,
                batch_id=BATCH_ID,
                stock_hash=stock_hash,
                modified_hash=modified_hash,
            )
        else:
            _validate_assignment_mapping(
                value,
                assignment_id=mapping["assignmentId"],
                arm_a_hash=stock_hash,
                arm_b_hash=modified_hash,
            )


@pytest.mark.parametrize("failure_point", ["write", "verify", "receipt", "publish"])
def test_transaction_failures_remove_created_stage_and_never_publish_batch(
    tmp_path, monkeypatch, failure_point
):
    import blind_artifacts

    fixture = _fixture(tmp_path)
    private = fixture["private_root"]
    private.create_dir("batches")
    private.create_dir("batches/unrelated")
    private.write_new_json("batches/unrelated/sentinel.json", {"keep": True})
    if failure_point in ("write", "receipt"):
        original = PrivateRoot.write_new_json

        def fail_write(self, relative, value):
            name = str(relative)
            selected = (
                name.endswith("A.json")
                if failure_point == "write"
                else name.endswith("blind-pack-receipt.json")
            )
            if ".staging/" in name and selected:
                raise LabContractError(f"injected {failure_point} failure")
            return original(self, relative, value)

        monkeypatch.setattr(PrivateRoot, "write_new_json", fail_write)
    elif failure_point == "verify":
        original = PrivateRoot.read_json

        def fail_read(self, relative):
            if ".staging/" in str(relative) and str(relative).endswith(
                "assignment.json"
            ):
                raise LabContractError("injected verification failure")
            return original(self, relative)

        monkeypatch.setattr(PrivateRoot, "read_json", fail_read)
    else:
        monkeypatch.setattr(
            blind_artifacts,
            "_atomic_publish",
            lambda *_args, **_kwargs: (_ for _ in ()).throw(
                blind_artifacts.BlindArtifactError("injected publish failure")
            ),
        )

    with pytest.raises((BlindControllerError, blind_artifacts.BlindArtifactError)):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert not (private.path / "batches" / BATCH_ID).exists()
    assert _staging_names(private) == []
    assert private.read_json("batches/unrelated/sentinel.json") == {"keep": True}


def test_publish_collision_preserves_colliding_destination_and_cleans_stage(
    tmp_path, monkeypatch
):
    import blind_artifacts

    fixture = _fixture(tmp_path)
    private = fixture["private_root"]
    original_publish = blind_artifacts._atomic_publish

    def collide(private_root, stage_name, batch_id):
        private_root.create_dir(f"batches/{batch_id}")
        private_root.write_new_json(
            f"batches/{batch_id}/collision.json", {"owner": "other"}
        )
        return original_publish(private_root, stage_name, batch_id)

    monkeypatch.setattr(blind_artifacts, "_atomic_publish", collide)

    with pytest.raises((BlindControllerError, blind_artifacts.BlindArtifactError)):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert private.read_json(f"batches/{BATCH_ID}/collision.json") == {"owner": "other"}
    assert not (
        private.path / "batches" / BATCH_ID / "blind-pack-receipt.json"
    ).exists()
    assert _staging_names(private) == []


@pytest.mark.parametrize("operation", ["fsync", "close"])
def test_post_rename_io_anomaly_returns_success_with_exact_openable_final(
    tmp_path, monkeypatch, operation
):
    import blind_artifacts

    fixture = _fixture(tmp_path)
    private = fixture["private_root"]
    private.create_dir("batches")
    batches = private.path / "batches"
    identity = (batches.stat().st_dev, batches.stat().st_ino)
    original = getattr(blind_artifacts.os, operation)

    def fail_after_commit(descriptor):
        opened = blind_artifacts.os.fstat(descriptor)
        is_batches = (opened.st_dev, opened.st_ino) == identity
        if is_batches and (batches / BATCH_ID).exists():
            if operation == "close":
                original(descriptor)
            raise OSError(f"injected post-rename {operation} failure")
        return original(descriptor)

    with monkeypatch.context() as patch:
        patch.setattr(blind_artifacts.os, operation, fail_after_commit)
        receipt = prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert private.read_json(f"batches/{BATCH_ID}/blind-pack-receipt.json") == receipt
    assert _staging_names(private) == []


def test_first_staging_metadata_failure_cleans_stage_but_preserves_collision(
    tmp_path, monkeypatch
):
    import blind_artifacts

    fixture = _fixture(tmp_path)
    private = fixture["private_root"]
    original_stat = blind_artifacts.os.stat
    failed = False

    def fail_first_stage_stat(path, *args, **kwargs):
        nonlocal failed
        if not failed and str(path).endswith(".staging"):
            original_stat(path, *args, **kwargs)
            failed = True
            private.create_dir(f"batches/{BATCH_ID}")
            private.write_new_json(
                f"batches/{BATCH_ID}/collision.json", {"owner": "other"}
            )
            raise OSError("injected first staging metadata failure")
        return original_stat(path, *args, **kwargs)

    monkeypatch.setattr(blind_artifacts.os, "stat", fail_first_stage_stat)

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4]))

    assert failed is True
    assert private.read_json(f"batches/{BATCH_ID}/collision.json") == {"owner": "other"}
    assert _staging_names(private) == []


def test_rejects_preexisting_destination_without_overwriting_it(tmp_path):
    fixture = _fixture(tmp_path)
    private = fixture["private_root"]
    private.create_dir("batches")
    private.create_dir(f"batches/{BATCH_ID}")
    private.write_new_json(f"batches/{BATCH_ID}/sentinel.json", {"owned": "before"})
    calls = []

    with pytest.raises(BlindControllerError):
        prepare_blind_batch(**fixture, seed_source=_seed_source(SEEDS[:4], calls))

    assert private.read_json(f"batches/{BATCH_ID}/sentinel.json") == {"owned": "before"}
    assert calls == []
    assert _staging_names(private) == []
