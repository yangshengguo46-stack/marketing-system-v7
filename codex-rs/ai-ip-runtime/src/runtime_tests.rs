use std::collections::BTreeSet;

use codex_ai_ip_domain::HeldOutMissionCase;
use codex_ai_ip_domain::MaterialKind;
use codex_ai_ip_domain::MissionMaterial;
use codex_ai_ip_domain::SubjectKind;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;

use super::prompt::render_evaluation_context;
use super::schema::normalize_schema;
use crate::ADDITIONAL_CONTEXT_KEY;
use crate::EVALUATION_CONTEXT_MAX_TOKENS;
use crate::ROOT_PROMPT_MAX_TOKENS;
use crate::RuntimePromptError;
use crate::StrictSchemaError;
use crate::approx_token_count;
use crate::content_package_schema;
use crate::evaluation_context;
use crate::root_prompt;
use crate::validate_responses_strict_subset;

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

fn assert_closed_objects(schema: &Value) {
    let Some(object) = schema.as_object() else {
        return;
    };

    if object.get("type") == Some(&json!("object")) || object.contains_key("properties") {
        assert_eq!(object.get("additionalProperties"), Some(&json!(false)));
    }

    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for property in properties.values() {
            assert_closed_objects(property);
        }
    }
    if let Some(definitions) = object.get("$defs").and_then(Value::as_object) {
        for definition in definitions.values() {
            assert_closed_objects(definition);
        }
    }
    if let Some(items) = object.get("items") {
        assert_closed_objects(items);
    }
    for composition in ["anyOf", "oneOf", "allOf"] {
        if let Some(variants) = object.get(composition).and_then(Value::as_array) {
            for variant in variants {
                assert_closed_objects(variant);
            }
        }
    }
}

fn assert_required_equals_properties_recursively(schema: &Value) {
    let Some(object) = schema.as_object() else {
        return;
    };

    if object.get("type") == Some(&json!("object")) || object.contains_key("properties") {
        let properties = object["properties"].as_object().unwrap();
        let expected = properties.keys().cloned().collect::<Vec<_>>();
        assert_eq!(object.get("required"), Some(&json!(expected)));
    }

    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for property in properties.values() {
            assert_required_equals_properties_recursively(property);
        }
    }
    if let Some(definitions) = object.get("$defs").and_then(Value::as_object) {
        for definition in definitions.values() {
            assert_required_equals_properties_recursively(definition);
        }
    }
    if let Some(items) = object.get("items") {
        assert_required_equals_properties_recursively(items);
    }
    for composition in ["anyOf", "oneOf", "allOf"] {
        if let Some(variants) = object.get(composition).and_then(Value::as_array) {
            for variant in variants {
                assert_required_equals_properties_recursively(variant);
            }
        }
    }
}

fn collect_refs(schema: &Value, refs: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };

    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        refs.push(reference.to_string());
    }
    if let Some(properties) = object.get("properties").and_then(Value::as_object) {
        for property in properties.values() {
            collect_refs(property, refs);
        }
    }
    if let Some(definitions) = object.get("$defs").and_then(Value::as_object) {
        for definition in definitions.values() {
            collect_refs(definition, refs);
        }
    }
    if let Some(items) = object.get("items") {
        collect_refs(items, refs);
    }
    for composition in ["anyOf", "oneOf", "allOf"] {
        if let Some(variants) = object.get(composition).and_then(Value::as_array) {
            for variant in variants {
                collect_refs(variant, refs);
            }
        }
    }
}

fn strict_error(schema: Value) -> StrictSchemaError {
    validate_responses_strict_subset(&schema).unwrap_err()
}

#[test]
fn content_package_schema_is_closed_and_uses_required_properties() {
    let schema = content_package_schema().unwrap();

    assert!(schema.get("$schema").is_none());
    assert_closed_objects(&schema);
    assert_required_equals_properties_recursively(&schema);
}

#[test]
fn content_package_schema_keeps_nullable_fields_required() {
    let schema = content_package_schema().unwrap();

    for pointer in [
        "/$defs/PublishableContent/properties/title",
        "/$defs/Claim/properties/resultReceiptRef",
    ] {
        let property = schema.pointer(pointer).unwrap();
        assert_eq!(property["type"], json!(["string", "null"]));
        let parent = schema
            .pointer(pointer.rsplit_once("/properties/").unwrap().0)
            .unwrap();
        let property_name = pointer.rsplit_once('/').unwrap().1;
        assert!(
            parent["required"]
                .as_array()
                .unwrap()
                .contains(&json!(property_name))
        );
    }
}

