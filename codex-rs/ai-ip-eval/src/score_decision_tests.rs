use pretty_assertions::assert_eq;

use crate::ArmReview;
use crate::DimensionScores;
use crate::SevereFlags;
use crate::score::BusinessDecision;
use crate::score::MappedPreference;
use crate::score::MappedReview;
use crate::score::ScoredValidOutcome;
use crate::score::score_valid_reviews;
use crate::score::verify_mapped_reviews;
use crate::score_decision::BlindDecision;
use crate::score_decision::BlindDecisionFailure;
use crate::score_decision::DecisionCommitments;
use crate::score_decision::review_submission_set_commitment;
use crate::score_decision::reviewer_mapping_set_commitment;

#[test]
fn valid_decision_is_exact_canonical_body_free_wire() {
    let decision =
        BlindDecision::valid(commitments(), scored(), "2026-08-29T01:02:03.004Z").unwrap();
    let bytes = decision.canonical_bytes().unwrap();

    assert_eq!(
        String::from_utf8(bytes.clone()).unwrap(),
        "{\"blindPackReceiptSha256\":\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\",\"decision\":\"PASS\",\"decisionPolicySha256\":\"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff\",\"frozenRunContextSha256\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"generatedAt\":\"2026-08-29T01:02:03.004Z\",\"metrics\":{\"candidatePreferenceCount\":2,\"candidateReadyForHumanReviewCount\":2,\"candidateSevereFailureCount\":0,\"candidateSevereFlags\":{\"fabricatedFactualClaim\":0,\"notActuallyUsable\":0,\"rightsOrPrivacyViolation\":0,\"wrongSubjectOrDesiredAction\":0},\"candidateTotals\":[18,18,18],\"experiencedOperatorOrDirectorCount\":2,\"genericTotals\":[15,15,15],\"medianCandidateTotal\":18,\"medianGenericTotal\":15,\"medianPairedDelta\":3,\"pairedDeltas\":[3,3,3],\"reviewerCount\":3},\"pairId\":\"pair-1\",\"reviewSubmissionsSha256\":\"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd\",\"reviewerMappingsSha256\":\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\",\"rubricSha256\":\"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee\",\"schemaVersion\":1,\"validationFailures\":[]}"
    );
    let text = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
    for forbidden in [
        "private reason",
        "reviewer-1",
        "qualification",
        "signature",
        "seed",
        "mapping.json",
        "candidate body",
        "generic body",
        "private/root",
        "skill.md",
        "totaltokens",
        "threadid",
        "responseid",
        "business_signal_pass_pending_foundation",
        "pass_to_phase_0b",
    ] {
        assert!(
            !text.contains(forbidden),
            "private decision leaked {forbidden}"
        );
    }
}

#[test]
fn invalid_decision_has_null_metrics_and_sorted_unique_failure_codes() {
    let decision = BlindDecision::invalid(
        commitments(),
        vec![
            BlindDecisionFailure::InsufficientExperiencedReviewers,
            BlindDecisionFailure::ReviewRubricCommitmentMismatch,
            BlindDecisionFailure::ReviewSeedCommitmentMismatch,
            BlindDecisionFailure::ReviewBundleCommitmentMismatch,
            BlindDecisionFailure::ReviewMappingCommitmentMismatch,
            BlindDecisionFailure::ReviewerQualificationMismatch,
            BlindDecisionFailure::ReviewerIdSetMismatch,
            BlindDecisionFailure::ReviewSubmissionInvalid,
            BlindDecisionFailure::ReviewRubricCommitmentMismatch,
        ],
        "2026-08-29T01:02:03.004Z",
    )
    .unwrap();

    assert_eq!(decision.decision(), BusinessDecision::InvalidProof);
    assert_eq!(decision.metrics(), None);
    assert_eq!(
        decision.validation_failures(),
        vec![
            BlindDecisionFailure::ReviewSubmissionInvalid,
            BlindDecisionFailure::ReviewerIdSetMismatch,
            BlindDecisionFailure::ReviewerQualificationMismatch,
            BlindDecisionFailure::ReviewMappingCommitmentMismatch,
            BlindDecisionFailure::ReviewBundleCommitmentMismatch,
            BlindDecisionFailure::ReviewSeedCommitmentMismatch,
            BlindDecisionFailure::ReviewRubricCommitmentMismatch,
            BlindDecisionFailure::InsufficientExperiencedReviewers,
        ]
    );
    let value: serde_json::Value =
        serde_json::from_slice(&decision.canonical_bytes().unwrap()).unwrap();
    assert_eq!(value["metrics"], serde_json::Value::Null);
    assert_eq!(
        value["validationFailures"],
        serde_json::json!([
            "REVIEW_SUBMISSION_INVALID",
            "REVIEWER_ID_SET_MISMATCH",
            "REVIEWER_QUALIFICATION_MISMATCH",
            "REVIEW_MAPPING_COMMITMENT_MISMATCH",
            "REVIEW_BUNDLE_COMMITMENT_MISMATCH",
            "REVIEW_SEED_COMMITMENT_MISMATCH",
            "REVIEW_RUBRIC_COMMITMENT_MISMATCH",
            "INSUFFICIENT_EXPERIENCED_REVIEWERS"
        ])
    );
}

