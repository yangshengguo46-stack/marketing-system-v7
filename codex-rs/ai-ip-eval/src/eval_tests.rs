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
use crate::BrokerRuntimeConfig;
use crate::Cli;
use crate::EvaluationCondition;
use crate::ExecutionBoundary;
use crate::ExecutionMode;
use crate::FrozenExecutionGuard;
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
use codex_responses_api_proxy::ExchangeObserver;
use codex_responses_api_proxy::ForwardResult;
use codex_responses_api_proxy::ObservedUsage;
use codex_responses_api_proxy::RequestGate;
use codex_responses_api_proxy::RequestMetadata;
use codex_responses_api_proxy::ResponseCompletedMetadata;
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
    let root = test_thread_id(root);
    RequestMetadata {
        method: "POST".to_string(),
        path: "/v1/responses".to_string(),
        window_id: Some(format!("{root}:0")),
        parent_thread_id: None,
        is_subagent: false,
    }
}

fn test_thread_id(label: &str) -> &'static str {
    match label {
        "root" => "0198f5aa-0000-7000-8000-000000000001",
        "root-1" => "0198f5aa-0000-7000-8000-000000000002",
        "root-2" => "0198f5aa-0000-7000-8000-000000000003",
        "child" => "0198f5aa-0000-7000-8000-000000000004",
        "grandchild" => "0198f5aa-0000-7000-8000-000000000005",
        "other" => "0198f5aa-0000-7000-8000-000000000006",
        "orphan" => "0198f5aa-0000-7000-8000-000000000007",
        "missing" => "0198f5aa-0000-7000-8000-000000000008",
        _ => panic!("unknown synthetic thread label"),
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
        runtime: std::sync::Arc::new(BrokerRuntimeConfig::new(cap, 321, 1024 * 1024).unwrap()),
    })
    .unwrap()
}

fn activate_first(gate: &PairCoordinator, root: &str, cap_window: Duration) {
    gate.commit_order_for_test(EvaluationCondition::Generic, EvaluationCondition::Candidate)
        .unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 1,
        condition: EvaluationCondition::Generic,
        root_thread_id: test_thread_id(root).to_string(),
        deadline: Instant::now() + cap_window,
        deadline_rfc3339: "2026-08-27T12:00:00Z".to_string(),
    })
    .unwrap();
}

fn observe_completion(gate: &PairCoordinator, permit: &codex_responses_api_proxy::RequestPermit) {
    gate.response_completed(
        permit,
        &ResponseCompletedMetadata {
            response_id: "response-1".to_string(),
            usage: Some(ObservedUsage {
                total_tokens: 3,
                input_tokens: 2,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
            }),
            actual_model: Some("mock-revision".to_string()),
            deployment_or_fingerprint: Some("mock-deployment".to_string()),
        },
    );
}

fn complete(gate: &PairCoordinator, permit: codex_responses_api_proxy::RequestPermit) {
    observe_completion(gate, &permit);
    gate.after_forward(permit, &ForwardResult::Completed { status: 200 });
}

fn second_sealed_pair(temp: &tempfile::TempDir) -> PairCoordinator {
    let gate = coordinator(temp, 1);
    activate_first(&gate, "root-1", Duration::from_secs(5));
    let first = gate
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    complete(&gate, first);
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    let second = gate
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    complete(&gate, second);
    gate.seal_arm().unwrap();
    gate
}

#[test]
fn pair_finish_rejects_teardown_failure_before_persisting_success() {
    let temp = tempfile::tempdir().unwrap();
    let gate = second_sealed_pair(&temp);

    let result = crate::runner::finish_pair_after_teardown(
        &gate,
        Instant::now() + Duration::from_secs(5),
        |_| Err(anyhow::anyhow!("injected broker teardown failure")),
        || Ok(()),
    );

    assert!(result.is_err());
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(!temp.path().join("receipts/pair-receipt.json").exists());
}

#[test]
fn pair_finish_rejects_deadline_or_final_rehash_before_persisting_success() {
    for failure in ["deadline", "rehash"] {
        let temp = tempfile::tempdir().unwrap();
        let gate = second_sealed_pair(&temp);
        let deadline = if failure == "deadline" {
            Instant::now()
        } else {
            Instant::now() + Duration::from_secs(5)
        };

        let result = crate::runner::finish_pair_after_teardown(
            &gate,
            deadline,
            |_| Ok(()),
            || match failure {
                "rehash" => Err(anyhow::anyhow!("injected final rehash failure")),
                "deadline" => Ok(()),
                _ => unreachable!(),
            },
        );

        assert!(result.is_err());
        assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
        assert!(!temp.path().join("receipts/pair-receipt.json").exists());
    }
}

#[test]
fn synchronous_pair_setup_is_deadline_checked_before_and_after_the_boundary() {
    let expired_before =
        crate::runner::run_sync_before_deadline(Instant::now(), || Ok::<_, anyhow::Error>(()));
    assert!(expired_before.is_err());

    let expired_after =
        crate::runner::run_sync_before_deadline(Instant::now() + Duration::from_millis(20), || {
            std::thread::sleep(Duration::from_millis(40));
            Ok::<_, anyhow::Error>(())
        });
    assert!(expired_after.is_err());
}

#[test]
fn absolute_pair_deadline_is_committed_before_full_context_verification() {
    let temp = tempfile::tempdir().unwrap();
    let context = temp.path().join("frozen-run-context.json");
    let mut value = json!({
        "schemaVersion": 1,
        "executionMode": "live",
        "providerMode": "not-run",
        "pairId": "a".repeat(64),
        "publicRunId": "b".repeat(64),
        "candidateSha": "candidate",
        "privateRoot": temp.path(),
        "repoRoot": temp.path(),
        "repoHead": "candidate",
        "providerUpstreamUrl": "http://127.0.0.1:1/v1/responses",
        "modelLabel": "mock",
        "maxOutputTokens": 1,
        "maxAttemptsPerArm": 1,
        "maxTotalTokens": 1,
        "maxElapsedSeconds": 1,
        "artifacts": {}
    });
    fs::write(&context, serde_json::to_vec(&value).unwrap()).unwrap();
    let started = Instant::now();
    let deadline = crate::runner::establish_pair_deadline(&context, started).unwrap();
    assert_eq!(deadline.duration_since(started), Duration::from_secs(1));

    value["maxElapsedSeconds"] = json!(60);
    fs::write(&context, serde_json::to_vec(&value).unwrap()).unwrap();
    assert_eq!(deadline.duration_since(started), Duration::from_secs(1));
}

