use pretty_assertions::assert_eq;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;

use crate::ArmReview;
use crate::DimensionScores;
use crate::EvaluationCondition;
use crate::FrozenContracts;
use crate::ReviewerArms;
use crate::ReviewerDeclaration;
use crate::ReviewerQualificationBinding;
use crate::ReviewerSubmission;
use crate::SevereFlags;
use crate::blind_bundle_model::BlindPackReceipt;
use crate::blind_bundle_model::ReviewBundleManifest;
use crate::blind_bundle_model::ReviewerMapping;
use crate::blind_bundle_model::ReviewerMappingCommitment;
use crate::blind_bundle_model::ReviewerQualificationCommitment;
use crate::score::BusinessDecision;
use crate::score::score_valid_reviews;
use crate::score_decision::BlindDecisionFailure;
use crate::score_decision::review_submission_set_commitment;
use crate::score_validation::ScoreBundleSourceBinding;
use crate::score_validation::ScoreReviewerEvidence;
use crate::score_validation::ScoreValidationInput;
use crate::score_validation::ScoreValidationOutcome;

const REVIEW_SUBMISSION_DOMAIN: &[u8] = b"AI-IP-REVIEW-SUBMISSION-V1\0";

#[test]
fn score_validation_maps_candidate_from_different_physical_arms_into_one_valid_outcome() {
    let fixture = ValidationFixture::valid();
    fixture.assert_submission_contracts_valid();
    let validated = crate::score_validation::validate_score_reviews(fixture.input()).unwrap();
    let ScoreValidationOutcome::Valid(mapped) = validated.outcome else {
        panic!("valid score evidence was classified INVALID_PROOF");
    };
    let scored = score_valid_reviews(&mapped).unwrap();

    assert_eq!(scored.decision(), BusinessDecision::Pass);
    assert_eq!(scored.metrics().candidate_preference_count, 2);
    assert_eq!(scored.metrics().candidate_ready_for_human_review_count, 2);
    assert_eq!(scored.metrics().median_candidate_total, 18);
    assert_eq!(scored.metrics().median_paired_delta, 3);
    assert_eq!(
        fixture.reviewers[0].mapping.a,
        EvaluationCondition::Candidate
    );
    assert_eq!(
        fixture.reviewers[1].mapping.b,
        EvaluationCondition::Candidate
    );
}

#[test]
fn score_validation_commits_malformed_review_bytes_before_invalid_proof() {
    let mut fixture = ValidationFixture::valid();
    fixture.reviewers[1].submission_raw = b"{".to_vec();
    let expected_commitment = review_submission_set_commitment(&fixture.review_entries()).unwrap();

    let validated = crate::score_validation::validate_score_reviews(fixture.input()).unwrap();
    assert_eq!(
        validated.commitments.review_submissions_sha256,
        expected_commitment
    );
    assert_eq!(
        validated.outcome,
        ScoreValidationOutcome::Invalid(vec![
            BlindDecisionFailure::ReviewSubmissionInvalid,
            BlindDecisionFailure::InsufficientExperiencedReviewers,
        ])
    );
}

struct ValidationFixture {
    pair_id: String,
    frozen_context_sha256: String,
    blind_pack_receipt_raw: Vec<u8>,
    rubric_sha256: String,
    decision_policy_sha256: String,
    reviewer_submission_schema_sha256: String,
    case_sha256: String,
    materials_manifest_sha256: String,
    source_materials_sha256: String,
    generic_package_sha256: String,
    candidate_package_sha256: String,
    reviewers: [OwnedReviewerEvidence; 3],
}

struct OwnedReviewerEvidence {
    declaration: ReviewerDeclaration,
    receipt_commitment: ReviewerMappingCommitment,
    mapping: ReviewerMapping,
    mapping_raw: Vec<u8>,
    review_bundle_raw: Vec<u8>,
    submission_raw: Vec<u8>,
}

