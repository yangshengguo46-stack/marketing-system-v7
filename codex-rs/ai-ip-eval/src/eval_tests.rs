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
use crate::MockProviderMode;
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

#[path = "blind_verify_tests.rs"]
mod blind_verify_tests;

#[path = "blind_verify_semantics_tests.rs"]
mod blind_verify_semantics_tests;

#[path = "blind_finalize_tests.rs"]
mod blind_finalize_tests;

#[path = "blind_bundle_tests.rs"]
mod blind_bundle_tests;

#[path = "blind_bundle_transaction_tests.rs"]
mod blind_bundle_transaction_tests;

#[path = "blind_bundle_transaction_failure_tests.rs"]
mod blind_bundle_transaction_failure_tests;

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

    let mock = ModeEvidence::Mock {
        provider_mode: MockProviderMode::NotRun,
        synthetic_fixture_sha256: "fixture".to_string(),
        arm_order_commitment: "order".to_string(),
    };
    let mock_json = serde_json::to_value(&mock).unwrap();
    assert_eq!(mock_json["executionMode"], json!("mock"));
    assert_eq!(mock_json["providerMode"], json!("not-run"));
    assert!(mock_json.get("providerRole").is_none());

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
        "normalizedFirstRootRequestCommitment": "t-normalized",
        "normalizedFirstRootBaseCommitment": "u",
        "firstRootTreatmentDiffCommitment": null,
        "appServerTranscriptSha256": "v",
        "brokerAttemptLedgerSha256": "w",
        "attemptIndexRootSha256": "x",
        "postprocessEvidenceIndexSha256": "0".repeat(64),
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

    let mut uppercase_postprocess_commitment = manifest_json.clone();
    uppercase_postprocess_commitment["postprocessEvidenceIndexSha256"] = json!("A".repeat(64));
    let uppercase_postprocess_commitment: crate::RunManifest =
        serde_json::from_value(uppercase_postprocess_commitment).unwrap();
    assert!(
        uppercase_postprocess_commitment
            .validate_execution_mode()
            .is_err()
    );

    let mut missing_postprocess_commitment = manifest_json.clone();
    missing_postprocess_commitment
        .as_object_mut()
        .unwrap()
        .remove("postprocessEvidenceIndexSha256");
    assert!(serde_json::from_value::<crate::RunManifest>(missing_postprocess_commitment).is_err());

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

    for name in ["replay-fixture-set.json", "replay-attestation.json"] {
        let resource = format!("tests/fixtures/{name}");
        let path = codex_utils_cargo_bin::find_resource!(resource).unwrap();
        let _: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    }
}

struct ReplayPairTestRun {
    _temp: tempfile::TempDir,
    private_root: std::path::PathBuf,
    frozen_json: serde_json::Value,
    manifests: Vec<crate::RunManifest>,
}

struct PreparedReplayTestContext {
    _temp: tempfile::TempDir,
    private_root: std::path::PathBuf,
    frozen: std::path::PathBuf,
    frozen_json: serde_json::Value,
}

fn test_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn create_owner_only_test_dir(path: &std::path::Path) {
    fs::create_dir(path).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

fn test_digest_hex(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn assert_replay_request_fixture_digests(fixture_root: &std::path::Path) {
    let fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-fixture-set.json")).unwrap())
            .unwrap();
    for (name, path) in [
        ("genericRequest", "replay-generic-request.json"),
        ("candidateRequest", "replay-candidate-request.json"),
    ] {
        let entry = fixture_set["fixtures"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == name)
            .unwrap();
        assert_eq!(entry["path"], path);
        assert_eq!(
            entry["sha256"],
            test_sha256(&fs::read(fixture_root.join(path)).unwrap())
        );
    }
}

fn resign_replay_fixture(fixture_root: &std::path::Path) {
    let fixture_set_path = fixture_root.join("replay-fixture-set.json");
    let mut fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture_set_path).unwrap()).unwrap();
    let case_bytes = fs::read(fixture_root.join("replay-case.json")).unwrap();
    let entries = fixture_set["fixtures"].as_array_mut().unwrap();
    entries
        .iter_mut()
        .find(|entry| entry["name"] == "case")
        .unwrap()["sha256"] = json!(test_sha256(&case_bytes));
    let commitments = [
        "case",
        "genericRequest",
        "candidateRequest",
        "genericTranscript",
        "candidateTranscript",
        "leadSkill",
    ]
    .map(|name| {
        let sha256 = entries.iter().find(|entry| entry["name"] == name).unwrap()["sha256"].clone();
        json!({"name": name, "sha256": sha256})
    });
    let canonical = crate::jcs::canonicalize_value(&json!(commitments)).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(b"AI-IP-REPLAY-PAIR-V2\0");
    hasher.update(b"synthetic-replay-fork");
    hasher.update(canonical);
    let pair_id = format!("{:x}", hasher.finalize());
    let attestation_path = fixture_root.join("replay-attestation.json");
    let mut attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation_path).unwrap()).unwrap();
    attestation["pairId"] = json!(pair_id);
    attestation["caseSha256"] = json!(test_sha256(&case_bytes));
    let typed_mission: HeldOutMissionCase = serde_json::from_slice(&case_bytes).unwrap();
    attestation["sourceMaterialsSha256"] = json!(test_sha256(
        &serde_json::to_vec(&typed_mission.materials).unwrap()
    ));
    let attestation_bytes = serde_json::to_vec_pretty(&attestation).unwrap();
    fs::write(&attestation_path, &attestation_bytes).unwrap();
    entries
        .iter_mut()
        .find(|entry| entry["name"] == "attestation")
        .unwrap()["sha256"] = json!(test_sha256(&attestation_bytes));
    fs::write(
        fixture_set_path,
        serde_json::to_vec_pretty(&fixture_set).unwrap(),
    )
    .unwrap();
}

fn replay_fixture_with_material() -> tempfile::TempDir {
    let source_manifest =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let source_root = source_manifest.parent().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture_root = temp.path();
    let fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(&source_manifest).unwrap()).unwrap();
    for entry in fixture_set["fixtures"].as_array().unwrap() {
        let path = entry["path"].as_str().unwrap();
        fs::copy(source_root.join(path), fixture_root.join(path)).unwrap();
    }
    fs::copy(
        &source_manifest,
        fixture_root.join("replay-fixture-set.json"),
    )
    .unwrap();
    let material_path = fixture_root.join("notes/evidence.txt");
    fs::create_dir(material_path.parent().unwrap()).unwrap();
    fs::write(&material_path, b"retained replay material\n").unwrap();
    let case_path = fixture_root.join("replay-case.json");
    let mut mission: serde_json::Value =
        serde_json::from_slice(&fs::read(&case_path).unwrap()).unwrap();
    mission["materials"] = json!([{
        "materialId": "replay-evidence",
        "relativePath": "notes/evidence.txt",
        "sha256": test_sha256(b"retained replay material\n"),
        "materialKind": "evidence"
    }]);
    let case_bytes = serde_json::to_vec_pretty(&mission).unwrap();
    fs::write(&case_path, &case_bytes).unwrap();
    resign_replay_fixture(fixture_root);
    temp
}

fn prepare_replay_test_context(
    fixture_root: &std::path::Path,
) -> anyhow::Result<PreparedReplayTestContext> {
    let temp = tempfile::tempdir()?;
    let private_root = temp.path().join("replay-private");
    fs::create_dir(&private_root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700))?;
    }
    let private_root = private_root.canonicalize()?;
    let codex_binary = private_root.join("synthetic-codex-binary");
    fs::write(&codex_binary, b"synthetic replay binary\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&codex_binary, fs::Permissions::from_mode(0o700))?;
    }
    let frozen = private_root.join("frozen-run-context.json");
    crate::freeze_replay_context(crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.to_path_buf(),
        fork_sha: "synthetic-replay-fork".to_string(),
        private_root: private_root.clone(),
        codex_bin: codex_binary,
        case: fixture_root.join("replay-case.json"),
        transcript: fixture_root.join("replay-transcript.jsonl"),
        fixture_set_manifest: fixture_root.join("replay-fixture-set.json"),
        output: frozen.clone(),
    })?;
    let frozen_json = serde_json::from_slice(&fs::read(&frozen)?)?;
    Ok(PreparedReplayTestContext {
        _temp: temp,
        private_root,
        frozen,
        frozen_json,
    })
}

fn run_frozen_replay_pair_from(
    fixture_root: &std::path::Path,
) -> anyhow::Result<ReplayPairTestRun> {
    let prepared = prepare_replay_test_context(fixture_root)?;

    crate::execute_cli(Cli {
        command: crate::EvalCommand::ReplayPair(crate::ReplayPairArgs {
            frozen_run_context: prepared.frozen,
        }),
    })?;
    let coordinator = prepared.private_root.join("replay-coordinator");
    for leaf in [
        "run-1-manifest.json",
        "run-2-manifest.json",
        "replay-pair-verification.json",
    ] {
        assert!(coordinator.join(leaf).is_file(), "missing {leaf}");
    }
    let mut manifests = Vec::new();
    for ordinal in [1_u8, 2] {
        let manifest: crate::RunManifest = serde_json::from_slice(&fs::read(
            coordinator.join(format!("run-{ordinal}-manifest.json")),
        )?)?;
        manifests.push(manifest);
    }
    Ok(ReplayPairTestRun {
        _temp: prepared._temp,
        private_root: prepared.private_root,
        frozen_json: prepared.frozen_json,
        manifests,
    })
}

fn run_frozen_replay_pair() -> anyhow::Result<ReplayPairTestRun> {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json")?;
    run_frozen_replay_pair_from(fixture_set.parent().unwrap())
}

#[test]
fn replay_pair_accepts_shared_verified_frozen_context() {
    let run = run_frozen_replay_pair().unwrap();
    assert_eq!(run.manifests.len(), 2);
}

#[test]
fn replay_pair_rejects_frozen_material_drift_before_coordinator_output() {
    let fixture = replay_fixture_with_material();
    let prepared = prepare_replay_test_context(fixture.path()).unwrap();
    fs::write(fixture.path().join("notes/evidence.txt"), b"changed\n").unwrap();

    let result = crate::run_replay_pair(crate::ReplayPairArgs {
        frozen_run_context: prepared.frozen,
    });

    let error = result.expect_err("accepted a changed frozen Replay material");
    assert!(format!("{error:#}").contains("material"));
    assert!(!prepared.private_root.join("replay-coordinator").exists());
}

#[test]
fn replay_material_freeze_rejects_invalid_files_and_declarations() {
    for mutation in ["digest", "missing", "collision", "duplicate-path"] {
        let fixture = replay_fixture_with_material();
        let fixture_root = fixture.path();
        let material_path = fixture_root.join("notes/evidence.txt");
        match mutation {
            "digest" => fs::write(&material_path, b"wrong digest\n").unwrap(),
            "missing" => fs::remove_file(&material_path).unwrap(),
            "collision" | "duplicate-path" => {
                let case_path = fixture_root.join("replay-case.json");
                let mut mission: serde_json::Value =
                    serde_json::from_slice(&fs::read(&case_path).unwrap()).unwrap();
                if mutation == "collision" {
                    mission["materials"][0]["relativePath"] = json!("replay-transcript.jsonl");
                    mission["materials"][0]["sha256"] = json!(test_sha256(
                        &fs::read(fixture_root.join("replay-transcript.jsonl")).unwrap()
                    ));
                } else {
                    let mut duplicate = mission["materials"][0].clone();
                    duplicate["materialId"] = json!("duplicate-evidence");
                    mission["materials"].as_array_mut().unwrap().push(duplicate);
                }
                fs::write(&case_path, serde_json::to_vec_pretty(&mission).unwrap()).unwrap();
                resign_replay_fixture(fixture_root);
            }
            _ => unreachable!(),
        }

        let error = prepare_replay_test_context(fixture_root)
            .err()
            .expect("accepted an invalid Replay material");
        let expected = match mutation {
            "digest" => "digest mismatch",
            "missing" => "canonicalize Replay material",
            "collision" => "collides with a frozen Replay fixture",
            "duplicate-path" => "duplicate material path",
            _ => unreachable!(),
        };
        assert!(
            format!("{error:#}").contains(expected),
            "unexpected {mutation} error: {error:#}"
        );
    }
}

#[cfg(unix)]
#[test]
fn replay_material_freeze_rejects_symlink_escape() {
    use std::os::unix::fs::symlink;

    let fixture = replay_fixture_with_material();
    let material_path = fixture.path().join("notes/evidence.txt");
    fs::remove_file(&material_path).unwrap();
    let outside = tempfile::NamedTempFile::new().unwrap();
    fs::write(outside.path(), b"retained replay material\n").unwrap();
    symlink(outside.path(), &material_path).unwrap();

    let error = prepare_replay_test_context(fixture.path())
        .err()
        .expect("accepted a Replay material path escape");
    assert!(format!("{error:#}").contains("traverses a link or escapes"));
}

#[test]
fn replay_verified_context_retains_inputs_and_rejects_drift() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let prepared = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let raw = fs::read(&prepared.frozen).unwrap();
    let verified = crate::runner::verify_replay_frozen_context(&prepared.frozen, &raw).unwrap();
    let projection = verified.projection();
    assert_eq!(projection.canonical_path, prepared.frozen);
    assert_eq!(projection.raw_bytes, raw);
    assert_eq!(projection.raw_sha256, test_sha256(&raw));
    assert_eq!(projection.private_root, prepared.private_root);
    assert_eq!(projection.pair_id, prepared.frozen_json["pairId"]);
    assert_eq!(projection.fork_sha, "synthetic-replay-fork");
    assert_eq!(
        projection.fixture_set_sha256,
        prepared.frozen_json["fixtureSetManifest"]["sha256"]
    );
    assert!(projection.materials.is_empty());
    assert_eq!(projection.materials_manifest_bytes, b"[]");
    assert_eq!(projection.attestation.reviewers.len(), 3);
    assert!(!projection.prompt_bytes.is_empty());
    assert!(!projection.additional_context_bytes.is_empty());
    assert!(!projection.schema_bytes.is_empty());
    assert!(!projection.thread_start_bytes.is_empty());
    assert!(!projection.turn_start_bytes.is_empty());
    assert_eq!(projection.model_label, "replay-fixture");
    assert_eq!(projection.provider_label, "not-run");
    assert_eq!(projection.max_provider_request_attempts, 0);
    assert_eq!(projection.max_total_tokens, 0);
    assert_eq!(projection.max_elapsed_seconds, 0);
    assert_eq!(
        projection.broker_component_sha256,
        test_sha256(b"replay:no-broker-component")
    );
    verified.reverify_all().unwrap();

    let codex = std::path::Path::new(
        prepared.frozen_json["codexBinary"]["path"]
            .as_str()
            .unwrap(),
    );
    fs::write(codex, b"changed replay binary\n").unwrap();
    assert!(verified.reverify_all().is_err());
}

#[test]
fn replay_verified_context_projects_and_reverifies_materials() {
    let fixture = replay_fixture_with_material();
    let prepared = prepare_replay_test_context(fixture.path()).unwrap();
    let raw = fs::read(&prepared.frozen).unwrap();
    let verified = crate::runner::verify_replay_frozen_context(&prepared.frozen, &raw).unwrap();
    let projection = verified.projection();
    assert_eq!(
        projection.fixture_set_sha256,
        test_sha256(&fs::read(fixture.path().join("replay-fixture-set.json")).unwrap())
    );
    assert_eq!(
        projection.materials,
        vec![crate::runner::ReplayMaterialInput {
            material_id: "replay-evidence".to_string(),
            relative_path: "notes/evidence.txt".to_string(),
            sha256: test_sha256(b"retained replay material\n"),
            bytes: b"retained replay material\n".to_vec(),
        }]
    );
    verified.reverify_all().unwrap();

    fs::write(fixture.path().join("notes/evidence.txt"), b"changed\n").unwrap();
    let error = verified.reverify_all().unwrap_err();
    assert!(format!("{error:#}").contains("Replay material replay-evidence"));
}

fn replace_with_same_owner_only_bytes(path: &std::path::Path) {
    let path = path.canonicalize().unwrap();
    let bytes = fs::read(&path).unwrap();
    let replacement = path.with_extension("replacement");
    let displaced = path.with_extension("displaced");
    crate::secure_fs::write_owner_only_new(&replacement, &bytes).unwrap();
    fs::rename(&path, &displaced).unwrap();
    fs::rename(&replacement, &path).unwrap();
}

#[test]
fn frozen_context_identity_replacement_replay_is_rejected() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let prepared = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let raw = fs::read(&prepared.frozen).unwrap();
    let verified = crate::runner::verify_replay_frozen_context(&prepared.frozen, &raw).unwrap();

    replace_with_same_owner_only_bytes(&prepared.frozen);

    let error = verified.reverify_all().unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "reverify Replay frozen context identity: frozen context path identity changed after verification"
    );
}

#[test]
fn replay_material_same_bytes_inode_replacement_is_rejected() {
    let fixture = replay_fixture_with_material();
    let prepared = prepare_replay_test_context(fixture.path()).unwrap();
    let raw = fs::read(&prepared.frozen).unwrap();
    let verified = crate::runner::verify_replay_frozen_context(&prepared.frozen, &raw).unwrap();
    let material_path = fixture.path().join("notes/evidence.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            material_path.parent().unwrap(),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    replace_with_same_owner_only_bytes(&material_path);

    let error = verified.reverify_all().unwrap_err();
    assert!(format!("{error:#}").contains("Replay material replay-evidence"));
}

#[test]
fn replay_verified_context_rejects_raw_path_sha_and_reference_drift() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let prepared = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let raw = fs::read(&prepared.frozen).unwrap();
    let mut with_lf = raw.clone();
    with_lf.push(b'\n');
    assert!(crate::runner::verify_replay_frozen_context(&prepared.frozen, &with_lf).is_err());
    assert!(
        crate::runner::verify_replay_frozen_context(fixture_set.parent().unwrap(), &raw).is_err()
    );

    for mutation in ["sha", "reference"] {
        let mut context: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        match mutation {
            "sha" => context["codexBinary"]["sha256"] = json!("0".repeat(64)),
            "reference" => context["codexBinary"]["path"] = json!("/missing/replay-codex"),
            _ => unreachable!(),
        }
        let bytes = serde_json::to_vec_pretty(&context).unwrap();
        assert!(
            crate::runner::verify_replay_frozen_context(&prepared.frozen, &bytes).is_err(),
            "accepted Replay {mutation} drift"
        );
    }
}

