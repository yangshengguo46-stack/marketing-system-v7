use std::collections::BTreeSet;

use codex_ai_ip_domain::HeldOutMissionCase;
use codex_ai_ip_domain::MaterialKind;
use codex_ai_ip_domain::MissionMaterial;
use codex_ai_ip_domain::SubjectKind;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::prompt::render_evaluation_context;
use crate::ADDITIONAL_CONTEXT_KEY;
use crate::EVALUATION_CONTEXT_MAX_TOKENS;
use crate::ROOT_PROMPT_MAX_TOKENS;
use crate::RuntimePromptError;
use crate::approx_token_count;
use crate::evaluation_context;
use crate::root_prompt;

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

fn mission_over_inner_limit() -> HeldOutMissionCase {
    let mut mission = valid_case();
    mission.objective = "x".repeat(2_500);
    mission
}

#[test]
fn root_and_both_context_layers_are_bounded_before_submission() {
    assert!(approx_token_count(root_prompt()) <= ROOT_PROMPT_MAX_TOKENS);
    let rendered = evaluation_context(&valid_case()).unwrap();
    assert!(approx_token_count(&rendered) <= EVALUATION_CONTEXT_MAX_TOKENS);
    let envelope: Value = serde_json::from_str(&rendered).unwrap();
    let keys = envelope
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    assert_eq!(keys, BTreeSet::from(["missionCase", "task"]));
}

#[test]
fn oversized_mission_is_rejected_before_envelope_serialization() {
    assert!(matches!(
        evaluation_context(&mission_over_inner_limit()),
        Err(RuntimePromptError::MissionTooLarge {
            max_tokens: 640,
            ..
        })
    ));
}

#[test]
fn outer_envelope_has_an_independent_limit() {
    let mission = json!({"objective": "x".repeat(3_600)});
    assert!(matches!(
        render_evaluation_context(mission, 640),
        Err(RuntimePromptError::EvaluationContextTooLarge {
            max_tokens: 900,
            ..
        })
    ));
}

#[test]
fn approximate_token_count_rounds_bytes_up() {
    assert_eq!(
        [0, 1, 1, 2],
        ["", "x", "xxxx", "xxxxx"].map(approx_token_count)
    );
}

#[test]
fn evaluation_context_uses_the_canonical_task_and_camel_case_mission_fields() {
    let rendered = evaluation_context(&valid_case()).unwrap();
    let envelope: Value = serde_json::from_str(&rendered).unwrap();

    assert_eq!(
        envelope,
        json!({
            "task": "基于 missionCase 与其中的相对路径材料，交付一份可直接拍摄或发布的短内容成果。先按需检查材料；只输出符合给定 JSON Schema 的对象；不要声称已经执行发布。",
            "missionCase": {
                "caseId": "case-1",
                "objective": "neutral objective",
                "subjectKind": "brand",
                "constraints": ["neutral constraint"],
                "materials": [{
                    "materialId": "material-1",
                    "relativePath": "evidence/a.txt",
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "materialKind": "evidence",
                }],
            },
        })
    );
}

#[test]
fn additional_context_key_is_stable() {
    assert_eq!(ADDITIONAL_CONTEXT_KEY, "ai_ip_evaluation");
}

#[test]
fn invalid_mission_is_rejected_before_serialization() {
    let mut mission = valid_case();
    mission.objective.clear();

    assert!(matches!(
        evaluation_context(&mission),
        Err(RuntimePromptError::InvalidMission(_))
    ));
}
