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
    let body = ledger
        .strip_suffix(b"\n")
        .context("attempt ledger framing is invalid")?;
    let lines = body.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    let maximum_records = usize::try_from(binding.max_attempts_per_arm)?
        .checked_mul(4)
        .context("attempt ledger record cap overflow")?;
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
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.len() > cap {
        bail!("private proof file exceeds its byte cap");
    }
    let bytes = crate::secure_fs::read_single_link_regular(&path)?;
    if u64::try_from(bytes.len())? > cap {
        bail!("private proof file exceeds its byte cap");
    }
    Ok(bytes)
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
