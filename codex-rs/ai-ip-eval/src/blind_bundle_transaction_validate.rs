use std::collections::BTreeSet;

use anyhow::Result;
use anyhow::bail;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::blind_bundle_model::BlindPairBundleProjection;
use crate::blind_bundle_model::PreparedBlindBundles;
use crate::blind_bundle_model::PreparedReviewerBundle;
use crate::blind_bundle_model::ReviewerQualificationCommitment;
use crate::blind_finalize::VerifiedBlindPair;

pub(crate) fn validate_prepared_binding(
    pair: &VerifiedBlindPair,
    prepared: &PreparedBlindBundles,
) -> Result<()> {
    let projection = pair.bundle_projection()?;
    let contracts = crate::contracts::FrozenContracts::load()?.blind_review_contracts()?;
    if projection.mode != prepared.mode
        || projection.private_root != prepared.private_root
        || projection.pair_id != prepared.pair_id
        || projection.frozen_run_context_sha256 != prepared.frozen_run_context_sha256
        || projection.pair_verification_sha256 != prepared.pair_verification_sha256
        || projection.pair_receipt_sha256 != prepared.pair_receipt_sha256.as_deref()
        || projection.case_bytes != prepared.case_bytes
        || projection.materials_manifest_bytes != prepared.materials_manifest_bytes
        || contracts.rubric_bytes != prepared.rubric_bytes
        || contracts.reviewer_submission_schema_bytes != prepared.reviewer_submission_schema_bytes
        || contracts.rubric_sha256 != prepared.rubric_sha256
        || contracts.decision_policy_sha256 != prepared.decision_policy_sha256
        || contracts.reviewer_submission_schema_sha256
            != prepared.reviewer_submission_schema_sha256
    {
        bail!("prepared blind bundles differ from the verified pair binding");
    }
    validate_materials(&projection, prepared)?;
    validate_reviewers(&projection, prepared)
}

fn validate_materials(
    projection: &BlindPairBundleProjection<'_>,
    prepared: &PreparedBlindBundles,
) -> Result<()> {
    if projection.materials.len() != prepared.materials.len() {
        bail!("prepared blind materials differ from the verified pair binding");
    }
    for (source, material) in projection.materials.iter().zip(&prepared.materials) {
        if source.material_id != material.material_id
            || source.relative_path != material.relative_path
            || source.sha256 != material.sha256
            || source.bytes != material.bytes
            || material.sha256 != sha256(&material.bytes)
        {
            bail!("prepared blind materials differ from the verified pair binding");
        }
    }
    Ok(())
}

