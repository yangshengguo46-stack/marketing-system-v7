#[test]
fn cost_contract_assets_expose_exact_launch_shapes() {
    let load_schema = |name: &str| -> serde_json::Value {
        let resource = format!("../../ai-ip-evals/schemas/{name}.schema.json");
        let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let strict = |schema: &serde_json::Value| {
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
    };

    let rate_card = load_schema("provider-rate-card");
    strict(&rate_card);
    assert_eq!(rate_card["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(rate_card["properties"]["currency"]["const"], "CNY");
    assert_eq!(
        rate_card["properties"]["rateUnit"]["const"],
        "fenPerMillionTokens"
    );

    let billing_policy = load_schema("billing-policy");
    strict(&billing_policy);
    assert_eq!(billing_policy["properties"]["currency"]["const"], "CNY");
    assert_eq!(
        billing_policy["properties"]["reasoningTokensBilledSeparately"]["const"],
        false
    );
    assert_eq!(
        billing_policy["properties"]["supplierActualPrecedence"]["const"],
        "maxEstimatedOrSupplierActual"
    );
    assert_eq!(
        billing_policy["properties"]["rounding"]["const"],
        "ceilingToFen"
    );

    let fx_policy = load_schema("fx-policy");
    strict(&fx_policy);
    assert_eq!(fx_policy["properties"]["mode"]["const"], "notApplicable");
    assert_eq!(fx_policy["properties"]["sourceCurrency"]["const"], "CNY");
    assert_eq!(fx_policy["properties"]["targetCurrency"]["const"], "CNY");
    assert_eq!(fx_policy["properties"]["numerator"]["const"], 1);
    assert_eq!(fx_policy["properties"]["denominator"]["const"], 1);

    let budget = load_schema("provider-budget-evidence");
    strict(&budget);
    assert_eq!(budget["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(budget["properties"]["currency"]["const"], "CNY");

    let supplier_statement = load_schema("supplier-statement");
    strict(&supplier_statement);
    assert_eq!(supplier_statement["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(supplier_statement["properties"]["currency"]["const"], "CNY");

    let receipt = load_schema("cost-receipt");
    strict(&receipt);
    for field in [
        "pairId",
        "executionContextSha256",
        "attemptLedgerSha256",
        "executionMode",
        "currency",
        "fx",
        "ceilings",
    ] {
        assert!(
            receipt["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|required| required == field),
            "receipt must require {field}"
        );
    }
    assert_eq!(receipt["properties"]["executionMode"]["const"], "live");
    assert_eq!(receipt["properties"]["currency"]["const"], "CNY");
    assert_eq!(receipt["properties"]["fx"]["additionalProperties"], false);
    assert_eq!(
        receipt["properties"]["fx"]["properties"]["mode"]["const"],
        "notApplicable"
    );
    assert_eq!(receipt["properties"]["ceilings"]["additionalProperties"], false);
}

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
        "badCommitmentKeyDigest" => value["commitmentKeySha256"] = serde_json::json!("ABC"),
        "missingCommitmentKeyDigest" => {
            value.as_object_mut().unwrap().remove("commitmentKeySha256");
        }
        "missingPublicRunId" => {
            value.as_object_mut().unwrap().remove("publicRunId");
        }
        "zeroRange" => value["maxOutputTokens"] = serde_json::json!(0),
        "nativeFieldInReplay" => value["modelLabel"] = serde_json::json!("forbidden"),
        "replayFieldInNative" => value["fixtureSetManifest"] = serde_json::json!({}),
        other => panic!("unknown negative mutation {other}"),
    }
}

struct LaterSchemaCase {
    name: &'static str,
    top_level_fields: &'static [&'static str],
    nested_fields: &'static [(&'static str, &'static [&'static str])],
}

#[test]
fn later_work_package_schemas_have_strict_positive_and_negative_fixtures() {
    let mut cases = [
        LaterSchemaCase {
            name: "cost-receipt",
            top_level_fields: &[
                "schemaVersion",
                "pairId",
                "frozenRunContextSha256",
                "executionContextSha256",
                "executionManifestSha256",
                "brokerReceiptSha256",
                "pairReceiptSha256",
                "attemptLedgerSha256",
                "condition",
                "runOrdinal",
                "executionMode",
                "attemptIndexRootSha256",
                "attemptRange",
                "providerLabel",
                "actualModelRevision",
                "rateCardSha256",
                "billingPolicyCommitment",
                "fxPolicySha256",
                "providerBudgetEvidenceSha256",
                "providerRequestAttemptCount",
                "providerCompletedResponseCount",
                "usageScope",
                "usage",
                "currency",
                "rateEffectiveAt",
                "fx",
                "ceilings",
                "calculatedAt",
                "calculation",
                "estimatedFen",
                "supplierStatementSha256",
                "supplierActualFen",
                "chargedFen",
                "withinCeilings",
            ],
            nested_fields: &[
                ("/attemptRange", &["startInclusive", "endExclusive"]),
                (
                    "/usage",
                    &[
                        "totalTokens",
                        "inputTokens",
                        "cachedInputTokens",
                        "cacheWriteInputTokens",
                        "outputTokens",
                        "reasoningOutputTokens",
                    ],
                ),
                (
                    "/calculation",
                    &["rateUnit", "rounding", "reasoningTokensBilledSeparately"],
                ),
                ("/fx", &["mode", "numerator", "denominator"]),
                (
                    "/ceilings",
                    &[
                        "approvedPerRunFen",
                        "approvedTotalFen",
                        "prepaidOrHardLimitFen",
                        "maxProviderRequestAttempts",
                        "maxTotalTokens",
                        "maxElapsedSeconds",
                    ],
                ),
            ],
        },
        LaterSchemaCase {
            name: "business-report",
            top_level_fields: &[
                "schemaVersion",
                "publicRunId",
                "decision",
                "forkSha",
                "codexBinarySha256",
                "evaluatorBinarySha256",
                "brokerComponentSha256",
                "modelLabel",
                "providerLabel",
                "providerCompatibilityName",
                "providerRole",
                "frozenRunContextCommitment",
                "executionContextCommitment",
                "attemptIndexRootCommitment",
                "providerEndpointCommitment",
                "privateCaseCommitment",
                "privateMaterialsCommitment",
                "sharedConfigSha256",
                "promptSha256",
                "additionalContextCommitment",
                "outputSchemaSha256",
                "normalizedThreadStartSha256",
                "normalizedTurnStartCommitment",
                "genericCatalogSha256",
                "candidateCatalogSha256",
                "candidateSkillUseVerified",
                "skillUseEvidenceCommitment",
                "pairManifestsCommitment",
                "attestationCommitment",
                "armOrderCommitment",
                "rateCardSha256",
                "reviewSubmissionsCommitment",
                "proofRootSha256",
                "rubricSha256",
                "reviewerCount",
                "experiencedOperatorOrDirectorCount",
                "candidatePreferenceCount",
                "candidateReadyForHumanReviewCount",
                "medianGenericScore",
                "medianCandidateScore",
                "medianPairedDelta",
                "candidateSevereFailureCount",
                "genericUsage",
                "candidateUsage",
                "genericCostFen",
                "candidateCostFen",
                "providerRequestAttemptCounts",
                "providerCompletedResponseCounts",
                "usageScope",
                "privateEvidenceRetentionDeadline",
                "capabilityStatus",
                "retentionStatus",
                "sourceMaterialRetention",
                "retentionCloseoutReceiptPath",
                "generatedAt",
            ],
            nested_fields: &[
                (
                    "/genericUsage",
                    &[
                        "totalTokens",
                        "inputTokens",
                        "cachedInputTokens",
                        "cacheWriteInputTokens",
                        "outputTokens",
                        "reasoningOutputTokens",
                    ],
                ),
                (
                    "/candidateUsage",
                    &[
                        "totalTokens",
                        "inputTokens",
                        "cachedInputTokens",
                        "cacheWriteInputTokens",
                        "outputTokens",
                        "reasoningOutputTokens",
                    ],
                ),
                (
                    "/capabilityStatus",
                    &[
                        "codePresent",
                        "mechanicalContracts",
                        "liveProviderReachable",
                        "businessBlindReview",
                        "publicationRetro",
                    ],
                ),
            ],
        },
        LaterSchemaCase {
            name: "report-index",
            top_level_fields: &["schemaVersion", "attempts"],
            nested_fields: &[(
                "/attempts/0",
                &[
                    "publicRunId",
                    "reportPath",
                    "reportSha256",
                    "decision",
                    "selected",
                    "generatedAt",
                ],
            )],
        },
        LaterSchemaCase {
            name: "attempt-index",
            top_level_fields: &[
                "schemaVersion",
                "recordType",
                "status",
                "pairId",
                "frozenRunContextSha256",
                "executionContextSha256",
                "globalAttemptIndex",
                "runOrdinal",
                "condition",
                "armAttemptIndex",
                "requestStartedAt",
                "requestCommitment",
                "normalizedRequestCommitment",
                "normalizedBaseCommitment",
                "treatmentDiffCommitment",
                "threadCommitment",
                "windowCommitment",
                "parentThreadCommitment",
                "deadline",
                "maxOutputTokens",
            ],
            nested_fields: &[],
        },
        LaterSchemaCase {
            name: "verification",
            top_level_fields: &[
                "schemaVersion",
                "verificationKind",
                "frozenRunContextSha256",
                "subjectSha256",
                "nextIndexSha256",
                "publicRunId",
                "verifierBinarySha256",
                "verifiedAt",
                "valid",
            ],
            nested_fields: &[],
        },
        LaterSchemaCase {
            name: "retention-closeout",
            top_level_fields: &[
                "schemaVersion",
                "pairCommitment",
                "reportCommitment",
                "proofRootSha256",
                "inventoryCommitment",
                "scheduledAt",
                "deletedAt",
                "status",
                "operator",
                "proofCopiesDeleted",
                "externalUserSourcesRetained",
                "method",
                "physicalSecureErasureGuaranteed",
                "failureReasons",
            ],
            nested_fields: &[],
        },
    ];

    cases.sort_by_key(|case| usize::from(case.name != "attempt-index"));
    for case in cases {
        let schema_resource = format!("../../ai-ip-evals/schemas/{}.schema.json", case.name);
        let schema_path = codex_utils_cargo_bin::find_resource!(schema_resource).unwrap();
        let schema_raw = std::fs::read(schema_path).unwrap();
        let schema: serde_json::Value = serde_json::from_slice(&schema_raw).unwrap();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert_eq!(
            schema["$id"],
            format!("https://openai.local/ai-ip/{}.schema.json", case.name)
        );
        let validator = crate::contracts::compile_schema(&schema_raw).unwrap();

        let canonical_raw = later_contract_fixture(case.name, "canonical");
        let canonical = crate::contracts::validate_instance(&validator, &canonical_raw).unwrap();
        assert_exact_object_fields(&canonical, "", case.top_level_fields);
        for (pointer, fields) in case.nested_fields {
            assert_exact_object_fields(&canonical, pointer, fields);
        }

        for variant in ["unknown", "missing", "wrong-type"] {
            let negative_raw = later_contract_fixture(case.name, variant);
            let negative: serde_json::Value = serde_json::from_slice(&negative_raw).unwrap();
            assert_eq!(
                mutation_count(&canonical, &negative),
                1,
                "{}.{} is not a one-field mutation",
                case.name,
                variant
            );
            assert!(
                crate::contracts::validate_instance(&validator, &negative_raw).is_err(),
                "{}.{} unexpectedly passed",
                case.name,
                variant
            );
        }

        assert_canonical_objects_are_exactly_closed(&schema, &validator, &canonical);
        assert_later_schema_semantics(case.name, &schema, &validator, &canonical);
    }
}

fn later_contract_fixture(name: &str, variant: &str) -> Vec<u8> {
    let resource = format!("tests/fixtures/contracts/later/{name}.{variant}.json");
    let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
    std::fs::read(path).unwrap()
}

fn assert_exact_object_fields(value: &serde_json::Value, pointer: &str, expected: &[&str]) {
    let mut actual = value
        .pointer(pointer)
        .unwrap_or_else(|| panic!("missing canonical object at {pointer}"))
        .as_object()
        .unwrap_or_else(|| panic!("canonical value at {pointer} is not an object"))
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "unexpected canonical fields at {pointer}");
}

fn mutation_count(left: &serde_json::Value, right: &serde_json::Value) -> usize {
    match (left, right) {
        (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
            let shared = left
                .iter()
                .filter_map(|(key, value)| right.get(key).map(|other| mutation_count(value, other)))
                .sum::<usize>();
            shared
                + left.keys().filter(|key| !right.contains_key(*key)).count()
                + right.keys().filter(|key| !left.contains_key(*key)).count()
        }
        (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
            left.iter()
                .zip(right)
                .map(|(value, other)| mutation_count(value, other))
                .sum::<usize>()
                + left.len().abs_diff(right.len())
        }
        _ => usize::from(left != right),
    }
}

fn assert_canonical_objects_are_exactly_closed(
    schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
) {
    assert_closed_value(schema, validator, canonical, schema, canonical, "");
}

fn assert_closed_value(
    root_schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    full_document: &serde_json::Value,
    schema_node: &serde_json::Value,
    instance: &serde_json::Value,
    pointer: &str,
) {
    let schema_node = select_schema_node(root_schema, schema_node, instance);
    match instance {
        serde_json::Value::Object(object) => {
            assert_eq!(
                schema_node.get("additionalProperties"),
                Some(&serde_json::Value::Bool(false)),
                "object schema is not closed at {pointer}"
            );
            let properties = schema_node["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("object schema has no properties at {pointer}"));
            let mut property_fields = properties.keys().map(String::as_str).collect::<Vec<_>>();
            property_fields.sort_unstable();
            let mut required_fields = schema_node["required"]
                .as_array()
                .unwrap_or_else(|| panic!("object schema has no required set at {pointer}"))
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>();
            required_fields.sort_unstable();
            assert_eq!(
                required_fields, property_fields,
                "required/properties drift at {pointer}"
            );
            let mut actual_fields = object.keys().map(String::as_str).collect::<Vec<_>>();
            actual_fields.sort_unstable();
            assert_eq!(
                actual_fields, property_fields,
                "canonical object fields drift at {pointer}"
            );

            for field in &required_fields {
                let mut missing = full_document.clone();
                object_at_pointer_mut(&mut missing, pointer).remove(*field);
                assert_invalid_value(
                    validator,
                    &missing,
                    &format!("missing required {pointer}/{field}"),
                );
            }
            let mut unknown = full_document.clone();
            object_at_pointer_mut(&mut unknown, pointer)
                .insert("unexpected".to_string(), serde_json::json!(false));
            assert_invalid_value(validator, &unknown, &format!("unknown field at {pointer}"));

            for (field, child) in object {
                assert_closed_value(
                    root_schema,
                    validator,
                    full_document,
                    &properties[field],
                    child,
                    &join_pointer(pointer, field),
                );
            }
        }
        serde_json::Value::Array(values) => {
            let items = schema_node
                .get("items")
                .unwrap_or_else(|| panic!("array schema has no items at {pointer}"));
            for (index, child) in values.iter().enumerate() {
                assert_closed_value(
                    root_schema,
                    validator,
                    full_document,
                    items,
                    child,
                    &join_pointer(pointer, &index.to_string()),
                );
            }
        }
        _ => {}
    }
}

fn select_schema_node<'a>(
    root_schema: &'a serde_json::Value,
    schema_node: &'a serde_json::Value,
    instance: &serde_json::Value,
) -> &'a serde_json::Value {
    let mut selected = resolve_local_ref(root_schema, schema_node);
    loop {
        let Some(branches) = selected.get("oneOf").and_then(serde_json::Value::as_array) else {
            return selected;
        };
        let matches = branches
            .iter()
            .filter(|branch| schema_branch_matches(root_schema, branch, instance))
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "oneOf did not select one canonical branch"
        );
        selected = resolve_local_ref(root_schema, matches[0]);
    }
}