impl ValidationFixture {
    fn valid() -> Self {
        let contracts = FrozenContracts::load()
            .unwrap()
            .blind_review_contracts()
            .unwrap();
        let declarations = reviewer_declarations();
        let orientations = [
            [EvaluationCondition::Candidate, EvaluationCondition::Generic],
            [EvaluationCondition::Generic, EvaluationCondition::Candidate],
            [EvaluationCondition::Candidate, EvaluationCondition::Generic],
        ];
        let logical_preferences = [
            EvaluationCondition::Candidate,
            EvaluationCondition::Candidate,
            EvaluationCondition::Generic,
        ];
        let case_sha256 = "c".repeat(64);
        let materials_manifest_sha256 = "d".repeat(64);
        let source_materials_sha256 = "e".repeat(64);
        let generic_package_sha256 = "3".repeat(64);
        let candidate_package_sha256 = "5".repeat(64);
        let reviewers = std::array::from_fn(|index| {
            let declaration = declarations[index].clone();
            let seed_prefix = index + 4;
            let seed_suffix = "b".repeat(63);
            let seed_commitment = format!("{seed_prefix}{seed_suffix}");
            let qualification = ReviewerQualificationCommitment {
                qualification_class: declaration.qualification_class.clone(),
                experienced_operator_or_director: declaration.experienced_operator_or_director,
                attestation_signed_payload_sha256: declaration.signed_payload_sha256.clone(),
                attestation_signature_evidence_sha256: declaration
                    .signature_evidence_sha256
                    .clone(),
            };
            let review_bundle = ReviewBundleManifest {
                schema_version: 1,
                pair_id: "pair-1".to_string(),
                reviewer_id: declaration.reviewer_id.clone(),
                qualification,
                case_sha256: case_sha256.clone(),
                materials_manifest_sha256: materials_manifest_sha256.clone(),
                source_materials_sha256: source_materials_sha256.clone(),
                a_sha256: match orientations[index][0] {
                    EvaluationCondition::Generic => generic_package_sha256.clone(),
                    EvaluationCondition::Candidate => candidate_package_sha256.clone(),
                },
                b_sha256: match orientations[index][1] {
                    EvaluationCondition::Generic => generic_package_sha256.clone(),
                    EvaluationCondition::Candidate => candidate_package_sha256.clone(),
                },
                rubric_sha256: contracts.rubric_sha256.clone(),
                reviewer_submission_schema_sha256: contracts
                    .reviewer_submission_schema_sha256
                    .clone(),
            };
            let review_bundle_raw = serde_json_canonicalizer::to_vec(&review_bundle).unwrap();
            let review_bundle_sha256 = sha256(&review_bundle_raw);
            let mapping = ReviewerMapping {
                schema_version: 1,
                pair_id: "pair-1".to_string(),
                reviewer_id: declaration.reviewer_id.clone(),
                review_bundle_sha256: review_bundle_sha256.clone(),
                seed_commitment: seed_commitment.clone(),
                a: orientations[index][0],
                b: orientations[index][1],
            };
            let mapping_raw = serde_json_canonicalizer::to_vec(&mapping).unwrap();
            let receipt_commitment = ReviewerMappingCommitment {
                reviewer_id: declaration.reviewer_id.clone(),
                review_bundle_sha256: review_bundle_sha256.clone(),
                mapping_sha256: sha256(&mapping_raw),
                seed_commitment,
            };
            let preferred = if orientations[index][0] == logical_preferences[index] {
                crate::PreferredArm::A
            } else {
                crate::PreferredArm::B
            };
            let submission_raw = signed_submission(
                &declaration,
                &review_bundle_sha256,
                &contracts.rubric_sha256,
                preferred,
                orientations[index],
                index < 2,
            );
            OwnedReviewerEvidence {
                declaration,
                receipt_commitment,
                mapping,
                mapping_raw,
                review_bundle_raw,
                submission_raw,
            }
        });
        let blind_pack_receipt_raw = serde_json_canonicalizer::to_vec(&BlindPackReceipt {
            schema_version: 1,
            pair_id: "pair-1".to_string(),
            frozen_run_context_sha256: "f".repeat(64),
            pair_receipt_sha256: None,
            pair_verification_sha256: "7".repeat(64),
            rubric_sha256: contracts.rubric_sha256.clone(),
            decision_policy_sha256: contracts.decision_policy_sha256.clone(),
            reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256.clone(),
            reviewer_mappings: reviewers
                .each_ref()
                .map(|reviewer| reviewer.receipt_commitment.clone())
                .to_vec(),
            inventory_root_sha256: "1".repeat(64),
            reviews_drop_sha256: sha256(br#"{"entries":[]}"#),
            generated_at: "2026-08-29T12:30:00.000Z".to_string(),
        })
        .unwrap();
        Self {
            pair_id: "pair-1".to_string(),
            frozen_context_sha256: "f".repeat(64),
            blind_pack_receipt_raw,
            rubric_sha256: contracts.rubric_sha256,
            decision_policy_sha256: contracts.decision_policy_sha256,
            reviewer_submission_schema_sha256: contracts.reviewer_submission_schema_sha256,
            case_sha256,
            materials_manifest_sha256,
            source_materials_sha256,
            generic_package_sha256,
            candidate_package_sha256,
            reviewers,
        }
    }

    fn input(&self) -> ScoreValidationInput<'_> {
        ScoreValidationInput {
            pair_id: &self.pair_id,
            frozen_context_sha256: &self.frozen_context_sha256,
            blind_pack_receipt_raw: &self.blind_pack_receipt_raw,
            rubric_sha256: &self.rubric_sha256,
            decision_policy_sha256: &self.decision_policy_sha256,
            bundle_sources: ScoreBundleSourceBinding {
                case_sha256: &self.case_sha256,
                materials_manifest_sha256: &self.materials_manifest_sha256,
                source_materials_sha256: &self.source_materials_sha256,
                generic_package_sha256: &self.generic_package_sha256,
                candidate_package_sha256: &self.candidate_package_sha256,
                reviewer_submission_schema_sha256: &self.reviewer_submission_schema_sha256,
            },
            reviewers: self
                .reviewers
                .each_ref()
                .map(|reviewer| ScoreReviewerEvidence {
                    declaration: &reviewer.declaration,
                    mapping_raw: &reviewer.mapping_raw,
                    review_bundle_raw: &reviewer.review_bundle_raw,
                    submission_raw: &reviewer.submission_raw,
                }),
        }
    }

