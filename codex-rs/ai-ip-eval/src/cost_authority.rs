use anyhow::{Result, bail};
use chrono::{DateTime, SecondsFormat, Timelike, Utc};

use crate::{
    ArmReceipt, EvaluationCondition, ExecutionMode, ModeEvidence, NativeHeldOutAttestation,
    PairReceipt, RunManifest,
};
use crate::blind_verify::{ExactDocument, PairEvidenceCore};
use crate::cost_contracts::{AttemptRangeV1, CostCalculationV1, CostCeilingsV1, CostReceiptV1};
use crate::cost_contracts::{FxReceiptV1, SupplierStatementV1, VerifiedCostInputs};
use crate::proof_ledger::{VerifiedLedgerArm, VerifiedNativeLedger};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CostArmProjection {
    pub(crate) manifest: RunManifest,
    pub(crate) manifest_bytes: Vec<u8>,
    pub(crate) manifest_sha256: String,
    pub(crate) mode_evidence: ModeEvidence,
    pub(crate) ledger: VerifiedLedgerArm,
    pub(crate) arm_receipt: ArmReceipt,
    pub(crate) arm_receipt_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CostPairProjection {
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: String,
    pub(crate) started_at: String,
    pub(crate) deadline: String,
    pub(crate) arm_order_commitment: String,
    pub(crate) provider_endpoint_commitment: Option<String>,
    pub(crate) pair_receipt: PairReceipt,
    pub(crate) pair_receipt_sha256: String,
    pub(crate) finished_at: String,
    pub(crate) attempt_ledger_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) arms: [CostArmProjection; 2],
}

pub(crate) struct NativeCostProjectionInput<'a> {
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: &'a str,
    pub(crate) started_at: &'a str,
    pub(crate) deadline: &'a str,
    pub(crate) arm_order_commitment: &'a str,
    pub(crate) manifests: &'a [ExactDocument<RunManifest>; 2],
    pub(crate) ledger: VerifiedNativeLedger,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct LiveArmAuthority {
    pub(crate) run_ordinal: u8,
    pub(crate) condition: crate::EvaluationCondition,
    pub(crate) core_run_ordinal: u8,
    pub(crate) core_condition: crate::EvaluationCondition,
    pub(crate) core_manifest_sha256: String,
    pub(crate) core_usage: crate::Usage,
    pub(crate) core_raw_response_count: u64,
    pub(crate) manifest_sha256: String,
    pub(crate) manifest_pair_id: String,
    pub(crate) manifest_frozen_sha256: String,
    pub(crate) manifest_execution_sha256: String,
    pub(crate) manifest_execution_mode: ExecutionMode,
    pub(crate) manifest_attempt_ledger_sha256: String,
    pub(crate) manifest_attempt_index_root_sha256: String,
    pub(crate) manifest_fork_sha: String,
    pub(crate) manifest_case_sha256: String,
    pub(crate) provider_label: String,
    pub(crate) model_label: String,
    pub(crate) actual_model_revision: String,
    pub(crate) usage_scope: String,
    pub(crate) usage: crate::Usage,
    pub(crate) provider_request_attempt_count: u64,
    pub(crate) provider_completed_response_count: u64,
    pub(crate) raw_response_count: u64,
    pub(crate) authorized_per_run_fen: u64,
    pub(crate) max_provider_request_attempts: u64,
    pub(crate) max_total_tokens: i64,
    pub(crate) max_elapsed_seconds: u64,
    pub(crate) elapsed_ms: u128,
    pub(crate) mode_evidence: ModeEvidence,
    pub(crate) ledger: VerifiedLedgerArm,
    pub(crate) receipt: ArmReceipt,
    pub(crate) receipt_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct LiveAuthorityData {
    pub(crate) private_root: String,
    pub(crate) pair_id: String,
    pub(crate) fork_sha: String,
    pub(crate) frozen_sha256: String,
    pub(crate) execution_mode: ExecutionMode,
    pub(crate) execution_context_sha256: String,
    pub(crate) started_at: String,
    pub(crate) deadline: String,
    pub(crate) arm_order_commitment: String,
    pub(crate) provider_endpoint_commitment: Option<String>,
    pub(crate) pair_receipt: PairReceipt,
    pub(crate) pair_receipt_sha256: String,
    pub(crate) attempt_ledger_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) attestation: NativeHeldOutAttestation,
    pub(crate) fixed_max_output_tokens: u64,
    pub(crate) fixed_max_provider_request_attempts: u64,
    pub(crate) fixed_max_total_tokens: u64,
    pub(crate) fixed_max_elapsed_seconds: u64,
    pub(crate) arms: [LiveArmAuthority; 2],
}