fn resolve_local_ref<'a>(
    root_schema: &'a serde_json::Value,
    schema_node: &'a serde_json::Value,
) -> &'a serde_json::Value {
    let mut resolved = schema_node;
    while let Some(reference) = resolved.get("$ref").and_then(serde_json::Value::as_str) {
        let pointer = reference
            .strip_prefix('#')
            .unwrap_or_else(|| panic!("non-local schema reference {reference}"));
        resolved = root_schema
            .pointer(pointer)
            .unwrap_or_else(|| panic!("missing local schema reference {reference}"));
    }
    resolved
}

fn schema_branch_matches(
    root_schema: &serde_json::Value,
    branch: &serde_json::Value,
    instance: &serde_json::Value,
) -> bool {
    let branch = resolve_local_ref(root_schema, branch);
    if let Some(expected) = branch.get("const") {
        return expected == instance;
    }
    if let Some(types) = branch.get("type") {
        let matches_type = match types {
            serde_json::Value::String(kind) => json_type_matches(kind, instance),
            serde_json::Value::Array(kinds) => kinds
                .iter()
                .filter_map(serde_json::Value::as_str)
                .any(|kind| json_type_matches(kind, instance)),
            _ => false,
        };
        if !matches_type {
            return false;
        }
    }
    branch
        .get("properties")
        .and_then(serde_json::Value::as_object)
        .is_none_or(|properties| {
            properties.iter().all(|(field, property)| {
                property
                    .get("const")
                    .is_none_or(|expected| instance.get(field) == Some(expected))
            })
        })
}

