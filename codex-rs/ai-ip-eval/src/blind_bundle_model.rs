use serde::Serialize;

use crate::EvaluationCondition;
use crate::ExecutionMode;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewerQualificationCommitment {
    pub(crate) qualification_class: String,
    pub(crate) experienced_operator_or_director: bool,
    pub(crate) attestation_signed_payload_sha256: String,
    pub(crate) attestation_signature_evidence_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewBundleManifest {
    pub(crate) schema_version: u32,
    pub(crate) pair_id: String,
    pub(crate) reviewer_id: String,
    pub(crate) qualification: ReviewerQualificationCommitment,
    pub(crate) case_sha256: String,
    pub(crate) materials_manifest_sha256: String,
    pub(crate) source_materials_sha256: String,
    pub(crate) a_sha256: String,
    pub(crate) b_sha256: String,
    pub(crate) rubric_sha256: String,
    pub(crate) reviewer_submission_schema_sha256: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewerMapping {
    pub(crate) schema_version: u32,
    pub(crate) pair_id: String,
    pub(crate) reviewer_id: String,
    pub(crate) review_bundle_sha256: String,
    pub(crate) seed_commitment: String,
    pub(crate) a: EvaluationCondition,
    pub(crate) b: EvaluationCondition,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlindArmProjection<'a> {
    pub(crate) condition: EvaluationCondition,
    pub(crate) package_bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlindMaterialProjection<'a> {
    pub(crate) material_id: &'a str,
    pub(crate) relative_path: &'a str,
    pub(crate) sha256: &'a str,
    pub(crate) bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct BlindReviewerProjection<'a> {
    pub(crate) reviewer_id: &'a str,
    pub(crate) qualification_class: &'a str,
    pub(crate) experienced_operator_or_director: bool,
    pub(crate) attestation_signed_payload_sha256: &'a str,
    pub(crate) attestation_signature_evidence_sha256: &'a str,
}

#[derive(Debug)]
pub(crate) struct BlindPairBundleProjection<'a> {
    pub(crate) mode: ExecutionMode,
    pub(crate) private_root: &'a std::path::Path,
    pub(crate) pair_id: &'a str,
    pub(crate) frozen_run_context_sha256: &'a str,
    pub(crate) pair_verification_sha256: &'a str,
    pub(crate) pair_receipt_sha256: Option<&'a str>,
    pub(crate) reviewers: [BlindReviewerProjection<'a>; 3],
    pub(crate) case_bytes: &'a [u8],
    pub(crate) materials_manifest: &'a [codex_ai_ip_domain::MissionMaterial],
    pub(crate) materials_manifest_bytes: &'a [u8],
    pub(crate) materials: Vec<BlindMaterialProjection<'a>>,
    pub(crate) arms: [BlindArmProjection<'a>; 2],
    pub(crate) forbidden_visible_markers: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct BlindReviewContracts {
    pub(crate) rubric_bytes: &'static [u8],
    pub(crate) rubric_sha256: String,
    pub(crate) decision_policy_sha256: String,
    pub(crate) reviewer_submission_schema_bytes: &'static [u8],
    pub(crate) reviewer_submission_schema_sha256: String,
}

#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) struct PreparedBlindMaterial {
    pub(crate) material_id: String,
    pub(crate) relative_path: String,
    pub(crate) sha256: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) struct PreparedReviewerBundle {
    pub(crate) reviewer_id: String,
    pub(crate) native_seed: Option<[u8; 32]>,
    pub(crate) seed_commitment: String,
    pub(crate) qualification: ReviewerQualificationCommitment,
    pub(crate) a_bytes: Vec<u8>,
    pub(crate) b_bytes: Vec<u8>,
    pub(crate) manifest: ReviewBundleManifest,
    pub(crate) review_bundle_bytes: Vec<u8>,
    pub(crate) review_bundle_sha256: String,
    pub(crate) mapping: ReviewerMapping,
    pub(crate) mapping_bytes: Vec<u8>,
    pub(crate) mapping_sha256: String,
}

#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub(crate) struct PreparedBlindBundles {
    pub(crate) mode: ExecutionMode,
    pub(crate) private_root: std::path::PathBuf,
    pub(crate) pair_id: String,
    pub(crate) frozen_run_context_sha256: String,
    pub(crate) pair_verification_sha256: String,
    pub(crate) pair_receipt_sha256: Option<String>,
    pub(crate) decision_policy_sha256: String,
    pub(crate) rubric_sha256: String,
    pub(crate) reviewer_submission_schema_sha256: String,
    pub(crate) case_bytes: Vec<u8>,
    pub(crate) materials_manifest_bytes: Vec<u8>,
    pub(crate) materials: Vec<PreparedBlindMaterial>,
    pub(crate) rubric_bytes: &'static [u8],
    pub(crate) reviewer_submission_schema_bytes: &'static [u8],
    pub(crate) reviewers: [PreparedReviewerBundle; 3],
}