/// Test-only observable source/sink used to exercise the production semantic gate without I/O.
#[cfg(test)]
pub(crate) trait SyntheticAuthorityBoundary {
    fn documents(&self) -> [Vec<u8>; 5];
    fn validate_usage(&self, data: &LiveAuthorityData) -> Result<()>;
    fn publish_receipt(&self, receipt: &CostReceiptV1) -> Result<()>;
    fn as_any(&self) -> &dyn std::any::Any;
}
#[cfg(test)]
pub(crate) struct SyntheticLiveCostAuthority {
    pub(crate) data: LiveAuthorityData,
    pub(crate) boundary: std::rc::Rc<dyn SyntheticAuthorityBoundary>,
}
enum LiveAuthoritySource {
    Production(Box<PairEvidenceCore>),
    #[cfg(test)]
    Synthetic(
        std::rc::Rc<dyn SyntheticAuthorityBoundary>,
        Option<crate::private_inventory::VerifiedPrivateInventory>,
    ),
}
pub(crate) struct VerifiedLiveCostAuthority {
    data: LiveAuthorityData,
    inputs: VerifiedCostInputs,
    source: LiveAuthoritySource,
    pub(crate) transaction: Option<crate::cost_inputs::PreparedCostReceiptContext>,
}
pub(crate) struct RetainedSupplierStatement {
    statement: SupplierStatementV1,
    sha256: String,
}

pub(crate) fn retain_supplier_statement(
    bytes: &[u8],
) -> Result<RetainedSupplierStatement> {
    use sha2::Digest;

    Ok(RetainedSupplierStatement {
        statement: crate::cost_contracts::FrozenCostContracts::load()?
            .validate_supplier_statement(bytes)?,
        sha256: format!("{:x}", sha2::Sha256::digest(bytes)),
    })
}
pub(crate) trait CostClock {
    fn now(&self) -> Result<DateTime<Utc>>;
}
pub(crate) fn prepare_live_cost_authority(
    core: PairEvidenceCore,
) -> Result<VerifiedLiveCostAuthority> {
    crate::blind_finalize::reverify_sealed_pair_authority(&core)?;
    let (data, attestation, raw) = production_parts(&core)?;
    prepare_authority(data, &attestation, raw.each_ref().map(Vec::as_slice), LiveAuthoritySource::Production(Box::new(core)))
}
#[cfg(test)]
pub(crate) fn prepare_synthetic_live_cost_authority(
    input: SyntheticLiveCostAuthority,
) -> Result<VerifiedLiveCostAuthority> {
    let SyntheticLiveCostAuthority { data, boundary } = input;
    let source = LiveAuthoritySource::Synthetic(boundary.clone(), None);
    boundary.validate_usage(&data)?;
    let documents = boundary.documents();
    prepare_authority(data, &documents[0], [&documents[1], &documents[2], &documents[3], &documents[4]].map(Vec::as_slice), source)
}
pub(crate) fn commit_cost_receipt(
    authority: &VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    statement: Option<&RetainedSupplierStatement>,
    clock: &dyn CostClock,
) -> Result<CostReceiptV1> {
    authority.reverify()?;
    let now = clock.now()?;
    let calculated_at = now.with_nanosecond(now.nanosecond() / 1_000_000 * 1_000_000)
        .ok_or_else(|| anyhow::anyhow!("normalize calculation clock"))?
        .to_rfc3339_opts(SecondsFormat::Millis, true);
    let receipt =
        calculate_cost_receipt_at(authority, condition, statement, &calculated_at)?;
    #[cfg(test)]
    if let LiveAuthoritySource::Synthetic(boundary, _) = &authority.source {
        boundary.publish_receipt(&receipt)?;
    }
    Ok(receipt)
}

