use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fs;
use std::time::Duration;
use std::time::Instant;

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

use crate::ArmActivation;
use crate::ArtifactCommitments;
use crate::BrokerGateConfig;
use crate::Cli;
use crate::EvaluationCondition;
use crate::ExecutionMode;
use crate::GitWorktreeCommitment;
use crate::ModeEvidence;
use crate::PairCoordinator;
use crate::PairPhase;
use crate::ProofBrokerCompatibilityName;
use crate::ProviderRole;
use crate::ReplayCollector;
use crate::ThreadLifecycle;
use crate::Usage;
use crate::commit_arm_order;
use crate::prepare_isolated_homes;
use crate::validate_case_boundary;
use crate::verify_frozen_context;
use clap::Parser;
use codex_responses_api_proxy::ForwardResult;
use codex_responses_api_proxy::RequestGate;
use codex_responses_api_proxy::RequestMetadata;
use codex_responses_api_proxy::TransformedRequestEvidence;
use codex_responses_api_proxy::TransformedRequestMetadata;
use sha2::Digest;
use sha2::Sha256;

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

fn transformed() -> TransformedRequestMetadata {
    TransformedRequestMetadata {
        content_length: 23,
        sha256: [7; 32],
        evidence: TransformedRequestEvidence {
            raw_sha256: [1; 32],
            normalized_sha256: [2; 32],
            normalized_base_commitment: [3; 32],
            treatment_diff_commitment: None,
        },
    }
}

fn root_request(root: &str) -> RequestMetadata {
    RequestMetadata {
        method: "POST".to_string(),
        path: "/v1/responses".to_string(),
        window_id: Some(format!("{root}:0")),
        parent_thread_id: None,
        is_subagent: false,
    }
}

fn coordinator(temp: &tempfile::TempDir, cap: u64) -> PairCoordinator {
    PairCoordinator::create(BrokerGateConfig {
        ledger_path: temp.path().join("attempt-index.jsonl"),
        receipt_dir: temp.path().join("receipts"),
        pair_id: "pair-1".to_string(),
        frozen_run_context_sha256: "a".repeat(64),
        execution_context_sha256: "b".repeat(64),
        arm_order_commitment: "c".repeat(64),
        max_attempts_per_arm: cap,
        max_output_tokens: 321,
    })
    .unwrap()
}

fn activate_first(gate: &PairCoordinator, root: &str, cap_window: Duration) {
    gate.commit_order_for_test(EvaluationCondition::Generic, EvaluationCondition::Candidate)
        .unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 1,
        condition: EvaluationCondition::Generic,
        root_thread_id: root.to_string(),
        deadline: Instant::now() + cap_window,
        deadline_rfc3339: "2026-08-27T12:00:00Z".to_string(),
    })
    .unwrap();
}

#[test]
fn atomic_pair_state_machine_poisoning_is_permanent() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 2);
    assert_eq!(gate.phase(), PairPhase::Created);

    assert!(
        gate.before_forward(&root_request("root-1"), &transformed())
            .is_err()
    );
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(
        gate.commit_order_for_test(EvaluationCondition::Generic, EvaluationCondition::Candidate)
            .is_err()
    );

    let duplicate = coordinator(&tempfile::tempdir().unwrap(), 2);
    assert!(
        duplicate
            .commit_order_for_test(EvaluationCondition::Generic, EvaluationCondition::Generic)
            .is_err()
    );
    assert!(matches!(duplicate.phase(), PairPhase::Poisoned { .. }));

    let happy_temp = tempfile::tempdir().unwrap();
    let happy = coordinator(&happy_temp, 2);
    activate_first(&happy, "root-1", Duration::from_secs(5));
    assert!(matches!(happy.phase(), PairPhase::Active1 { .. }));
    let first = happy
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    happy.after_forward(first, &ForwardResult::Completed { status: 200 });
    let receipt1 = happy.seal_arm().unwrap();
    assert_eq!(happy.phase(), PairPhase::Sealed1);
    happy
        .activate_arm(ArmActivation {
            run_ordinal: 2,
            condition: EvaluationCondition::Candidate,
            root_thread_id: "root-2".to_string(),
            deadline: Instant::now() + Duration::from_secs(5),
            deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
        })
        .unwrap();
    let second = happy
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    happy.after_forward(second, &ForwardResult::Completed { status: 200 });
    let receipt2 = happy.seal_arm().unwrap();
    assert_eq!(
        receipt2.previous_arm_receipt_sha256,
        Some(receipt1.receipt_sha256)
    );
    let final_receipt = happy.finish().unwrap();
    assert_eq!(happy.phase(), PairPhase::Finished);
    assert_eq!(final_receipt.total_attempt_count, 2);
    assert!(
        happy
            .before_forward(&root_request("root-2"), &transformed())
            .is_err()
    );
    assert!(matches!(happy.phase(), PairPhase::Poisoned { .. }));
}

