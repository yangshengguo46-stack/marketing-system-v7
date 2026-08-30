use sha2::Digest;

const INPUT_CAP: usize = 64 * 1024;
const RECEIPT_CAP: usize = 128 * 1024;
const SAFE_INTEGER: u64 = 9_007_199_254_740_991;

fn fixture(name: &str) -> Vec<u8> {
    let resource = format!("tests/fixtures/contracts/06b1/{name}");
    std::fs::read(codex_utils_cargo_bin::find_resource!(resource).unwrap()).unwrap()
}

fn receipt_bytes(value: &serde_json::Value) -> Vec<u8> {
    let typed: crate::cost_contracts::CostReceiptV1 =
        serde_json::from_value(value.clone()).unwrap();
    crate::jcs::canonicalize_value(&serde_json::to_value(typed).unwrap()).unwrap()
}

#[test]
fn executable_06b1_cost_contracts_accept_only_exact_shapes() {
    let rate = fixture("provider-rate-card.canonical.json");
    let policy = fixture("billing-policy.canonical.json");
    let fx = fixture("fx-policy.canonical.json");
    let budget = fixture("provider-budget-evidence.canonical.json");
    let statement = fixture("supplier-statement.canonical.json");
    let receipt = fixture("cost-receipt.canonical.json");
    let contracts = crate::cost_contracts::FrozenCostContracts::load().unwrap();
    let verified = contracts
        .validate_inputs(&rate, &policy, &fx, &budget)
        .unwrap();
    assert_eq!(
        contracts.validate_rate_card(&rate).unwrap(),
        verified.rate_card
    );
    assert_eq!(
        contracts.validate_billing_policy(&policy).unwrap(),
        verified.billing_policy
    );
    assert_eq!(
        contracts.validate_fx_policy(&fx).unwrap(),
        verified.fx_policy
    );
    assert_eq!(
        contracts
            .validate_provider_budget_evidence(&budget)
            .unwrap(),
        verified.budget
    );
    assert_eq!(verified.rate_card.provider_label, "approved-provider");
    assert_eq!(
        verified.rate_card_sha256,
        format!("{:x}", sha2::Sha256::digest(&rate))
    );
    assert_ne!(
        verified.rate_card_sha256,
        format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&verified.rate_card).unwrap())
        )
    );
    assert_eq!(
        contracts
            .validate_supplier_statement(&statement)
            .unwrap()
            .condition,
        crate::EvaluationCondition::Candidate
    );
    assert_eq!(
        contracts.validate_receipt(&receipt).unwrap().execution_mode,
        "live"
    );

    let cases: Vec<serde_json::Value> =
        serde_json::from_slice(&fixture("negative-cases.json")).unwrap();
    for case in cases {
        let result = run_case(
            &contracts, &case, &rate, &policy, &fx, &budget, &statement, &receipt,
        );
        let expected = case["expected"].as_str().unwrap();
        if expected == "accept" {
            result
                .unwrap_or_else(|error| panic!("{} unexpectedly rejected: {error}", case["name"]));
        } else {
            let error = result.unwrap_err();
            assert!(
                error.contains(expected),
                "{} expected {expected:?}, got {error:?}",
                case["name"]
            );
        }
    }
    assert_eq!(
        contracts
            .validate_supplier_statement(&fixture("supplier-statement.condition-mismatch.json"))
            .unwrap()
            .condition,
        crate::EvaluationCondition::Generic
    );
    assert!(
        crate::cost_contracts::test_typed_deep_equality_probe()
            .unwrap_err()
            .to_string()
            .contains("typed deep equality guard rejected")
    );
}

