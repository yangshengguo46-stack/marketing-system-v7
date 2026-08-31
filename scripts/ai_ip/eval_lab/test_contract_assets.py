import json
from pathlib import Path

import pytest
from jsonschema import Draft202012Validator, ValidationError


REPO_ROOT = Path(__file__).resolve().parents[3]
LAB_ROOT = REPO_ROOT / "ai-ip-evals" / "lab"

SOURCE_DIGESTS = {
    "douyin-community-mcp-a113-2026-08-20.json": "f3171fbbad16cd5fc2d3907d619132fcf873a16225fa29eb78e59be0986ff3f9",
    "a115-golden-gift-real-e2e-2026-08-20.json": "4fb16c9fe54a313f51a479d60e51e7361726a36c8dd2a1a7a6a5cabcf1398c7e",
    "a116-vertical-incubation-skill-live-2026-08-20.json": "6b304738309195216b203dc846ee7491e430a396d69fb4001cef93f870af889c",
}

ASSET_PATHS = (
    "schemas/source-import.schema.json",
    "schemas/case-bundle.schema.json",
    "schemas/case-answer.schema.json",
    "schemas/reviewer.schema.json",
    "schemas/blind-review.schema.json",
    "schemas/batch.schema.json",
    "rubrics/golden-gift-l1-l2-rubric.json",
    "rubrics/fixture-qualification-policy.json",
    "fixtures/golden-gift-li-culture-v1/source-import.json",
    "fixtures/golden-gift-li-culture-v1/case-blueprint.json",
    "fixtures/synthetic/arm-strong.json",
    "fixtures/synthetic/arm-known-failure.json",
)


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    value: dict[str, object] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _load(relative_path: str) -> object:
    return json.loads(
        (LAB_ROOT / relative_path).read_text(encoding="utf-8"),
        object_pairs_hook=_reject_duplicate_keys,
        parse_float=lambda value: (_ for _ in ()).throw(
            ValueError(f"floating-point wire value: {value}")
        ),
    )


@pytest.fixture(scope="module")
def assets() -> dict[str, object]:
    return {relative_path: _load(relative_path) for relative_path in ASSET_PATHS}


def test_all_contract_assets_are_present_and_parse_as_exact_json(assets):
    assert tuple(assets) == ASSET_PATHS


def test_schemas_are_draft_2020_12_and_reject_unknown_root_fields(assets):
    object_kinds = {
        "schemas/source-import.schema.json": ["SourceImportManifest"],
        "schemas/case-bundle.schema.json": [
            "CaseBlueprint",
            "ContentPacket",
            "OutcomePacket",
            "ReferenceDossier",
            "CaseCompilationReceipt",
        ],
        "schemas/case-answer.schema.json": ["CaseAnswer"],
        "schemas/reviewer.schema.json": [
            "ReviewerProfile",
            "CalibrationAttempt",
            "QualificationReceipt",
        ],
        "schemas/blind-review.schema.json": [
            "BlindAssignment",
            "ReviewSubmission",
            "BlindPackReceipt",
        ],
        "schemas/batch.schema.json": [
            "EvaluationBatch",
            "BlindStatistics",
            "BatchDecision",
        ],
    }
    for relative_path in ASSET_PATHS[:6]:
        schema = assets[relative_path]
        Draft202012Validator.check_schema(schema)
        assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema"
        assert schema["properties"]["schemaVersion"] == {"const": 1}
        assert "objectKind" in schema["required"]
        assert schema["additionalProperties"] is False
        declared_kind = schema["properties"]["objectKind"]
        assert (
            declared_kind.get("enum", [declared_kind.get("const")])
            == object_kinds[relative_path]
        )
        errors = Draft202012Validator(schema).iter_errors(
            {
                "schemaVersion": 1,
                "objectKind": object_kinds[relative_path][0],
                "unexpected": "rejected",
            }
        )
        assert any(error.validator == "additionalProperties" for error in errors)


def test_source_manifest_binds_only_the_three_frozen_public_sources(assets):
    source_manifest = assets["fixtures/golden-gift-li-culture-v1/source-import.json"]
    assert source_manifest["caseFamilyId"] == "golden-gift-li-culture-v1"
    assert source_manifest["sources"] == [
        {
            "sourceId": "v6-a113-community-acceptance",
            "relativePath": "douyin-community-mcp-a113-2026-08-20.json",
            "sha256": SOURCE_DIGESTS["douyin-community-mcp-a113-2026-08-20.json"],
            "packetRoles": ["contentEvidence", "referenceEvidence"],
        },
        {
            "sourceId": "v6-a115-known-failure",
            "relativePath": "a115-golden-gift-real-e2e-2026-08-20.json",
            "sha256": SOURCE_DIGESTS["a115-golden-gift-real-e2e-2026-08-20.json"],
            "packetRoles": ["outcomeEvidence", "referenceEvidence"],
        },
        {
            "sourceId": "v6-a116-pollution-diagnostic",
            "relativePath": "a116-vertical-incubation-skill-live-2026-08-20.json",
            "sha256": SOURCE_DIGESTS[
                "a116-vertical-incubation-skill-live-2026-08-20.json"
            ],
            "packetRoles": ["outcomeEvidence", "referenceEvidence"],
        },
    ]
    Draft202012Validator(assets["schemas/source-import.schema.json"]).validate(
        source_manifest
    )


