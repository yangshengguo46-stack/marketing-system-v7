use std::collections::HashSet;
use std::fs;

use codex_ai_ip_domain::HeldOutMissionCase;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::TokenUsageBreakdown;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStatus;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::ModeEvidence;
use crate::ProofBrokerCompatibilityName;
use crate::ProviderRole;
use crate::ReplayCollector;
use crate::Usage;
use crate::validate_case_boundary;

fn usage() -> TokenUsageBreakdown {
    TokenUsageBreakdown {
        total_tokens: 15,
        input_tokens: 10,
        cached_input_tokens: 2,
        cache_write_input_tokens: 1,
        output_tokens: 5,
        reasoning_output_tokens: 3,
    }
}

pub(super) fn mission_case() -> HeldOutMissionCase {
    serde_json::from_value(json!({
        "caseId": "replay-case",
        "objective": "Produce a synthetic test package",
        "subjectKind": "brand",
        "constraints": [],
        "materials": []
    }))
    .unwrap()
}

pub(super) fn package_json() -> String {
    json!({
        "missionSummary": "Synthetic replay summary",
        "influenceRelation": {
            "subject": "Synthetic brand",
            "subjectKind": "brand",
            "audience": "Synthetic audience",
            "desiredAction": "Review the synthetic draft"
        },
        "actionFunnel": [{
            "audienceState": "unaware",
            "intendedChange": "aware",
            "nextAction": "review"
        }],
        "strategicJudgment": "Synthetic judgment",
        "publishableContent": {
            "format": "shortVideo",
            "title": "Synthetic title",
            "body": "Synthetic body",
            "productionNotes": ["Synthetic note"]
        },
        "claims": [{
            "text": "Synthetic interpretation",
            "status": "modelInterpretation",
            "sourceRefs": [],
            "resultReceiptRef": null
        }],
        "openQuestions": [],
        "measurementPlan": {
            "successSignal": "Synthetic review",
            "collectionMethod": "Synthetic collection",
            "observationWindow": "Synthetic window"
        },
        "readiness": "draft"
    })
    .to_string()
}

fn collector() -> ReplayCollector {
    ReplayCollector::new(
        "root-thread",
        "root-turn",
        HashSet::from(["root-thread".to_string()]),
    )
}

fn raw_notification(usage: Option<TokenUsageBreakdown>) -> ServerNotification {
    raw_notification_for("resp-1", usage)
}

fn raw_notification_for(
    response_id: &str,
    usage: Option<TokenUsageBreakdown>,
) -> ServerNotification {
    serde_json::from_value(json!({
        "method": "rawResponse/completed",
        "params": {
            "threadId": "root-thread",
            "turnId": "root-turn",
            "responseId": response_id,
            "usage": usage
        }
    }))
    .unwrap()
}

fn item_notification(text: &str, id: &str) -> ServerNotification {
    serde_json::from_value(json!({
        "method": "item/completed",
        "params": {
            "threadId": "root-thread",
            "turnId": "root-turn",
            "completedAtMs": 1787616000000_i64,
            "item": {"type": "agentMessage", "id": id, "text": text}
        }
    }))
    .unwrap()
}

fn turn_notification(text: &str) -> ServerNotification {
    ServerNotification::TurnCompleted(TurnCompletedNotification {
        thread_id: "root-thread".to_string(),
        turn: Turn {
            id: "root-turn".to_string(),
            items: vec![ThreadItem::AgentMessage {
                id: "final-1".to_string(),
                text: text.to_string(),
                phase: None,
                memory_citation: None,
                delivery: None,
            }],
            items_view: TurnItemsView::Full,
            status: TurnStatus::Completed,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
        },
    })
}

#[test]
fn collector_deduplicates_raw_usage_and_returns_the_validated_final_package() {
    let mut collector = collector();
    let raw = raw_notification(Some(usage()));
    collector.ingest(raw.clone()).unwrap();
    collector.ingest(raw).unwrap();
    let package = package_json();
    collector
        .ingest(item_notification(&package, "final-1"))
        .unwrap();
    collector.ingest(turn_notification(&package)).unwrap();

    let collected = collector.finish(&mission_case()).unwrap();

    assert_eq!(
        collected.usage,
        Usage {
            total_tokens: 15,
            input_tokens: 10,
            cached_input_tokens: 2,
            cache_write_input_tokens: 1,
            output_tokens: 5,
            reasoning_output_tokens: 3,
        }
    );
    assert_eq!(collected.raw_response_count, 1);
    assert_eq!(
        collected.content_package.mission_summary,
        "Synthetic replay summary"
    );
}

