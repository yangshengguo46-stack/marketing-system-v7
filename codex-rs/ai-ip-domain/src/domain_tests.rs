use pretty_assertions::assert_eq;

use crate::ActionFunnelStep;
use crate::Claim;
use crate::ClaimStatus;
use crate::ContentPackage;
use crate::HeldOutMissionCase;
use crate::InfluenceRelation;
use crate::MaterialKind;
use crate::MeasurementPlan;
use crate::MissionMaterial;
use crate::PublishableContent;
use crate::Readiness;
use crate::SubjectKind;

fn valid_case() -> HeldOutMissionCase {
    HeldOutMissionCase {
        case_id: "case-1".into(),
        objective: "neutral objective".into(),
        subject_kind: SubjectKind::Brand,
        constraints: vec!["neutral constraint".into()],
        materials: vec![MissionMaterial {
            material_id: "material-1".into(),
            relative_path: "evidence/a.txt".into(),
            sha256: "a".repeat(64),
            material_kind: MaterialKind::Evidence,
        }],
    }
}

fn valid_package(subject_kind: SubjectKind) -> ContentPackage {
    ContentPackage {
        mission_summary: "neutral summary".into(),
        influence_relation: InfluenceRelation {
            subject: "neutral subject".into(),
            subject_kind,
            audience: "neutral audience".into(),
            desired_action: "报名参加志愿活动".into(),
        },
        action_funnel: vec![ActionFunnelStep {
            audience_state: "unaware".into(),
            intended_change: "aware".into(),
            next_action: "learn more".into(),
        }],
        strategic_judgment: "neutral judgment".into(),
        publishable_content: PublishableContent {
            format: "text".into(),
            title: Some("neutral title".into()),
            body: "neutral body".into(),
            production_notes: vec!["neutral note".into()],
        },
        claims: vec![Claim {
            text: "neutral claim".into(),
            status: ClaimStatus::ModelInterpretation,
            source_refs: Vec::new(),
            result_receipt_ref: None,
        }],
        open_questions: Vec::new(),
        measurement_plan: MeasurementPlan {
            success_signal: "neutral signal".into(),
            collection_method: "neutral collection".into(),
            observation_window: "neutral window".into(),
        },
        readiness: Readiness::Draft,
    }
}

#[test]
fn mission_enums_use_camel_case_wire_values() {
    assert_eq!(
        serde_json::to_string(&SubjectKind::Person).unwrap(),
        "\"person\""
    );
    assert_eq!(
        serde_json::to_string(&SubjectKind::Brand).unwrap(),
        "\"brand\""
    );
    assert_eq!(
        serde_json::to_string(&SubjectKind::Product).unwrap(),
        "\"product\""
    );
    assert_eq!(
        serde_json::to_string(&SubjectKind::Organization).unwrap(),
        "\"organization\""
    );
    assert_eq!(
        serde_json::to_string(&SubjectKind::Hybrid).unwrap(),
        "\"hybrid\""
    );
    assert_eq!(
        serde_json::to_string(&MaterialKind::UserInput).unwrap(),
        "\"userInput\""
    );
    assert_eq!(
        serde_json::to_string(&MaterialKind::Evidence).unwrap(),
        "\"evidence\""
    );
    assert_eq!(
        serde_json::to_string(&MaterialKind::ActualResultReceipt).unwrap(),
        "\"actualResultReceipt\""
    );
    assert_eq!(
        serde_json::to_string(&MaterialKind::Other).unwrap(),
        "\"other\""
    );
}

#[test]
fn mission_rejects_unknown_json_fields() {
    let value = serde_json::json!({
        "caseId": "case-1",
        "objective": "neutral objective",
        "subjectKind": "brand",
        "constraints": [],
        "materials": [],
        "extra": true,
    });

    assert!(serde_json::from_value::<HeldOutMissionCase>(value).is_err());
}