fn assert_generated_commitment_identity(
    private_root: &std::path::Path,
    context: &serde_json::Value,
) {
    let key_path = private_root.join("coordinator/commitment-key.bin");
    let key = crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(private_root).unwrap();
    assert_eq!(context["commitmentKeySha256"], key.key_sha256());
    assert_eq!(
        context["publicRunId"],
        key.derive_public_run_id(context["pairId"].as_str().unwrap())
            .unwrap()
    );
    let context_bytes = serde_json::to_vec(context).unwrap();
    assert!(!key.key_material_occurs_in(&context_bytes));
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(key_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(metadata.nlink(), 1);
    }
}

fn freeze_native_commitment_context()
-> (tempfile::TempDir, std::path::PathBuf, serde_json::Value) {
    let temp = tempfile::tempdir().unwrap();
    let args = native_freeze_args_for_cost_inputs(&temp, true);
    let private_root = args.private_root.clone();
    let output = args.output.clone();
    crate::freeze_live_context(args).unwrap();
    let context = serde_json::from_slice(&fs::read(output).unwrap()).unwrap();
    (temp, private_root, context)
}

#[test]
fn freeze_derives_public_run_id_from_private_commitment_key() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let replay_a = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let replay_b = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let (_native_temp_a, native_root_a, native_a) = freeze_native_commitment_context();
    let (_native_temp_b, native_root_b, native_b) = freeze_native_commitment_context();

    for (private_root, context) in [
        (&replay_a.private_root, &replay_a.frozen_json),
        (&replay_b.private_root, &replay_b.frozen_json),
        (&native_root_a, &native_a),
        (&native_root_b, &native_b),
    ] {
        assert_generated_commitment_identity(private_root, context);
    }
    assert_ne!(
        replay_a.frozen_json["publicRunId"],
        replay_b.frozen_json["publicRunId"]
    );
    assert_ne!(native_a["publicRunId"], native_b["publicRunId"]);
}

fn write_existing_commitment_key(private_root: &std::path::Path) -> Vec<u8> {
    let coordinator = private_root.join("coordinator");
    if !coordinator.exists() {
        crate::secure_fs::create_owner_only_dir_new(&coordinator).unwrap();
    }
    let key_bytes = vec![0xa5; 32];
    crate::secure_fs::write_owner_only_new(
        &coordinator.join("commitment-key.bin"),
        &key_bytes,
    )
    .unwrap();
    key_bytes
}

fn clone_live_freeze_args(
    args: &crate::model::LiveFreezeArgs,
) -> crate::model::LiveFreezeArgs {
    crate::model::LiveFreezeArgs {
        repo_root: args.repo_root.clone(),
        evidence_repo_root: args.evidence_repo_root.clone(),
        fork_sha: args.fork_sha.clone(),
        private_root: args.private_root.clone(),
        codex_bin: args.codex_bin.clone(),
        case: args.case.clone(),
        material_root: args.material_root.clone(),
        attestation: args.attestation.clone(),
        provider_budget_evidence: args.provider_budget_evidence.clone(),
        rate_card: args.rate_card.clone(),
        billing_policy: args.billing_policy.clone(),
        fx_policy: args.fx_policy.clone(),
        lead_skill: args.lead_skill.clone(),
        model_label: args.model_label.clone(),
        provider_label: args.provider_label.clone(),
        provider_role: args.provider_role,
        provider_upstream_url: args.provider_upstream_url.clone(),
        authorized_total_cost_fen: args.authorized_total_cost_fen,
        authorized_per_run_cost_fen: args.authorized_per_run_cost_fen,
        max_provider_request_attempts_per_run: args.max_provider_request_attempts_per_run,
        max_total_tokens_per_run: args.max_total_tokens_per_run,
        max_elapsed_seconds_per_run: args.max_elapsed_seconds_per_run,
        max_output_tokens_per_request: args.max_output_tokens_per_request,
        output: args.output.clone(),
    }
}

#[test]
fn freeze_refuses_existing_commitment_key() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let replay_temp = tempfile::tempdir().unwrap();
    let replay_root = replay_temp.path().join("replay-private");
    create_owner_only_test_dir(&replay_root);
    let replay_root = replay_root.canonicalize().unwrap();
    let replay_binary = replay_root.join("synthetic-codex-binary");
    fs::write(&replay_binary, b"synthetic replay binary\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&replay_binary, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let replay_output = replay_root.join("frozen-run-context.json");
    let replay_key = write_existing_commitment_key(&replay_root);
    let replay_error = crate::runner::freeze_replay_context_with_hook(
        crate::model::ReplayFreezeArgs {
            repo_root: fixture_set.parent().unwrap().to_path_buf(),
            fork_sha: "synthetic-replay-fork".to_string(),
            private_root: replay_root.clone(),
            codex_bin: replay_binary,
            case: fixture_set.parent().unwrap().join("replay-case.json"),
            transcript: fixture_set.parent().unwrap().join("replay-transcript.jsonl"),
            fixture_set_manifest: fixture_set.clone(),
            output: replay_output.clone(),
        },
        || Ok(()),
    )
    .unwrap_err();
    assert!(format!("{replay_error:#}").contains("create private file"));
    assert!(!replay_output.exists());
    assert_eq!(
        fs::read(replay_root.join("coordinator/commitment-key.bin")).unwrap(),
        replay_key
    );

    let native_temp = tempfile::tempdir().unwrap();
    let native_args = native_freeze_args_for_cost_inputs(&native_temp, true);
    let native_root = native_args.private_root.clone();
    let native_output = native_args.output.clone();
    let native_key = write_existing_commitment_key(&native_root);
    let native_error = crate::freeze_live_context(native_args).unwrap_err();
    assert!(format!("{native_error:#}").contains("create private file"));
    assert!(!native_output.exists());
    assert_eq!(
        fs::read(native_root.join("coordinator/commitment-key.bin")).unwrap(),
        native_key
    );

    let partial_replay_temp = tempfile::tempdir().unwrap();
    let partial_replay_root = partial_replay_temp.path().join("replay-private");
    create_owner_only_test_dir(&partial_replay_root);
    let partial_replay_root = partial_replay_root.canonicalize().unwrap();
    let partial_replay_binary = partial_replay_root.join("synthetic-codex-binary");
    fs::write(&partial_replay_binary, b"synthetic replay binary\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &partial_replay_binary,
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    let fixture_root = fixture_set.parent().unwrap().to_path_buf();
    let partial_replay_output = partial_replay_root.join("frozen-run-context.json");
    let partial_replay_args = || crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.clone(),
        fork_sha: "different-replay-fork".to_string(),
        private_root: partial_replay_root.clone(),
        codex_bin: partial_replay_binary.clone(),
        case: fixture_root.join("replay-case.json"),
        transcript: fixture_root.join("replay-transcript.jsonl"),
        fixture_set_manifest: fixture_root.join("replay-fixture-set.json"),
        output: partial_replay_output.clone(),
    };
    let first_replay_error = crate::freeze_replay_context(partial_replay_args()).unwrap_err();
    assert!(format!("{first_replay_error:#}").contains("attestation"));
    assert!(!partial_replay_output.exists());
    let partial_replay_key =
        fs::read(partial_replay_root.join("coordinator/commitment-key.bin")).unwrap();
    let retry_replay_error = crate::freeze_replay_context(partial_replay_args()).unwrap_err();
    assert!(format!("{retry_replay_error:#}").contains("create private file"));
    assert_eq!(
        fs::read(partial_replay_root.join("coordinator/commitment-key.bin")).unwrap(),
        partial_replay_key
    );

    let partial_native_temp = tempfile::tempdir().unwrap();
    let partial_native_args = native_freeze_args_for_cost_inputs(&partial_native_temp, true);
    let retry_native_args = clone_live_freeze_args(&partial_native_args);
    fs::write(
        partial_native_args
            .material_root
            .join("notes/evidence.txt"),
        b"changed after declaration\n",
    )
    .unwrap();
    let partial_native_root = partial_native_args.private_root.clone();
    let partial_native_output = partial_native_args.output.clone();
    let first_native_error = crate::freeze_live_context(partial_native_args).unwrap_err();
    assert!(format!("{first_native_error:#}").contains("digest mismatch"));
    assert!(!partial_native_output.exists());
    let partial_native_key =
        fs::read(partial_native_root.join("coordinator/commitment-key.bin")).unwrap();
    let retry_native_error = crate::freeze_live_context(retry_native_args).unwrap_err();
    assert!(format!("{retry_native_error:#}").contains("create private file"));
    assert_eq!(
        fs::read(partial_native_root.join("coordinator/commitment-key.bin")).unwrap(),
        partial_native_key
    );
}

fn replace_commitment_key_with_bytes(private_root: &std::path::Path, bytes: &[u8; 32]) {
    let path = private_root.join("coordinator/commitment-key.bin");
    let replacement = private_root.join("coordinator/replacement-key.bin");
    let displaced = private_root.join("coordinator/displaced-key.bin");
    crate::secure_fs::write_owner_only_new(&replacement, bytes).unwrap();
    fs::rename(&path, &displaced).unwrap();
    fs::rename(&replacement, &path).unwrap();
}

fn mutate_context_field(context: &mut serde_json::Value, field: &str) {
    let current = context[field].as_str().unwrap();
    let first = if current.starts_with('0') { "1" } else { "0" };
    context[field] = json!(format!("{first}{}", &current[1..]));
}

#[test]
fn proof_commitment_key_mutations_are_rejected_for_replay_and_native() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let replay_bit = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let replay_replacement = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let (_native_bit_temp, native_bit_root, native_bit_context) =
        freeze_native_commitment_context();
    let (_native_replacement_temp, native_replacement_root, native_replacement_context) =
        freeze_native_commitment_context();

    for (private_root, context, replacement) in [
        (&replay_bit.private_root, &replay_bit.frozen_json, false),
        (
            &replay_replacement.private_root,
            &replay_replacement.frozen_json,
            true,
        ),
        (&native_bit_root, &native_bit_context, false),
        (
            &native_replacement_root,
            &native_replacement_context,
            true,
        ),
    ] {
        let context_path = private_root.join("frozen-run-context.json");
        let raw = fs::read(&context_path).unwrap();
        let replay = context["executionMode"] == "replay";
        if replay {
            let verified =
                crate::runner::verify_replay_frozen_context(&context_path, &raw).unwrap();
            if replacement {
                replace_commitment_key_with_bytes(private_root, &[0x7e; 32]);
            } else {
                let key_path = private_root.join("coordinator/commitment-key.bin");
                let mut bytes = fs::read(&key_path).unwrap();
                bytes[0] ^= 1;
                fs::write(key_path, bytes).unwrap();
            }
            assert!(
                crate::runner::verify_replay_frozen_context(&context_path, &raw).is_err(),
                "Replay accepted commitment key mutation, replacement={replacement}"
            );
            assert!(verified.reverify_all().is_err());
        } else {
            let verified = verify_frozen_context(&context_path).unwrap();
            if replacement {
                replace_commitment_key_with_bytes(private_root, &[0x7e; 32]);
            } else {
                let key_path = private_root.join("coordinator/commitment-key.bin");
                let mut bytes = fs::read(&key_path).unwrap();
                bytes[0] ^= 1;
                fs::write(key_path, bytes).unwrap();
            }
            assert!(
                verify_frozen_context(&context_path).is_err(),
                "Native accepted commitment key mutation, replacement={replacement}"
            );
            assert!(verified.reverify_all().is_err());
        }
    }
}

#[test]
fn proof_commitment_context_mutations_are_rejected_for_replay_and_native() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let replay = prepare_replay_test_context(fixture_set.parent().unwrap()).unwrap();
    let replay_raw = fs::read(&replay.frozen).unwrap();
    let replay_verified =
        crate::runner::verify_replay_frozen_context(&replay.frozen, &replay_raw).unwrap();
    let replay_key =
        crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(&replay.private_root)
            .unwrap();
    assert!(!replay_key.key_material_occurs_in(format!("{replay_verified:?}").as_bytes()));
    replay_verified.clone().reverify_all().unwrap();
    for field in ["commitmentKeySha256", "pairId", "publicRunId"] {
        let mut context: serde_json::Value = serde_json::from_slice(&replay_raw).unwrap();
        mutate_context_field(&mut context, field);
        let mutated = serde_json::to_vec_pretty(&context).unwrap();
        fs::write(&replay.frozen, &mutated).unwrap();
        let verify_error =
            crate::runner::verify_replay_frozen_context(&replay.frozen, &mutated).unwrap_err();
        assert!(!replay_key.key_material_occurs_in(format!("{verify_error:#}").as_bytes()));
        let reverify_error = replay_verified.reverify_all().unwrap_err();
        assert!(!replay_key.key_material_occurs_in(format!("{reverify_error:#}").as_bytes()));
        fs::write(&replay.frozen, &replay_raw).unwrap();
        replay_verified.reverify_all().unwrap();
    }

    let (_native_temp, native_root, _) = freeze_native_commitment_context();
    let native_path = native_root.join("frozen-run-context.json");
    let native_raw = fs::read(&native_path).unwrap();
    let native_verified = verify_frozen_context(&native_path).unwrap();
    let native_key =
        crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(&native_root).unwrap();
    assert!(!native_key.key_material_occurs_in(format!("{native_verified:?}").as_bytes()));
    native_verified.clone().reverify_all().unwrap();
    for field in ["commitmentKeySha256", "pairId", "publicRunId"] {
        let mut context: serde_json::Value = serde_json::from_slice(&native_raw).unwrap();
        mutate_context_field(&mut context, field);
        fs::write(&native_path, serde_json::to_vec_pretty(&context).unwrap()).unwrap();
        let verify_error = verify_frozen_context(&native_path).unwrap_err();
        assert!(!native_key.key_material_occurs_in(format!("{verify_error:#}").as_bytes()));
        let reverify_error = native_verified.reverify_all().unwrap_err();
        assert!(!native_key.key_material_occurs_in(format!("{reverify_error:#}").as_bytes()));
        fs::write(&native_path, &native_raw).unwrap();
        native_verified.reverify_all().unwrap();
    }
}

fn assert_inventory_covers_commitment_key(private_root: &std::path::Path) {
    let context: serde_json::Value = serde_json::from_slice(
        &fs::read(private_root.join("frozen-run-context.json")).unwrap(),
    )
    .unwrap();
    let inventory = fs::read_to_string(private_root.join("coordinator/private-inventory.jsonl"))
        .unwrap();
    let record = inventory
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .find(|record| record["relativePath"] == "coordinator/commitment-key.bin")
        .expect("commitment key missing from private inventory");
    assert_eq!(record["kind"], "file");
    assert_eq!(record["sha256"], context["commitmentKeySha256"]);
    crate::private_inventory::verify_private_inventory(private_root).unwrap();

    let key =
        crate::proof_commitment::RetainedProofCommitmentKey::read_fixed(private_root).unwrap();
    let raw_key = fs::read(private_root.join("coordinator/commitment-key.bin")).unwrap();
    let lowercase_hex = raw_key
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let uppercase_hex = lowercase_hex.to_ascii_uppercase();
    assert!(key.key_material_occurs_in(&raw_key));
    assert!(key.key_material_occurs_in(lowercase_hex.as_bytes()));
    assert!(key.key_material_occurs_in(uppercase_hex.as_bytes()));
    fn inspect(
        key: &crate::proof_commitment::RetainedProofCommitmentKey,
        root: &std::path::Path,
        path: &std::path::Path,
    ) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let entry_path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                inspect(key, root, &entry_path);
            } else if entry_path != root.join("coordinator/commitment-key.bin") {
                let bytes = fs::read(&entry_path).unwrap();
                assert!(
                    !key.key_material_occurs_in(&bytes),
                    "raw key leaked into {}",
                    entry_path.display()
                );
            }
        }
    }
    inspect(&key, private_root, private_root);
}

#[test]
fn proof_commitment_inventory_covers_replay_and_native_without_key_disclosure() {
    let replay = run_frozen_replay_pair().unwrap();
    assert_inventory_covers_commitment_key(&replay.private_root);

    let native = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    native.result.unwrap();
    assert_inventory_covers_commitment_key(&native.live_root);
}

struct ExpectedReplayRequestEvidence {
    generic_raw: String,
    generic_normalized: String,
    candidate_raw: String,
    candidate_normalized: String,
    normalized_base: String,
    treatment: String,
}

fn materialize_request_home_token(
    value: &mut serde_json::Value,
    token: &str,
    codex_home: &std::path::Path,
) {
    match value {
        serde_json::Value::Object(object) => {
            for value in object.values_mut() {
                materialize_request_home_token(value, token, codex_home);
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                materialize_request_home_token(value, token, codex_home);
            }
        }
        serde_json::Value::String(value) if value.contains(token) => {
            *value = value.replace(token, codex_home.to_str().unwrap());
        }
        _ => {}
    }
}

