use std::path::Path;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::ModeEvidence;
use crate::RunManifest;
use crate::Usage;

const ATTEMPT_SCHEMA: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/attempt-index.schema.json");
const LEDGER: &str = "coordinator/attempt-index.jsonl";
const MIB: u64 = 1024 * 1024;

pub(crate) struct BoundRunManifest<'a> {
    pub(crate) manifest: &'a RunManifest,
    pub(crate) raw_sha256: &'a str,
}

pub(crate) struct NativeLedgerBinding<'a> {
    pub(crate) pair_id: &'a str,
    pub(crate) frozen_run_context_sha256: &'a str,
    pub(crate) execution_context_sha256: &'a str,
    pub(crate) arm_order_commitment: &'a str,
    pub(crate) deadline: &'a str,
    pub(crate) max_output_tokens: u64,
    pub(crate) max_attempts_per_arm: u64,
    pub(crate) max_total_tokens: u64,
    pub(crate) manifests: [BoundRunManifest<'a>; 2],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ParsedLedgerArm {
    pub(crate) condition: EvaluationCondition,
    pub(crate) global_start_inclusive: u64,
    pub(crate) global_end_exclusive: u64,
    pub(crate) attempt_count: u64,
    pub(crate) attempts: Vec<ParsedAttempt>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ParsedAttemptLedger {
    pub(crate) attempt_index_sha256: String,
    pub(crate) attempt_index_root_sha256: String,
    pub(crate) arms: [ParsedLedgerArm; 2],
}

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
    pub(crate) arms: [VerifiedLedgerArm; 2],
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct RequestRecord {
    schema_version: u32,
    record_type: String,
    status: String,
    pair_id: String,
    frozen_run_context_sha256: String,
    execution_context_sha256: String,
    global_attempt_index: u64,
    run_ordinal: u8,
    condition: EvaluationCondition,
    arm_attempt_index: u64,
    request_started_at: String,
    request_commitment: String,
    normalized_request_commitment: String,
    normalized_base_commitment: String,
    treatment_diff_commitment: Option<String>,
    thread_commitment: String,
    window_commitment: String,
    parent_thread_commitment: Option<String>,
    deadline: String,
    max_output_tokens: u64,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct TerminalRecord {
    schema_version: u32,
    record_type: String,
    global_attempt_index: u64,
    request_record_sha256: String,
    ended_at: String,
    status: String,
    response_id_commitment: Option<String>,
    actual_model_revision: Option<String>,
    deployment_commitment: Option<String>,
    usage: Option<Usage>,
    failure_class: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ParsedAttempt {
    pub(crate) request_raw_sha256: String,
    pub(crate) request: RequestRecord,
    pub(crate) terminal: TerminalRecord,
}

pub(crate) fn parse_native_attempt_ledger(
    private_root: &Path,
    binding: &NativeLedgerBinding<'_>,
) -> Result<ParsedAttemptLedger> {
    validate_binding(binding)?;
    let ledger_cap = binding
        .max_attempts_per_arm
        .checked_mul(4)
        .and_then(|count| count.checked_mul(MIB + 1))
        .context("attempt ledger byte cap overflow")?;
    let ledger = read_bounded(private_root, Path::new(LEDGER), ledger_cap)?;
    derive_arms(&ledger, binding)
}

pub(crate) fn derive_native_attempt_ledger(
    private_root: &Path,
    binding: &NativeLedgerBinding<'_>,
) -> Result<VerifiedAttemptLedger> {
    let parsed = parse_native_attempt_ledger(private_root, binding)?;
    validate_semantic_binding(binding)?;
    let mut pair_total_tokens = 0_i64;
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
        pair_total_tokens = pair_total_tokens
            .checked_add(usage.total_tokens)
            .context("pair token total overflow")?;
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
    if u64::try_from(pair_total_tokens)? > binding.max_total_tokens {
        bail!("pair token cap exceeded");
    }
    Ok(VerifiedAttemptLedger {
        attempt_index_sha256: parsed.attempt_index_sha256,
        attempt_index_root_sha256: parsed.attempt_index_root_sha256,
        arms: arms
            .try_into()
            .map_err(|_| anyhow::anyhow!("missing arm"))?,
    })
}

fn validate_semantic_binding(binding: &NativeLedgerBinding<'_>) -> Result<()> {
    let max_total_tokens = i64::try_from(binding.max_total_tokens)?;
    for bound in &binding.manifests {
        let manifest = bound.manifest;
        if manifest.pair_id != binding.pair_id
            || manifest.frozen_run_context_sha256 != binding.frozen_run_context_sha256
            || manifest.execution_context_sha256 != binding.execution_context_sha256
            || manifest.max_total_tokens != max_total_tokens
            || !manifest.tree_closed
        {
            bail!("run manifest semantic binding is invalid");
        }
        if !is_lower_sha256(bound.raw_sha256) {
            bail!("raw manifest SHA must be lowercase SHA-256 hex");
        }
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
    if attempt_index == 0
        && (request.request_commitment != manifest.first_root_provider_request_commitment
            || request.normalized_request_commitment
                != manifest.normalized_first_root_request_commitment
            || request.normalized_base_commitment != manifest.normalized_first_root_base_commitment
            || request.treatment_diff_commitment != manifest.first_root_treatment_diff_commitment)
    {
        bail!("first request commitments are invalid");
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

fn validate_binding(binding: &NativeLedgerBinding<'_>) -> Result<()> {
    if binding.max_attempts_per_arm == 0 {
        bail!("native attempt ledger cap must be positive");
    }
    for (index, bound) in binding.manifests.iter().enumerate() {
        let manifest = bound.manifest;
        manifest.validate_execution_mode()?;
        let expected_ordinal = u8::try_from(index + 1)?;
        if !matches!(
            manifest.execution_mode,
            ExecutionMode::Mock | ExecutionMode::Live
        ) || manifest.schema_version != 1
            || manifest.run_ordinal != expected_ordinal
            || manifest.max_provider_request_attempts != binding.max_attempts_per_arm
            || manifest.provider_request_attempt_count == 0
            || manifest.provider_request_attempt_count > binding.max_attempts_per_arm
            || mode_order(manifest)? != binding.arm_order_commitment
        {
            bail!("run manifest binding is invalid");
        }
    }
    if binding.manifests[0].manifest.condition == binding.manifests[1].manifest.condition {
        bail!("native proof ledger arms must have opposite conditions");
    }
    Ok(())
}

fn derive_arms(ledger: &[u8], binding: &NativeLedgerBinding<'_>) -> Result<ParsedAttemptLedger> {
    if ledger.is_empty() || ledger.last() != Some(&b'\n') || ledger.contains(&b'\r') {
        bail!("attempt ledger must be nonempty LF-framed JSONL");
    }
    let maximum_records = usize::try_from(binding.max_attempts_per_arm)?
        .checked_mul(4)
        .context("attempt ledger record cap overflow")?;
    let observed_records = maximum_records
        .checked_add(1)
        .context("attempt ledger record probe overflow")?;
    let body = ledger
        .strip_suffix(b"\n")
        .context("attempt ledger framing is invalid")?;
    let lines = body
        .split(|byte| *byte == b'\n')
        .take(observed_records)
        .collect::<Vec<_>>();
    if lines.len() > maximum_records || lines.len() % 2 != 0 {
        bail!("attempt ledger record count is invalid");
    }
    let validator = crate::contracts::compile_schema(ATTEMPT_SCHEMA)?;
    let mut root = [0_u8; 32];
    let mut byte_end = 0_usize;
    let mut next_global = 0_u64;
    let mut derived = Vec::with_capacity(2);
    for ordinal in 1..=2_u8 {
        let manifest = binding.manifests[usize::from(ordinal - 1)].manifest;
        let start = next_global;
        let mut count = 0_u64;
        let mut attempts = Vec::new();
        while let Some(request_line) = lines.get(pair_offset(next_global)?) {
            let request: RequestRecord = parse_record(request_line, "request", &validator)?;
            if request.run_ordinal != ordinal {
                break;
            }
            let terminal_line = lines
                .get(
                    pair_offset(next_global)?
                        .checked_add(1)
                        .context("terminal record offset overflow")?,
                )
                .context("request record has no terminal record")?;
            let terminal: TerminalRecord = parse_record(terminal_line, "terminal", &validator)?;
            if request.global_attempt_index != next_global
                || request.arm_attempt_index != count
                || request.condition != manifest.condition
                || terminal.global_attempt_index != next_global
            {
                bail!("attempt ledger record identity or sequence is invalid");
            }
            attempts.push(ParsedAttempt {
                request_raw_sha256: sha256(request_line),
                request,
                terminal,
            });
            for line in [*request_line, *terminal_line] {
                root = fold(root, line);
                byte_end = byte_end
                    .checked_add(line.len() + 1)
                    .context("attempt ledger prefix overflow")?;
            }
            next_global = next_global
                .checked_add(1)
                .context("attempt index overflow")?;
            count = count.checked_add(1).context("arm attempt count overflow")?;
        }
        if count == 0 {
            bail!("attempt ledger is missing an arm");
        }
        let end = start.checked_add(count).context("attempt range overflow")?;
        let prefix_sha256 = sha256(&ledger[..byte_end]);
        let prefix_root = hex(root);
        if manifest.provider_request_attempt_count != count
            || manifest.broker_attempt_ledger_sha256 != prefix_sha256
            || manifest.attempt_index_root_sha256 != prefix_root
        {
            bail!("run manifest attempt ledger summary is invalid");
        }
        derived.push(ParsedLedgerArm {
            condition: manifest.condition,
            global_start_inclusive: start,
            global_end_exclusive: end,
            attempt_count: count,
            attempts,
        });
    }
    if pair_offset(next_global)? != lines.len() {
        bail!("attempt ledger contains records after the second arm");
    }
    Ok(ParsedAttemptLedger {
        attempt_index_sha256: sha256(ledger),
        attempt_index_root_sha256: hex(root),
        arms: derived
            .try_into()
            .map_err(|_| anyhow::anyhow!("missing arm"))?,
    })
}

fn pair_offset(index: u64) -> Result<usize> {
    usize::try_from(index)?
        .checked_mul(2)
        .context("attempt record offset overflow")
}

fn parse_record<T: serde::de::DeserializeOwned + Serialize>(
    raw: &[u8],
    expected_type: &str,
    validator: &jsonschema::Validator,
) -> Result<T> {
    if raw.len() > usize::try_from(MIB)? {
        bail!("attempt ledger record exceeds its byte cap");
    }
    let value = crate::jcs::parse_json(raw)?;
    validator
        .validate(&value)
        .map_err(|error| anyhow::anyhow!("attempt schema validation failed: {error}"))?;
    if value.get("recordType").and_then(serde_json::Value::as_str) != Some(expected_type) {
        bail!("attempt ledger records do not alternate request and terminal");
    }
    let typed: T = serde_json::from_value(value)?;
    if serde_json::to_vec(&typed)? != raw {
        bail!("attempt ledger record is not exact compact typed JSON");
    }
    Ok(typed)
}

fn read_bounded(root: &Path, relative: &Path, cap: u64) -> Result<Vec<u8>> {
    let path = crate::secure_fs::resolve_private_relative(root, relative)?;
    crate::secure_fs::read_single_link_regular_bounded(&path, cap)
}

fn mode_order(manifest: &RunManifest) -> Result<&str> {
    match &manifest.mode_evidence {
        ModeEvidence::Mock {
            arm_order_commitment,
            ..
        }
        | ModeEvidence::Live {
            arm_order_commitment,
            ..
        } => Ok(arm_order_commitment),
        ModeEvidence::Replay { .. } => bail!("replay manifest has no native proof ledger"),
    }
}

fn fold(root: [u8; 32], line: &[u8]) -> [u8; 32] {
    let leaf = Sha256::digest(line);
    let mut hasher = Sha256::new();
    hasher.update(root);
    hasher.update(leaf);
    hasher.finalize().into()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