#[test]
fn collector_rejects_conflicting_duplicate_usage() {
    let mut collector = collector();
    collector.ingest(raw_notification(Some(usage()))).unwrap();
    let mut conflicting = usage();
    conflicting.total_tokens = 16;

    let error = collector
        .ingest(raw_notification(Some(conflicting)))
        .unwrap_err();

    assert!(error.to_string().contains("conflicting duplicate"));
}

#[test]
fn collector_rejects_missing_negative_or_internally_inconsistent_usage() {
    let mut cases = vec![(None, "missing usage")];
    let mut negative = usage();
    negative.reasoning_output_tokens = -1;
    cases.push((Some(negative), "negative"));
    let mut bad_total = usage();
    bad_total.total_tokens = 14;
    cases.push((Some(bad_total), "totalTokens"));
    let mut bad_cache = usage();
    bad_cache.cached_input_tokens = 10;
    bad_cache.cache_write_input_tokens = 1;
    cases.push((Some(bad_cache), "cached"));
    let mut bad_reasoning = usage();
    bad_reasoning.reasoning_output_tokens = 6;
    cases.push((Some(bad_reasoning), "reasoning"));

    for (usage, expected) in cases {
        let error = collector().ingest(raw_notification(usage)).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "unexpected error: {error:#}"
        );
    }
}

#[test]
fn collector_rejects_unknown_thread_missing_terminal_and_final_mismatch() {
    let unknown: ServerNotification = serde_json::from_value(json!({
        "method": "rawResponse/completed",
        "params": {
            "threadId": "unknown-thread",
            "turnId": "turn",
            "responseId": "resp",
            "usage": usage()
        }
    }))
    .unwrap();
    assert!(
        collector()
            .ingest(unknown)
            .unwrap_err()
            .to_string()
            .contains("unknown thread")
    );

    let mut missing_terminal = collector();
    missing_terminal
        .ingest(raw_notification(Some(usage())))
        .unwrap();
    missing_terminal
        .ingest(item_notification(&package_json(), "final-1"))
        .unwrap();
    assert!(
        missing_terminal
            .finish(&mission_case())
            .unwrap_err()
            .to_string()
            .contains("terminal")
    );

    let mut mismatch = collector();
    mismatch.ingest(raw_notification(Some(usage()))).unwrap();
    mismatch
        .ingest(item_notification(&package_json(), "final-1"))
        .unwrap();
    mismatch.ingest(turn_notification("{}")).unwrap();
    assert!(
        mismatch
            .finish(&mission_case())
            .unwrap_err()
            .to_string()
            .contains("byte-identical")
    );
}

#[test]
fn collector_rejects_aggregate_usage_overflow() {
    let mut collector = collector();
    let first = TokenUsageBreakdown {
        total_tokens: i64::MAX,
        input_tokens: i64::MAX,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
    };
    let second = TokenUsageBreakdown {
        total_tokens: 1,
        input_tokens: 1,
        cached_input_tokens: 0,
        cache_write_input_tokens: 0,
        output_tokens: 0,
        reasoning_output_tokens: 0,
    };
    collector
        .ingest(raw_notification_for("resp-max", Some(first)))
        .unwrap();
    collector
        .ingest(raw_notification_for("resp-overflow", Some(second)))
        .unwrap();
    let package = package_json();
    collector
        .ingest(item_notification(&package, "final-1"))
        .unwrap();
    collector.ingest(turn_notification(&package)).unwrap();

    assert!(
        collector
            .finish(&mission_case())
            .unwrap_err()
            .to_string()
            .contains("overflow")
    );
}

#[test]
fn collector_rejects_multiple_root_final_messages() {
    let mut collector = collector();
    collector
        .ingest(item_notification(&package_json(), "final-1"))
        .unwrap();

    let error = collector
        .ingest(item_notification(&package_json(), "final-2"))
        .unwrap_err();

    assert!(error.to_string().contains("multiple root final"));
}