    fn review_entries(&self) -> [(&str, &[u8]); 3] {
        self.reviewers.each_ref().map(|reviewer| {
            (
                reviewer.declaration.reviewer_id.as_str(),
                reviewer.submission_raw.as_slice(),
            )
        })
    }

    fn assert_submission_contracts_valid(&self) {
        let contracts = FrozenContracts::load().unwrap();
        for reviewer in &self.reviewers {
            contracts
                .validate_reviewer_submission(&reviewer.submission_raw)
                .unwrap();
        }
    }
}

fn signed_submission(
    declaration: &ReviewerDeclaration,
    review_bundle_sha256: &str,
    rubric_sha256: &str,
    preferred: crate::PreferredArm,
    orientation: [EvaluationCondition; 2],
    candidate_ready: bool,
) -> Vec<u8> {
    let arm_for = |condition| match condition {
        EvaluationCondition::Generic => {
            arm(/*total*/ 15, /*ready_for_human_review*/ false)
        }
        EvaluationCondition::Candidate => arm(/*total*/ 18, candidate_ready),
    };
    let reviewer_id = &declaration.reviewer_id;
    let signature_evidence = format!("synthetic review evidence for {reviewer_id}");
    let value = serde_json::to_value(ReviewerSubmission {
        schema_version: 1,
        reviewer_id: declaration.reviewer_id.clone(),
        review_bundle_sha256: review_bundle_sha256.to_string(),
        rubric_sha256: rubric_sha256.to_string(),
        qualification: ReviewerQualificationBinding {
            qualification_class: declaration.qualification_class.clone(),
            experienced_operator_or_director: declaration.experienced_operator_or_director,
            attestation_signed_payload_sha256: declaration.signed_payload_sha256.clone(),
            attestation_signature_evidence_sha256: declaration.signature_evidence_sha256.clone(),
        },
        preferred,
        arms: ReviewerArms {
            a: arm_for(orientation[0]),
            b: arm_for(orientation[1]),
        },
        signed_at: "2026-08-29T12:40:00Z".to_string(),
        signed_payload_sha256: String::new(),
        signature_evidence_sha256: sha256(signature_evidence.as_bytes()),
        signature_evidence,
    })
    .unwrap();
    resign_submission(value)
}