#[test]
fn frozen_pair_total_token_ceiling_fails_the_terminal_and_poisons() {
    let temp = tempfile::tempdir().unwrap();
    let gate = PairCoordinator::create(BrokerGateConfig {
        ledger_path: temp.path().join("attempt-index.jsonl"),
        receipt_dir: temp.path().join("receipts"),
        pair_id: "pair-1".to_string(),
        frozen_run_context_sha256: "a".repeat(64),
        execution_context_sha256: "b".repeat(64),
        arm_order_commitment: "c".repeat(64),
        runtime: std::sync::Arc::new(
            BrokerRuntimeConfig::with_run_limits(1, 321, 1024 * 1024, 2).unwrap(),
        ),
    })
    .unwrap();
    activate_first(&gate, "root", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    complete(&gate, permit);

    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    let records: Vec<serde_json::Value> =
        fs::read_to_string(temp.path().join("attempt-index.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(records[1]["status"], json!("failed"));
    assert_eq!(records[1]["failureClass"], json!("maxTotalTokensExceeded"));
}

#[test]
fn successful_completion_requires_exactly_one_consistent_usage_report() {
    for usage in [
        None,
        Some(ObservedUsage {
            total_tokens: 9,
            input_tokens: 2,
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            output_tokens: 1,
            reasoning_output_tokens: 0,
        }),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let gate = coordinator(&temp, 1);
        activate_first(&gate, "root", Duration::from_secs(5));
        let permit = gate
            .before_forward(&root_request("root"), &transformed())
            .unwrap();
        gate.response_completed(
            &permit,
            &ResponseCompletedMetadata {
                response_id: "invalid-usage".to_string(),
                usage,
                actual_model: Some("mock-revision".to_string()),
                deployment_or_fingerprint: None,
            },
        );
        gate.after_forward(permit, &ForwardResult::Completed { status: 200 });
        assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
        let ledger = fs::read_to_string(temp.path().join("attempt-index.jsonl")).unwrap();
        assert!(ledger.contains("invalidResponseUsage"));
    }
}

#[test]
fn frozen_elapsed_ceiling_is_one_shared_arm_deadline_not_per_call() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let deadline = Instant::now() + Duration::from_millis(80);
        crate::runner::run_before_deadline(deadline, async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<_, anyhow::Error>(())
        })
        .await
        .unwrap();
        assert!(
            crate::runner::run_before_deadline(deadline, async {
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok::<_, anyhow::Error>(())
            })
            .await
            .is_err()
        );
    });
}

fn strict_live_context(temp: &tempfile::TempDir) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }
    let repository = temp.path().join("context-repo");
    fs::create_dir(&repository).unwrap();
    fs::write(repository.join("source"), b"committed source\n").unwrap();
    fs::create_dir_all(repository.join("codex-rs/responses-api-proxy/src")).unwrap();
    fs::write(
        repository.join("codex-rs/responses-api-proxy/src/broker.rs"),
        b"// synthetic committed broker source\n",
    )
    .unwrap();
    let run_git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    run_git(&["init", "--quiet"]);
    run_git(&[
        "add",
        "source",
        "codex-rs/responses-api-proxy/src/broker.rs",
    ]);
    run_git(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "freeze",
    ]);
    let head = String::from_utf8(run_git(&["rev-parse", "HEAD"]))
        .unwrap()
        .trim()
        .to_string();
    let artifacts_dir = temp.path().join("context-artifacts");
    let frozen_inputs = temp.path().join("frozen-inputs");
    let imported_inputs = temp.path().join("inputs");
    let case_root = imported_inputs.join("case");
    fs::create_dir(&artifacts_dir).unwrap();
    fs::create_dir(&frozen_inputs).unwrap();
    fs::create_dir_all(&case_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&frozen_inputs, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&imported_inputs, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&case_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let private_leaf = |name: &str| match name {
        "providerBudgetReceipt" => Some("provider-budget-receipt.json"),
        "rateCard" => Some("rate-card.json"),
        "billingPolicy" => Some("billing-policy.json"),
        "fxPolicy" => Some("fx-policy.json"),
        "skill" => Some("lead-skill.md"),
        "schema" => Some("content-package-schema.json"),
        "prompt" => Some("root-prompt.txt"),
        "threadStartRequest" => Some("thread-start-request.json"),
        "turnStartRequest" => Some("turn-start-request.json"),
        _ => None,
    };
    let case_bytes = serde_json::to_vec_pretty(&mission_case()).unwrap();
    let materials_bytes = serde_json::to_vec(&mission_case().materials).unwrap();
    let attestation_bytes = serde_json::to_vec_pretty(&json!({
        "caseSha256": format!("{:x}", Sha256::digest(&case_bytes)),
        "sourceMaterialsSha256": format!("{:x}", Sha256::digest(&materials_bytes)),
    }))
    .unwrap();
    let artifacts: BTreeMap<String, serde_json::Value> = crate::REQUIRED_EXECUTION_ARTIFACTS
        .iter()
        .map(|name| {
            let (path, bytes) = match *name {
                "evaluatorBinary" => {
                    let path = std::env::current_exe().unwrap().canonicalize().unwrap();
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                }
                "brokerSource" => {
                    let path = repository
                        .join("codex-rs/responses-api-proxy/src/broker.rs")
                        .canonicalize()
                        .unwrap();
                    let bytes = fs::read(&path).unwrap();
                    (path, bytes)
                }
                "source" => (case_root.join("case.json"), case_bytes.clone()),
                "attestation" => (
                    imported_inputs.join("held-out-attestation.json"),
                    attestation_bytes.clone(),
                ),
                "materials" => (
                    imported_inputs.join("materials-manifest.json"),
                    materials_bytes.clone(),
                ),
                _ => (
                    private_leaf(name)
                        .map(|leaf| frozen_inputs.join(leaf))
                        .unwrap_or_else(|| artifacts_dir.join(name)),
                    format!("{name}\n").into_bytes(),
                ),
            };
            if !path.exists() {
                fs::write(&path, &bytes).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
                }
            }
            let path = path.canonicalize().unwrap();
            (
                name.to_string(),
                json!({"path": path, "sha256": format!("{:x}", Sha256::digest(&bytes))}),
            )
        })
        .collect();
    let path = temp.path().join("frozen-run-context.json");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&json!({
            "schemaVersion": 1,
            "executionMode": "live",
            "providerMode": "not-run",
            "pairId": "a".repeat(64),
            "publicRunId": "b".repeat(64),
            "candidateSha": head,
            "privateRoot": temp.path().canonicalize().unwrap(),
            "repoRoot": repository.canonicalize().unwrap(),
            "repoHead": head,
            "providerUpstreamUrl": "http://127.0.0.1:1/v1/responses",
            "modelLabel": "local-mock",
            "maxOutputTokens": 321,
            "maxAttemptsPerArm": 2,
            "maxTotalTokens": 100,
            "maxElapsedSeconds": 5,
            "artifacts": artifacts,
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
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
    complete(&happy, first);
    let receipt1 = happy.seal_arm().unwrap();
    assert_eq!(happy.phase(), PairPhase::Sealed1);
    happy
        .activate_arm(ArmActivation {
            run_ordinal: 2,
            condition: EvaluationCondition::Candidate,
            root_thread_id: test_thread_id("root-2").to_string(),
            deadline: Instant::now() + Duration::from_secs(5),
            deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
        })
        .unwrap();
    let second = happy
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    complete(&happy, second);
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
    complete(&gate, root);

    gate.observe_lifecycle(ThreadLifecycle::Subagent {
        thread_id: test_thread_id("child").to_string(),
        parent_thread_id: test_thread_id("root").to_string(),
    })
    .unwrap();

    let child = RequestMetadata {
        method: "POST".to_string(),
        path: "/v1/responses".to_string(),
        window_id: Some(format!("{}:0", test_thread_id("child"))),
        parent_thread_id: Some(test_thread_id("root").to_string()),
        is_subagent: true,
    };
    let child_permit = gate.before_forward(&child, &transformed()).unwrap();
    complete(&gate, child_permit);
    assert!(gate.known_threads().contains(test_thread_id("child")));

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
            window_id: Some(format!("{}:0", test_thread_id("other"))),
            ..root_request("root")
        },
        RequestMetadata {
            window_id: Some(format!("{}:0", test_thread_id("orphan"))),
            parent_thread_id: Some(test_thread_id("missing").to_string()),
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
    for _ in 0..2 {
        let permit = gate
            .before_forward(&root_request("root"), &transformed())
            .unwrap();
        complete(&gate, permit);
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

#[cfg(unix)]
#[test]
fn ledger_identity_or_prefix_change_is_detected_before_arm_receipt_persistence() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 1);
    activate_first(&gate, "root", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    complete(&gate, permit);
    let ledger = temp.path().join("attempt-index.jsonl");
    fs::rename(&ledger, temp.path().join("detached-ledger.jsonl")).unwrap();
    fs::write(&ledger, b"replacement\n").unwrap();

    assert!(gate.seal_arm().is_err());
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(!temp.path().join("receipts/arm-1-receipt.json").exists());
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
    complete(&gate, permit);
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    let permit = gate
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    complete(&gate, permit);
    let second = gate.seal_arm().unwrap();
    let first_bytes = fs::read(temp.path().join("receipts/arm-1-receipt.json")).unwrap();
    assert_eq!(
        second.previous_arm_receipt_sha256,
        Some(format!("{:x}", Sha256::digest(first_bytes)))
    );
}

#[cfg(unix)]
#[test]
fn pair_finish_revalidates_actual_generated_arm_receipts_before_success() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 1);
    activate_first(&gate, "root-1", Duration::from_secs(5));
    let first = gate
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    complete(&gate, first);
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    let second = gate
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    complete(&gate, second);
    gate.seal_arm().unwrap();
    fs::write(
        temp.path().join("receipts/arm-1-receipt.json"),
        b"tampered\n",
    )
    .unwrap();

    assert!(gate.finish().is_err());
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(!temp.path().join("receipts/pair-receipt.json").exists());
}