fn calculate_cost_receipt_at(
    authority: &VerifiedLiveCostAuthority,
    condition: crate::EvaluationCondition,
    statement: Option<&RetainedSupplierStatement>,
    calculated_at: &str,
) -> Result<CostReceiptV1> {
    let mut selected = authority.data.arms.iter().filter(|arm| arm.condition == condition);
    let arm = selected.next().ok_or_else(|| anyhow::anyhow!("selected receipt condition is absent"))?;
    if selected.next().is_some() { bail!("selected receipt condition is ambiguous") }
    let calculated = parse_millis(calculated_at)?;
    if calculated < parse_millis(&authority.data.pair_receipt.finished_at)? {
        bail!("calculation is before finish")
    }
    if calculated >= parse_millis(&authority.data.attestation.retention_deadline)? {
        bail!("calculation is at or after retention deadline")
    }
    let supplier_actual_fen = if let Some(retained) = statement {
        let issued = parse_millis(&retained.statement.issued_at)?;
        if issued < parse_millis(&arm.receipt.sealed_at)? || issued > calculated {
            bail!("supplier issue time is outside the selected calculation interval")
        }
        if retained.statement.condition != condition
            || retained.statement.pair_id != authority.data.pair_id
            || retained.statement.provider_label != arm.provider_label
            || retained.statement.actual_model_revision != arm.actual_model_revision
        {
            bail!("supplier authority differs from the selected arm")
        }
        Some(retained.statement.actual_fen)
    } else {
        None
    };
    let attestation = &authority.data.attestation;
    let calculated = crate::cost::calculate_cost(&crate::cost::CostCalculationInput {
        inputs: &authority.inputs,
        usage: &arm.usage,
        supplier_actual_fen,
        approved_per_run_fen: attestation.approved_per_run_fen,
        max_total_tokens: attestation.max_total_tokens_per_run,
    })?;
    let receipt = CostReceiptV1 {
        schema_version: 1, pair_id: authority.data.pair_id.clone(),
        frozen_run_context_sha256: authority.data.frozen_sha256.clone(),
        execution_context_sha256: authority.data.execution_context_sha256.clone(),
        execution_manifest_sha256: arm.manifest_sha256.clone(), broker_receipt_sha256: arm.receipt_sha256.clone(),
        pair_receipt_sha256: authority.data.pair_receipt_sha256.clone(), attempt_ledger_sha256: authority.data.attempt_ledger_sha256.clone(),
        condition, run_ordinal: arm.run_ordinal, execution_mode: "live".into(),
        attempt_index_root_sha256: arm.ledger.attempt_index_prefix_root_sha256.clone(),
        attempt_range: AttemptRangeV1 { start_inclusive: arm.ledger.global_start_inclusive, end_exclusive: arm.ledger.global_end_exclusive },
        provider_label: arm.provider_label.clone(), actual_model_revision: arm.actual_model_revision.clone(),
        rate_card_sha256: authority.inputs.rate_card_sha256.clone(), billing_policy_commitment: authority.inputs.billing_policy_commitment.clone(),
        fx_policy_sha256: authority.inputs.fx_policy_sha256.clone(), provider_budget_evidence_sha256: authority.inputs.provider_budget_evidence_sha256.clone(),
        provider_request_attempt_count: arm.provider_request_attempt_count, provider_completed_response_count: arm.provider_completed_response_count,
        usage_scope: "rootSessionTree".into(), usage: arm.usage.clone(), currency: "CNY".into(),
        rate_effective_at: authority.inputs.rate_card.effective_at.clone(), fx: FxReceiptV1 { mode: "notApplicable".into(), numerator: 1, denominator: 1 },
        ceilings: CostCeilingsV1 { approved_per_run_fen: attestation.approved_per_run_fen, approved_total_fen: attestation.approved_total_fen,
            prepaid_or_hard_limit_fen: authority.inputs.budget.prepaid_or_hard_limit_fen,
            max_provider_request_attempts: attestation.max_provider_request_attempts_per_run,
            max_total_tokens: attestation.max_total_tokens_per_run, max_elapsed_seconds: attestation.max_elapsed_seconds_per_run },
        calculated_at: calculated_at.to_string(),
        calculation: CostCalculationV1 { rate_unit: "fenPerMillionTokens".into(),
            rounding: "ceilingToFen".into(), reasoning_tokens_billed_separately: false },
        estimated_fen: calculated.estimated_fen, supplier_statement_sha256: statement.map(|value| value.sha256.clone()),
        supplier_actual_fen: calculated.supplier_actual_fen, charged_fen: calculated.charged_fen, within_ceilings: calculated.within_ceilings,
    };
    let value = serde_json::to_value(&receipt)?;
    reject_unsafe_integers(&value)?;
    let bytes = crate::jcs::canonicalize_value(&value)?;
    let validated = crate::cost_contracts::validate_cost_receipt(&bytes)?;
    if validated != receipt { bail!("canonical receipt validator changed the typed value") }
    Ok(validated)
}

pub(crate) fn revalidate_cost_receipt(
    authority: &VerifiedLiveCostAuthority,
    statement: Option<&RetainedSupplierStatement>,
    expected: &CostReceiptV1,
) -> Result<()> {
    authority.reverify()?;
    let actual = calculate_cost_receipt_at(
        authority, expected.condition, statement, &expected.calculated_at,
    )?;
    if actual == *expected { return Ok(()) }
    bail!("fresh authority changed the whole prospective cost receipt")
}

