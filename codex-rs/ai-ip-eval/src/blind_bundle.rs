use std::collections::BTreeSet;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use rand::SeedableRng;
use rand::TryRngCore;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::blind_bundle_model::BlindPairBundleProjection;
use crate::blind_bundle_model::PreparedBlindBundles;
use crate::blind_bundle_model::PreparedBlindMaterial;
use crate::blind_bundle_model::PreparedReviewerBundle;
use crate::blind_bundle_model::ReviewBundleManifest;
use crate::blind_bundle_model::ReviewerMapping;
use crate::blind_bundle_model::ReviewerQualificationCommitment;
use crate::blind_finalize::VerifiedBlindPair;

const REPLAY_SEED_DOMAIN: &[u8] = b"AI-IP-REPLAY-SEED-V1\0";
const SEED_COMMITMENT_DOMAIN: &[u8] = b"AI-IP-BLIND-SEED-COMMITMENT-V1\0";
const NATIVE_SEED_ATTEMPTS: usize = 32;

pub(crate) fn prepare_replay_blind_bundles(
    pair: &VerifiedBlindPair,
    replay_seeds: &[String],
) -> Result<PreparedBlindBundles> {
    pair.reverify_unchanged()?;
    let projection = pair.bundle_projection()?;
    let prepared = prepare_replay_projection(&projection, replay_seeds)?;
    pair.reverify_unchanged()?;
    Ok(prepared)
}

pub(crate) fn prepare_native_blind_bundles(
    pair: &VerifiedBlindPair,
    rng: &mut impl TryRngCore,
) -> Result<PreparedBlindBundles> {
    pair.reverify_unchanged()?;
    let projection = pair.bundle_projection()?;
    let prepared = prepare_native_projection(&projection, rng)?;
    pair.reverify_unchanged()?;
    Ok(prepared)
}

fn prepare_replay_projection(
    projection: &BlindPairBundleProjection<'_>,
    replay_seeds: &[String],
) -> Result<PreparedBlindBundles> {
    if projection.mode != ExecutionMode::Replay
        || replay_seeds.len() != 3
        || replay_seeds.iter().any(String::is_empty)
        || replay_seeds.iter().collect::<BTreeSet<_>>().len() != 3
    {
        bail!("Replay bundle preparation requires three distinct nonempty seeds");
    }
    let seeds = replay_seeds
        .iter()
        .map(|seed| derive_replay_seed(seed))
        .collect::<Result<Vec<_>>>()?
        .try_into()
        .expect("three replay seeds were validated");
    if !seed_set_is_acceptable(&seeds) {
        bail!("Replay seed set has duplicate bytes or one orientation for all reviewers");
    }
    prepare_projection(projection, seeds)
}

fn prepare_native_projection(
    projection: &BlindPairBundleProjection<'_>,
    rng: &mut impl TryRngCore,
) -> Result<PreparedBlindBundles> {
    if !matches!(projection.mode, ExecutionMode::Mock | ExecutionMode::Live) {
        bail!("Native bundle preparation requires a Mock or Live pair");
    }
    for _attempt in 0..NATIVE_SEED_ATTEMPTS {
        let mut raw = [0_u8; 96];
        rng.try_fill_bytes(&mut raw)
            .map_err(|_| anyhow::anyhow!("generate native blind seed set"))?;
        let seeds = std::array::from_fn(|index| {
            raw[index * 32..(index + 1) * 32]
                .try_into()
                .expect("fixed seed chunk")
        });
        if seed_set_is_acceptable(&seeds) {
            return prepare_projection(projection, seeds);
        }
    }
    bail!("native blind seed resampling exhausted after 32 complete sets")
}

#[cfg(test)]
pub(crate) fn test_prepare_replay_projection(
    projection: &BlindPairBundleProjection<'_>,
    replay_seeds: &[String],
) -> Result<PreparedBlindBundles> {
    prepare_replay_projection(projection, replay_seeds)
}

#[cfg(test)]
pub(crate) fn test_prepare_native_projection(
    projection: &BlindPairBundleProjection<'_>,
    rng: &mut impl TryRngCore,
) -> Result<PreparedBlindBundles> {
    prepare_native_projection(projection, rng)
}