#[test]
fn arm_order_uses_an_owner_only_os_seed_after_context_verification() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = strict_live_context(&temp);
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let order_dir = temp.path().join("order");
    let order = commit_arm_order(&frozen, &order_dir).unwrap();
    assert_ne!(order.first(), order.second());
    assert_eq!(
        fs::read(order_dir.join("arm-order-seed.bin"))
            .unwrap()
            .len(),
        32
    );
    assert_eq!(order.seed_commitment().len(), 64);
    assert!(commit_arm_order(&frozen, &order_dir).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(order_dir.join("arm-order-seed.bin"))
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
    let frozen_path = strict_live_context(&temp);
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
        frozen_run_context_sha256: frozen.sha256().to_string(),
        execution_context_sha256: "b".repeat(64),
        arm_order_commitment: order.seed_commitment().to_string(),
        runtime: std::sync::Arc::new(BrokerRuntimeConfig::new(1, 1, 1024).unwrap()),
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
            "--material-root",
            "/private/materials",
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

fn write_nonempty_source_fixture(
    root: &std::path::Path,
    relative_path: &str,
    declared_digest: Option<&str>,
    attested_case_digest: Option<&str>,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let material_root = root.join("source-materials");
    fs::create_dir(&material_root).unwrap();
    let material_path = material_root.join(relative_path);
    if let Some(parent) = material_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&material_path, b"source-visible evidence\n").unwrap();
    let actual_digest = format!("{:x}", Sha256::digest(b"source-visible evidence\n"));
    let digest = declared_digest.unwrap_or(&actual_digest);
    let case_value = json!({
        "caseId": "held-out-material-case",
        "objective": "Use the declared evidence without changing it",
        "subjectKind": "brand",
        "constraints": [],
        "materials": [{
            "materialId": "evidence-1",
            "relativePath": relative_path,
            "sha256": digest,
            "materialKind": "evidence"
        }]
    });
    let case_bytes = serde_json::to_vec_pretty(&case_value).unwrap();
    let case_path = root.join("external-case.json");
    fs::write(&case_path, &case_bytes).unwrap();
    let materials_bytes = format!(
        "[{{\"materialId\":\"evidence-1\",\"relativePath\":{},\"sha256\":\"{}\",\"materialKind\":\"evidence\"}}]",
        serde_json::to_string(relative_path).unwrap(),
        digest,
    );
    let attestation = json!({
        "caseSha256": attested_case_digest
            .map(str::to_string)
            .unwrap_or_else(|| format!("{:x}", Sha256::digest(&case_bytes))),
        "sourceMaterialsSha256": format!("{:x}", Sha256::digest(materials_bytes.as_bytes()))
    });
    let attestation_path = root.join("external-attestation.json");
    fs::write(
        &attestation_path,
        serde_json::to_vec_pretty(&attestation).unwrap(),
    )
    .unwrap();
    (case_path, material_root, attestation_path)
}