#[test]
fn collector_rejects_a_final_package_without_any_raw_completion() {
    let mut collector = collector();
    let package = package_json();
    collector
        .ingest(item_notification(&package, "final-1"))
        .unwrap();
    collector.ingest(turn_notification(&package)).unwrap();
    assert!(
        collector
            .finish(&mission_case())
            .unwrap_err()
            .to_string()
            .contains("raw response")
    );
}

#[test]
fn execution_mode_evidence_and_provider_role_are_disjoint() {
    let replay = ModeEvidence::Replay {
        fixture_set_sha256: "a".repeat(64),
    };
    let replay_json = serde_json::to_value(&replay).unwrap();
    assert_eq!(replay_json["executionMode"], json!("replay"));
    assert!(replay_json.get("providerRole").is_none());

    let live = ModeEvidence::Live {
        attestation_sha256: "a".repeat(64),
        provider_budget_evidence_sha256: "b".repeat(64),
        approval_commitment: "approval".to_string(),
        provider_endpoint_commitment: "endpoint".to_string(),
        provider_role: ProviderRole::TargetVolcengine,
        arm_order_commitment: "order".to_string(),
        rate_card_sha256: "c".repeat(64),
        billing_policy_sha256: "d".repeat(64),
        fx_policy_sha256: None,
        authorized_pair_cost_fen: 100,
        retention_deadline: "2026-09-01T00:00:00Z".to_string(),
    };
    let live_json = serde_json::to_value(&live).unwrap();
    assert_eq!(live_json["providerRole"], json!("targetVolcengine"));

    let unknown_role = json!({
        "executionMode": "live",
        "attestationSha256": "a",
        "providerBudgetEvidenceSha256": "b",
        "approvalCommitment": "approval",
        "providerEndpointCommitment": "endpoint",
        "providerRole": "unknown",
        "armOrderCommitment": "order",
        "rateCardSha256": "c",
        "billingPolicySha256": "d",
        "fxPolicySha256": null,
        "authorizedPairCostFen": 100,
        "retentionDeadline": "2026-09-01T00:00:00Z"
    });
    assert!(serde_json::from_value::<ModeEvidence>(unknown_role).is_err());
    assert!(serde_json::from_value::<ExecutionMode>(json!("liveToken")).is_err());
    assert_eq!(
        serde_json::to_value(EvaluationCondition::Candidate).unwrap(),
        json!("candidate")
    );
}

#[test]
fn broker_compatibility_name_accepts_only_exact_open_ai() {
    assert_eq!(
        serde_json::to_value(ProofBrokerCompatibilityName::OpenAi).unwrap(),
        json!("OpenAI")
    );
    for rejected in ["openai", "Volcengine", "GLM"] {
        assert!(serde_json::from_value::<ProofBrokerCompatibilityName>(json!(rejected)).is_err());
    }
}

