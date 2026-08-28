use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

use crate::EvaluationCondition;
use crate::RunManifest;
use crate::Usage;

use super::BoundRunManifest;
use super::NativeLedgerBinding;
use super::ParsedAttempt;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedLedgerArm {
    pub(crate) condition: EvaluationCondition,
    pub(crate) global_start_inclusive: u64,
    pub(crate) global_end_exclusive: u64,
    pub(crate) provider_request_attempt_count: u64,
    pub(crate) provider_completed_response_count: u64,
    pub(crate) raw_response_count: u64,
    pub(crate) usage: Usage,
    pub(crate) run_manifest_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedAttemptLedger {
    pub(crate) attempt_index_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) model_label: String,
    pub(crate) actual_model_revision: String,
    pub(crate) deployment_or_fingerprint_commitment: Option<String>,
    pub(crate) arms: [VerifiedLedgerArm; 2],
}

pub(crate) fn derive_native_attempt_ledger(
    private_root: &Path,
    binding: &NativeLedgerBinding<'_>,
) -> Result<VerifiedAttemptLedger> {
    let parsed = super::parse_native_attempt_ledger(private_root, binding)?;
    validate_semantic_binding(binding)?;
    let first_manifest = binding.manifests[0].manifest;
    let second_manifest = binding.manifests[1].manifest;
    if first_manifest.actual_model_revision != second_manifest.actual_model_revision {
        bail!("cross-arm model identity is invalid");
    }
    if first_manifest.model_label != second_manifest.model_label {
        bail!("cross-arm model label identity is invalid");
    }
    if first_manifest.deployment_or_fingerprint_commitment
        != second_manifest.deployment_or_fingerprint_commitment
    {
        bail!("cross-arm deployment identity is invalid");
    }

    let mut arms = Vec::with_capacity(2);
    for (index, parsed_arm) in parsed.arms.iter().enumerate() {
        let bound = &binding.manifests[index];
        let manifest = bound.manifest;
        let mut usage = Usage::default();
        for (attempt_index, attempt) in parsed_arm.attempts.iter().enumerate() {
            verify_attempt(attempt, attempt_index, manifest, binding)?;
            let observed = attempt
                .terminal
                .usage
                .as_ref()
                .context("completed terminal usage is missing")?;
            validate_usage(observed)?;
            add_usage(&mut usage, observed)?;
        }
        if u64::try_from(usage.total_tokens)? > binding.max_total_tokens_per_run {
            bail!("run token cap exceeded");
        }
        if manifest.usage_scope != "completeNativeThreadTree" {
            bail!("run manifest usage scope is invalid");
        }
        if manifest.provider_completed_response_count != parsed_arm.attempt_count
            || manifest.raw_response_count != parsed_arm.attempt_count
            || manifest.usage != usage
        {
            bail!("run manifest response counts or usage are invalid");
        }
        arms.push(VerifiedLedgerArm {
            condition: parsed_arm.condition,
            global_start_inclusive: parsed_arm.global_start_inclusive,
            global_end_exclusive: parsed_arm.global_end_exclusive,
            provider_request_attempt_count: parsed_arm.attempt_count,
            provider_completed_response_count: parsed_arm.attempt_count,
            raw_response_count: parsed_arm.attempt_count,
            usage,
            run_manifest_sha256: bound.raw_sha256.to_string(),
        });
    }
    Ok(VerifiedAttemptLedger {
        attempt_index_sha256: parsed.attempt_index_sha256,
        attempt_index_root_sha256: parsed.attempt_index_root_sha256,
        model_label: first_manifest.model_label.clone(),
        actual_model_revision: first_manifest.actual_model_revision.clone(),
        deployment_or_fingerprint_commitment: first_manifest
            .deployment_or_fingerprint_commitment
            .clone(),
        arms: arms
            .try_into()
            .map_err(|_| anyhow::anyhow!("missing arm"))?,
    })
}

fn validate_semantic_binding(binding: &NativeLedgerBinding<'_>) -> Result<()> {
    let max_total_tokens = i64::try_from(binding.max_total_tokens_per_run)?;
    for bound in &binding.manifests {
        validate_raw_manifest(bound)?;
        let manifest = bound.manifest;
        if manifest.pair_id != binding.pair_id
            || manifest.frozen_run_context_sha256 != binding.frozen_run_context_sha256
            || manifest.execution_context_sha256 != binding.execution_context_sha256
            || manifest.max_total_tokens != max_total_tokens
            || !manifest.tree_closed
        {
            bail!("run manifest semantic binding is invalid");
        }
    }
    Ok(())
}

