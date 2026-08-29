use crate::model::EvaluationCondition;
use crate::model::Usage;
use chrono::DateTime;
use chrono::SecondsFormat;
use jsonschema::Validator;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

const INPUT_CAP_BYTES: usize = 64 * 1024;
const RECEIPT_CAP_BYTES: usize = 128 * 1024;

const RATE_CARD_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/provider-rate-card.schema.json");
const BILLING_POLICY_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/billing-policy.schema.json");
const FX_POLICY_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/fx-policy.schema.json");
const BUDGET_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/provider-budget-evidence.schema.json");
const SUPPLIER_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/supplier-statement.schema.json");
const RECEIPT_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/cost-receipt.schema.json");

const RATE_CARD_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/provider-rate-card.canonical.json");
const BILLING_POLICY_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/billing-policy.canonical.json");
const FX_POLICY_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/fx-policy.canonical.json");
const BUDGET_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/provider-budget-evidence.canonical.json");
const SUPPLIER_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/supplier-statement.canonical.json");
const RECEIPT_FIXTURE_BYTES: &[u8] =
    include_bytes!("../tests/fixtures/contracts/06b1/cost-receipt.canonical.json");

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ProviderRateCardV1 {
    pub(crate) schema_version: u8,
    pub(crate) provider_label: String,
    pub(crate) model_label: String,
    pub(crate) currency: String,
    pub(crate) rate_unit: String,
    pub(crate) effective_at: String,
    pub(crate) expires_at: Option<String>,
    pub(crate) uncached_input_fen_per_million: u64,
    pub(crate) cached_input_fen_per_million: u64,
    pub(crate) cache_write_input_fen_per_million: u64,
    pub(crate) output_fen_per_million: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct BillingPolicyV1 {
    pub(crate) schema_version: u8,
    pub(crate) provider_label: String,
    pub(crate) currency: String,
    pub(crate) effective_at: String,
    pub(crate) source_commitment: String,
    pub(crate) reasoning_tokens_billed_separately: bool,
    pub(crate) supplier_actual_precedence: String,
    pub(crate) rounding: String,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct FxPolicyV1 {
    pub(crate) schema_version: u8,
    pub(crate) mode: String,
    pub(crate) source_currency: String,
    pub(crate) target_currency: String,
    pub(crate) numerator: u8,
    pub(crate) denominator: u8,
    pub(crate) effective_at: String,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ProviderBudgetEvidenceV1 {
    pub(crate) schema_version: u8,
    pub(crate) provider_label: String,
    pub(crate) approval_id: String,
    pub(crate) currency: String,
    pub(crate) account_scope_commitment: String,
    pub(crate) prepaid_or_hard_limit_fen: u64,
    pub(crate) valid_from: String,
    pub(crate) valid_until: String,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct SupplierStatementV1 {
    pub(crate) schema_version: u8,
    pub(crate) pair_id: String,
    pub(crate) condition: EvaluationCondition,
    pub(crate) provider_label: String,
    pub(crate) actual_model_revision: String,
    pub(crate) currency: String,
    pub(crate) actual_fen: u64,
    pub(crate) issued_at: String,
    pub(crate) statement_reference_commitment: String,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CostReceiptV1 {
    pub(crate) schema_version: u8,
    pub(crate) pair_id: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) execution_context_sha256: String,
    pub(crate) execution_manifest_sha256: String,
    pub(crate) broker_receipt_sha256: String,
    pub(crate) pair_receipt_sha256: String,
    pub(crate) attempt_ledger_sha256: String,
    pub(crate) condition: EvaluationCondition,
    pub(crate) run_ordinal: u8,
    pub(crate) execution_mode: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) attempt_range: AttemptRangeV1,
    pub(crate) provider_label: String,
    pub(crate) actual_model_revision: String,
    pub(crate) rate_card_sha256: String,
    pub(crate) billing_policy_commitment: String,
    pub(crate) fx_policy_sha256: String,
    pub(crate) provider_budget_evidence_sha256: String,
    pub(crate) provider_request_attempt_count: u64,
    pub(crate) provider_completed_response_count: u64,
    pub(crate) usage_scope: String,
    pub(crate) usage: Usage,
    pub(crate) currency: String,
    pub(crate) rate_effective_at: String,
    pub(crate) fx: FxReceiptV1,
    pub(crate) ceilings: CostCeilingsV1,
    pub(crate) calculated_at: String,
    pub(crate) calculation: CostCalculationV1,
    pub(crate) estimated_fen: u64,
    pub(crate) supplier_statement_sha256: Option<String>,
    pub(crate) supplier_actual_fen: Option<u64>,
    pub(crate) charged_fen: u64,
    pub(crate) within_ceilings: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct AttemptRangeV1 {
    pub(crate) start_inclusive: u64,
    pub(crate) end_exclusive: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct FxReceiptV1 {
    pub(crate) mode: String,
    pub(crate) numerator: u8,
    pub(crate) denominator: u8,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CostCeilingsV1 {
    pub(crate) approved_per_run_fen: u64,
    pub(crate) approved_total_fen: u64,
    pub(crate) prepaid_or_hard_limit_fen: u64,
    pub(crate) max_provider_request_attempts: u64,
    pub(crate) max_total_tokens: u64,
    pub(crate) max_elapsed_seconds: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct CostCalculationV1 {
    pub(crate) rate_unit: String,
    pub(crate) rounding: String,
    pub(crate) reasoning_tokens_billed_separately: bool,
}

pub(crate) struct FrozenCostContracts {
    rate_card: Validator,
    billing_policy: Validator,
    fx_policy: Validator,
    budget: Validator,
    supplier_statement: Validator,
    receipt: Validator,
    _schema_sha256s: [String; 6],
}

pub(crate) struct VerifiedCostInputs {
    pub(crate) rate_card: ProviderRateCardV1,
    pub(crate) rate_card_sha256: String,
    pub(crate) billing_policy: BillingPolicyV1,
    pub(crate) billing_policy_commitment: String,
    pub(crate) fx_policy: FxPolicyV1,
    pub(crate) fx_policy_sha256: String,
    pub(crate) budget: ProviderBudgetEvidenceV1,
    pub(crate) provider_budget_evidence_sha256: String,
}

impl FrozenCostContracts {
    pub(crate) fn load() -> anyhow::Result<Self> {
        let contracts = Self {
            rate_card: crate::contracts::compile_schema(RATE_CARD_SCHEMA_BYTES)?,
            billing_policy: crate::contracts::compile_schema(BILLING_POLICY_SCHEMA_BYTES)?,
            fx_policy: crate::contracts::compile_schema(FX_POLICY_SCHEMA_BYTES)?,
            budget: crate::contracts::compile_schema(BUDGET_SCHEMA_BYTES)?,
            supplier_statement: crate::contracts::compile_schema(SUPPLIER_SCHEMA_BYTES)?,
            receipt: crate::contracts::compile_schema(RECEIPT_SCHEMA_BYTES)?,
            _schema_sha256s: [
                sha256(RATE_CARD_SCHEMA_BYTES),
                sha256(BILLING_POLICY_SCHEMA_BYTES),
                sha256(FX_POLICY_SCHEMA_BYTES),
                sha256(BUDGET_SCHEMA_BYTES),
                sha256(SUPPLIER_SCHEMA_BYTES),
                sha256(RECEIPT_SCHEMA_BYTES),
            ],
        };
        contracts.validate_inputs(
            RATE_CARD_FIXTURE_BYTES,
            BILLING_POLICY_FIXTURE_BYTES,
            FX_POLICY_FIXTURE_BYTES,
            BUDGET_FIXTURE_BYTES,
        )?;
        contracts.validate_supplier_statement(SUPPLIER_FIXTURE_BYTES)?;
        contracts.validate_receipt(RECEIPT_FIXTURE_BYTES)?;
        Ok(contracts)
    }

    pub(crate) fn validate_inputs(
        &self,
        rate_card: &[u8],
        billing_policy: &[u8],
        fx_policy: &[u8],
        budget: &[u8],
    ) -> anyhow::Result<VerifiedCostInputs> {
        let rate_card_sha256 = sha256(rate_card);
        let billing_policy_commitment = sha256(billing_policy);
        let fx_policy_sha256 = sha256(fx_policy);
        let provider_budget_evidence_sha256 = sha256(budget);
        let rate_card = self.validate_rate_card(rate_card)?;
        let billing_policy = self.validate_billing_policy(billing_policy)?;
        let fx_policy = self.validate_fx_policy(fx_policy)?;
        let budget = self.validate_provider_budget_evidence(budget)?;
        validate_input_semantics(&rate_card, &billing_policy, &fx_policy, &budget)?;
        Ok(VerifiedCostInputs {
            rate_card_sha256,
            billing_policy_commitment,
            fx_policy_sha256,
            provider_budget_evidence_sha256,
            rate_card,
            billing_policy,
            fx_policy,
            budget,
        })
    }

    pub(crate) fn validate_rate_card(&self, bytes: &[u8]) -> anyhow::Result<ProviderRateCardV1> {
        let typed = decode_exact(&self.rate_card, bytes, INPUT_CAP_BYTES)?;
        validate_rate_card_semantics(&typed)?;
        Ok(typed)
    }

    pub(crate) fn validate_billing_policy(&self, bytes: &[u8]) -> anyhow::Result<BillingPolicyV1> {
        let typed: BillingPolicyV1 = decode_exact(&self.billing_policy, bytes, INPUT_CAP_BYTES)?;
        parse_timestamp(&typed.effective_at)?;
        Ok(typed)
    }

    pub(crate) fn validate_fx_policy(&self, bytes: &[u8]) -> anyhow::Result<FxPolicyV1> {
        let typed: FxPolicyV1 = decode_exact(&self.fx_policy, bytes, INPUT_CAP_BYTES)?;
        parse_timestamp(&typed.effective_at)?;
        Ok(typed)
    }

    pub(crate) fn validate_provider_budget_evidence(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<ProviderBudgetEvidenceV1> {
        let typed: ProviderBudgetEvidenceV1 = decode_exact(&self.budget, bytes, INPUT_CAP_BYTES)?;
        if parse_timestamp(&typed.valid_from)? >= parse_timestamp(&typed.valid_until)? {
            anyhow::bail!("budget validity window is not ordered")
        }
        Ok(typed)
    }

    pub(crate) fn validate_supplier_statement(
        &self,
        bytes: &[u8],
    ) -> anyhow::Result<SupplierStatementV1> {
        let statement: SupplierStatementV1 =
            decode_exact(&self.supplier_statement, bytes, INPUT_CAP_BYTES)?;
        parse_timestamp(&statement.issued_at)?;
        Ok(statement)
    }

    pub(crate) fn validate_receipt(&self, bytes: &[u8]) -> anyhow::Result<CostReceiptV1> {
        let receipt: CostReceiptV1 = decode_exact(&self.receipt, bytes, RECEIPT_CAP_BYTES)?;
        let canonical = crate::jcs::canonicalize_value(&serde_json::to_value(&receipt)?)?;
        if canonical != bytes {
            anyhow::bail!("generated cost receipt is not exact JCS")
        }
        validate_receipt_semantics(&receipt)?;
        Ok(receipt)
    }
}

pub(crate) fn validate_cost_receipt(bytes: &[u8]) -> anyhow::Result<CostReceiptV1> {
    FrozenCostContracts::load()?.validate_receipt(bytes)
}

fn decode_exact<T>(validator: &Validator, bytes: &[u8], cap: usize) -> anyhow::Result<T>
where
    T: DeserializeOwned + serde::Serialize,
{
    if bytes.len() > cap {
        anyhow::bail!("cost contract exceeds {cap}-byte cap")
    }
    let value = crate::contracts::validate_instance(validator, bytes)?;
    let typed = serde_json::from_value(value.clone())?;
    assert_typed_deep_equality(&typed, &value)?;
    Ok(typed)
}

fn assert_typed_deep_equality(
    typed: &impl serde::Serialize,
    original: &serde_json::Value,
) -> anyhow::Result<()> {
    if serde_json::to_value(typed)? != *original {
        anyhow::bail!("typed deep equality guard rejected a lossy contract decode")
    }
    Ok(())
}

fn validate_input_semantics(
    rate_card: &ProviderRateCardV1,
    billing_policy: &BillingPolicyV1,
    _fx_policy: &FxPolicyV1,
    budget: &ProviderBudgetEvidenceV1,
) -> anyhow::Result<()> {
    if rate_card.provider_label != billing_policy.provider_label
        || rate_card.provider_label != budget.provider_label
    {
        anyhow::bail!("rate card, billing policy, and budget provider labels differ")
    }
    Ok(())
}

fn validate_rate_card_semantics(rate_card: &ProviderRateCardV1) -> anyhow::Result<()> {
    let rate_effective = parse_timestamp(&rate_card.effective_at)?;
    if let Some(expires_at) = &rate_card.expires_at
        && rate_effective >= parse_timestamp(expires_at)?
    {
        anyhow::bail!("rate card expiry must follow effective time")
    }
    Ok(())
}

fn validate_receipt_semantics(receipt: &CostReceiptV1) -> anyhow::Result<()> {
    if receipt.attempt_range.start_inclusive >= receipt.attempt_range.end_exclusive {
        anyhow::bail!("receipt attempt range is not ordered")
    }
    if receipt.provider_completed_response_count > receipt.provider_request_attempt_count {
        anyhow::bail!("completed response count exceeds request attempt count")
    }
    if receipt.ceilings.approved_per_run_fen > receipt.ceilings.approved_total_fen
        || receipt.ceilings.prepaid_or_hard_limit_fen > receipt.ceilings.approved_total_fen
    {
        anyhow::bail!("receipt ceilings exceed approved total")
    }
    let expected_charged = receipt
        .estimated_fen
        .max(receipt.supplier_actual_fen.unwrap_or(0));
    if receipt.charged_fen != expected_charged {
        anyhow::bail!("receipt charged amount does not match estimate/supplier maximum")
    }
    let total_tokens = u64::try_from(receipt.usage.total_tokens)
        .map_err(|_| anyhow::anyhow!("receipt total tokens cannot be negative"))?;
    let expected_within = total_tokens <= receipt.ceilings.max_total_tokens
        && receipt.charged_fen <= receipt.ceilings.approved_per_run_fen;
    if receipt.within_ceilings != expected_within {
        anyhow::bail!("receipt withinCeilings does not match post-run ceilings")
    }
    if receipt.provider_request_attempt_count > receipt.ceilings.max_provider_request_attempts {
        anyhow::bail!("receipt attempt count exceeds maximum request attempts")
    }
    let rate_effective = parse_timestamp(&receipt.rate_effective_at)?;
    if rate_effective > parse_timestamp(&receipt.calculated_at)? {
        anyhow::bail!("receipt rate effective time follows calculation time")
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> anyhow::Result<DateTime<chrono::FixedOffset>> {
    let timestamp = DateTime::parse_from_rfc3339(value)?;
    if timestamp.offset().local_minus_utc() != 0
        || timestamp.to_rfc3339_opts(SecondsFormat::Millis, true) != value
    {
        anyhow::bail!("timestamp is not exact UTC milliseconds")
    }
    Ok(timestamp)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
pub(crate) fn test_typed_deep_equality_probe() -> anyhow::Result<()> {
    #[derive(serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        retained: String,
        #[serde(skip_serializing)]
        hidden: String,
    }
    let value = serde_json::json!({"retained": "kept", "hidden": "dropped"});
    let typed: Probe = serde_json::from_value(value.clone())?;
    assert_typed_deep_equality(&typed, &value)
}