#[test]
fn gate_accepts_only_root_or_known_descendants_and_enforces_caps_deadlines_and_lifecycle() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 2);
    activate_first(&gate, "root", Duration::from_secs(5));
    let root = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    gate.after_forward(root, &ForwardResult::Completed { status: 200 });

    let child = RequestMetadata {
        method: "POST".to_string(),
        path: "/v1/responses".to_string(),
        window_id: Some("child:0".to_string()),
        parent_thread_id: Some("root".to_string()),
        is_subagent: true,
    };
    let child_permit = gate.before_forward(&child, &transformed()).unwrap();
    gate.after_forward(child_permit, &ForwardResult::Completed { status: 200 });
    assert!(gate.known_threads().contains("child"));

    assert!(gate.before_forward(&child, &transformed()).is_err());
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));

    for malformed in [
        RequestMetadata {
            window_id: Some("bad".into()),
            ..root_request("root")
        },
        RequestMetadata {
            path: "/v1/chat/completions".into(),
            ..root_request("root")
        },
        RequestMetadata {
            window_id: Some("other:0".into()),
            ..root_request("root")
        },
        RequestMetadata {
            window_id: Some("orphan:0".into()),
            parent_thread_id: Some("missing".into()),
            is_subagent: true,
            ..root_request("root")
        },
    ] {
        let isolated = coordinator(&tempfile::tempdir().unwrap(), 2);
        activate_first(&isolated, "root", Duration::from_secs(5));
        assert!(isolated.before_forward(&malformed, &transformed()).is_err());
        assert!(matches!(isolated.phase(), PairPhase::Poisoned { .. }));
    }

    for lifecycle in [ThreadLifecycle::GuardianReview, ThreadLifecycle::Guardian] {
        let isolated = coordinator(&tempfile::tempdir().unwrap(), 2);
        activate_first(&isolated, "root", Duration::from_secs(5));
        assert!(isolated.observe_lifecycle(lifecycle.clone()).is_err());
        assert!(
            isolated
                .before_forward(&root_request("root"), &transformed())
                .is_err()
        );
    }

    let expired = coordinator(&tempfile::tempdir().unwrap(), 2);
    activate_first(&expired, "root", Duration::ZERO);
    assert!(
        expired
            .before_forward(&root_request("root"), &transformed())
            .is_err()
    );
}

