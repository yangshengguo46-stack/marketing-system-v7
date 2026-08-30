use std::cell::Cell;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

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
use crate::cost_inputs::CostReceiptCheckpoint;
use crate::cost_inputs::publish_cost_receipt_for_test;
use crate::cost_inputs::publish_cost_receipt_with_sync_for_test;
use crate::private_inventory::InventoryKind;
use crate::private_inventory::batch::ExpectedInventoryEntry;
use crate::private_inventory::batch::append_private_inventory_batch_from_root;

const STARTED: &str = "2026-08-30T09:30:00.000Z";
const FINISHED: &str = "2026-08-30T10:00:00.000Z";
const DEADLINE: &str = "2026-08-30T10:30:00.000Z";
const RETENTION: &str = "2026-08-30T11:00:00.000Z";
const ATTESTATION: usize = 0;
const RATE: usize = 1;
const BILLING: usize = 2;
const FX: usize = 3;
const BUDGET: usize = 4;

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SyntheticResponseUsage {
    response_id: String,
    usage: Usage,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SyntheticSignedUsage {
    bytes: Vec<u8>,
    sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SyntheticCostBoundary {
    documents: [Vec<u8>; 5],
    response_usage: [[SyntheticSignedUsage; 2]; 2],
    published_receipt: Option<crate::cost_contracts::CostReceiptV1>,
}

fn state(boundary: &dyn SyntheticAuthorityBoundary) -> &std::cell::RefCell<SyntheticCostBoundary> {
    boundary.as_any().downcast_ref().unwrap()
}

fn input_state(input: &SyntheticLiveCostAuthority) -> &std::cell::RefCell<SyntheticCostBoundary> {
    state(input.boundary.as_ref())
}

fn usage_parts(usage: &Usage) -> [i64; 6] {
    [usage.total_tokens, usage.input_tokens, usage.cached_input_tokens, usage.cache_write_input_tokens, usage.output_tokens, usage.reasoning_output_tokens]
}

fn signed_usage_rows(side: &SyntheticSignedUsage) -> anyhow::Result<Vec<SyntheticResponseUsage>> {
    anyhow::ensure!(side.sha256 == sha256(&side.bytes), "response usage side commitment differs");
    let mut rows: Vec<SyntheticResponseUsage> = serde_json::from_slice(&side.bytes)?;
    rows.sort_by(|left, right| left.response_id.cmp(&right.response_id));
    anyhow::ensure!(!rows.is_empty() && rows.iter().all(|row| !row.response_id.is_empty())
        && !rows.windows(2).any(|pair| pair[0].response_id == pair[1].response_id), "response usage side is not a unique nonempty multiset");
    Ok(rows)
}

impl SyntheticAuthorityBoundary for std::cell::RefCell<SyntheticCostBoundary> {
    fn documents(&self) -> [Vec<u8>; 5] { self.borrow().documents.clone() }

    fn validate_usage(&self, data: &LiveAuthorityData) -> anyhow::Result<()> {
        for (arm, sides) in data.arms.iter().zip(&self.borrow().response_usage) {
            let broker = signed_usage_rows(&sides[0])?; let app_server = signed_usage_rows(&sides[1])?;
            anyhow::ensure!(broker == app_server, "broker/App Server response usage multiset differs");
            let mut total = [0_i64; 6];
            for row in &broker { for (sum, value) in total.iter_mut().zip(usage_parts(&row.usage)) { *sum = sum.checked_add(value).ok_or_else(|| anyhow::anyhow!("response usage sum overflow"))?; } }
            anyhow::ensure!(total == usage_parts(&arm.usage), "response usage multiset aggregate differs");
        }
        Ok(())
    }

    fn publish_receipt(&self, receipt: &crate::cost_contracts::CostReceiptV1) -> anyhow::Result<()> {
        let mut state = self.borrow_mut();
        anyhow::ensure!(state.published_receipt.is_none(), "synthetic receipt sink is already occupied");
        state.published_receipt = Some(receipt.clone());
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any { self }
}

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
    crate::jcs::canonicalize_value(&value).unwrap()
}

#[test]
fn cost_authority_json_bytes_canonicalizes_equivalent_source_order_after_edit() {
    let first = json_bytes(br#"{"\uE000":1,"\uD800\uDC00":2}"#, |value| value["m"] = Value::String("changed".into()));
    let second = json_bytes(br#"{"\uD800\uDC00":2,"\uE000":1}"#, |value| value["m"] = Value::String("changed".into()));
    let expected = b"{\"m\":\"changed\",\"\xF0\x90\x80\x80\":2,\"\xEE\x80\x80\":1}";

    assert_eq!(first, second);
    assert_eq!(first, expected);
}

fn mode_evidence(input: &SyntheticLiveCostAuthority) -> ModeEvidence {
    let state = input_state(input).borrow();
    ModeEvidence::Live {
        attestation_sha256: sha256(&state.documents[ATTESTATION]),
        provider_budget_evidence_sha256: sha256(&state.documents[BUDGET]),
        approval_commitment: input.data.attestation.approval_id.clone(),
        provider_endpoint_commitment: input
            .data
            .provider_endpoint_commitment
            .clone()
            .unwrap(),
        provider_role: ProviderRole::TargetVolcengine,
        arm_order_commitment: input.data.arm_order_commitment.clone(),
        rate_card_sha256: sha256(&state.documents[RATE]),
        billing_policy_sha256: sha256(&state.documents[BILLING]),
        fx_policy_sha256: Some(sha256(&state.documents[FX])),
        authorized_pair_cost_fen: input.data.attestation.approved_total_fen,
        retention_deadline: input.data.attestation.retention_deadline.clone(),
    }
}

fn rebind(input: &mut SyntheticLiveCostAuthority) {
    let commitments = {
        let documents = &input_state(input).borrow().documents;
        [RATE, BILLING, FX, BUDGET].map(|index| sha256(&documents[index]))
    };
    input.data.attestation.rate_card_sha256 = commitments[0].clone();
    input.data.attestation.billing_policy_commitment = commitments[1].clone();
    input.data.attestation.fx_policy_sha256 = commitments[2].clone();
    input.data.attestation.provider_budget_evidence_sha256 = commitments[3].clone();
    resign_attestation(input);
}

fn resign_attestation(input: &mut SyntheticLiveCostAuthority) {
    input_state(input).borrow_mut().documents[ATTESTATION] = serde_json::to_vec(&input.data.attestation).unwrap();
    refresh_evidence(input);
}

fn refresh_evidence(input: &mut SyntheticLiveCostAuthority) {
    let evidence = mode_evidence(input);
    for arm in &mut input.data.arms {
        arm.mode_evidence = evidence.clone();
    }
}

fn edit_document(input: &SyntheticLiveCostAuthority, index: usize, edits: impl FnOnce(&mut Value)) {
    let bytes = input_state(input).borrow().documents[index].clone();
    input_state(input).borrow_mut().documents[index] = json_bytes(&bytes, edits);
}

fn signed_usage(response_id: &str, usage: &Usage) -> SyntheticSignedUsage {
    let bytes = serde_json::to_vec(&vec![SyntheticResponseUsage { response_id: response_id.into(), usage: usage.clone() }]).unwrap();
    SyntheticSignedUsage { sha256: sha256(&bytes), bytes }
}

fn synthetic_input() -> SyntheticLiveCostAuthority {
    synthetic_input_with_order([
        EvaluationCondition::Generic,
        EvaluationCondition::Candidate,
    ])
}

fn synthetic_input_with_order(
    conditions: [EvaluationCondition; 2],
) -> SyntheticLiveCostAuthority {
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
    let arm_order_commitment = match conditions {
        [EvaluationCondition::Generic, EvaluationCondition::Candidate] => hex('4'),
        [EvaluationCondition::Candidate, EvaluationCondition::Generic] => hex('2'),
        _ => panic!("synthetic authority requires distinct conditions"),
    };
    let attempt_ledger_sha256 = hex('9');
    let roots = [hex('8'), hex('a')];
    let manifest_shas = [hex('5'), hex('f')];
    let receipt_shas = [hex('6'), hex('d')];
    let usage = Usage { total_tokens: 150, input_tokens: 100, cached_input_tokens: 20,
        cache_write_input_tokens: 10, output_tokens: 50, reasoning_output_tokens: 15 };
    let response_usage = [0_usize, 1].map(|index| {
        let side = signed_usage(&format!("response-{index}"), &usage);
        [side.clone(), side]
    });
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
            first_condition: conditions[0], second_condition: conditions[1],
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
            manifest_attempt_ledger_sha256: ledger.attempt_index_prefix_sha256.clone(), manifest_attempt_index_root_sha256: ledger.attempt_index_prefix_root_sha256.clone(),
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
        attempt_index_root_sha256: roots[1].clone(), attestation, fixed_max_output_tokens: 4_096,
        fixed_max_provider_request_attempts: 2, fixed_max_total_tokens: 1_000, fixed_max_elapsed_seconds: 60, arms,
    };
    let boundary = SyntheticCostBoundary { documents: [Vec::new(), rate_card, billing_policy, fx_policy, budget], response_usage, published_receipt: None };
    let mut input = SyntheticLiveCostAuthority { data, boundary: std::rc::Rc::new(std::cell::RefCell::new(boundary)) };
    rebind(&mut input);
    input
}

fn synthetic_manifest_bytes(input: &SyntheticLiveCostAuthority) -> [Vec<u8>; 2] {
    input.data.arms.each_ref().map(|arm| {
        crate::jcs::canonicalize_value(&serde_json::to_value(crate::RunManifest {
            schema_version: 1, pair_id: input.data.pair_id.clone(),
            frozen_run_context_sha256: input.data.frozen_sha256.clone(),
            execution_context_sha256: input.data.execution_context_sha256.clone(),
            run_ordinal: arm.run_ordinal, condition: arm.condition,
            fork_sha: input.data.fork_sha.clone(), case_sha256: arm.manifest_case_sha256.clone(),
            source_materials_sha256: hex('0'), prompt_sha256: hex('0'), additional_context_sha256: hex('0'),
            output_schema_sha256: hex('0'), thread_start_request_sha256: hex('0'), turn_start_request_sha256: hex('0'),
            shared_config_sha256: hex('0'), effective_config_sha256: hex('0'), config_layers_sha256: hex('0'),
            native_skill_sha256: Some(hex('0')), pre_skill_catalog_sha256: hex('0'), post_skill_catalog_sha256: hex('0'),
            normalized_base_catalog_sha256: hex('0'), skill_use_evidence_sha256: (arm.condition == EvaluationCondition::Candidate).then(|| hex('0')),
            codex_binary_sha256: hex('0'), evaluator_binary_sha256: hex('0'), broker_component_sha256: hex('0'),
            first_root_provider_request_commitment: hex('0'), normalized_first_root_request_commitment: hex('0'),
            normalized_first_root_base_commitment: hex('0'), first_root_treatment_diff_commitment: (arm.condition == EvaluationCondition::Candidate).then(|| hex('0')),
            app_server_transcript_sha256: hex('0'), broker_attempt_ledger_sha256: arm.manifest_attempt_ledger_sha256.clone(),
            attempt_index_root_sha256: arm.manifest_attempt_index_root_sha256.clone(), postprocess_evidence_index_sha256: hex('0'),
            content_package_sha256: hex('0'), root_thread_id: format!("root-thread-{}", arm.run_ordinal),
            root_turn_id: format!("root-turn-{}", arm.run_ordinal), session_id: format!("session-{}", arm.run_ordinal),
            provider_request_attempt_count: arm.provider_request_attempt_count,
            provider_completed_response_count: arm.provider_completed_response_count,
            raw_response_count: arm.raw_response_count, usage_scope: arm.usage_scope.clone(), usage: arm.usage.clone(),
            model_label: arm.model_label.clone(), actual_model_revision: arm.actual_model_revision.clone(),
            deployment_or_fingerprint_commitment: input.data.provider_endpoint_commitment.clone(), provider_label: arm.provider_label.clone(),
            provider_compatibility_name: crate::ProofBrokerCompatibilityName::OpenAi,
            authorized_evaluation_run_cost_fen: arm.authorized_per_run_fen,
            max_provider_request_attempts: arm.max_provider_request_attempts, max_total_tokens: arm.max_total_tokens,
            max_elapsed_seconds: arm.max_elapsed_seconds, elapsed_ms: arm.elapsed_ms, tree_closed: true,
            execution_mode: crate::ExecutionMode::Live, mode_evidence: arm.mode_evidence.clone(),
        }).unwrap()).unwrap()
    })
}

fn bind_synthetic_manifest_hashes(input: &mut SyntheticLiveCostAuthority, bytes: &[Vec<u8>; 2]) {
    let hashes = bytes.each_ref().map(|bytes| sha256(bytes));
    for (arm, hash) in input.data.arms.iter_mut().zip(&hashes) {
        arm.core_manifest_sha256 = hash.clone();
        arm.manifest_sha256 = hash.clone();
        arm.ledger.run_manifest_sha256 = hash.clone();
        arm.receipt.run_manifest_sha256 = Some(hash.clone());
    }
    input.data.pair_receipt.first_run_manifest_sha256 = Some(hashes[0].clone());
    input.data.pair_receipt.second_run_manifest_sha256 = Some(hashes[1].clone());
}

#[derive(Clone, Copy)]
enum Mutation {
    UsageDrift,
    AttestationRawUnknown,
    AttestationTypedMismatch,
    AttestationRateCommitment,
    AttestationBillingCommitment,
    AttestationFxCommitment,
    AttestationBudgetCommitment,
    ManifestLedger,
    ManifestRoot,
    PairFinalRoot,
    FullLedger,
    InFlight,
    DuplicateCandidate,
    MissingCandidate,
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
    SafeIntegerMaximum,
}

fn mutate(mut input: SyntheticLiveCostAuthority, mutation: Mutation) -> SyntheticLiveCostAuthority {
    match mutation {
        Mutation::UsageDrift => {
            let mut state = input_state(&input).borrow_mut();
            let side = &mut state.response_usage[0][0];
            let mut records: Vec<SyntheticResponseUsage> = serde_json::from_slice(&side.bytes).unwrap();
            records[0].usage.total_tokens += 1;
            side.bytes = serde_json::to_vec(&records).unwrap(); side.sha256 = sha256(&side.bytes);
        }
        Mutation::AttestationRawUnknown => {
            edit_document(&input, ATTESTATION, |value| value["unknown"] = true.into());
            refresh_evidence(&mut input); return input;
        }
        Mutation::AttestationTypedMismatch => {
            input.data.attestation.source_materials_sha256 = hex('0');
            refresh_evidence(&mut input); return input;
        }
        Mutation::AttestationRateCommitment => input.data.attestation.rate_card_sha256 = hex('0'),
        Mutation::AttestationBillingCommitment => input.data.attestation.billing_policy_commitment = hex('0'),
        Mutation::AttestationFxCommitment => input.data.attestation.fx_policy_sha256 = hex('0'),
        Mutation::AttestationBudgetCommitment => input.data.attestation.provider_budget_evidence_sha256 = hex('0'),
        Mutation::ManifestLedger => input.data.arms[0].manifest_attempt_ledger_sha256 = hex('0'),
        Mutation::ManifestRoot => input.data.arms[0].manifest_attempt_index_root_sha256 = hex('0'),
        Mutation::PairFinalRoot => {
            input.data.attempt_index_root_sha256 = hex('0');
            input.data.pair_receipt.final_attempt_index_root = hex('0');
        }
        Mutation::FullLedger => input.data.attempt_ledger_sha256 = hex('b'),
        Mutation::InFlight => input.data.arms[0].receipt.in_flight = 1,
        Mutation::DuplicateCandidate | Mutation::MissingCandidate => {
            let condition = if matches!(mutation, Mutation::DuplicateCandidate) { EvaluationCondition::Candidate } else { EvaluationCondition::Generic };
            for arm in &mut input.data.arms {
                arm.condition = condition; arm.core_condition = condition; arm.ledger.condition = condition; arm.receipt.condition = condition;
                arm.receipt.first_condition = condition; arm.receipt.second_condition = condition;
            }
        }
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
            input.data.attestation.max_provider_request_attempts_per_run = 1;
            input.data.fixed_max_provider_request_attempts = 1;
            for arm in &mut input.data.arms {
                arm.max_provider_request_attempts = 1;
            }
            let arm = &mut input.data.arms[0];
            arm.provider_request_attempt_count = 2; arm.ledger.provider_request_attempt_count = 2; arm.receipt.attempt_count = 2;
            arm.ledger.global_end_exclusive = 2; arm.receipt.global_attempt_end_exclusive = 2;
            input.data.pair_receipt.total_attempt_count = 3;
        }
        Mutation::Deadline => input.data.arms[1].receipt.sealed_at = "2026-08-30T10:31:00.000Z".into(),
        Mutation::BudgetAbove => {
            edit_document(&input, BUDGET, |value| value["prepaidOrHardLimitFen"] = 801.into());
        }
        Mutation::RateEffectiveMismatch => {
            input.data.attestation.rate_effective_at = "2026-08-30T09:01:00.000Z".into();
        }
        Mutation::RateAfterStart => {
            edit_document(&input, RATE, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
            input.data.attestation.rate_effective_at = "2026-08-30T09:31:00.000Z".into();
        }
        Mutation::BillingAfterStart => {
            edit_document(&input, BILLING, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::FxAfterStart => {
            edit_document(&input, FX, |value| value["effectiveAt"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::RateExpiryEqual => {
            edit_document(&input, RATE, |value| value["expiresAt"] = Value::String(FINISHED.into()));
        }
        Mutation::RateExpired => {
            edit_document(&input, RATE, |value| value["expiresAt"] = Value::String("2026-08-30T09:59:00.000Z".into()));
        }
        Mutation::BudgetWindow => {
            edit_document(&input, BUDGET, |value| value["validFrom"] = Value::String("2026-08-30T09:31:00.000Z".into()));
        }
        Mutation::UnsafeInteger => {
            let unsafe_value = 9_007_199_254_740_992_u64;
            input.data.attestation.max_total_tokens_per_run = unsafe_value;
            input.data.fixed_max_total_tokens = unsafe_value;
            for arm in &mut input.data.arms {
                arm.max_total_tokens = i64::try_from(unsafe_value).unwrap();
            }
        }
        Mutation::SafeIntegerMaximum => {
            input.data.attestation.approved_total_fen = 9_007_199_254_740_991;
            edit_document(&input, BUDGET, |value| value["prepaidOrHardLimitFen"] = 9_007_199_254_740_991_u64.into());
        }
    }
    if matches!(mutation, Mutation::AttestationRateCommitment | Mutation::AttestationBillingCommitment | Mutation::AttestationFxCommitment | Mutation::AttestationBudgetCommitment) {
        resign_attestation(&mut input);
    } else {
        rebind(&mut input);
    }
    input
}

fn rejected(mutation: Mutation, expected: &str) {
    let input = mutate(synthetic_input(), mutation);
    let boundary = input.boundary.clone();
    let before = state(boundary.as_ref()).borrow().clone();
    let error = match prepare_synthetic_live_cost_authority(input) {
        Ok(_) => panic!("mutated authority unexpectedly verified"),
        Err(error) => error,
    };
    assert!(error.to_string().contains(expected), "{error:#}");
    assert_eq!(*state(boundary.as_ref()).borrow(), before);
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
fn cost_authority_rejects_unvalidated_or_typed_mismatched_attestation() {
    rejected(Mutation::AttestationRawUnknown, "attestation");
    rejected(Mutation::AttestationTypedMismatch, "attestation");
}

#[test]
fn cost_authority_rejects_attestation_cost_input_commitment_drift() {
    for mutation in [Mutation::AttestationRateCommitment, Mutation::AttestationBillingCommitment, Mutation::AttestationFxCommitment, Mutation::AttestationBudgetCommitment] {
        rejected(mutation, "attestation cost");
    }
}

#[test]
fn cost_authority_rejects_manifest_attempt_or_pair_final_root_drift() {
    for mutation in [Mutation::ManifestLedger, Mutation::ManifestRoot, Mutation::PairFinalRoot] {
        rejected(mutation, "attempt commitment");
    }
}

#[test]
fn cost_authority_rejects_full_ledger_final_prefix_drift() {
    rejected(Mutation::FullLedger, "full attempt ledger");
}

#[test]
fn cost_authority_rejects_unsealed_arm_receipt_state() {
    rejected(Mutation::InFlight, "sealed receipt");
}

#[test]
fn cost_authority_rejects_duplicate_or_missing_condition() {
    rejected(Mutation::DuplicateCandidate, "ordered conditions");
    rejected(Mutation::MissingCandidate, "ordered conditions");
}

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
    rejected(Mutation::RateAfterStart, "attestation contract");
    for mutation in [Mutation::BillingAfterStart, Mutation::FxAfterStart] {
        rejected(mutation, "effective after run start");
    }
}

pub(crate) struct FixedClock {
    value: anyhow::Result<DateTime<Utc>>,
    calls: Cell<u8>,
}

impl CostClock for FixedClock {
    fn now(&self) -> anyhow::Result<DateTime<Utc>> {
        self.calls.set(self.calls.get() + 1);
        self.value.as_ref().map(Clone::clone).map_err(|error| anyhow::anyhow!(error.to_string()))
    }
}

impl FixedClock {
    pub(crate) fn calls(&self) -> u8 {
        self.calls.get()
    }
}

fn clock(value: &str) -> FixedClock {
    FixedClock {
        value: Ok(DateTime::parse_from_rfc3339(value).unwrap().with_timezone(&Utc)),
        calls: Cell::new(0),
    }
}

fn commit_error(input: SyntheticLiveCostAuthority, statement: Option<&RetainedSupplierStatement>, clock: &FixedClock) -> anyhow::Error {
    let boundary = input.boundary.clone();
    let before = state(boundary.as_ref()).borrow().clone();
    let authority = prepare_synthetic_live_cost_authority(input).unwrap_or_else(|error| panic!("{error:#}"));
    let error = match commit_cost_receipt(&authority, EvaluationCondition::Candidate, statement, clock) {
        Ok(_) => panic!("receipt unexpectedly committed"),
        Err(error) => error,
    };
    assert_eq!(*state(boundary.as_ref()).borrow(), before);
    error
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
    retain_supplier_statement(&bytes).unwrap()
}

#[test]
fn cost_authority_rejects_supplier_issue_time_after_calculation() {
    let statement = supplier(|value| value["issuedAt"] = Value::String("2026-08-30T10:02:00.000Z".into()));
    let clock = clock("2026-08-30T10:01:00.000Z");
    assert!(commit_error(synthetic_input(), Some(&statement), &clock).to_string().contains("supplier issue"));
}

#[test]
fn cost_authority_rejects_supplier_issue_time_before_selected_seal() {
    let statement = supplier(|value| value["issuedAt"] = Value::String("2026-08-30T09:49:59.999Z".into()));
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
fn cost_authority_accepts_exact_jcs_safe_integer_boundary() {
    let input = mutate(synthetic_input(), Mutation::SafeIntegerMaximum);
    let boundary = input.boundary.clone();
    let authority = prepare_synthetic_live_cost_authority(input).unwrap();
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, None, &clock("2026-08-30T10:01:00.000Z")).unwrap();
    assert_eq!(receipt.ceilings.approved_total_fen, 9_007_199_254_740_991);
    assert_eq!(receipt.ceilings.prepaid_or_hard_limit_fen, 9_007_199_254_740_991);
    assert_eq!(state(boundary.as_ref()).borrow().published_receipt.as_ref(), Some(&receipt));
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
    let input = synthetic_input(); let boundary = input.boundary.clone();
    let authority = prepare_synthetic_live_cost_authority(input).unwrap();
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, None, &clock("2026-08-30T10:01:00.000Z")).unwrap();
    assert_eq!(receipt.condition, EvaluationCondition::Candidate);
    assert_eq!(receipt.run_ordinal, 2);
    assert_eq!(receipt.broker_receipt_sha256, hex('d'));
    assert_eq!(receipt.pair_receipt_sha256, hex('4'));
    assert_eq!(receipt.supplier_actual_fen, None);
    assert!(receipt.within_ceilings);
    assert_eq!(receipt_sha(&receipt), "738400372f6cbba6d70e076b6db9eb669304e3e66da79180b99d2b5d1049109d");
    assert_eq!(state(boundary.as_ref()).borrow().published_receipt.as_ref(), Some(&receipt));
}

#[test]
fn cost_authority_builds_candidate_first_committed_receipt() {
    let input = synthetic_input_with_order([
        EvaluationCondition::Candidate,
        EvaluationCondition::Generic,
    ]);
    let boundary = input.boundary.clone();
    let before = state(boundary.as_ref()).borrow().clone();
    let authority = prepare_synthetic_live_cost_authority(input).unwrap();
    assert_eq!(*state(boundary.as_ref()).borrow(), before);

    let receipt = commit_cost_receipt(
        &authority,
        EvaluationCondition::Candidate,
        None,
        &clock("2026-08-30T10:01:00.000Z"),
    )
    .unwrap();
    assert_eq!(receipt.condition, EvaluationCondition::Candidate);
    assert_eq!(receipt.run_ordinal, 1);
    assert_eq!(receipt.execution_manifest_sha256, hex('5'));
    assert_eq!(receipt.broker_receipt_sha256, hex('6'));
    assert_eq!(receipt.attempt_ledger_sha256, hex('9'));
    assert_eq!(receipt.attempt_index_root_sha256, hex('8'));
    assert_eq!(receipt.attempt_range.start_inclusive, 0);
    assert_eq!(receipt.attempt_range.end_exclusive, 1);
    assert_eq!(
        state(boundary.as_ref()).borrow().published_receipt.as_ref(),
        Some(&receipt),
    );
}

#[test]
fn cost_authority_builds_exact_happy_receipt_with_over_ceiling_supplier_actual() {
    let statement = supplier(|value| value["actualFen"] = 450.into());
    let input = synthetic_input(); let boundary = input.boundary.clone();
    let authority = prepare_synthetic_live_cost_authority(input).unwrap();
    let receipt = commit_cost_receipt(&authority, EvaluationCondition::Candidate, Some(&statement), &clock("2026-08-30T10:01:00.000Z")).unwrap();
    assert_eq!(receipt.supplier_actual_fen, Some(450));
    assert_eq!(receipt.charged_fen, 450);
    assert!(!receipt.within_ceilings);
    assert_eq!(receipt_sha(&receipt), "e4197bede0a13e2df6609b80e77b08e3bc7d6627d0f178e783151881a678a210");
    assert_eq!(state(boundary.as_ref()).borrow().published_receipt.as_ref(), Some(&receipt));
}

pub(crate) struct CostTransactionWorld {
    _temp: tempfile::TempDir,
    root: PathBuf,
    supplier_path: PathBuf,
    receipt_path: PathBuf,
}

impl CostTransactionWorld {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn receipt_path(&self) -> &Path {
        &self.receipt_path
    }
}

fn transaction_root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    #[cfg(unix)]
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    #[cfg(unix)]
    let root = temp.path().canonicalize().unwrap();
    #[cfg(windows)]
    let root = {
        let root = temp.path().join("private");
        crate::secure_fs::create_owner_only_dir_new(&root).unwrap();
        root.canonicalize().unwrap()
    };
    (temp, root)
}

fn supplier_raw(edits: impl FnOnce(&mut Value)) -> Vec<u8> {
    json_bytes(
        include_bytes!("../tests/fixtures/contracts/06b1/supplier-statement.canonical.json"),
        |value| {
            value["pairId"] = Value::String(hex('c'));
            value["issuedAt"] = Value::String("2026-08-30T10:00:30.000Z".into());
            edits(value);
        },
    )
}

fn transaction_world(supplier: Option<&[u8]>, covered: bool) -> CostTransactionWorld {
    let (_temp, root) = transaction_root();
    for relative in ["inputs", "inputs/supplier-statements", "coordinator", "coordinator/cost"] {
        crate::secure_fs::create_owner_only_dir_new(&root.join(relative)).unwrap();
    }
    let (_, manifest_bytes) = transaction_synthetic_input(&root);
    for (ordinal, bytes) in [1_u8, 2].into_iter().zip(manifest_bytes) {
        crate::secure_fs::write_owner_only_new(
            &root.join(format!("coordinator/run-{ordinal}-manifest.json")),
            &bytes,
        )
        .unwrap();
    }
    crate::private_inventory::bootstrap_private_inventory(
        &root,
        &hex('c'),
        &hex('1'),
        "2026-08-30T09:00:00.000Z",
    )
    .unwrap();
    let supplier_path = root.join("inputs/supplier-statements/candidate.json");
    if let Some(bytes) = supplier {
        crate::secure_fs::write_owner_only_new(&supplier_path, bytes).unwrap();
        if covered {
            let old_root = crate::private_inventory::verify_private_inventory(&root)
                .unwrap_err();
            assert!(old_root.to_string().contains("inventory"));
            let inventory_bytes = fs::read(root.join("coordinator/private-inventory.jsonl")).unwrap();
            append_private_inventory_batch_from_root(
                &root,
                &sha256(&inventory_bytes),
                &[ExpectedInventoryEntry {
                    relative_path: "inputs/supplier-statements/candidate.json".into(),
                    kind: InventoryKind::File,
                    sha256: Some(sha256(bytes)),
                }],
            )
            .unwrap();
        }
    }
    CostTransactionWorld {
        _temp,
        supplier_path,
        receipt_path: root.join("coordinator/cost/candidate-receipt.json"),
        root,
    }
}

fn transaction_authority(root: &Path) -> VerifiedLiveCostAuthority {
    let (input, _) = transaction_synthetic_input(root);
    prepare_synthetic_live_cost_authority(input)
        .unwrap()
        .rebuild(crate::private_inventory::verify_private_inventory_state(root).unwrap())
        .unwrap()
}

fn receipt_transaction_authority(root: &Path) -> VerifiedLiveCostAuthority {
    let (input, _) = transaction_synthetic_input(root);
    prepare_synthetic_live_cost_authority(input).unwrap()
}

fn transaction_synthetic_input(root: &Path) -> (SyntheticLiveCostAuthority, [Vec<u8>; 2]) {
    let mut input = synthetic_input();
    input.data.private_root = root.to_str().unwrap().to_string();
    input.data.attestation.private_root = input.data.private_root.clone();
    rebind(&mut input);
    let manifest_bytes = synthetic_manifest_bytes(&input);
    bind_synthetic_manifest_hashes(&mut input, &manifest_bytes);
    (input, manifest_bytes)
}

pub(crate) fn cost_binding_world(with_over_ceiling_supplier: bool) -> CostTransactionWorld {
    cost_binding_world_for(EvaluationCondition::Candidate, with_over_ceiling_supplier)
}

pub(crate) fn generic_cost_binding_world() -> CostTransactionWorld {
    cost_binding_world_for(EvaluationCondition::Generic, false)
}

fn cost_binding_world_for(
    condition: EvaluationCondition,
    with_over_ceiling_supplier: bool,
) -> CostTransactionWorld {
    let supplier = with_over_ceiling_supplier.then(|| supplier_raw(|value| value["actualFen"] = 450.into()));
    let mut world = transaction_world(supplier.as_deref(), false);
    world.receipt_path = world.root.join(match condition {
        EvaluationCondition::Generic => "coordinator/cost/generic-receipt.json",
        EvaluationCondition::Candidate => "coordinator/cost/candidate-receipt.json",
    });
    run_transaction_for_condition(
        &world,
        condition,
        with_over_ceiling_supplier,
        &transaction_clock(),
        &mut |_| Ok(()),
    )
    .unwrap();
    world
}

pub(crate) fn cost_binding_authority(root: &Path) -> VerifiedLiveCostAuthority {
    transaction_authority(root)
}

pub(crate) fn cost_binding_clock() -> FixedClock {
    clock("2026-08-30T10:01:00.000Z")
}

fn run_transaction(
    world: &CostTransactionWorld,
    supplier: bool,
    clock: &FixedClock,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    run_transaction_for_condition(
        world,
        EvaluationCondition::Candidate,
        supplier,
        clock,
        hook,
    )
}

fn run_transaction_for_condition(
    world: &CostTransactionWorld,
    condition: EvaluationCondition,
    supplier: bool,
    clock: &FixedClock,
    hook: &mut dyn FnMut(CostReceiptCheckpoint) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    publish_cost_receipt_for_test(
        receipt_transaction_authority(&world.root),
        condition,
        supplier.then_some(world.supplier_path.as_path()),
        clock,
        hook,
    )
}

fn transaction_clock() -> FixedClock {
    clock("2026-08-30T10:01:00.000Z")
}

fn inventory_bytes(root: &Path) -> Vec<u8> {
    fs::read(root.join("coordinator/private-inventory.jsonl")).unwrap()
}

fn assert_transaction_error_contains(error: anyhow::Error, expected: &str) {
    assert!(error.to_string().contains(expected), "{error:#}");
}

fn receipt(world: &CostTransactionWorld) -> crate::cost_contracts::CostReceiptV1 {
    let bytes = fs::read(&world.receipt_path).unwrap();
    assert_eq!(
        bytes,
        crate::jcs::canonicalize_value(&crate::jcs::parse_json(&bytes).unwrap()).unwrap()
    );
    crate::cost_contracts::validate_cost_receipt(&bytes).unwrap()
}

#[test]
fn cost_receipt_transaction_publishes_exact_no_supplier_receipt() {
    let world = transaction_world(None, false);
    run_transaction(&world, false, &transaction_clock(), &mut |_| Ok(())).unwrap();
    let receipt = receipt(&world);
    assert_eq!(receipt.supplier_statement_sha256, None);
    assert_eq!(receipt.supplier_actual_fen, None);
    crate::private_inventory::verify_private_inventory(&world.root).unwrap();
}

#[test]
fn cost_receipt_transaction_publishes_pending_supplier_state_a() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap();
    assert_eq!(receipt(&world).supplier_statement_sha256, Some(sha256(&bytes)));
    crate::private_inventory::verify_private_inventory(&world.root).unwrap();
}

#[test]
fn cost_receipt_transaction_resumes_covered_supplier_state_b() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), true);
    let before = inventory_bytes(&world.root);
    run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap();
    assert!(inventory_bytes(&world.root).starts_with(&before));
    assert_eq!(receipt(&world).supplier_statement_sha256, Some(sha256(&bytes)));
}

#[test]
fn cost_receipt_transaction_rejects_omitted_pending_supplier() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    let before = inventory_bytes(&world.root);
    let error = run_transaction(&world, false, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "omitted supplier");
    assert_eq!(inventory_bytes(&world.root), before);
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_rejects_omitted_covered_supplier() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), true);
    let before = inventory_bytes(&world.root);
    let error = run_transaction(&world, false, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "omitted supplier");
    assert_eq!(inventory_bytes(&world.root), before);
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_rejects_missing_inventory_covered_supplier() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), true);
    fs::remove_file(&world.supplier_path).unwrap();
    let error = run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "supplier");
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_rejects_changed_or_wrong_identity_supplier_before_append() {
    for field in ["pairId", "providerLabel"] {
        let bytes = supplier_raw(|value| value[field] = Value::String(
            if field == "pairId" { hex('e') } else { "wrong-authority".into() },
        ));
        let world = transaction_world(Some(&bytes), false);
        let before = inventory_bytes(&world.root);
        let error = run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
        assert_transaction_error_contains(error, "supplier authority");
        assert_eq!(inventory_bytes(&world.root), before);
        assert!(!world.receipt_path.exists());
    }
}

