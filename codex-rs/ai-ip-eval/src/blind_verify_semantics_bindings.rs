use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::MockProviderMode;
use crate::ModeEvidence;
use crate::ProofBrokerCompatibilityName;
use crate::ReviewerDeclaration;
use crate::RunManifest;
use crate::blind::FrozenInputToken;

use super::ExecutionContext;
use super::NativeExecutionContext;
use super::PairRequestParity;
use super::PairVerification;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PairMaterialInput {
    pub(crate) material_id: String,
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

pub(super) struct ContentBinding {
    pub(super) mode: ExecutionMode,
    pub(super) private_root: PathBuf,
    pub(super) pair_id: String,
    pub(super) fork_sha: String,
    pub(super) frozen_sha: String,
    fixture_set_sha: Option<String>,
    attestation_sha: Option<String>,
    pub(super) reviewers: [ReviewerDeclaration; 3],
    pub(super) mission_case: codex_ai_ip_domain::HeldOutMissionCase,
    pub(super) case_bytes: Vec<u8>,
    pub(super) materials_manifest_bytes: Vec<u8>,
    pub(super) materials: Vec<PairMaterialInput>,
    prompt_bytes: Vec<u8>,
    additional_context_bytes: Vec<u8>,
    schema_bytes: Vec<u8>,
    thread_start_bytes: Vec<u8>,
    turn_start_bytes: Vec<u8>,
    pub(super) skill_bytes: Vec<u8>,
    codex_binary_sha: String,
    evaluator_binary_sha: String,
    broker_component_sha: String,
    model_label: String,
    provider_label: String,
    pub(super) max_output_tokens: u64,
    pub(super) max_attempts: u64,
    pub(super) max_total_tokens: u64,
    max_elapsed_seconds: u64,
    replay_request_parity: Option<PairRequestParity>,
}

pub(super) fn content_binding(inputs: &FrozenInputToken) -> Result<ContentBinding> {
    match inputs {
        FrozenInputToken::Replay(verified) => {
            let source = verified.projection();
            let homes = crate::IsolatedHomes {
                generic_home: source.private_root.join("replay-generic-home"),
                generic_codex_home: source.private_root.join("replay-generic-home/.codex"),
                candidate_home: source.private_root.join("replay-candidate-home"),
                candidate_codex_home: source.private_root.join("replay-candidate-home/.codex"),
            };
            let request_parity = replay_request_parity(verified, &homes)?;
            Ok(ContentBinding {
                mode: ExecutionMode::Replay,
                private_root: source.private_root.to_path_buf(),
                pair_id: source.pair_id.to_string(),
                fork_sha: source.fork_sha.to_string(),
                frozen_sha: source.raw_sha256.to_string(),
                fixture_set_sha: Some(source.fixture_set_sha256.to_string()),
                attestation_sha: None,
                reviewers: exact_reviewers(&source.attestation.reviewers)?,
                mission_case: source.mission_case.clone(),
                case_bytes: verified.fixture_bytes("case")?.to_vec(),
                materials_manifest_bytes: source.materials_manifest_bytes.to_vec(),
                materials: source
                    .materials
                    .into_iter()
                    .map(|item| PairMaterialInput {
                        material_id: item.material_id,
                        relative_path: item.relative_path,
                        sha256: item.sha256,
                        bytes: item.bytes,
                    })
                    .collect(),
                prompt_bytes: source.prompt_bytes.to_vec(),
                additional_context_bytes: source.additional_context_bytes.to_vec(),
                schema_bytes: source.schema_bytes.to_vec(),
                thread_start_bytes: source.thread_start_bytes.to_vec(),
                turn_start_bytes: source.turn_start_bytes.to_vec(),
                skill_bytes: verified.fixture_bytes("leadSkill")?.to_vec(),
                codex_binary_sha: source.codex_binary_sha256.to_string(),
                evaluator_binary_sha: source.evaluator_binary_sha256.to_string(),
                broker_component_sha: source.broker_component_sha256,
                model_label: source.model_label.to_string(),
                provider_label: source.provider_label.to_string(),
                max_output_tokens: 0,
                max_attempts: source.max_provider_request_attempts,
                max_total_tokens: source.max_total_tokens,
                max_elapsed_seconds: source.max_elapsed_seconds,
                replay_request_parity: Some(request_parity),
            })
        }
        FrozenInputToken::Native { frozen, content } => {
            let source = content.projection();
            Ok(ContentBinding {
                mode: ExecutionMode::Mock,
                private_root: frozen.private_root().to_path_buf(),
                pair_id: frozen.pair_id().to_string(),
                fork_sha: frozen.fork_sha().to_string(),
                frozen_sha: frozen.sha256().to_string(),
                fixture_set_sha: None,
                attestation_sha: Some(sha256(&source.attestation_bytes)),
                reviewers: source.reviewers.clone(),
                mission_case: source.mission_case.clone(),
                case_bytes: source.case_bytes.clone(),
                materials_manifest_bytes: source.materials_manifest_bytes.clone(),
                materials: source
                    .materials
                    .iter()
                    .map(|item| PairMaterialInput {
                        material_id: item.material_id.clone(),
                        relative_path: item.relative_path.clone(),
                        sha256: item.sha256.clone(),
                        bytes: item.bytes.clone(),
                    })
                    .collect(),
                prompt_bytes: source.prompt_bytes.clone(),
                additional_context_bytes: source.additional_context_bytes.clone(),
                schema_bytes: source.schema_bytes.clone(),
                thread_start_bytes: source.thread_start_bytes.clone(),
                turn_start_bytes: source.turn_start_bytes.clone(),
                skill_bytes: source.skill_bytes.clone(),
                codex_binary_sha: source.codex_binary_sha256.clone(),
                evaluator_binary_sha: source.evaluator_binary_sha256.clone(),
                broker_component_sha: source.broker_component_sha256.clone(),
                model_label: source.model_label.clone(),
                provider_label: source.provider_label.clone(),
                max_output_tokens: source.max_output_tokens,
                max_attempts: source.max_provider_request_attempts,
                max_total_tokens: source.max_total_tokens_per_run,
                max_elapsed_seconds: source.max_elapsed_seconds_per_run,
                replay_request_parity: None,
            })
        }
    }
}

fn replay_request_parity(
    verified: &crate::runner::VerifiedReplayFrozenContext,
    homes: &crate::IsolatedHomes,
) -> Result<PairRequestParity> {
    let generic =
        crate::runner::canonicalize_replay_request(verified, EvaluationCondition::Generic, homes)?;
    let candidate = crate::runner::canonicalize_replay_request(
        verified,
        EvaluationCondition::Candidate,
        homes,
    )?;
    if generic.normalized_base_commitment != candidate.normalized_base_commitment
        || generic.treatment_diff_commitment.is_some()
    {
        bail!("retained Replay requests differ outside the canonical Skill treatment");
    }
    let treatment = candidate
        .treatment_diff_commitment
        .context("retained Replay candidate request has no Skill treatment")?;
    Ok(PairRequestParity {
        candidate_normalized_commitment: digest_hex(candidate.normalized_sha256),
        candidate_raw_commitment: digest_hex(candidate.raw_sha256),
        candidate_treatment_diff_commitment: digest_hex(treatment),
        generic_normalized_commitment: digest_hex(generic.normalized_sha256),
        generic_raw_commitment: digest_hex(generic.raw_sha256),
        normalized_base_commitment: digest_hex(generic.normalized_base_commitment),
        schema_version: 1,
    })
}

fn exact_reviewers(reviewers: &[ReviewerDeclaration]) -> Result<[ReviewerDeclaration; 3]> {
    reviewers
        .to_vec()
        .try_into()
        .map_err(|items: Vec<_>| anyhow::anyhow!("expected three reviewers, got {}", items.len()))
}

pub(super) fn verify_manifest_bindings(
    content: &ContentBinding,
    arms: &[super::ExactDocument<RunManifest>; 2],
) -> Result<()> {
    let case_sha = sha256(&content.case_bytes);
    let materials_sha = sha256(&content.materials_manifest_bytes);
    let skill_sha = sha256(&content.skill_bytes);
    for arm in arms {
        let manifest = &arm.typed;
        if manifest.case_sha256 != case_sha
            || manifest.source_materials_sha256 != materials_sha
            || manifest.prompt_sha256 != sha256(&content.prompt_bytes)
            || manifest.additional_context_sha256 != sha256(&content.additional_context_bytes)
            || manifest.output_schema_sha256 != sha256(&content.schema_bytes)
            || manifest.thread_start_request_sha256 != sha256(&content.thread_start_bytes)
            || manifest.turn_start_request_sha256 != sha256(&content.turn_start_bytes)
            || manifest.codex_binary_sha256 != content.codex_binary_sha
            || manifest.evaluator_binary_sha256 != content.evaluator_binary_sha
            || manifest.broker_component_sha256 != content.broker_component_sha
            || manifest.model_label != content.model_label
            || manifest.provider_label != content.provider_label
            || manifest.provider_compatibility_name != ProofBrokerCompatibilityName::OpenAi
            || manifest.max_provider_request_attempts != content.max_attempts
            || manifest.max_total_tokens != i64::try_from(content.max_total_tokens)?
            || manifest.max_elapsed_seconds != content.max_elapsed_seconds
            || manifest.authorized_evaluation_run_cost_fen != 0
            || !manifest.tree_closed
        {
            bail!("run manifest differs from verified C1 content inputs");
        }
        let expected_skill =
            (manifest.condition == EvaluationCondition::Candidate).then_some(skill_sha.clone());
        if manifest.native_skill_sha256 != expected_skill {
            bail!("run manifest Skill treatment differs from the assigned condition");
        }
        if content.mode == ExecutionMode::Replay
            && (manifest.provider_request_attempt_count != 0
                || manifest.provider_completed_response_count != 0
                || manifest.broker_attempt_ledger_sha256 != sha256(b"replay:no-broker-ledger")
                || manifest.attempt_index_root_sha256 != sha256(b"replay:no-provider-attempts")
                || manifest.usage_scope != "completeTypedReplayTranscript"
                || manifest.actual_model_revision != "replay-fixture-recording"
                || manifest.deployment_or_fingerprint_commitment.is_some()
                || manifest.elapsed_ms != 0)
        {
            bail!("Replay manifest violates provider-not-run semantics");
        }
        match (&manifest.mode_evidence, content.mode) {
            (ModeEvidence::Replay { fixture_set_sha256 }, ExecutionMode::Replay)
                if Some(fixture_set_sha256) == content.fixture_set_sha.as_ref() => {}
            (
                ModeEvidence::Mock {
                    provider_mode: MockProviderMode::NotRun,
                    synthetic_fixture_sha256,
                    ..
                },
                ExecutionMode::Mock,
            ) if Some(synthetic_fixture_sha256) == content.attestation_sha.as_ref() => {}
            _ => bail!("run manifest mode evidence differs from verified C1 inputs"),
        }
    }
    let [first, second] = arms.each_ref().map(|arm| &arm.typed);
    if first.shared_config_sha256 != second.shared_config_sha256
        || first.effective_config_sha256 != second.effective_config_sha256
        || first.config_layers_sha256 != second.config_layers_sha256
        || first.normalized_base_catalog_sha256 != second.normalized_base_catalog_sha256
        || first.normalized_first_root_base_commitment
            != second.normalized_first_root_base_commitment
        || first.actual_model_revision != second.actual_model_revision
        || first.deployment_or_fingerprint_commitment != second.deployment_or_fingerprint_commitment
        || first.mode_evidence != second.mode_evidence
    {
        bail!("sealed arms differ outside the authorized Skill treatment and outputs");
    }
    Ok(())
}

pub(super) fn verify_execution_and_pair(
    content: &ContentBinding,
    execution: &ExecutionContext,
    pair: &PairVerification,
    arms: &[super::ExactDocument<RunManifest>; 2],
) -> Result<()> {
    verify_deterministic_shared_config(content, execution, arms)?;
    match (execution, pair) {
        (ExecutionContext::Replay(_), PairVerification::Replay(pair)) => {
            if pair.candidate_skill_sha256 != sha256(&content.skill_bytes)
                || pair.normalized_base_catalog_sha256
                    != arms[0].typed.normalized_base_catalog_sha256
            {
                bail!("Replay semantic envelopes differ from verified C1 inputs");
            }
            if content.replay_request_parity.as_ref() != Some(&pair.request_parity) {
                bail!("Replay request commitments differ from retained frozen fixtures");
            }
            verify_request_parity(&pair.request_parity, arms)?;
        }
        (ExecutionContext::Native(execution), PairVerification::Native(pair)) => {
            verify_native_execution(content, execution, arms)?;
            if pair.effective_config_sha256 != arms[0].typed.effective_config_sha256
                || pair.config_layers_sha256 != arms[0].typed.config_layers_sha256
                || pair.normalized_base_catalog_sha256
                    != arms[0].typed.normalized_base_catalog_sha256
            {
                bail!("Native pair parity differs from the sealed manifests");
            }
            verify_request_parity(&pair.request_parity, arms)?;
        }
        _ => bail!("semantic envelope mode is invalid"),
    }
    Ok(())
}

fn verify_deterministic_shared_config(
    content: &ContentBinding,
    execution: &ExecutionContext,
    arms: &[super::ExactDocument<RunManifest>; 2],
) -> Result<()> {
    let broker_port = match execution {
        ExecutionContext::Replay(_) => 1,
        ExecutionContext::Native(context) => context.broker.port,
    };
    let expected = crate::build_shared_config(&content.model_label, broker_port)?;
    let expected_sha = sha256(&expected.bytes);
    if arms
        .iter()
        .any(|arm| arm.typed.shared_config_sha256 != expected_sha)
        || matches!(
            execution,
            ExecutionContext::Native(context) if context.shared_config_sha256 != expected_sha
        )
    {
        bail!("shared config bytes differ from deterministic producer config");
    }
    Ok(())
}

fn verify_native_execution(
    content: &ContentBinding,
    execution: &NativeExecutionContext,
    arms: &[super::ExactDocument<RunManifest>; 2],
) -> Result<()> {
    let started = DateTime::parse_from_rfc3339(&execution.started_at)?;
    let expected_deadline = started
        .checked_add_signed(chrono::Duration::seconds(i64::try_from(
            content.max_elapsed_seconds,
        )?))
        .context("native execution deadline overflow")?;
    if execution.model_label != content.model_label
        || execution.max_output_tokens_per_request != content.max_output_tokens
        || execution.max_provider_request_attempts_per_run != content.max_attempts
        || execution.max_total_tokens_per_run != content.max_total_tokens
        || execution.max_elapsed_seconds_per_run != content.max_elapsed_seconds
        || execution.shared_config_sha256 != arms[0].typed.shared_config_sha256
        || !is_lower_sha256(&execution.path_sha256)
        || execution.generic_home != content.private_root.join("generic-home")
        || execution.generic_codex_home != content.private_root.join("generic-home/.codex")
        || execution.candidate_home != content.private_root.join("candidate-home")
        || execution.candidate_codex_home != content.private_root.join("candidate-home/.codex")
        || execution.broker.host != "127.0.0.1"
        || execution.broker.path != "/v1/responses"
        || execution.broker.port == 0
        || DateTime::parse_from_rfc3339(&execution.deadline)? != expected_deadline
    {
        bail!("Native execution context differs from verified inputs or manifests");
    }
    for arm in arms {
        match &arm.typed.mode_evidence {
            ModeEvidence::Mock {
                arm_order_commitment,
                ..
            } if arm_order_commitment == &execution.arm_order_commitment => {}
            _ => bail!("Native arm order differs from execution context"),
        }
    }
    Ok(())
}

fn verify_request_parity(
    parity: &PairRequestParity,
    arms: &[super::ExactDocument<RunManifest>; 2],
) -> Result<()> {
    let generic = arms
        .iter()
        .find(|arm| arm.typed.condition == EvaluationCondition::Generic)
        .context("missing generic arm")?;
    let candidate = arms
        .iter()
        .find(|arm| arm.typed.condition == EvaluationCondition::Candidate)
        .context("missing candidate arm")?;
    if parity.schema_version != 1
        || parity.generic_raw_commitment != generic.typed.first_root_provider_request_commitment
        || parity.generic_normalized_commitment
            != generic.typed.normalized_first_root_request_commitment
        || generic.typed.first_root_treatment_diff_commitment.is_some()
        || parity.candidate_raw_commitment != candidate.typed.first_root_provider_request_commitment
        || parity.candidate_normalized_commitment
            != candidate.typed.normalized_first_root_request_commitment
        || candidate
            .typed
            .first_root_treatment_diff_commitment
            .as_deref()
            != Some(&parity.candidate_treatment_diff_commitment)
        || parity.normalized_base_commitment != generic.typed.normalized_first_root_base_commitment
        || parity.normalized_base_commitment
            != candidate.typed.normalized_first_root_base_commitment
    {
        bail!("request parity differs from the sealed run manifests");
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