#[test]
fn mission_accepts_values_at_each_declared_boundary() {
    let mut case = valid_case();
    case.case_id = "a".repeat(128);
    case.objective = "o".repeat(8 * 1024);
    case.constraints = vec!["c".repeat(2 * 1024); 64];
    case.materials[0].material_id = "m".repeat(128);
    case.materials[0].relative_path = "p".repeat(1024);
    case.validate().unwrap();

    let mut many_materials = valid_case();
    many_materials.materials = (0..128)
        .map(|index| MissionMaterial {
            material_id: format!("material-{index}"),
            relative_path: "evidence/a.txt".into(),
            sha256: "a".repeat(64),
            material_kind: MaterialKind::Evidence,
        })
        .collect();
    many_materials.validate().unwrap();
}

#[test]
fn mission_rejects_values_one_byte_or_item_past_each_boundary() {
    let mut case = valid_case();
    case.case_id = "a".repeat(129);
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["caseId: invalid internal id"]
    );

    let mut case = valid_case();
    case.objective = "o".repeat(8 * 1024 + 1);
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["objective: exceeds 8192 bytes"]
    );

    let mut case = valid_case();
    case.constraints = vec!["constraint".into(); 65];
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["constraints: exceeds 64 items"]
    );

    let mut case = valid_case();
    case.constraints = vec!["c".repeat(2 * 1024 + 1)];
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["constraints[0]: exceeds 2048 bytes"]
    );

    let mut case = valid_case();
    case.materials[0].material_id = "m".repeat(129);
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["materials[0].materialId: invalid internal id"]
    );

    let mut case = valid_case();
    case.materials[0].relative_path = "p".repeat(1025);
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["materials[0].relativePath: exceeds 1024 bytes"]
    );

    let mut case = valid_case();
    case.materials = (0..129)
        .map(|index| MissionMaterial {
            material_id: format!("material-{index}"),
            relative_path: "evidence/a.txt".into(),
            sha256: "a".repeat(64),
            material_kind: MaterialKind::Evidence,
        })
        .collect();
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["materials: exceeds 128 items"]
    );
}

#[test]
fn mission_rejects_empty_objective_constraint_and_material_path() {
    let mut case = valid_case();
    case.objective.clear();
    case.constraints[0].clear();
    case.materials[0].relative_path.clear();
    assert_eq!(
        case.validate().unwrap_err().errors(),
        [
            "objective: must not be empty",
            "constraints[0]: must not be empty",
            "materials[0].relativePath: must not be empty",
        ]
    );
}

#[test]
fn mission_rejects_invalid_or_reserved_ids() {
    let mut case = valid_case();
    case.case_id = "-case".into();
    case.materials[0].material_id = "__ai_ip_dev__item".into();
    assert_eq!(
        case.validate().unwrap_err().errors(),
        [
            "caseId: invalid internal id",
            "materials[0].materialId: invalid internal id",
        ]
    );

    let mut case = valid_case();
    case.case_id = "__ai_ip_dev__case".into();
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["caseId: reserved development namespace"]
    );
}

#[test]
fn mission_rejects_duplicate_material_ids_and_non_lowercase_digests() {
    let mut case = valid_case();
    let mut duplicate = case.materials[0].clone();
    duplicate.relative_path = "evidence/b.txt".into();
    duplicate.sha256 = "A".repeat(64);
    case.materials.push(duplicate);
    assert_eq!(
        case.validate().unwrap_err().errors(),
        [
            "materials[1].materialId: duplicate",
            "materials[1].sha256: must be 64 lowercase hexadecimal characters",
        ]
    );
}

#[test]
fn mission_rejects_non_normalized_relative_paths() {
    for path in [
        "/secret",
        "C:\\secret",
        "C:/secret",
        "//root",
        "evidence/./a.txt",
        "evidence/../a.txt",
        "evidence//a.txt",
    ] {
        let mut case = valid_case();
        case.materials[0].relative_path = path.into();
        assert_eq!(
            case.validate().unwrap_err().errors(),
            &["materials[0].relativePath: must be a normalized relative path"],
            "path={path}"
        );
    }
}