#[test]
fn cost_receipt_transaction_rejects_wrong_leaf_or_extra_pending_leaf() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    crate::secure_fs::write_owner_only_new(
        &world.root.join("inputs/supplier-statements/generic.json"),
        &bytes,
    )
    .unwrap();
    let error = run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "unexpected path");
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_rejects_existing_receipt_before_supplier_append() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    crate::secure_fs::write_owner_only_new(&world.receipt_path, b"existing").unwrap();
    let before = inventory_bytes(&world.root);
    let error = run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "receipt destination already exists");
    assert_eq!(inventory_bytes(&world.root), before);
}

#[test]
fn cost_receipt_transaction_rejects_supplier_replacement_after_retain() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    let mut hook = |checkpoint| {
        if checkpoint == CostReceiptCheckpoint::AfterSupplierRetainBeforePairVerify {
            fs::write(&world.supplier_path, vec![b' '; bytes.len()])?;
        }
        Ok(())
    };
    let error = run_transaction(&world, true, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "retained private file");
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_before_supplier_append_leaves_pending_state() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    let before = inventory_bytes(&world.root);
    let mut hook = |checkpoint| match checkpoint {
        CostReceiptCheckpoint::BeforeSupplierInventoryAppend => anyhow::bail!("stop before supplier append"),
        _ => Ok(()),
    };
    let error = run_transaction(&world, true, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "stop before supplier append");
    assert_eq!(inventory_bytes(&world.root), before);
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_supplier_replacement_fails_expected_sha_append() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    let before = inventory_bytes(&world.root);
    let mut hook = |checkpoint| {
        if checkpoint == CostReceiptCheckpoint::BeforeSupplierInventoryAppend {
            fs::write(&world.supplier_path, vec![b' '; bytes.len()])?;
        }
        Ok(())
    };
    let error = run_transaction(&world, true, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(
        error,
        "inventory append target is absent, recorded, or mismatched",
    );
    assert_eq!(inventory_bytes(&world.root), before);
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_after_supplier_append_resumes_as_state_b() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), false);
    let mut hook = |checkpoint| match checkpoint {
        CostReceiptCheckpoint::AfterSupplierInventoryAppend => anyhow::bail!("stop after supplier append"),
        _ => Ok(()),
    };
    let error = run_transaction(&world, true, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "stop after supplier append");
    crate::private_inventory::verify_private_inventory(&world.root).unwrap();
    run_transaction(&world, true, &transaction_clock(), &mut |_| Ok(())).unwrap();
    assert_eq!(receipt(&world).supplier_statement_sha256, Some(sha256(&bytes)));
}

