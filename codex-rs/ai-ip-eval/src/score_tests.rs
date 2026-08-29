use pretty_assertions::assert_eq;

use crate::ArmReview;
use crate::DimensionScores;
use crate::SevereFlags;
use crate::score::BusinessDecision;
use crate::score::MappedPreference;
use crate::score::MappedReview;
use crate::score::MappedReviewSetError;
use crate::score::score_valid_reviews;
use crate::score::verify_mapped_reviews;

#[test]
fn exact_pass_thresholds_are_inclusive() {
    let outcome = score(&baseline_reviews()).unwrap();

    assert_eq!(outcome.decision(), BusinessDecision::Pass);
    assert_eq!(outcome.metrics().reviewer_count, 3);
    assert_eq!(outcome.metrics().experienced_operator_or_director_count, 2);
    assert_eq!(outcome.metrics().candidate_preference_count, 2);
    assert_eq!(outcome.metrics().candidate_ready_for_human_review_count, 2);
    assert_eq!(outcome.metrics().generic_totals, [15, 15, 15]);
    assert_eq!(outcome.metrics().candidate_totals, [18, 18, 18]);
    assert_eq!(outcome.metrics().paired_deltas, [3, 3, 3]);
    assert_eq!(outcome.metrics().median_generic_total, 15);
    assert_eq!(outcome.metrics().median_candidate_total, 18);
    assert_eq!(outcome.metrics().median_paired_delta, 3);
    assert_eq!(outcome.metrics().candidate_severe_failure_count, 0);
    assert_eq!(outcome.metrics().candidate_severe_flags.total(), 0);
}

#[test]
fn every_below_threshold_row_iterates() {
    let cases = [
        (
            "one candidate preference",
            mutate(|reviews| reviews[1].preferred = MappedPreference::Generic),
        ),
        (
            "tie is excluded from candidate preferences",
            mutate(|reviews| reviews[1].preferred = MappedPreference::Tie),
        ),
        (
            "median paired delta two",
            mutate(|reviews| {
                reviews[0].generic = arm(16, false);
                reviews[1].generic = arm(16, false);
                reviews[2].generic = arm(14, false);
            }),
        ),
        (
            "median candidate total seventeen",
            mutate(|reviews| {
                reviews[0].candidate = arm(17, true);
                reviews[1].candidate = arm(17, true);
                reviews[2].candidate = arm(19, false);
                reviews[0].generic = arm(14, false);
                reviews[1].generic = arm(14, false);
                reviews[2].generic = arm(16, false);
            }),
        ),
        (
            "one candidate ready",
            mutate(|reviews| reviews[1].candidate.ready_for_human_review = false),
        ),
    ];

    for (name, reviews) in cases {
        let outcome = score(&reviews).unwrap();
        assert_eq!(
            outcome.decision(),
            BusinessDecision::IterateSmallestLeadChange,
            "{name}"
        );
    }
}

#[test]
fn every_candidate_severe_flag_iterates_and_is_counted() {
    let cases: [(&str, fn(&mut SevereFlags), [u8; 4]); 4] = [
        (
            "fabricated factual claim",
            |flags| flags.fabricated_factual_claim = true,
            [1, 0, 0, 0],
        ),
        (
            "wrong subject or desired action",
            |flags| flags.wrong_subject_or_desired_action = true,
            [0, 1, 0, 0],
        ),
        (
            "not actually usable",
            |flags| flags.not_actually_usable = true,
            [0, 0, 1, 0],
        ),
        (
            "rights or privacy violation",
            |flags| flags.rights_or_privacy_violation = true,
            [0, 0, 0, 1],
        ),
    ];

    for (name, mutation, expected) in cases {
        let reviews = mutate(|reviews| mutation(&mut reviews[0].candidate.severe_flags));
        let outcome = score(&reviews).unwrap();
        assert_eq!(
            outcome.decision(),
            BusinessDecision::IterateSmallestLeadChange,
            "{name}"
        );
        let flags = &outcome.metrics().candidate_severe_flags;
        assert_eq!(
            [
                flags.fabricated_factual_claim,
                flags.wrong_subject_or_desired_action,
                flags.not_actually_usable,
                flags.rights_or_privacy_violation,
            ],
            expected,
            "{name}"
        );
        assert_eq!(
            outcome.metrics().candidate_severe_failure_count,
            1,
            "{name}"
        );
    }
}