fn prepare_projection(
    projection: &BlindPairBundleProjection<'_>,
    seeds: [[u8; 32]; 3],
) -> Result<PreparedBlindBundles> {
    validate_projection(projection)?;
    let contracts = crate::contracts::FrozenContracts::load()?.blind_review_contracts()?;
    let compact_manifest = serde_json::to_vec(projection.materials_manifest)?;
    if compact_manifest != projection.materials_manifest_bytes {
        bail!("verified material manifest is not the producer-order compact encoding");
    }
    let case_sha256 = sha256(projection.case_bytes);
    let materials_manifest_sha256 = sha256(projection.materials_manifest_bytes);
    verify_visible_sources(projection, &contracts)?;
    let mut prepared_reviewers = Vec::with_capacity(3);
    for ((reviewer, seed), orientation) in projection
        .reviewers
        .iter()
        .zip(seeds)
        .zip(seeds.map(|seed| orientation(&seed)))
    {
        let a_bytes = arm_bytes(projection, orientation[0])?.to_vec();
        let b_bytes = arm_bytes(projection, orientation[1])?.to_vec();
        let qualification = ReviewerQualificationCommitment {
            qualification_class: reviewer.qualification_class.to_string(),
            experienced_operator_or_director: reviewer.experienced_operator_or_director,
            attestation_signed_payload_sha256: reviewer
                .attestation_signed_payload_sha256
                .to_string(),
            attestation_signature_evidence_sha256: reviewer
                .attestation_signature_evidence_sha256
                .to_string(),
        };
        let manifest = ReviewBundleManifest {
            schema_version: 1,
            pair_id: projection.pair_id.to_string(),
            reviewer_id: reviewer.reviewer_id.to_string(),
            qualification: qualification.clone(),
            case_sha256: case_sha256.clone(),
            materials_manifest_sha256: materials_manifest_sha256.clone(),
            source_materials_sha256: sha256(&compact_manifest),
            a_sha256: sha256(&a_bytes),
            b_sha256: sha256(&b_bytes),
            rubric_sha256: contracts.rubric_sha256.clone(),
            reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256.clone(),
        };
        let review_bundle_bytes = canonical(&manifest)?;
        verify_visible_json(
            "review-bundle.json",
            &review_bundle_bytes,
            &projection.forbidden_visible_markers,
        )?;
        let review_bundle_sha256 = sha256(&review_bundle_bytes);
        let seed_commitment = seed_commitment(&seed);
        let mapping = ReviewerMapping {
            schema_version: 1,
            pair_id: projection.pair_id.to_string(),
            reviewer_id: reviewer.reviewer_id.to_string(),
            review_bundle_sha256: review_bundle_sha256.clone(),
            seed_commitment: seed_commitment.clone(),
            a: orientation[0],
            b: orientation[1],
        };
        let mapping_bytes = canonical(&mapping)?;
        let mapping_sha256 = sha256(&mapping_bytes);
        prepared_reviewers.push(PreparedReviewerBundle {
            reviewer_id: reviewer.reviewer_id.to_string(),
            native_seed: (projection.mode != ExecutionMode::Replay).then_some(seed),
            seed_commitment,
            qualification,
            a_bytes,
            b_bytes,
            manifest,
            review_bundle_bytes,
            review_bundle_sha256,
            mapping,
            mapping_bytes,
            mapping_sha256,
        });
    }
    let Ok(prepared_reviewers) = prepared_reviewers.try_into() else {
        bail!("sealed reviewer array changed during preparation");
    };
    Ok(PreparedBlindBundles {
        mode: projection.mode,
        private_root: projection.private_root.to_path_buf(),
        pair_id: projection.pair_id.to_string(),
        frozen_run_context_sha256: projection.frozen_run_context_sha256.to_string(),
        pair_verification_sha256: projection.pair_verification_sha256.to_string(),
        pair_receipt_sha256: projection.pair_receipt_sha256.map(str::to_string),
        decision_policy_sha256: contracts.decision_policy_sha256,
        rubric_sha256: contracts.rubric_sha256,
        reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256,
        case_bytes: projection.case_bytes.to_vec(),
        materials_manifest_bytes: projection.materials_manifest_bytes.to_vec(),
        materials: projection
            .materials
            .iter()
            .map(|material| PreparedBlindMaterial {
                material_id: material.material_id.to_string(),
                relative_path: material.relative_path.to_string(),
                sha256: material.sha256.to_string(),
                bytes: material.bytes.to_vec(),
            })
            .collect(),
        rubric_bytes: contracts.rubric_bytes,
        reviewer_submission_schema_bytes: contracts.reviewer_submission_schema_bytes,
        reviewers: prepared_reviewers,
    })
}

fn validate_projection(projection: &BlindPairBundleProjection<'_>) -> Result<()> {
    if projection
        .reviewers
        .iter()
        .map(|reviewer| reviewer.reviewer_id)
        .collect::<BTreeSet<_>>()
        .len()
        != 3
        || projection.arms[0].condition == projection.arms[1].condition
        || !projection
            .arms
            .iter()
            .any(|arm| arm.condition == EvaluationCondition::Generic)
        || !projection
            .arms
            .iter()
            .any(|arm| arm.condition == EvaluationCondition::Candidate)
    {
        bail!("verified blind projection has invalid reviewer or arm cardinality");
    }
    if projection.materials.len() != projection.materials_manifest.len() {
        bail!("verified blind projection material cardinality changed");
    }
    for (material, declared) in projection
        .materials
        .iter()
        .zip(projection.materials_manifest)
    {
        if material.material_id != declared.material_id
            || material.relative_path != declared.relative_path
            || material.sha256 != declared.sha256
            || sha256(material.bytes) != material.sha256
        {
            bail!("verified blind projection material changed");
        }
    }
    let native_receipt = projection.pair_receipt_sha256.is_some();
    if (projection.mode == ExecutionMode::Replay) == native_receipt {
        bail!("verified blind projection pair receipt differs from execution mode");
    }
    Ok(())
}