fn canonicalize_replay_request_fixtures(
    fixture_root: &std::path::Path,
    private_root: &std::path::Path,
) -> ExpectedReplayRequestEvidence {
    let homes = crate::runner::IsolatedHomes {
        generic_home: private_root.join("replay-generic-home"),
        generic_codex_home: private_root.join("replay-generic-home/.codex"),
        candidate_home: private_root.join("replay-candidate-home"),
        candidate_codex_home: private_root.join("replay-candidate-home/.codex"),
    };
    let mut generic: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture_root.join("replay-generic-request.json")).unwrap(),
    )
    .unwrap();
    materialize_request_home_token(
        &mut generic,
        "$GENERIC_CODEX_HOME",
        &homes.generic_codex_home,
    );
    let mut candidate: serde_json::Value = serde_json::from_slice(
        &fs::read(fixture_root.join("replay-candidate-request.json")).unwrap(),
    )
    .unwrap();
    materialize_request_home_token(
        &mut candidate,
        "$CANDIDATE_CODEX_HOME",
        &homes.candidate_codex_home,
    );
    let generic_threads = HashSet::from(["0198f5aa-0000-7000-8000-000000000002".to_string()]);
    let candidate_threads = HashSet::from(["0198f5aa-0000-7000-8000-000000000003".to_string()]);
    let generic_evidence = crate::runner::canonicalize_first_root_request(
        &generic,
        &crate::runner::RequestCanonicalizationContext {
            condition: EvaluationCondition::Generic,
            known_thread_ids: &generic_threads,
            generic_codex_home: &homes.generic_codex_home,
            candidate_codex_home: &homes.candidate_codex_home,
            deadline_rfc3339: "2026-08-27T12:01:00Z",
        },
    )
    .unwrap();
    let candidate_evidence = crate::runner::canonicalize_first_root_request(
        &candidate,
        &crate::runner::RequestCanonicalizationContext {
            condition: EvaluationCondition::Candidate,
            known_thread_ids: &candidate_threads,
            generic_codex_home: &homes.generic_codex_home,
            candidate_codex_home: &homes.candidate_codex_home,
            deadline_rfc3339: "2026-08-27T12:01:00Z",
        },
    )
    .unwrap();
    assert_eq!(
        generic_evidence.normalized_base_commitment,
        candidate_evidence.normalized_base_commitment
    );
    ExpectedReplayRequestEvidence {
        generic_raw: test_digest_hex(generic_evidence.raw_sha256),
        generic_normalized: test_digest_hex(generic_evidence.normalized_sha256),
        candidate_raw: test_digest_hex(candidate_evidence.raw_sha256),
        candidate_normalized: test_digest_hex(candidate_evidence.normalized_sha256),
        normalized_base: test_digest_hex(generic_evidence.normalized_base_commitment),
        treatment: test_digest_hex(candidate_evidence.treatment_diff_commitment.unwrap()),
    }
}

#[test]
fn replay_request_evidence_is_derived_from_frozen_request_fixtures() {
    let fixture_set =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let fixture_root = fixture_set.parent().unwrap();
    assert_replay_request_fixture_digests(fixture_root);

    let run = run_frozen_replay_pair_from(fixture_root).unwrap();
    let generic = run
        .manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Generic)
        .unwrap();
    let candidate = run
        .manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Candidate)
        .unwrap();
    let expected = canonicalize_replay_request_fixtures(fixture_root, &run.private_root);
    assert_eq!(
        generic.first_root_provider_request_commitment,
        expected.generic_raw
    );
    assert_eq!(
        candidate.first_root_provider_request_commitment,
        expected.candidate_raw
    );
    assert_eq!(
        generic.normalized_first_root_request_commitment,
        expected.generic_normalized
    );
    assert_eq!(
        candidate.normalized_first_root_request_commitment,
        expected.candidate_normalized
    );
    assert_eq!(
        generic.normalized_first_root_base_commitment,
        expected.normalized_base
    );
    assert_eq!(
        candidate.normalized_first_root_base_commitment,
        expected.normalized_base
    );
    assert_eq!(generic.first_root_treatment_diff_commitment, None);
    assert_eq!(
        candidate.first_root_treatment_diff_commitment,
        Some(expected.treatment.clone())
    );
    assert_ne!(
        generic.first_root_provider_request_commitment,
        generic.app_server_transcript_sha256
    );
    assert_ne!(
        candidate.first_root_treatment_diff_commitment,
        candidate.skill_use_evidence_sha256
    );

    let verification: serde_json::Value = serde_json::from_slice(
        &fs::read(
            run.private_root
                .join("replay-coordinator/replay-pair-verification.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        verification["requestParity"],
        json!({
            "schemaVersion": 1,
            "normalizedBaseCommitment": expected.normalized_base,
            "genericRawCommitment": expected.generic_raw,
            "genericNormalizedCommitment": expected.generic_normalized,
            "candidateRawCommitment": expected.candidate_raw,
            "candidateNormalizedCommitment": expected.candidate_normalized,
            "candidateTreatmentDiffCommitment": expected.treatment
        })
    );
}

#[test]
fn replay_request_fixture_drift_fails_before_manifests() {
    let source_manifest =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let source_root = source_manifest.parent().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture_root = temp.path().join("fixtures");
    fs::create_dir(&fixture_root).unwrap();
    let mut fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(&source_manifest).unwrap()).unwrap();
    for entry in fixture_set["fixtures"].as_array().unwrap() {
        let path = entry["path"].as_str().unwrap();
        fs::copy(source_root.join(path), fixture_root.join(path)).unwrap();
    }

    let request_path = fixture_root.join("replay-generic-request.json");
    let mut request: serde_json::Value =
        serde_json::from_slice(&fs::read(&request_path).unwrap()).unwrap();
    request["model"] = json!("drifted-replay-model");
    let request_bytes = serde_json::to_vec_pretty(&request).unwrap();
    fs::write(&request_path, &request_bytes).unwrap();
    let request_entry = fixture_set["fixtures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "genericRequest")
        .unwrap();
    request_entry["sha256"] = json!(test_sha256(&request_bytes));
    let fixture_sha256 = fixture_set["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["name"].as_str().unwrap(),
                entry["sha256"].as_str().unwrap(),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    let pair_inputs = [
        "case",
        "genericRequest",
        "candidateRequest",
        "genericTranscript",
        "candidateTranscript",
        "leadSkill",
    ]
    .map(|name| json!({"name": name, "sha256": fixture_sha256[name]}));
    let canonical_pair_inputs =
        crate::jcs::canonicalize_value(&serde_json::Value::Array(pair_inputs.into())).unwrap();
    let mut pair_hasher = Sha256::new();
    pair_hasher.update(b"AI-IP-REPLAY-PAIR-V2\0");
    pair_hasher.update(b"synthetic-replay-fork");
    pair_hasher.update(canonical_pair_inputs);
    let mut attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-attestation.json")).unwrap())
            .unwrap();
    attestation["pairId"] = json!(format!("{:x}", pair_hasher.finalize()));
    let attestation_bytes = serde_json::to_vec_pretty(&attestation).unwrap();
    fs::write(
        fixture_root.join("replay-attestation.json"),
        &attestation_bytes,
    )
    .unwrap();
    fixture_set["fixtures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "attestation")
        .unwrap()["sha256"] = json!(test_sha256(&attestation_bytes));
    fs::write(
        fixture_root.join("replay-fixture-set.json"),
        serde_json::to_vec_pretty(&fixture_set).unwrap(),
    )
    .unwrap();
    assert_replay_request_fixture_digests(&fixture_root);

    let prepared = prepare_replay_test_context(&fixture_root).unwrap();
    let result = crate::run_replay_pair(crate::ReplayPairArgs {
        frozen_run_context: prepared.frozen.clone(),
    });
    assert!(result.is_err(), "one-arm request drift was accepted");
    assert!(
        result.unwrap_err().to_string().contains(
            "first-root requests differ outside the one canonical target Skill treatment"
        )
    );
    let coordinator = prepared.private_root.join("replay-coordinator");
    assert!(!coordinator.join("run-1-manifest.json").exists());
    assert!(!coordinator.join("run-2-manifest.json").exists());
}

#[test]
fn paired_outputs_are_independently_valid_and_may_differ() {
    let run = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    assert!(run.result.is_ok(), "pair failed: {:?}", run.result.err());
    let manifests = read_native_mock_manifests(&run.live_root);
    let generic = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Generic)
        .unwrap();
    let candidate = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Candidate)
        .unwrap();
    assert_ne!(
        generic.content_package_sha256,
        candidate.content_package_sha256
    );
    let verification_path = run.live_root.join("coordinator/pair-verification.json");
    let verification_text = fs::read_to_string(verification_path).unwrap();
    let verification: serde_json::Value = serde_json::from_str(&verification_text).unwrap();
    assert_eq!(
        verification["genericContentPackageSha256"],
        generic.content_package_sha256
    );
    assert_eq!(
        verification["candidateContentPackageSha256"],
        candidate.content_package_sha256
    );
    assert!(verification.get("contentPackageSha256").is_none());
    assert!(!verification_text.contains("Synthetic body"));
    assert!(!verification_text.contains("Refined synthetic body"));
}

#[test]
fn replay_pair_executes_frozen_typed_pair_without_live_or_provider_surfaces() {
    let run = run_frozen_replay_pair().unwrap();
    assert_eq!(run.frozen_json["executionMode"], json!("replay"));
    assert_eq!(run.frozen_json["providerMode"], json!("not-run"));
    for forbidden in [
        "providerUpstreamUrl",
        "providerRole",
        "approvalCommitment",
        "authorizedTotalCostFen",
        "bearer",
    ] {
        assert!(run.frozen_json.get(forbidden).is_none());
    }
    for manifest in &run.manifests {
        manifest.validate_execution_mode().unwrap();
        assert_eq!(manifest.execution_mode, ExecutionMode::Replay);
        assert_eq!(manifest.authorized_evaluation_run_cost_fen, 0);
        assert_eq!(manifest.provider_request_attempt_count, 0);
    }
    let manifests = &run.manifests;
    let generic = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Generic)
        .unwrap();
    let candidate = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Candidate)
        .unwrap();
    assert_eq!(generic.native_skill_sha256, None);
    assert_eq!(generic.skill_use_evidence_sha256, None);
    assert_eq!(generic.first_root_treatment_diff_commitment, None);
    assert!(candidate.native_skill_sha256.is_some());
    assert!(candidate.skill_use_evidence_sha256.is_some());
    assert!(candidate.first_root_treatment_diff_commitment.is_some());
    assert_eq!(
        generic.normalized_base_catalog_sha256,
        candidate.normalized_base_catalog_sha256
    );
    assert_ne!(
        generic.content_package_sha256,
        candidate.content_package_sha256
    );
    let coordinator = run.private_root.join("replay-coordinator");
    assert!(!coordinator.join("attempt-index.jsonl").exists());
    assert!(!coordinator.join("receipts").exists());
}

#[test]
fn replay_pair_reverifies_every_fixture_set_byte_before_creating_evidence() {
    let source_manifest =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let source_root = source_manifest.parent().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture_root = temp.path().join("fixtures");
    fs::create_dir(&fixture_root).unwrap();
    for leaf in [
        "replay-fixture-set.json",
        "replay-case.json",
        "replay-generic-request.json",
        "replay-candidate-request.json",
        "replay-transcript.jsonl",
        "replay-candidate-transcript.jsonl",
        "replay-lead-skill.md",
        "replay-attestation.json",
    ] {
        fs::copy(source_root.join(leaf), fixture_root.join(leaf)).unwrap();
    }
    let private_root = temp.path().join("private");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let codex_binary = private_root.join("synthetic-codex");
    fs::write(&codex_binary, b"synthetic\n").unwrap();
    let frozen = private_root.join("frozen-run-context.json");
    crate::freeze_replay_context(crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.clone(),
        fork_sha: "synthetic-replay-fork".to_string(),
        private_root: private_root.clone(),
        codex_bin: codex_binary,
        case: fixture_root.join("replay-case.json"),
        transcript: fixture_root.join("replay-transcript.jsonl"),
        fixture_set_manifest: fixture_root.join("replay-fixture-set.json"),
        output: frozen.clone(),
    })
    .unwrap();
    fs::write(
        fixture_root.join("replay-attestation.json"),
        b"{\"tampered\":true}\n",
    )
    .unwrap();

    let error = crate::run_replay_pair(crate::ReplayPairArgs {
        frozen_run_context: frozen,
    })
    .unwrap_err();
    assert!(error.to_string().contains("bytes or identity changed"));
    assert!(!private_root.join("replay-coordinator").exists());
}

