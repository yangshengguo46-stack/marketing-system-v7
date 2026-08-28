use crate::CatalogRoots;
use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::RunManifest;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_app_server_protocol::ApprovalsReviewer;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStatus;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use std::collections::HashSet;
use std::path::Path;

pub(crate) use crate::model::ArmPostprocessCapture;
pub(crate) use crate::model::BrokerCompletionSidecar;
pub(crate) use crate::model::BrokerSnapshotSidecar;
pub(crate) use crate::model::CatalogRootsSidecar;
pub(crate) use crate::model::CatalogSidecar;
pub(crate) use crate::model::ConfigSidecar;
pub(crate) use crate::model::QuietTreeSidecar;
pub(crate) use crate::model::StartSidecar;
pub(crate) use crate::model::TreeScanSidecar;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveEvidenceSource {
    ReplaySynthetic,
    NativeRecorded,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PostprocessSidecarKind {
    Notifications,
    Start,
    Config,
    PreCatalog,
    PostCatalog,
    QuietTree,
    BrokerSnapshot,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PostprocessSidecarEntry {
    pub kind: PostprocessSidecarKind,
    pub relative_path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArmPostprocessIndex {
    pub schema_version: u32,
    pub pair_id: String,
    pub run_ordinal: u8,
    pub condition: EvaluationCondition,
    pub evidence_source: ArchiveEvidenceSource,
    pub sidecars: Vec<PostprocessSidecarEntry>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedPostprocessSummary {
    pub(crate) index: ArmPostprocessIndex,
    pub(crate) content_package: codex_ai_ip_domain::ContentPackage,
    pub(crate) content_package_bytes: Vec<u8>,
    pub(crate) usage: crate::Usage,
    pub(crate) raw_response_count: u64,
    pub(crate) tree_closed: bool,
    pub(crate) skill_use: crate::SkillUseOutcome,
    pub(crate) normalized_base_catalog_sha256: String,
    pub(crate) broker_global_attempt_start_inclusive: u64,
    pub(crate) broker_global_attempt_end_exclusive: u64,
}

impl CatalogRootsSidecar {
    fn as_roots(&self) -> CatalogRoots {
        CatalogRoots {
            codex_home: self.codex_home.clone(),
            host_home: self.host_home.clone(),
            case_dir: self.case_dir.clone(),
        }
    }
}

pub(crate) const SIDECARS: [(PostprocessSidecarKind, &str); 7] = [
    (PostprocessSidecarKind::Notifications, "notifications.jsonl"),
    (PostprocessSidecarKind::Start, "start.json"),
    (PostprocessSidecarKind::Config, "config.json"),
    (PostprocessSidecarKind::PreCatalog, "pre-catalog.json"),
    (PostprocessSidecarKind::PostCatalog, "post-catalog.json"),
    (PostprocessSidecarKind::QuietTree, "quiet-tree.json"),
    (
        PostprocessSidecarKind::BrokerSnapshot,
        "broker-snapshot.json",
    ),
];

pub fn verify_postprocess_archive(
    private_root: &Path,
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
) -> Result<ArmPostprocessIndex> {
    Ok(
        verify_postprocess_archive_summary(private_root, manifest, mission_case, skill_bytes)?
            .index,
    )
}

pub(crate) fn verify_postprocess_archive_summary(
    private_root: &Path,
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
) -> Result<VerifiedPostprocessSummary> {
    let expected_attempt_start =
        expected_attempt_start(private_root, manifest, mission_case, skill_bytes)?;
    verify_postprocess_archive_summary_at(
        private_root,
        manifest,
        mission_case,
        skill_bytes,
        expected_attempt_start,
    )
}

pub(crate) fn verify_sequential_postprocess_archive_summary(
    private_root: &Path,
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
    prior: &VerifiedPostprocessSummary,
) -> Result<VerifiedPostprocessSummary> {
    manifest.validate_execution_mode()?;
    let expected_source = evidence_source(manifest.execution_mode);
    if manifest.run_ordinal != 2
        || prior.index.pair_id != manifest.pair_id
        || prior.index.run_ordinal != 1
        || prior.index.condition == manifest.condition
        || prior.index.evidence_source != expected_source
        || prior.broker_global_attempt_start_inclusive != 0
        || manifest.execution_mode == ExecutionMode::Replay
            && prior.broker_global_attempt_end_exclusive != 0
    {
        bail!("prior summary identity does not authorize the sequential attempt range");
    }
    let expected_attempt_start = match manifest.execution_mode {
        ExecutionMode::Replay => 0,
        ExecutionMode::Mock | ExecutionMode::Live => prior.broker_global_attempt_end_exclusive,
    };
    verify_postprocess_archive_summary_at(
        private_root,
        manifest,
        mission_case,
        skill_bytes,
        expected_attempt_start,
    )
}

fn verify_postprocess_archive_summary_at(
    private_root: &Path,
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
    expected_attempt_start: u64,
) -> Result<VerifiedPostprocessSummary> {
    manifest.validate_execution_mode()?;
    let coordinator = match manifest.execution_mode {
        ExecutionMode::Replay => Path::new("replay-coordinator"),
        ExecutionMode::Mock | ExecutionMode::Live => Path::new("coordinator"),
    };
    let index_relative = coordinator.join(format!(
        "run-{}-postprocess-index.json",
        manifest.run_ordinal
    ));
    let index_bytes = read_sidecar(private_root, &index_relative, 1024 * 1024)?;
    if sha256(&index_bytes) != manifest.postprocess_evidence_index_sha256 {
        bail!("manifest does not bind the raw postprocess index");
    }
    let value = crate::jcs::parse_json(&index_bytes)?;
    let index: ArmPostprocessIndex = serde_json::from_value(value)?;
    if index_bytes != crate::jcs::canonicalize_value(&serde_json::to_value(&index)?)? {
        bail!("postprocess index is not exact canonical typed JSON");
    }
    let expected_source = evidence_source(manifest.execution_mode);
    if index.schema_version != 1
        || index.pair_id != manifest.pair_id
        || index.run_ordinal != manifest.run_ordinal
        || index.condition != manifest.condition
        || index.evidence_source != expected_source
        || index.sidecars.len() != SIDECARS.len()
    {
        bail!("postprocess index identity is invalid");
    }
    let mut bytes = Vec::with_capacity(SIDECARS.len());
    for (entry, (kind, suffix)) in index.sidecars.iter().zip(SIDECARS) {
        let relative = coordinator.join(format!("run-{}-{suffix}", manifest.run_ordinal));
        if entry.kind != kind || entry.relative_path != relative_to_utf8(&relative)? {
            bail!("postprocess sidecar order or path is invalid");
        }
        let cap = if kind == PostprocessSidecarKind::Notifications {
            u64::try_from(crate::evidence::MAX_ARCHIVE_NOTIFICATION_BYTES)?
        } else {
            1024 * 1024
        };
        let current = read_sidecar(private_root, &relative, cap)?;
        if entry.sha256 != sha256(&current) {
            bail!("postprocess sidecar digest changed");
        }
        bytes.push(current);
    }
    verify_semantics(
        manifest,
        mission_case,
        skill_bytes,
        &bytes,
        index,
        expected_attempt_start,
    )
}

fn verify_semantics(
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
    bytes: &[Vec<u8>],
    index: ArmPostprocessIndex,
    expected_attempt_start: u64,
) -> Result<VerifiedPostprocessSummary> {
    let notifications = crate::evidence::parse_notification_archive(&bytes[0])?;
    if sha256(&bytes[0]) != manifest.app_server_transcript_sha256 {
        bail!("notification transcript differs from the manifest");
    }
    let start: StartSidecar = parse_json(&bytes[1])?;
    let config: ConfigSidecar = parse_json(&bytes[2])?;
    let pre: CatalogSidecar = parse_json(&bytes[3])?;
    let post: CatalogSidecar = parse_json(&bytes[4])?;
    let quiet: QuietTreeSidecar = parse_json(&bytes[5])?;
    let broker: BrokerSnapshotSidecar = parse_json(&bytes[6])?;
    if start.schema_version != 1
        || start.thread.thread.id != manifest.root_thread_id
        || start.thread.thread.session_id != manifest.session_id
        || start.thread.thread.model_provider != start.thread.model_provider
        || start.thread.thread.ephemeral
        || start.thread.model != manifest.model_label
        || start.thread.model_provider
            != if manifest.execution_mode == ExecutionMode::Replay {
                "replay-not-run"
            } else {
                "ai-ip-proof-broker"
            }
        || start.thread.service_tier.is_some()
        || start.thread.approval_policy != AskForApproval::Never
        || start.thread.approvals_reviewer != ApprovalsReviewer::User
        || start
            .thread
            .active_permission_profile
            .as_ref()
            .map(|profile| profile.id.as_str())
            != Some(crate::app_server::EVALUATION_PERMISSION_PROFILE)
        || !start.thread.runtime_workspace_roots.is_empty()
        || !start.thread.instruction_sources.is_empty()
        || start.thread.reasoning_effort.is_some()
        || serde_json::to_value(&start.thread.sandbox)?
            != serde_json::json!({"type": "dangerFullAccess"})
        || serde_json::to_value(&start.thread.multi_agent_mode)?
            != serde_json::json!("explicitRequestOnly")
        || start.turn.turn.id != manifest.root_turn_id
        || start.turn.turn.status != TurnStatus::InProgress
        || start.turn.turn.items_view != TurnItemsView::Full
        || start.turn.turn.error.is_some()
        || !start.turn.turn.items.is_empty()
        || start.thread.cwd.as_path() != pre.roots.case_dir
        || start.thread.thread.cwd.as_path() != pre.roots.case_dir
        || quiet.root.cwd.as_path() != pre.roots.case_dir
        || quiet.root.id != manifest.root_thread_id
        || quiet.root.session_id != manifest.session_id
    {
        bail!("start responses differ from the manifest or catalog roots");
    }
    if config.canonical_config_path != pre.roots.codex_home.join("config.toml") {
        bail!("config evidence path differs from the catalog CODEX_HOME");
    }
    verify_config(manifest, config)?;
    let pre_snapshot = crate::normalize_catalog(&pre.response, &pre.roots.as_roots())?;
    let post_snapshot = crate::normalize_catalog(&post.response, &post.roots.as_roots())?;
    crate::validate_stable_catalog(&pre_snapshot, &post_snapshot)?;
    let normalized_base_catalog_sha256 = pre_snapshot.normalized_base_catalog_sha256();
    if pre.schema_version != 1
        || post.schema_version != 1
        || pre_snapshot.sha256 != manifest.pre_skill_catalog_sha256
        || post_snapshot.sha256 != manifest.post_skill_catalog_sha256
        || normalized_base_catalog_sha256 != manifest.normalized_base_catalog_sha256
        || pre.roots.codex_home != post.roots.codex_home
        || pre.roots.host_home != post.roots.host_home
        || pre.roots.case_dir != post.roots.case_dir
    {
        bail!("raw catalog evidence differs from the manifest");
    }
    let first = crate::TreeScan::from_typed_pages(
        &quiet.root,
        &quiet.first.ancestor_pages,
        &quiet.first.loaded_pages,
        &quiet.first.loaded_reads,
    )?;
    let second = crate::TreeScan::from_typed_pages(
        &quiet.root,
        &quiet.second.ancestor_pages,
        &quiet.second.loaded_pages,
        &quiet.second.loaded_reads,
    )?;
    if quiet.schema_version != 1 || first != second || first.thread_ids().is_empty() {
        bail!("quiet tree evidence is invalid");
    }
    let mut replay = crate::ReplayCollector::new(
        manifest.root_thread_id.clone(),
        manifest.root_turn_id.clone(),
        HashSet::from([manifest.root_thread_id.clone()]),
    );
    let mut tree = crate::TreeEventCollector::new(&quiet.root)?;
    let skill_path = pre
        .roots
        .codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
        .join("SKILL.md");
    let mut skill = match manifest.condition {
        EvaluationCondition::Generic => {
            crate::SkillUseTracker::new_forbidden(&skill_path, skill_bytes)?
        }
        EvaluationCondition::Candidate => {
            crate::SkillUseTracker::new_required(&skill_path, skill_bytes)?
        }
    };
    for notification in notifications {
        replay.ingest(notification.clone())?;
        tree.ingest(notification.clone())?;
        skill.ingest(&notification)?;
    }
    let completions = broker
        .completions
        .iter()
        .map(broker_completion)
        .collect::<Vec<_>>();
    crate::evidence::verify_broker_completion_metadata(
        &completions,
        &manifest.actual_model_revision,
        manifest.deployment_or_fingerprint_commitment.as_deref(),
    )?;
    let tree = tree.close(&first, &second, &completions, broker.in_flight)?;
    let replay = replay.finish(mission_case)?;
    let skill = skill.finish()?;
    let content_package_bytes = serde_json::to_vec(&replay.content_package)?;
    let provider_completion_count = if manifest.execution_mode == ExecutionMode::Replay {
        0
    } else {
        u64::try_from(completions.len())?
    };
    let expected_attempt_end = expected_attempt_start
        .checked_add(manifest.provider_request_attempt_count)
        .context("postprocess attempt range overflow")?;
    if broker.schema_version != 1
        || broker.pair_id != manifest.pair_id
        || broker.run_ordinal != manifest.run_ordinal
        || broker.condition != manifest.condition
        || broker.in_flight != 0
        || broker.attempt_index_file_sha256 != manifest.broker_attempt_ledger_sha256
        || broker.attempt_index_merkle_root != manifest.attempt_index_root_sha256
        || broker.global_attempt_start_inclusive != expected_attempt_start
        || broker.global_attempt_end_exclusive != expected_attempt_end
        || provider_completion_count != manifest.provider_completed_response_count
        || replay.usage != manifest.usage
        || replay.raw_response_count != manifest.raw_response_count
        || sha256(&content_package_bytes) != manifest.content_package_sha256
        || tree.usage != manifest.usage
        || tree.raw_response_count != manifest.raw_response_count
        || tree.tree_closed != manifest.tree_closed
        || skill.evidence_sha256 != manifest.skill_use_evidence_sha256
        || skill.successful_read_observed != manifest.skill_use_evidence_sha256.is_some()
    {
        bail!("postprocess archive semantics differ from the manifest");
    }
    Ok(VerifiedPostprocessSummary {
        index,
        content_package: replay.content_package,
        content_package_bytes,
        usage: replay.usage,
        raw_response_count: replay.raw_response_count,
        tree_closed: tree.tree_closed,
        skill_use: skill,
        normalized_base_catalog_sha256,
        broker_global_attempt_start_inclusive: broker.global_attempt_start_inclusive,
        broker_global_attempt_end_exclusive: broker.global_attempt_end_exclusive,
    })
}

fn evidence_source(mode: ExecutionMode) -> ArchiveEvidenceSource {
    match mode {
        ExecutionMode::Replay => ArchiveEvidenceSource::ReplaySynthetic,
        ExecutionMode::Mock | ExecutionMode::Live => ArchiveEvidenceSource::NativeRecorded,
    }
}

fn expected_attempt_start(
    private_root: &Path,
    manifest: &RunManifest,
    mission_case: &codex_ai_ip_domain::HeldOutMissionCase,
    skill_bytes: &[u8],
) -> Result<u64> {
    match (manifest.run_ordinal, manifest.execution_mode) {
        (1, _) | (2, ExecutionMode::Replay) => return Ok(0),
        (2, ExecutionMode::Mock | ExecutionMode::Live) => {}
        _ => bail!("postprocess archive has an invalid run ordinal"),
    }
    let prior_relative = Path::new("coordinator/run-1-manifest.json");
    let prior: RunManifest = parse_json(&read_sidecar(private_root, prior_relative, 1024 * 1024)?)?;
    if prior.pair_id != manifest.pair_id
        || prior.run_ordinal != 1
        || prior.execution_mode != manifest.execution_mode
        || prior.condition == manifest.condition
    {
        bail!("prior manifest identity does not authorize the attempt range");
    }
    verify_postprocess_archive(private_root, &prior, mission_case, skill_bytes)?;
    Ok(prior.provider_request_attempt_count)
}

fn verify_config(manifest: &RunManifest, config: ConfigSidecar) -> Result<()> {
    if config.schema_version != 1
        || sha256(config.expected_config_utf8.as_bytes()) != manifest.shared_config_sha256
    {
        bail!("config evidence bytes differ from the manifest");
    }
    let evidence = crate::audit_frozen_config(
        &config.response,
        &config.requirements,
        config.canonical_config_path,
        config.expected_config_utf8.into_bytes(),
        config.expected_layer_config,
    )?;
    if evidence.effective_config_sha256 != manifest.effective_config_sha256
        || evidence.config_layers_sha256 != manifest.config_layers_sha256
    {
        bail!("native config audit differs from the manifest");
    }
    Ok(())
}

fn parse_json<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    Ok(serde_json::from_value(crate::jcs::parse_json(bytes)?)?)
}

fn broker_completion(
    completion: &BrokerCompletionSidecar,
) -> codex_responses_api_proxy::ResponseCompletedMetadata {
    codex_responses_api_proxy::ResponseCompletedMetadata {
        response_id: completion.response_id.clone(),
        usage: completion
            .usage
            .as_ref()
            .map(|usage| codex_responses_api_proxy::ObservedUsage {
                total_tokens: usage.total_tokens,
                input_tokens: usage.input_tokens,
                cached_input_tokens: usage.cached_input_tokens,
                cache_write_input_tokens: usage.cache_write_input_tokens,
                output_tokens: usage.output_tokens,
                reasoning_output_tokens: usage.reasoning_output_tokens,
            }),
        actual_model: completion.actual_model.clone(),
        deployment_or_fingerprint: completion.deployment_or_fingerprint.clone(),
    }
}

fn read_sidecar(root: &Path, relative: &Path, cap: u64) -> Result<Vec<u8>> {
    let path = crate::secure_fs::resolve_private_relative(root, relative)?;
    crate::secure_fs::read_single_link_regular_bounded(&path, cap)
}

pub(crate) fn relative_to_utf8(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .context("postprocess relative path is not UTF-8")?
        .replace('\\', "/"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
