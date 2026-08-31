import hashlib
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from case_factory import CaseFactoryError, compile_golden_gift_case
from contracts import canonical_json_bytes, load_exact_json, sha256_json
from private_fs import PrivateRoot

REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"
CASE_ID = "golden-gift-li-culture-v1"
COMPILED_AT = "2026-08-21T00:00:00Z"
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
FORBIDDEN_CONTENT_KEYS = {
    "thread_id",
    "project_id",
    "runs",
    "performance",
    "human_review_passed",
    "required_path",
    "default_root",
    "content_root",
    "knownFailures",
}


def _source_values():
    return {
        "A113": {
            "audit_id": "A113",
            "recorded_at": "2026-08-20",
            "provider": {
                "revision": "abc123",
                "license_status": "local experiment only",
            },
            "credentials": {"credential_values_recorded": False},
            "deerflow_acceptance": {
                "query": "黄金礼品 人情世故",
                "results": 2,
                "tool_payload_bytes": 1850,
                "status": "passed",
            },
            "production_status": "not approved for production",
        },
        "A115": {
            "audit": "A115",
            "date": "2026-08-20",
            "status": "partial downstream passed; content root failed",
            "semantic_result": {
                "failure": "content-root-abstraction-stopped-too-early"
            },
            "post_run_defect": {
                "symptom": "raw-project-question-rendered-as-account-stance"
            },
            "secrets_included": False,
        },
        "A116": {
            "audit": "A116",
            "date": "2026-08-20",
            "status": "accepted with performance debt",
            "accepted_full_run": {
                "remaining_failure": "unsupported-existing-experience-claim"
            },
            "failed_runs_retained": [
                {"failure": "invented-existing-insight-and-first-hand-experience"},
                {"failure": "generic-root-replaced-vertical-candidate"},
                {"failure": "inactive-local-branch-reintroduced"},
            ],
            "secrets_included": False,
        },
    }


def _write_json(path, value):
    path.write_bytes(canonical_json_bytes(value) + b"\n")


def _write_manifest(source_root, manifest_path):
    roles = {
        "A113": ["contentEvidence", "referenceEvidence"],
        "A115": ["outcomeEvidence", "referenceEvidence"],
        "A116": ["outcomeEvidence", "referenceEvidence"],
    }
    sources = []
    for audit in ("A113", "A115", "A116"):
        source_path = source_root / SOURCE_NAMES[audit]
        sources.append(
            {
                "sourceId": SOURCE_IDS[audit],
                "relativePath": SOURCE_NAMES[audit],
                "sha256": hashlib.sha256(source_path.read_bytes()).hexdigest(),
                "packetRoles": roles[audit],
            }
        )
    _write_json(
        manifest_path,
        {
            "schemaVersion": 1,
            "objectKind": "SourceImportManifest",
            "caseFamilyId": CASE_ID,
            "sources": sources,
        },
    )


def _fixture(tmp_path):
    source_root = tmp_path / "source"
    source_root.mkdir()
    values = _source_values()
    for audit, value in values.items():
        _write_json(source_root / SOURCE_NAMES[audit], value)
    manifest_path = tmp_path / "source-import.json"
    _write_manifest(source_root, manifest_path)
    blueprint_path = tmp_path / "case-blueprint.json"
    blueprint_path.write_bytes(
        (LAB_ROOT / "fixtures" / CASE_ID / "case-blueprint.json").read_bytes()
    )
    private_parent = tmp_path / "private-parent"
    private_parent.mkdir(mode=0o700)
    private_parent.chmod(0o700)
    private = PrivateRoot.create_new(private_parent / "root")
    return source_root, blueprint_path, manifest_path, private


def _compile(fixture, compiled_at=COMPILED_AT):
    source_root, blueprint_path, manifest_path, private = fixture
    return compile_golden_gift_case(
        source_root, blueprint_path, manifest_path, private, compiled_at
    )