#[test]
fn replay_freeze_rejects_attestation_not_bound_to_acyclic_pair() {
    let source_manifest =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let source_root = source_manifest.parent().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture_root = temp.path().join("fixtures");
    fs::create_dir(&fixture_root).unwrap();
    for leaf in [
        "replay-fixture-set.json",
        "replay-case.json",
        "replay-generic-request.json",
        "replay-candidate-request.json",
        "replay-transcript.jsonl",
        "replay-candidate-transcript.jsonl",
        "replay-lead-skill.md",
        "replay-attestation.json",
    ] {
        fs::copy(source_root.join(leaf), fixture_root.join(leaf)).unwrap();
    }
    let attestation_path = fixture_root.join("replay-attestation.json");
    let mut attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation_path).unwrap()).unwrap();
    attestation["pairId"] = serde_json::json!("0".repeat(64));
    fs::write(
        &attestation_path,
        serde_json::to_vec_pretty(&attestation).unwrap(),
    )
    .unwrap();
    let mut fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-fixture-set.json")).unwrap())
            .unwrap();
    fixture_set["fixtures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "attestation")
        .unwrap()["sha256"] = serde_json::json!(test_sha256(&fs::read(&attestation_path).unwrap()));
    fs::write(
        fixture_root.join("replay-fixture-set.json"),
        serde_json::to_vec_pretty(&fixture_set).unwrap(),
    )
    .unwrap();

    let private_root = temp.path().join("private");
    fs::create_dir(&private_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&private_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let codex_binary = private_root.join("synthetic-codex");
    fs::write(&codex_binary, b"synthetic\n").unwrap();
    let output = private_root.join("frozen-run-context.json");
    let error = crate::freeze_replay_context(crate::model::ReplayFreezeArgs {
        repo_root: fixture_root.clone(),
        fork_sha: "synthetic-replay-fork".to_string(),
        private_root,
        codex_bin: codex_binary,
        case: fixture_root.join("replay-case.json"),
        transcript: fixture_root.join("replay-transcript.jsonl"),
        fixture_set_manifest: fixture_root.join("replay-fixture-set.json"),
        output: output.clone(),
    })
    .unwrap_err();

    assert!(error.to_string().contains("does not bind the frozen pair"));
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn replay_freeze_reuses_initially_verified_case_and_attestation_buffers_after_swap() {
    let source_manifest =
        codex_utils_cargo_bin::find_resource!("tests/fixtures/replay-fixture-set.json").unwrap();
    let source_root = source_manifest.parent().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let fixture_root = temp.path().join("fixtures");
    fs::create_dir(&fixture_root).unwrap();
    for leaf in [
        "replay-fixture-set.json",
        "replay-case.json",
        "replay-generic-request.json",
        "replay-candidate-request.json",
        "replay-transcript.jsonl",
        "replay-candidate-transcript.jsonl",
        "replay-lead-skill.md",
        "replay-attestation.json",
    ] {
        fs::copy(source_root.join(leaf), fixture_root.join(leaf)).unwrap();
    }
    let case_path = fixture_root.join("replay-case.json");
    let mut invalid_case = serde_json::to_value(mission_case()).unwrap();
    invalid_case["objective"] = json!("");
    let invalid_case_bytes = serde_json::to_vec_pretty(&invalid_case).unwrap();
    fs::write(&case_path, &invalid_case_bytes).unwrap();
    let mut fixture_set: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture_root.join("replay-fixture-set.json")).unwrap())
            .unwrap();
    fixture_set["fixtures"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entry| entry["name"] == "case")
        .unwrap()["sha256"] = json!(test_sha256(&invalid_case_bytes));
    let commitments = fixture_set["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["name"] != "attestation")
        .map(|entry| json!({"name": entry["name"], "sha256": entry["sha256"]}))
        .collect::<Vec<_>>();
    let mut pair_hasher = Sha256::new();
    pair_hasher.update(b"AI-IP-REPLAY-PAIR-V2\0");
    pair_hasher.update(b"synthetic-replay-fork");
    pair_hasher
        .update(crate::jcs::canonicalize_value(&serde_json::Value::Array(commitments)).unwrap());
    let old_pair_id = format!("{:x}", pair_hasher.finalize());
    fs::write(
        fixture_root.join("replay-fixture-set.json"),
        serde_json::to_vec_pretty(&fixture_set).unwrap(),
    )
    .unwrap();
    let replacement_case = temp.path().join("replacement-case.json");
    let valid_case_bytes = serde_json::to_vec_pretty(&mission_case()).unwrap();
    fs::write(&replacement_case, &valid_case_bytes).unwrap();
    let attestation_path = fixture_root.join("replay-attestation.json");
    let mut replacement_attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation_path).unwrap()).unwrap();
    replacement_attestation["pairId"] = json!(old_pair_id);
    replacement_attestation["caseSha256"] = json!(test_sha256(&valid_case_bytes));
    replacement_attestation["sourceMaterialsSha256"] = json!(test_sha256(
        &serde_json::to_vec(&mission_case().materials).unwrap()
    ));
    let replacement_attestation_path = temp.path().join("replacement-attestation.json");
    fs::write(
        &replacement_attestation_path,
        serde_json::to_vec_pretty(&replacement_attestation).unwrap(),
    )
    .unwrap();
    let private_root = temp.path().join("private");
    create_owner_only_test_dir(&private_root);
    let codex_binary = private_root.join("synthetic-codex");
    fs::write(&codex_binary, b"synthetic\n").unwrap();
    let output = private_root.join("frozen-run-context.json");
    let result = crate::runner::freeze_replay_context_with_hook(
        crate::model::ReplayFreezeArgs {
            repo_root: fixture_root.clone(),
            fork_sha: "synthetic-replay-fork".to_string(),
            private_root,
            codex_bin: codex_binary,
            case: case_path.clone(),
            transcript: fixture_root.join("replay-transcript.jsonl"),
            fixture_set_manifest: fixture_root.join("replay-fixture-set.json"),
            output: output.clone(),
        },
        || {
            fs::rename(&case_path, temp.path().join("invalid-case.json"))?;
            fs::rename(&replacement_case, &case_path)?;
            fs::rename(&attestation_path, temp.path().join("old-attestation.json"))?;
            fs::rename(&replacement_attestation_path, &attestation_path)?;
            Ok(())
        },
    );
    assert!(result.is_err(), "freeze consumed pathname-swapped bytes");
    assert!(!output.exists());
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
        sha256: [1; 32],
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
        require_proof_bindings: false,
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
        "commitmentKeySha256": "c".repeat(64),
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
        require_proof_bindings: false,
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
    let supplier_statements = imported_inputs.join("supplier-statements");
    fs::create_dir(&artifacts_dir).unwrap();
    fs::create_dir(&frozen_inputs).unwrap();
    fs::create_dir_all(&case_root).unwrap();
    fs::create_dir(&supplier_statements).unwrap();
    fs::create_dir_all(temp.path().join("coordinator/cost")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&frozen_inputs, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&imported_inputs, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&case_root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&supplier_statements, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(
            temp.path().join("coordinator"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::set_permissions(
            temp.path().join("coordinator/cost"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
    }
    let private_leaf = |name: &str| match name {
        "skill" => Some("lead-skill.md"),
        "schema" => Some("content-package-schema.json"),
        "prompt" => Some("root-prompt.txt"),
        "additionalContext" => Some("additional-context.txt"),
        "threadStartRequest" => Some("thread-start-request.json"),
        "turnStartRequest" => Some("turn-start-request.json"),
        _ => None,
    };
    let case_bytes = serde_json::to_vec_pretty(&mission_case()).unwrap();
    let materials_bytes = serde_json::to_vec(&mission_case().materials).unwrap();
    let mut attestation = complete_native_attestation_value(&case_bytes, temp.path());
    attestation["candidateSha"] = json!(head);
    attestation["maxProviderRequestAttemptsPerRun"] = json!(2);
    attestation["maxTotalTokensPerRun"] = json!(100);
    attestation["maxElapsedSecondsPerRun"] = json!(5);
    attestation["maxOutputTokensPerRequest"] = json!(321);
    attestation["providerBudgetEvidenceSha256"] = json!(test_sha256(b"providerBudgetEvidence\n"));
    attestation["rateCardSha256"] = json!(test_sha256(b"rateCard\n"));
    attestation["billingPolicyCommitment"] = json!(test_sha256(b"billingPolicy\n"));
    attestation["fxPolicySha256"] = json!(test_sha256(b"fxPolicy\n"));
    let attestation_bytes = serde_json::to_vec_pretty(&attestation).unwrap();
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
                "providerBudgetEvidence" | "rateCard" | "billingPolicy" | "fxPolicy" => (
                    imported_inputs.join(match *name {
                        "providerBudgetEvidence" => "provider-budget-evidence.json",
                        "rateCard" => "rate-card.json",
                        "billingPolicy" => "billing-policy.json",
                        "fxPolicy" => "fx-policy.json",
                        _ => unreachable!(),
                    }),
                    format!("{name}\n").into_bytes(),
                ),
                "additionalContext" => (
                    frozen_inputs.join("additional-context.txt"),
                    codex_ai_ip_runtime::evaluation_context(&mission_case())
                        .unwrap()
                        .into_bytes(),
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
    let pair_id = test_sha256(format!("{head}:local-mock").as_bytes());
    let commitment_key_bytes = [0x5a; 32];
    crate::secure_fs::write_owner_only_new(
        &temp
            .path()
            .canonicalize()
            .unwrap()
            .join("coordinator/commitment-key.bin"),
        &commitment_key_bytes,
    )
    .unwrap();
    let commitment_key =
        crate::proof_commitment::ProofCommitmentKey::from_test_bytes(commitment_key_bytes);
    let unordered_context = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "executionMode": "live",
        "providerMode": "not-run",
        "pairId": pair_id.clone(),
        "publicRunId": crate::proof_commitment::derive_public_run_id(
            &commitment_key,
            &pair_id,
        ).unwrap(),
        "commitmentKeySha256": test_sha256(&commitment_key_bytes),
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
        "supplierStatementPolicy": {
            "directory": "inputs/supplier-statements",
            "allowedLeaves": ["generic.json", "candidate.json"],
            "nestedEntriesAllowed": false
        },
        "artifacts": artifacts,
    }))
    .unwrap();
    fs::write(
        &path,
        crate::runner::encode_native_context_for_test(&unordered_context).unwrap(),
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
fn native_verified_context_retains_inputs_and_rejects_drift() {
    let temp = tempfile::tempdir().unwrap();
    let path = strict_live_context(&temp);
    let verified = verify_frozen_context(&path).unwrap();
    assert_eq!(verified.raw_bytes().unwrap(), fs::read(&path).unwrap());
    assert_eq!(verified.private_root(), temp.path().canonicalize().unwrap());
    assert_eq!(verified.model_label(), "local-mock");
    assert_eq!(verified.provider_mode(), "not-run");
    assert_eq!(verified.max_total_tokens_per_run(), 100);
    assert_eq!(verified.max_elapsed_seconds_per_run(), 5);
    verified.reverify_all().unwrap();

    let context: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let source = context["artifacts"]["source"]["path"].as_str().unwrap();
    fs::write(source, b"changed source bytes\n").unwrap();
    assert!(verified.reverify_all().is_err());
}

#[test]
fn frozen_context_identity_replacement_native_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let path = strict_live_context(&temp);
    let verified = verify_frozen_context(&path).unwrap();

    replace_with_same_owner_only_bytes(&path);

    let error = verified.reverify_all().unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "reverify native frozen context identity: frozen context path identity changed after verification"
    );
}

#[test]
fn native_attestation_is_cross_bound_at_the_verify_boundary() {
    for mutation in [
        "candidate",
        "privateRoot",
        "case",
        "materials",
        "budget",
        "rateCard",
        "billingPolicy",
        "fxPolicy",
        "limits",
        "approval",
        "retention",
        "timestamp",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let context_path = strict_live_context(&temp);
        verify_frozen_context(&context_path).unwrap();
        let mut context: serde_json::Value =
            serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
        let attestation_path = std::path::PathBuf::from(
            context["artifacts"]["attestation"]["path"]
                .as_str()
                .unwrap(),
        );
        let mut attestation: serde_json::Value =
            serde_json::from_slice(&fs::read(&attestation_path).unwrap()).unwrap();
        match mutation {
            "candidate" => attestation["candidateSha"] = json!("changed-candidate"),
            "privateRoot" => attestation["privateRoot"] = json!("/changed/private/root"),
            "case" => attestation["caseSha256"] = json!("1".repeat(64)),
            "materials" => attestation["sourceMaterialsSha256"] = json!("2".repeat(64)),
            "budget" => attestation["providerBudgetEvidenceSha256"] = json!("3".repeat(64)),
            "rateCard" => attestation["rateCardSha256"] = json!("4".repeat(64)),
            "billingPolicy" => attestation["billingPolicyCommitment"] = json!("5".repeat(64)),
            "fxPolicy" => attestation["fxPolicySha256"] = json!("6".repeat(64)),
            "limits" => attestation["maxTotalTokensPerRun"] = json!(101),
            "approval" => attestation["approvedTotalFen"] = json!(1),
            "retention" => attestation["retentionDeadline"] = json!("2026-08-27T09:00:00Z"),
            "timestamp" => attestation["caseSelectedAt"] = json!("2099-08-27T07:00:00Z"),
            other => panic!("unknown native verify mutation {other}"),
        }
        let attestation_bytes = serde_json::to_vec_pretty(&attestation).unwrap();
        fs::write(&attestation_path, &attestation_bytes).unwrap();
        context["artifacts"]["attestation"]["sha256"] = json!(test_sha256(&attestation_bytes));
        fs::write(&context_path, serde_json::to_vec_pretty(&context).unwrap()).unwrap();
        assert!(
            verify_frozen_context(&context_path).is_err(),
            "verify accepted native attestation {mutation} drift"
        );
    }
}

#[test]
fn native_attestation_is_cross_bound_at_the_freeze_boundary() {
    for mutation in [
        "candidate",
        "providerRole",
        "budget",
        "limits",
        "rateCard",
        "retention",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let seed_context = strict_live_context(&temp);
        let seed: serde_json::Value =
            serde_json::from_slice(&fs::read(seed_context).unwrap()).unwrap();
        let artifacts = seed["artifacts"].as_object().unwrap();
        let artifact =
            |name: &str| std::path::PathBuf::from(artifacts[name]["path"].as_str().unwrap());
        let private_root = temp.path().join(format!("native-freeze-{mutation}"));
        create_owner_only_test_dir(&private_root);
        let private_root = private_root.canonicalize().unwrap();
        let codex_binary = temp.path().join(format!("native-codex-{mutation}"));
        fs::copy(std::env::current_exe().unwrap(), &codex_binary).unwrap();
        let (case, material_root, attestation) = write_nonempty_source_fixture(
            temp.path(),
            &private_root,
            "notes/evidence.txt",
            None,
            None,
        );
        let output = private_root.join("frozen-run-context.json");
        let mut args = crate::model::LiveFreezeArgs {
            repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
            evidence_repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
            fork_sha: seed["repoHead"].as_str().unwrap().to_string(),
            private_root,
            codex_bin: codex_binary,
            case,
            material_root,
            attestation,
            provider_budget_evidence: artifact("providerBudgetEvidence"),
            rate_card: artifact("rateCard"),
            billing_policy: artifact("billingPolicy"),
            fx_policy: artifact("fxPolicy"),
            lead_skill: artifact("skill"),
            model_label: "local-mock".to_string(),
            provider_label: "local-mock".to_string(),
            provider_role: ProviderRole::ApprovedReference,
            provider_upstream_url: "http://127.0.0.1:1/v1/responses".to_string(),
            authorized_total_cost_fen: 0,
            authorized_per_run_cost_fen: 0,
            max_provider_request_attempts_per_run: 2,
            max_total_tokens_per_run: 10,
            max_elapsed_seconds_per_run: 180,
            max_output_tokens_per_request: 17,
            output: output.clone(),
        };
        install_retention_managed_cost_inputs(&mut args);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&args.attestation).unwrap()).unwrap();
        match mutation {
            "candidate" => value["candidateSha"] = json!("changed-candidate"),
            "providerRole" => value["providerRole"] = json!("targetVolcengine"),
            "budget" => value["approvedTotalFen"] = json!(1),
            "limits" => value["maxTotalTokensPerRun"] = json!(11),
            "rateCard" => value["rateCardSha256"] = json!("7".repeat(64)),
            "retention" => value["retentionDeadline"] = json!("2026-08-27T09:00:00Z"),
            other => panic!("unknown native freeze mutation {other}"),
        }
        fs::write(
            &args.attestation,
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
        assert!(
            crate::runner::freeze_live_context(args).is_err(),
            "freeze accepted native attestation {mutation} drift"
        );
        assert!(!output.exists());
    }
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
    let order_dir = frozen.canonical_path().parent().unwrap().join("order");
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
fn arm_order_context_recheck_rejects_over_cap_before_outputs() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = strict_live_context(&temp);
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let mut context = fs::OpenOptions::new()
        .append(true)
        .open(frozen.canonical_path())
        .unwrap();
    std::io::Write::write_all(&mut context, &vec![b'x'; 1024 * 1024]).unwrap();
    context.sync_all().unwrap();
    let order_dir = frozen.canonical_path().parent().unwrap().join("order");

    let error = commit_arm_order(&frozen, &order_dir).unwrap_err();

    assert!(!order_dir.exists());
    assert!(!order_dir.join("arm-order-seed.bin").exists());
    assert_eq!(error.to_string(), "frozen context exceeds its byte cap");
}

#[test]
fn production_coordinator_accepts_only_the_opaque_os_committed_order() {
    let temp = tempfile::tempdir().unwrap();
    let frozen_path = strict_live_context(&temp);
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let order_dir = frozen.canonical_path().parent().unwrap().join("order");
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
        require_proof_bindings: false,
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
    assert!(Cli::try_parse_from(["codex-ai-ip-eval", "replay-pair"]).is_err());
    assert!(
        Cli::try_parse_from([
            "codex-ai-ip-eval",
            "replay-pair",
            "--frozen-run-context",
            "/private/replay-frozen.json"
        ])
        .is_ok()
    );
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
fn score_cli_has_one_exact_four_path_argument_shape() {
    let cli = Cli::try_parse_from([
        "codex-ai-ip-eval",
        "score",
        "--mapping-dir",
        "coordinator/mappings",
        "--reviews-dir",
        "reviews",
        "--output",
        "coordinator/decision.private.json",
        "--frozen-run-context",
        "/private/frozen-run-context.json",
    ])
    .unwrap();
    let crate::EvalCommand::Score(args) = cli.command else {
        panic!("score command parsed as the wrong variant");
    };
    assert_eq!(
        args.mapping_dir,
        std::path::PathBuf::from("coordinator/mappings")
    );
    assert_eq!(args.reviews_dir, std::path::PathBuf::from("reviews"));
    assert_eq!(
        args.output,
        std::path::PathBuf::from("coordinator/decision.private.json")
    );
    assert_eq!(
        args.frozen_run_context,
        std::path::PathBuf::from("/private/frozen-run-context.json")
    );

    for missing in [
        "--mapping-dir",
        "--reviews-dir",
        "--output",
        "--frozen-run-context",
    ] {
        let arguments = [
            ("--mapping-dir", "coordinator/mappings"),
            ("--reviews-dir", "reviews"),
            ("--output", "coordinator/decision.private.json"),
            ("--frozen-run-context", "/private/frozen-run-context.json"),
        ];
        let mut command = vec!["codex-ai-ip-eval", "score"];
        for (name, value) in arguments {
            if name != missing {
                command.extend([name, value]);
            }
        }
        assert!(
            Cli::try_parse_from(command).is_err(),
            "accepted missing {missing}"
        );
    }
    assert!(Cli::try_parse_from(["codex-ai-ip-eval", "score", "--input", "legacy.json",]).is_err());
}

fn write_nonempty_source_fixture(
    root: &std::path::Path,
    private_root: &std::path::Path,
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
    let attestation_path = root.join("external-attestation.json");
    fs::write(
        &attestation_path,
        serde_json::to_vec_pretty(&complete_native_attestation_value(
            &case_bytes,
            private_root,
        ))
        .unwrap(),
    )
    .unwrap();
    if let Some(attested_case_digest) = attested_case_digest {
        let mut attestation: serde_json::Value =
            serde_json::from_slice(&fs::read(&attestation_path).unwrap()).unwrap();
        attestation["caseSha256"] = json!(attested_case_digest);
        fs::write(
            &attestation_path,
            serde_json::to_vec_pretty(&attestation).unwrap(),
        )
        .unwrap();
    }
    (case_path, material_root, attestation_path)
}

fn retain_test_import_inputs(
    private_root: &std::path::Path,
    attestation: &std::path::Path,
) -> anyhow::Result<crate::runner::RetainedLiveInputDirectory> {
    let private_root = private_root.canonicalize()?;
    let inputs = private_root.join("inputs");
    if !inputs.exists() {
        create_owner_only_test_dir(&inputs);
    }
    let fixed_attestation = inputs.join("held-out-attestation.json");
    fs::copy(attestation, &fixed_attestation)?;
    let mut paths = BTreeMap::new();
    for (name, leaf, fixture) in [
        (
            "budget",
            "provider-budget-evidence.json",
            "provider-budget-evidence.canonical.json",
        ),
        (
            "rate",
            "rate-card.json",
            "provider-rate-card.canonical.json",
        ),
        (
            "billing",
            "billing-policy.json",
            "billing-policy.canonical.json",
        ),
        ("fx", "fx-policy.json", "fx-policy.canonical.json"),
    ] {
        let resource = format!("tests/fixtures/contracts/06b1/{fixture}");
        let source = codex_utils_cargo_bin::find_resource!(resource).unwrap();
        let destination = inputs.join(leaf);
        fs::copy(source, &destination)?;
        paths.insert(name, destination);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in paths.values().chain([&fixed_attestation]) {
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
    }
    crate::runner::RetainedLiveInputDirectory::retain(
        &private_root,
        &fixed_attestation,
        &paths["budget"],
        &paths["rate"],
        &paths["billing"],
        &paths["fx"],
    )
}

fn import_test_source(
    case: &std::path::Path,
    material_root: &std::path::Path,
    attestation: &std::path::Path,
    private_root: &std::path::Path,
) -> anyhow::Result<crate::runner::ImportedSourceProof> {
    let retained = retain_test_import_inputs(private_root, attestation)?;
    crate::runner::import_live_source_proof_into_existing_inputs(case, material_root, &retained)
}

fn complete_native_attestation_value(
    case_bytes: &[u8],
    private_root: &std::path::Path,
) -> serde_json::Value {
    let resource = codex_utils_cargo_bin::find_resource!(
        "tests/fixtures/contracts/06a/canonical-native-attestation.json"
    )
    .unwrap();
    let mut attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(resource).unwrap()).unwrap();
    let mission: HeldOutMissionCase = serde_json::from_slice(case_bytes).unwrap();
    attestation["candidateSha"] = json!("synthetic-candidate-sha");
    attestation["candidateFrozenAt"] = json!("2026-08-27T06:00:00Z");
    attestation["caseSelectedAt"] = json!("2026-08-27T07:00:00Z");
    attestation["caseSha256"] = json!(test_sha256(case_bytes));
    attestation["sourceMaterialsSha256"] = json!(test_sha256(
        &serde_json::to_vec(&mission.materials).unwrap()
    ));
    attestation["privateRoot"] = json!(private_root.canonicalize().unwrap());
    attestation["approvedTotalFen"] = json!(0);
    attestation["approvedPerRunFen"] = json!(0);
    attestation["signedAt"] = json!("2026-08-27T08:00:00Z");
    attestation["retentionDeadline"] = json!("2099-09-04T08:00:00Z");
    attestation["rateEffectiveAt"] = json!("2026-08-27T00:00:00Z");
    for (index, reviewer) in attestation["reviewers"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        reviewer["declaredAt"] = json!(format!("2026-08-27T07:2{index}:00Z"));
        let mut payload = reviewer.clone();
        payload
            .as_object_mut()
            .unwrap()
            .remove("signedPayloadSha256");
        payload
            .as_object_mut()
            .unwrap()
            .remove("signatureEvidenceSha256");
        reviewer["signedPayloadSha256"] = json!(
            crate::jcs::commitment(
                b"AI-IP-REVIEWER-QUALIFICATION-V1\0",
                &serde_json::to_vec(&payload).unwrap(),
            )
            .unwrap()
            .sha256
        );
    }
    attestation
}

fn bind_native_attestation_to_live_args(args: &crate::model::LiveFreezeArgs) {
    let mut attestation: serde_json::Value =
        serde_json::from_slice(&fs::read(&args.attestation).unwrap()).unwrap();
    attestation["candidateSha"] = json!(args.fork_sha);
    attestation["privateRoot"] = json!(args.private_root.canonicalize().unwrap());
    attestation["providerRole"] = json!(match args.provider_role {
        ProviderRole::TargetVolcengine => "targetVolcengine",
        ProviderRole::ApprovedReference => "approvedReference",
    });
    attestation["approvedTotalFen"] = json!(args.authorized_total_cost_fen);
    attestation["approvedPerRunFen"] = json!(args.authorized_per_run_cost_fen);
    attestation["maxProviderRequestAttemptsPerRun"] =
        json!(args.max_provider_request_attempts_per_run);
    attestation["maxTotalTokensPerRun"] = json!(args.max_total_tokens_per_run);
    attestation["maxElapsedSecondsPerRun"] = json!(args.max_elapsed_seconds_per_run);
    attestation["maxOutputTokensPerRequest"] = json!(args.max_output_tokens_per_request);
    for (field, path) in [
        (
            "providerBudgetEvidenceSha256",
            &args.provider_budget_evidence,
        ),
        ("rateCardSha256", &args.rate_card),
        ("billingPolicyCommitment", &args.billing_policy),
        ("fxPolicySha256", &args.fx_policy),
    ] {
        attestation[field] = json!(test_sha256(&fs::read(path).unwrap()));
    }
    fs::write(
        &args.attestation,
        serde_json::to_vec_pretty(&attestation).unwrap(),
    )
    .unwrap();
}

fn native_freeze_args_for_cost_inputs(
    temp: &tempfile::TempDir,
    retention_managed: bool,
) -> crate::model::LiveFreezeArgs {
    let seed_context = strict_live_context(temp);
    let seed: serde_json::Value = serde_json::from_slice(&fs::read(seed_context).unwrap()).unwrap();
    let artifacts = seed["artifacts"].as_object().unwrap();
    let artifact = |name: &str| std::path::PathBuf::from(artifacts[name]["path"].as_str().unwrap());
    let private_root = temp.path().join(if retention_managed {
        "retention-managed-private"
    } else {
        "arbitrary-source-private"
    });
    create_owner_only_test_dir(&private_root);
    let private_root = private_root.canonicalize().unwrap();
    let codex_binary = temp.path().join(if retention_managed {
        "retention-managed-codex"
    } else {
        "arbitrary-source-codex"
    });
    fs::copy(std::env::current_exe().unwrap(), &codex_binary).unwrap();
    let (case, material_root, external_attestation) =
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);
    let mut args = crate::model::LiveFreezeArgs {
        repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        evidence_repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        fork_sha: seed["repoHead"].as_str().unwrap().to_string(),
        private_root: private_root.clone(),
        codex_bin: codex_binary,
        case,
        material_root,
        attestation: external_attestation,
        provider_budget_evidence: artifact("providerBudgetEvidence"),
        rate_card: artifact("rateCard"),
        billing_policy: artifact("billingPolicy"),
        fx_policy: artifact("fxPolicy"),
        lead_skill: artifact("skill"),
        model_label: "local-mock".to_string(),
        provider_label: "local-mock".to_string(),
        provider_role: ProviderRole::ApprovedReference,
        provider_upstream_url: "http://127.0.0.1:1/v1/responses".to_string(),
        authorized_total_cost_fen: 0,
        authorized_per_run_cost_fen: 0,
        max_provider_request_attempts_per_run: 2,
        max_total_tokens_per_run: 10,
        max_elapsed_seconds_per_run: 180,
        max_output_tokens_per_request: 17,
        output: private_root.join("frozen-run-context.json"),
    };
    if retention_managed {
        install_retention_managed_cost_inputs(&mut args);
    } else {
        bind_native_attestation_to_live_args(&args);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&args.attestation, fs::Permissions::from_mode(0o600)).unwrap();
    }
    args
}

