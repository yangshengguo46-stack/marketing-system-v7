#[test]
fn reviewer_contract_assets_expose_exact_behavior() {
    let path = codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/content-package-blind-review.json"
    )
    .unwrap();
    let rubric = std::fs::read(path).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&rubric).unwrap();

    assert_eq!(value["dimensions"].as_array().unwrap().len(), 6);
    assert!(value.get("pass").is_none());
    let rubric_text = String::from_utf8(rubric).unwrap().to_ascii_lowercase();
    assert!(!rubric_text.contains("candidate"));
    assert!(!rubric_text.contains("generic"));
}

#[test]
fn reviewer_visible_rubric_rejects_every_semantic_drift() {
    let path = codex_utils_cargo_bin::find_resource!(
        "../../ai-ip-evals/rubrics/content-package-blind-review.json"
    )
    .unwrap();
    let raw = std::fs::read(path).unwrap();
    crate::contracts::validate_reviewer_rubric(&raw).unwrap();
    let canonical: serde_json::Value = serde_json::from_slice(&raw).unwrap();
    for name in [
        "dimension ID",
        "severe flag",
        "scale definition",
        "reviewer instruction",
        "missing top-level field",
        "threshold leak",
        "condition leak",
    ] {
        let mut drifted = canonical.clone();
        match name {
            "dimension ID" => drifted["dimensions"][0]["id"] = serde_json::json!("changed"),
            "severe flag" => drifted["severeFlags"][0]["id"] = serde_json::json!("changed"),
            "scale definition" => {
                drifted["scale"]["definitions"][0]["definition"] = serde_json::json!("changed")
            }
            "reviewer instruction" => {
                drifted["reviewerInstructions"][0] = serde_json::json!("changed")
            }
            "missing top-level field" => {
                drifted
                    .as_object_mut()
                    .unwrap()
                    .remove("reviewerInstructions");
            }
            "threshold leak" => drifted["threshold"] = serde_json::json!(18),
            "condition leak" => drifted["condition"] = serde_json::json!("candidate"),
            other => panic!("unknown rubric mutation {other}"),
        }
        assert!(
            crate::contracts::validate_reviewer_rubric(&serde_json::to_vec(&drifted).unwrap())
                .is_err(),
            "rubric accepted {name} drift"
        );
    }
}

fn contract_fixture(name: &str) -> Vec<u8> {
    let resource = format!("tests/fixtures/contracts/06a/{name}");
    let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
    std::fs::read(path).unwrap()
}

#[test]
fn executable_06a_schemas_accept_only_the_frozen_shapes() {
    let canonical_review = contract_fixture("canonical-review.json");
    let canonical_native_attestation = contract_fixture("canonical-native-attestation.json");
    let canonical_replay_attestation = contract_fixture("canonical-replay-attestation.json");
    let canonical_native_context = contract_fixture("canonical-native-context.json");
    let canonical_replay_context = contract_fixture("canonical-replay-context.json");
    let negative_cases: serde_json::Value =
        serde_json::from_slice(&contract_fixture("negative-cases.json")).unwrap();
    let contracts = crate::FrozenContracts::load().unwrap();

    contracts
        .validate_reviewer_submission(&canonical_review)
        .unwrap();
    contracts
        .validate_native_attestation(&canonical_native_attestation)
        .unwrap();
    contracts
        .validate_replay_attestation(&canonical_replay_attestation)
        .unwrap();
    contracts
        .validate_native_context(&canonical_native_context)
        .unwrap();
    contracts
        .validate_replay_context(&canonical_replay_context)
        .unwrap();

    for case in negative_cases.as_array().unwrap() {
        let contract = case["contract"].as_str().unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(match contract {
            "review" => &canonical_review,
            "nativeAttestation" => &canonical_native_attestation,
            "replayAttestation" => &canonical_replay_attestation,
            "nativeContext" => &canonical_native_context,
            "replayContext" => &canonical_replay_context,
            other => panic!("unknown negative contract {other}"),
        })
        .unwrap();
        apply_negative_mutation(&mut value, case["mutation"].as_str().unwrap());
        let raw = serde_json::to_vec(&value).unwrap();
        let result = match contract {
            "review" => contracts.validate_reviewer_submission(&raw).map(|_| ()),
            "nativeAttestation" => contracts.validate_native_attestation(&raw).map(|_| ()),
            "replayAttestation" => contracts.validate_replay_attestation(&raw).map(|_| ()),
            "nativeContext" => contracts.validate_native_context(&raw),
            "replayContext" => contracts.validate_replay_context(&raw),
            other => panic!("unknown negative contract {other}"),
        };
        assert!(
            result.is_err(),
            "negative case {} unexpectedly passed",
            case["name"]
        );
    }
}

fn apply_negative_mutation(value: &mut serde_json::Value, mutation: &str) {
    match mutation {
        "unknownTopLevel" => value["unknown"] = serde_json::json!(true),
        "missingSchemaVersion" => {
            value.as_object_mut().unwrap().remove("schemaVersion");
        }
        "wrongSchemaVersionType" => value["schemaVersion"] = serde_json::json!("1"),
        "wrongReviewerIdPattern" => value["reviewerId"] = serde_json::json!("x"),
        "scoreOutOfRange" => {
            value["arms"]["A"]["scores"]["businessOutcomeClarity"] = serde_json::json!(5);
        }
        "emptyReasons" => value["arms"]["A"]["reasons"] = serde_json::json!([]),
        "badTimestamp" => value["signedAt"] = serde_json::json!("not-rfc3339"),
        "flippedReviewField" => value["preferred"] = serde_json::json!("B"),
        "duplicateReviewerId" => {
            value["reviewers"][1]["reviewerId"] = value["reviewers"][0]["reviewerId"].clone();
        }
        "twoReviewers" => {
            value["reviewers"].as_array_mut().unwrap().pop();
        }
        "fourReviewers" => {
            let reviewer = value["reviewers"][2].clone();
            value["reviewers"].as_array_mut().unwrap().push(reviewer);
        }
        "flippedExperience" => {
            let current = value["reviewers"][0]["experiencedOperatorOrDirector"]
                .as_bool()
                .unwrap();
            value["reviewers"][0]["experiencedOperatorOrDirector"] = serde_json::json!(!current);
        }
        "flippedQualificationClass" => {
            value["reviewers"][0]["qualificationClass"] = serde_json::json!("changed");
        }
        "flippedDeclarationId" => {
            value["reviewers"][0]["reviewerId"] = serde_json::json!("changed-reviewer");
        }
        "wrongExecutionMode" => value["executionMode"] = serde_json::json!("wrong"),
        "wrongPaidProviderCost" => value["paidProviderCostFen"] = serde_json::json!(1),
        "wrongSyntheticOnly" => value["syntheticOnly"] = serde_json::json!(false),
        "badDigest" => value["caseSha256"] = serde_json::json!("ABC"),
        "zeroRange" => value["maxOutputTokens"] = serde_json::json!(0),
        "nativeFieldInReplay" => value["modelLabel"] = serde_json::json!("forbidden"),
        "replayFieldInNative" => value["fixtureSetManifest"] = serde_json::json!({}),
        other => panic!("unknown negative mutation {other}"),
    }
}
