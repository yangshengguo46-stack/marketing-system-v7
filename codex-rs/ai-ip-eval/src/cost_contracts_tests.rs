use sha2::Digest;

fn fixture(name: &str) -> Vec<u8> {
    let resource = format!("tests/fixtures/contracts/06b1/{name}");
    let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
    std::fs::read(path).unwrap()
}

#[test]
fn executable_06b1_cost_contracts_accept_only_exact_shapes() {
    let rate_card = fixture("provider-rate-card.canonical.json");
    let billing_policy = fixture("billing-policy.canonical.json");
    let fx_policy = fixture("fx-policy.canonical.json");
    let budget = fixture("provider-budget-evidence.canonical.json");
    let supplier_statement = fixture("supplier-statement.canonical.json");
    let receipt = fixture("cost-receipt.canonical.json");
    let contracts = crate::cost_contracts::FrozenCostContracts::load().unwrap();

    let verified = contracts
        .validate_inputs(&rate_card, &billing_policy, &fx_policy, &budget)
        .unwrap();
    assert_eq!(verified.rate_card.provider_label, "approved-provider");
    assert_eq!(verified.billing_policy.currency, "CNY");
    assert_eq!(verified.fx_policy.mode, "notApplicable");
    assert_eq!(verified.budget.prepaid_or_hard_limit_fen, 800);
    assert_eq!(
        verified.rate_card_sha256,
        format!("{:x}", sha2::Sha256::digest(&rate_card)),
        "source input commitments bind their original pretty bytes"
    );
    assert_ne!(
        verified.rate_card_sha256,
        format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&verified.rate_card).unwrap())
        ),
        "source inputs must not be silently reserialized before commitment"
    );
    assert_eq!(verified.billing_policy_commitment.len(), 64);
    assert_eq!(verified.fx_policy_sha256.len(), 64);
    assert_eq!(verified.provider_budget_evidence_sha256.len(), 64);

    let duplicate_rate_card = br#"{"schemaVersion":1,"schemaVersion":1}"#;
    assert!(
        contracts
            .validate_inputs(duplicate_rate_card, &billing_policy, &fx_policy, &budget)
            .is_err(),
        "duplicate JSON keys must be rejected before schema validation"
    );

    let mut equivalent_number: serde_json::Value = crate::jcs::parse_json(&rate_card).unwrap();
    equivalent_number["schemaVersion"] = serde_json::json!(1.0);
    assert!(
        contracts
            .validate_inputs(
                &serde_json::to_vec(&equivalent_number).unwrap(),
                &billing_policy,
                &fx_policy,
                &budget,
            )
            .is_err(),
        "typed decode must preserve the schema-accepted rate card value exactly"
    );

    assert!(
        contracts
            .validate_inputs(
                &vec![b' '; 64 * 1024 + 1],
                &billing_policy,
                &fx_policy,
                &budget,
            )
            .is_err(),
        "source input byte cap must be enforced"
    );

    let mut provider_mismatch: serde_json::Value = crate::jcs::parse_json(&billing_policy).unwrap();
    provider_mismatch["providerLabel"] = serde_json::json!("other-provider");
    assert!(
        contracts
            .validate_inputs(
                &rate_card,
                &serde_json::to_vec(&provider_mismatch).unwrap(),
                &fx_policy,
                &budget,
            )
            .is_err(),
        "rate, policy, and budget provider labels must agree"
    );

    let mut invalid_window: serde_json::Value = crate::jcs::parse_json(&budget).unwrap();
    invalid_window["validUntil"] = serde_json::json!("2026-08-30T09:00:00.000Z");
    assert!(
        contracts
            .validate_inputs(
                &rate_card,
                &billing_policy,
                &fx_policy,
                &serde_json::to_vec(&invalid_window).unwrap(),
            )
            .is_err(),
        "budget validity window must be ordered"
    );

    let supplier = contracts
        .validate_supplier_statement(&supplier_statement)
        .unwrap();
    assert_eq!(supplier.condition, crate::EvaluationCondition::Candidate);
    assert_eq!(supplier.currency, "CNY");

    let accepted_receipt = contracts.validate_receipt(&receipt).unwrap();
    assert_eq!(accepted_receipt.execution_mode, "live");
    assert_eq!(accepted_receipt.fx.mode, "notApplicable");
    assert_eq!(accepted_receipt.ceilings.approved_total_fen, 800);
    assert_eq!(
        crate::cost_contracts::validate_cost_receipt(&receipt)
            .unwrap()
            .pair_id,
        accepted_receipt.pair_id
    );

    let receipt_value = crate::jcs::parse_json(&receipt).unwrap();
    assert!(
        contracts
            .validate_receipt(&serde_json::to_vec_pretty(&receipt_value).unwrap())
            .is_err(),
        "generated receipts must be exact JCS bytes"
    );
    let mut null_pair = receipt_value;
    null_pair["pairId"] = serde_json::Value::Null;
    assert!(
        contracts
            .validate_receipt(&serde_json::to_vec(&null_pair).unwrap())
            .is_err(),
        "receipt pair ID cannot be null"
    );
}
