use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeSet;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::SkillUseOutcome;
use crate::blind::FrozenInputToken;
use crate::blind_bundle_model::BlindArmProjection;
use crate::blind_bundle_model::BlindMaterialProjection;
use crate::blind_bundle_model::BlindPairBundleProjection;
use crate::blind_bundle_model::BlindReviewerProjection;
use crate::blind_verify::PairEvidenceCore;

const TREATMENT_MARKERS: &[u8] = include_bytes!("../tests/fixtures/blind/treatment-markers.json");

#[derive(Debug)]
pub(crate) struct VerifiedBlindPair {
    core: PairEvidenceCore,
}

pub(crate) struct BlindBundleTransactionCursor {
    inventory_root_sha256: String,
    retained_private_root: crate::secure_fs_retain::RetainedPrivateRoot,
}

impl BlindBundleTransactionCursor {
    pub(crate) fn inventory_root_sha256(&self) -> &str {
        &self.inventory_root_sha256
    }

    pub(crate) fn reverify_root_unchanged(&self) -> Result<()> {
        self.retained_private_root.reverify_unchanged()
    }
}

impl VerifiedBlindPair {
    pub(crate) fn bundle_projection(&self) -> Result<BlindPairBundleProjection<'_>> {
        let forbidden_visible_markers = treatment_markers_for(&self.core)?
            .into_iter()
            .filter(|marker| !matches!(marker.as_str(), "candidate" | "generic"))
            .collect();
        Ok(BlindPairBundleProjection {
            mode: self.core.mode,
            private_root: &self.core.private_root,
            pair_id: &self.core.pair_id,
            frozen_run_context_sha256: &self.core.frozen_run_context_sha256,
            pair_verification_sha256: &self.core.pair_verification_raw_sha256,
            pair_receipt_sha256: self.core.native_pair_receipt_raw_sha256.as_deref(),
            reviewers: self
                .core
                .reviewers
                .each_ref()
                .map(|reviewer| BlindReviewerProjection {
                    reviewer_id: &reviewer.reviewer_id,
                    qualification_class: &reviewer.qualification_class,
                    experienced_operator_or_director: reviewer.experienced_operator_or_director,
                    attestation_signed_payload_sha256: &reviewer.signed_payload_sha256,
                    attestation_signature_evidence_sha256: &reviewer.signature_evidence_sha256,
                }),
            case_bytes: &self.core.case_bytes,
            materials_manifest: &self.core.materials_manifest,
            materials_manifest_bytes: &self.core.materials_manifest_bytes,
            materials: self
                .core
                .materials
                .iter()
                .map(|material| BlindMaterialProjection {
                    material_id: &material.material_id,
                    relative_path: &material.relative_path,
                    sha256: &material.sha256,
                    bytes: &material.bytes,
                })
                .collect(),
            arms: self.core.arms.each_ref().map(|arm| BlindArmProjection {
                condition: arm.condition,
                package_bytes: &arm.content_package_bytes,
            }),
            forbidden_visible_markers,
        })
    }

    pub(crate) fn reverify_unchanged(&self) -> Result<()> {
        reverify_core(&self.core)
    }

    pub(crate) fn begin_bundle_transaction(&self) -> Result<BlindBundleTransactionCursor> {
        let retained_private_root =
            crate::secure_fs_retain::RetainedPrivateRoot::retain(&self.core.private_root)?;
        reverify_core(&self.core)?;
        retained_private_root.reverify_unchanged()?;
        Ok(BlindBundleTransactionCursor {
            inventory_root_sha256: self.core.inventory.inventory_root_sha256().to_string(),
            retained_private_root,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TreatmentMarkerTable {
    schema_version: u32,
    markers: Vec<String>,
}

pub(crate) fn finalize_blind_pair(core: PairEvidenceCore) -> Result<VerifiedBlindPair> {
    reverify_core(&core)?;
    Ok(VerifiedBlindPair { core })
}

fn reverify_core(core: &PairEvidenceCore) -> Result<()> {
    verify_treatment_and_skill(core)?;
    match &core.inputs {
        FrozenInputToken::Replay(inputs) => inputs.reverify_all()?,
        FrozenInputToken::Native { frozen, content } => {
            frozen.reverify_all()?;
            content.reverify_unchanged()?;
        }
    }
    core.inventory.reverify_unchanged()?;
    crate::blind::ensure_destinations_absent(&core.private_root)?;
    Ok(())
}

pub(crate) fn verify_treatment_and_skill(core: &PairEvidenceCore) -> Result<()> {
    let markers = treatment_markers_for(core)?;
    for arm in &core.arms {
        verify_decoded_package_strings(&arm.content_package, &markers)?;
    }
    let skill_bytes = match &core.inputs {
        FrozenInputToken::Replay(inputs) => inputs.fixture_bytes("leadSkill")?,
        FrozenInputToken::Native { content, .. } => &content.projection().skill_bytes,
    };
    let expected_candidate = SkillUseOutcome {
        successful_read_observed: true,
        evidence_sha256: Some(complete_skill_read_commitment(skill_bytes)?),
    };
    let expected_generic = SkillUseOutcome {
        successful_read_observed: false,
        evidence_sha256: None,
    };
    for arm in &core.arms {
        let expected = match arm.condition {
            EvaluationCondition::Generic => &expected_generic,
            EvaluationCondition::Candidate => &expected_candidate,
        };
        if &arm.skill_use != expected {
            bail!("sealed Skill-use evidence does not bind the canonical complete Skill");
        }
    }
    Ok(())
}

pub(crate) fn embedded_treatment_markers() -> Result<Vec<String>> {
    let table: TreatmentMarkerTable =
        serde_json::from_value(crate::jcs::parse_json(TREATMENT_MARKERS)?)?;
    let normalized = table
        .markers
        .iter()
        .map(|marker| marker.to_lowercase())
        .collect::<BTreeSet<_>>();
    if table.schema_version != 1
        || table.markers.is_empty()
        || table.markers.len() > 32
        || table.markers.iter().any(String::is_empty)
        || normalized.len() != table.markers.len()
    {
        bail!("embedded treatment marker table is invalid");
    }
    Ok(table.markers)
}

pub(crate) fn verify_decoded_package_strings(
    package: &codex_ai_ip_domain::ContentPackage,
    markers: &[String],
) -> Result<()> {
    let normalized = markers
        .iter()
        .map(|marker| marker.to_lowercase())
        .collect::<Vec<_>>();
    if normalized.iter().any(String::is_empty)
        || value_contains_marker(&serde_json::to_value(package)?, &normalized)
    {
        bail!("sealed package contains a forbidden treatment marker");
    }
    Ok(())
}

fn treatment_markers_for(core: &PairEvidenceCore) -> Result<Vec<String>> {
    let mut markers = embedded_treatment_markers()?;
    let (generic_home, candidate_home) = match core.mode {
        ExecutionMode::Replay => (
            core.private_root.join("replay-generic-home"),
            core.private_root.join("replay-candidate-home"),
        ),
        ExecutionMode::Mock | ExecutionMode::Live => (
            core.private_root.join("generic-home"),
            core.private_root.join("candidate-home"),
        ),
    };
    let candidate_codex_home = candidate_home.join(".codex");
    let canonical_skill_path = candidate_codex_home
        .join("skills")
        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
        .join("SKILL.md");
    for marker in [
        "candidate".to_string(),
        "generic".to_string(),
        codex_ai_ip_runtime::LEAD_SKILL_NAME.to_string(),
        path_marker(&core.private_root)?,
        path_marker(&generic_home)?,
        path_marker(&generic_home.join(".codex"))?,
        path_marker(&candidate_home)?,
        path_marker(&candidate_codex_home)?,
        path_marker(&canonical_skill_path)?,
    ] {
        markers.push(marker);
    }
    Ok(markers)
}

fn path_marker(path: &std::path::Path) -> Result<String> {
    Ok(path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("blind marker path is not UTF-8"))?
        .to_string())
}

fn complete_skill_read_commitment(skill_bytes: &[u8]) -> Result<String> {
    let evidence = serde_json::json!({
        "schemaVersion": 1,
        "skillSha256": format!("{:x}", Sha256::digest(skill_bytes)),
        "readBeforeFinal": true
    });
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&evidence)?)
    ))
}

fn value_contains_marker(value: &Value, markers: &[String]) -> bool {
    match value {
        Value::String(text) => {
            let normalized = text.to_lowercase();
            markers.iter().any(|marker| normalized.contains(marker))
        }
        Value::Array(values) => values
            .iter()
            .any(|value| value_contains_marker(value, markers)),
        Value::Object(values) => values
            .values()
            .any(|value| value_contains_marker(value, markers)),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}