fn validate_raw_manifest(bound: &BoundRunManifest<'_>) -> Result<()> {
    if !is_lower_sha256(bound.raw_sha256) {
        bail!("raw manifest SHA must be lowercase SHA-256 hex");
    }
    if super::sha256(bound.raw_bytes) != bound.raw_sha256 {
        bail!("raw manifest SHA does not match bytes");
    }
    let value = crate::jcs::parse_json(bound.raw_bytes).context("parse raw run manifest")?;
    let parsed: RunManifest = serde_json::from_value(value).context("decode raw run manifest")?;
    if &parsed != bound.manifest {
        bail!("raw manifest does not match typed manifest");
    }
    Ok(())
}

fn verify_attempt(
    attempt: &ParsedAttempt,
    attempt_index: usize,
    manifest: &RunManifest,
    binding: &NativeLedgerBinding<'_>,
) -> Result<()> {
    let request = &attempt.request;
    let terminal = &attempt.terminal;
    if terminal.request_record_sha256 != attempt.request_raw_sha256 {
        bail!("terminal request record SHA is invalid");
    }
    if request.pair_id != binding.pair_id
        || request.frozen_run_context_sha256 != binding.frozen_run_context_sha256
        || request.execution_context_sha256 != binding.execution_context_sha256
    {
        bail!("attempt request identity is invalid");
    }
    if request.deadline != binding.deadline {
        bail!("attempt request deadline is invalid");
    }
    if request.max_output_tokens != binding.max_output_tokens {
        bail!("attempt request output limit is invalid");
    }
    if attempt_index == 0 {
        if request.request_commitment != manifest.first_root_provider_request_commitment
            || request.normalized_request_commitment
                != manifest.normalized_first_root_request_commitment
            || request.normalized_base_commitment != manifest.normalized_first_root_base_commitment
            || request.treatment_diff_commitment != manifest.first_root_treatment_diff_commitment
        {
            bail!("first request commitments are invalid");
        }
        if request.thread_commitment != super::sha256(manifest.root_thread_id.as_bytes()) {
            bail!("first request thread commitment is invalid");
        }
        if request.parent_thread_commitment.is_some() {
            bail!("first request parent thread must be absent");
        }
    }
    if terminal.status != "completed"
        || terminal.response_id_commitment.is_none()
        || terminal.failure_class.is_some()
    {
        bail!("attempt does not have a completed terminal");
    }
    if terminal.actual_model_revision.as_deref() != Some(&manifest.actual_model_revision) {
        bail!("completed terminal model is invalid");
    }
    if terminal.deployment_commitment.as_ref()
        != manifest.deployment_or_fingerprint_commitment.as_ref()
    {
        bail!("completed terminal deployment is invalid");
    }
    Ok(())
}

fn validate_usage(usage: &Usage) -> Result<()> {
    let values = [
        usage.total_tokens,
        usage.input_tokens,
        usage.cached_input_tokens,
        usage.cache_write_input_tokens,
        usage.output_tokens,
        usage.reasoning_output_tokens,
    ];
    let total = usage
        .input_tokens
        .checked_add(usage.output_tokens)
        .context("usage arithmetic overflow")?;
    let cached = usage
        .cached_input_tokens
        .checked_add(usage.cache_write_input_tokens)
        .context("usage arithmetic overflow")?;
    if values.iter().any(|value| *value < 0)
        || usage.total_tokens != total
        || cached > usage.input_tokens
        || usage.reasoning_output_tokens > usage.output_tokens
    {
        bail!("usage arithmetic is invalid");
    }
    Ok(())
}

fn add_usage(total: &mut Usage, observed: &Usage) -> Result<()> {
    macro_rules! add_field {
        ($field:ident) => {
            total.$field = total
                .$field
                .checked_add(observed.$field)
                .context("usage aggregate overflow")?;
        };
    }
    add_field!(total_tokens);
    add_field!(input_tokens);
    add_field!(cached_input_tokens);
    add_field!(cache_write_input_tokens);
    add_field!(output_tokens);
    add_field!(reasoning_output_tokens);
    Ok(())
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}