#[test]
fn cost_receipt_transaction_before_receipt_create_leaves_no_receipt() {
    let bytes = supplier_raw(|_| {});
    let world = transaction_world(Some(&bytes), true);
    let mut hook = |checkpoint| match checkpoint {
        CostReceiptCheckpoint::BeforeReceiptCreate => anyhow::bail!("stop before receipt create"),
        _ => Ok(()),
    };
    let error = run_transaction(&world, true, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "stop before receipt create");
    assert!(!world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_receipt_replacement_fails_expected_sha_append() {
    let world = transaction_world(None, false);
    let mut hook = |checkpoint| {
        if checkpoint == CostReceiptCheckpoint::BeforeReceiptInventoryAppend {
            let length = fs::metadata(&world.receipt_path)?.len();
            fs::write(&world.receipt_path, vec![b' '; usize::try_from(length)?])?;
        }
        Ok(())
    };
    let error = run_transaction(&world, false, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(
        error,
        "inventory append target is absent, recorded, or mismatched",
    );
    assert!(crate::private_inventory::verify_private_inventory(&world.root).is_err());
}

#[test]
fn cost_receipt_transaction_after_receipt_create_rerun_fails_closed() {
    let world = transaction_world(None, false);
    let mut hook = |checkpoint| match checkpoint {
        CostReceiptCheckpoint::AfterReceiptCreateBeforeInventoryAppend => anyhow::bail!("stop after receipt create"),
        _ => Ok(()),
    };
    let error = run_transaction(&world, false, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "stop after receipt create");
    assert!(world.receipt_path.exists());
    let error = run_transaction(&world, false, &transaction_clock(), &mut |_| Ok(())).unwrap_err();
    assert_transaction_error_contains(error, "receipt destination already exists");
}

#[test]
fn cost_receipt_transaction_parent_sync_failure_precedes_post_create_checkpoint() {
    let world = transaction_world(None, false);
    let before = inventory_bytes(&world.root);
    let sync_seen = Cell::new(false);
    let post_create_seen = Cell::new(false);
    let mut hook = |checkpoint| {
        if checkpoint == CostReceiptCheckpoint::AfterReceiptCreateBeforeInventoryAppend {
            post_create_seen.set(true);
        }
        Ok(())
    };
    let mut sync_parent = |parent: &Path| {
        assert_eq!(parent, world.root.join("coordinator/cost"));
        sync_seen.set(true);
        anyhow::bail!("injected receipt parent sync failure")
    };
    let result = publish_cost_receipt_with_sync_for_test(
        transaction_authority(&world.root),
        EvaluationCondition::Candidate,
        None,
        &transaction_clock(),
        &mut hook,
        &mut sync_parent,
    );
    assert!(sync_seen.get(), "receipt parent sync seam was not consumed");
    assert_transaction_error_contains(result.unwrap_err(), "sync retained cost receipt parent");
    assert!(!post_create_seen.get());
    assert_eq!(inventory_bytes(&world.root), before);
    assert!(world.receipt_path.exists());
}

#[test]
fn cost_receipt_transaction_after_receipt_append_has_valid_inventory() {
    let world = transaction_world(None, false);
    let mut hook = |checkpoint| match checkpoint {
        CostReceiptCheckpoint::AfterReceiptInventoryAppend => anyhow::bail!("stop after receipt append"),
        _ => Ok(()),
    };
    let error = run_transaction(&world, false, &transaction_clock(), &mut hook).unwrap_err();
    assert_transaction_error_contains(error, "stop after receipt append");
    receipt(&world);
    crate::private_inventory::verify_private_inventory(&world.root).unwrap();
}

#[test]
fn cost_receipt_transaction_calls_user_clock_exactly_once() {
    let world = transaction_world(None, false);
    let clock = transaction_clock();
    run_transaction(&world, false, &clock, &mut |_| Ok(())).unwrap();
    assert_eq!(clock.calls.get(), 1);
}
