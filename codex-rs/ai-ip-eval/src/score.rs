use std::fmt;

use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;

use crate::ArmReview;
use crate::DimensionScores;

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BusinessDecision {
    Pass,
    IterateSmallestLeadChange,
    InvalidProof,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum MappedPreference {
    Generic,
    Candidate,
    Tie,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct MappedReview {
    pub(crate) preferred: MappedPreference,
    pub(crate) generic: ArmReview,
    pub(crate) candidate: ArmReview,
    pub(crate) experienced_operator_or_director: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct VerifiedMappedReviews([MappedReview; 3]);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum MappedReviewSetError {
    WrongReviewCount,
    InsufficientExperiencedReviewers,
}

impl fmt::Display for MappedReviewSetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongReviewCount => {
                formatter.write_str("score requires exactly three mapped reviews")
            }
            Self::InsufficientExperiencedReviewers => {
                formatter.write_str("mapped reviews have fewer than two experienced reviewers")
            }
        }
    }
}

impl std::error::Error for MappedReviewSetError {}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CandidateSevereFlagCounts {
    pub fabricated_factual_claim: u8,
    pub wrong_subject_or_desired_action: u8,
    pub not_actually_usable: u8,
    pub rights_or_privacy_violation: u8,
}

impl CandidateSevereFlagCounts {
    pub fn total(&self) -> u16 {
        u16::from(self.fabricated_factual_claim)
            + u16::from(self.wrong_subject_or_desired_action)
            + u16::from(self.not_actually_usable)
            + u16::from(self.rights_or_privacy_violation)
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DecisionMetrics {
    pub reviewer_count: u8,
    pub experienced_operator_or_director_count: u8,
    pub candidate_preference_count: u8,
    pub candidate_ready_for_human_review_count: u8,
    pub generic_totals: [u8; 3],
    pub candidate_totals: [u8; 3],
    pub paired_deltas: [i16; 3],
    pub median_generic_total: u8,
    pub median_candidate_total: u8,
    pub median_paired_delta: i16,
    pub candidate_severe_failure_count: u8,
    pub candidate_severe_flags: CandidateSevereFlagCounts,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ScoredValidOutcome {
    decision: BusinessDecision,
    metrics: DecisionMetrics,
}

impl ScoredValidOutcome {
    pub(crate) fn decision(&self) -> BusinessDecision {
        self.decision
    }

    pub(crate) fn metrics(&self) -> &DecisionMetrics {
        &self.metrics
    }

    pub(crate) fn into_parts(self) -> (BusinessDecision, DecisionMetrics) {
        (self.decision, self.metrics)
    }
}

pub(crate) fn verify_mapped_reviews(
    reviews: &[MappedReview],
) -> std::result::Result<VerifiedMappedReviews, MappedReviewSetError> {
    let reviews: [MappedReview; 3] = reviews
        .to_vec()
        .try_into()
        .map_err(|_| MappedReviewSetError::WrongReviewCount)?;
    if reviews
        .iter()
        .filter(|review| review.experienced_operator_or_director)
        .count()
        < 2
    {
        return Err(MappedReviewSetError::InsufficientExperiencedReviewers);
    }
    Ok(VerifiedMappedReviews(reviews))
}

pub(crate) fn score_valid_reviews(reviews: &VerifiedMappedReviews) -> Result<ScoredValidOutcome> {
    let reviews = &reviews.0;
    let mut generic_totals = [0; 3];
    let mut candidate_totals = [0; 3];
    let mut paired_deltas = [0; 3];
    let mut experienced_operator_or_director_count = 0;
    let mut candidate_preference_count = 0;
    let mut candidate_ready_for_human_review_count = 0;
    let mut candidate_severe_flags = CandidateSevereFlagCounts {
        fabricated_factual_claim: 0,
        wrong_subject_or_desired_action: 0,
        not_actually_usable: 0,
        rights_or_privacy_violation: 0,
    };

    for (index, review) in reviews.iter().enumerate() {
        generic_totals[index] = dimension_total(&review.generic.scores)?;
        candidate_totals[index] = dimension_total(&review.candidate.scores)?;
        paired_deltas[index] = i16::from(candidate_totals[index])
            .checked_sub(i16::from(generic_totals[index]))
            .ok_or_else(|| anyhow::anyhow!("review paired delta overflow"))?;
        experienced_operator_or_director_count += u8::from(review.experienced_operator_or_director);
        candidate_preference_count += u8::from(review.preferred == MappedPreference::Candidate);
        candidate_ready_for_human_review_count += u8::from(review.candidate.ready_for_human_review);
        candidate_severe_flags.fabricated_factual_claim +=
            u8::from(review.candidate.severe_flags.fabricated_factual_claim);
        candidate_severe_flags.wrong_subject_or_desired_action += u8::from(
            review
                .candidate
                .severe_flags
                .wrong_subject_or_desired_action,
        );
        candidate_severe_flags.not_actually_usable +=
            u8::from(review.candidate.severe_flags.not_actually_usable);
        candidate_severe_flags.rights_or_privacy_violation +=
            u8::from(review.candidate.severe_flags.rights_or_privacy_violation);
    }
    generic_totals.sort_unstable();
    candidate_totals.sort_unstable();
    paired_deltas.sort_unstable();
    let candidate_severe_failure_count = u8::try_from(candidate_severe_flags.total())
        .map_err(|_| anyhow::anyhow!("candidate severe failure count overflow"))?;
    let metrics = DecisionMetrics {
        reviewer_count: 3,
        experienced_operator_or_director_count,
        candidate_preference_count,
        candidate_ready_for_human_review_count,
        generic_totals,
        candidate_totals,
        paired_deltas,
        median_generic_total: generic_totals[1],
        median_candidate_total: candidate_totals[1],
        median_paired_delta: paired_deltas[1],
        candidate_severe_failure_count,
        candidate_severe_flags,
    };
    let decision = if metrics.candidate_preference_count >= 2
        && metrics.median_paired_delta >= 3
        && metrics.median_candidate_total >= 18
        && metrics.candidate_ready_for_human_review_count >= 2
        && metrics.candidate_severe_failure_count == 0
    {
        BusinessDecision::Pass
    } else {
        BusinessDecision::IterateSmallestLeadChange
    };
    Ok(ScoredValidOutcome { decision, metrics })
}

fn dimension_total(scores: &DimensionScores) -> Result<u8> {
    [
        scores.business_outcome_clarity,
        scores.subject_audience_action_fit,
        scores.strategic_judgment,
        scores.publishable_usability,
        scores.evidence_integrity,
        scores.measurement_usefulness,
    ]
    .into_iter()
    .try_fold(0_u8, u8::checked_add)
    .ok_or_else(|| anyhow::anyhow!("review dimension total overflow"))
}