fn install_retention_managed_cost_inputs(args: &mut crate::model::LiveFreezeArgs) {
    let inputs = args.private_root.join("inputs");
    create_owner_only_test_dir(&inputs);
    let attestation = inputs.join("held-out-attestation.json");
    fs::rename(&args.attestation, &attestation).unwrap();
    args.attestation = attestation;
    for (field, leaf, fixture) in [
        (
            "budget",
            "provider-budget-evidence.json",
            "provider-budget-evidence.canonical.json",
        ),
        (
            "rate",
            "rate-card.json",
            "provider-rate-card.canonical.json",
        ),
        (
            "billing",
            "billing-policy.json",
            "billing-policy.canonical.json",
        ),
        ("fx", "fx-policy.json", "fx-policy.canonical.json"),
    ] {
        let resource = format!("tests/fixtures/contracts/06b1/{fixture}");
        let source = codex_utils_cargo_bin::find_resource!(resource).unwrap();
        let destination = inputs.join(leaf);
        fs::copy(source, &destination).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o600)).unwrap();
        }
        match field {
            "budget" => args.provider_budget_evidence = destination,
            "rate" => args.rate_card = destination,
            "billing" => args.billing_policy = destination,
            "fx" => args.fx_policy = destination,
            other => panic!("unknown cost input field {other}"),
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&args.attestation, fs::Permissions::from_mode(0o600)).unwrap();
    }
    bind_native_attestation_to_live_args(args);
}

#[test]
fn native_freeze_requires_retention_managed_cost_inputs() {
    for mutation in [
        "external",
        "home-sibling",
        "repo-sibling",
        "downloads-sibling",
        "old-frozen-inputs",
        "equivalent-copy",
        "wrong-leaf",
        "extra-file",
        "case-directory",
        "materials-manifest",
        "supplier-statements",
        "duplicate-key",
        "over-cap",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut args = native_freeze_args_for_cost_inputs(&temp, mutation != "external");
        match mutation {
            "external" => {}
            "home-sibling" | "repo-sibling" | "downloads-sibling" | "old-frozen-inputs"
            | "equivalent-copy" => {
                let directory = temp.path().join(mutation);
                create_owner_only_test_dir(&directory);
                let copy = directory.join("rate-card.json");
                fs::copy(&args.rate_card, &copy).unwrap();
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&copy, fs::Permissions::from_mode(0o600)).unwrap();
                }
                args.rate_card = copy;
            }
            "wrong-leaf" => {
                let copy = args.private_root.join("inputs/rate-card-copy.json");
                fs::copy(&args.rate_card, &copy).unwrap();
                args.rate_card = copy;
            }
            "extra-file" => {
                fs::write(args.private_root.join("inputs/foo"), b"unexpected\n").unwrap();
            }
            "case-directory" => {
                create_owner_only_test_dir(&args.private_root.join("inputs/case"));
            }
            "materials-manifest" => {
                fs::write(
                    args.private_root.join("inputs/materials-manifest.json"),
                    b"[]",
                )
                .unwrap();
            }
            "supplier-statements" => {
                create_owner_only_test_dir(&args.private_root.join("inputs/supplier-statements"));
            }
            "duplicate-key" => {
                let bytes = fs::read_to_string(&args.rate_card).unwrap();
                fs::write(
                    &args.rate_card,
                    bytes.replacen(
                        "\"schemaVersion\": 1,",
                        "\"schemaVersion\": 1,\"schemaVersion\": 1,",
                        1,
                    ),
                )
                .unwrap();
            }
            "over-cap" => fs::write(&args.rate_card, vec![b' '; 64 * 1024 + 1]).unwrap(),
            other => panic!("unknown fixed-input mutation {other}"),
        }
        let output = args.output.clone();
        assert!(
            crate::runner::freeze_live_context(args).is_err(),
            "freeze accepted {mutation} private input drift"
        );
        assert!(!output.exists(), "freeze published context for {mutation}");
    }

    #[cfg(unix)]
    for mutation in ["symlink", "hardlink"] {
        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let args = native_freeze_args_for_cost_inputs(&temp, true);
        let output = args.output.clone();
        let retained_path = args.rate_card.clone();
        let bytes = fs::read(&retained_path).unwrap();
        let external = temp.path().join(format!("{mutation}-rate-card.json"));
        fs::write(&external, bytes).unwrap();
        fs::set_permissions(&external, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&retained_path).unwrap();
        match mutation {
            "symlink" => symlink(&external, &retained_path).unwrap(),
            "hardlink" => fs::hard_link(&external, &retained_path).unwrap(),
            _ => unreachable!(),
        }
        assert!(
            crate::runner::freeze_live_context(args).is_err(),
            "freeze accepted {mutation} fixed input"
        );
        assert!(!output.exists(), "freeze published context for {mutation}");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let args = native_freeze_args_for_cost_inputs(&temp, true);
        let retained = crate::runner::RetainedLiveInputDirectory::retain(
            &args.private_root,
            &args.attestation,
            &args.provider_budget_evidence,
            &args.rate_card,
            &args.billing_policy,
            &args.fx_policy,
        )
        .unwrap();
        let displaced = temp.path().join("retained-rate-card.json");
        let bytes = fs::read(&args.rate_card).unwrap();
        fs::rename(&args.rate_card, displaced).unwrap();
        fs::write(&args.rate_card, bytes).unwrap();
        fs::set_permissions(&args.rate_card, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(retained.reverify_unchanged().is_err());

        let temp = tempfile::tempdir().unwrap();
        let args = native_freeze_args_for_cost_inputs(&temp, true);
        let retained = crate::runner::RetainedLiveInputDirectory::retain(
            &args.private_root,
            &args.attestation,
            &args.provider_budget_evidence,
            &args.rate_card,
            &args.billing_policy,
            &args.fx_policy,
        )
        .unwrap();
        let inputs = args.private_root.join("inputs");
        let displaced = args.private_root.join("retained-inputs");
        fs::rename(&inputs, displaced).unwrap();
        create_owner_only_test_dir(&inputs);
        assert!(retained.reverify_unchanged().is_err());
    }
}

#[cfg(unix)]
#[test]
fn native_freeze_rejects_transient_retained_input_replacement() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let args = native_freeze_args_for_cost_inputs(&temp, true);
    let output = args.output.clone();
    let attestation = args.attestation.clone();
    let displaced = temp.path().join("retained-attestation.json");
    let original = fs::read(&attestation).unwrap();
    let transient =
        serde_json::to_vec(&serde_json::from_slice::<serde_json::Value>(&original).unwrap())
            .unwrap();
    assert_ne!(transient, original);

    let mut trace = Vec::new();
    let result = crate::runner::freeze_live_context_with_hook(args, |point| {
        match point {
            crate::runner::LiveFreezeHookPoint::BeforeArtifactFreeze => {
                trace.push("before");
                fs::rename(&attestation, &displaced)?;
                fs::write(&attestation, &transient)?;
                fs::set_permissions(&attestation, fs::Permissions::from_mode(0o600))?;
            }
            crate::runner::LiveFreezeHookPoint::AfterArtifactFreeze => {
                trace.push("after");
                fs::remove_file(&attestation)?;
                fs::rename(&displaced, &attestation)?;
            }
        }
        Ok(())
    });
    let restored_inside_freeze = !displaced.exists()
        && fs::read(&attestation)
            .map(|bytes| bytes == original)
            .unwrap_or(false);
    if displaced.exists() {
        let _ = fs::remove_file(&attestation);
        fs::rename(&displaced, &attestation).unwrap();
    }

    assert_eq!(trace, ["before", "after"]);
    assert!(
        restored_inside_freeze,
        "AfterArtifactFreeze did not restore the retained leaf inside freeze"
    );
    let error = result.expect_err("freeze accepted a transient retained replacement");
    assert!(
        format!("{error:#}").contains("retained private file identity changed"),
        "freeze failed for a reason other than retained identity revalidation: {error:#}"
    );
    assert!(
        !output.exists(),
        "freeze published a transient artifact SHA"
    );
}

#[cfg(unix)]
#[test]
fn native_cost_input_artifacts_use_exact_names_and_bytes() {
    use std::os::unix::fs::MetadataExt;

    let temp = tempfile::tempdir().unwrap();
    let args = native_freeze_args_for_cost_inputs(&temp, true);
    let originals = [
        args.attestation.clone(),
        args.provider_budget_evidence.clone(),
        args.rate_card.clone(),
        args.billing_policy.clone(),
        args.fx_policy.clone(),
    ]
    .map(|path| {
        let metadata = fs::metadata(&path).unwrap();
        (
            path.clone(),
            metadata.dev(),
            metadata.ino(),
            fs::read(path).unwrap(),
        )
    });
    let output = args.output.clone();
    let private_root = args.private_root.clone();
    let expected_artifacts = [
        ("attestation", args.attestation.clone()),
        (
            "providerBudgetEvidence",
            args.provider_budget_evidence.clone(),
        ),
        ("rateCard", args.rate_card.clone()),
        ("billingPolicy", args.billing_policy.clone()),
        ("fxPolicy", args.fx_policy.clone()),
    ];

    crate::runner::freeze_live_context(args).unwrap();

    let context: serde_json::Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(
        context["supplierStatementPolicy"],
        json!({
            "directory": "inputs/supplier-statements",
            "allowedLeaves": ["generic.json", "candidate.json"],
            "nestedEntriesAllowed": false
        })
    );
    assert!(context["artifacts"].get("providerBudgetReceipt").is_none());
    for (name, path) in &expected_artifacts {
        assert_eq!(context["artifacts"][*name]["path"], json!(path));
    }
    for (path, dev, ino, bytes) in originals {
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!((metadata.dev(), metadata.ino()), (dev, ino));
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    assert!(!private_root.join("frozen-inputs/rate-card.json").exists());
    assert!(private_root.join("inputs/supplier-statements").is_dir());
    assert!(private_root.join("coordinator/cost").is_dir());

    let context_bytes = fs::read(&output).unwrap();
    crate::private_inventory::bootstrap_private_inventory(
        &private_root,
        context["pairId"].as_str().unwrap(),
        &test_sha256(&context_bytes),
        "2026-08-30T10:00:00.000Z",
    )
    .unwrap();
    let inventory =
        fs::read_to_string(private_root.join("coordinator/private-inventory.jsonl")).unwrap();
    for relative in [
        "inputs/held-out-attestation.json",
        "inputs/provider-budget-evidence.json",
        "inputs/rate-card.json",
        "inputs/billing-policy.json",
        "inputs/fx-policy.json",
        "inputs/supplier-statements",
        "coordinator/cost",
    ] {
        assert!(inventory.contains(&format!("\"relativePath\":\"{relative}\"")));
    }

    let contracts = crate::FrozenContracts::load().unwrap();
    for mutation in [
        "unknown",
        "reversed",
        "extra-leaf",
        "missing-leaf",
        "directory",
        "nested",
        "stale-budget-key",
        "both-budget-keys",
    ] {
        let mut drifted = context.clone();
        match mutation {
            "unknown" => drifted["supplierStatementPolicy"]["unknown"] = json!(true),
            "reversed" => {
                drifted["supplierStatementPolicy"]["allowedLeaves"] =
                    json!(["candidate.json", "generic.json"])
            }
            "extra-leaf" => {
                drifted["supplierStatementPolicy"]["allowedLeaves"] =
                    json!(["generic.json", "candidate.json", "other.json"])
            }
            "missing-leaf" => {
                drifted["supplierStatementPolicy"]["allowedLeaves"] = json!(["generic.json"])
            }
            "directory" => {
                drifted["supplierStatementPolicy"]["directory"] = json!("inputs/statements")
            }
            "nested" => drifted["supplierStatementPolicy"]["nestedEntriesAllowed"] = json!(true),
            "stale-budget-key" => {
                let budget = drifted["artifacts"]
                    .as_object_mut()
                    .unwrap()
                    .remove("providerBudgetEvidence")
                    .unwrap();
                drifted["artifacts"]["providerBudgetReceipt"] = budget;
            }
            "both-budget-keys" => {
                drifted["artifacts"]["providerBudgetReceipt"] =
                    drifted["artifacts"]["providerBudgetEvidence"].clone()
            }
            other => panic!("unknown native context mutation {other}"),
        }
        let bytes = serde_json::to_vec(&drifted).unwrap();
        assert!(
            contracts.validate_native_context(&bytes).is_err(),
            "{mutation}"
        );
        assert!(
            crate::runner::encode_native_context_for_test(&bytes).is_err(),
            "typed context accepted {mutation}"
        );
    }

    for mutation in ["stale", "both"] {
        let mut attestation: serde_json::Value =
            serde_json::from_slice(&fs::read(&expected_artifacts[0].1).unwrap()).unwrap();
        let budget = attestation
            .as_object_mut()
            .unwrap()
            .remove("providerBudgetEvidenceSha256")
            .unwrap();
        attestation["providerBudgetReceiptSha256"] = budget.clone();
        if mutation == "both" {
            attestation["providerBudgetEvidenceSha256"] = budget;
        }
        assert!(
            contracts
                .validate_native_attestation(&serde_json::to_vec(&attestation).unwrap())
                .is_err(),
            "attestation accepted {mutation} budget key"
        );
    }
}

#[cfg(unix)]
#[test]
fn replay_fixture_read_reuses_the_verified_handle_buffer_after_path_swap() {
    let temp = tempfile::tempdir().unwrap();
    let fixture = temp.path().join("fixture.json");
    let replacement = temp.path().join("replacement.json");
    let displaced = temp.path().join("displaced.json");
    let committed = b"{\"committed\":true}\n";
    fs::write(&fixture, committed).unwrap();
    fs::write(&replacement, b"{\"attacker\":true}\n").unwrap();
    let actual = crate::runner::read_replay_reference_with_hook(
        &fixture.canonicalize().unwrap(),
        &test_sha256(committed),
        |path| {
            fs::rename(path, &displaced)?;
            fs::rename(&replacement, path)?;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(actual, committed);
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);

    let imported = import_test_source(&case, &material_root, &attestation, &private_root).unwrap();

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
    let legacy_private = temp.path().join("legacy-private-proof");
    create_owner_only_test_dir(&legacy_private);
    let complete: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation).unwrap()).unwrap();
    fs::write(
        &attestation,
        serde_json::to_vec(&json!({
            "caseSha256": complete["caseSha256"],
            "sourceMaterialsSha256": complete["sourceMaterialsSha256"]
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(import_test_source(&case, &material_root, &attestation, &legacy_private).is_err());
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
            &private_root,
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
            import_test_source(&case, &material_root, &attestation, &private_root).is_err(),
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);

    assert!(import_test_source(&case, &material_root, &attestation, &private_root).is_err());
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&attestation).unwrap()).unwrap();
    value["unapprovedField"] = json!(true);
    fs::write(&attestation, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

    assert!(import_test_source(&case, &material_root, &attestation, &private_root).is_err());
    assert!(!private_root.join("inputs/case").exists());
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);

    let retained = retain_test_import_inputs(&private_root, &attestation).unwrap();
    let result = crate::runner::import_live_source_proof_into_existing_inputs_with_hook(
        &case,
        &material_root,
        &retained,
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);
    let imported = import_test_source(&case, &material_root, &attestation, &private_root).unwrap();
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
        write_nonempty_source_fixture(temp.path(), &private_root, "notes/evidence.txt", None, None);
    let imported = import_test_source(&case, &material_root, &attestation, &private_root).unwrap();
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
            require_proof_bindings: false,
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
            require_proof_bindings: false,
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

#[test]
fn pair_inspector_candidate_first_model_drift_poisons_without_pair_receipt_or_later_upstream() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let upstream_count = Arc::new(AtomicUsize::new(0));
    let worker_count = upstream_count.clone();
    let worker = std::thread::spawn(move || {
        while let Some(request) = upstream.recv_timeout(Duration::from_secs(2)).unwrap() {
            worker_count.fetch_add(1, Ordering::SeqCst);
            request
                .respond(
                    tiny_http::Response::from_string(concat!(
                        "data: {\"type\":\"response.completed\",\"response\":{",
                        "\"id\":\"mock-response\",\"model\":\"mock-revision\",",
                        "\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n"
                    ))
                    .with_header(
                        tiny_http::Header::from_bytes("content-type", "text/event-stream")
                            .unwrap(),
                    ),
                )
                .unwrap();
        }
    });

    let temp = tempfile::tempdir().unwrap();
    let homes = crate::IsolatedHomes {
        generic_home: temp.path().join("generic-home"),
        generic_codex_home: temp.path().join("generic-home/.codex"),
        candidate_home: temp.path().join("candidate-home"),
        candidate_codex_home: temp.path().join("candidate-home/.codex"),
    };
    let runtime = Arc::new(BrokerRuntimeConfig::new(1, 77, 16 * 1024).unwrap());
    let gate = Arc::new(
        PairCoordinator::create(BrokerGateConfig {
            ledger_path: temp.path().join("attempt-index.jsonl"),
            receipt_dir: temp.path().join("receipts"),
            pair_id: "pair-parity".to_string(),
            frozen_run_context_sha256: "a".repeat(64),
            execution_context_sha256: "b".repeat(64),
            arm_order_commitment: "c".repeat(64),
            require_proof_bindings: false,
            runtime: runtime.clone(),
        })
        .unwrap(),
    );
    gate.commit_order_for_test(EvaluationCondition::Candidate, EvaluationCondition::Generic)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let deadline_rfc3339 = (chrono::Utc::now() + chrono::Duration::seconds(10)).to_rfc3339();
    gate.activate_arm(ArmActivation {
        run_ordinal: 1,
        condition: EvaluationCondition::Candidate,
        root_thread_id: test_thread_id("root-1").to_string(),
        deadline,
        deadline_rfc3339: deadline_rfc3339.clone(),
    })
    .unwrap();
    let inspector = Arc::new(crate::runner::PairRequestInspector::new(
        gate.clone(),
        homes.clone(),
        deadline_rfc3339.clone(),
    ));
    let config = codex_responses_api_proxy::ProxyConfig {
        listen_port: None,
        upstream_url: reqwest::Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap(),
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: None,
        request_transform: Some(runtime.request_transform(inspector)),
    };
    let bound = codex_responses_api_proxy::bind(&config).unwrap();
    let proxy = codex_responses_api_proxy::activate(
        bound,
        config,
        codex_responses_api_proxy::local_mock_auth_header(),
        gate.clone(),
        gate.clone(),
    )
    .unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let url = format!("http://{}/v1/responses", proxy.addr());
    let base_body = |model: &str,
                     root: &str,
                     home: &std::path::Path,
                     codex_home: &std::path::Path| {
        json!({
            "model": model,
            "instructions": "Return the frozen synthetic package.",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "frozen mission"}]}],
            "tools": [{"type": "function", "name": "read_file", "description": "Read one file", "parameters": {"type": "object"}}],
            "metadata": {
                "aiIpThreadId": root,
                "aiIpHome": home,
                "aiIpCodexHome": codex_home
            }
        })
    };
    let mut candidate_body = base_body(
        "unauthorized-model-drift",
        test_thread_id("root-1"),
        &homes.candidate_home,
        &homes.candidate_codex_home,
    );
    candidate_body["input"].as_array_mut().unwrap().push(
        crate::runner::canonical_target_skill_treatment(&homes.candidate_codex_home),
    );
    let first = client
        .post(&url)
        .header(
            "x-codex-window-id",
            format!("{}:0", test_thread_id("root-1")),
        )
        .json(&candidate_body)
        .send()
        .unwrap();
    let first_status = first.status();
    let first_text = first.text().unwrap();
    assert!(
        first_status.is_success(),
        "{first_status}: {first_text}; phase={:?}",
        gate.phase()
    );
    gate.seal_arm().unwrap();
    gate.activate_arm(ArmActivation {
        run_ordinal: 2,
        condition: EvaluationCondition::Generic,
        root_thread_id: test_thread_id("root-2").to_string(),
        deadline,
        deadline_rfc3339,
    })
    .unwrap();
    let generic_body = base_body(
        "local-mock",
        test_thread_id("root-2"),
        &homes.generic_home,
        &homes.generic_codex_home,
    );
    let second = client
        .post(&url)
        .header(
            "x-codex-window-id",
            format!("{}:0", test_thread_id("root-2")),
        )
        .json(&generic_body)
        .send()
        .unwrap();
    let second_was_rejected = !second.status().is_success();
    let _ = second.text().unwrap();
    let later = client
        .post(&url)
        .header(
            "x-codex-window-id",
            format!("{}:1", test_thread_id("root-2")),
        )
        .json(&generic_body)
        .send()
        .unwrap();
    let later_was_rejected = !later.status().is_success();
    let _ = later.text().unwrap();
    proxy.shutdown_with_timeout(Duration::from_secs(2)).unwrap();
    worker.join().unwrap();

    assert!(second_was_rejected);
    assert!(later_was_rejected);
    assert_eq!(upstream_count.load(Ordering::SeqCst), 1);
    assert!(matches!(gate.phase(), PairPhase::Poisoned { .. }));
    assert!(!temp.path().join("receipts/pair-receipt.json").exists());
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
            require_proof_bindings: false,
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

