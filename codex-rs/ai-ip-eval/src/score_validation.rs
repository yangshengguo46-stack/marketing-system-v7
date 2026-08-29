//! Pure semantic validation boundary for retained blind review evidence.

use std::collections::BTreeSet;

use anyhow::Result;
use anyhow::bail;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::FrozenContracts;
use crate::PreferredArm;
use crate::ReviewerDeclaration;
use crate::ReviewerQualificationBinding;
use crate::blind_bundle_model::BlindPackReceipt;
use crate::blind_bundle_model::ReviewBundleManifest;
use crate::blind_bundle_model::ReviewerMapping;
use crate::blind_bundle_model::ReviewerMappingCommitment;
use crate::blind_bundle_model::ReviewerQualificationCommitment;
use crate::score::MappedPreference;
use crate::score::MappedReview;
use crate::score::VerifiedMappedReviews;
use crate::score::verify_mapped_reviews;
use crate::score_decision::BlindDecisionFailure;
use crate::score_decision::DecisionCommitments;
use crate::score_decision::review_submission_set_commitment;
use crate::score_decision::reviewer_mapping_set_commitment;

pub(crate) struct ScoreReviewerEvidence<'a> {
    pub(crate) declaration: &'a ReviewerDeclaration,
    pub(crate) mapping_raw: &'a [u8],
    pub(crate) review_bundle_raw: &'a [u8],
    pub(crate) submission_raw: &'a [u8],
}

pub(crate) struct ScoreBundleSourceBinding<'a> {
    pub(crate) case_sha256: &'a str,
    pub(crate) materials_manifest_sha256: &'a str,
    pub(crate) source_materials_sha256: &'a str,
    pub(crate) generic_package_sha256: &'a str,
    pub(crate) candidate_package_sha256: &'a str,
    pub(crate) reviewer_submission_schema_sha256: &'a str,
}