#[test]
fn exact_jcs_receipt_semantic_mutations_are_rejected() {
    let rate = fixture("provider-rate-card.canonical.json");
    let policy = fixture("billing-policy.canonical.json");
    let fx = fixture("fx-policy.canonical.json");
    let budget = fixture("provider-budget-evidence.canonical.json");
    let receipt = fixture("cost-receipt.canonical.json");
    let contracts = crate::cost_contracts::FrozenCostContracts::load().unwrap();
    let inputs = contracts
        .validate_inputs(&rate, &policy, &fx, &budget)
        .unwrap();
    let receipt_value = crate::jcs::parse_json(&receipt).unwrap();

    for field in [
        "totalTokens",
        "inputTokens",
        "cachedInputTokens",
        "cacheWriteInputTokens",
        "outputTokens",
        "reasoningOutputTokens",
    ] {
        let mut value = receipt_value.clone();
        value["usage"][field] = serde_json::json!(-1);
        let bytes = receipt_bytes(&value);
        let typed: crate::cost_contracts::CostReceiptV1 = serde_json::from_slice(&bytes).unwrap();
        assert!(
            crate::cost::calculate_cost(&crate::cost::CostCalculationInput {
                inputs: &inputs,
                usage: &typed.usage,
                supplier_actual_fen: None,
                approved_per_run_fen: typed.ceilings.approved_per_run_fen,
                max_total_tokens: typed.ceilings.max_total_tokens,
            })
            .is_err(),
            "{field} must be rejected by the typed usage layer"
        );
        assert!(
            contracts.validate_receipt(&bytes).is_err(),
            "{field} exact-JCS receipt must be rejected"
        );
    }

    for (mutate, expected) in [
        (
            Box::new(|value: &mut serde_json::Value| {
                value["usage"]["inputTokens"] = serde_json::json!(99);
            }) as Box<dyn Fn(&mut serde_json::Value)>,
            "total_tokens must equal input_tokens plus output_tokens",
        ),
        (
            Box::new(|value: &mut serde_json::Value| {
                value["usage"]["cachedInputTokens"] = serde_json::json!(91);
            }) as Box<dyn Fn(&mut serde_json::Value)>,
            "cached and cache-write tokens cannot exceed input_tokens",
        ),
        (
            Box::new(|value: &mut serde_json::Value| {
                value["usage"]["reasoningOutputTokens"] = serde_json::json!(51);
            }) as Box<dyn Fn(&mut serde_json::Value)>,
            "reasoning_output_tokens cannot exceed output_tokens",
        ),
        (
            Box::new(|value: &mut serde_json::Value| {
                value["attemptRange"]["endExclusive"] = serde_json::json!(4);
            }) as Box<dyn Fn(&mut serde_json::Value)>,
            "receipt attempt range length does not match request attempt count",
        ),
    ] {
        let mut value = receipt_value.clone();
        mutate(&mut value);
        let error = contracts
            .validate_receipt(&receipt_bytes(&value))
            .unwrap_err();
        assert!(error.to_string().contains(expected));
    }

    let mut empty_attempt_range = receipt_value.clone();
    empty_attempt_range["attemptRange"]["endExclusive"] = serde_json::json!(1);
    empty_attempt_range["providerRequestAttemptCount"] = serde_json::json!(0);
    let error = contracts
        .validate_receipt(&receipt_bytes(&empty_attempt_range))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("receipt attempt range is not ordered")
    );

    let mut valid_overage = receipt_value;
    valid_overage["usage"]["inputTokens"] = serde_json::json!(951);
    valid_overage["usage"]["totalTokens"] = serde_json::json!(1001);
    valid_overage["withinCeilings"] = serde_json::json!(false);
    contracts
        .validate_receipt(&receipt_bytes(&valid_overage))
        .unwrap();
}