#[test]
fn scorer_rejects_a_non_three_review_set_and_dimension_overflow() {
    let error = verify_mapped_reviews(&baseline_reviews()[..2]).unwrap_err();
    assert_eq!(error, MappedReviewSetError::WrongReviewCount);

    let mut reviews = baseline_reviews();
    reviews[0].candidate.scores = DimensionScores {
        business_outcome_clarity: u8::MAX,
        subject_audience_action_fit: 1,
        ..scores(0)
    };
    let error = score(&reviews).unwrap_err();
    assert_eq!(error.to_string(), "review dimension total overflow");

    let mut reviews = baseline_reviews();
    reviews[1].experienced_operator_or_director = false;
    let error = verify_mapped_reviews(&reviews).unwrap_err();
    assert_eq!(
        error,
        MappedReviewSetError::InsufficientExperiencedReviewers
    );
}

#[test]
fn paired_delta_is_per_reviewer_signed_and_generic_severe_is_ignored() {
    let reviews = mutate(|reviews| {
        reviews[0].generic = arm(0, false);
        reviews[0].candidate = arm(10, true);
        reviews[1].generic = arm(10, false);
        reviews[1].candidate = arm(20, true);
        reviews[2].generic = arm(20, false);
        reviews[2].candidate = arm(0, false);
        for review in reviews {
            review.generic.severe_flags.fabricated_factual_claim = true;
            review.generic.severe_flags.wrong_subject_or_desired_action = true;
            review.generic.severe_flags.not_actually_usable = true;
            review.generic.severe_flags.rights_or_privacy_violation = true;
        }
    });
    let outcome = score(&reviews).unwrap();

    assert_eq!(outcome.metrics().generic_totals, [0, 10, 20]);
    assert_eq!(outcome.metrics().candidate_totals, [0, 10, 20]);
    assert_eq!(outcome.metrics().paired_deltas, [-20, 10, 10]);
    assert_eq!(outcome.metrics().median_generic_total, 10);
    assert_eq!(outcome.metrics().median_candidate_total, 10);
    assert_eq!(outcome.metrics().median_paired_delta, 10);
    assert_eq!(outcome.metrics().candidate_severe_failure_count, 0);
}

fn baseline_reviews() -> [MappedReview; 3] {
    [
        MappedReview {
            preferred: MappedPreference::Candidate,
            generic: arm(15, false),
            candidate: arm(18, true),
            experienced_operator_or_director: true,
        },
        MappedReview {
            preferred: MappedPreference::Candidate,
            generic: arm(15, false),
            candidate: arm(18, true),
            experienced_operator_or_director: true,
        },
        MappedReview {
            preferred: MappedPreference::Generic,
            generic: arm(15, false),
            candidate: arm(18, false),
            experienced_operator_or_director: false,
        },
    ]
}

fn mutate(mutation: impl FnOnce(&mut [MappedReview; 3])) -> [MappedReview; 3] {
    let mut reviews = baseline_reviews();
    mutation(&mut reviews);
    reviews
}

fn score(reviews: &[MappedReview]) -> anyhow::Result<crate::score::ScoredValidOutcome> {
    let verified = verify_mapped_reviews(reviews)?;
    score_valid_reviews(&verified)
}

fn arm(total: u8, ready_for_human_review: bool) -> ArmReview {
    ArmReview {
        scores: scores(total),
        ready_for_human_review,
        reasons: vec!["private reason".to_string()],
        severe_flags: no_severe_flags(),
    }
}

fn scores(total: u8) -> DimensionScores {
    assert!(total <= 24);
    let base = total / 6;
    let remainder = usize::from(total % 6);
    let mut values = [base; 6];
    for value in values.iter_mut().take(remainder) {
        *value += 1;
    }
    DimensionScores {
        business_outcome_clarity: values[0],
        subject_audience_action_fit: values[1],
        strategic_judgment: values[2],
        publishable_usability: values[3],
        evidence_integrity: values[4],
        measurement_usefulness: values[5],
    }
}

fn no_severe_flags() -> SevereFlags {
    SevereFlags {
        fabricated_factual_claim: false,
        wrong_subject_or_desired_action: false,
        not_actually_usable: false,
        rights_or_privacy_violation: false,
    }
}