fn production_parts(core: &PairEvidenceCore) -> Result<(LiveAuthorityData, Vec<u8>, [Vec<u8>; 4])> {
    let crate::blind::FrozenInputToken::Native { frozen, content } = &core.inputs else {
        bail!("Replay cannot construct a Live cost authority")
    };
    let projection = core.cost_projection.as_ref().ok_or_else(|| anyhow::anyhow!("Live cost projection is absent"))?;
    if core.mode != projection.execution_mode { bail!("core and cost projection modes differ") }
    let retained = content.projection();
    let arms = [0_usize, 1].map(|index| {
        let projected = &projection.arms[index];
        let manifest = &projected.manifest;
        let core_arm = &core.arms[index];
        LiveArmAuthority {
            run_ordinal: manifest.run_ordinal, condition: manifest.condition,
            core_run_ordinal: core_arm.run_ordinal, core_condition: core_arm.condition,
            core_manifest_sha256: core_arm.run_manifest_raw_sha256.clone(), core_usage: core_arm.usage.clone(),
            core_raw_response_count: core_arm.raw_response_count,
            manifest_sha256: projected.manifest_sha256.clone(), manifest_pair_id: manifest.pair_id.clone(),
            manifest_frozen_sha256: manifest.frozen_run_context_sha256.clone(),
            manifest_execution_sha256: manifest.execution_context_sha256.clone(), manifest_execution_mode: manifest.execution_mode,
            manifest_attempt_ledger_sha256: manifest.broker_attempt_ledger_sha256.clone(),
            manifest_attempt_index_root_sha256: manifest.attempt_index_root_sha256.clone(),
            manifest_fork_sha: manifest.fork_sha.clone(), manifest_case_sha256: manifest.case_sha256.clone(),
            provider_label: manifest.provider_label.clone(), model_label: manifest.model_label.clone(),
            actual_model_revision: manifest.actual_model_revision.clone(),
            usage_scope: manifest.usage_scope.clone(), usage: manifest.usage.clone(), provider_request_attempt_count: manifest.provider_request_attempt_count,
            provider_completed_response_count: manifest.provider_completed_response_count, raw_response_count: manifest.raw_response_count,
            authorized_per_run_fen: manifest.authorized_evaluation_run_cost_fen, max_provider_request_attempts: manifest.max_provider_request_attempts,
            max_total_tokens: manifest.max_total_tokens, max_elapsed_seconds: manifest.max_elapsed_seconds, elapsed_ms: manifest.elapsed_ms,
            mode_evidence: projected.mode_evidence.clone(), ledger: projected.ledger.clone(),
            receipt: projected.arm_receipt.clone(), receipt_sha256: projected.arm_receipt_sha256.clone(),
        }
    });
    let data = LiveAuthorityData {
        private_root: core.private_root.to_str().ok_or_else(|| anyhow::anyhow!("private root is not UTF-8"))?.into(),
        pair_id: core.pair_id.clone(), fork_sha: core.fork_sha.clone(),
        frozen_sha256: core.frozen_run_context_sha256.clone(), execution_mode: projection.execution_mode,
        execution_context_sha256: projection.execution_context_sha256.clone(), started_at: projection.started_at.clone(), deadline: projection.deadline.clone(),
        arm_order_commitment: projection.arm_order_commitment.clone(), provider_endpoint_commitment: projection.provider_endpoint_commitment.clone(),
        pair_receipt: projection.pair_receipt.clone(), pair_receipt_sha256: projection.pair_receipt_sha256.clone(),
        attempt_ledger_sha256: projection.attempt_ledger_sha256.clone(),
        attempt_index_root_sha256: projection.attempt_index_root_sha256.clone(),
        attestation: retained.attestation.clone(), fixed_max_output_tokens: frozen.max_output_tokens(),
        fixed_max_provider_request_attempts: frozen.max_attempts_per_arm(), fixed_max_total_tokens: frozen.max_total_tokens_per_run(),
        fixed_max_elapsed_seconds: frozen.max_elapsed_seconds_per_run(), arms,
    };
    let raw: [Vec<u8>; 4] = ["rateCard", "billingPolicy", "fxPolicy", "providerBudgetEvidence"].into_iter()
        .map(|name| frozen.artifact_bytes(name)).collect::<Result<Vec<_>>>()?.try_into().map_err(|_| anyhow::anyhow!("missing frozen cost input"))?;
    Ok((data, retained.attestation_bytes.clone(), raw))
}
fn prepare_authority(data: LiveAuthorityData, attestation_raw: &[u8], raw: [&[u8]; 4], source: LiveAuthoritySource) -> Result<VerifiedLiveCostAuthority> {
    let inputs = validate_parts(&data, attestation_raw, raw)?;
    Ok(VerifiedLiveCostAuthority { data, inputs, source, transaction: None })
}
fn validate_parts(data: &LiveAuthorityData, attestation_raw: &[u8], raw: [&[u8]; 4]) -> Result<VerifiedCostInputs> {
    let attestation = crate::contracts::FrozenContracts::load()?.validate_native_attestation(attestation_raw)
        .map_err(|error| anyhow::anyhow!("attestation contract rejected exact bytes: {error}"))?;
    if attestation != data.attestation { bail!("attestation raw bytes differ from typed authority") }
    let inputs = crate::cost_contracts::FrozenCostContracts::load()?.validate_inputs(raw[0], raw[1], raw[2], raw[3])?;
    validate_authority(data, &inputs, &sha256(attestation_raw))?;
    Ok(inputs)
}

impl VerifiedLiveCostAuthority {
    pub(crate) fn reverify(&self) -> Result<()> {
        let inputs = match &self.source {
            LiveAuthoritySource::Production(core) => {
                crate::blind_finalize::reverify_sealed_pair_authority(core)?;
                let (data, attestation, raw) = production_parts(core)?;
                if data != self.data { bail!("Live cost authority changed during reverify") }
                validate_parts(&data, &attestation, raw.each_ref().map(Vec::as_slice))?
            }
            #[cfg(test)]
            LiveAuthoritySource::Synthetic(boundary, inventory) => {
                if let Some(inventory) = inventory {
                    inventory.reverify_unchanged()?;
                }
                boundary.validate_usage(&self.data)?;
                let documents = boundary.documents();
                validate_parts(&self.data, &documents[0], [&documents[1], &documents[2], &documents[3], &documents[4]].map(Vec::as_slice))?
            }
        };
        if !same_inputs(&inputs, &self.inputs)? { bail!("Live cost inputs changed during reverify") }
        Ok(())
    }