fn json_type_matches(kind: &str, instance: &serde_json::Value) -> bool {
    match kind {
        "null" => instance.is_null(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "string" => instance.is_string(),
        "integer" => instance.as_i64().is_some() || instance.as_u64().is_some(),
        "number" => instance.is_number(),
        "boolean" => instance.is_boolean(),
        other => panic!("unsupported JSON Schema type {other}"),
    }
}

fn object_at_pointer_mut<'a>(
    document: &'a mut serde_json::Value,
    pointer: &str,
) -> &'a mut serde_json::Map<String, serde_json::Value> {
    document
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("missing object pointer {pointer}"))
        .as_object_mut()
        .unwrap_or_else(|| panic!("pointer is not an object {pointer}"))
}

fn join_pointer(pointer: &str, token: &str) -> String {
    let token = token.replace('~', "~0").replace('/', "~1");
    format!("{pointer}/{token}")
}

fn assert_valid_value(validator: &jsonschema::Validator, value: &serde_json::Value, name: &str) {
    let raw = serde_json::to_vec(value).unwrap();
    crate::contracts::validate_instance(validator, &raw)
        .unwrap_or_else(|error| panic!("{name} unexpectedly failed: {error}"));
}

fn assert_invalid_value(validator: &jsonschema::Validator, value: &serde_json::Value, name: &str) {
    let raw = serde_json::to_vec(value).unwrap();
    assert!(
        crate::contracts::validate_instance(validator, &raw).is_err(),
        "{name} unexpectedly passed"
    );
}