def _all_keys(value):
    if type(value) is dict:
        return set(value) | set().union(*(_all_keys(item) for item in value.values()))
    if type(value) is list:
        return set().union(*(_all_keys(item) for item in value))
    return set()


def test_compiles_physically_separated_packets_and_receipt(tmp_path):
    fixture = _fixture(tmp_path)
    source_root, _, manifest_path, private = fixture

    receipt = _compile(fixture)

    case_root = private.path / "cases" / CASE_ID
    relative_files = sorted(
        path.relative_to(case_root).as_posix()
        for path in case_root.rglob("*")
        if path.is_file()
    )
    assert relative_files == [
        "content/content-packet.json",
        "coordinator/case-compilation-receipt.json",
        "outcome/outcome-packet.json",
        "reference/reference-dossier.json",
    ]
    content = private.read_json(f"cases/{CASE_ID}/content/content-packet.json")
    outcome = private.read_json(f"cases/{CASE_ID}/outcome/outcome-packet.json")
    reference = private.read_json(f"cases/{CASE_ID}/reference/reference-dossier.json")
    stored_receipt = private.read_json(
        f"cases/{CASE_ID}/coordinator/case-compilation-receipt.json"
    )

    blueprint = load_exact_json(LAB_ROOT / "fixtures" / CASE_ID / "case-blueprint.json")
    assert content["mission"] == blueprint["mission"]
    assert content["unknowns"] == blueprint["knownUnknowns"]
    assert content["sourceRefs"] == [SOURCE_IDS["A113"]]
    assert content["knownEvidence"] == {
        "sourceId": SOURCE_IDS["A113"],
        "providerRevision": "abc123",
        "localExperimentLicenseStatus": "local experiment only",
        "query": "黄金礼品 人情世故",
        "resultCount": 2,
        "payloadSizeBytes": 1850,
        "limitations": [
            "not approved for production",
            (
                "A113 recorded_at has day precision; ordering treats 2026-08-20 "
                "as inclusive end-of-day UTC without inventing second-level precision."
            ),
        ],
    }
    assert not (FORBIDDEN_CONTENT_KEYS & _all_keys(content))
    assert outcome["knownFailures"] == [
        "A115: content-root-abstraction-stopped-too-early",
        "A115: raw-project-question-rendered-as-account-stance",
        "A116 diagnostic: unsupported-existing-experience-claim",
        "A116 diagnostic: invented-existing-insight-and-first-hand-experience",
        "A116 diagnostic: generic-root-replaced-vertical-candidate",
        "A116 diagnostic: inactive-local-branch-reintroduced",
    ]
    assert outcome["sourceRefs"] == [SOURCE_IDS["A115"], SOURCE_IDS["A116"]]
    assert reference["directionFamilies"] == blueprint["directionFamilies"]
    assert len(reference["directionFamilies"]) == 3
    assert all(not item["mandatoryRoot"] for item in reference["directionFamilies"])
    assert reference["nonCopyBoundaries"] == [
        "The three direction families are conditional options, not mandatory roots.",
        "A115 failures are counterexamples, not answer templates.",
        "A116 is diagnostic evidence and must not become a gold content root.",
    ]
    assert stored_receipt == receipt
    assert receipt == {
        "schemaVersion": 1,
        "objectKind": "CaseCompilationReceipt",
        "caseId": CASE_ID,
        "contentPacketSha256": sha256_json(content),
        "outcomePacketSha256": sha256_json(outcome),
        "referenceDossierSha256": sha256_json(reference),
        "sourceManifestSha256": hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
    }
    assert sorted(path.name for path in source_root.iterdir()) == sorted(
        SOURCE_NAMES.values()
    )


def _mutate_wrong_sha(fixture, tmp_path):
    manifest_path = fixture[2]
    manifest = load_exact_json(manifest_path)
    manifest["sources"][0]["sha256"] = "0" * 64
    _write_json(manifest_path, manifest)


