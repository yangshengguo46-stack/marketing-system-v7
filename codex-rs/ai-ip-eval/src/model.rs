use anyhow::bail;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvaluationCondition {
    Generic,
    Candidate,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Replay,
    Live,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize, ValueEnum)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRole {
    #[value(name = "targetVolcengine")]
    TargetVolcengine,
    #[value(name = "approvedReference")]
    ApprovedReference,
}

/// Typed evaluator command line. There is intentionally no single-arm live
/// command: the only provider-capable surface is `live-pair`.
#[derive(Debug, Parser)]
#[command(name = "codex-ai-ip-eval")]
pub struct Cli {
    #[command(subcommand)]
    pub command: EvalCommand,
}

#[derive(Debug, Subcommand)]
pub enum EvalCommand {
    ReplayPair(ReplayPairArgs),
    FreezeRunContext(FreezeRunContextCommand),
    LivePair(LivePairArgs),
    BlindPack(PathInputArgs),
    Score(PathInputArgs),
    MakeCostReceipt(PathInputArgs),
    AnnotateCost(PathInputArgs),
    Summarize(PathInputArgs),
    VerifyReport(PathInputArgs),
    PublishReport(PathInputArgs),
    VerifyLiveProof(PathInputArgs),
    FinalizeCheckpoint(PathInputArgs),
    RetentionCloseout(PathInputArgs),
}

#[derive(Debug, Args)]
pub struct ReplayPairArgs {
    #[arg(long)]
    pub frozen_run_context: PathBuf,
}

#[derive(Debug, Args)]
pub struct FreezeRunContextCommand {
    #[command(subcommand)]
    pub mode: FreezeRunContextArgs,
}

/// Replay and live freezing are disjoint at the parser and Rust type levels.
#[derive(Debug, Subcommand)]
pub enum FreezeRunContextArgs {
    Replay(ReplayFreezeArgs),
    Live(LiveFreezeArgs),
}

#[derive(Debug, Args)]
pub struct ReplayFreezeArgs {
    #[arg(long)]
    pub repo_root: PathBuf,
    #[arg(long)]
    pub fork_sha: String,
    #[arg(long)]
    pub private_root: PathBuf,
    #[arg(long)]
    pub codex_bin: PathBuf,
    #[arg(long)]
    pub case: PathBuf,
    #[arg(long)]
    pub transcript: PathBuf,
    #[arg(long)]
    pub fixture_set_manifest: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct LiveFreezeArgs {
    #[arg(long)]
    pub repo_root: PathBuf,
    #[arg(long)]
    pub evidence_repo_root: PathBuf,
    #[arg(long)]
    pub fork_sha: String,
    #[arg(long)]
    pub private_root: PathBuf,
    #[arg(long)]
    pub codex_bin: PathBuf,
    #[arg(long)]
    pub case: PathBuf,
    #[arg(long)]
    pub material_root: PathBuf,
    #[arg(long)]
    pub attestation: PathBuf,
    #[arg(long)]
    pub provider_budget_evidence: PathBuf,
    #[arg(long)]
    pub rate_card: PathBuf,
    #[arg(long)]
    pub billing_policy: PathBuf,
    #[arg(long)]
    pub fx_policy: PathBuf,
    #[arg(long)]
    pub lead_skill: PathBuf,
    #[arg(long)]
    pub model_label: String,
    #[arg(long)]
    pub provider_label: String,
    #[arg(long, value_enum)]
    pub provider_role: ProviderRole,
    #[arg(long)]
    pub provider_upstream_url: String,
    #[arg(long)]
    pub authorized_total_cost_fen: u64,
    #[arg(long)]
    pub authorized_per_run_cost_fen: u64,
    #[arg(long)]
    pub max_provider_request_attempts_per_run: u64,
    #[arg(long)]
    pub max_total_tokens_per_run: u64,
    #[arg(long)]
    pub max_elapsed_seconds_per_run: u64,
    #[arg(long)]
    pub max_output_tokens_per_request: u64,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Debug, Args)]
pub struct LivePairArgs {
    #[arg(long)]
    pub frozen_run_context: PathBuf,
}

#[derive(Debug, Args)]
pub struct PathInputArgs {
    #[arg(long)]
    pub input: PathBuf,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProofBrokerCompatibilityName {
    #[serde(rename = "OpenAI")]
    OpenAi,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    deny_unknown_fields,
    tag = "executionMode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ModeEvidence {
    Replay {
        fixture_set_sha256: String,
    },
    Live {
        attestation_sha256: String,
        provider_budget_evidence_sha256: String,
        approval_commitment: String,
        provider_endpoint_commitment: String,
        provider_role: ProviderRole,
        arm_order_commitment: String,
        rate_card_sha256: String,
        billing_policy_sha256: String,
        fx_policy_sha256: Option<String>,
        authorized_pair_cost_fen: u64,
        retention_deadline: String,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Usage {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RunManifest {
    pub schema_version: u32,
    pub pair_id: String,
    pub frozen_run_context_sha256: String,
    pub execution_context_sha256: String,
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub fork_sha: String,
    pub case_sha256: String,
    pub source_materials_sha256: String,
    pub prompt_sha256: String,
    pub additional_context_sha256: String,
    pub output_schema_sha256: String,
    pub thread_start_request_sha256: String,
    pub turn_start_request_sha256: String,
    pub shared_config_sha256: String,
    pub effective_config_sha256: String,
    pub config_layers_sha256: String,
    pub native_skill_sha256: Option<String>,
    pub pre_skill_catalog_sha256: String,
    pub post_skill_catalog_sha256: String,
    pub normalized_base_catalog_sha256: String,
    pub skill_use_evidence_sha256: Option<String>,
    pub codex_binary_sha256: String,
    pub evaluator_binary_sha256: String,
    pub broker_component_sha256: String,
    pub first_root_provider_request_commitment: String,
    pub normalized_first_root_request_commitment: String,
    pub normalized_first_root_base_commitment: String,
    pub first_root_treatment_diff_commitment: Option<String>,
    pub app_server_transcript_sha256: String,
    pub broker_attempt_ledger_sha256: String,
    pub attempt_index_root_sha256: String,
    pub content_package_sha256: String,
    pub root_thread_id: String,
    pub root_turn_id: String,
    pub session_id: String,
    pub provider_request_attempt_count: u64,
    pub provider_completed_response_count: u64,
    pub raw_response_count: u64,
    pub usage_scope: String,
    pub usage: Usage,
    pub model_label: String,
    pub actual_model_revision: String,
    pub deployment_or_fingerprint_commitment: Option<String>,
    pub provider_label: String,
    pub provider_compatibility_name: ProofBrokerCompatibilityName,
    pub authorized_evaluation_run_cost_fen: u64,
    pub max_provider_request_attempts: u64,
    pub max_total_tokens: i64,
    pub max_elapsed_seconds: u64,
    pub elapsed_ms: u128,
    pub tree_closed: bool,
    pub execution_mode: ExecutionMode,
    pub mode_evidence: ModeEvidence,
}

impl RunManifest {
    pub fn validate_execution_mode(&self) -> anyhow::Result<()> {
        let matches = matches!(
            (self.execution_mode, &self.mode_evidence),
            (ExecutionMode::Replay, ModeEvidence::Replay { .. })
                | (ExecutionMode::Live, ModeEvidence::Live { .. })
        );
        if !matches {
            bail!("executionMode does not match modeEvidence");
        }
        Ok(())
    }

    pub fn is_g2_eligible(&self) -> bool {
        self.execution_mode == ExecutionMode::Live
            && matches!(self.mode_evidence, ModeEvidence::Live { .. })
    }
}