fn assert_rejected_pointer_values(
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
    mutations: &[(&str, serde_json::Value)],
) {
    for (pointer, replacement) in mutations {
        let mut mutated = canonical.clone();
        *mutated
            .pointer_mut(pointer)
            .unwrap_or_else(|| panic!("missing mutation pointer {pointer}")) = replacement.clone();
        assert_invalid_value(validator, &mutated, pointer);
    }
}

fn assert_later_schema_semantics(
    name: &str,
    schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
) {
    match name {
        "cost-receipt" => assert_cost_receipt_semantics(schema, validator, canonical),
        "business-report" => assert_business_report_semantics(schema, validator, canonical),
        "report-index" => assert_report_index_semantics(validator, canonical),
        "attempt-index" => assert_attempt_index_semantics(schema, validator, canonical),
        "verification" => assert_verification_semantics(validator, canonical),
        "retention-closeout" => assert_retention_semantics(validator, canonical),
        other => panic!("unknown later schema {other}"),
    }
}

fn assert_schema_integer_maxima(schema: &serde_json::Value, pointers: &[&str], maximum: u64) {
    let expected = serde_json::json!(maximum);
    for pointer in pointers {
        assert_eq!(
            schema.pointer(pointer),
            Some(&expected),
            "wrong or missing integer maximum at {pointer}"
        );
    }
}