#[test]
fn attempt_index_is_gap_free_append_only_and_each_request_has_one_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 3);
    activate_first(&gate, "root", Duration::from_secs(5));
    for status in [
        ForwardResult::Completed { status: 200 },
        ForwardResult::Completed { status: 200 },
    ] {
        let permit = gate
            .before_forward(&root_request("root"), &transformed())
            .unwrap();
        gate.after_forward(permit, &status);
    }
    gate.seal_arm().unwrap();

    let bytes = fs::read(temp.path().join("attempt-index.jsonl")).unwrap();
    let lines: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["recordType"], json!("request"));
    assert_eq!(lines[1]["recordType"], json!("terminal"));
    assert_eq!(lines[2]["globalAttemptIndex"], json!(1));
    assert_eq!(lines[3]["globalAttemptIndex"], json!(1));
    assert_eq!(lines[0]["armAttemptIndex"], json!(0));
    assert_eq!(lines[2]["armAttemptIndex"], json!(1));
    for terminal in [&lines[1], &lines[3]] {
        assert!(terminal.get("responseIdCommitment").is_some());
        assert!(terminal.get("actualModelRevision").is_some());
        assert!(terminal.get("deploymentCommitment").is_some());
        assert!(terminal.get("usage").is_some());
        assert!(terminal.get("failureClass").is_some());
    }
    assert_eq!(gate.attempt_count(), 2);
    assert_eq!(gate.in_flight_count(), 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.path().join("attempt-index.jsonl"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(temp.path().join("receipts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn failed_attempt_appends_terminal_then_permanently_poisons_the_pair() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 2);
    activate_first(&gate, "root", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    gate.after_forward(
        permit,
        &ForwardResult::Failed {
            class: codex_responses_api_proxy::ForwardErrorClass::Upstream,
        },
    );

    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(
        gate.before_forward(&root_request("root"), &transformed())
            .is_err()
    );
    let ledger = fs::read_to_string(temp.path().join("attempt-index.jsonl")).unwrap();
    let records: Vec<serde_json::Value> = ledger
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["status"], json!("failed"));
    assert!(temp.path().join("receipts/poison.json").is_file());
}

#[test]
fn second_arm_receipt_chains_the_actual_first_receipt_file_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 2);
    activate_first(&gate, "root-1", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    gate.after_forward(permit, &ForwardResult::Completed { status: 200 });
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: "root-2".to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    let permit = gate
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    gate.after_forward(permit, &ForwardResult::Completed { status: 200 });
    let second = gate.seal_arm().unwrap();
    let first_bytes = fs::read(temp.path().join("receipts/arm-1-receipt.json")).unwrap();
    assert_eq!(
        second.previous_arm_receipt_sha256,
        Some(format!("{:x}", Sha256::digest(first_bytes)))
    );
}

#[test]
fn arm_order_uses_an_owner_only_os_seed_after_context_verification() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = temp.path().join("frozen-run-context.json");
    fs::write(&frozen_path, b"{\"executionMode\":\"live\"}\n").unwrap();
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let order = commit_arm_order(&frozen, temp.path()).unwrap();
    assert_ne!(order.first(), order.second());
    assert_eq!(
        fs::read(temp.path().join("arm-order-seed.bin"))
            .unwrap()
            .len(),
        32
    );
    assert_eq!(order.seed_commitment().len(), 64);
    assert!(commit_arm_order(&frozen, temp.path()).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(temp.path().join("arm-order-seed.bin"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn production_coordinator_accepts_only_the_opaque_os_committed_order() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = temp.path().join("frozen-run-context.json");
    fs::write(&frozen_path, b"{\"executionMode\":\"live\"}\n").unwrap();
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let order_dir = temp.path().join("order");
    let order = commit_arm_order(&frozen, &order_dir).unwrap();
    let expected_phase = PairPhase::OrderCommitted {
        first: order.first(),
        second: order.second(),
    };
    let gate = PairCoordinator::create(BrokerGateConfig {
        ledger_path: temp.path().join("attempt-index.jsonl"),
        receipt_dir: temp.path().join("receipts"),
        pair_id: "pair-live".to_string(),
        frozen_run_context_sha256: frozen.sha256,
        execution_context_sha256: "b".repeat(64),
        arm_order_commitment: order.seed_commitment().to_string(),
        max_attempts_per_arm: 1,
        max_output_tokens: 1,
    })
    .unwrap();

    gate.commit_order(order).unwrap();
    assert_eq!(gate.phase(), expected_phase);
}

#[test]
fn arm_order_commitment_rejects_a_replay_frozen_context() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = temp.path().join("frozen-run-context.json");
    fs::write(&frozen_path, b"{\"executionMode\":\"replay\"}\n").unwrap();
    assert!(verify_frozen_context(&frozen_path).is_err());
}

#[test]
fn shared_config_and_homes_are_byte_identical_except_for_candidate_skill() {
    let temp = tempfile::tempdir().unwrap();
    let config = crate::build_shared_config("approved-model", 43123).unwrap();
    let homes = prepare_isolated_homes(
        temp.path(),
        &config.bytes,
        "deliver-ai-ip-content-package",
        b"synthetic skill\n",
    )
    .unwrap();
    assert_eq!(
        fs::read(homes.generic_codex_home.join("config.toml")).unwrap(),
        config.bytes
    );
    assert_eq!(
        fs::read(homes.candidate_codex_home.join("config.toml")).unwrap(),
        config.bytes
    );
    assert!(
        fs::read_dir(homes.generic_codex_home.join("skills"))
            .unwrap()
            .next()
            .is_none()
    );
    assert_eq!(
        fs::read(
            homes
                .candidate_codex_home
                .join("skills/deliver-ai-ip-content-package/SKILL.md")
        )
        .unwrap(),
        b"synthetic skill\n"
    );
    let all_bytes = fs::read(homes.generic_codex_home.join("config.toml")).unwrap();
    assert!(
        !String::from_utf8_lossy(&all_bytes)
            .to_ascii_lowercase()
            .contains("key")
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&homes.generic_home)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&homes.candidate_home)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
}

#[test]
fn child_environment_is_allowlisted_and_clears_secret_and_proxy_sentinels() {
    let host = BTreeMap::from([
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        (
            "OPENAI_API_KEY".to_string(),
            "credential-sentinel".to_string(),
        ),
        ("SESSION_TOKEN".to_string(), "token-sentinel".to_string()),
        ("HTTPS_PROXY".to_string(), "proxy-sentinel".to_string()),
        (
            "AWS_SECRET_ACCESS_KEY".to_string(),
            "cloud-sentinel".to_string(),
        ),
    ]);
    let policy = crate::ChildEnvironment::from_environment(
        &host,
        "/private/arm/home",
        "/private/arm/home/.codex",
        "/private/arm/tmp",
    )
    .unwrap();
    assert_eq!(
        policy.variables().get("PATH").map(String::as_str),
        Some("/usr/bin:/bin")
    );
    assert_eq!(
        policy.variables().get("HOME").map(String::as_str),
        Some("/private/arm/home")
    );
    for forbidden in [
        "OPENAI_API_KEY",
        "SESSION_TOKEN",
        "HTTPS_PROXY",
        "AWS_SECRET_ACCESS_KEY",
    ] {
        assert!(!policy.variables().contains_key(forbidden));
    }

    #[cfg(unix)]
    {
        let output = policy.command("/usr/bin/env").output().unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(!output.contains("credential-sentinel"));
        assert!(!output.contains("token-sentinel"));
        assert!(!output.contains("proxy-sentinel"));
        assert!(!output.contains("cloud-sentinel"));
    }
}

#[test]
fn cli_freeze_modes_are_disjoint_and_live_pair_has_no_override_or_single_arm() {
    let replay = Cli::try_parse_from([
        "codex-ai-ip-eval",
        "freeze-run-context",
        "replay",
        "--repo-root",
        "/repo",
        "--fork-sha",
        "abc",
        "--private-root",
        "/private",
        "--codex-bin",
        "/bin/codex",
        "--case",
        "/fixtures/case.json",
        "--transcript",
        "/fixtures/transcript.jsonl",
        "--fixture-set-manifest",
        "/fixtures/set.json",
        "--output",
        "/private/frozen.json",
    ]);
    assert!(replay.is_ok());
    assert!(
        Cli::try_parse_from([
            "codex-ai-ip-eval",
            "freeze-run-context",
            "replay",
            "--repo-root",
            "/repo",
            "--fork-sha",
            "abc",
            "--private-root",
            "/private",
            "--codex-bin",
            "/bin/codex",
            "--case",
            "/fixtures/case.json",
            "--transcript",
            "/fixtures/transcript.jsonl",
            "--fixture-set-manifest",
            "/fixtures/set.json",
            "--output",
            "/private/frozen.json",
            "--provider-label",
            "forbidden",
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "codex-ai-ip-eval",
            "freeze-run-context",
            "live",
            "--repo-root",
            "/repo",
            "--evidence-repo-root",
            "/evidence",
            "--fork-sha",
            "abc",
            "--case",
            "/private/inputs/case.json",
            "--attestation",
            "/private/inputs/attestation.json",
            "--provider-budget-evidence",
            "/private/inputs/budget.json",
            "--rate-card",
            "/private/inputs/rate.json",
            "--billing-policy",
            "/private/inputs/billing.json",
            "--fx-policy",
            "/private/inputs/fx.json",
            "--codex-bin",
            "/bin/codex",
            "--lead-skill",
            "/repo/SKILL.md",
            "--model-label",
            "approved-model",
            "--provider-label",
            "approved-provider",
            "--provider-role",
            "targetVolcengine",
            "--provider-upstream-url",
            "https://provider.invalid/v1/responses",
            "--authorized-total-cost-fen",
            "100",
            "--authorized-per-run-cost-fen",
            "50",
            "--max-provider-request-attempts-per-run",
            "3",
            "--max-total-tokens-per-run",
            "1000",
            "--max-elapsed-seconds-per-run",
            "60",
            "--max-output-tokens-per-request",
            "321",
            "--private-root",
            "/private",
            "--output",
            "/private/coordinator/frozen.json",
        ])
        .is_ok()
    );
    assert!(
        Cli::try_parse_from([
            "codex-ai-ip-eval",
            "live-pair",
            "--frozen-run-context",
            "/private/frozen.json"
        ])
        .is_ok()
    );
    assert!(Cli::try_parse_from(["codex-ai-ip-eval", "replay-pair"]).is_ok());
    assert!(
        Cli::try_parse_from([
            "codex-ai-ip-eval",
            "live-pair",
            "--frozen-run-context",
            "/private/frozen.json",
            "--model",
            "override"
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["codex-ai-ip-eval", "run"]).is_err());
}

#[test]
fn frozen_artifacts_rehash_every_named_boundary_and_detect_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let mut paths = BTreeMap::new();
    for name in [
        "source",
        "materials",
        "codexBinary",
        "evaluatorBinary",
        "brokerSource",
        "config",
        "schema",
        "prompt",
        "skill",
        "threadStartRequest",
        "turnStartRequest",
        "receipt",
    ] {
        let path = temp.path().join(name);
        fs::write(&path, format!("{name}\n")).unwrap();
        paths.insert(name.to_string(), path);
    }
    let frozen = ArtifactCommitments::freeze(paths.clone()).unwrap();
    frozen.verify().unwrap();
    fs::write(paths.get("skill").unwrap(), b"changed\n").unwrap();
    assert!(frozen.verify().unwrap_err().to_string().contains("skill"));

    let expected = Sha256::digest(b"source\n");
    assert_eq!(frozen.sha256("source").unwrap(), format!("{expected:x}"));
}

#[test]
fn frozen_worktree_revalidates_exact_head_and_clean_status_at_each_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let repository = temp.path().join("repo");
    fs::create_dir(&repository).unwrap();
    fs::write(repository.join("source.rs"), b"frozen\n").unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "--quiet"]);
    git(&["add", "source.rs"]);
    git(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "freeze",
    ]);
    let frozen = GitWorktreeCommitment::freeze(&repository).unwrap();
    frozen.verify().unwrap();

    fs::write(repository.join("untracked"), b"drift\n").unwrap();
    assert!(
        frozen
            .verify()
            .unwrap_err()
            .to_string()
            .contains("not clean")
    );
}