#[test]
fn content_package_schema_uses_root_defs_and_resolving_local_refs() {
    let schema = content_package_schema().unwrap();
    let definitions = schema.get("$defs").and_then(Value::as_object).unwrap();
    let mut refs = Vec::new();
    collect_refs(&schema, &mut refs);

    let reference = refs
        .into_iter()
        .find(|reference| reference.starts_with("#/$defs/"))
        .unwrap();
    let token = reference.trim_start_matches("#/$defs/");
    assert!(definitions.contains_key(token));
}

#[test]
fn validator_accepts_required_nullable_property() {
    let schema = json!({
        "type": "object",
        "properties": {
            "title": {"anyOf": [{"type": "string"}, {"type": "null"}]}
        },
        "required": ["title"],
        "additionalProperties": false,
    });

    validate_responses_strict_subset(&schema).unwrap();
}

#[test]
fn validator_resolves_escaped_definition_tokens() {
    let schema = json!({
        "type": "object",
        "properties": {"x": {"$ref": "#/$defs/a~1b~0c"}},
        "required": ["x"],
        "additionalProperties": false,
        "$defs": {
            "a/b~c": {
                "type": "object",
                "properties": {"value": {"type": "string"}},
                "required": ["value"],
                "additionalProperties": false,
            }
        }
    });

    validate_responses_strict_subset(&schema).unwrap();
}

#[test]
fn normalization_descends_into_unsupported_compositions_before_validation() {
    let mut schema = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "definitions": {
            "Leaf": {"type": "object", "properties": {"value": {"type": "string"}}}
        },
        "type": "object",
        "properties": {
            "choice": {"oneOf": [
                {"type": "object", "properties": {"leaf": {"$ref": "#/definitions/Leaf"}}}
            ]},
            "combined": {"allOf": [
                {"type": "object", "properties": {"leaf": {"$ref": "#/definitions/Leaf"}}}
            ]}
        }
    });
    normalize_schema(&mut schema).unwrap();
    assert!(schema.get("$schema").is_none());
    assert!(schema.get("definitions").is_none());
    assert!(schema.get("$defs").is_some());
    for (pointer, required) in [
        ("/properties/choice/oneOf/0", json!(["leaf"])),
        ("/properties/combined/allOf/0", json!(["leaf"])),
        ("/$defs/Leaf", json!(["value"])),
    ] {
        let object = schema.pointer(pointer).unwrap();
        assert_eq!(object["additionalProperties"], false);
        assert_eq!(object["required"], required);
    }
    assert_eq!(
        schema.pointer("/properties/choice/oneOf/0/properties/leaf/$ref"),
        Some(&json!("#/$defs/Leaf"))
    );
    assert_eq!(
        schema.pointer("/properties/combined/allOf/0/properties/leaf/$ref"),
        Some(&json!("#/$defs/Leaf"))
    );
    let StrictSchemaError::Invalid { path, message } = strict_error(schema) else {
        panic!("expected invalid strict schema");
    };
    assert_eq!(
        (path, message),
        (
            "$.properties.choice".to_string(),
            "unsupported keyword oneOf".to_string(),
        ),
    );
}