fn assert_cost_receipt_semantics(
    schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
) {
    assert_schema_integer_maxima(
        schema,
        &[
            "/properties/attemptRange/properties/startInclusive/maximum",
            "/properties/attemptRange/properties/endExclusive/maximum",
            "/properties/providerRequestAttemptCount/maximum",
            "/properties/providerCompletedResponseCount/maximum",
            "/properties/estimatedFen/maximum",
            "/properties/supplierActualFen/maximum",
            "/properties/chargedFen/maximum",
        ],
        u64::MAX,
    );
    assert_schema_integer_maxima(
        schema,
        &[
            "/$defs/usage/properties/totalTokens/maximum",
            "/$defs/usage/properties/inputTokens/maximum",
            "/$defs/usage/properties/cachedInputTokens/maximum",
            "/$defs/usage/properties/cacheWriteInputTokens/maximum",
            "/$defs/usage/properties/outputTokens/maximum",
            "/$defs/usage/properties/reasoningOutputTokens/maximum",
        ],
        i64::MAX as u64,
    );

    let mut supplier = canonical.clone();
    supplier["supplierStatementSha256"] =
        serde_json::json!("3333333333333333333333333333333333333333333333333333333333333333");
    supplier["supplierActualFen"] = serde_json::json!(15);
    assert_valid_value(validator, &supplier, "supplier-statement cost branch");

    let mut statement_without_amount = supplier.clone();
    statement_without_amount["supplierActualFen"] = serde_json::Value::Null;
    assert_invalid_value(
        validator,
        &statement_without_amount,
        "supplier statement without actual amount",
    );
    let mut amount_without_statement = canonical.clone();
    amount_without_statement["supplierActualFen"] = serde_json::json!(15);
    assert_invalid_value(
        validator,
        &amount_without_statement,
        "supplier actual amount without statement",
    );

    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/frozenRunContextSha256", serde_json::json!("ABC")),
            ("/condition", serde_json::json!("control")),
            ("/runOrdinal", serde_json::json!(0)),
            ("/attemptRange/startInclusive", serde_json::json!(-1)),
            ("/providerLabel", serde_json::json!("")),
            ("/usage/totalTokens", serde_json::json!(-1)),
            (
                "/usage/totalTokens",
                serde_json::json!(9_223_372_036_854_775_808_u64),
            ),
            ("/calculatedAt", serde_json::json!("not-a-time")),
            ("/calculation/rateUnit", serde_json::json!("tokens")),
        ],
    );
}

