use std::cell::Cell;

use chrono::DateTime;
use chrono::Utc;
use pretty_assertions::assert_eq;
use serde_json::Value;
use sha2::Digest;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::ModeEvidence;
use crate::ProviderRole;
use crate::Usage;
use crate::cost_authority::*;

const STARTED: &str = "2026-08-30T09:30:00.000Z";
const FINISHED: &str = "2026-08-30T10:00:00.000Z";
const DEADLINE: &str = "2026-08-30T10:30:00.000Z";
const RETENTION: &str = "2026-08-30T11:00:00.000Z";

fn hex(byte: char) -> String {
    byte.to_string().repeat(64)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn receipt_sha(receipt: &crate::cost_contracts::CostReceiptV1) -> String {
    sha256(&crate::jcs::canonicalize_value(&serde_json::to_value(receipt).unwrap()).unwrap())
}

fn json_bytes(source: &[u8], edits: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(source).unwrap();
    edits(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn mode_evidence(input: &SyntheticLiveCostAuthority) -> ModeEvidence {
    ModeEvidence::Live {
        attestation_sha256: input.data.attestation_sha256.clone(),
        provider_budget_evidence_sha256: sha256(&input.budget),
        approval_commitment: input.data.attestation.approval_id.clone(),
        provider_endpoint_commitment: input
            .data
            .provider_endpoint_commitment
            .clone()
            .unwrap(),
        provider_role: ProviderRole::TargetVolcengine,
        arm_order_commitment: input.data.arm_order_commitment.clone(),
        rate_card_sha256: sha256(&input.rate_card),
        billing_policy_sha256: sha256(&input.billing_policy),
        fx_policy_sha256: Some(sha256(&input.fx_policy)),
        authorized_pair_cost_fen: input.data.attestation.approved_total_fen,
        retention_deadline: input.data.attestation.retention_deadline.clone(),
    }
}

fn rebind(input: &mut SyntheticLiveCostAuthority) {
    input.data.attestation.rate_card_sha256 = sha256(&input.rate_card);
    input.data.attestation.billing_policy_commitment = sha256(&input.billing_policy);
    input.data.attestation.fx_policy_sha256 = sha256(&input.fx_policy);
    input.data.attestation.provider_budget_evidence_sha256 = sha256(&input.budget);
    input.data.attestation_sha256 =
        sha256(&serde_json::to_vec(&input.data.attestation).unwrap());
    let evidence = mode_evidence(input);
    for arm in &mut input.data.arms {
        arm.mode_evidence = evidence.clone();
    }
}

fn synthetic_input() -> SyntheticLiveCostAuthority {
    let effective = |value: &mut Value| value["effectiveAt"] = Value::String("2026-08-30T09:00:00.000Z".into());
    let rate_card = json_bytes(include_bytes!("../tests/fixtures/contracts/06b1/provider-rate-card.canonical.json"), effective);
    let billing_policy = json_bytes(include_bytes!("../tests/fixtures/contracts/06b1/billing-policy.canonical.json"), effective);
    let fx_policy = json_bytes(include_bytes!("../tests/fixtures/contracts/06b1/fx-policy.canonical.json"), effective);
    let budget = json_bytes(
        include_bytes!("../tests/fixtures/contracts/06b1/provider-budget-evidence.canonical.json"),
        |value| {
            value["validFrom"] = Value::String("2026-08-30T09:00:00.000Z".into());
            value["validUntil"] = Value::String(RETENTION.into());
        },
    );
    let mut attestation: crate::NativeHeldOutAttestation = serde_json::from_slice(include_bytes!("../tests/fixtures/contracts/06a/canonical-native-attestation.json")).unwrap();
    attestation.candidate_frozen_at = "2026-08-30T09:00:00.000Z".into();
    attestation.case_selected_at = "2026-08-30T09:10:00.000Z".into();
    attestation.signed_at = "2026-08-30T09:20:00.000Z".into();
    attestation.retention_deadline = RETENTION.into();
    attestation.rate_effective_at = "2026-08-30T09:00:00.000Z".into();
    attestation.private_root = "/synthetic/private".into();
    attestation.approval_id = "approval-2026-08-30".into();
    attestation.approved_total_fen = 800; attestation.approved_per_run_fen = 400;
    attestation.max_provider_request_attempts_per_run = 2; attestation.max_total_tokens_per_run = 1_000;
    attestation.max_elapsed_seconds_per_run = 60; attestation.max_output_tokens_per_request = 4_096;
    attestation.provider_role = "targetVolcengine".into();
    attestation.target_provider_model_evidence_commitment = hex('3');
    let pair_id = hex('c'); let frozen_sha256 = hex('1'); let execution_context_sha256 = hex('e');
    let arm_order_commitment = hex('4'); let attempt_ledger_sha256 = hex('b');
    let roots = [hex('8'), hex('a')];
    let manifest_shas = [hex('5'), hex('f')];
    let receipt_shas = [hex('6'), hex('d')];
    let usage = Usage { total_tokens: 150, input_tokens: 100, cached_input_tokens: 20,
        cache_write_input_tokens: 10, output_tokens: 50, reasoning_output_tokens: 15 };
    let conditions = [EvaluationCondition::Generic, EvaluationCondition::Candidate];
    let arms = [0_usize, 1].map(|index| {
        let start = u64::try_from(index).unwrap();
        let ordinal = u8::try_from(index + 1).unwrap();
        let condition = conditions[index];
        let ledger = crate::proof_ledger::VerifiedLedgerArm {
            condition, global_start_inclusive: start, global_end_exclusive: start + 1,
            provider_request_attempt_count: 1, provider_completed_response_count: 1, raw_response_count: 1, usage: usage.clone(),
            attempt_index_prefix_sha256: hex(['7', '9'][index]),
            attempt_index_prefix_root_sha256: roots[index].clone(), run_manifest_sha256: manifest_shas[index].clone(),
        };
        let receipt = crate::ArmReceipt {
            schema_version: 1, pair_id: pair_id.clone(), frozen_run_context_sha256: frozen_sha256.clone(),
            execution_context_sha256: execution_context_sha256.clone(), run_ordinal: ordinal, condition,
            first_condition: EvaluationCondition::Generic, second_condition: EvaluationCondition::Candidate,
            attempt_index_file_sha256: ledger.attempt_index_prefix_sha256.clone(), attempt_index_merkle_root: roots[index].clone(),
            global_attempt_start_inclusive: start, global_attempt_end_exclusive: start + 1,
            attempt_count: 1, completion_count: 1, failure_count: 0, timeout_count: 0, in_flight: 0,
            sealed_at: ["2026-08-30T09:40:00.000Z", "2026-08-30T09:50:00.000Z"][index].into(),
            previous_arm_receipt_sha256: (index == 1).then(|| receipt_shas[0].clone()),
            run_manifest_sha256: Some(manifest_shas[index].clone()), receipt_sha256: receipt_shas[index].clone(),
        };
        LiveArmAuthority {
            run_ordinal: ordinal, condition, core_run_ordinal: ordinal, core_condition: condition,
            core_manifest_sha256: manifest_shas[index].clone(), core_usage: usage.clone(), core_raw_response_count: 1,
            manifest_sha256: manifest_shas[index].clone(), manifest_pair_id: pair_id.clone(), manifest_frozen_sha256: frozen_sha256.clone(),
            manifest_execution_sha256: execution_context_sha256.clone(), manifest_execution_mode: ExecutionMode::Live, manifest_fork_sha: "a".repeat(40), manifest_case_sha256: hex('1'),
            provider_label: "approved-provider".into(), model_label: "approved-model".into(), actual_model_revision: "approved-model-revision".into(),
            usage_scope: "completeNativeThreadTree".into(), usage: usage.clone(), provider_request_attempt_count: 1,
            provider_completed_response_count: 1, raw_response_count: 1, authorized_per_run_fen: 400,
            max_provider_request_attempts: 2, max_total_tokens: 1_000, max_elapsed_seconds: 60, elapsed_ms: 30_000,
            mode_evidence: ModeEvidence::Replay { fixture_set_sha256: hex('0') }, ledger, receipt, receipt_sha256: receipt_shas[index].clone(),
        }
    });
    let pair_receipt = crate::PairReceipt {
        schema_version: 1, pair_id: pair_id.clone(), frozen_run_context_sha256: frozen_sha256.clone(), execution_context_sha256: execution_context_sha256.clone(),
        first_arm_receipt_sha256: receipt_shas[0].clone(), second_arm_receipt_sha256: receipt_shas[1].clone(),
        first_run_manifest_sha256: Some(manifest_shas[0].clone()), second_run_manifest_sha256: Some(manifest_shas[1].clone()),
        total_attempt_count: 2, total_completion_count: 2, total_failure_count: 0, total_timeout_count: 0,
        arm_order_commitment: arm_order_commitment.clone(), final_attempt_index_root: roots[1].clone(), finished_at: FINISHED.into(),
    };
    let data = LiveAuthorityData {
        private_root: "/synthetic/private".into(), pair_id, fork_sha: "a".repeat(40), frozen_sha256, execution_mode: ExecutionMode::Live,
        execution_context_sha256, started_at: STARTED.into(), deadline: DEADLINE.into(), arm_order_commitment,
        provider_endpoint_commitment: Some(hex('3')), pair_receipt, pair_receipt_sha256: hex('4'), attempt_ledger_sha256,
        attempt_index_root_sha256: roots[1].clone(), attestation, attestation_sha256: String::new(), fixed_max_output_tokens: 4_096,
        fixed_max_provider_request_attempts: 2, fixed_max_total_tokens: 1_000, fixed_max_elapsed_seconds: 60, arms,
    };
    let mut input = SyntheticLiveCostAuthority { data, rate_card, billing_policy, fx_policy, budget };
    rebind(&mut input);
    input
}

#[derive(Clone, Copy)]
enum Mutation {
    UsageDrift,
    Endpoint,
    Order,
    FutureSigned,
    Retention,
    Fork,
    ProviderRole,
    TargetEvidence,
    MaxOutput,
    AttemptCap,
    Deadline,
    BudgetAbove,
    RateEffectiveMismatch,
    RateAfterStart,
    BillingAfterStart,
    FxAfterStart,
    RateExpiryEqual,
    RateExpired,
    BudgetWindow,
    UnsafeInteger,
}

fn mutate(mut input: SyntheticLiveCostAuthority, mutation: Mutation) -> SyntheticLiveCostAuthority {
    match mutation {
        Mutation::UsageDrift => input.data.arms[0].core_usage.total_tokens += 1,
        Mutation::Endpoint => input.data.provider_endpoint_commitment = Some(hex('2')),
        Mutation::Order => input.data.arm_order_commitment = hex('2'),
        Mutation::FutureSigned => input.data.attestation.signed_at = "2026-08-30T09:31:00.000Z".into(),
        Mutation::Retention => input.data.attestation.retention_deadline = FINISHED.into(),
        Mutation::Fork => input.data.attestation.candidate_sha = "b".repeat(40),
        Mutation::ProviderRole => input.data.attestation.provider_role = "approvedReference".into(),
        Mutation::TargetEvidence => {
            input.data.attestation.target_provider_model_evidence_commitment = hex('2');
        }
        Mutation::MaxOutput => input.data.fixed_max_output_tokens += 1,
        Mutation::AttemptCap => {
            input.data.attestation.max_provider_request_attempts_per_run = 0;
            input.data.fixed_max_provider_request_attempts = 0;
            for arm in &mut input.data.arms {
                arm.max_provider_request_attempts = 0;
            }
        }
        Mutation::Deadline => input.data.arms[1].receipt.sealed_at = "2026-08-30T10:31:00.000Z".into(),
        Mutation::BudgetAbove => {
            input.budget = json_bytes(&input.budget, |value| value["prepaidOrHardLimitFen"] = 801.into());
        }
        Mutation::RateEffectiveMismatch => {
            input.data.attestation.rate_effective_at = "2026-08-30T09:01:00.000Z".into();
        }
        Mutation::RateAfterStart => {
            input.rate_card = json_bytes(&input.rate_card, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
            input.data.attestation.rate_effective_at = "2026-08-30T09:31:00.000Z".into();
        }
        Mutation::BillingAfterStart => {
            input.billing_policy = json_bytes(&input.billing_policy, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::FxAfterStart => {
            input.fx_policy = json_bytes(&input.fx_policy, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::RateExpiryEqual => {
            input.rate_card = json_bytes(&input.rate_card, |value| value["expiresAt"] = Value::String(FINISHED.into()));
        }
        Mutation::RateExpired => {
            input.rate_card = json_bytes(&input.rate_card, |value| value["expiresAt"] = Value::String("2026-08-30T09:59:00.000Z".into()));
        }
        Mutation::BudgetWindow => {
            input.budget = json_bytes(&input.budget, |value| value["validFrom"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::UnsafeInteger => {
            let unsafe_value = 9_007_199_254_740_992_u64;
            input.data.attestation.max_total_tokens_per_run = unsafe_value;
            input.data.fixed_max_total_tokens = unsafe_value;
            for arm in &mut input.data.arms {
                arm.max_total_tokens = i64::try_from(unsafe_value).unwrap();
            }
        }
    }
    rebind(&mut input);
    input
}

fn rejected(mutation: Mutation, expected: &str) {
    let sentinel = b"inventory-sentinel".to_vec();
    let before = sentinel.clone();
    let error = match prepare_synthetic_live_cost_authority(mutate(synthetic_input(), mutation)) {
        Ok(_) => panic!("mutated authority unexpectedly verified"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(expected), "{error:#}");
    assert_eq!(sentinel, before);
}

macro_rules! rejection_test {
    ($name:ident, $mutation:ident, $expected:literal) => {
        #[test]
        fn $name() {
            rejected(Mutation::$mutation, $expected);
        }
    };
}

rejection_test!(cost_authority_rejects_broker_app_server_response_usage_multiset_drift, UsageDrift, "usage");
rejection_test!(cost_authority_rejects_provider_endpoint_commitment_drift, Endpoint, "endpoint");
rejection_test!(cost_authority_rejects_arm_order_commitment_drift, Order, "arm order");
rejection_test!(cost_authority_rejects_future_attestation_signed_at, FutureSigned, "signedAt");
rejection_test!(cost_authority_rejects_inverted_or_expired_retention_timeline, Retention, "retention");
rejection_test!(cost_authority_rejects_candidate_fork_sha_drift, Fork, "candidate");
rejection_test!(cost_authority_rejects_fixed_max_output_tokens_drift, MaxOutput, "max output");
rejection_test!(cost_authority_rejects_provider_budget_above_user_approved_total, BudgetAbove, "approved total");
rejection_test!(cost_authority_rejects_rate_card_effective_time_drift, RateEffectiveMismatch, "rate effective");
rejection_test!(cost_authority_rejects_rate_expiry_equal_to_finish, RateExpiryEqual, "rate expiry");
rejection_test!(cost_authority_rejects_rate_expired_during_run, RateExpired, "rate expiry");
rejection_test!(cost_authority_rejects_invalid_budget_validity_window, BudgetWindow, "budget validity");

#[test]
fn cost_authority_rejects_provider_role_or_target_evidence_drift() {
    rejected(Mutation::ProviderRole, "provider role");
    rejected(Mutation::TargetEvidence, "target provider");
}

#[test]
fn cost_authority_rejects_attempt_cap_or_deadline_hard_gate_violation() {
    rejected(Mutation::AttemptCap, "attempt cap");
    rejected(Mutation::Deadline, "deadline");
}

#[test]
fn cost_authority_rejects_policy_effective_after_run_start() {
    for mutation in [Mutation::RateAfterStart, Mutation::BillingAfterStart, Mutation::FxAfterStart] {
        rejected(mutation, "effective after run start");
    }
}

struct FixedClock {
    value: anyhow::Result<DateTime<Utc>>,
    calls: Cell<u8>,
}

impl CostClock for FixedClock {
    fn now(&self) -> anyhow::Result<DateTime<Utc>> {
        self.calls.set(self.calls.get() + 1);
        self.value.as_ref().map(Clone::clone).map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

fn clock(value: &str) -> FixedClock {
    FixedClock {
        value: Ok(DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc)),
        calls: Cell::new(0),
    }
}

fn commit_error(input: SyntheticLiveCostAuthority, statement: Option<&RetainedSupplierStatement>, clock: &FixedClock) -> anyhow::Error {
    let authority = prepare_synthetic_live_cost_authority(input).unwrap_or_else(|error| panic!("{error:#}"));
    match commit_cost_receipt(&authority, EvaluationCondition::Candidate, statement, clock) {
        Ok(_) => panic!("receipt unexpectedly committed"),
        Err(error) => error,
    }
}

#[test]
fn cost_authority_rejects_calculation_before_finish() {
    let clock = clock("2026-08-30T09:59:59.999Z");
    assert!(commit_error(synthetic_input(), None, &clock).to_string().contains("before finish"));
}

#[test]
fn cost_authority_rejects_calculation_at_or_after_retention() {
    for value in [RETENTION, "2026-08-30T11:00:00.001Z"] {
        let clock = clock(value);
        assert!(commit_error(synthetic_input(), None, &clock).to_string().contains("retention"));
    }
}

fn supplier(edits: impl FnOnce(&mut Value)) -> RetainedSupplierStatement {
    let bytes = json_bytes(
        include_bytes!("../tests/fixtures/contracts/06b1/supplier-statement.canonical.json"),
        |value| {
            value["pairId"] = Value::String(hex('c'));
            value["issuedAt"] = Value::String("2026-08-30T10:00:30.000Z".into());
            edits(value);
        },
    );
    retain_synthetic_supplier_statement(&bytes).unwrap()
}

#[test]
fn cost_authority_rejects_supplier_issue_time_after_calculation() {
    let statement = supplier(|value| value["issuedAt"] = Value::String("2026-08-30T10:02:00.000Z".into()));
    let clock = clock("2026-08-30T10:01:00.000Z");
    assert!(commit_error(synthetic_input(), Some(&statement), &clock).to_string().contains("supplier issue"));
}

#[test]
fn cost_authority_rejects_supplier_authority_mismatch() {
    for field in ["condition", "pairId", "providerLabel", "actualModelRevision"] {
        let statement = supplier(|value| value[field] = Value::String(match field {
            "condition" => "generic".into(),
            "pairId" => hex('0'),
            _ => "wrong-authority".into(),
        }));
        let clock = clock("2026-08-30T10:01:00.000Z");
        assert!(commit_error(synthetic_input(), Some(&statement), &clock).to_string().contains("supplier authority"));
    }
}

#[test]
fn cost_authority_rejects_emitted_integer_above_jcs_safe_boundary() {
    let clock = clock("2026-08-30T10:01:00.000Z");
    assert!(commit_error(mutate(synthetic_input(), Mutation::UnsafeInteger), None, &clock).to_string().contains("safe integer"));
}

#[test]
fn cost_authority_propagates_clock_error_once() {
    let clock = FixedClock {
        value: Err(anyhow::anyhow!("synthetic clock failed")),
        calls: Cell::new(0),
    };
    let error = commit_error(synthetic_input(), None, &clock);
    assert!(error.to_string().contains("synthetic clock failed"));
    assert_eq!(clock.calls.get(), 1);
}

#[test]
fn cost_authority_normalizes_submillisecond_clock_exactly_once() {
    let authority = prepare_synthetic_live_cost_authority(synthetic_input()).unwrap();
    let clock = clock("2026-08-30T10:01:00.123456Z");
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, None, &clock).unwrap();
    assert_eq!(receipt.calculated_at, "2026-08-30T10:01:00.123Z");
    assert_eq!(clock.calls.get(), 1);
}

#[test]
fn cost_authority_builds_exact_happy_receipt_without_supplier() {
    let authority = prepare_synthetic_live_cost_authority(synthetic_input()).unwrap();
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, None, &clock("2026-08-30T10:01:00.000Z")).unwrap();
    assert_eq!(receipt.condition, EvaluationCondition::Candidate);
    assert_eq!(receipt.run_ordinal, 2);
    assert_eq!(receipt.broker_receipt_sha256, hex('d'));
    assert_eq!(receipt.pair_receipt_sha256, hex('4'));
    assert_eq!(receipt.supplier_actual_fen, None);
    assert!(receipt.within_ceilings);
    assert_eq!(receipt_sha(&receipt), "8301aeceb7bbaf03e744d1a6da46543ee646ef8276b9cdc9dd32622cb538fbd6");
}

#[test]
fn cost_authority_builds_exact_happy_receipt_with_over_ceiling_supplier_actual() {
    let statement = supplier(|value| value["actualFen"] = 450.into());
    let authority = prepare_synthetic_live_cost_authority(synthetic_input()).unwrap();
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, Some(&statement), &clock("2026-08-30T10:01:00.000Z")).unwrap();
    assert_eq!(receipt.supplier_actual_fen, Some(450));
    assert_eq!(receipt.charged_fen, 450);
    assert!(!receipt.within_ceilings);
    assert_eq!(receipt_sha(&receipt), "5ccbf2ee5c4f6c10298767a060230a732f08caa2271abd73df3a589d6c0d14eb");
}