#[test]
fn live_freeze_imports_nonempty_case_attestation_and_declared_materials() {
    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);

    let imported =
        crate::runner::import_live_source_proof(&case, &material_root, &attestation, &private_root)
            .unwrap();

    assert_eq!(
        imported.case_path,
        private_root
            .join("inputs/case/case.json")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(
        imported.attestation_path,
        private_root
            .join("inputs/held-out-attestation.json")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(
        imported.material_paths,
        BTreeMap::from([(
            "evidence-1".to_string(),
            private_root
                .join("inputs/case/notes/evidence.txt")
                .canonicalize()
                .unwrap(),
        )])
    );
    assert_eq!(
        fs::read(imported.material_paths["evidence-1"].clone()).unwrap(),
        b"source-visible evidence\n"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in imported
            .material_paths
            .values()
            .chain([&imported.case_path, &imported.attestation_path])
        {
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}

#[test]
fn live_freeze_rejects_material_digest_path_type_and_attestation_drift() {
    for mutation in ["digest", "path", "type", "attestation"] {
        let temp = tempfile::tempdir().unwrap();
        let private_root = temp.path().join("private-proof");
        fs::create_dir(&private_root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let relative = if mutation == "path" {
            "../escape.txt"
        } else {
            "notes/evidence.txt"
        };
        let declared_digest = (mutation == "digest")
            .then_some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let attested_case_digest = (mutation == "attestation")
            .then_some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        let (case, material_root, attestation) = write_nonempty_source_fixture(
            temp.path(),
            relative,
            declared_digest,
            attested_case_digest,
        );
        if mutation == "type" {
            let material = material_root.join(relative);
            fs::remove_file(&material).unwrap();
            fs::create_dir(&material).unwrap();
        }

        assert!(
            crate::runner::import_live_source_proof(
                &case,
                &material_root,
                &attestation,
                &private_root,
            )
            .is_err(),
            "mutation {mutation}"
        );
        assert!(!private_root.join("inputs/case/case.json").exists());
    }
}

#[test]
fn live_freeze_rejects_a_prepopulated_managed_case_tree() {
    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir_all(private_root.join("inputs/case")).unwrap();
    fs::write(
        private_root.join("inputs/case/extra-file"),
        b"attacker bytes\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            private_root.as_path(),
            &private_root.join("inputs"),
            &private_root.join("inputs/case"),
        ] {
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);

    assert!(
        crate::runner::import_live_source_proof(
            &case,
            &material_root,
            &attestation,
            &private_root,
        )
        .is_err()
    );
}

#[test]
fn live_freeze_rejects_unknown_attestation_fields() {
    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation).unwrap()).unwrap();
    value["unapprovedField"] = json!(true);
    fs::write(&attestation, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    assert!(
        crate::runner::import_live_source_proof(
            &case,
            &material_root,
            &attestation,
            &private_root,
        )
        .is_err()
    );
    assert!(!private_root.join("inputs").exists());
}

#[cfg(unix)]
#[test]
fn live_freeze_import_is_anchored_against_parent_directory_substitution() {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir(&private_root).unwrap();
    fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    let external = temp.path().join("attacker-directory");
    fs::create_dir(&external).unwrap();
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);

    let result = crate::runner::import_live_source_proof_with_hook(
        &case,
        &material_root,
        &attestation,
        &private_root,
        |case_root| {
            fs::rename(case_root.join("notes"), case_root.join("retained-notes"))?;
            symlink(&external, case_root.join("notes"))?;
            Ok(())
        },
    );

    assert!(result.is_err());
    assert!(!external.join("evidence.txt").exists());
}

#[test]
fn managed_source_tree_rejects_every_undeclared_entry() {
    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);
    let imported =
        crate::runner::import_live_source_proof(&case, &material_root, &attestation, &private_root)
            .unwrap();
    fs::write(
        private_root.join("inputs/case/extra-file"),
        b"attacker bytes\n",
    )
    .unwrap();

    assert!(crate::runner::validate_imported_source_proof(&imported).is_err());
}

#[test]
fn between_arm_managed_tree_mutation_poisons_without_a_pair_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let private_root = temp.path().join("private-proof");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (case, material_root, attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);
    let imported =
        crate::runner::import_live_source_proof(&case, &material_root, &attestation, &private_root)
            .unwrap();
    let gate = coordinator(&temp, 1);
    activate_first(&gate, "root-1", Duration::from_secs(5));
    let first = gate
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    complete(&gate, first);
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    fs::write(
        imported.case_path.parent().unwrap().join("extra-file"),
        b"between-arm mutation\n",
    )
    .unwrap();

    assert!(
        crate::runner::validate_imported_source_before_arm(
            &imported,
            &gate,
            Instant::now() + Duration::from_secs(5),
        )
        .is_err()
    );
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(!temp.path().join("receipts/pair-receipt.json").exists());
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

#[cfg(unix)]
#[test]
fn artifact_consumption_is_anchored_to_the_verified_handle_and_digest() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("original-parent");
    fs::create_dir(&parent).unwrap();
    let artifact = parent.join("source");
    fs::write(&artifact, b"trusted\n").unwrap();
    let frozen =
        ArtifactCommitments::freeze(BTreeMap::from([("source".to_string(), artifact)])).unwrap();
    assert_eq!(frozen.read_verified("source").unwrap(), b"trusted\n");

    let moved = temp.path().join("moved-parent");
    fs::rename(&parent, &moved).unwrap();
    fs::create_dir(&parent).unwrap();
    fs::write(parent.join("source"), b"substituted\n").unwrap();
    assert!(frozen.read_verified("source").is_err());
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
            runtime: std::sync::Arc::new(BrokerRuntimeConfig::new(1, 1, 1024).unwrap()),
        })
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn receipt_writes_remain_anchored_if_the_parent_path_is_replaced() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 1);
    activate_first(&gate, "root", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    complete(&gate, permit);
    let anchored = temp.path().join("anchored-receipts");
    fs::rename(temp.path().join("receipts"), &anchored).unwrap();
    let external = temp.path().join("external-receipts-after-create");
    fs::create_dir(&external).unwrap();
    std::os::unix::fs::symlink(&external, temp.path().join("receipts")).unwrap();

    gate.seal_arm().unwrap();
    assert!(anchored.join("arm-1-receipt.json").is_file());
    assert!(fs::read_dir(external).unwrap().next().is_none());
}

#[test]
fn descendant_forward_requires_a_prior_immutable_lifecycle_edge() {
    let child_request = RequestMetadata {
        method: "POST".to_string(),
        path: "/v1/responses".to_string(),
        window_id: Some(format!("{}:0", test_thread_id("child"))),
        parent_thread_id: Some(test_thread_id("root").to_string()),
        is_subagent: true,
    };

    let rejected_temp = tempfile::tempdir().unwrap();
    let rejected = coordinator(&rejected_temp, 2);
    activate_first(&rejected, "root", Duration::from_secs(5));
    assert!(
        rejected
            .before_forward(&child_request, &transformed())
            .is_err()
    );
    assert!(matches!(rejected.phase(), PairPhase::Poisoned { .. }));

    let accepted_temp = tempfile::tempdir().unwrap();
    let accepted = coordinator(&accepted_temp, 2);
    activate_first(&accepted, "root", Duration::from_secs(5));
    accepted
        .observe_lifecycle(ThreadLifecycle::Subagent {
            thread_id: test_thread_id("child").to_string(),
            parent_thread_id: test_thread_id("root").to_string(),
        })
        .unwrap();
    let permit = accepted
        .before_forward(&child_request, &transformed())
        .unwrap();
    complete(&accepted, permit);

    assert!(
        accepted
            .observe_lifecycle(ThreadLifecycle::Subagent {
                thread_id: test_thread_id("child").to_string(),
                parent_thread_id: test_thread_id("root").to_string(),
            })
            .is_err()
    );
    assert!(matches!(accepted.phase(), PairPhase::Poisoned { .. }));
}

#[test]
fn canonical_protocol_thread_and_window_ids_are_required() {
    for malformed in [
        "0198F5AA-0000-7000-8000-000000000001:0",
        "0198f5aa000070008000000000000001:0",
        "0198f5aa-0000-7000-8000-000000000001:00",
        "not-a-thread:0",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let gate = coordinator(&temp, 1);
        activate_first(&gate, "root", Duration::from_secs(5));
        let request = RequestMetadata {
            window_id: Some(malformed.to_string()),
            ..root_request("root")
        };
        assert!(gate.before_forward(&request, &transformed()).is_err());
        assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    }
}