#[test]
fn mission_reports_multiple_errors_in_catalog_order() {
    let case = HeldOutMissionCase {
        case_id: "__ai_ip_dev__case".into(),
        objective: String::new(),
        subject_kind: SubjectKind::Brand,
        constraints: vec![String::new(), "c".repeat(2 * 1024 + 1)],
        materials: vec![MissionMaterial {
            material_id: "material-1".into(),
            relative_path: "evidence/../a.txt".into(),
            sha256: "A".repeat(64),
            material_kind: MaterialKind::Evidence,
        }],
    };

    assert_eq!(
        case.validate().unwrap_err().errors(),
        [
            "caseId: reserved development namespace",
            "objective: must not be empty",
            "constraints[0]: must not be empty",
            "constraints[1]: exceeds 2048 bytes",
            "materials[0].relativePath: must be a normalized relative path",
            "materials[0].sha256: must be 64 lowercase hexadecimal characters",
        ]
    );
}

#[test]
fn content_enums_use_camel_case_wire_values() {
    assert_eq!(
        serde_json::to_string(&ClaimStatus::UserFact).unwrap(),
        "\"userFact\""
    );
    assert_eq!(
        serde_json::to_string(&ClaimStatus::ExternalEvidence).unwrap(),
        "\"externalEvidence\""
    );
    assert_eq!(
        serde_json::to_string(&ClaimStatus::ModelInterpretation).unwrap(),
        "\"modelInterpretation\""
    );
    assert_eq!(
        serde_json::to_string(&ClaimStatus::CreativeHypothesis).unwrap(),
        "\"creativeHypothesis\""
    );
    assert_eq!(
        serde_json::to_string(&ClaimStatus::Unknown).unwrap(),
        "\"unknown\""
    );
    assert_eq!(
        serde_json::to_string(&ClaimStatus::ActualResult).unwrap(),
        "\"actualResult\""
    );
    assert_eq!(
        serde_json::to_string(&Readiness::Draft).unwrap(),
        "\"draft\""
    );
    assert_eq!(
        serde_json::to_string(&Readiness::ReadyForHumanReview).unwrap(),
        "\"readyForHumanReview\""
    );
    assert_eq!(
        serde_json::to_string(&Readiness::BlockedByMissingEvidence).unwrap(),
        "\"blockedByMissingEvidence\""
    );
}

#[test]
fn content_rejects_unknown_json_fields() {
    let mut value = serde_json::to_value(valid_package(SubjectKind::Brand)).unwrap();
    value["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ContentPackage>(value).is_err());
}

#[test]
fn content_rejects_each_empty_required_string() {
    let mut package = valid_package(SubjectKind::Brand);
    package.mission_summary.clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["missionSummary: must not be empty"]
    );

    let mut package = valid_package(SubjectKind::Brand);
    package.influence_relation.subject.clear();
    package.influence_relation.audience.clear();
    package.influence_relation.desired_action.clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        [
            "influenceRelation.subject: must not be empty",
            "influenceRelation.audience: must not be empty",
            "influenceRelation.desiredAction: must not be empty",
        ]
    );

    let mut package = valid_package(SubjectKind::Brand);
    package.action_funnel[0].audience_state.clear();
    package.action_funnel[0].intended_change.clear();
    package.action_funnel[0].next_action.clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        [
            "actionFunnel[0].audienceState: must not be empty",
            "actionFunnel[0].intendedChange: must not be empty",
            "actionFunnel[0].nextAction: must not be empty",
        ]
    );

    let mut package = valid_package(SubjectKind::Brand);
    package.strategic_judgment.clear();
    package.publishable_content.format.clear();
    package.publishable_content.title = Some(String::new());
    package.publishable_content.body.clear();
    package.publishable_content.production_notes[0].clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        [
            "strategicJudgment: must not be empty",
            "publishableContent.format: must not be empty",
            "publishableContent.title: must not be empty when present",
            "publishableContent.body: must not be empty",
            "publishableContent.productionNotes[0]: must not be empty",
        ]
    );

    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].text.clear();
    package.claims[0].source_refs = vec![String::new()];
    package.open_questions = vec![String::new()];
    package.measurement_plan.success_signal.clear();
    package.measurement_plan.collection_method.clear();
    package.measurement_plan.observation_window.clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        [
            "claims[0].text: must not be empty",
            "claims[0].sourceRefs[0]: must not be empty",
            "openQuestions[0]: must not be empty",
            "measurementPlan.successSignal: must not be empty",
            "measurementPlan.collectionMethod: must not be empty",
            "measurementPlan.observationWindow: must not be empty",
        ]
    );
}