fn assert_business_report_semantics(
    schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
) {
    assert_schema_integer_maxima(
        schema,
        &[
            "/properties/genericCostFen/maximum",
            "/properties/candidateCostFen/maximum",
            "/$defs/pairCounts/items/maximum",
        ],
        u64::MAX,
    );
    assert_schema_integer_maxima(
        schema,
        &[
            "/$defs/usage/properties/totalTokens/maximum",
            "/$defs/usage/properties/inputTokens/maximum",
            "/$defs/usage/properties/cachedInputTokens/maximum",
            "/$defs/usage/properties/cacheWriteInputTokens/maximum",
            "/$defs/usage/properties/outputTokens/maximum",
            "/$defs/usage/properties/reasoningOutputTokens/maximum",
        ],
        i64::MAX as u64,
    );

    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/experiencedOperatorOrDirectorCount", serde_json::json!(1)),
            ("/candidatePreferenceCount", serde_json::json!(1)),
            ("/candidateReadyForHumanReviewCount", serde_json::json!(1)),
            ("/medianCandidateScore", serde_json::json!(17)),
            ("/medianPairedDelta", serde_json::json!(2)),
            ("/candidateSkillUseVerified", serde_json::json!(false)),
            ("/candidateSevereFailureCount", serde_json::json!(1)),
            (
                "/capabilityStatus/liveProviderReachable",
                serde_json::json!(false),
            ),
            (
                "/capabilityStatus/businessBlindReview",
                serde_json::json!("iterate"),
            ),
            ("/forkSha", serde_json::json!("ABC")),
            (
                "/privateEvidenceRetentionDeadline",
                serde_json::json!("not-a-time"),
            ),
            ("/providerRole", serde_json::json!("other")),
            ("/medianGenericScore", serde_json::json!(25)),
            (
                "/genericUsage/totalTokens",
                serde_json::json!(9_223_372_036_854_775_808_u64),
            ),
            (
                "/candidateUsage/totalTokens",
                serde_json::json!(9_223_372_036_854_775_808_u64),
            ),
            ("/retentionStatus", serde_json::json!("complete")),
            ("/sourceMaterialRetention", serde_json::json!("deleted")),
        ],
    );

    let mut iterate = canonical.clone();
    iterate["decision"] = serde_json::json!("ITERATE_SMALLEST_LEAD_CHANGE");
    iterate["capabilityStatus"]["businessBlindReview"] = serde_json::json!("iterate");
    assert_valid_value(validator, &iterate, "iterate business report branch");
    let mut invalid = canonical.clone();
    invalid["decision"] = serde_json::json!("INVALID_PROOF");
    invalid["capabilityStatus"]["businessBlindReview"] = serde_json::json!("invalid");
    assert_valid_value(validator, &invalid, "invalid business report branch");

    iterate["capabilityStatus"]["businessBlindReview"] = serde_json::json!("passed");
    assert_invalid_value(validator, &iterate, "iterate decision with passed status");
    invalid["capabilityStatus"]["businessBlindReview"] = serde_json::json!("passed");
    assert_invalid_value(validator, &invalid, "invalid decision with passed status");
}

fn assert_report_index_semantics(validator: &jsonschema::Validator, canonical: &serde_json::Value) {
    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/attempts/0/publicRunId", serde_json::json!("ABC")),
            (
                "/attempts/0/reportPath",
                serde_json::json!("/tmp/report.json"),
            ),
            ("/attempts/0/decision", serde_json::json!("PASS")),
            ("/attempts/0/generatedAt", serde_json::json!("not-a-time")),
            ("/attempts", serde_json::json!([])),
        ],
    );
    let attempt = canonical["attempts"][0].clone();
    let mut four_attempts = Vec::new();
    for suffix in ['a', 'b', 'c', 'd'] {
        let mut item = attempt.clone();
        let run_id = suffix.to_string().repeat(64);
        item["publicRunId"] = serde_json::json!(run_id);
        item["reportPath"] = serde_json::json!(format!(
            "docs/evidence/business-proof/{}/report.json",
            suffix.to_string().repeat(64)
        ));
        item["reportSha256"] = serde_json::json!(suffix.to_string().repeat(64));
        item["selected"] = serde_json::json!(false);
        four_attempts.push(item);
    }
    let mut too_many = canonical.clone();
    too_many["attempts"] = serde_json::Value::Array(four_attempts);
    assert_invalid_value(validator, &too_many, "four report attempts");

    let mut two_selected = canonical.clone();
    let mut second = attempt;
    second["publicRunId"] =
        serde_json::json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    second["reportPath"] = serde_json::json!(
        "docs/evidence/business-proof/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/report.json"
    );
    second["reportSha256"] =
        serde_json::json!("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
    two_selected["attempts"]
        .as_array_mut()
        .unwrap()
        .push(second);
    assert_invalid_value(validator, &two_selected, "two selected report attempts");
}