struct NativeMockPairTestRun {
    _temp: tempfile::TempDir,
    live_root: std::path::PathBuf,
    result: anyhow::Result<()>,
}

fn run_native_mock_pair_with_marker(
    marker: Option<&str>,
    max_total_tokens_per_run: u64,
) -> NativeMockPairTestRun {
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    let upstream = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let upstream_addr = upstream.server_addr().to_ip().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = std::thread::spawn(move || {
        let mut index = 0_u64;
        while !worker_stop.load(Ordering::SeqCst) {
            let Some(request) = upstream.recv_timeout(Duration::from_millis(100)).unwrap() else {
                continue;
            };
            let body = format!(
                "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"mock-response-{index}\",\"model\":\"mock-revision\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}}}}\n\n"
            );
            request
                .respond(tiny_http::Response::from_string(body).with_header(
                    tiny_http::Header::from_bytes("content-type", "text/event-stream").unwrap(),
                ))
                .unwrap();
            index += 1;
        }
    });
    let temp = tempfile::tempdir().unwrap();
    let seed_context = strict_live_context(&temp);
    let seed: serde_json::Value = serde_json::from_slice(&fs::read(seed_context).unwrap()).unwrap();
    let artifacts = seed["artifacts"].as_object().unwrap();
    let artifact = |name: &str| std::path::PathBuf::from(artifacts[name]["path"].as_str().unwrap());
    let mock_codex = crate::native_app_server_fixture::copy_into(temp.path()).unwrap();
    let live_root = temp.path().join("native-private");
    fs::create_dir(&live_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&live_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let live_root = live_root.canonicalize().unwrap();
    let (live_case, live_material_root, live_attestation) =
        write_nonempty_source_fixture(temp.path(), &live_root, "notes/evidence.txt", None, None);
    let output = live_root.join("frozen-run-context.json");
    let mut live_args = crate::model::LiveFreezeArgs {
        repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        evidence_repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        fork_sha: seed["repoHead"].as_str().unwrap().to_string(),
        private_root: live_root.clone(),
        codex_bin: mock_codex,
        case: live_case,
        material_root: live_material_root,
        attestation: live_attestation,
        provider_budget_evidence: artifact("providerBudgetEvidence"),
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
        max_total_tokens_per_run,
        max_elapsed_seconds_per_run: 180,
        max_output_tokens_per_request: 17,
        output: output.clone(),
    };
    install_retention_managed_cost_inputs(&mut live_args);
    crate::execute_cli(Cli {
        command: crate::EvalCommand::FreezeRunContext(crate::model::FreezeRunContextCommand {
            mode: crate::FreezeRunContextArgs::Live(live_args),
        }),
    })
    .unwrap();
    if let Some(marker) = marker {
        let marker = live_root.join(marker);
        fs::write(&marker, b"test behavior marker\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    let result = crate::execute_cli(Cli {
        command: crate::EvalCommand::LivePair(crate::model::LivePairArgs {
            frozen_run_context: output,
        }),
    });
    stop.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    NativeMockPairTestRun {
        _temp: temp,
        live_root,
        result,
    }
}

fn assert_native_initial_inventory_rejects(relative: &str) {
    let run = run_native_mock_pair_with_marker(Some(relative), 10);
    assert!(
        run.result.is_err(),
        "native pair learned pre-bootstrap entry {relative}"
    );
    assert!(
        !run.live_root
            .join("coordinator/private-inventory.jsonl")
            .exists(),
        "native pair published an initial inventory for {relative}"
    );
    assert!(
        !run.live_root.join("coordinator/pair-marker.json").exists(),
        "native pair published an initial marker for {relative}"
    );
}

#[test]
fn native_initial_inventory_rejects_prebootstrap_supplier_entry() {
    assert_native_initial_inventory_rejects("inputs/supplier-statements/generic.json");
}

#[test]
fn native_initial_inventory_rejects_prebootstrap_cost_entry() {
    assert_native_initial_inventory_rejects("coordinator/cost/unexpected.json");
}

#[test]
fn native_mock_pair_reaches_verified_content_projection() {
    let run = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    run.result.as_ref().unwrap();
    let frozen_path = run.live_root.join("frozen-run-context.json");
    let frozen_json: serde_json::Value =
        serde_json::from_slice(&fs::read(&frozen_path).unwrap()).unwrap();
    let artifact_path = |name: &str| {
        std::path::PathBuf::from(frozen_json["artifacts"][name]["path"].as_str().unwrap())
    };
    let artifact_bytes = |name: &str| fs::read(artifact_path(name)).unwrap();
    let case_bytes = artifact_bytes("source");
    let mission_case: HeldOutMissionCase = serde_json::from_slice(&case_bytes).unwrap();
    let attestation_bytes = artifact_bytes("attestation");
    let attestation: crate::NativeHeldOutAttestation =
        serde_json::from_slice(&attestation_bytes).unwrap();
    let reviewers: [crate::ReviewerDeclaration; 3] =
        attestation.reviewers.clone().try_into().unwrap();
    let materials_manifest_bytes = artifact_bytes("materials");
    let materials_manifest: Vec<codex_ai_ip_domain::MissionMaterial> =
        serde_json::from_slice(&materials_manifest_bytes).unwrap();
    let material_bytes = artifact_bytes("material:evidence-1");
    let expected = crate::runner::NativeContentInputs {
        mission_case,
        case_bytes,
        attestation,
        reviewers,
        attestation_bytes,
        materials_manifest,
        materials_manifest_bytes,
        materials: vec![crate::runner::NativeMaterialInput {
            material_id: "evidence-1".to_string(),
            relative_path: "notes/evidence.txt".to_string(),
            sha256: test_sha256(&material_bytes),
            bytes: material_bytes.clone(),
        }],
        prompt_bytes: artifact_bytes("prompt"),
        additional_context_bytes: artifact_bytes("additionalContext"),
        schema_bytes: artifact_bytes("schema"),
        thread_start_bytes: artifact_bytes("threadStartRequest"),
        turn_start_bytes: artifact_bytes("turnStartRequest"),
        skill_bytes: artifact_bytes("skill"),
        codex_binary_sha256: test_sha256(&artifact_bytes("codexBinary")),
        evaluator_binary_sha256: test_sha256(&artifact_bytes("evaluatorBinary")),
        broker_component_sha256: test_sha256(&artifact_bytes("brokerSource")),
        model_label: "local-mock".to_string(),
        provider_mode: "not-run".to_string(),
        provider_label: "synthetic-loopback-mock".to_string(),
        provider_compatibility_name: ProofBrokerCompatibilityName::OpenAi,
        max_output_tokens: 17,
        max_provider_request_attempts: 2,
        max_total_tokens_per_run: 10,
        max_elapsed_seconds_per_run: 180,
    };
    let frozen = verify_frozen_context(&frozen_path).unwrap();
    let verified = frozen.native_content_inputs().unwrap();
    assert_eq!(verified.projection(), &expected);
    verified.reverify_unchanged().unwrap();

    let material_path = artifact_path("material:evidence-1");
    fs::write(&material_path, b"changed native material\n").unwrap();
    let error = verified.reverify_unchanged().unwrap_err();
    assert!(format!("{error:#}").contains("artifact bytes changed after freeze"));
    fs::write(&material_path, &material_bytes).unwrap();
    verified.reverify_unchanged().unwrap();
    replace_with_same_owner_only_bytes(&material_path);
    let error = verified.reverify_unchanged().unwrap_err();
    assert!(format!("{error:#}").contains("artifact path identity changed after freeze"));
}

#[test]
fn native_runtime_allows_both_arms_to_reach_the_per_run_token_ceiling() {
    let run =
        run_native_mock_pair_with_marker(/*marker*/ None, /*max_total_tokens_per_run*/ 4);
    assert!(run.result.is_ok(), "pair failed: {:?}", run.result.err());

    let manifests = read_native_mock_manifests(&run.live_root);
    assert_eq!(
        manifests
            .into_iter()
            .map(|manifest| (manifest.usage, manifest.max_total_tokens))
            .collect::<Vec<_>>(),
        vec![
            (
                Usage {
                    total_tokens: 4,
                    input_tokens: 2,
                    cached_input_tokens: 0,
                    cache_write_input_tokens: 0,
                    output_tokens: 2,
                    reasoning_output_tokens: 0,
                },
                4,
            ),
            (
                Usage {
                    total_tokens: 4,
                    input_tokens: 2,
                    cached_input_tokens: 0,
                    cache_write_input_tokens: 0,
                    output_tokens: 2,
                    reasoning_output_tokens: 0,
                },
                4,
            ),
        ]
    );
}

#[test]
fn native_pair_token_cap_overflow_fails_before_coordinator_evidence() {
    let run = run_native_mock_pair_with_marker(
        /*marker*/ None,
        /*max_total_tokens_per_run*/ u64::MAX,
    );
    let error = run.result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("native pair token ceiling is not representable"),
        "unexpected error: {error:#}"
    );
    assert!(!run.live_root.join("coordinator").exists());
}

#[test]
fn generic_target_skill_read_poisons_without_pair_receipt() {
    let run = run_native_mock_pair_with_marker(
        Some("generic-target-skill-read"),
        /*max_total_tokens_per_run*/ 10,
    );
    assert!(
        run.result
            .unwrap_err()
            .to_string()
            .contains("generic arm read")
    );
    assert!(
        run.live_root
            .join("coordinator/receipts/poison.json")
            .is_file()
    );
    assert!(
        !run.live_root
            .join("coordinator/receipts/pair-receipt.json")
            .exists()
    );
}

#[test]
fn generic_target_skill_read_during_quiet_window_poisons_without_pair_receipt() {
    let run = run_native_mock_pair_with_marker(
        Some("generic-target-skill-read-quiet-window"),
        /*max_total_tokens_per_run*/ 10,
    );
    assert!(
        run.result
            .unwrap_err()
            .to_string()
            .contains("generic arm read")
    );
    assert!(
        run.live_root
            .join("coordinator/receipts/poison.json")
            .is_file()
    );
    assert!(
        !run.live_root
            .join("coordinator/receipts/pair-receipt.json")
            .exists()
    );
}

