use std::collections::HashSet;
use std::fmt;

use chrono::DateTime;
use jsonschema::Validator;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

use crate::jcs::commitment;
use crate::jcs::parse_json;

const REVIEW_RUBRIC_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/content-package-blind-review.json");
const DECISION_POLICY_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/blind-review-decision-policy.json");
const REVIEWER_SUBMISSION_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/rubrics/reviewer-submission.schema.json");
const HELD_OUT_ATTESTATION_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/held-out-attestation.schema.json");
const FROZEN_RUN_CONTEXT_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../ai-ip-evals/schemas/frozen-run-context.schema.json");

const REVIEWER_QUALIFICATION_DOMAIN: &[u8] = b"AI-IP-REVIEWER-QUALIFICATION-V1\0";
const REVIEW_SUBMISSION_DOMAIN: &[u8] = b"AI-IP-REVIEW-SUBMISSION-V1\0";
const REVIEW_RUBRIC_JCS_SHA256: &str =
    "63614f7ceaaebd785cc00c552a3c5156d3ecc70634958c87e2814f195591e56d";

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ContractError {
    Invalid(String),
}

impl fmt::Display for ContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ContractError {}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewerSubmission {
    pub schema_version: u32,
    pub reviewer_id: String,
    pub review_bundle_sha256: String,
    pub rubric_sha256: String,
    pub qualification: ReviewerQualificationBinding,
    pub preferred: PreferredArm,
    pub arms: ReviewerArms,
    pub signed_at: String,
    pub signed_payload_sha256: String,
    pub signature_evidence: String,
    pub signature_evidence_sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewerQualificationBinding {
    pub qualification_class: String,
    pub experienced_operator_or_director: bool,
    pub attestation_signed_payload_sha256: String,
    pub attestation_signature_evidence_sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
pub enum PreferredArm {
    A,
    B,
    #[serde(rename = "tie")]
    Tie,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewerArms {
    #[serde(rename = "A")]
    pub a: ArmReview,
    #[serde(rename = "B")]
    pub b: ArmReview,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ArmReview {
    pub scores: DimensionScores,
    pub ready_for_human_review: bool,
    pub reasons: Vec<String>,
    pub severe_flags: SevereFlags,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct DimensionScores {
    pub business_outcome_clarity: u8,
    pub subject_audience_action_fit: u8,
    pub strategic_judgment: u8,
    pub publishable_usability: u8,
    pub evidence_integrity: u8,
    pub measurement_usefulness: u8,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SevereFlags {
    pub fabricated_factual_claim: bool,
    pub wrong_subject_or_desired_action: bool,
    pub not_actually_usable: bool,
    pub rights_or_privacy_violation: bool,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewerDeclaration {
    pub reviewer_id: String,
    pub qualification_class: String,
    pub experienced_operator_or_director: bool,
    pub declared_at: String,
    pub signed_payload_sha256: String,
    pub signature_evidence: String,
    pub signature_evidence_sha256: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NativeHeldOutAttestation {
    pub schema_version: u32,
    pub execution_mode: String,
    pub provider_mode: String,
    pub candidate_sha: String,
    pub candidate_frozen_at: String,
    pub case_selected_at: String,
    pub case_sha256: String,
    pub source_materials_sha256: String,
    pub private_root: String,
    pub case_class: String,
    pub not_one_of_five_frozen_classes: bool,
    pub material_authorization_scope: Vec<String>,
    pub provider_disclosure: String,
    pub provider_role: String,
    pub target_provider_model_evidence_commitment: String,
    pub reviewers: Vec<ReviewerDeclaration>,
    pub approval_id: String,
    pub approved_total_fen: u64,
    pub approved_per_run_fen: u64,
    pub max_provider_request_attempts_per_run: u64,
    pub max_total_tokens_per_run: u64,
    pub max_elapsed_seconds_per_run: u64,
    pub max_output_tokens_per_request: u64,
    pub signed_at: String,
    pub retention_deadline: String,
    pub provider_budget_evidence_sha256: String,
    pub rate_card_sha256: String,
    pub billing_policy_commitment: String,
    pub fx_policy_sha256: String,
    pub rate_currency: String,
    pub rate_effective_at: String,
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReplayReviewAttestation {
    pub schema_version: u32,
    pub execution_mode: String,
    pub provider_mode: String,
    pub paid_provider_cost_fen: u64,
    pub synthetic_only: bool,
    pub pair_id: String,
    pub case_sha256: String,
    pub source_materials_sha256: String,
    pub reviewers: Vec<ReviewerDeclaration>,
    pub signed_at: String,
}

pub struct FrozenContracts {
    reviewer_submission: Validator,
    held_out_attestation: Validator,
    frozen_run_context: Validator,
}

impl FrozenContracts {
    pub fn load() -> Result<Self, ContractError> {
        validate_reviewer_rubric(REVIEW_RUBRIC_BYTES)?;
        let policy = parse_json(DECISION_POLICY_BYTES).map_err(invalid)?;
        if policy
            != serde_json::json!({
                "schemaVersion": 1,
                "candidatePreferenceCount": 2,
                "medianPairedDelta": 3,
                "medianCandidateTotal": 18,
                "candidateReadyForHumanReviewCount": 2,
                "candidateSevereFailureCount": 0
            })
        {
            return Err(ContractError::Invalid(
                "embedded decision policy changed".to_string(),
            ));
        }
        Ok(Self {
            reviewer_submission: compile_schema(REVIEWER_SUBMISSION_SCHEMA_BYTES)?,
            held_out_attestation: compile_schema(HELD_OUT_ATTESTATION_SCHEMA_BYTES)?,
            frozen_run_context: compile_schema(FROZEN_RUN_CONTEXT_SCHEMA_BYTES)?,
        })
    }

    pub fn reviewer_rubric_bytes(&self) -> &'static [u8] {
        REVIEW_RUBRIC_BYTES
    }

    pub fn reviewer_submission_schema_bytes(&self) -> &'static [u8] {
        REVIEWER_SUBMISSION_SCHEMA_BYTES
    }

    pub fn validate_reviewer_submission(
        &self,
        raw: &[u8],
    ) -> Result<ReviewerSubmission, ContractError> {
        let value = validate_instance(&self.reviewer_submission, raw)?;
        let submission: ReviewerSubmission = deserialize_value(value.clone())?;
        parse_timestamp(&submission.signed_at)?;
        validate_signature_evidence(
            &submission.signature_evidence,
            &submission.signature_evidence_sha256,
        )?;
        validate_signed_payload(
            value,
            REVIEW_SUBMISSION_DOMAIN,
            &submission.signed_payload_sha256,
        )?;
        Ok(submission)
    }

    pub fn validate_native_attestation(
        &self,
        raw: &[u8],
    ) -> Result<NativeHeldOutAttestation, ContractError> {
        let value = validate_instance(&self.held_out_attestation, raw)?;
        if value["executionMode"] != "live" {
            return Err(ContractError::Invalid(
                "attestation is not the native branch".to_string(),
            ));
        }
        let attestation: NativeHeldOutAttestation = deserialize_value(value.clone())?;
        validate_reviewer_declarations(&value, &attestation.reviewers)?;
        let candidate_frozen_at = parse_timestamp(&attestation.candidate_frozen_at)?;
        let case_selected_at = parse_timestamp(&attestation.case_selected_at)?;
        let signed_at = parse_timestamp(&attestation.signed_at)?;
        let retention_deadline = parse_timestamp(&attestation.retention_deadline)?;
        let rate_effective_at = parse_timestamp(&attestation.rate_effective_at)?;
        if candidate_frozen_at >= case_selected_at
            || case_selected_at > signed_at
            || signed_at >= retention_deadline
            || rate_effective_at > signed_at
            || attestation.reviewers.iter().any(|reviewer| {
                parse_timestamp(&reviewer.declared_at)
                    .map(|declared_at| declared_at > signed_at)
                    .unwrap_or(true)
            })
            || attestation.approved_per_run_fen > attestation.approved_total_fen
        {
            return Err(ContractError::Invalid(
                "native attestation ordering or budget is invalid".to_string(),
            ));
        }
        Ok(attestation)
    }

    pub fn validate_replay_attestation(
        &self,
        raw: &[u8],
    ) -> Result<ReplayReviewAttestation, ContractError> {
        let value = validate_instance(&self.held_out_attestation, raw)?;
        if value["executionMode"] != "replay" {
            return Err(ContractError::Invalid(
                "attestation is not the replay branch".to_string(),
            ));
        }
        let attestation: ReplayReviewAttestation = deserialize_value(value.clone())?;
        let signed_at = parse_timestamp(&attestation.signed_at)?;
        validate_reviewer_declarations(&value, &attestation.reviewers)?;
        if attestation.reviewers.iter().any(|reviewer| {
            parse_timestamp(&reviewer.declared_at)
                .map(|declared_at| declared_at > signed_at)
                .unwrap_or(true)
        }) {
            return Err(ContractError::Invalid(
                "reviewer declaration follows attestation signature".to_string(),
            ));
        }
        Ok(attestation)
    }

    pub fn validate_native_context(&self, raw: &[u8]) -> Result<(), ContractError> {
        let value = validate_instance(&self.frozen_run_context, raw)?;
        if value["executionMode"] != "live" {
            return Err(ContractError::Invalid(
                "frozen context is not the native branch".to_string(),
            ));
        }
        Ok(())
    }

    pub fn validate_replay_context(&self, raw: &[u8]) -> Result<(), ContractError> {
        let value = validate_instance(&self.frozen_run_context, raw)?;
        if value["executionMode"] != "replay" {
            return Err(ContractError::Invalid(
                "frozen context is not the replay branch".to_string(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_reviewer_rubric(raw: &[u8]) -> Result<(), ContractError> {
    let rubric = parse_json(raw).map_err(invalid)?;
    let keys = rubric
        .as_object()
        .map(|object| object.keys().map(String::as_str).collect::<HashSet<_>>());
    let forbidden = String::from_utf8_lossy(raw).to_ascii_lowercase();
    let frozen = commitment(b"", raw).map_err(invalid)?;
    if keys
        != Some(HashSet::from([
            "schemaVersion",
            "scale",
            "dimensions",
            "severeFlags",
            "reviewerInstructions",
        ]))
        || ["candidate", "generic", "threshold", "pass"]
            .iter()
            .any(|term| forbidden.contains(term))
        || frozen.sha256 != REVIEW_RUBRIC_JCS_SHA256
    {
        return Err(ContractError::Invalid(
            "reviewer-visible rubric changed".to_string(),
        ));
    }
    Ok(())
}

pub fn validate_reviewer_submission(raw: &[u8]) -> Result<ReviewerSubmission, ContractError> {
    FrozenContracts::load()?.validate_reviewer_submission(raw)
}

fn compile_schema(raw: &[u8]) -> Result<Validator, ContractError> {
    let schema = parse_json(raw).map_err(invalid)?;
    reject_external_references(&schema)?;
    jsonschema::draft202012::meta::validate(&schema).map_err(|error| {
        ContractError::Invalid(format!("invalid Draft 2020-12 schema: {error}"))
    })?;
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|error| ContractError::Invalid(format!("compile offline schema: {error}")))
}

fn reject_external_references(value: &Value) -> Result<(), ContractError> {
    match value {
        Value::Array(values) => {
            for value in values {
                reject_external_references(value)?;
            }
        }
        Value::Object(values) => {
            if let Some(reference) = values.get("$ref").and_then(Value::as_str)
                && !reference.starts_with('#')
            {
                return Err(ContractError::Invalid(
                    "external schema references are forbidden".to_string(),
                ));
            }
            for value in values.values() {
                reject_external_references(value)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

fn validate_instance(validator: &Validator, raw: &[u8]) -> Result<Value, ContractError> {
    let value = parse_json(raw).map_err(invalid)?;
    validator
        .validate(&value)
        .map_err(|error| ContractError::Invalid(format!("schema validation failed: {error}")))?;
    Ok(value)
}

fn deserialize_value<T: DeserializeOwned>(value: Value) -> Result<T, ContractError> {
    serde_json::from_value(value)
        .map_err(|error| ContractError::Invalid(format!("typed contract parse failed: {error}")))
}

fn validate_reviewer_declarations(
    attestation: &Value,
    reviewers: &[ReviewerDeclaration],
) -> Result<(), ContractError> {
    let reviewer_values = attestation["reviewers"]
        .as_array()
        .ok_or_else(|| ContractError::Invalid("reviewers must be an array".to_string()))?;
    let mut reviewer_ids = HashSet::with_capacity(reviewers.len());
    let mut experienced_count = 0;
    for (reviewer, value) in reviewers.iter().zip(reviewer_values) {
        if !reviewer_ids.insert(&reviewer.reviewer_id) {
            return Err(ContractError::Invalid(
                "reviewer IDs must be unique".to_string(),
            ));
        }
        experienced_count += usize::from(reviewer.experienced_operator_or_director);
        parse_timestamp(&reviewer.declared_at)?;
        validate_signature_evidence(
            &reviewer.signature_evidence,
            &reviewer.signature_evidence_sha256,
        )?;
        validate_signed_payload(
            value.clone(),
            REVIEWER_QUALIFICATION_DOMAIN,
            &reviewer.signed_payload_sha256,
        )?;
    }
    if reviewers.len() != 3 || experienced_count < 2 {
        return Err(ContractError::Invalid(
            "exactly three reviewers with two experienced declarations are required".to_string(),
        ));
    }
    Ok(())
}

fn validate_signed_payload(
    mut value: Value,
    domain: &[u8],
    expected: &str,
) -> Result<(), ContractError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ContractError::Invalid("signed payload must be an object".to_string()))?;
    object.remove("signedPayloadSha256");
    object.remove("signatureEvidenceSha256");
    let raw = serde_json::to_vec(&value)
        .map_err(|error| ContractError::Invalid(format!("serialize signed payload: {error}")))?;
    let actual = commitment(domain, &raw).map_err(invalid)?.sha256;
    if actual != expected {
        return Err(ContractError::Invalid(
            "signed payload commitment mismatch".to_string(),
        ));
    }
    Ok(())
}

fn validate_signature_evidence(evidence: &str, expected: &str) -> Result<(), ContractError> {
    let actual = format!("{:x}", Sha256::digest(evidence.as_bytes()));
    if actual != expected {
        return Err(ContractError::Invalid(
            "signature evidence commitment mismatch".to_string(),
        ));
    }
    Ok(())
}

fn parse_timestamp(value: &str) -> Result<DateTime<chrono::FixedOffset>, ContractError> {
    DateTime::parse_from_rfc3339(value)
        .map_err(|error| ContractError::Invalid(format!("invalid RFC3339 timestamp: {error}")))
}

fn invalid(error: impl fmt::Display) -> ContractError {
    ContractError::Invalid(error.to_string())
}
