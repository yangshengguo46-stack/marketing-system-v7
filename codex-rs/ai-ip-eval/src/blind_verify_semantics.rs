use std::path::PathBuf;

use anyhow::Result;
use anyhow::bail;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::ReviewerDeclaration;
use crate::SkillUseOutcome;
use crate::Usage;
use crate::blind::FrozenInputToken;
use crate::cost_authority::CostPairProjection;
use crate::cost_authority::NativeCostProjectionInput;
use crate::private_inventory::VerifiedPrivateInventory;
use crate::proof_archive::VerifiedPostprocessSummary;
use crate::proof_ledger::BoundRunManifest;
use crate::proof_ledger::NativeLedgerBinding;
use crate::proof_ledger::VerifiedNativeLedger;

use super::ExecutionContext;
use super::ParsedPairEvidence;
use super::semantics_bindings::ContentBinding;
use super::semantics_bindings::PairMaterialInput;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PairArmCore {
    pub(crate) run_ordinal: u8,
    pub(crate) condition: EvaluationCondition,
    pub(crate) run_manifest_raw_sha256: String,
    pub(crate) content_package: codex_ai_ip_domain::ContentPackage,
    pub(crate) content_package_bytes: Vec<u8>,
    pub(crate) usage: Usage,
    pub(crate) raw_response_count: u64,
    pub(crate) skill_use: SkillUseOutcome,
}

#[derive(Debug)]
pub(crate) struct PairEvidenceCore {
    pub(crate) inputs: FrozenInputToken,
    pub(crate) inventory: VerifiedPrivateInventory,
    pub(crate) mode: ExecutionMode,
    pub(crate) private_root: PathBuf,
    pub(crate) pair_id: String,
    pub(crate) fork_sha: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) initial_inventory_root_sha256: String,
    pub(crate) pair_verification_raw_sha256: String,
    pub(crate) native_pair_receipt_raw_sha256: Option<String>,
    pub(crate) cost_projection: Option<CostPairProjection>,
    pub(crate) reviewers: [ReviewerDeclaration; 3],
    pub(crate) mission_case: codex_ai_ip_domain::HeldOutMissionCase,
    pub(crate) case_bytes: Vec<u8>,
    pub(crate) materials_manifest: Vec<codex_ai_ip_domain::MissionMaterial>,
    pub(crate) materials_manifest_bytes: Vec<u8>,
    pub(crate) materials: Vec<PairMaterialInput>,
    pub(crate) arms: [PairArmCore; 2],
}

pub(super) fn verify_parsed_pair_evidence(parsed: ParsedPairEvidence) -> Result<PairEvidenceCore> {
    let ParsedPairEvidence {
        inputs,
        inventory,
        inventory_binding,
        execution_context,
        arms,
        pair_verification,
    } = parsed;
    let content = super::semantics_bindings::content_binding(&inputs)?;
    if inventory_binding.private_root != content.private_root
        || inventory_binding.pair_id != content.pair_id
        || inventory_binding.frozen_run_context_sha256 != content.frozen_sha
        || inventory_binding.inventory_root_sha256 != inventory.inventory_root_sha256()
    {
        bail!("retained inventory binding differs from verified C1 content inputs");
    }
    super::semantics_bindings::verify_manifest_bindings(&content, &arms)?;
    super::semantics_bindings::verify_execution_and_pair(
        &content,
        &execution_context.typed,
        &pair_verification.typed,
        &arms,
    )?;
    let first = crate::proof_archive::verify_postprocess_archive_summary(
        &content.private_root,
        &arms[0].typed,
        &content.mission_case,
        &content.skill_bytes,
    )?;
    let second = crate::proof_archive::verify_sequential_postprocess_archive_summary(
        &content.private_root,
        &arms[1].typed,
        &content.mission_case,
        &content.skill_bytes,
        &first,
    )?;
    let summaries = [first, second];
    verify_archive_projections(&arms, &summaries)?;
    let cost_projection = verify_proof_ledger(&content, &execution_context, &arms, &summaries)?;
    let native_pair_receipt_raw_sha256 = cost_projection
        .as_ref()
        .map(|projection| projection.pair_receipt_sha256.clone());
    let [first_arm, second_arm] = arms;
    let [first_summary, second_summary] = summaries;
    let core_arms = [
        arm_core(first_arm, first_summary),
        arm_core(second_arm, second_summary),
    ];
    Ok(PairEvidenceCore {
        inputs,
        inventory,
        mode: content.mode,
        private_root: content.private_root,
        pair_id: content.pair_id,
        fork_sha: content.fork_sha,
        frozen_run_context_sha256: content.frozen_sha,
        initial_inventory_root_sha256: inventory_binding.inventory_root_sha256,
        pair_verification_raw_sha256: pair_verification.sha256,
        native_pair_receipt_raw_sha256,
        cost_projection,
        reviewers: content.reviewers,
        materials_manifest: content.mission_case.materials.clone(),
        mission_case: content.mission_case,
        case_bytes: content.case_bytes,
        materials_manifest_bytes: content.materials_manifest_bytes,
        materials: content.materials,
        arms: core_arms,
    })
}