#[cfg(unix)]
#[test]
fn frozen_artifacts_reject_hardlinks_and_symlink_substitution() {
    let temp = tempfile::tempdir().unwrap();
    let original = temp.path().join("original");
    let hardlink = temp.path().join("hardlink");
    fs::write(&original, b"same bytes\n").unwrap();
    fs::hard_link(&original, &hardlink).unwrap();
    assert!(
        ArtifactCommitments::freeze(BTreeMap::from([("source".to_string(), original.clone(),)]))
            .is_err()
    );

    fs::remove_file(&hardlink).unwrap();
    let frozen =
        ArtifactCommitments::freeze(BTreeMap::from([("source".to_string(), original.clone())]))
            .unwrap();
    let replacement = temp.path().join("replacement");
    fs::write(&replacement, b"same bytes\n").unwrap();
    fs::remove_file(&original).unwrap();
    std::os::unix::fs::symlink(&replacement, &original).unwrap();
    assert!(frozen.verify().is_err());
    assert!(
        ArtifactCommitments::freeze(BTreeMap::from([("source".to_string(), original,)])).is_err()
    );
}

#[cfg(unix)]
#[test]
fn live_context_and_receipt_directory_reject_initial_symlink_aliases() {
    let temp = tempfile::tempdir().unwrap();
    let real_context = temp.path().join("real-context.json");
    let context_alias = temp.path().join("context-alias.json");
    fs::write(&real_context, b"{\"executionMode\":\"live\"}\n").unwrap();
    std::os::unix::fs::symlink(&real_context, &context_alias).unwrap();
    assert!(verify_frozen_context(&context_alias).is_err());

    let external_receipts = temp.path().join("external-receipts");
    let receipt_alias = temp.path().join("receipt-alias");
    fs::create_dir(&external_receipts).unwrap();
    std::os::unix::fs::symlink(&external_receipts, &receipt_alias).unwrap();
    assert!(
        PairCoordinator::create(BrokerGateConfig {
            ledger_path: temp.path().join("unsafe-attempt-index.jsonl"),
            receipt_dir: receipt_alias,
            pair_id: "pair-unsafe".to_string(),
            frozen_run_context_sha256: "a".repeat(64),
            execution_context_sha256: "b".repeat(64),
            arm_order_commitment: "c".repeat(64),
            max_attempts_per_arm: 1,
            max_output_tokens: 1,
        })
        .is_err()
    );
}