#[test]
fn completed_forward_requires_exactly_one_response_completed() {
    let missing_temp = tempfile::tempdir().unwrap();
    let missing = coordinator(&missing_temp, 1);
    activate_first(&missing, "root", Duration::from_secs(5));
    let permit = missing
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    missing.after_forward(permit, &ForwardResult::Completed { status: 200 });
    assert!(matches!(missing.phase(), PairPhase::Poisoned { .. }));
    let records: Vec<serde_json::Value> =
        fs::read_to_string(missing_temp.path().join("attempt-index.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(records[1]["status"], json!("failed"));
    assert_eq!(
        records[1]["failureClass"],
        json!("missingResponseCompleted")
    );

    let duplicate_temp = tempfile::tempdir().unwrap();
    let duplicate = coordinator(&duplicate_temp, 1);
    activate_first(&duplicate, "root", Duration::from_secs(5));
    let permit = duplicate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    observe_completion(&duplicate, &permit);
    duplicate.response_completed(
        &permit,
        &ResponseCompletedMetadata {
            response_id: "response-duplicate".to_string(),
            usage: None,
            actual_model: None,
            deployment_or_fingerprint: None,
        },
    );
    duplicate.after_forward(permit, &ForwardResult::Completed { status: 200 });
    assert!(matches!(duplicate.phase(), PairPhase::Poisoned { .. }));
    let ledger = fs::read_to_string(duplicate_temp.path().join("attempt-index.jsonl")).unwrap();
    assert!(ledger.contains("duplicateResponseCompleted"));
}

#[test]
fn terminal_state_commits_only_after_terminal_fsync() {
    let temp = tempfile::tempdir().unwrap();
    let gate = coordinator(&temp, 1);
    activate_first(&gate, "root", Duration::from_secs(5));
    let permit = gate
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    observe_completion(&gate, &permit);
    gate.fail_next_terminal_append_for_test();
    gate.after_forward(permit, &ForwardResult::Completed { status: 200 });

    assert_eq!(gate.in_flight_count(), 1);
    assert_eq!(gate.attempt_count(), 0);
    let ledger = fs::read_to_string(temp.path().join("attempt-index.jsonl")).unwrap();
    assert_eq!(ledger.lines().count(), 1);
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
}

#[test]
fn receipt_persistence_failures_immediately_poison_and_are_not_retryable() {
    let arm_temp = tempfile::tempdir().unwrap();
    let arm = coordinator(&arm_temp, 1);
    activate_first(&arm, "root", Duration::from_secs(5));
    let permit = arm
        .before_forward(&root_request("root"), &transformed())
        .unwrap();
    complete(&arm, permit);
    fs::write(
        arm_temp.path().join("receipts/arm-1-receipt.json"),
        b"collision",
    )
    .unwrap();
    assert!(arm.seal_arm().is_err());
    assert!(matches!(arm.phase(), PairPhase::Poisoned { .. }));
    assert!(arm_temp.path().join("receipts/poison.json").is_file());
    assert!(arm.seal_arm().is_err());

    let pair_temp = tempfile::tempdir().unwrap();
    let pair = coordinator(&pair_temp, 1);
    activate_first(&pair, "root-1", Duration::from_secs(5));
    let permit = pair
        .before_forward(&root_request("root-1"), &transformed())
        .unwrap();
    complete(&pair, permit);
    pair.seal_arm().unwrap();
    pair.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline: Instant::now() + Duration::from_secs(5),
        deadline_rfc3339: "2026-08-27T12:01:00Z".to_string(),
    })
    .unwrap();
    let permit = pair
        .before_forward(&root_request("root-2"), &transformed())
        .unwrap();
    complete(&pair, permit);
    pair.seal_arm().unwrap();
    fs::write(
        pair_temp.path().join("receipts/pair-receipt.json"),
        b"collision",
    )
    .unwrap();
    assert!(pair.finish().is_err());
    assert!(matches!(pair.phase(), PairPhase::Poisoned { .. }));
    assert!(pair.finish().is_err());
}

#[test]
fn isolated_homes_require_fresh_empty_roots_and_complete_tree_parity() {
    let stale = tempfile::tempdir().unwrap();
    fs::create_dir(stale.path().join("generic-home")).unwrap();
    fs::write(stale.path().join("generic-home/auth.json"), b"stale").unwrap();
    assert!(
        prepare_isolated_homes(
            stale.path(),
            b"model = \"mock\"\n",
            "target-skill",
            b"skill\n",
        )
        .is_err()
    );

    let fresh = tempfile::tempdir().unwrap();
    let homes = prepare_isolated_homes(
        fresh.path(),
        b"model = \"mock\"\n",
        "target-skill",
        b"skill\n",
    )
    .unwrap();
    crate::verify_isolated_home_parity(&homes, "target-skill", b"skill\n").unwrap();
    fs::write(homes.generic_codex_home.join("unexpected"), b"drift").unwrap();
    assert!(crate::verify_isolated_home_parity(&homes, "target-skill", b"skill\n").is_err());
}

#[test]
fn minimal_live_context_and_skipped_rehash_boundaries_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let minimal = temp.path().join("minimal.json");
    fs::write(&minimal, b"{\"executionMode\":\"live\"}\n").unwrap();
    assert!(verify_frozen_context(&minimal).is_err());

    let mut paths = BTreeMap::new();
    for name in crate::REQUIRED_EXECUTION_ARTIFACTS {
        let path = temp.path().join(name);
        fs::write(&path, format!("{name}\n")).unwrap();
        paths.insert(name.to_string(), path);
    }
    let repository = temp.path().join("repo");
    fs::create_dir(&repository).unwrap();
    fs::write(repository.join("source"), b"source\n").unwrap();
    let run_git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    run_git(&["init", "--quiet"]);
    run_git(&["add", "source"]);
    run_git(&[
        "-c",
        "user.name=Synthetic",
        "-c",
        "user.email=synthetic@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "freeze",
    ]);
    let mut guard = FrozenExecutionGuard::create(paths, &repository).unwrap();
    guard.advance(ExecutionBoundary::ContextFrozen).unwrap();
    assert!(guard.advance(ExecutionBoundary::Arm1Pre).is_err());
}

#[test]
fn hand_authored_external_upstream_and_identity_drift_fail_before_activation() {
    for mutation in [
        "external",
        "candidate",
        "pair",
        "public",
        "artifactPath",
        "evaluatorPath",
        "brokerPath",
        "privateRoot",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = strict_live_context(&temp);
        let mut context: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match mutation {
            "external" => {
                context["providerUpstreamUrl"] = json!("https://example.invalid/v1/responses")
            }
            "candidate" => context["candidateSha"] = json!("different-head"),
            "pair" => context["pairId"] = json!("not-a-canonical-id"),
            "public" => context["publicRunId"] = json!("ABCDEF"),
            "artifactPath" => {
                let source = context["artifacts"]["source"]["path"].as_str().unwrap();
                let source = std::path::Path::new(source);
                context["artifacts"]["source"]["path"] = json!(
                    source
                        .parent()
                        .unwrap()
                        .join("..")
                        .join(source.parent().unwrap().file_name().unwrap())
                        .join(source.file_name().unwrap())
                );
            }
            "evaluatorPath" => {
                context["artifacts"]["evaluatorBinary"] = context["artifacts"]["source"].clone();
            }
            "brokerPath" => {
                context["artifacts"]["brokerSource"] = context["artifacts"]["source"].clone();
            }
            "privateRoot" => {
                let private_root = std::path::Path::new(context["privateRoot"].as_str().unwrap());
                context["privateRoot"] = json!(
                    private_root
                        .parent()
                        .unwrap()
                        .join("..")
                        .join(private_root.parent().unwrap().file_name().unwrap())
                        .join(private_root.file_name().unwrap())
                );
            }
            _ => unreachable!(),
        }
        fs::write(&path, serde_json::to_vec_pretty(&context).unwrap()).unwrap();
        assert!(verify_frozen_context(&path).is_err(), "mutation {mutation}");
        assert!(!temp.path().join("coordinator").exists());
    }
}

struct IntegrationInspector;

impl codex_responses_api_proxy::RequestInspector for IntegrationInspector {
    fn inspect(&self, body: &serde_json::Value) -> anyhow::Result<TransformedRequestEvidence> {
        let digest: [u8; 32] = Sha256::digest(serde_json::to_vec(body)?).into();
        Ok(TransformedRequestEvidence {
            raw_sha256: digest,
            normalized_sha256: digest,
            normalized_base_commitment: digest,
            treatment_diff_commitment: None,
        })
    }
}