fn read_native_mock_manifests(live_root: &std::path::Path) -> Vec<crate::RunManifest> {
    [1_u8, 2]
        .into_iter()
        .map(|ordinal| {
            serde_json::from_slice(
                &fs::read(live_root.join(format!("coordinator/run-{ordinal}-manifest.json")))
                    .unwrap(),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn local_mock_manifests_are_typed_mock_and_never_g2_eligible() {
    let run = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    assert!(run.result.is_ok(), "pair failed: {:?}", run.result.err());
    let manifests = read_native_mock_manifests(&run.live_root);

    assert!(
        manifests
            .iter()
            .all(|manifest| manifest.execution_mode == ExecutionMode::Mock)
    );
    assert!(
        manifests
            .iter()
            .all(|manifest| matches!(manifest.mode_evidence, ModeEvidence::Mock { .. }))
    );
    assert!(manifests.iter().all(|manifest| !manifest.is_g2_eligible()));
}

#[test]
fn mock_manifest_binds_actual_additional_context_not_turn_request() {
    let run = run_native_mock_pair_with_marker(
        /*marker*/ None, /*max_total_tokens_per_run*/ 10,
    );
    assert!(run.result.is_ok(), "pair failed: {:?}", run.result.err());
    let manifest = read_native_mock_manifests(&run.live_root).remove(0);
    let mission: HeldOutMissionCase =
        serde_json::from_slice(&fs::read(run.live_root.join("inputs/case/case.json")).unwrap())
            .unwrap();
    let expected = format!(
        "{:x}",
        Sha256::digest(
            codex_ai_ip_runtime::evaluation_context(&mission)
                .unwrap()
                .as_bytes()
        )
    );

    assert_eq!(manifest.additional_context_sha256, expected);
    assert_ne!(
        manifest.additional_context_sha256,
        manifest.turn_start_request_sha256
    );
}

fn wait_for_native_fixture_child(
    child: &mut std::process::Child,
    description: &str,
) -> anyhow::Result<std::process::ExitStatus> {
    use anyhow::Context as _;

    let timeout = Duration::from_secs(5);
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("check {description} fixture status"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            child
                .kill()
                .with_context(|| format!("kill timed-out {description} fixture"))?;
            let status = child
                .wait()
                .with_context(|| format!("reap timed-out {description} fixture"))?;
            anyhow::bail!(
                "{description} fixture timed out after {} seconds and was killed with {status}",
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn read_native_fixture_stderr(child: &mut std::process::Child) -> anyhow::Result<String> {
    use anyhow::Context as _;
    use std::io::Read;

    let mut stderr = child
        .stderr
        .take()
        .context("fixture stderr was not captured")?;
    let mut bytes = Vec::new();
    stderr
        .read_to_end(&mut bytes)
        .context("read fixture stderr")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_native_fixture_stdout(child: &mut std::process::Child) -> anyhow::Result<String> {
    use anyhow::Context as _;
    use std::io::Read;

    let mut stdout = child
        .stdout
        .take()
        .context("fixture stdout was not captured")?;
    let mut output = String::new();
    stdout
        .read_to_string(&mut output)
        .context("read fixture stdout")?;
    Ok(output)
}

fn ensure_native_fixture_child_terminal(
    child: &mut std::process::Child,
) -> anyhow::Result<std::process::ExitStatus> {
    let inspection = child.try_wait();
    if let Ok(Some(status)) = inspection {
        return Ok(status);
    }
    let kill = child.kill();
    match child.wait() {
        Ok(status) => Ok(status),
        Err(wait_error) => anyhow::bail!(
            "fixture could not be made terminal; inspection: {}; kill: {}; wait: {wait_error}",
            match inspection {
                Ok(None) => "still running".to_string(),
                Ok(Some(status)) => format!("already terminal with {status}"),
                Err(error) => format!("failed: {error}"),
            },
            match kill {
                Ok(()) => "ok".to_string(),
                Err(error) => format!("failed: {error}"),
            },
        ),
    }
}

fn terminate_native_fixture_child(child: &mut std::process::Child) -> String {
    let status = match child.try_wait() {
        Ok(Some(status)) => format!("already exited with {status}"),
        Ok(None) => match child.kill().and_then(|()| child.wait()) {
            Ok(status) => format!("killed and reaped with {status}"),
            Err(error) => format!("cleanup failed: {error}"),
        },
        Err(error) => match child.kill().and_then(|()| child.wait()) {
            Ok(status) => {
                format!(
                    "could not inspect child before cleanup: {error}; killed and reaped with {status}"
                )
            }
            Err(cleanup_error) => {
                format!(
                    "could not inspect child before cleanup: {error}; cleanup failed: {cleanup_error}"
                )
            }
        },
    };
    let stderr =
        read_native_fixture_stderr(child).unwrap_or_else(|error| format!("unavailable: {error:#}"));
    format!("{status}; stderr: {stderr}")
}

struct NativeFixtureBrokerRun {
    home: std::path::PathBuf,
    codex_home: std::path::PathBuf,
    port: u16,
    requests: Vec<Vec<u8>>,
    status: std::process::ExitStatus,
    stdout: String,
    stderr: String,
}

enum NativeFixtureBrokerFault {
    None,
    AfterSpawn {
        cleanup_events: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    },
}

fn record_native_fixture_cleanup_event(
    cleanup_events: &Option<std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>>,
    event: &'static str,
) {
    if let Some(cleanup_events) = cleanup_events
        && let Ok(mut cleanup_events) = cleanup_events.lock()
    {
        cleanup_events.push(event);
    }
}

fn read_native_fixture_broker_request(stream: &mut std::net::TcpStream) -> anyhow::Result<Vec<u8>> {
    use anyhow::Context as _;
    use std::io::Read;

    const MAX_REQUEST_BYTES: usize = 64 * 1024;
    fn read_request_bytes(
        stream: &mut std::net::TcpStream,
        scratch: &mut [u8],
        deadline: Instant,
    ) -> anyhow::Result<usize> {
        loop {
            match stream.read(scratch) {
                Ok(read) => return Ok(read),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    anyhow::ensure!(
                        Instant::now() < deadline,
                        "fixture broker request read exceeded five-second test bound"
                    );
                }
                Err(error) => return Err(error).context("read fixture broker request"),
            }
        }
    }

    let mut request = Vec::new();
    let mut scratch = [0_u8; 4096];
    let deadline = Instant::now() + Duration::from_secs(5);
    let (header_end, content_length) = loop {
        let read = read_request_bytes(stream, &mut scratch, deadline)?;
        anyhow::ensure!(read != 0, "fixture broker request ended before headers");
        request.extend_from_slice(&scratch[..read]);
        anyhow::ensure!(
            request.len() <= MAX_REQUEST_BYTES,
            "fixture broker request exceeded test bound"
        );
        let Some(header_end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") else {
            continue;
        };
        let header_end = header_end + 4;
        let headers = std::str::from_utf8(&request[..header_end])?;
        let lengths = headers
            .split("\r\n")
            .filter_map(|line| line.split_once(':'))
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim().parse::<usize>())
            .collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(lengths.len() == 1, "expected one request Content-Length");
        break (header_end, lengths[0]);
    };
    let total_length = header_end
        .checked_add(content_length)
        .context("fixture broker request length overflow")?;
    anyhow::ensure!(
        total_length <= MAX_REQUEST_BYTES,
        "fixture broker request exceeded test bound"
    );
    while request.len() < total_length {
        let read = read_request_bytes(stream, &mut scratch, deadline)
            .context("read fixture broker request body")?;
        anyhow::ensure!(read != 0, "fixture broker request body ended early");
        request.extend_from_slice(&scratch[..read]);
    }
    anyhow::ensure!(
        request.len() == total_length,
        "fixture broker request had trailing bytes"
    );
    Ok(request)
}

fn run_native_fixture_broker_contract(
    responses: Vec<Vec<u8>>,
) -> anyhow::Result<NativeFixtureBrokerRun> {
    run_native_fixture_broker_contract_with_fault(responses, NativeFixtureBrokerFault::None)
}

fn run_native_fixture_broker_contract_with_fault(
    responses: Vec<Vec<u8>>,
    fault: NativeFixtureBrokerFault,
) -> anyhow::Result<NativeFixtureBrokerRun> {
    use anyhow::Context as _;
    use std::io::Write;
    use std::net::TcpListener;
    use std::process::Command;
    use std::process::Stdio;

    let fixture = crate::native_app_server_fixture::locate()
        .context("native App Server fixture target must be available")?;
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let temp = tempfile::tempdir()?;
    let home = temp.path().join("home");
    let codex_home = home.join(".codex");
    create_owner_only_test_dir(&home);
    create_owner_only_test_dir(&codex_home);
    fs::write(
        codex_home.join("config.toml"),
        format!(
            concat!(
                "model = \"fixture-test\"\n",
                "model_provider = \"ai-ip-proof-broker\"\n",
                "[model_providers.ai-ip-proof-broker]\n",
                "name = \"OpenAI\"\n",
                "base_url = \"http://127.0.0.1:{}/v1\"\n",
                "wire_api = \"responses\"\n",
                "requires_openai_auth = false\n"
            ),
            port,
        ),
    )?;
    let worker = std::thread::Builder::new()
        .name("native-fixture-broker".to_string())
        .spawn(move || -> anyhow::Result<Vec<Vec<u8>>> {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut requests = Vec::with_capacity(responses.len());
            for response in responses {
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            anyhow::ensure!(
                                Instant::now() < deadline,
                                "fixture did not connect to broker within five seconds"
                            );
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                requests.push(read_native_fixture_broker_request(&mut stream)?);
                stream.write_all(&response)?;
                stream.flush()?;
            }
            Ok(requests)
        })
        .context("spawn native fixture broker worker")?;
    let mut child = match Command::new(fixture)
        .args(["app-server", "--listen", "stdio://", "--strict-config"])
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let broker = worker
                .join()
                .map_err(|_| anyhow::anyhow!("fixture broker worker panicked"));
            return Err(error).with_context(|| match broker {
                Ok(Ok(_)) => "broker worker finished after fixture spawn failed".to_string(),
                Ok(Err(error)) | Err(error) => {
                    format!("broker worker cleanup after fixture spawn failed: {error:#}")
                }
            });
        }
    };
    let cleanup_events = match &fault {
        NativeFixtureBrokerFault::None => None,
        NativeFixtureBrokerFault::AfterSpawn { cleanup_events } => Some(cleanup_events.clone()),
    };
    let operation = (|| -> anyhow::Result<std::process::ExitStatus> {
        if matches!(fault, NativeFixtureBrokerFault::AfterSpawn { .. }) {
            anyhow::bail!("controlled broker-contract failure after fixture spawn");
        }
        let requests = [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": crate::initialize_params()}),
            json!({"jsonrpc": "2.0", "method": "initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "thread/start", "params": {"cwd": temp.path()}}),
            json!({"jsonrpc": "2.0", "id": 3, "method": "turn/start", "params": {}}),
        ];
        let stdin = child
            .stdin
            .as_mut()
            .context("fixture stdin was not captured")?;
        for request in requests {
            writeln!(stdin, "{}", serde_json::to_string(&request)?)?;
        }
        stdin.flush()?;
        drop(child.stdin.take());
        wait_for_native_fixture_child(&mut child, "broker-contract")
    })();

    let child_terminal = ensure_native_fixture_child_terminal(&mut child);
    if child_terminal.is_ok() {
        record_native_fixture_cleanup_event(&cleanup_events, "child-terminal");
    }
    let stdout = if child_terminal.is_ok() {
        read_native_fixture_stdout(&mut child)
    } else {
        Err(anyhow::anyhow!(
            "fixture stdout not read because child terminal state was not established"
        ))
    };
    record_native_fixture_cleanup_event(&cleanup_events, "stdout-read");
    let stderr = if child_terminal.is_ok() {
        read_native_fixture_stderr(&mut child)
    } else {
        Err(anyhow::anyhow!(
            "fixture stderr not read because child terminal state was not established"
        ))
    };
    record_native_fixture_cleanup_event(&cleanup_events, "stderr-read");
    let broker = worker
        .join()
        .map_err(|_| anyhow::anyhow!("fixture broker worker panicked"));
    record_native_fixture_cleanup_event(&cleanup_events, "broker-joined");

    let cleanup_context = format!(
        "child terminal: {}; stdout read: {}; stderr read: {}; broker worker: {}",
        match &child_terminal {
            Ok(status) => format!("{status}"),
            Err(error) => format!("failed: {error:#}"),
        },
        match &stdout {
            Ok(_) => "ok".to_string(),
            Err(error) => format!("failed: {error:#}"),
        },
        match &stderr {
            Ok(_) => "ok".to_string(),
            Err(error) => format!("failed: {error:#}"),
        },
        match &broker {
            Ok(Ok(_)) => "ok".to_string(),
            Ok(Err(error)) => format!("failed: {error:#}"),
            Err(error) => format!("failed: {error:#}"),
        },
    );
    let status = operation.with_context(|| cleanup_context.clone())?;
    child_terminal.with_context(|| cleanup_context.clone())?;
    let stdout = stdout.with_context(|| cleanup_context.clone())?;
    let stderr = stderr.with_context(|| cleanup_context.clone())?;
    let requests = broker
        .with_context(|| cleanup_context.clone())?
        .with_context(|| cleanup_context.clone())?;
    Ok(NativeFixtureBrokerRun {
        home,
        codex_home,
        port,
        requests,
        status,
        stdout,
        stderr,
    })
}

#[test]
fn native_app_server_fixture_reaps_child_and_joins_broker_after_post_spawn_failure() {
    let cleanup_events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let error = match run_native_fixture_broker_contract_with_fault(
        vec![Vec::new()],
        NativeFixtureBrokerFault::AfterSpawn {
            cleanup_events: cleanup_events.clone(),
        },
    ) {
        Ok(_) => panic!("controlled post-spawn failure unexpectedly succeeded"),
        Err(error) => error,
    };

    assert!(
        format!("{error:#}").contains("controlled broker-contract failure after fixture spawn")
    );
    assert_eq!(
        *cleanup_events.lock().unwrap(),
        [
            "child-terminal",
            "stdout-read",
            "stderr-read",
            "broker-joined"
        ]
    );
}

fn completed_sse(response_id: &str) -> String {
    format!(
        "data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"{response_id}\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}}}}\n\n"
    )
}

fn split_fixture_broker_request(request: &[u8]) -> (&str, &[u8]) {
    let header_end = request
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap();
    (
        std::str::from_utf8(&request[..header_end]).unwrap(),
        &request[header_end + 4..],
    )
}

#[test]
fn native_app_server_fixture_emits_exact_broker_http_contract() {
    let response_bodies = [
        completed_sse("root-response"),
        completed_sse("child-response"),
    ];
    let responses = response_bodies
        .iter()
        .map(|body| {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .into_bytes()
        })
        .collect();
    let run = run_native_fixture_broker_contract(responses).unwrap();
    assert!(
        run.status.success(),
        "fixture failed exact broker contract: {}",
        run.stderr
    );
    assert!(run.stdout.contains("root-response"));
    assert!(run.stdout.contains("child-response"));
    assert_eq!(run.requests.len(), 2);

    let root_thread_id = "0198f5aa-0000-7000-8000-000000000101";
    let child_thread_id = "0198f5aa-0000-7000-8000-000000000201";
    let expected_bodies = [
        json!({
            "model": "local-mock",
            "instructions": "Return the frozen synthetic package.",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "frozen mission"}]}],
            "tools": [{"type": "function", "name": "read_file", "description": "Read one file", "parameters": {"type": "object"}}],
            "metadata": {"aiIpThreadId": root_thread_id, "aiIpHome": run.home, "aiIpCodexHome": run.codex_home}
        }),
        json!({
            "model": "local-mock",
            "instructions": "Return the frozen synthetic package.",
            "input": [{"role": "user", "content": [{"type": "input_text", "text": "frozen mission"}]}],
            "tools": [{"type": "function", "name": "read_file", "description": "Read one file", "parameters": {"type": "object"}}],
            "metadata": {"aiIpThreadId": child_thread_id, "aiIpHome": run.home, "aiIpCodexHome": run.codex_home}
        }),
    ];
    for (index, request) in run.requests.iter().enumerate() {
        let (headers, body) = split_fixture_broker_request(request);
        let expected_body = serde_json::to_vec(&expected_bodies[index]).unwrap();
        let mut lines = headers.lines();
        assert_eq!(lines.next(), Some("POST /v1/responses HTTP/1.1"));
        let actual_headers = lines
            .map(|line| {
                let (name, value) = line.split_once(':').unwrap();
                (name.to_ascii_lowercase(), value.trim().to_string())
            })
            .fold(
                BTreeMap::<_, Vec<_>>::new(),
                |mut headers, (name, value)| {
                    headers.entry(name).or_default().push(value);
                    headers
                },
            );
        let mut expected_headers = BTreeMap::from([
            ("host".to_string(), vec![format!("127.0.0.1:{}", run.port)]),
            (
                "content-type".to_string(),
                vec!["application/json".to_string()],
            ),
            (
                "content-length".to_string(),
                vec![expected_body.len().to_string()],
            ),
            ("connection".to_string(), vec!["close".to_string()]),
            (
                "x-codex-window-id".to_string(),
                vec![format!(
                    "{}:0",
                    if index == 0 {
                        root_thread_id
                    } else {
                        child_thread_id
                    }
                )],
            ),
        ]);
        if index == 1 {
            expected_headers.insert(
                "x-codex-parent-thread-id".to_string(),
                vec![root_thread_id.to_string()],
            );
            expected_headers.insert(
                "x-openai-subagent".to_string(),
                vec!["collab_spawn".to_string()],
            );
        }
        assert_eq!(actual_headers, expected_headers);
        assert_eq!(body, expected_body);
    }
}

#[test]
fn native_app_server_fixture_accepts_chunked_sse_extensions_and_trailers() {
    let responses = ["chunked-root", "chunked-child"]
        .map(|response_id| {
            let body = completed_sse(response_id);
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x};fixture=yes\r\n{body}\r\n0\r\nx-fixture-trailer: done\r\n\r\n",
                body.len()
            )
            .into_bytes()
        })
        .to_vec();
    let run = run_native_fixture_broker_contract(responses).unwrap();
    assert!(
        run.status.success(),
        "fixture rejected valid chunked broker response: {}",
        run.stderr
    );
    assert!(run.stdout.contains("chunked-root"));
    assert!(run.stdout.contains("chunked-child"));
}

#[test]
fn native_app_server_fixture_accepts_chunked_bws_and_quoted_extensions() {
    let responses = [
        ("quoted-root", " \t; fixture = \"quoted;semicolon\""),
        (
            "quoted-child",
            "\t; first=token; second=\"escaped\\\"quote;still-quoted\"",
        ),
    ]
    .map(|(response_id, extensions)| {
        let body = completed_sse(response_id);
        format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}{extensions}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        )
        .into_bytes()
    })
    .to_vec();
    let run = run_native_fixture_broker_contract(responses).unwrap();
    assert!(
        run.status.success(),
        "fixture rejected valid chunk extensions with BWS/quotes: {}",
        run.stderr
    );
    assert!(run.stdout.contains("quoted-root"));
    assert!(run.stdout.contains("quoted-child"));
}

#[test]
fn native_app_server_fixture_rejects_malformed_or_ambiguous_broker_framing() {
    let completed = completed_sse("must-not-complete");
    let malformed = [
        (
            "invalid status line",
            b"HTTP/1.1 nope\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
        ),
        (
            "conflicting Content-Length values",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{completed}",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "Content-Length with Transfer-Encoding",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{completed}\r\n0\r\n\r\n",
                completed.len(),
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "obfuscated Transfer-Encoding with Content-Length",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nTransfer-Encoding : chunked\r\nConnection: close\r\n\r\n{completed}",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "obfuscated Content-Length with Transfer-Encoding",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length : {}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{completed}\r\n0\r\n\r\n",
                completed.len(),
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "non-digit Content-Length token",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: +{}\r\nConnection: close\r\n\r\n{completed}",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "malformed chunk extension",
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x};=bad\r\n{completed}\r\n0\r\n\r\n",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "malformed trailer name",
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{completed}\r\n0\r\nbad trailer: done\r\n\r\n",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "response header value control",
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nX-Bad: bad\x01value\r\nConnection: close\r\n\r\n{completed}",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "leading OWS before a data chunk size",
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n {:x};foo=bar\r\n{completed}\r\n0\r\n\r\n",
                completed.len()
            )
            .into_bytes(),
        ),
        (
            "trailing OWS on a last chunk without extensions",
            format!(
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{completed}\r\n0 \r\n\r\n",
                completed.len()
            )
            .into_bytes(),
        ),
    ];
    let mut accepted = Vec::new();
    for (description, response) in malformed {
        let run = run_native_fixture_broker_contract(vec![response]).unwrap();
        if run.status.success() || run.stdout.contains("rawResponse/completed") {
            accepted.push(description);
        }
    }
    assert!(accepted.is_empty(), "fixture accepted {accepted:?}");
}

