//! Immutable blind-decision preparation and publication.

use anyhow::Result;
use anyhow::bail;
use chrono::SecondsFormat;
use chrono::Utc;
use sha2::Digest;
use sha2::Sha256;

use crate::ScoreArgs;
use crate::blind_verify::PairEvidenceCore;
use crate::private_inventory::batch::RetainedPendingReview;
use crate::private_inventory::batch::RetainedScoreDecisionStaging;
use crate::score_authority::VerifiedScoreAuthority;
use crate::score_validation::ScoreBundleSourceBinding;
use crate::score_validation::ScoreReviewerEvidence;
use crate::score_validation::ScoreValidationInput;
use crate::score_validation::ScoreValidationOutcome;
use crate::score_validation::ValidatedScoreReviews;
use crate::secure_fs_retain::RetainedBoundedFile;
use crate::secure_fs_retain::RetainedLeafPermissions;

pub(crate) struct RetainedReviewerEvidence {
    mapping: RetainedBoundedFile,
    bundle: RetainedBoundedFile,
}

pub(crate) struct ScoreSourceCommitments {
    case_sha256: String,
    materials_manifest_sha256: String,
    source_materials_sha256: String,
    generic_package_sha256: String,
    candidate_package_sha256: String,
    rubric_sha256: String,
    decision_policy_sha256: String,
    reviewer_submission_schema_sha256: String,
}

impl RetainedReviewerEvidence {
    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        self.mapping.reverify_unchanged()?;
        self.bundle.reverify_unchanged()?;
        self.mapping.reverify_unchanged()
    }
}

impl ScoreSourceCommitments {
    pub(crate) fn from_pair(pair: &PairEvidenceCore) -> Result<Self> {
        let compact_materials = serde_json::to_vec(&pair.materials_manifest)?;
        if compact_materials != pair.materials_manifest_bytes {
            bail!("score materials differ from their sealed producer encoding");
        }
        let contracts = crate::FrozenContracts::load()?.blind_review_contracts()?;
        Ok(Self {
            case_sha256: sha256(&pair.case_bytes),
            materials_manifest_sha256: sha256(&pair.materials_manifest_bytes),
            source_materials_sha256: sha256(&compact_materials),
            generic_package_sha256: package_sha(pair, crate::EvaluationCondition::Generic)?,
            candidate_package_sha256: package_sha(pair, crate::EvaluationCondition::Candidate)?,
            rubric_sha256: contracts.rubric_sha256,
            decision_policy_sha256: contracts.decision_policy_sha256,
            reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256,
        })
    }
}

pub(crate) fn retain_reviewer_evidence(
    pair: &PairEvidenceCore,
) -> Result<[RetainedReviewerEvidence; 3]> {
    let evidence = pair
        .reviewers
        .iter()
        .map(|reviewer| {
            let mapping = pair.private_root.join(format!(
                "coordinator/mappings/{}.json",
                reviewer.reviewer_id
            ));
            let bundle = pair.private_root.join(format!(
                "reviewer/{}/review-bundle.json",
                reviewer.reviewer_id
            ));
            Ok(RetainedReviewerEvidence {
                mapping: RetainedBoundedFile::retain(
                    &mapping,
                    64 * 1024,
                    RetainedLeafPermissions::RequireOwnerOnly,
                )?,
                bundle: RetainedBoundedFile::retain(
                    &bundle,
                    1024 * 1024,
                    RetainedLeafPermissions::RequireOwnerOnly,
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    evidence
        .try_into()
        .map_err(|_| anyhow::anyhow!("score requires exactly three retained reviewer bundles"))
}

pub(crate) fn validate_reviews(
    pair: &PairEvidenceCore,
    receipt_raw: &[u8],
    sources: &ScoreSourceCommitments,
    evidence: &[RetainedReviewerEvidence; 3],
    reviews: &[RetainedPendingReview; 3],
) -> Result<ValidatedScoreReviews> {
    crate::score_validation::validate_score_reviews(ScoreValidationInput {
        pair_id: &pair.pair_id,
        frozen_context_sha256: &pair.frozen_run_context_sha256,
        blind_pack_receipt_raw: receipt_raw,
        rubric_sha256: &sources.rubric_sha256,
        decision_policy_sha256: &sources.decision_policy_sha256,
        bundle_sources: ScoreBundleSourceBinding {
            case_sha256: &sources.case_sha256,
            materials_manifest_sha256: &sources.materials_manifest_sha256,
            source_materials_sha256: &sources.source_materials_sha256,
            generic_package_sha256: &sources.generic_package_sha256,
            candidate_package_sha256: &sources.candidate_package_sha256,
            reviewer_submission_schema_sha256: &sources.reviewer_submission_schema_sha256,
        },
        reviewers: std::array::from_fn(|index| ScoreReviewerEvidence {
            declaration: &pair.reviewers[index],
            mapping_raw: evidence[index].mapping.raw_bytes(),
            review_bundle_raw: evidence[index].bundle.raw_bytes(),
            submission_raw: reviews[index].raw_bytes(),
        }),
    })
}

pub(crate) fn commit_score(
    authority: &VerifiedScoreAuthority,
    args: &ScoreArgs,
    validated: ValidatedScoreReviews,
) -> Result<()> {
    let generated_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let decision = match validated.outcome {
        ScoreValidationOutcome::Valid(reviews) => crate::score_decision::BlindDecision::valid(
            validated.commitments,
            crate::score::score_valid_reviews(&reviews)?,
            &generated_at,
        )?,
        ScoreValidationOutcome::Invalid(failures) => crate::score_decision::BlindDecision::invalid(
            validated.commitments,
            failures,
            &generated_at,
        )?,
    };
    let decision_bytes = decision.canonical_bytes()?;
    authority.reverify_unchanged(args)?;
    crate::secure_fs::write_owner_only_new(authority.paths().staging_output(), &decision_bytes)?;
    let staging = RetainedScoreDecisionStaging::retain(authority.private_root(), &decision_bytes)?;
    authority.reverify_with_staging(args, &staging)?;
    staging.publish_no_replace(std::path::Path::new(crate::score_authority::OUTPUT))?;
    crate::private_inventory::batch::append_private_inventory_batch_from_root(
        authority.private_root(),
        authority.inventory_root_sha256(),
        &authority.inventory_entries(&decision_bytes),
    )?;
    Ok(())
}

fn package_sha(pair: &PairEvidenceCore, condition: crate::EvaluationCondition) -> Result<String> {
    pair.arms
        .iter()
        .find(|arm| arm.condition == condition)
        .map(|arm| sha256(&arm.content_package_bytes))
        .ok_or_else(|| anyhow::anyhow!("sealed score pair is missing one condition"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