#[test]
fn shared_runtime_drives_exact_proxy_transform_and_rejections_never_reach_upstream() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::thread;

    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    let count_worker = count.clone();
    let observed_worker = observed.clone();
    let upstream_worker = thread::spawn(move || {
        while let Some(mut request) = upstream.recv_timeout(Duration::from_secs(2)).unwrap() {
            count_worker.fetch_add(1, Ordering::SeqCst);
            let mut body = Vec::new();
            request.as_reader().read_to_end(&mut body).unwrap();
            observed_worker.lock().unwrap().push(body);
            let response = tiny_http::Response::from_string(concat!(
                "data: {\"type\":\"response.completed\",\"response\":{",
                "\"id\":\"mock-response\",\"model\":\"mock-revision\",",
                "\"system_fingerprint\":\"mock-deployment\",",
                "\"usage\":{\"input_tokens\":2,\"output_tokens\":1,\"total_tokens\":3}}}\n\n"
            ))
            .with_header(
                tiny_http::Header::from_bytes("content-type", "text/event-stream").unwrap(),
            );
            request.respond(response).unwrap();
        }
    });

    let temp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(BrokerRuntimeConfig::new(1, 77, 4096).unwrap());
    let gate = Arc::new(
        PairCoordinator::create(BrokerGateConfig {
            ledger_path: temp.path().join("attempt-index.jsonl"),
            receipt_dir: temp.path().join("receipts"),
            pair_id: "pair-loopback".to_string(),
            frozen_run_context_sha256: "a".repeat(64),
            execution_context_sha256: "b".repeat(64),
            arm_order_commitment: "c".repeat(64),
            runtime: runtime.clone(),
        })
        .unwrap(),
    );
    activate_first(&gate, "root", Duration::from_secs(5));
    let config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap(),
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: Some(runtime.request_transform(Arc::new(IntegrationInspector))),
    };
    let bound = codex_responses_api_proxy::bind(&config).unwrap();
    let proxy = codex_responses_api_proxy::activate(
        bound,
        config,
        codex_responses_api_proxy::local_mock_auth_header(),
        gate.clone(),
        gate,
    )
    .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let url = format!("http://{}/v1/responses", proxy.addr());
    let response = client
        .post(&url)
        .header("x-codex-window-id", format!("{}:0", test_thread_id("root")))
        .json(&json!({"model":"mock","input":[]}))
        .send()
        .unwrap();
    assert!(response.status().is_success());
    let _ = response.text().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let bodies = observed.lock().unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bodies[0]).unwrap(),
        json!({"model":"mock","input":[],"max_output_tokens":77})
    );
    drop(bodies);

    for rejected in [
        client
            .post(&url)
            .header("x-codex-window-id", format!("{}:1", test_thread_id("root"))),
        client
            .post(&url)
            .header("x-codex-window-id", "not-canonical:0"),
        client
            .post(format!("http://{}/v1/chat/completions", proxy.addr()))
            .header("x-codex-window-id", format!("{}:0", test_thread_id("root"))),
        client
            .post(&url)
            .header(
                "x-codex-window-id",
                format!("{}:0", test_thread_id("child")),
            )
            .header("x-codex-parent-thread-id", test_thread_id("root"))
            .header("x-openai-subagent", "collab_spawn"),
    ] {
        let response = rejected
            .json(&json!({"model":"mock","input":[]}))
            .send()
            .unwrap();
        assert!(!response.status().is_success());
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    proxy.shutdown_with_timeout(Duration::from_secs(2)).unwrap();
    upstream_worker.join().unwrap();

    for rejection in [
        LoopbackRejection::WrongPath,
        LoopbackRejection::MalformedWindow,
        LoopbackRejection::UnknownDescendant,
        LoopbackRejection::ExpiredDeadline,
        LoopbackRejection::AttemptCap,
    ] {
        assert_loopback_rejection_never_forwards(rejection);
    }
}

#[derive(Clone, Copy)]
enum LoopbackRejection {
    WrongPath,
    MalformedWindow,
    UnknownDescendant,
    ExpiredDeadline,
    AttemptCap,
}