#[test]
fn run_manifest_rejects_cross_mode_and_replay_is_never_g2_eligible() {
    let manifest_json = json!({
        "schemaVersion": 1,
        "pairId": "pair",
        "frozenRunContextSha256": "a",
        "executionContextSha256": "b",
        "runOrdinal": 1,
        "condition": "generic",
        "forkSha": "c",
        "caseSha256": "d",
        "sourceMaterialsSha256": "e",
        "promptSha256": "f",
        "additionalContextSha256": "g",
        "outputSchemaSha256": "h",
        "threadStartRequestSha256": "i",
        "turnStartRequestSha256": "j",
        "sharedConfigSha256": "k",
        "effectiveConfigSha256": "l",
        "configLayersSha256": "m",
        "nativeSkillSha256": null,
        "preSkillCatalogSha256": "n",
        "postSkillCatalogSha256": "o",
        "normalizedBaseCatalogSha256": "p",
        "skillUseEvidenceSha256": null,
        "codexBinarySha256": "q",
        "evaluatorBinarySha256": "r",
        "brokerComponentSha256": "s",
        "firstRootProviderRequestCommitment": "t",
        "normalizedFirstRootBaseCommitment": "u",
        "firstRootTreatmentDiffCommitment": null,
        "appServerTranscriptSha256": "v",
        "brokerAttemptLedgerSha256": "w",
        "attemptIndexRootSha256": "x",
        "contentPackageSha256": "y",
        "rootThreadId": "root-thread",
        "rootTurnId": "root-turn",
        "sessionId": "session",
        "providerRequestAttemptCount": 1,
        "providerCompletedResponseCount": 1,
        "rawResponseCount": 1,
        "usageScope": "tree",
        "usage": {
            "totalTokens": 15,
            "inputTokens": 10,
            "cachedInputTokens": 2,
            "cacheWriteInputTokens": 1,
            "outputTokens": 5,
            "reasoningOutputTokens": 3
        },
        "modelLabel": "synthetic",
        "actualModelRevision": "synthetic-revision",
        "deploymentOrFingerprintCommitment": null,
        "providerLabel": "synthetic-provider",
        "providerCompatibilityName": "OpenAI",
        "authorizedEvaluationRunCostFen": 0,
        "maxProviderRequestAttempts": 1,
        "maxTotalTokens": 100,
        "maxElapsedSeconds": 30,
        "elapsedMs": 1,
        "treeClosed": true,
        "executionMode": "replay",
        "modeEvidence": {
            "executionMode": "replay",
            "fixtureSetSha256": "z"
        }
    });
    let manifest: crate::RunManifest = serde_json::from_value(manifest_json.clone()).unwrap();
    manifest.validate_execution_mode().unwrap();
    assert!(!manifest.is_g2_eligible());

    let mut mismatched = manifest_json.clone();
    mismatched["executionMode"] = json!("live");
    let mismatched: crate::RunManifest = serde_json::from_value(mismatched).unwrap();
    assert!(mismatched.validate_execution_mode().is_err());

    let mut top_level_role = manifest_json.clone();
    top_level_role["providerRole"] = json!("targetVolcengine");
    assert!(serde_json::from_value::<crate::RunManifest>(top_level_role).is_err());

    let mut replay_with_live_field = manifest_json;
    replay_with_live_field["modeEvidence"]["approvalCommitment"] = json!("forbidden");
    assert!(serde_json::from_value::<crate::RunManifest>(replay_with_live_field).is_err());
}

#[test]
fn committed_replay_transcript_is_typed_and_contains_the_validated_final_package() {
    let transcript_path =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-transcript.jsonl").unwrap();
    let case_path =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-case.json").unwrap();
    let mission: HeldOutMissionCase =
        serde_json::from_slice(&fs::read(case_path).unwrap()).unwrap();
    let transcript = fs::read_to_string(transcript_path).unwrap();
    let mut collector = collector();
    for line in transcript.lines() {
        let notification: ServerNotification = serde_json::from_str(line).unwrap();
        collector.ingest(notification).unwrap();
    }
    let result = collector.finish(&mission).unwrap();
    assert_eq!(result.raw_response_count, 1);
    assert_eq!(result.usage.total_tokens, 150);

    for name in [
        "replay-fixture-set.json",
        "replay-attestation.json",
        "replay-review-1.json",
        "replay-review-2.json",
        "replay-review-3.json",
    ] {
        let resource = format!("tests/fixtures/{name}");
        let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
        let _: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    }
}

#[test]
fn replay_and_live_case_boundaries_use_an_isolated_temporary_git_repository() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repo");
    let private = temp.path().join("private");
    fs::create_dir_all(&repository).unwrap();
    fs::create_dir_all(&private).unwrap();
    let committed_case = repository.join("case.json");
    let private_case = private.join("case.json");
    fs::write(&committed_case, b"{}\n").unwrap();
    fs::write(&private_case, b"{}\n").unwrap();
    let run = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    run(&["init", "--quiet"]);
    run(&["add", "case.json"]);
    run(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "synthetic fixture",
    ]);

    validate_case_boundary(&repository, &committed_case, ExecutionMode::Replay).unwrap();
    validate_case_boundary(&repository, &private_case, ExecutionMode::Live).unwrap();
    assert!(validate_case_boundary(&repository, &committed_case, ExecutionMode::Live).is_err());
    assert!(validate_case_boundary(&repository, &private_case, ExecutionMode::Replay).is_err());
}