#[test]
fn native_app_server_fixture_stays_within_test_io_budget() {
    let fixture = crate::native_app_server_fixture::locate()
        .expect("native App Server fixture target must be available to this test");
    let actual_bytes = fixture.metadata().unwrap().len();
    let maximum_bytes = if cfg!(target_os = "macos") {
        12 * 1024 * 1024
    } else {
        64 * 1024 * 1024
    };

    assert!(
        actual_bytes <= maximum_bytes,
        "native App Server fixture actual size is {actual_bytes} bytes; maximum test-infrastructure I/O budget is {maximum_bytes} bytes"
    );
}

#[test]
fn native_app_server_fixture_uses_production_argv_and_stdout() {
    use anyhow::Context as _;
    use std::io::BufRead;
    use std::io::BufReader;
    use std::io::Write;
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::mpsc;

    let fixture = crate::native_app_server_fixture::locate()
        .expect("native App Server fixture target must be available to this test");
    let evaluator = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert_ne!(fixture, evaluator);

    let mut wrong_argv = Command::new(&fixture)
        .arg("not-app-server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let wrong_status =
        wait_for_native_fixture_child(&mut wrong_argv, "wrong-argv").unwrap_or_else(|error| {
            panic!(
                "wrong-argv fixture failed: {error:#}; {}",
                terminate_native_fixture_child(&mut wrong_argv)
            )
        });
    let wrong_stderr = read_native_fixture_stderr(&mut wrong_argv).unwrap();
    assert!(
        !wrong_status.success(),
        "native App Server fixture accepted wrong argv; stderr: {wrong_stderr}"
    );

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let codex_home = home.join(".codex");
    create_owner_only_test_dir(&home);
    create_owner_only_test_dir(&codex_home);
    fs::write(codex_home.join("config.toml"), "model = \"fixture-test\"\n").unwrap();

    let mut child = Command::new(&fixture)
        .args(["app-server", "--listen", "stdio://", "--strict-config"])
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (response_sender, response_receiver) = mpsc::sync_channel(1);
    let stdout_reader = std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        let mut line = String::new();
        let response = stdout
            .read_line(&mut line)
            .context("read fixture stdout JSONL")
            .map(|bytes| (bytes, line));
        let _ = response_sender.send(response);
    });
    let production_result = (|| -> anyhow::Result<()> {
        let request_id = 7;
        let request = json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "initialize",
            "params": crate::initialize_params()
        });
        let stdin = child
            .stdin
            .as_mut()
            .context("fixture stdin was not captured")?;
        writeln!(stdin, "{}", serde_json::to_string(&request)?).context("write initialize")?;
        stdin.flush().context("flush initialize")?;

        let (bytes, line) = response_receiver
            .recv_timeout(Duration::from_secs(5))
            .context("fixture did not emit stdout JSONL within 5 seconds")??;
        anyhow::ensure!(bytes != 0, "fixture emitted no JSONL response");
        let response: serde_json::Value =
            serde_json::from_str(&line).context("parse fixture JSONL")?;
        anyhow::ensure!(
            json!({
                "id": response["id"],
                "result": {"userAgent": response["result"]["userAgent"]}
            }) == json!({"id": request_id, "result": {"userAgent": "mock-app-server"}}),
            "fixture initialize response did not include the expected id and user agent"
        );

        drop(child.stdin.take());
        let status = wait_for_native_fixture_child(&mut child, "production-argv")?;
        let stderr = read_native_fixture_stderr(&mut child)?;
        anyhow::ensure!(
            status.success(),
            "fixture failed after stdin closed: {status}; stderr: {stderr}"
        );
        let launch: serde_json::Value = serde_json::from_slice(
            &fs::read(home.join("app-server-launch.json"))
                .with_context(|| format!("read fixture launch record; stderr: {stderr}"))?,
        )
        .with_context(|| format!("parse fixture launch record; stderr: {stderr}"))?;
        anyhow::ensure!(
            launch["argv"] == json!(["app-server", "--listen", "stdio://", "--strict-config"]),
            "fixture recorded unexpected production argv; stderr: {stderr}"
        );
        Ok(())
    })();
    if let Err(error) = production_result {
        let diagnostic = terminate_native_fixture_child(&mut child);
        let _ = stdout_reader.join();
        panic!("production-argv fixture failed: {error:#}; {diagnostic}");
    }
    stdout_reader.join().unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn native_app_server_fixture_rejects_non_loopback_before_connect() {
    use anyhow::Context as _;
    use std::io::Write;
    use std::net::TcpListener;
    use std::process::Command;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;

    fn stop_and_join_listener(
        stop: &AtomicBool,
        worker: std::thread::JoinHandle<std::io::Result<()>>,
    ) -> anyhow::Result<()> {
        stop.store(true, Ordering::Release);
        worker
            .join()
            .map_err(|_| anyhow::anyhow!("controlled non-loopback listener thread panicked"))??;
        Ok(())
    }

    let fixture = crate::native_app_server_fixture::locate()
        .expect("native App Server fixture target must be available to this test");
    let listener = TcpListener::bind(("0.0.0.0", 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let listener_address = listener.local_addr().unwrap();
    let port = listener_address.port();
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let codex_home = home.join(".codex");
    create_owner_only_test_dir(&home);
    create_owner_only_test_dir(&codex_home);
    fs::write(
        codex_home.join("config.toml"),
        format!(
            concat!(
                "model = \"fixture-test\"\n",
                "model_provider = \"ai-ip-proof-broker\"\n",
                "[model_providers.ai-ip-proof-broker]\n",
                "name = \"OpenAI\"\n",
                "base_url = \"http://0.0.0.0:{}/v1\"\n",
                "wire_api = \"responses\"\n",
                "requires_openai_auth = false\n",
                "request_max_retries = 0\n",
                "stream_max_retries = 0\n",
                "supports_websockets = false\n"
            ),
            port,
        ),
    )
    .unwrap();

    let (accepted_sender, accepted_receiver) = mpsc::sync_channel(1);
    let listener_stop = Arc::new(AtomicBool::new(false));
    let worker_stop = Arc::clone(&listener_stop);
    let listener_worker = std::thread::spawn(move || {
        loop {
            match listener.accept() {
                Ok((stream, peer_address)) => {
                    accepted_sender.send(peer_address).map_err(|_| {
                        std::io::Error::new(
                            std::io::ErrorKind::BrokenPipe,
                            "non-loopback listener observer was dropped",
                        )
                    })?;
                    drop(stream);
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if worker_stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => return Err(error),
            }
        }
    });
    let mut child = match Command::new(&fixture)
        .args(["app-server", "--listen", "stdio://", "--strict-config"])
        .env("HOME", &home)
        .env("CODEX_HOME", &codex_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            panic!("launch non-loopback fixture: {error}; listener cleanup: {listener_cleanup:?}");
        }
    };

    let write_result = (|| -> anyhow::Result<()> {
        let requests = [
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": crate::initialize_params()
            }),
            json!({"jsonrpc": "2.0", "method": "initialized"}),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "thread/start",
                "params": {"cwd": temp.path()}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "turn/start",
                "params": {}
            }),
        ];
        let stdin = child
            .stdin
            .as_mut()
            .context("fixture stdin was not captured")?;
        for request in requests {
            writeln!(stdin, "{}", serde_json::to_string(&request)?)
                .context("write fixture JSONL request")?;
        }
        stdin.flush().context("flush fixture JSONL requests")?;
        drop(child.stdin.take());
        Ok(())
    })();
    if let Err(error) = write_result {
        let child_cleanup = terminate_native_fixture_child(&mut child);
        let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
        panic!(
            "write non-loopback fixture requests: {error:#}; child: {child_cleanup}; listener cleanup: {listener_cleanup:?}"
        );
    }

    enum Observation {
        Accepted(std::net::SocketAddr),
        ChildExited(std::process::ExitStatus),
        ChildStatusError(std::io::Error),
        ListenerDisconnected,
        Deadline,
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    let observation = loop {
        match accepted_receiver.try_recv() {
            Ok(peer_address) => break Observation::Accepted(peer_address),
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                break Observation::ListenerDisconnected;
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break Observation::ChildExited(status),
            Ok(None) => {}
            Err(error) => break Observation::ChildStatusError(error),
        }
        if Instant::now() >= deadline {
            break Observation::Deadline;
        }
        std::thread::sleep(Duration::from_millis(10));
    };

    match observation {
        Observation::Accepted(peer_address) => {
            let child_cleanup = terminate_native_fixture_child(&mut child);
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            panic!(
                "native App Server fixture reached the controlled non-loopback listener at {listener_address} from {peer_address}; it must reject the broker URL before connect; child: {child_cleanup}; listener cleanup: {listener_cleanup:?}"
            );
        }
        Observation::ChildExited(status) => {
            let stderr = read_native_fixture_stderr(&mut child)
                .unwrap_or_else(|error| format!("unavailable: {error:#}"));
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            match accepted_receiver.try_recv() {
                Ok(peer_address) => {
                    panic!(
                        "native App Server fixture reached the controlled non-loopback listener at {listener_address} from late peer {peer_address} before rejection; child exited with {status}; stderr: {stderr}; listener cleanup: {listener_cleanup:?}"
                    );
                }
                Err(mpsc::TryRecvError::Disconnected) if listener_cleanup.is_err() => {
                    panic!(
                        "controlled non-loopback listener stopped unexpectedly; child exited with {status}; stderr: {stderr}; listener cleanup: {listener_cleanup:?}"
                    );
                }
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => {}
            }
            listener_cleanup.unwrap();
            assert!(
                !status.success(),
                "native App Server fixture accepted the non-loopback broker URL without connecting; stderr: {stderr}"
            );
            let lower_stderr = stderr.to_ascii_lowercase();
            assert!(
                stderr.contains(&format!("0.0.0.0:{port}")) || lower_stderr.contains("loopback"),
                "native App Server fixture exited nonzero without the expected URL rejection diagnostic; stderr: {stderr}"
            );
        }
        Observation::Deadline => {
            let child_cleanup = terminate_native_fixture_child(&mut child);
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            let late_peer = accepted_receiver.try_recv();
            if let Ok(peer_address) = late_peer {
                panic!(
                    "native App Server fixture reached the controlled non-loopback listener at {listener_address} from late peer {peer_address} at the five-second boundary; child: {child_cleanup}; listener cleanup: {listener_cleanup:?}"
                );
            }
            panic!(
                "native App Server fixture did not reject the non-loopback broker URL with a nonzero exit within five seconds; child: {child_cleanup}; late peer: {late_peer:?}; listener cleanup: {listener_cleanup:?}"
            );
        }
        Observation::ChildStatusError(error) => {
            let child_cleanup = terminate_native_fixture_child(&mut child);
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            let late_peer = accepted_receiver.try_recv();
            panic!(
                "inspect non-loopback fixture status: {error}; child: {child_cleanup}; late peer: {late_peer:?}; listener cleanup: {listener_cleanup:?}"
            );
        }
        Observation::ListenerDisconnected => {
            let child_cleanup = terminate_native_fixture_child(&mut child);
            let listener_cleanup = stop_and_join_listener(&listener_stop, listener_worker);
            let late_peer = accepted_receiver.try_recv();
            panic!(
                "controlled non-loopback listener disconnected before observation; child: {child_cleanup}; late peer: {late_peer:?}; listener cleanup: {listener_cleanup:?}"
            );
        }
    };
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
    let mock_codex = crate::native_app_server_fixture::copy_into(temp.path()).unwrap();
    let live_root = temp.path().join("live-private");
    fs::create_dir(&live_root).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&live_root, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let live_root = live_root.canonicalize().unwrap();
    let (live_case, live_material_root, live_attestation) =
        write_nonempty_source_fixture(temp.path(), &live_root, "notes/evidence.txt", None, None);
    let output = live_root.join("frozen-run-context.json");
    let mut live_args = crate::model::LiveFreezeArgs {
        repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        evidence_repo_root: std::path::PathBuf::from(seed["repoRoot"].as_str().unwrap()),
        fork_sha: seed["repoHead"].as_str().unwrap().to_string(),
        private_root: live_root.clone(),
        codex_bin: mock_codex,
        case: live_case,
        material_root: live_material_root,
        attestation: live_attestation,
        provider_budget_evidence: artifact("providerBudgetEvidence"),
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
    };
    install_retention_managed_cost_inputs(&mut live_args);
    crate::execute_cli(Cli {
        command: crate::EvalCommand::FreezeRunContext(crate::model::FreezeRunContextCommand {
            mode: crate::FreezeRunContextArgs::Live(live_args),
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
        (
            "providerBudgetEvidence",
            "inputs/provider-budget-evidence.json",
        ),
        ("rateCard", "inputs/rate-card.json"),
        ("billingPolicy", "inputs/billing-policy.json"),
        ("fxPolicy", "inputs/fx-policy.json"),
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
    let lead_skill_name = codex_ai_ip_runtime::LEAD_SKILL_NAME;
    assert!(
        live_root
            .join(format!(
                "candidate-home/.codex/skills/{lead_skill_name}/SKILL.md"
            ))
            .is_file(),
        "candidate must install the exact runtime Lead Skill name"
    );
    assert!(
        !live_root
            .join(format!(
                "generic-home/.codex/skills/{lead_skill_name}/SKILL.md"
            ))
            .exists(),
        "generic must not install the target Skill"
    );
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
        assert_eq!(
            launch["argv"],
            json!(["app-server", "--listen", "stdio://", "--strict-config"])
        );
        let keys = launch["envKeys"].as_array().unwrap();
        for forbidden in ["OPENAI_API_KEY", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"] {
            assert!(!keys.iter().any(|key| key == forbidden));
        }
        let methods =
            fs::read_to_string(live_root.join(home).join("app-server-methods.log")).unwrap();
        for required in [
            "skills/list",
            "thread/list",
            "thread/loaded/list",
            "thread/read",
        ] {
            assert!(
                methods.lines().any(|method| method == required),
                "{required}"
            );
        }
        assert_eq!(
            methods
                .lines()
                .filter(|method| *method == "skills/list")
                .count(),
            2,
            "each arm must collect typed pre/post Skill catalogs"
        );
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
    let mut manifests = Vec::new();
    for run_ordinal in [1_u8, 2_u8] {
        let path = live_root
            .join("coordinator")
            .join(format!("run-{run_ordinal}-manifest.json"));
        let manifest: crate::RunManifest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest.validate_execution_mode().unwrap();
        assert_eq!(manifest.run_ordinal, run_ordinal);
        assert_eq!(manifest.execution_mode, ExecutionMode::Mock);
        assert!(matches!(manifest.mode_evidence, ModeEvidence::Mock { .. }));
        assert!(!manifest.is_g2_eligible());
        assert_eq!(manifest.authorized_evaluation_run_cost_fen, 0);
        assert_eq!(
            manifest.pre_skill_catalog_sha256,
            manifest.post_skill_catalog_sha256
        );
        assert!(!manifest.content_package_sha256.is_empty());
        assert!(!manifest.first_root_provider_request_commitment.is_empty());
        assert!(!manifest.normalized_first_root_base_commitment.is_empty());
        manifests.push(manifest);
    }
    let generic_manifest = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Generic)
        .unwrap();
    let candidate_manifest = manifests
        .iter()
        .find(|manifest| manifest.condition == EvaluationCondition::Candidate)
        .unwrap();
    assert_eq!(generic_manifest.native_skill_sha256, None);
    assert_eq!(generic_manifest.skill_use_evidence_sha256, None);
    assert_eq!(generic_manifest.first_root_treatment_diff_commitment, None);
    assert!(candidate_manifest.native_skill_sha256.is_some());
    assert!(candidate_manifest.skill_use_evidence_sha256.is_some());
    assert!(
        candidate_manifest
            .first_root_treatment_diff_commitment
            .is_some()
    );
    assert_eq!(
        generic_manifest.normalized_base_catalog_sha256,
        candidate_manifest.normalized_base_catalog_sha256
    );
    assert_eq!(
        generic_manifest.normalized_first_root_base_commitment,
        candidate_manifest.normalized_first_root_base_commitment
    );
    assert_ne!(
        generic_manifest.content_package_sha256,
        candidate_manifest.content_package_sha256
    );
    assert_eq!(
        generic_manifest.effective_config_sha256,
        candidate_manifest.effective_config_sha256
    );
    assert_eq!(
        generic_manifest.config_layers_sha256,
        candidate_manifest.config_layers_sha256
    );
    let pair_verification =
        fs::read_to_string(live_root.join("coordinator/pair-verification.json")).unwrap();
    let pair_verification_json: serde_json::Value =
        serde_json::from_str(&pair_verification).unwrap();
    assert_eq!(
        pair_verification_json["requestParity"]["normalizedBaseCommitment"],
        json!(generic_manifest.normalized_first_root_base_commitment)
    );
    assert_eq!(
        pair_verification_json["genericContentPackageSha256"],
        generic_manifest.content_package_sha256
    );
    assert_eq!(
        pair_verification_json["candidateContentPackageSha256"],
        candidate_manifest.content_package_sha256
    );
    assert!(pair_verification_json.get("contentPackageSha256").is_none());
    assert!(!pair_verification.contains("frozen mission"));
    assert!(!pair_verification.contains("Synthetic replay summary"));
    assert!(
        live_root
            .join("coordinator/receipts/pair-receipt.json")
            .is_file()
    );
    let pair_receipt: crate::PairReceipt = serde_json::from_slice(
        &fs::read(live_root.join("coordinator/receipts/pair-receipt.json")).unwrap(),
    )
    .unwrap();
    assert!(pair_receipt.first_run_manifest_sha256.is_some());
    assert!(pair_receipt.second_run_manifest_sha256.is_some());
    let ledger = fs::read_to_string(live_root.join("coordinator/attempt-index.jsonl")).unwrap();
    assert!(!ledger.contains("frozen mission"));
    assert!(!ledger.contains("Return the frozen synthetic package"));
    assert_eq!(ledger.lines().count(), 8);
}