fn arm_bytes<'a>(
    projection: &BlindPairBundleProjection<'a>,
    condition: EvaluationCondition,
) -> Result<&'a [u8]> {
    projection
        .arms
        .iter()
        .find(|arm| arm.condition == condition)
        .map(|arm| arm.package_bytes)
        .context("verified blind projection is missing an assigned arm")
}

fn derive_replay_seed(seed: &str) -> Result<[u8; 32]> {
    let byte_len = u64::try_from(seed.len()).context("Replay seed is too long")?;
    let mut hasher = Sha256::new();
    hasher.update(REPLAY_SEED_DOMAIN);
    hasher.update(byte_len.to_be_bytes());
    hasher.update(seed.as_bytes());
    Ok(hasher.finalize().into())
}

fn seed_commitment(seed: &[u8; 32]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SEED_COMMITMENT_DOMAIN);
    hasher.update(seed);
    format!("{:x}", hasher.finalize())
}

fn orientation(seed: &[u8; 32]) -> [EvaluationCondition; 2] {
    let mut order = [EvaluationCondition::Generic, EvaluationCondition::Candidate];
    order.shuffle(&mut StdRng::from_seed(*seed));
    order
}

fn seed_set_is_acceptable(seeds: &[[u8; 32]; 3]) -> bool {
    let unique = seeds.iter().collect::<BTreeSet<_>>().len() == 3;
    let commitments = seeds.map(|seed| seed_commitment(&seed));
    let unique_commitments = commitments.iter().collect::<BTreeSet<_>>().len() == 3;
    let orientations = seeds.map(|seed| orientation(&seed));
    unique
        && unique_commitments
        && !(orientations[0] == orientations[1] && orientations[1] == orientations[2])
}

fn verify_visible_sources(
    projection: &BlindPairBundleProjection<'_>,
    contracts: &crate::blind_bundle_model::BlindReviewContracts,
) -> Result<()> {
    if projection.forbidden_visible_markers.is_empty() {
        bail!("verified blind projection has no visibility marker set");
    }
    for (label, bytes) in [
        ("case.json", projection.case_bytes),
        (
            "materials-manifest.json",
            projection.materials_manifest_bytes,
        ),
        ("A/B package 1", projection.arms[0].package_bytes),
        ("A/B package 2", projection.arms[1].package_bytes),
        ("rubric.json", contracts.rubric_bytes),
        (
            "reviewer-submission.schema.json",
            contracts.reviewer_submission_schema_bytes,
        ),
    ] {
        verify_visible_json(label, bytes, &projection.forbidden_visible_markers)?;
    }
    for reviewer in &projection.reviewers {
        for (label, value) in [
            ("reviewerId", reviewer.reviewer_id),
            ("qualificationClass", reviewer.qualification_class),
        ] {
            verify_visible_bytes(
                label,
                value.as_bytes(),
                &projection.forbidden_visible_markers,
            )?;
        }
    }
    for material in &projection.materials {
        verify_visible_bytes(
            "material relative path",
            material.relative_path.as_bytes(),
            &projection.forbidden_visible_markers,
        )?;
        verify_visible_document(
            "material bytes",
            material.bytes,
            &projection.forbidden_visible_markers,
        )?;
    }
    Ok(())
}

fn verify_visible_bytes(label: &str, bytes: &[u8], markers: &[String]) -> Result<()> {
    let normalized = String::from_utf8_lossy(bytes).to_lowercase();
    if markers
        .iter()
        .any(|marker| normalized.contains(&marker.to_lowercase()))
    {
        bail!("reviewer-visible {label} contains a forbidden treatment marker");
    }
    Ok(())
}

fn verify_visible_json(label: &str, bytes: &[u8], markers: &[String]) -> Result<()> {
    let value = crate::jcs::parse_json(bytes).context("parse reviewer-visible JSON")?;
    verify_visible_value(label, &value, markers)
}

fn verify_visible_document(label: &str, bytes: &[u8], markers: &[String]) -> Result<()> {
    match crate::jcs::parse_json(bytes) {
        Ok(value) => verify_visible_value(label, &value, markers),
        Err(_) => verify_visible_bytes(label, bytes, markers),
    }
}

fn verify_visible_value(label: &str, value: &serde_json::Value, markers: &[String]) -> Result<()> {
    match value {
        serde_json::Value::String(text) => verify_visible_bytes(label, text.as_bytes(), markers),
        serde_json::Value::Array(values) => {
            for value in values {
                verify_visible_value(label, value, markers)?;
            }
            Ok(())
        }
        serde_json::Value::Object(values) => {
            for (key, value) in values {
                verify_visible_bytes(label, key.as_bytes(), markers)?;
                verify_visible_value(label, value, markers)?;
            }
            Ok(())
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            Ok(())
        }
    }
}

fn canonical(value: &impl Serialize) -> Result<Vec<u8>> {
    Ok(crate::jcs::canonicalize_value(&serde_json::to_value(
        value,
    )?)?)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
