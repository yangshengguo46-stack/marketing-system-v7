mod support;

use anyhow::Context;
use anyhow::Result;
use chrono::SecondsFormat;
use pretty_assertions::assert_eq;
use serde_json::Value;

use support::score_world::ScoreWorld;
use support::score_world::assert_success;

#[test]
fn score_cli_iterates_for_a_contract_valid_candidate_severe_failure() -> Result<()> {
    let world = ScoreWorld::new()?;
    world.install_reviews()?;
    let candidate_arm = world.candidate_arm("reviewer-1")?;
    world.mutate_review("reviewer-1", |review| {
        review["arms"][candidate_arm]["severeFlags"]["fabricatedFactualClaim"] = Value::Bool(true);
    })?;

    let score = world.score()?;
    let decision = successful_decision(&world, &score)?;
    assert_eq!(decision["decision"], "ITERATE_SMALLEST_LEAD_CHANGE");
    assert_eq!(decision["validationFailures"], serde_json::json!([]));
    assert_eq!(
        decision["metrics"],
        serde_json::json!({
            "reviewerCount": 3,
            "experiencedOperatorOrDirectorCount": 2,
            "candidatePreferenceCount": 2,
            "candidateReadyForHumanReviewCount": 2,
            "genericTotals": [15, 15, 15],
            "candidateTotals": [18, 18, 18],
            "pairedDeltas": [3, 3, 3],
            "medianGenericTotal": 15,
            "medianCandidateTotal": 18,
            "medianPairedDelta": 3,
            "candidateSevereFailureCount": 1,
            "candidateSevereFlags": {
                "fabricatedFactualClaim": 1,
                "wrongSubjectOrDesiredAction": 0,
                "notActuallyUsable": 0,
                "rightsOrPrivacyViolation": 0,
            },
        })
    );
    Ok(())
}

#[test]
fn score_cli_publishes_semantic_invalid_for_a_resigned_rubric_mismatch() -> Result<()> {
    let world = ScoreWorld::new()?;
    world.install_reviews()?;
    world.mutate_review("reviewer-3", |review| {
        review["rubricSha256"] = Value::String("8".repeat(64));
    })?;

    let score = world.score()?;
    let decision = successful_decision(&world, &score)?;
    assert_eq!(decision["decision"], "INVALID_PROOF");
    assert_eq!(decision["metrics"], Value::Null);
    assert_eq!(
        decision["validationFailures"],
        serde_json::json!(["REVIEW_RUBRIC_COMMITMENT_MISMATCH"])
    );
    Ok(())
}

#[test]
fn score_cli_rejects_a_different_current_evaluator_without_side_effects() -> Result<()> {
    let world = ScoreWorld::new()?;
    world.install_reviews()?;
    let before = world.tree_snapshot()?;

    let score = world.score_from_copied_evaluator()?;
    assert!(!score.status.success());
    assert!(score.stdout.is_empty());
    assert_eq!(
        score.stderr,
        b"Error: replay evaluator executable differs from the frozen binary\n"
    );
    world.assert_decision_paths_absent();
    assert_eq!(world.tree_snapshot()?, before);
    Ok(())
}

#[test]
fn score_cli_rejects_frozen_codex_byte_drift_without_side_effects() -> Result<()> {
    let world = ScoreWorld::with_copied_codex_binary()?;
    world.install_reviews()?;
    let before = world.tree_snapshot()?;
    world.mutate_frozen_codex_binary()?;

    let score = world.score()?;
    assert!(!score.status.success());
    assert!(score.stdout.is_empty());
    assert_eq!(
        score.stderr,
        b"Error: frozen replay Codex binary bytes or identity changed\n"
    );
    world.assert_decision_paths_absent();
    assert_eq!(world.tree_snapshot()?, before);
    Ok(())
}

fn successful_decision(world: &ScoreWorld, output: &std::process::Output) -> Result<Value> {
    assert_success("score", output);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let bytes = world.decision_bytes()?;
    assert!(!bytes.ends_with(b"\n"));
    let decision: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(serde_json_canonicalizer::to_vec(&decision)?, bytes);
    assert_eq!(decision["schemaVersion"], 1);
    let commitments = world.expected_commitments()?;
    for (key, expected) in commitments.as_object().context("decision commitments")? {
        assert_eq!(&decision[key], expected, "decision commitment {key}");
    }
    let generated = chrono::DateTime::parse_from_rfc3339(
        decision["generatedAt"]
            .as_str()
            .context("decision generatedAt")?,
    )?;
    assert_eq!(
        generated
            .to_utc()
            .to_rfc3339_opts(SecondsFormat::Millis, true),
        decision["generatedAt"]
    );
    world.assert_staging_absent();
    let decision_text = String::from_utf8(bytes)?;
    for forbidden in [
        "reviewer-1",
        "qualificationClass",
        "signatureEvidence",
        "synthetic review evidence",
        "private review reason",
        "seedCommitment",
        "coordinator/mappings",
    ] {
        assert!(
            !decision_text.contains(forbidden),
            "decision leak: {forbidden}"
        );
    }
    Ok(decision)
}