fn assert_loopback_rejection_never_forwards(rejection: LoopbackRejection) {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let worker_count = count.clone();
    let worker = std::thread::spawn(move || {
        if let Some(request) = upstream.recv_timeout(Duration::from_millis(500)).unwrap() {
            worker_count.fetch_add(1, Ordering::SeqCst);
            request
                .respond(tiny_http::Response::from_string("unexpected"))
                .unwrap();
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let runtime = Arc::new(BrokerRuntimeConfig::new(1, 55, 4096).unwrap());
    let gate = Arc::new(
        PairCoordinator::create(BrokerGateConfig {
            ledger_path: temp.path().join("attempt-index.jsonl"),
            receipt_dir: temp.path().join("receipts"),
            pair_id: "pair-rejection".to_string(),
            frozen_run_context_sha256: "a".repeat(64),
            execution_context_sha256: "b".repeat(64),
            arm_order_commitment: "c".repeat(64),
            runtime: runtime.clone(),
        })
        .unwrap(),
    );
    activate_first(
        &gate,
        "root",
        if matches!(rejection, LoopbackRejection::ExpiredDeadline) {
            Duration::ZERO
        } else {
            Duration::from_secs(5)
        },
    );
    if matches!(rejection, LoopbackRejection::AttemptCap) {
        let permit = gate
            .before_forward(&root_request("root"), &transformed())
            .unwrap();
        complete(&gate, permit);
    }
    let config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap(),
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: Some(runtime.request_transform(Arc::new(IntegrationInspector))),
    };
    let bound = codex_responses_api_proxy::bind(&config).unwrap();
    let proxy = codex_responses_api_proxy::activate(
        bound,
        config,
        codex_responses_api_proxy::local_mock_auth_header(),
        gate.clone(),
        gate,
    )
    .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let path = if matches!(rejection, LoopbackRejection::WrongPath) {
        "/v1/chat/completions"
    } else {
        "/v1/responses"
    };
    let mut request = client
        .post(format!("http://{}{path}", proxy.addr()))
        .header(
            "x-codex-window-id",
            if matches!(rejection, LoopbackRejection::MalformedWindow) {
                "not-canonical:0".to_string()
            } else if matches!(rejection, LoopbackRejection::UnknownDescendant) {
                format!("{}:0", test_thread_id("child"))
            } else {
                format!("{}:0", test_thread_id("root"))
            },
        );
    if matches!(rejection, LoopbackRejection::UnknownDescendant) {
        request = request
            .header("x-codex-parent-thread-id", test_thread_id("root"))
            .header("x-openai-subagent", "collab_spawn");
    }
    let response = request
        .json(&json!({"model":"mock","input":[]}))
        .send()
        .unwrap();
    assert!(!response.status().is_success());
    proxy.shutdown_with_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[test]
fn native_app_server_fixture() {
    use std::fs::OpenOptions;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::io::Write;
    use std::os::fd::FromRawFd as _;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    if std::env::var_os("AI_IP_NATIVE_APP_SERVER_FIXTURE").is_none() {
        return;
    }

    fn send(output: &mut fs::File, value: &serde_json::Value) {
        serde_json::to_writer(&mut *output, value).unwrap();
        output.write_all(b"\n").unwrap();
        output.flush().unwrap();
    }

    fn thread(
        id: &str,
        parent: Option<&str>,
        source: Option<&str>,
        candidate: bool,
        cwd: &std::path::Path,
    ) -> serde_json::Value {
        json!({
            "id": id,
            "extra": null,
            "sessionId": if candidate { "session-candidate" } else { "session-generic" },
            "forkedFromId": null,
            "parentThreadId": parent,
            "preview": "",
            "ephemeral": false,
            "section": null,
            "sectionEnteredAt": null,
            "projectId": null,
            "historyMode": "legacy",
            "modelProvider": "ai-ip-proof-broker",
            "createdAt": 0,
            "updatedAt": 0,
            "recencyAt": null,
            "status": {"type": "idle"},
            "path": null,
            "cwd": cwd,
            "cliVersion": "mock",
            "source": "appServer",
            "canAcceptDirectInput": true,
            "threadSource": source,
            "agentNickname": null,
            "agentRole": null,
            "gitInfo": null,
            "name": null,
            "turns": []
        })
    }

    fn turn(id: &str, status: &str) -> serde_json::Value {
        json!({
            "id": id,
            "items": [],
            "itemsView": "full",
            "status": status,
            "error": null,
            "startedAt": null,
            "completedAt": null,
            "durationMs": 1
        })
    }

    fn call_broker(
        output: &mut fs::File,
        config: &serde_json::Value,
        thread_id: &str,
        turn_id: &str,
        parent_id: Option<&str>,
    ) {
        let base_url = config["model_providers"]["ai-ip-proof-broker"]["base_url"]
            .as_str()
            .unwrap();
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .build()
            .unwrap();
        let mut request = client
            .post(format!("{base_url}/responses"))
            .header("content-type", "application/json")
            .header("x-codex-window-id", format!("{thread_id}:0"));
        if let Some(parent_id) = parent_id {
            request = request
                .header("x-codex-parent-thread-id", parent_id)
                .header("x-openai-subagent", "collab_spawn");
        }
        let body = request
            .body(r#"{"model":"local-mock","input":[]}"#)
            .send()
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .unwrap();
        let completed: serde_json::Value = body
            .lines()
            .find_map(|line| line.strip_prefix("data: "))
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .unwrap();
        let response = &completed["response"];
        let usage = &response["usage"];
        send(
            output,
            &json!({
                "method": "rawResponse/completed",
                "params": {
                    "threadId": thread_id,
                    "turnId": turn_id,
                    "responseId": response["id"],
                    "usage": {
                        "totalTokens": usage["total_tokens"],
                        "inputTokens": usage["input_tokens"],
                        "cachedInputTokens": 0,
                        "cacheWriteInputTokens": 0,
                        "outputTokens": usage["output_tokens"],
                        "reasoningOutputTokens": 0
                    }
                }
            }),
        );
    }

    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let codex_home = std::path::PathBuf::from(std::env::var_os("CODEX_HOME").unwrap());
    let mut launch_keys = std::env::vars_os()
        .map(|(key, _)| key.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    launch_keys.sort();
    fs::write(
        home.join("app-server-launch.json"),
        serde_json::to_vec(&json!({
            "argv": std::env::args().skip(1).collect::<Vec<_>>(),
            "home": home,
            "codexHome": codex_home,
            "path": std::env::var("PATH").unwrap(),
            "envKeys": launch_keys
        }))
        .unwrap(),
    )
    .unwrap();
    let config_toml = fs::read_to_string(codex_home.join("config.toml")).unwrap();
    let config: serde_json::Value =
        serde_json::to_value(toml::from_str::<toml::Value>(&config_toml).unwrap()).unwrap();
    let candidate = home.to_string_lossy().contains("candidate-home");
    let thread_id = if candidate {
        "0198f5aa-0000-7000-8000-000000000102"
    } else {
        "0198f5aa-0000-7000-8000-000000000101"
    };
    let child_id = if candidate {
        "0198f5aa-0000-7000-8000-000000000202"
    } else {
        "0198f5aa-0000-7000-8000-000000000201"
    };
    let eval_root = home.parent().unwrap().to_path_buf();
    let mut root = thread(thread_id, None, None, candidate, &eval_root);
    let mut child = thread(
        child_id,
        Some(thread_id),
        Some("subagent"),
        candidate,
        &eval_root,
    );
    let stdin = BufReader::new(std::io::stdin().lock());
    let mut output = unsafe { fs::File::from_raw_fd(3) };

    for line in stdin.lines() {
        let message: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let method = message.get("method").and_then(serde_json::Value::as_str);
        writeln!(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(home.join("app-server-methods.log"))
                .unwrap(),
            "{}",
            method.unwrap_or("None")
        )
        .unwrap();
        let Some(request_id) = message.get("id") else {
            continue;
        };
        let result = match method {
            Some("initialize") => json!({
                "userAgent": "mock-app-server",
                "codexHome": codex_home,
                "platformFamily": "unix",
                "platformOs": "mock"
            }),
            Some("config/read") => json!({
                "config": config,
                "origins": {},
                "layers": [{
                    "name": {"type": "user", "file": codex_home.join("config.toml"), "profile": null},
                    "version": "v1",
                    "config": config
                }]
            }),
            Some("configRequirements/read") => json!({"requirements": null}),
            Some("thread/start") => {
                let cwd = std::path::PathBuf::from(message["params"]["cwd"].as_str().unwrap());
                root["cwd"] = json!(cwd);
                child["cwd"] = json!(cwd);
                json!({
                    "thread": root,
                    "model": "local-mock",
                    "modelProvider": "ai-ip-proof-broker",
                    "serviceTier": null,
                    "cwd": cwd,
                    "runtimeWorkspaceRoots": [],
                    "instructionSources": [],
                    "approvalPolicy": "never",
                    "approvalsReviewer": "user",
                    "sandbox": {"type": "dangerFullAccess"},
                    "activePermissionProfile": {"id": "ai-ip-eval", "extends": null},
                    "reasoningEffort": null,
                    "multiAgentMode": "explicitRequestOnly"
                })
            }
            Some("turn/start") => {
                let turn_id = if candidate {
                    "turn-candidate"
                } else {
                    "turn-generic"
                };
                let child_turn_id = format!("child-{turn_id}");
                send(
                    &mut output,
                    &json!({"id": request_id, "result": {"turn": turn(turn_id, "inProgress")}}),
                );
                send(
                    &mut output,
                    &json!({"method": "turn/started", "params": {"threadId": thread_id, "turn": turn(turn_id, "inProgress")}}),
                );
                send(
                    &mut output,
                    &json!({"method": "thread/started", "params": {"thread": child}}),
                );
                send(
                    &mut output,
                    &json!({"method": "turn/started", "params": {"threadId": child_id, "turn": turn(&child_turn_id, "inProgress")}}),
                );
                std::thread::sleep(Duration::from_millis(200));
                call_broker(&mut output, &config, thread_id, turn_id, None);
                call_broker(
                    &mut output,
                    &config,
                    child_id,
                    &child_turn_id,
                    Some(thread_id),
                );
                send(
                    &mut output,
                    &json!({"method": "turn/completed", "params": {"threadId": child_id, "turn": turn(&child_turn_id, "completed")}}),
                );
                send(
                    &mut output,
                    &json!({"method": "turn/completed", "params": {"threadId": thread_id, "turn": turn(turn_id, "completed")}}),
                );
                child["turns"] = json!([turn(&child_turn_id, "completed")]);
                root["turns"] = json!([turn(turn_id, "completed")]);
                continue;
            }
            Some("thread/list") => {
                let timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64();
                writeln!(
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(home.join("thread-list-times.log"))
                        .unwrap(),
                    "{timestamp}"
                )
                .unwrap();
                json!({"data": [child], "nextCursor": null, "backwardsCursor": null})
            }
            Some("thread/loaded/list") => {
                json!({"data": [thread_id, child_id], "nextCursor": null})
            }
            Some("thread/read") => json!({
                "thread": if message["params"]["threadId"] == thread_id { &root } else { &child }
            }),
            _ => {
                send(
                    &mut output,
                    &json!({"id": request_id, "error": {"code": -32601, "message": "unsupported"}}),
                );
                continue;
            }
        };
        send(&mut output, &json!({"id": request_id, "result": result}));
    }
}

#[test]
fn typed_live_freeze_and_cli_pair_execute_both_mock_arms_atomically() {
    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let worker = std::thread::spawn(move || {
        for index in 0..4 {
            let request = upstream
                .recv_timeout(Duration::from_secs(240))
                .unwrap()
                .unwrap();
            let body = format!(
                "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"mock-response-{index}\",\"model\":\"mock-revision\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}}}}\n\n"
            );
            request
                .respond(tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes("content-type", "text/event-stream").unwrap(),
                ))
                .unwrap();
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let seed_context = strict_live_context(&temp);
    let seed: serde_json::Value = serde_json::from_slice(&fs::read(seed_context).unwrap()).unwrap();
    let artifacts = seed["artifacts"].as_object().unwrap();
    let artifact = |name: &str| std::path::PathBuf::from(artifacts[name]["path"].as_str().unwrap());
    let mock_codex = temp.path().join("native-mock-app-server-test-harness");
    fs::copy(std::env::current_exe().unwrap(), &mock_codex).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&mock_codex, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let (live_case, live_material_root, live_attestation) =
        write_nonempty_source_fixture(temp.path(), "notes/evidence.txt", None, None);
    let live_root = temp.path().join("live-private");
    fs::create_dir(&live_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&live_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let output = live_root.join("frozen-run-context.json");
    crate::execute_cli(Cli {
        command: crate::EvalCommand::FreezeRunContext(crate::model::FreezeRunContextCommand {
            mode: crate::FreezeRunContextArgs::Live(crate::model::LiveFreezeArgs {
                repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
                evidence_repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
                fork_sha: seed["repoHead"].as_str().unwrap().to_string(),
                private_root: live_root.clone(),
                codex_bin: mock_codex,
                case: live_case,
                material_root: live_material_root,
                attestation: live_attestation,
                provider_budget_evidence: artifact("providerBudgetReceipt"),
                rate_card: artifact("rateCard"),
                billing_policy: artifact("billingPolicy"),
                fx_policy: artifact("fxPolicy"),
                lead_skill: artifact("skill"),
                model_label: "local-mock".to_string(),
                provider_label: "local-mock".to_string(),
                provider_role: ProviderRole::ApprovedReference,
                provider_upstream_url: format!("http://{upstream_addr}/v1/responses"),
                authorized_total_cost_fen: 0,
                authorized_per_run_cost_fen: 0,
                max_provider_request_attempts_per_run: 2,
                max_total_tokens_per_run: 10,
                max_elapsed_seconds_per_run: 180,
                max_output_tokens_per_request: 17,
                output: output.clone(),
            }),
        }),
    })
    .unwrap();
    let frozen_json: serde_json::Value =
        serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(frozen_json["providerMode"], json!("not-run"));
    for (name, relative) in [
        ("source", "inputs/case/case.json"),
        ("attestation", "inputs/held-out-attestation.json"),
        ("materials", "inputs/materials-manifest.json"),
        ("material:evidence-1", "inputs/case/notes/evidence.txt"),
        ("providerBudgetReceipt", "provider-budget-receipt.json"),
        ("rateCard", "rate-card.json"),
        ("billingPolicy", "billing-policy.json"),
        ("fxPolicy", "fx-policy.json"),
        ("skill", "lead-skill.md"),
    ] {
        assert_eq!(
            frozen_json["artifacts"][name]["path"],
            json!(
                live_root
                    .join(if relative.starts_with("inputs/") {
                        relative.to_string()
                    } else {
                        format!("frozen-inputs/{relative}")
                    })
                    .canonicalize()
                    .unwrap()
            )
        );
    }
    assert_ne!(
        frozen_json["artifacts"]["codexBinary"]["path"],
        frozen_json["artifacts"]["evaluatorBinary"]["path"]
    );
    assert!(
        frozen_json["artifacts"]["brokerSource"]["path"]
            .as_str()
            .unwrap()
            .ends_with("responses-api-proxy/src/broker.rs")
    );
    assert!(frozen_json["artifacts"].get("config").is_none());
    assert!(frozen_json["artifacts"].get("receipt").is_none());
    verify_frozen_context(&output).unwrap();
    let pair_result = crate::execute_cli(Cli {
        command: crate::EvalCommand::LivePair(crate::model::LivePairArgs {
            frozen_run_context: output,
        }),
    });
    if let Err(error) = pair_result {
        let stderr = fs::read_to_string(live_root.join("coordinator/app-server-1.stderr"))
            .unwrap_or_else(|read_error| format!("unavailable: {read_error}"));
        let generic_methods =
            fs::read_to_string(live_root.join("generic-home/app-server-methods.log"))
                .unwrap_or_else(|read_error| format!("unavailable: {read_error}"));
        let candidate_methods =
            fs::read_to_string(live_root.join("candidate-home/app-server-methods.log"))
                .unwrap_or_else(|read_error| format!("unavailable: {read_error}"));
        panic!(
            "pair failed: {error:#}; app server stderr: {stderr}; generic methods: {generic_methods}; candidate methods: {candidate_methods}"
        );
    }
    worker.join().unwrap();
    for home in ["generic-home", "candidate-home"] {
        let launch: serde_json::Value = serde_json::from_slice(
            &fs::read(live_root.join(home).join("app-server-launch.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            launch["home"],
            json!(live_root.join(home).canonicalize().unwrap())
        );
        assert_eq!(
            launch["codexHome"],
            json!(live_root.join(home).join(".codex").canonicalize().unwrap())
        );
        assert_eq!(launch["path"], json!(std::env::var("PATH").unwrap()));
        let keys = launch["envKeys"].as_array().unwrap();
        for forbidden in ["OPENAI_API_KEY", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
            assert!(!keys.iter().any(|key| key == forbidden));
        }
        let methods =
            fs::read_to_string(live_root.join(home).join("app-server-methods.log")).unwrap();
        for required in ["thread/list", "thread/loaded/list", "thread/read"] {
            assert!(
                methods.lines().any(|method| method == required),
                "{required}"
            );
        }
        let list_times = fs::read_to_string(live_root.join(home).join("thread-list-times.log"))
            .unwrap()
            .lines()
            .map(|value| value.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(list_times.len(), 2);
        assert!(list_times[1] - list_times[0] >= 1.9);
    }
    let execution: serde_json::Value = serde_json::from_slice(
        &fs::read(live_root.join("coordinator/execution-context.json")).unwrap(),
    )
    .unwrap();
    assert!(execution["broker"]["port"].as_u64().unwrap() > 0);
    assert!(matches!(
        execution["firstCondition"].as_str(),
        Some("generic" | "candidate")
    ));
    assert!(matches!(
        execution["secondCondition"].as_str(),
        Some("generic" | "candidate")
    ));
    assert_ne!(execution["firstCondition"], execution["secondCondition"]);
    let started_at =
        chrono::DateTime::parse_from_rfc3339(execution["startedAt"].as_str().unwrap()).unwrap();
    let deadline =
        chrono::DateTime::parse_from_rfc3339(execution["deadline"].as_str().unwrap()).unwrap();
    assert_eq!((deadline - started_at).num_seconds(), 180);
    assert_eq!(
        execution["pathSha256"],
        json!(format!(
            "{:x}",
            Sha256::digest(std::env::var("PATH").unwrap().as_bytes())
        ))
    );
    assert_eq!(execution["maxTotalTokensPerRun"], json!(10));
    assert_eq!(execution["maxElapsedSecondsPerRun"], json!(180));
    assert!(
        live_root
            .join("coordinator/receipts/pair-receipt.json")
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(live_root.join("coordinator/attempt-index.jsonl"))
            .unwrap()
            .lines()
            .count(),
        8
    );
}