    pub(crate) fn transaction_binding(&self) -> (&str, &str, &str) {
        (&self.data.pair_id, &self.data.frozen_sha256, &self.data.private_root)
    }

    pub(crate) fn rebuild(
        self,
        inventory: crate::private_inventory::VerifiedPrivateInventory,
    ) -> Result<Self> {
        let root = std::path::Path::new(&self.data.private_root).to_path_buf();
        inventory.verify_binding(&self.data.pair_id, &self.data.frozen_sha256, &root)?;
        match self.source {
            LiveAuthoritySource::Production(_) => {
                let path = root.join("frozen-run-context.json");
                let snapshot = crate::blind::read_context_snapshot(&path)?;
                let inputs = snapshot.verified_inputs()?;
                let core = crate::blind_verify::verify_pair_evidence_core_from(
                    &snapshot, inputs, inventory,
                )?;
                prepare_live_cost_authority(core)
            }
            #[cfg(test)]
            LiveAuthoritySource::Synthetic(boundary, _) => {
                boundary.validate_usage(&self.data)?;
                let documents = boundary.documents();
                let raw = [&documents[1], &documents[2], &documents[3], &documents[4]];
                let source = LiveAuthoritySource::Synthetic(boundary, Some(inventory));
                prepare_authority(self.data, &documents[0], raw.map(Vec::as_slice), source)
            }
        }
    }
}

fn same_inputs(left: &VerifiedCostInputs, right: &VerifiedCostInputs) -> Result<bool> {
    Ok(left.rate_card_sha256 == right.rate_card_sha256 && left.billing_policy_commitment == right.billing_policy_commitment
        && left.fx_policy_sha256 == right.fx_policy_sha256 && left.provider_budget_evidence_sha256 == right.provider_budget_evidence_sha256
        && serde_json::to_value((&left.rate_card, &left.billing_policy, &left.fx_policy, &left.budget))?
            == serde_json::to_value((&right.rate_card, &right.billing_policy, &right.fx_policy, &right.budget))?)
}

fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(bytes))
}