fn assert_attempt_index_semantics(
    schema: &serde_json::Value,
    validator: &jsonschema::Validator,
    canonical: &serde_json::Value,
) {
    let mut candidate = canonical.clone();
    candidate["runOrdinal"] = serde_json::json!(2);
    candidate["condition"] = serde_json::json!("candidate");
    candidate["treatmentDiffCommitment"] =
        serde_json::json!("3333333333333333333333333333333333333333333333333333333333333333");
    assert_valid_value(validator, &candidate, "candidate request record");

    let mut descendant = canonical.clone();
    descendant["globalAttemptIndex"] = serde_json::json!(1);
    descendant["armAttemptIndex"] = serde_json::json!(1);
    descendant["parentThreadCommitment"] =
        serde_json::json!("4444444444444444444444444444444444444444444444444444444444444444");
    assert_valid_value(validator, &descendant, "non-first request record");

    let mut generic_with_treatment = canonical.clone();
    generic_with_treatment["treatmentDiffCommitment"] =
        serde_json::json!("3333333333333333333333333333333333333333333333333333333333333333");
    assert_invalid_value(
        validator,
        &generic_with_treatment,
        "generic treatment commitment",
    );
    let mut candidate_without_treatment = candidate.clone();
    candidate_without_treatment["treatmentDiffCommitment"] = serde_json::Value::Null;
    assert_invalid_value(
        validator,
        &candidate_without_treatment,
        "candidate without treatment commitment",
    );

    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/recordType", serde_json::json!("terminal")),
            ("/status", serde_json::json!("queued")),
            ("/pairId", serde_json::json!("ABC")),
            ("/globalAttemptIndex", serde_json::json!(-1)),
            ("/runOrdinal", serde_json::json!(0)),
            ("/runOrdinal", serde_json::json!(3)),
            ("/condition", serde_json::json!("control")),
            ("/requestStartedAt", serde_json::json!("not-a-time")),
            ("/deadline", serde_json::json!("not-a-time")),
            ("/maxOutputTokens", serde_json::json!(0)),
        ],
    );

    let terminal = serde_json::json!({
        "schemaVersion": 1,
        "recordType": "terminal",
        "globalAttemptIndex": 0,
        "requestRecordSha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "endedAt": "2026-08-28T10:01:00Z",
        "status": "completed",
        "responseIdCommitment": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "actualModelRevision": "model-revision-2026-08-28",
        "deploymentCommitment": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
        "usage": {
            "totalTokens": 30,
            "inputTokens": 20,
            "cachedInputTokens": 5,
            "cacheWriteInputTokens": 2,
            "outputTokens": 10,
            "reasoningOutputTokens": 3
        },
        "failureClass": null
    });
    const TERMINAL_FIELDS: &[&str] = &[
        "schemaVersion",
        "recordType",
        "globalAttemptIndex",
        "requestRecordSha256",
        "endedAt",
        "status",
        "responseIdCommitment",
        "actualModelRevision",
        "deploymentCommitment",
        "usage",
        "failureClass",
    ];
    const TERMINAL_USAGE_FIELDS: &[&str] = &[
        "totalTokens",
        "inputTokens",
        "cachedInputTokens",
        "cacheWriteInputTokens",
        "outputTokens",
        "reasoningOutputTokens",
    ];
    assert_exact_object_fields(&terminal, "", TERMINAL_FIELDS);
    assert_exact_object_fields(&terminal, "/usage", TERMINAL_USAGE_FIELDS);
    assert_valid_value(validator, &terminal, "completed terminal record");
    assert_canonical_objects_are_exactly_closed(schema, validator, &terminal);

    let mut failed = terminal.clone();
    failed["status"] = serde_json::json!("failed");
    failed["failureClass"] = serde_json::json!("upstreamRejected");
    assert_valid_value(
        validator,
        &failed,
        "failed terminal with retained response evidence",
    );
    let mut failed_without_response_evidence = failed.clone();
    failed_without_response_evidence["responseIdCommitment"] = serde_json::Value::Null;
    failed_without_response_evidence["actualModelRevision"] = serde_json::Value::Null;
    failed_without_response_evidence["deploymentCommitment"] = serde_json::Value::Null;
    failed_without_response_evidence["usage"] = serde_json::Value::Null;
    assert_valid_value(
        validator,
        &failed_without_response_evidence,
        "failed terminal without response evidence",
    );
    let mut timeout = failed_without_response_evidence;
    timeout["status"] = serde_json::json!("timeout");
    timeout["failureClass"] = serde_json::json!("deadlineExceeded");
    assert_valid_value(validator, &timeout, "timeout terminal record");

    let mut completed_without_response = terminal.clone();
    completed_without_response["responseIdCommitment"] = serde_json::Value::Null;
    assert_invalid_value(
        validator,
        &completed_without_response,
        "completed terminal without response commitment",
    );
    let mut completed_without_usage = terminal.clone();
    completed_without_usage["usage"] = serde_json::Value::Null;
    assert_invalid_value(
        validator,
        &completed_without_usage,
        "completed terminal without usage",
    );
    let mut completed_with_failure = terminal.clone();
    completed_with_failure["failureClass"] = serde_json::json!("unexpectedFailure");
    assert_invalid_value(
        validator,
        &completed_with_failure,
        "completed terminal with failure class",
    );
    let mut failed_without_failure = terminal.clone();
    failed_without_failure["status"] = serde_json::json!("failed");
    assert_invalid_value(
        validator,
        &failed_without_failure,
        "failed terminal without failure class",
    );
    let mut timeout_without_failure = terminal.clone();
    timeout_without_failure["status"] = serde_json::json!("timeout");
    assert_invalid_value(
        validator,
        &timeout_without_failure,
        "timeout terminal without failure class",
    );

    assert_rejected_pointer_values(
        validator,
        &terminal,
        &[
            ("/recordType", serde_json::json!("request")),
            ("/globalAttemptIndex", serde_json::json!(-1)),
            ("/requestRecordSha256", serde_json::json!("ABC")),
            ("/endedAt", serde_json::json!("not-a-time")),
            ("/status", serde_json::json!("forwarding")),
            ("/responseIdCommitment", serde_json::json!(7)),
            ("/actualModelRevision", serde_json::json!(7)),
            ("/deploymentCommitment", serde_json::json!("ABC")),
            ("/usage/totalTokens", serde_json::json!(-1)),
            ("/failureClass", serde_json::json!(false)),
        ],
    );
}