#[test]
fn content_requires_a_nonempty_action_funnel() {
    let mut package = valid_package(SubjectKind::Brand);
    package.action_funnel.clear();
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["actionFunnel: must contain at least one step"]
    );
}

#[test]
fn organization_subject_can_target_a_membership_action() {
    let package = valid_package(SubjectKind::Organization);
    assert_eq!(
        package.influence_relation.desired_action,
        "报名参加志愿活动"
    );
    package.validate().unwrap();
}

#[test]
fn user_fact_requires_user_input_source() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::UserFact;
    package.claims[0].source_refs.clear();
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].sourceRefs: userFact requires at least one source"]
    );
}

#[test]
fn external_evidence_without_source_is_rejected() {
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ExternalEvidence;
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["claims[0].sourceRefs: externalEvidence requires at least one source"]
    );
}

#[test]
fn actual_result_without_receipt_is_rejected() {
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ActualResult;
    package.claims[0].source_refs = vec!["material-1".into()];
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["claims[0].resultReceiptRef: actualResult requires a nonempty receipt"]
    );
}

#[test]
fn claim_reference_must_resolve_inside_the_same_case() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ExternalEvidence;
    package.claims[0].source_refs = vec!["other-case:material-1".into()];
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].sourceRefs[0]: unknown material id"]
    );
}

#[test]
fn actual_result_receipt_must_have_the_declared_receipt_kind() {
    let mut case = valid_case();
    case.materials[0].material_kind = MaterialKind::Evidence;
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ActualResult;
    package.claims[0].source_refs = vec!["material-1".into()];
    package.claims[0].result_receipt_ref = Some("material-1".into());
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].resultReceiptRef: requires actualResultReceipt material"]
    );
}

#[test]
fn content_package_accepts_same_case_evidence_references() {
    let mut case = valid_case();
    case.materials = vec![
        MissionMaterial {
            material_id: "user-input-1".into(),
            relative_path: "inputs/a.txt".into(),
            sha256: "a".repeat(64),
            material_kind: MaterialKind::UserInput,
        },
        MissionMaterial {
            material_id: "evidence-1".into(),
            relative_path: "evidence/b.txt".into(),
            sha256: "b".repeat(64),
            material_kind: MaterialKind::Evidence,
        },
        MissionMaterial {
            material_id: "result-source-1".into(),
            relative_path: "results/a.txt".into(),
            sha256: "c".repeat(64),
            material_kind: MaterialKind::Other,
        },
        MissionMaterial {
            material_id: "receipt-1".into(),
            relative_path: "receipts/a.txt".into(),
            sha256: "d".repeat(64),
            material_kind: MaterialKind::ActualResultReceipt,
        },
    ];
    let mut package = valid_package(SubjectKind::Brand);
    package.claims = vec![
        Claim {
            text: "neutral user fact".into(),
            status: ClaimStatus::UserFact,
            source_refs: vec!["user-input-1".into()],
            result_receipt_ref: None,
        },
        Claim {
            text: "neutral evidence".into(),
            status: ClaimStatus::ExternalEvidence,
            source_refs: vec!["evidence-1".into()],
            result_receipt_ref: None,
        },
        Claim {
            text: "neutral result".into(),
            status: ClaimStatus::ActualResult,
            source_refs: vec!["result-source-1".into()],
            result_receipt_ref: Some("receipt-1".into()),
        },
    ];

    package.validate_against(&case).unwrap();
}