fn validate_authority(data: &LiveAuthorityData, inputs: &VerifiedCostInputs,
    exact_attestation_sha256: &str) -> Result<()> {
    let attestation = &data.attestation;
    let [first, second] = &data.arms;
    let live_mode_matches = data.execution_mode == ExecutionMode::Live
        && attestation.execution_mode == "live"
        && first.mode_evidence == second.mode_evidence;
    if !live_mode_matches { bail!("Live mode evidence is inconsistent") }
    let ModeEvidence::Live {
        attestation_sha256, provider_budget_evidence_sha256, approval_commitment,
        provider_endpoint_commitment, provider_role, arm_order_commitment, rate_card_sha256,
        billing_policy_sha256, fx_policy_sha256, authorized_pair_cost_fen, retention_deadline,
    } = &first.mode_evidence else { bail!("Live authority requires exact Live mode evidence") };
    let endpoint_matches = data.provider_endpoint_commitment.as_deref()
        == Some(provider_endpoint_commitment)
        && attestation.target_provider_model_evidence_commitment == *provider_endpoint_commitment;
    if !endpoint_matches { bail!("provider endpoint differs from target provider evidence") }
    let provider_role_matches = serde_json::to_value(provider_role)?
        == serde_json::Value::String(attestation.provider_role.clone());
    if !provider_role_matches { bail!("provider role differs from attestation") }
    let cost_commitments_match = attestation_sha256 == exact_attestation_sha256
        && provider_budget_evidence_sha256 == &inputs.provider_budget_evidence_sha256
        && approval_commitment == &attestation.approval_id
        && rate_card_sha256 == &inputs.rate_card_sha256
        && billing_policy_sha256 == &inputs.billing_policy_commitment
        && fx_policy_sha256.as_deref() == Some(&inputs.fx_policy_sha256)
        && attestation.rate_card_sha256 == inputs.rate_card_sha256
        && attestation.billing_policy_commitment == inputs.billing_policy_commitment
        && attestation.fx_policy_sha256 == inputs.fx_policy_sha256
        && attestation.provider_budget_evidence_sha256 == inputs.provider_budget_evidence_sha256
        && *authorized_pair_cost_fen == attestation.approved_total_fen
        && retention_deadline == &attestation.retention_deadline;
    if !cost_commitments_match { bail!("attestation cost input commitments differ") }
    let order_matches = data.arm_order_commitment == *arm_order_commitment
        && data.pair_receipt.arm_order_commitment == data.arm_order_commitment;
    if !order_matches { bail!("arm order commitment differs") }
    if !has_exact_condition_set(first.condition, second.condition) { bail!("authority does not contain exact ordered conditions") }
    let candidate_frozen = parse_millis(&attestation.candidate_frozen_at)?;
    let case_selected = parse_millis(&attestation.case_selected_at)?;
    let signed = parse_millis(&attestation.signed_at)?; let started = parse_millis(&data.started_at)?;
    let finished = parse_millis(&data.pair_receipt.finished_at)?; let deadline = parse_millis(&data.deadline)?;
    let retention = parse_millis(&attestation.retention_deadline)?;
    let attestation_timeline_valid = candidate_frozen < case_selected
        && case_selected <= signed && signed <= started;
    if !attestation_timeline_valid { bail!("attestation signedAt timeline is invalid") }
    if started >= finished || finished > deadline { bail!("execution deadline timeline is invalid") }
    if finished >= retention { bail!("retention timeline is inverted or expired") }
    if inputs.rate_card.effective_at != attestation.rate_effective_at { bail!("rate effective time differs from attestation") }
    for effective in [&inputs.rate_card.effective_at, &inputs.billing_policy.effective_at,
        &inputs.fx_policy.effective_at] {
        if parse_millis(effective)? > started { bail!("cost policy is effective after run start") }
    }
    let rate_expired = inputs.rate_card.expires_at.as_ref().is_some_and(|value|
        parse_millis(value).map(|time| time <= finished).unwrap_or(true));
    if rate_expired { bail!("rate expiry does not strictly follow finish") }
    if parse_millis(&inputs.budget.valid_from)? > started
        || parse_millis(&inputs.budget.valid_until)? <= finished
    { bail!("budget validity window does not cover the run") }
    let approval_matches = attestation.candidate_sha == data.fork_sha
        && attestation.private_root == data.private_root
        && attestation.approved_per_run_fen <= attestation.approved_total_fen
        && inputs.budget.approval_id == attestation.approval_id
        && inputs.budget.prepaid_or_hard_limit_fen <= attestation.approved_total_fen;
    if !approval_matches { bail!("candidate, private root, or approved total authority differs") }
    if data.fixed_max_output_tokens != attestation.max_output_tokens_per_request { bail!("fixed max output tokens differ") }
    let frozen_limits_match = data.fixed_max_provider_request_attempts
        == attestation.max_provider_request_attempts_per_run
        && data.fixed_max_total_tokens == attestation.max_total_tokens_per_run
        && data.fixed_max_elapsed_seconds == attestation.max_elapsed_seconds_per_run;
    if !frozen_limits_match { bail!("frozen hard gates differ from attestation") }
    let provider_matches = first.provider_label == second.provider_label
        && first.model_label == second.model_label
        && first.actual_model_revision == second.actual_model_revision
        && inputs.rate_card.provider_label == first.provider_label
        && inputs.rate_card.model_label == first.model_label
        && inputs.billing_policy.provider_label == first.provider_label
        && inputs.budget.provider_label == first.provider_label;
    if !provider_matches { bail!("provider or model identity differs") }
    let currencies = (inputs.rate_card.currency.as_str(), inputs.billing_policy.currency.as_str(),
        inputs.budget.currency.as_str(), attestation.rate_currency.as_str());
    let fx = (
        inputs.fx_policy.mode.as_str(), inputs.fx_policy.source_currency.as_str(),
        inputs.fx_policy.target_currency.as_str(), inputs.fx_policy.numerator,
        inputs.fx_policy.denominator,
    );
    if currencies != ("CNY", "CNY", "CNY", "CNY") || fx != ("notApplicable", "CNY", "CNY", 1, 1) { bail!("cost currency or FX authority differs") }
    let final_root_matches = data.pair_receipt.final_attempt_index_root
        == second.ledger.attempt_index_prefix_root_sha256
        && data.pair_receipt.final_attempt_index_root == second.receipt.attempt_index_merkle_root;
    if !final_root_matches { bail!("pair final attempt commitment differs from ordered second arm") }
    if data.attempt_ledger_sha256 != second.ledger.attempt_index_prefix_sha256 { bail!("full attempt ledger commitment differs from ordered second arm") }
    let pair_identity_matches = data.pair_receipt.pair_id == data.pair_id
        && data.pair_receipt.frozen_run_context_sha256 == data.frozen_sha256
        && data.pair_receipt.execution_context_sha256 == data.execution_context_sha256
        && data.pair_receipt.final_attempt_index_root == data.attempt_index_root_sha256
        && data.pair_receipt.first_arm_receipt_sha256 == first.receipt_sha256
        && data.pair_receipt.second_arm_receipt_sha256 == second.receipt_sha256
        && data.pair_receipt.first_run_manifest_sha256.as_deref() == Some(&first.manifest_sha256)
        && data.pair_receipt.second_run_manifest_sha256.as_deref() == Some(&second.manifest_sha256);
    let pair_counts_match = first.receipt.attempt_count.checked_add(second.receipt.attempt_count)
        == Some(data.pair_receipt.total_attempt_count)
        && first.receipt.completion_count.checked_add(second.receipt.completion_count)
            == Some(data.pair_receipt.total_completion_count)
        && first.receipt.failure_count.checked_add(second.receipt.failure_count)
            == Some(data.pair_receipt.total_failure_count)
        && first.receipt.timeout_count.checked_add(second.receipt.timeout_count)
            == Some(data.pair_receipt.total_timeout_count);
    if !pair_identity_matches || !pair_counts_match { bail!("pair receipt identity or counts differ") }
    for (index, arm) in data.arms.iter().enumerate() {
        let ordinal = u8::try_from(index + 1)?;
        let ordinal_matches = arm.run_ordinal == ordinal && arm.core_run_ordinal == ordinal
            && arm.receipt.run_ordinal == ordinal && arm.condition == arm.core_condition
            && arm.condition == arm.ledger.condition && arm.condition == arm.receipt.condition;
        if !ordinal_matches { bail!("arm ordinal or condition differs") }
        let previous_receipt_matches = (index == 0 && arm.receipt.previous_arm_receipt_sha256.is_none())
            || (index == 1 && arm.receipt.previous_arm_receipt_sha256.as_deref() == Some(&first.receipt_sha256));
        let receipt_identity_matches = arm.receipt.pair_id == data.pair_id
            && arm.receipt.frozen_run_context_sha256 == data.frozen_sha256
            && arm.receipt.execution_context_sha256 == data.execution_context_sha256
            && arm.receipt.first_condition == first.condition
            && arm.receipt.second_condition == second.condition && previous_receipt_matches;
        if !receipt_identity_matches { bail!("arm receipt pair or order identity differs") }
        let manifest_identity_matches = arm.manifest_pair_id == data.pair_id
            && arm.manifest_frozen_sha256 == data.frozen_sha256
            && arm.manifest_execution_sha256 == data.execution_context_sha256
            && arm.manifest_execution_mode == ExecutionMode::Live
            && arm.manifest_fork_sha == data.fork_sha
            && arm.manifest_case_sha256 == attestation.case_sha256;
        if !manifest_identity_matches { bail!("manifest pair or candidate identity differs") }
        let usage_matches = arm.usage_scope == "completeNativeThreadTree"
            && arm.usage == arm.core_usage && arm.usage == arm.ledger.usage;
        if !usage_matches { bail!("broker/App Server response usage multiset differs") }
        let document_sha_matches = arm.manifest_sha256 == arm.core_manifest_sha256
            && arm.manifest_sha256 == arm.ledger.run_manifest_sha256
            && arm.receipt.run_manifest_sha256.as_deref() == Some(&arm.manifest_sha256)
            && arm.receipt_sha256 == arm.receipt.receipt_sha256;
        if !document_sha_matches { bail!("manifest or arm receipt SHA differs") }
        let attempt_commitments_match = arm.manifest_attempt_ledger_sha256
            == arm.ledger.attempt_index_prefix_sha256
            && arm.manifest_attempt_ledger_sha256 == arm.receipt.attempt_index_file_sha256
            && arm.manifest_attempt_index_root_sha256 == arm.ledger.attempt_index_prefix_root_sha256
            && arm.manifest_attempt_index_root_sha256 == arm.receipt.attempt_index_merkle_root;
        if !attempt_commitments_match { bail!("manifest attempt commitment differs from ledger or receipt") }
        let response_counts_match = arm.provider_request_attempt_count
            == arm.ledger.provider_request_attempt_count
            && arm.provider_completed_response_count == arm.ledger.provider_completed_response_count
            && arm.raw_response_count == arm.core_raw_response_count
            && arm.raw_response_count == arm.ledger.raw_response_count
            && arm.receipt.attempt_count == arm.ledger.provider_request_attempt_count
            && arm.receipt.completion_count == arm.ledger.provider_completed_response_count;
        if !response_counts_match { bail!("arm response counts differ") }
        let sealed_counts_match = arm.receipt.in_flight == 0 && arm.receipt.failure_count == 0
            && arm.receipt.timeout_count == 0
            && arm.receipt.global_attempt_end_exclusive
                .checked_sub(arm.receipt.global_attempt_start_inclusive)
                == Some(arm.receipt.attempt_count);
        if !sealed_counts_match { bail!("sealed receipt invariants differ") }
        let range_matches = arm.receipt.global_attempt_start_inclusive == arm.ledger.global_start_inclusive
            && arm.receipt.global_attempt_end_exclusive == arm.ledger.global_end_exclusive
            && arm.receipt.attempt_index_file_sha256 == arm.ledger.attempt_index_prefix_sha256
            && arm.receipt.attempt_index_merkle_root == arm.ledger.attempt_index_prefix_root_sha256;
        if !range_matches { bail!("attempt range or root differs") }
        let hard_gates_match = arm.authorized_per_run_fen == attestation.approved_per_run_fen
            && arm.max_provider_request_attempts == attestation.max_provider_request_attempts_per_run
            && u64::try_from(arm.max_total_tokens)? == attestation.max_total_tokens_per_run
            && arm.max_elapsed_seconds == attestation.max_elapsed_seconds_per_run;
        if !hard_gates_match { bail!("manifest hard gates differ") }
        if arm.provider_request_attempt_count > attestation.max_provider_request_attempts_per_run { bail!("provider request attempt cap exceeded") }
        let elapsed_limit = u128::from(attestation.max_elapsed_seconds_per_run)
            .checked_mul(1_000).ok_or_else(|| anyhow::anyhow!("elapsed hard gate overflow"))?;
        if arm.elapsed_ms > elapsed_limit || parse_millis(&arm.receipt.sealed_at)? > deadline { bail!("arm deadline or elapsed hard gate exceeded") }
    }
    if parse_millis(&first.receipt.sealed_at)? > parse_millis(&second.receipt.sealed_at)?
        || parse_millis(&second.receipt.sealed_at)? > finished { bail!("sealed receipt timestamp order differs") }
    Ok(())
}