pub(crate) struct ScoreValidationInput<'a> {
    pub(crate) pair_id: &'a str,
    pub(crate) frozen_context_sha256: &'a str,
    pub(crate) blind_pack_receipt_raw: &'a [u8],
    pub(crate) rubric_sha256: &'a str,
    pub(crate) decision_policy_sha256: &'a str,
    pub(crate) bundle_sources: ScoreBundleSourceBinding<'a>,
    pub(crate) reviewers: [ScoreReviewerEvidence<'a>; 3],
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum ScoreValidationOutcome {
    Valid(VerifiedMappedReviews),
    Invalid(Vec<BlindDecisionFailure>),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ValidatedScoreReviews {
    pub(crate) commitments: DecisionCommitments,
    pub(crate) outcome: ScoreValidationOutcome,
}

pub(crate) fn validate_score_reviews(
    input: ScoreValidationInput<'_>,
) -> Result<ValidatedScoreReviews> {
    let mapping_entries = input.reviewers.each_ref().map(|reviewer| {
        (
            reviewer.declaration.reviewer_id.as_str(),
            reviewer.mapping_raw,
        )
    });
    let review_entries = input.reviewers.each_ref().map(|reviewer| {
        (
            reviewer.declaration.reviewer_id.as_str(),
            reviewer.submission_raw,
        )
    });
    let commitments = DecisionCommitments {
        pair_id: input.pair_id.to_string(),
        frozen_run_context_sha256: input.frozen_context_sha256.to_string(),
        blind_pack_receipt_sha256: sha256(input.blind_pack_receipt_raw),
        reviewer_mappings_sha256: reviewer_mapping_set_commitment(&mapping_entries)?,
        review_submissions_sha256: review_submission_set_commitment(&review_entries)?,
        rubric_sha256: input.rubric_sha256.to_string(),
        decision_policy_sha256: input.decision_policy_sha256.to_string(),
    };
    let contracts = FrozenContracts::load().map_err(anyhow::Error::new)?;
    let receipt = exact_jcs::<BlindPackReceipt>(input.blind_pack_receipt_raw)
        .map_err(|()| anyhow::anyhow!("verified blind receipt is not exact canonical JSON"))?;
    let expected_ids = input
        .reviewers
        .iter()
        .map(|reviewer| reviewer.declaration.reviewer_id.as_str())
        .collect::<BTreeSet<_>>();
    let receipt_ids = receipt
        .reviewer_mappings
        .iter()
        .map(|commitment| commitment.reviewer_id.as_str())
        .collect::<BTreeSet<_>>();
    if receipt.schema_version != 1
        || receipt.pair_id != input.pair_id
        || receipt.frozen_run_context_sha256 != input.frozen_context_sha256
        || receipt.rubric_sha256 != input.rubric_sha256
        || receipt.decision_policy_sha256 != input.decision_policy_sha256
        || receipt.reviewer_submission_schema_sha256
            != input.bundle_sources.reviewer_submission_schema_sha256
        || expected_ids.len() != 3
        || receipt.reviewer_mappings.len() != 3
        || receipt_ids != expected_ids
    {
        bail!("verified blind receipt does not match score authority");
    }
    let mut failures = BTreeSet::new();
    let mut mapped_reviews = Vec::with_capacity(3);
    let mut matching_experienced_count = 0;

    for reviewer in &input.reviewers {
        let receipt_commitment = receipt
            .reviewer_mappings
            .iter()
            .find(|commitment| commitment.reviewer_id == reviewer.declaration.reviewer_id)
            .ok_or_else(|| anyhow::anyhow!("verified blind receipt lost reviewer commitment"))?;
        let expected_qualification = qualification_for(reviewer.declaration);
        let mapping = exact_jcs::<ReviewerMapping>(reviewer.mapping_raw).ok();
        let mapping_valid = validate_mapping(
            reviewer,
            receipt_commitment,
            mapping.as_ref(),
            input.pair_id,
            &mut failures,
        );
        let bundle = exact_jcs::<ReviewBundleManifest>(reviewer.review_bundle_raw).ok();
        let bundle_valid = validate_bundle(
            reviewer,
            receipt_commitment,
            bundle.as_ref(),
            mapping.as_ref(),
            &expected_qualification,
            &input,
            &mut failures,
        );
        let Ok(submission) = contracts.validate_reviewer_submission(reviewer.submission_raw) else {
            failures.insert(BlindDecisionFailure::ReviewSubmissionInvalid);
            continue;
        };
        let id_valid = submission.reviewer_id == reviewer.declaration.reviewer_id;
        if !id_valid {
            failures.insert(BlindDecisionFailure::ReviewerIdSetMismatch);
        }
        let qualification_valid =
            qualification_matches(&submission.qualification, reviewer.declaration);
        if !qualification_valid {
            failures.insert(BlindDecisionFailure::ReviewerQualificationMismatch);
        }
        if submission.review_bundle_sha256 != receipt_commitment.review_bundle_sha256 {
            failures.insert(BlindDecisionFailure::ReviewBundleCommitmentMismatch);
        }
        if submission.rubric_sha256 != input.rubric_sha256 {
            failures.insert(BlindDecisionFailure::ReviewRubricCommitmentMismatch);
        }
        if id_valid && qualification_valid && reviewer.declaration.experienced_operator_or_director
        {
            matching_experienced_count += 1;
        }
        if mapping_valid
            && bundle_valid
            && id_valid
            && qualification_valid
            && submission.review_bundle_sha256 == receipt_commitment.review_bundle_sha256
            && submission.rubric_sha256 == input.rubric_sha256
        {
            let mapping =
                mapping.ok_or_else(|| anyhow::anyhow!("validated mapping disappeared"))?;
            mapped_reviews.push(map_review(mapping, submission));
        }
    }
    if matching_experienced_count < 2 {
        failures.insert(BlindDecisionFailure::InsufficientExperiencedReviewers);
    }
    let outcome = if failures.is_empty() {
        ScoreValidationOutcome::Valid(
            verify_mapped_reviews(&mapped_reviews).map_err(anyhow::Error::new)?,
        )
    } else {
        ScoreValidationOutcome::Invalid(failures.into_iter().collect())
    };
    Ok(ValidatedScoreReviews {
        commitments,
        outcome,
    })
}

fn validate_mapping(
    reviewer: &ScoreReviewerEvidence<'_>,
    receipt: &ReviewerMappingCommitment,
    mapping: Option<&ReviewerMapping>,
    pair_id: &str,
    failures: &mut BTreeSet<BlindDecisionFailure>,
) -> bool {
    let raw_valid = sha256(reviewer.mapping_raw) == receipt.mapping_sha256;
    let Some(mapping) = mapping else {
        failures.insert(BlindDecisionFailure::ReviewMappingCommitmentMismatch);
        return false;
    };
    let identity_valid = mapping.reviewer_id == reviewer.declaration.reviewer_id
        && receipt.reviewer_id == reviewer.declaration.reviewer_id;
    if !identity_valid {
        failures.insert(BlindDecisionFailure::ReviewerIdSetMismatch);
    }
    let structure_valid =
        mapping.schema_version == 1 && mapping.pair_id == pair_id && mapping.a != mapping.b;
    if !raw_valid || !structure_valid {
        failures.insert(BlindDecisionFailure::ReviewMappingCommitmentMismatch);
    }
    let bundle_valid = mapping.review_bundle_sha256 == receipt.review_bundle_sha256;
    if !bundle_valid {
        failures.insert(BlindDecisionFailure::ReviewBundleCommitmentMismatch);
    }
    let seed_valid = mapping.seed_commitment == receipt.seed_commitment;
    if !seed_valid {
        failures.insert(BlindDecisionFailure::ReviewSeedCommitmentMismatch);
    }
    raw_valid && identity_valid && structure_valid && bundle_valid && seed_valid
}

fn validate_bundle(
    reviewer: &ScoreReviewerEvidence<'_>,
    receipt: &ReviewerMappingCommitment,
    bundle: Option<&ReviewBundleManifest>,
    mapping: Option<&ReviewerMapping>,
    expected_qualification: &ReviewerQualificationCommitment,
    input: &ScoreValidationInput<'_>,
    failures: &mut BTreeSet<BlindDecisionFailure>,
) -> bool {
    let Some(bundle) = bundle else {
        failures.insert(BlindDecisionFailure::ReviewBundleCommitmentMismatch);
        return false;
    };
    let qualification_valid = &bundle.qualification == expected_qualification;
    if !qualification_valid {
        failures.insert(BlindDecisionFailure::ReviewerQualificationMismatch);
    }
    let rubric_valid = bundle.rubric_sha256 == input.rubric_sha256;
    if !rubric_valid {
        failures.insert(BlindDecisionFailure::ReviewRubricCommitmentMismatch);
    }
    let reviewer_id_valid = bundle.reviewer_id == reviewer.declaration.reviewer_id;
    if !reviewer_id_valid {
        failures.insert(BlindDecisionFailure::ReviewerIdSetMismatch);
    }
    let sources = &input.bundle_sources;
    let identity_and_sources_valid = sha256(reviewer.review_bundle_raw)
        == receipt.review_bundle_sha256
        && bundle.schema_version == 1
        && bundle.pair_id == input.pair_id
        && bundle.case_sha256 == sources.case_sha256
        && bundle.materials_manifest_sha256 == sources.materials_manifest_sha256
        && bundle.source_materials_sha256 == sources.source_materials_sha256
        && bundle.reviewer_submission_schema_sha256 == sources.reviewer_submission_schema_sha256;
    let orientation_valid = mapping.is_some_and(|mapping| {
        bundle.a_sha256 == package_sha(mapping.a, sources)
            && bundle.b_sha256 == package_sha(mapping.b, sources)
    });
    if !identity_and_sources_valid || mapping.is_some() && !orientation_valid {
        failures.insert(BlindDecisionFailure::ReviewBundleCommitmentMismatch);
    }
    qualification_valid
        && rubric_valid
        && reviewer_id_valid
        && identity_and_sources_valid
        && orientation_valid
}

fn package_sha<'a>(
    condition: EvaluationCondition,
    sources: &'a ScoreBundleSourceBinding<'_>,
) -> &'a str {
    match condition {
        EvaluationCondition::Generic => sources.generic_package_sha256,
        EvaluationCondition::Candidate => sources.candidate_package_sha256,
    }
}