fn arm_core(
    arm: super::ExactDocument<crate::RunManifest>,
    summary: VerifiedPostprocessSummary,
) -> PairArmCore {
    PairArmCore {
        run_ordinal: arm.typed.run_ordinal,
        condition: arm.typed.condition,
        run_manifest_raw_sha256: arm.sha256,
        content_package: summary.content_package,
        content_package_bytes: summary.content_package_bytes,
        usage: summary.usage,
        raw_response_count: summary.raw_response_count,
        skill_use: summary.skill_use,
    }
}

fn verify_archive_projections(
    arms: &[super::ExactDocument<crate::RunManifest>; 2],
    summaries: &[VerifiedPostprocessSummary; 2],
) -> Result<()> {
    for (arm, summary) in arms.iter().zip(summaries) {
        let manifest = &arm.typed;
        if summary.usage != manifest.usage
            || summary.raw_response_count != manifest.raw_response_count
            || summary.tree_closed != manifest.tree_closed
            || summary.normalized_base_catalog_sha256 != manifest.normalized_base_catalog_sha256
            || summary.skill_use.evidence_sha256 != manifest.skill_use_evidence_sha256
            || sha256(&summary.content_package_bytes) != manifest.content_package_sha256
        {
            bail!("archive summary differs from its sealed run manifest");
        }
        let expected_skill_use = manifest.condition == EvaluationCondition::Candidate;
        if summary.skill_use.successful_read_observed != expected_skill_use
            || expected_skill_use != summary.skill_use.evidence_sha256.is_some()
        {
            bail!("archive Skill-use outcome differs from the assigned treatment");
        }
    }
    if summaries[0].normalized_base_catalog_sha256 != summaries[1].normalized_base_catalog_sha256 {
        bail!("cross-arm archive catalog parity is invalid");
    }
    Ok(())
}

fn verify_proof_ledger(
    content: &ContentBinding,
    execution: &super::ExactDocument<ExecutionContext>,
    arms: &[super::ExactDocument<crate::RunManifest>; 2],
    summaries: &[VerifiedPostprocessSummary; 2],
) -> Result<Option<CostPairProjection>> {
    match &execution.typed {
        ExecutionContext::Replay(_) => {
            crate::proof_ledger::verify_replay_proof_ledger_absence(&content.private_root)?;
            Ok(None)
        }
        ExecutionContext::Native(context) => {
            let binding = NativeLedgerBinding {
                pair_id: &content.pair_id,
                frozen_run_context_sha256: &content.frozen_sha,
                execution_context_sha256: &execution.sha256,
                arm_order_commitment: &context.arm_order_commitment,
                deadline: &context.deadline,
                max_output_tokens: content.max_output_tokens,
                max_attempts_per_arm: content.max_attempts,
                max_total_tokens_per_run: content.max_total_tokens,
                manifests: arms.each_ref().map(|arm| BoundRunManifest {
                    manifest: &arm.typed,
                    raw_bytes: &arm.raw_bytes,
                    raw_sha256: &arm.sha256,
                }),
            };
            let ledger =
                crate::proof_ledger::verify_native_proof_ledger(&content.private_root, &binding)?;
            verify_native_ledger_projection(&ledger, arms, summaries)?;
            crate::cost_authority::project_verified_native_cost(NativeCostProjectionInput {
                execution_mode: content.mode,
                execution_context_sha256: &execution.sha256,
                started_at: &context.started_at,
                deadline: &context.deadline,
                arm_order_commitment: &context.arm_order_commitment,
                manifests: arms,
                ledger,
            })
            .map(Some)
        }
    }
}

fn verify_native_ledger_projection(
    ledger: &VerifiedNativeLedger,
    arms: &[super::ExactDocument<crate::RunManifest>; 2],
    summaries: &[VerifiedPostprocessSummary; 2],
) -> Result<()> {
    for ((verified, arm), summary) in ledger.attempt_ledger.arms.iter().zip(arms).zip(summaries) {
        let manifest = &arm.typed;
        if verified.condition != manifest.condition
            || verified.global_start_inclusive != summary.broker_global_attempt_start_inclusive
            || verified.global_end_exclusive != summary.broker_global_attempt_end_exclusive
            || verified.provider_request_attempt_count != manifest.provider_request_attempt_count
            || verified.provider_completed_response_count
                != manifest.provider_completed_response_count
            || verified.raw_response_count != summary.raw_response_count
            || verified.usage != summary.usage
            || verified.attempt_index_prefix_sha256 != manifest.broker_attempt_ledger_sha256
            || verified.attempt_index_prefix_root_sha256 != manifest.attempt_index_root_sha256
            || verified.run_manifest_sha256 != arm.sha256
        {
            bail!("Native ledger projection differs from archives or run manifests");
        }
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