fn run_case(
    contracts: &crate::cost_contracts::FrozenCostContracts,
    case: &serde_json::Value,
    rate: &[u8],
    policy: &[u8],
    fx: &[u8],
    budget: &[u8],
    statement: &[u8],
    receipt: &[u8],
) -> Result<(), String> {
    let mutation = case["mutation"].as_str().unwrap();
    let contract = case["contract"].as_str().unwrap();
    let mut value = crate::jcs::parse_json(match contract {
        "rate" => rate,
        "policy" => policy,
        "fx" => fx,
        "budget" => budget,
        "statement" => statement,
        "receipt" => receipt,
        _ => panic!("unknown contract"),
    })
    .unwrap();
    let mut raw = None;
    match mutation {
        "unknown" => value["unknown"] = serde_json::json!(true),
        "missing" => { value.as_object_mut().unwrap().remove("currency"); }
        "wrongType" => value["numerator"] = serde_json::json!("1"),
        "label" => value["providerLabel"] = serde_json::json!(" invalid"),
        "timestamp" => value["effectiveAt"] = serde_json::json!("2026-08-30T10:00:00Z"),
        "budgetOrder" => value["validUntil"] = serde_json::json!("2026-08-30T09:00:00.000Z"),
        "nonCny" => value["currency"] = serde_json::json!("USD"),
        "providerMismatch" => value["providerLabel"] = serde_json::json!("other-provider"),
        "nonApplicable" => value["mode"] = serde_json::json!("spot"),
        "duplicate" => raw = Some(String::from_utf8(rate.to_vec()).unwrap().replacen("\"providerLabel\": \"approved-provider\",", "\"providerLabel\":\"approved-provider\",\"providerLabel\":\"approved-provider\",", 1).into_bytes()),
        "inputAtCap" | "statementAtCap" => { let base = serde_json::to_vec(&value).unwrap(); let padding = INPUT_CAP - base.len(); raw = Some([base, vec![b' '; padding]].concat()); }
        "inputOverCap" | "statementOverCap" => raw = Some(vec![b' '; INPUT_CAP + 1]),
        "nonJcs" => raw = Some(serde_json::to_vec_pretty(&value).unwrap()),
        "nullPair" => value["pairId"] = serde_json::Value::Null,
        "charged" => value["chargedFen"] = serde_json::json!(11),
        "within" => value["withinCeilings"] = serde_json::json!(false),
        "attemptCap" => value["providerRequestAttemptCount"] = serde_json::json!(3),
        "rateAfterCalculated" => value["rateEffectiveAt"] = serde_json::json!("2026-08-30T10:03:00.000Z"),
        "validOverage" => { value["usage"]["inputTokens"] = serde_json::json!(951); value["usage"]["totalTokens"] = serde_json::json!(1001); value["withinCeilings"] = serde_json::json!(false); }
        "safeInteger" => { value["estimatedFen"] = serde_json::json!(SAFE_INTEGER); value["chargedFen"] = serde_json::json!(SAFE_INTEGER); value["ceilings"]["approvedPerRunFen"] = serde_json::json!(SAFE_INTEGER); value["ceilings"]["approvedTotalFen"] = serde_json::json!(SAFE_INTEGER); value["ceilings"]["prepaidOrHardLimitFen"] = serde_json::json!(SAFE_INTEGER); }
        "unsafeInteger" => value["chargedFen"] = serde_json::json!(SAFE_INTEGER + 1),
        "unsafeIntegerPlus" => value["chargedFen"] = serde_json::json!(SAFE_INTEGER + 2),
        "i64Max" => value["chargedFen"] = serde_json::json!(i64::MAX),
        "u64Max" => value["chargedFen"] = serde_json::json!(u64::MAX),
        "receiptAtCap" => { let base = receipt_bytes(&value); let padding = RECEIPT_CAP - base.len(); raw = Some([base, vec![b' '; padding]].concat()); }
        "receiptOverCap" => raw = Some(vec![b' '; RECEIPT_CAP + 1]),
        _ => panic!("unknown mutation {mutation}"),
    }
    let raw = raw.unwrap_or_else(|| {
        if contract == "receipt"
            && !matches!(
                mutation,
                "unsafeInteger" | "unsafeIntegerPlus" | "i64Max" | "u64Max" | "nullPair"
            )
        {
            receipt_bytes(&value)
        } else {
            serde_json::to_vec(&value).unwrap()
        }
    });
    let result = match contract {
        "rate" => contracts
            .validate_inputs(&raw, policy, fx, budget)
            .map(|_| ()),
        "policy" => contracts
            .validate_inputs(rate, &raw, fx, budget)
            .map(|_| ()),
        "fx" => contracts
            .validate_inputs(rate, policy, &raw, budget)
            .map(|_| ()),
        "budget" => contracts
            .validate_inputs(rate, policy, fx, &raw)
            .map(|_| ()),
        "statement" => contracts.validate_supplier_statement(&raw).map(|_| ()),
        "receipt" => contracts.validate_receipt(&raw).map(|_| ()),
        _ => unreachable!(),
    };
    result.map_err(|error| error.to_string())
}