fn has_exact_condition_set(first: EvaluationCondition, second: EvaluationCondition) -> bool {
    matches!([first, second], [EvaluationCondition::Generic, EvaluationCondition::Candidate]
        | [EvaluationCondition::Candidate, EvaluationCondition::Generic])
}
fn parse_millis(value: &str) -> Result<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(value)?;
    if parsed.offset().local_minus_utc() != 0
        || parsed.to_rfc3339_opts(SecondsFormat::Millis, true) != value
    { bail!("timestamp is not exact UTC milliseconds") }
    Ok(parsed.with_timezone(&Utc))
}

fn reject_unsafe_integers(value: &serde_json::Value) -> Result<()> {
    match value {
        serde_json::Value::Number(number)
            if number.as_u64().is_some_and(|value| value > 9_007_199_254_740_991) =>
                bail!("receipt integer exceeds the JCS safe integer boundary"),
        serde_json::Value::Array(values) => for value in values { reject_unsafe_integers(value)? },
        serde_json::Value::Object(values) => for value in values.values() { reject_unsafe_integers(value)? },
        _ => {}
    }
    Ok(())
}

pub(crate) fn project_verified_native_cost(
    input: NativeCostProjectionInput<'_>,
) -> Result<CostPairProjection> {
    let provider_endpoint_commitment = match (
        input.execution_mode,
        &input.manifests[0].typed.mode_evidence,
        &input.manifests[1].typed.mode_evidence,
    ) {
        (
            ExecutionMode::Mock,
            first @ ModeEvidence::Mock { .. },
            second @ ModeEvidence::Mock { .. },
        ) if first == second => None,
        (
            ExecutionMode::Live,
            first @ ModeEvidence::Live {
                provider_endpoint_commitment,
                ..
            },
            second @ ModeEvidence::Live { .. },
        ) if first == second => Some(provider_endpoint_commitment.clone()),
        (ExecutionMode::Replay, _, _) => bail!("Replay has no Native cost projection"),
        (ExecutionMode::Mock | ExecutionMode::Live, _, _) => {
            bail!("Native cost projection mode evidence is inconsistent")
        }
    };
    let VerifiedNativeLedger {
        attempt_ledger,
        arm_receipts,
        pair_receipt,
        pair_receipt_sha256,
    } = input.ledger;
    let crate::proof_ledger::VerifiedAttemptLedger {
        attempt_index_sha256,
        attempt_index_root_sha256,
        arms: [first_ledger, second_ledger],
        ..
    } = attempt_ledger;
    let [first_receipt, second_receipt] = arm_receipts;
    let [first_manifest, second_manifest] = input.manifests;
    let project_arm =
        |manifest: &ExactDocument<RunManifest>, ledger, arm_receipt: ArmReceipt| {
            CostArmProjection {
                manifest: manifest.typed.clone(),
                manifest_bytes: manifest.raw_bytes.clone(),
                manifest_sha256: manifest.sha256.clone(),
                mode_evidence: manifest.typed.mode_evidence.clone(),
                ledger,
                arm_receipt_sha256: arm_receipt.receipt_sha256.clone(),
                arm_receipt,
            }
        };
    let finished_at = pair_receipt.finished_at.clone();

    Ok(CostPairProjection {
        execution_mode: input.execution_mode,
        execution_context_sha256: input.execution_context_sha256.to_string(),
        started_at: input.started_at.to_string(),
        deadline: input.deadline.to_string(),
        arm_order_commitment: input.arm_order_commitment.to_string(),
        provider_endpoint_commitment,
        pair_receipt,
        pair_receipt_sha256,
        finished_at,
        attempt_ledger_sha256: attempt_index_sha256,
        attempt_index_root_sha256,
        arms: [
            project_arm(first_manifest, first_ledger, first_receipt),
            project_arm(second_manifest, second_ledger, second_receipt),
        ],
    })
}