#[test]
fn held_out_material_paths_cannot_escape_case_root() {
    let mut case = valid_case();
    case.materials[0].relative_path = "evidence/../a.txt".into();
    assert_eq!(
        case.validate().unwrap_err().errors(),
        &["materials[0].relativePath: must be a normalized relative path"]
    );
}

#[test]
fn claim_references_require_their_declared_material_kinds() {
    let case = valid_case();
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::UserFact;
    package.claims[0].source_refs = vec!["material-1".into()];
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].sourceRefs[0]: userFact requires userInput material"]
    );

    let mut case = valid_case();
    case.materials[0].material_kind = MaterialKind::UserInput;
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].status = ClaimStatus::ExternalEvidence;
    package.claims[0].source_refs = vec!["material-1".into()];
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["claims[0].sourceRefs[0]: externalEvidence requires evidence material"]
    );
}

#[test]
fn only_actual_results_may_set_a_receipt() {
    let mut package = valid_package(SubjectKind::Brand);
    package.claims[0].result_receipt_ref = Some("material-1".into());
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["claims[0].resultReceiptRef: only actualResult may set a receipt"]
    );
}

#[test]
fn content_requires_matching_subject_kind() {
    let case = valid_case();
    let package = valid_package(SubjectKind::Organization);
    assert_eq!(
        package.validate_against(&case).unwrap_err().errors(),
        &["influenceRelation.subjectKind: does not match mission case"]
    );
}

#[test]
fn readiness_boundaries_are_explicit() {
    let mut package = valid_package(SubjectKind::Brand);
    package.readiness = Readiness::ReadyForHumanReview;
    package.open_questions = vec!["neutral open question".into()];
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["readiness: readyForHumanReview cannot contain missing-evidence markers"]
    );
    package.open_questions.clear();
    package.readiness = Readiness::BlockedByMissingEvidence;
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["readiness: blockedByMissingEvidence requires a missing-evidence marker"]
    );

    package.claims[0].status = ClaimStatus::Unknown;
    package.readiness = Readiness::ReadyForHumanReview;
    assert_eq!(
        package.validate().unwrap_err().errors(),
        &["readiness: readyForHumanReview cannot contain missing-evidence markers"]
    );

    package.readiness = Readiness::BlockedByMissingEvidence;
    package.validate().unwrap();

    package.readiness = Readiness::Draft;
    package.open_questions = vec!["neutral open question".into()];
    package.validate().unwrap();
}

#[test]
fn content_reports_aggregate_errors_in_catalog_order() {
    let mut package = valid_package(SubjectKind::Brand);
    package.mission_summary.clear();
    package.influence_relation.subject.clear();
    package.action_funnel.clear();
    package.strategic_judgment.clear();
    package.publishable_content.body.clear();
    package.claims[0].status = ClaimStatus::ActualResult;
    package.claims[0].text.clear();
    package.claims[0].source_refs.clear();
    package.open_questions = vec![String::new()];
    package.measurement_plan.success_signal.clear();
    package.readiness = Readiness::ReadyForHumanReview;
    assert_eq!(
        package.validate().unwrap_err().errors(),
        [
            "missionSummary: must not be empty",
            "influenceRelation.subject: must not be empty",
            "actionFunnel: must contain at least one step",
            "strategicJudgment: must not be empty",
            "publishableContent.body: must not be empty",
            "claims[0].text: must not be empty",
            "openQuestions[0]: must not be empty",
            "measurementPlan.successSignal: must not be empty",
            "claims[0].sourceRefs: actualResult requires at least one source",
            "claims[0].resultReceiptRef: actualResult requires a nonempty receipt",
            "readiness: readyForHumanReview cannot contain missing-evidence markers",
        ]
    );
}