fn assert_verification_semantics(validator: &jsonschema::Validator, canonical: &serde_json::Value) {
    let mut live_proof = canonical.clone();
    live_proof["verificationKind"] = serde_json::json!("liveProof");
    live_proof["nextIndexSha256"] = serde_json::Value::Null;
    assert_valid_value(validator, &live_proof, "live-proof verification branch");

    let mut report_without_index = canonical.clone();
    report_without_index["nextIndexSha256"] = serde_json::Value::Null;
    assert_invalid_value(
        validator,
        &report_without_index,
        "report without next index",
    );
    let mut live_proof_with_index = live_proof;
    live_proof_with_index["nextIndexSha256"] =
        serde_json::json!("cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc");
    assert_invalid_value(
        validator,
        &live_proof_with_index,
        "live proof with next index",
    );

    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/verificationKind", serde_json::json!("other")),
            ("/frozenRunContextSha256", serde_json::json!("ABC")),
            ("/verifiedAt", serde_json::json!("not-a-time")),
            ("/valid", serde_json::json!(false)),
        ],
    );
}

fn assert_retention_semantics(validator: &jsonschema::Validator, canonical: &serde_json::Value) {
    let mut failed = canonical.clone();
    failed["deletedAt"] = serde_json::Value::Null;
    failed["status"] = serde_json::json!("failed");
    failed["proofCopiesDeleted"] = serde_json::json!(false);
    failed["failureReasons"] = serde_json::json!(["filesystemDeleteFailed"]);
    assert_valid_value(validator, &failed, "failed retention branch");

    let mut succeeded_as_failed = canonical.clone();
    succeeded_as_failed["status"] = serde_json::json!("failed");
    assert_invalid_value(
        validator,
        &succeeded_as_failed,
        "inconsistent failed retention",
    );
    let mut failed_as_succeeded = failed;
    failed_as_succeeded["status"] = serde_json::json!("succeeded");
    assert_invalid_value(
        validator,
        &failed_as_succeeded,
        "inconsistent successful retention",
    );

    assert_rejected_pointer_values(
        validator,
        canonical,
        &[
            ("/proofRootSha256", serde_json::json!("ABC")),
            ("/scheduledAt", serde_json::json!("not-a-time")),
            ("/status", serde_json::json!("complete")),
            ("/operator", serde_json::json!("")),
            ("/externalUserSourcesRetained", serde_json::json!(false)),
            ("/method", serde_json::json!("secure-erase")),
            ("/physicalSecureErasureGuaranteed", serde_json::json!(true)),
        ],
    );
}