fn validate_reviewers(
    projection: &BlindPairBundleProjection<'_>,
    prepared: &PreparedBlindBundles,
) -> Result<()> {
    let replay = prepared.mode == ExecutionMode::Replay;
    if replay != prepared.pair_receipt_sha256.is_none()
        || prepared
            .reviewers
            .iter()
            .any(|reviewer| replay != reviewer.native_seed.is_none())
    {
        bail!("prepared blind bundle mode and private seed shape differ");
    }
    let uniqueness = [
        prepared
            .reviewers
            .iter()
            .map(|reviewer| reviewer.reviewer_id.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        prepared
            .reviewers
            .iter()
            .map(|reviewer| reviewer.review_bundle_sha256.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        prepared
            .reviewers
            .iter()
            .map(|reviewer| reviewer.mapping_sha256.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        prepared
            .reviewers
            .iter()
            .map(|reviewer| reviewer.seed_commitment.as_str())
            .collect::<BTreeSet<_>>()
            .len(),
    ];
    if uniqueness != [3, 3, 3, 3] {
        bail!("prepared blind bundle commitments are not unique");
    }
    let orientations = prepared.reviewers.each_ref().map(|reviewer| {
        [reviewer.mapping.a, reviewer.mapping.b]
    });
    if orientations[0] == orientations[1] && orientations[1] == orientations[2] {
        bail!("prepared blind mappings have one orientation for all reviewers");
    }
    if !replay
        && prepared
            .reviewers
            .iter()
            .map(|reviewer| reviewer.native_seed.as_ref().expect("Native shape was checked"))
            .collect::<BTreeSet<_>>()
            .len()
            != 3
    {
        bail!("prepared Native seed bytes are not unique");
    }
    let source_materials_sha256 = sha256(&serde_json::to_vec(projection.materials_manifest)?);
    for (source, reviewer) in projection.reviewers.iter().zip(&prepared.reviewers) {
        let qualification = ReviewerQualificationCommitment {
            qualification_class: source.qualification_class.to_string(),
            experienced_operator_or_director: source.experienced_operator_or_director,
            attestation_signed_payload_sha256: source
                .attestation_signed_payload_sha256
                .to_string(),
            attestation_signature_evidence_sha256: source
                .attestation_signature_evidence_sha256
                .to_string(),
        };
        if source.reviewer_id != reviewer.reviewer_id || reviewer.qualification != qualification {
            bail!("prepared blind reviewer qualification differs from the sealed declaration");
        }
        validate_reviewer_wire(projection, prepared, reviewer, &source_materials_sha256)?;
    }
    Ok(())
}

fn validate_reviewer_wire(
    projection: &BlindPairBundleProjection<'_>,
    prepared: &PreparedBlindBundles,
    reviewer: &PreparedReviewerBundle,
    source_materials_sha256: &str,
) -> Result<()> {
    let mapping = &reviewer.mapping;
    let manifest = &reviewer.manifest;
    let a_bytes = arm_bytes(projection, mapping.a)?;
    let b_bytes = arm_bytes(projection, mapping.b)?;
    if mapping.schema_version != 1
        || mapping.pair_id != prepared.pair_id
        || mapping.reviewer_id != reviewer.reviewer_id
        || mapping.a == mapping.b
        || reviewer.a_bytes != a_bytes
        || reviewer.b_bytes != b_bytes
        || mapping.review_bundle_sha256 != reviewer.review_bundle_sha256
        || mapping.seed_commitment != reviewer.seed_commitment
        || manifest.schema_version != 1
        || manifest.pair_id != prepared.pair_id
        || manifest.reviewer_id != reviewer.reviewer_id
        || manifest.qualification != reviewer.qualification
        || manifest.case_sha256 != sha256(&prepared.case_bytes)
        || manifest.materials_manifest_sha256 != sha256(&prepared.materials_manifest_bytes)
        || manifest.source_materials_sha256 != source_materials_sha256
        || manifest.a_sha256 != sha256(a_bytes)
        || manifest.b_sha256 != sha256(b_bytes)
        || manifest.rubric_sha256 != prepared.rubric_sha256
        || manifest.reviewer_submission_schema_sha256
            != prepared.reviewer_submission_schema_sha256
        || canonical(manifest)? != reviewer.review_bundle_bytes
        || reviewer.review_bundle_sha256 != sha256(&reviewer.review_bundle_bytes)
        || canonical(mapping)? != reviewer.mapping_bytes
        || reviewer.mapping_sha256 != sha256(&reviewer.mapping_bytes)
    {
        bail!("prepared blind reviewer bundle is not bound to its exact sources");
    }
    if let Some(seed) = reviewer.native_seed {
        if crate::blind_bundle::seed_commitment(&seed) != reviewer.seed_commitment
            || crate::blind_bundle::orientation(&seed) != [mapping.a, mapping.b]
        {
            bail!("prepared Native seed differs from its mapping commitment");
        }
    }
    Ok(())
}

fn arm_bytes<'a>(
    projection: &'a BlindPairBundleProjection<'_>,
    condition: EvaluationCondition,
) -> Result<&'a [u8]> {
    projection
        .arms
        .iter()
        .find(|arm| arm.condition == condition)
        .map(|arm| arm.package_bytes)
        .ok_or_else(|| anyhow::anyhow!("prepared mapping references an absent blind condition"))
}

fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    Ok(crate::jcs::canonicalize_value(&serde_json::to_value(value)?)?)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
