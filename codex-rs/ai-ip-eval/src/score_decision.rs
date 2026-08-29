use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::SecondsFormat;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::score::BusinessDecision;
use crate::score::DecisionMetrics;
use crate::score::ScoredValidOutcome;

#[derive(Debug, Clone, Copy, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BlindDecisionFailure {
    ReviewSubmissionInvalid,
    ReviewerIdSetMismatch,
    ReviewerQualificationMismatch,
    ReviewMappingCommitmentMismatch,
    ReviewBundleCommitmentMismatch,
    ReviewSeedCommitmentMismatch,
    ReviewRubricCommitmentMismatch,
    InsufficientExperiencedReviewers,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct DecisionCommitments {
    pub(crate) pair_id: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) blind_pack_receipt_sha256: String,
    pub(crate) reviewer_mappings_sha256: String,
    pub(crate) review_submissions_sha256: String,
    pub(crate) rubric_sha256: String,
    pub(crate) decision_policy_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BlindDecision {
    schema_version: u32,
    pair_id: String,
    frozen_run_context_sha256: String,
    blind_pack_receipt_sha256: String,
    reviewer_mappings_sha256: String,
    review_submissions_sha256: String,
    rubric_sha256: String,
    decision_policy_sha256: String,
    decision: BusinessDecision,
    metrics: Option<DecisionMetrics>,
    validation_failures: Vec<BlindDecisionFailure>,
    generated_at: String,
}

impl BlindDecision {
    pub(crate) fn valid(
        commitments: DecisionCommitments,
        scored: ScoredValidOutcome,
        generated_at: &str,
    ) -> Result<Self> {
        let (decision, metrics) = scored.into_parts();
        if decision == BusinessDecision::InvalidProof {
            bail!("valid blind decision cannot use INVALID_PROOF");
        }
        Self::new(
            commitments,
            decision,
            Some(metrics),
            Vec::new(),
            generated_at,
        )
    }

    pub(crate) fn invalid(
        commitments: DecisionCommitments,
        failures: Vec<BlindDecisionFailure>,
        generated_at: &str,
    ) -> Result<Self> {
        let failures = failures.into_iter().collect::<BTreeSet<_>>();
        if failures.is_empty() {
            bail!("invalid blind decision requires at least one validation failure");
        }
        Self::new(
            commitments,
            BusinessDecision::InvalidProof,
            None,
            failures.into_iter().collect(),
            generated_at,
        )
    }

    pub(crate) fn canonical_bytes(&self) -> Result<Vec<u8>> {
        serde_json_canonicalizer::to_vec(self).context("canonicalize blind decision")
    }

    pub(crate) fn decision(&self) -> BusinessDecision {
        self.decision
    }

    pub(crate) fn metrics(&self) -> Option<&DecisionMetrics> {
        self.metrics.as_ref()
    }

    pub(crate) fn validation_failures(&self) -> &[BlindDecisionFailure] {
        &self.validation_failures
    }

    fn new(
        commitments: DecisionCommitments,
        decision: BusinessDecision,
        metrics: Option<DecisionMetrics>,
        validation_failures: Vec<BlindDecisionFailure>,
        generated_at: &str,
    ) -> Result<Self> {
        if commitments.pair_id.is_empty() {
            bail!("blind decision pair ID is empty");
        }
        for commitment in [
            &commitments.frozen_run_context_sha256,
            &commitments.blind_pack_receipt_sha256,
            &commitments.reviewer_mappings_sha256,
            &commitments.review_submissions_sha256,
            &commitments.rubric_sha256,
            &commitments.decision_policy_sha256,
        ] {
            if !lower_sha256(commitment) {
                bail!("blind decision commitment is not lowercase SHA-256");
            }
        }
        let parsed = chrono::DateTime::parse_from_rfc3339(generated_at)
            .context("parse blind decision generatedAt")?;
        if parsed.to_utc().to_rfc3339_opts(SecondsFormat::Millis, true) != generated_at {
            bail!("blind decision generatedAt is not canonical UTC milliseconds");
        }
        Ok(Self {
            schema_version: 1,
            pair_id: commitments.pair_id,
            frozen_run_context_sha256: commitments.frozen_run_context_sha256,
            blind_pack_receipt_sha256: commitments.blind_pack_receipt_sha256,
            reviewer_mappings_sha256: commitments.reviewer_mappings_sha256,
            review_submissions_sha256: commitments.review_submissions_sha256,
            rubric_sha256: commitments.rubric_sha256,
            decision_policy_sha256: commitments.decision_policy_sha256,
            decision,
            metrics,
            validation_failures,
            generated_at: generated_at.to_string(),
        })
    }
}

pub(crate) fn reviewer_mapping_set_commitment(entries: &[(&str, &[u8])]) -> Result<String> {
    raw_set_commitment(b"AI-IP-BLIND-MAPPING-SET-V1\0", entries)
}

pub(crate) fn review_submission_set_commitment(entries: &[(&str, &[u8])]) -> Result<String> {
    raw_set_commitment(b"AI-IP-BLIND-REVIEW-SET-V1\0", entries)
}

fn raw_set_commitment(domain: &[u8], entries: &[(&str, &[u8])]) -> Result<String> {
    if entries.len() != 3 {
        bail!("raw set commitment requires exactly three entries");
    }
    let mut entries = entries.to_vec();
    entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
    if entries
        .windows(2)
        .any(|pair| pair[0].0.as_bytes() == pair[1].0.as_bytes())
    {
        bail!("raw set commitment IDs must be unique");
    }
    let mut digest = Sha256::new();
    digest.update(domain);
    for (id, raw) in entries {
        digest.update(
            u32::try_from(id.len())
                .context("raw set commitment ID is too long")?
                .to_be_bytes(),
        );
        digest.update(id.as_bytes());
        digest.update(Sha256::digest(raw));
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}