fn qualification_for(declaration: &ReviewerDeclaration) -> ReviewerQualificationCommitment {
    ReviewerQualificationCommitment {
        qualification_class: declaration.qualification_class.clone(),
        experienced_operator_or_director: declaration.experienced_operator_or_director,
        attestation_signed_payload_sha256: declaration.signed_payload_sha256.clone(),
        attestation_signature_evidence_sha256: declaration.signature_evidence_sha256.clone(),
    }
}

fn qualification_matches(
    qualification: &ReviewerQualificationBinding,
    declaration: &ReviewerDeclaration,
) -> bool {
    qualification.qualification_class == declaration.qualification_class
        && qualification.experienced_operator_or_director
            == declaration.experienced_operator_or_director
        && qualification.attestation_signed_payload_sha256 == declaration.signed_payload_sha256
        && qualification.attestation_signature_evidence_sha256
            == declaration.signature_evidence_sha256
}

fn map_review(mapping: ReviewerMapping, submission: crate::ReviewerSubmission) -> MappedReview {
    let preferred = match submission.preferred {
        PreferredArm::A => map_preference(mapping.a),
        PreferredArm::B => map_preference(mapping.b),
        PreferredArm::Tie => MappedPreference::Tie,
    };
    let (generic, candidate) = match mapping.a {
        EvaluationCondition::Generic => (submission.arms.a, submission.arms.b),
        EvaluationCondition::Candidate => (submission.arms.b, submission.arms.a),
    };
    MappedReview {
        preferred,
        generic,
        candidate,
        experienced_operator_or_director: submission.qualification.experienced_operator_or_director,
    }
}

fn map_preference(condition: EvaluationCondition) -> MappedPreference {
    match condition {
        EvaluationCondition::Generic => MappedPreference::Generic,
        EvaluationCondition::Candidate => MappedPreference::Candidate,
    }
}

fn exact_jcs<T>(raw: &[u8]) -> std::result::Result<T, ()>
where
    T: DeserializeOwned + Serialize,
{
    let value = crate::jcs::parse_json(raw).map_err(|_| ())?;
    let parsed = serde_json::from_value(value).map_err(|_| ())?;
    let canonical = serde_json_canonicalizer::to_vec(&parsed).map_err(|_| ())?;
    if canonical != raw {
        return Err(());
    }
    Ok(parsed)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