#[test]
fn validator_rejects_invalid_strict_schema_fixtures() {
    let fixtures = [
        (
            "root anyOf",
            json!({
                "type": "object", "properties": {}, "required": [],
                "additionalProperties": false, "anyOf": []
            }),
            ("$", "root must be an object schema without anyOf"),
        ),
        (
            "nested schema dialect",
            json!({
                "type": "object", "properties": {"x": {"$schema": "draft"}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword $schema"),
        ),
        (
            "nested oneOf",
            json!({
                "type": "object", "properties": {"x": {"oneOf": [{"type": "string"}]}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword oneOf"),
        ),
        (
            "nested allOf",
            json!({
                "type": "object", "properties": {"x": {"allOf": [{"type": "string"}]}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword allOf"),
        ),
        (
            "nested not",
            json!({
                "type": "object", "properties": {"x": {"not": {"type": "string"}}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword not"),
        ),
        (
            "nested if",
            json!({
                "type": "object", "properties": {"x": {"if": {"type": "string"}}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword if"),
        ),
        (
            "unknown keyword",
            json!({
                "type": "object", "properties": {"x": {"default": "value"}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "unsupported keyword default"),
        ),
        (
            "properties array",
            json!({
                "type": "object", "properties": [], "required": [], "additionalProperties": false
            }),
            ("$", "properties must be an object"),
        ),
        (
            "defs array",
            json!({
                "type": "object", "properties": {}, "required": [],
                "additionalProperties": false, "$defs": []
            }),
            ("$", "$defs must be an object"),
        ),
        (
            "items array",
            json!({
                "type": "object", "properties": {"x": {"type": "array", "items": []}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "items must be an object schema"),
        ),
        (
            "anyOf empty",
            json!({
                "type": "object", "properties": {"x": {"anyOf": []}},
                "required": ["x"], "additionalProperties": false
            }),
            (
                "$.properties.x",
                "anyOf must be a nonempty array of object schemas",
            ),
        ),
        (
            "anyOf non-array",
            json!({
                "type": "object", "properties": {"x": {"anyOf": {}}},
                "required": ["x"], "additionalProperties": false
            }),
            (
                "$.properties.x",
                "anyOf must be a nonempty array of object schemas",
            ),
        ),
        (
            "missing additional properties",
            json!({"type": "object", "properties": {"x": {"type": "string"}}, "required": ["x"]}),
            ("$", "object must set additionalProperties to false"),
        ),
        (
            "true additional properties",
            json!({
                "type": "object", "properties": {"x": {"type": "string"}},
                "required": ["x"], "additionalProperties": true
            }),
            ("$", "object must set additionalProperties to false"),
        ),
        (
            "required missing property",
            json!({
                "type": "object", "properties": {"x": {"type": "string"}},
                "required": [], "additionalProperties": false
            }),
            ("$", "required must contain every property exactly once"),
        ),
        (
            "required extra property",
            json!({
                "type": "object", "properties": {"x": {"type": "string"}},
                "required": ["x", "y"], "additionalProperties": false
            }),
            ("$", "required must contain every property exactly once"),
        ),
        (
            "required duplicate property",
            json!({
                "type": "object", "properties": {"x": {"type": "string"}},
                "required": ["x", "x"], "additionalProperties": false
            }),
            ("$", "required must contain every property exactly once"),
        ),
        (
            "required non-array",
            json!({
                "type": "object", "properties": {"x": {"type": "string"}},
                "required": "x", "additionalProperties": false
            }),
            ("$", "required must contain every property exactly once"),
        ),
        (
            "nested required non-array",
            json!({
                "type": "object",
                "properties": {"x": {"type": "string", "required": "x"}},
                "required": ["x"],
                "additionalProperties": false
            }),
            (
                "$.properties.x",
                "required must contain every property exactly once",
            ),
        ),
        (
            "unsupported type",
            json!({"type": "date", "properties": {}, "required": [], "additionalProperties": false}),
            ("$", "type must contain unique supported primitive names"),
        ),
        (
            "duplicate types",
            json!({"type": ["string", "string"], "properties": {}, "required": [], "additionalProperties": false}),
            ("$", "type must contain unique supported primitive names"),
        ),
        (
            "empty type array",
            json!({"type": [], "properties": {}, "required": [], "additionalProperties": false}),
            ("$", "type must contain unique supported primitive names"),
        ),
        (
            "non-string type entry",
            json!({"type": [1], "properties": {}, "required": [], "additionalProperties": false}),
            ("$", "type must contain unique supported primitive names"),
        ),
        (
            "legacy definition reference",
            json!({
                "type": "object", "properties": {"x": {"$ref": "#/definitions/X"}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "local ref must use #/$defs/"),
        ),
        (
            "dangling local reference",
            json!({
                "type": "object", "properties": {"x": {"$ref": "#/$defs/Missing"}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "dangling local ref"),
        ),
        (
            "local reference with raw pointer separator",
            json!({
                "type": "object",
                "properties": {"x": {"$ref": "#/$defs/a/b"}},
                "required": ["x"],
                "additionalProperties": false,
                "$defs": {
                    "a/b": {
                        "type": "object",
                        "properties": {},
                        "required": [],
                        "additionalProperties": false
                    }
                }
            }),
            ("$.properties.x", "local ref must use #/$defs/"),
        ),
        (
            "invalid pointer escape",
            json!({
                "type": "object", "properties": {"x": {"$ref": "#/$defs/a~2b"}},
                "required": ["x"], "additionalProperties": false
            }),
            ("$.properties.x", "invalid JSON Pointer escape"),
        ),
    ];

    for (name, schema, expected) in fixtures {
        let StrictSchemaError::Invalid { path, message } = strict_error(schema) else {
            panic!("{name}: expected invalid strict schema");
        };
        assert_eq!(
            (path, message),
            (expected.0.to_string(), expected.1.to_string()),
            "{name}"
        );
    }
}

#[test]
fn lead_skill_asset_has_valid_native_frontmatter_and_body() {
    let path = codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-assets/skills/deliver-ai-ip-content-package/SKILL.md"
    )
    .expect("resolve lead skill asset");
    let contents = std::fs::read_to_string(path).expect("read lead skill asset");
    let metadata = codex_skills::parse_skill_frontmatter_metadata(&contents, || {
        "deliver-ai-ip-content-package".to_string()
    })
    .expect("parse lead skill frontmatter");

    assert_eq!(metadata.name, "deliver-ai-ip-content-package");
    assert_eq!(
        metadata.description,
        "Use when the user needs an evidence-aware, publishable AI IP content package tied to a real audience action."
    );
    let (_, body) = contents
        .split_once("\n---\n")
        .expect("lead skill frontmatter terminator");
    assert!(!body.trim().is_empty());
}