def test_blueprint_keeps_three_non_mandatory_direction_families(assets):
    blueprint = assets["fixtures/golden-gift-li-culture-v1/case-blueprint.json"]
    assert blueprint["taskLevels"] == ["L1", "L2"]
    assert blueprint["diagnosticOnly"] is True
    assert [item["title"] for item in blueprint["directionFamilies"]] == [
        "礼仪知识与传统/现代冲突",
        "人情往来、关系判断与生活情境",
        "礼品选择、赠礼风险与品牌商业承接",
    ]
    assert all(
        item["mandatoryRoot"] is False for item in blueprint["directionFamilies"]
    )
    Draft202012Validator(assets["schemas/case-bundle.schema.json"]).validate(blueprint)


def test_union_schema_rejects_a_field_owned_by_another_object_kind(assets):
    contaminated_blueprint = dict(
        assets["fixtures/golden-gift-li-culture-v1/case-blueprint.json"]
    )
    contaminated_blueprint["knownFailures"] = []
    with pytest.raises(ValidationError):
        Draft202012Validator(assets["schemas/case-bundle.schema.json"]).validate(
            contaminated_blueprint
        )


def test_synthetic_arms_are_valid_and_known_failure_is_evidence_based(assets):
    synthetic_arms = [
        assets["fixtures/synthetic/arm-strong.json"],
        assets["fixtures/synthetic/arm-known-failure.json"],
    ]
    answer_validator = Draft202012Validator(assets["schemas/case-answer.schema.json"])
    for arm in synthetic_arms:
        assert arm["synthetic"] is True
        answer_validator.validate(arm)

    known_failure = synthetic_arms[1]
    assert known_failure["readiness"] == "notUsable"
    assert any(
        claim["claimType"] == "existingExperience" and claim["support"] == "unsupported"
        for claim in known_failure["claims"]
    )
    assert all(
        "A116" not in option["rationale"]
        for option in known_failure["directionOptions"]
    )


def test_wire_enums_are_frozen_exactly(assets):
    assert assets["schemas/case-answer.schema.json"]["properties"]["readiness"][
        "enum"
    ] == [
        "readyForHumanReview",
        "blockedByMissingEvidence",
        "notUsable",
    ]
    assert assets["schemas/reviewer.schema.json"]["properties"]["status"]["enum"] == [
        "qualified",
        "notQualified",
        "expired",
    ]
    blind_properties = assets["schemas/blind-review.schema.json"]["properties"]
    assert blind_properties["eligibility"]["enum"] == [
        "both",
        "aOnly",
        "bOnly",
        "neither",
    ]
    assert blind_properties["preference"]["enum"] == [
        "A",
        "B",
        "nearTie",
        "abstain",
    ]
    assert assets["schemas/batch.schema.json"]["properties"]["disposition"]["enum"] == [
        "fixtureValid",
        "iterate",
        "invalid",
    ]


def test_policies_freeze_diagnostic_rubric_without_business_pass(assets):
    rubric = assets["rubrics/golden-gift-l1-l2-rubric.json"]
    assert rubric == {
        "schemaVersion": 1,
        "rubricId": "golden-gift-l1-l2-v1",
        "dimensions": [
            "businessSubjectClarity",
            "audienceActionFit",
            "directionBreadthAndTradeoffs",
            "evidenceAndUnknownDiscipline",
            "sustainableIpPotential",
            "commercialConnectionWithoutForcedSelling",
        ],
        "severeFlags": [
            "fabricatedExistingExperience",
            "wrongBusinessSubject",
            "wrongDesiredAction",
            "singlePathPresentedAsProvenTruth",
            "notActuallyUsable",
            "rightsOrPrivacyViolation",
        ],
        "preferenceOptions": ["A", "B", "nearTie", "abstain"],
        "eligibilityOptions": ["both", "aOnly", "bOnly", "neither"],
    }
    assert assets["rubrics/fixture-qualification-policy.json"] == {
        "schemaVersion": 1,
        "policyId": "fixture-reviewer-qualification-v1",
        "diagnosticOnly": True,
        "minimumAnchorCorrect": 4,
        "maximumSevereMisses": 0,
        "minimumRepeatAgreementPermille": 1000,
        "minimumSwapAgreementPermille": 1000,
        "qualificationLifetimeDays": 7,
    }
    encoded_assets = json.dumps(assets, ensure_ascii=False, sort_keys=True)
    for forbidden in (
        "G2=PASS",
        "PASS_TO_PHASE_0B",
        '"promotionEligible": true',
        '"requiredPath"',
        '"defaultRoot"',
        '"contentRoot"',
    ):
        assert forbidden not in encoded_assets