fn resign_submission(mut value: Value) -> Vec<u8> {
    let signature_evidence_sha256 = sha256(value["signatureEvidence"].as_str().unwrap().as_bytes());
    value["signatureEvidenceSha256"] = Value::String(signature_evidence_sha256);
    let mut signed = value.clone();
    signed
        .as_object_mut()
        .unwrap()
        .remove("signedPayloadSha256");
    signed
        .as_object_mut()
        .unwrap()
        .remove("signatureEvidenceSha256");
    value["signedPayloadSha256"] = Value::String(
        crate::jcs::commitment(
            REVIEW_SUBMISSION_DOMAIN,
            &serde_json::to_vec(&signed).unwrap(),
        )
        .unwrap()
        .sha256,
    );
    crate::jcs::canonicalize_value(&value).unwrap()
}

fn reviewer_declarations() -> [ReviewerDeclaration; 3] {
    [
        reviewer_declaration(
            "reviewer-1",
            "experienced-content-operator",
            /*experienced_operator_or_director*/ true,
            "633fb13185126729de46fb01b706605e432fd133a1addfd95630c5eedce12f7c",
            "synthetic qualification signature one",
            "a394b9f51e09b9102741df17e00165efb9fafba47bf980c69a8f6c5ea74dbf5c",
        ),
        reviewer_declaration(
            "reviewer-2",
            "experienced-director",
            /*experienced_operator_or_director*/ true,
            "1ba1a478f4477cf69f623402ca892051c523639a91441c58096f81a988e4a413",
            "synthetic qualification signature two",
            "c895d2d232f4126a8f5f3f44c4bcccae978bcd3334e2f3a641e78281640f7e5a",
        ),
        reviewer_declaration(
            "reviewer-3",
            "independent-business-reviewer",
            /*experienced_operator_or_director*/ false,
            "2576d3d81b94a78d0b357760dc27d9727e104570e83c74fd25137db3bb0af1e4",
            "synthetic qualification signature three",
            "ed9e7b85339c38c56621498a1bfdd7764c0184cd4834d45084cc7e8f3565a39d",
        ),
    ]
}

fn reviewer_declaration(
    reviewer_id: &str,
    qualification_class: &str,
    experienced_operator_or_director: bool,
    signed_payload_sha256: &str,
    signature_evidence: &str,
    signature_evidence_sha256: &str,
) -> ReviewerDeclaration {
    ReviewerDeclaration {
        reviewer_id: reviewer_id.to_string(),
        qualification_class: qualification_class.to_string(),
        experienced_operator_or_director,
        declared_at: "2026-08-28T07:20:00Z".to_string(),
        signed_payload_sha256: signed_payload_sha256.to_string(),
        signature_evidence: signature_evidence.to_string(),
        signature_evidence_sha256: signature_evidence_sha256.to_string(),
    }
}

fn arm(total: u8, ready_for_human_review: bool) -> ArmReview {
    let base = total / 6;
    let remainder = usize::from(total % 6);
    let mut values = [base; 6];
    for value in values.iter_mut().take(remainder) {
        *value += 1;
    }
    ArmReview {
        scores: DimensionScores {
            business_outcome_clarity: values[0],
            subject_audience_action_fit: values[1],
            strategic_judgment: values[2],
            publishable_usability: values[3],
            evidence_integrity: values[4],
            measurement_usefulness: values[5],
        },
        ready_for_human_review,
        reasons: vec!["private review reason".to_string()],
        severe_flags: SevereFlags {
            fabricated_factual_claim: false,
            wrong_subject_or_desired_action: false,
            not_actually_usable: false,
            rights_or_privacy_violation: false,
        },
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