def _mutate_missing_source(fixture, tmp_path):
    (fixture[0] / SOURCE_NAMES["A115"]).unlink()


def _mutate_extra_source(fixture, tmp_path):
    _write_json(fixture[0] / "extra.json", {"not": "authorized"})


def _mutate_source_symlink(fixture, tmp_path):
    source = fixture[0] / SOURCE_NAMES["A113"]
    target = tmp_path / "outside-source.json"
    target.write_bytes(source.read_bytes())
    source.unlink()
    source.symlink_to(target)


def _mutate_a113_credentials(fixture, tmp_path):
    path = fixture[0] / SOURCE_NAMES["A113"]
    value = load_exact_json(path)
    value["credentials"]["credential_values_recorded"] = True
    _write_json(path, value)
    _write_manifest(fixture[0], fixture[2])


def _mutate_a115_secrets(fixture, tmp_path):
    path = fixture[0] / SOURCE_NAMES["A115"]
    value = load_exact_json(path)
    value["secrets_included"] = True
    _write_json(path, value)
    _write_manifest(fixture[0], fixture[2])


def _mutate_a116_secrets(fixture, tmp_path):
    path = fixture[0] / SOURCE_NAMES["A116"]
    value = load_exact_json(path)
    value["secrets_included"] = True
    _write_json(path, value)
    _write_manifest(fixture[0], fixture[2])


def _mutate_case_family(fixture, tmp_path):
    blueprint = load_exact_json(fixture[1])
    blueprint["caseFamilyId"] = "changed-case-family"
    _write_json(fixture[1], blueprint)


def _mutate_a113_identity(fixture, tmp_path):
    path = fixture[0] / SOURCE_NAMES["A113"]
    value = load_exact_json(path)
    value["audit_id"] = "A999"
    _write_json(path, value)
    _write_manifest(fixture[0], fixture[2])


@pytest.mark.parametrize(
    "mutation",
    [
        _mutate_wrong_sha,
        _mutate_missing_source,
        _mutate_extra_source,
        _mutate_source_symlink,
        _mutate_a113_credentials,
        _mutate_a115_secrets,
        _mutate_a116_secrets,
        _mutate_case_family,
        _mutate_a113_identity,
    ],
    ids=lambda mutation: mutation.__name__.removeprefix("_mutate_"),
)
def test_mutation_fails_before_creating_any_case_subtree(tmp_path, mutation):
    fixture = _fixture(tmp_path)
    mutation(fixture, tmp_path)

    with pytest.raises(CaseFactoryError):
        _compile(fixture)

    assert not (fixture[3].path / "cases" / CASE_ID).exists()
    assert (fixture[3].path / ".ai-ip-private-root-v1.json").is_file()


def test_compiled_at_before_inclusive_source_day_fails_without_partial_case(tmp_path):
    fixture = _fixture(tmp_path)

    with pytest.raises(CaseFactoryError):
        _compile(fixture, "2026-08-20T23:59:59Z")

    assert not (fixture[3].path / "cases" / CASE_ID).exists()
    assert (fixture[3].path / ".ai-ip-private-root-v1.json").is_file()


@pytest.mark.parametrize("invalid_target", ["manifest", "blueprint"])
def test_contracts_validate_before_the_source_root_is_inspected(
    tmp_path, invalid_target
):
    fixture = _fixture(tmp_path)
    target = fixture[2] if invalid_target == "manifest" else fixture[1]
    value = load_exact_json(target)
    value["unexpected"] = "schema rejection"
    _write_json(target, value)
    missing_source_root = tmp_path / "does-not-exist"

    with pytest.raises(CaseFactoryError, match=invalid_target):
        compile_golden_gift_case(
            missing_source_root, fixture[1], fixture[2], fixture[3], COMPILED_AT
        )

    assert not (fixture[3].path / "cases" / CASE_ID).exists()