#[test]
fn decision_rejects_impossible_shapes_and_noncanonical_authority() {
    let empty =
        BlindDecision::invalid(commitments(), Vec::new(), "2026-08-29T01:02:03.004Z").unwrap_err();
    assert_eq!(
        empty.to_string(),
        "invalid blind decision requires at least one validation failure"
    );

    let mut bad = commitments();
    bad.rubric_sha256 = "A".repeat(64);
    let error = BlindDecision::valid(bad, scored(), "2026-08-29T01:02:03.004Z").unwrap_err();
    assert_eq!(
        error.to_string(),
        "blind decision commitment is not lowercase SHA-256"
    );

    let timestamp =
        BlindDecision::valid(commitments(), scored(), "2026-08-29T09:02:03.004+08:00").unwrap_err();
    assert_eq!(
        timestamp.to_string(),
        "blind decision generatedAt is not canonical UTC milliseconds"
    );
}

#[test]
fn raw_set_commitments_are_exact_order_independent_and_require_three_unique_ids() {
    let entries = [
        ("reviewer-2", b"two".as_slice()),
        ("reviewer-1", b"one"),
        ("reviewer-3", b"three"),
    ];
    let reverse = [
        ("reviewer-3", b"three".as_slice()),
        ("reviewer-1", b"one"),
        ("reviewer-2", b"two"),
    ];
    let mapping = reviewer_mapping_set_commitment(&entries).unwrap();
    let reviews = review_submission_set_commitment(&entries).unwrap();

    assert_eq!(mapping, reviewer_mapping_set_commitment(&reverse).unwrap());
    assert_eq!(reviews, review_submission_set_commitment(&reverse).unwrap());
    assert_eq!(
        mapping,
        "cca888805d1410284b2a9ee8558ffca36265e45e44501479d307d6037938c6a0"
    );
    assert_eq!(
        reviews,
        "d7d9101c95f6c74490dd698376b6dc725887e8e68a6b13bc1cf5945545eb8bd5"
    );
    assert_ne!(mapping, reviews);

    let duplicate = reviewer_mapping_set_commitment(&[
        ("reviewer-1", b"one"),
        ("reviewer-1", b"two"),
        ("reviewer-3", b"three"),
    ])
    .unwrap_err();
    assert_eq!(
        duplicate.to_string(),
        "raw set commitment IDs must be unique"
    );
    for invalid in [
        &entries[..2],
        &[
            ("reviewer-1", b"one".as_slice()),
            ("reviewer-2", b"two"),
            ("reviewer-3", b"three"),
            ("reviewer-4", b"four"),
        ][..],
    ] {
        let error = review_submission_set_commitment(invalid).unwrap_err();
        assert_eq!(
            error.to_string(),
            "raw set commitment requires exactly three entries"
        );
    }
}

fn commitments() -> DecisionCommitments {
    DecisionCommitments {
        pair_id: "pair-1".to_string(),
        frozen_run_context_sha256: "a".repeat(64),
        blind_pack_receipt_sha256: "b".repeat(64),
        reviewer_mappings_sha256: "c".repeat(64),
        review_submissions_sha256: "d".repeat(64),
        rubric_sha256: "e".repeat(64),
        decision_policy_sha256: "f".repeat(64),
    }
}

fn scored() -> ScoredValidOutcome {
    let reviews = [
        mapped(MappedPreference::Candidate, true, true),
        mapped(MappedPreference::Candidate, true, true),
        mapped(MappedPreference::Generic, false, false),
    ];
    score_valid_reviews(&verify_mapped_reviews(&reviews).unwrap()).unwrap()
}

fn mapped(preferred: MappedPreference, candidate_ready: bool, experienced: bool) -> MappedReview {
    MappedReview {
        preferred,
        generic: arm(15, false),
        candidate: arm(18, candidate_ready),
        experienced_operator_or_director: experienced,
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
        reasons: vec!["private reason".to_string()],
        severe_flags: SevereFlags {
            fabricated_factual_claim: false,
            wrong_subject_or_desired_action: false,
            not_actually_usable: false,
            rights_or_privacy_violation: false,
        },
    }
}
