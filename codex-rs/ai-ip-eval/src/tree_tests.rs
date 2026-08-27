use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadListResponse;
use codex_app_server_protocol::ThreadLoadedListResponse;
use codex_app_server_protocol::ThreadReadResponse;
use codex_responses_api_proxy::ObservedUsage;
use codex_responses_api_proxy::ResponseCompletedMetadata;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::SkillUseTracker;
use crate::TreeEventCollector;
use crate::TreeScan;
use crate::tests::package_json;

fn thread(id: &str, parent: Option<&str>) -> Thread {
    let mut value = json!({
        "id": id,
        "extra": null,
        "sessionId": "session-1",
        "forkedFromId": null,
        "parentThreadId": parent,
        "preview": "synthetic",
        "ephemeral": false,
        "section": null,
        "sectionEnteredAt": null,
        "projectId": null,
        "historyMode": "legacy",
        "modelProvider": "ai-ip-proof-broker",
        "createdAt": 1,
        "updatedAt": 1,
        "recencyAt": null,
        "status": {"type": "idle"},
        "path": null,
        "cwd": "/synthetic/case",
        "cliVersion": "synthetic",
        "source": "appServer",
        "canAcceptDirectInput": true,
        "threadSource": if parent.is_some() { json!("subagent") } else { json!("user") },
        "agentNickname": null,
        "agentRole": null,
        "gitInfo": null,
        "name": null,
        "turns": []
    });
    if parent.is_some() {
        value["turns"] = json!([{
            "id": format!("turn-{id}"),
            "items": [],
            "itemsView": "full",
            "status": "completed",
            "error": null,
            "startedAt": null,
            "completedAt": null,
            "durationMs": 1
        }]);
    }
    serde_json::from_value(value).unwrap()
}

fn notification(value: serde_json::Value) -> ServerNotification {
    serde_json::from_value(value).unwrap()
}

fn thread_started(thread: &Thread) -> ServerNotification {
    notification(json!({"method": "thread/started", "params": {"thread": thread}}))
}

fn turn_started(thread_id: &str, turn_id: &str) -> ServerNotification {
    notification(json!({
        "method": "turn/started",
        "params": {
            "threadId": thread_id,
            "turn": {
                "id": turn_id,
                "items": [],
                "itemsView": "full",
                "status": "inProgress",
                "error": null,
                "startedAt": null,
                "completedAt": null,
                "durationMs": null
            }
        }
    }))
}

fn turn_completed(thread_id: &str, turn_id: &str) -> ServerNotification {
    notification(json!({
        "method": "turn/completed",
        "params": {
            "threadId": thread_id,
            "turn": {
                "id": turn_id,
                "items": [],
                "itemsView": "full",
                "status": "completed",
                "error": null,
                "startedAt": null,
                "completedAt": null,
                "durationMs": null
            }
        }
    }))
}

fn raw(thread_id: &str, turn_id: &str, response_id: &str, total: i64) -> ServerNotification {
    notification(json!({
        "method": "rawResponse/completed",
        "params": {
            "threadId": thread_id,
            "turnId": turn_id,
            "responseId": response_id,
            "usage": {
                "totalTokens": total,
                "inputTokens": total - 1,
                "cachedInputTokens": 0,
                "cacheWriteInputTokens": 0,
                "outputTokens": 1,
                "reasoningOutputTokens": 0
            }
        }
    }))
}

fn scans(root: &Thread, descendants: &[Thread]) -> TreeScan {
    let ancestor_pages = vec![
        ThreadListResponse {
            data: descendants[..2].to_vec(),
            next_cursor: Some("page-2".to_string()),
            backwards_cursor: None,
        },
        ThreadListResponse {
            data: descendants[2..].to_vec(),
            next_cursor: None,
            backwards_cursor: None,
        },
    ];
    let all = std::iter::once(root)
        .chain(descendants.iter())
        .cloned()
        .collect::<Vec<_>>();
    let loaded_pages = vec![
        ThreadLoadedListResponse {
            data: all[..2].iter().map(|thread| thread.id.clone()).collect(),
            next_cursor: Some("loaded-2".to_string()),
        },
        ThreadLoadedListResponse {
            data: all[2..].iter().map(|thread| thread.id.clone()).collect(),
            next_cursor: None,
        },
    ];
    let reads = all
        .into_iter()
        .map(|thread| ThreadReadResponse { thread })
        .collect::<Vec<_>>();
    TreeScan::from_typed_pages(root, &ancestor_pages, &loaded_pages, &reads).unwrap()
}

fn root_scan(root: &Thread) -> TreeScan {
    TreeScan::from_typed_pages(
        root,
        &[ThreadListResponse {
            data: vec![],
            next_cursor: None,
            backwards_cursor: None,
        }],
        &[ThreadLoadedListResponse {
            data: vec![root.id.clone()],
            next_cursor: None,
        }],
        &[ThreadReadResponse {
            thread: root.clone(),
        }],
    )
    .unwrap()
}

#[test]
fn tree_collector_closes_root_child_grandchild_and_sibling_with_exact_usage_parity() {
    let root = thread("root", None);
    let child = thread("child", Some("root"));
    let grandchild = thread("grandchild", Some("child"));
    let sibling = thread("sibling", Some("root"));
    let descendants = vec![child.clone(), grandchild.clone(), sibling.clone()];
    let first_scan = scans(&root, &descendants);
    let second_scan = scans(&root, &descendants);
    assert_eq!(first_scan.thread_count(), 4);
    let broker_threads = ["root", "child", "grandchild", "sibling"]
        .into_iter()
        .map(str::to_string)
        .collect();
    first_scan
        .verify_broker_thread_ids(&broker_threads)
        .unwrap();
    assert!(
        first_scan
            .verify_broker_thread_ids(&["root".to_string()].into_iter().collect())
            .is_err()
    );

    let mut collector = TreeEventCollector::new(&root).unwrap();
    for descendant in [&child, &grandchild, &sibling] {
        collector.ingest(thread_started(descendant)).unwrap();
    }
    let arms = [
        ("root", "turn-root", "response-root", 10),
        ("child", "turn-child", "response-child", 20),
        ("grandchild", "turn-grandchild", "response-grandchild", 30),
        ("sibling", "turn-sibling", "response-sibling", 40),
    ];
    for (thread_id, turn_id, response_id, total) in arms {
        collector.ingest(turn_started(thread_id, turn_id)).unwrap();
        collector
            .ingest(raw(thread_id, turn_id, response_id, total))
            .unwrap();
        collector
            .ingest(turn_completed(thread_id, turn_id))
            .unwrap();
    }
    let broker = arms
        .into_iter()
        .map(|(_, _, response_id, total)| ResponseCompletedMetadata {
            response_id: response_id.to_string(),
            usage: Some(ObservedUsage {
                total_tokens: total,
                input_tokens: total - 1,
                cached_input_tokens: 0,
                cache_write_input_tokens: 0,
                output_tokens: 1,
                reasoning_output_tokens: 0,
            }),
            actual_model: Some("synthetic".to_string()),
            deployment_or_fingerprint: None,
        })
        .collect::<Vec<_>>();

    let evidence = collector
        .close(&first_scan, &second_scan, &broker, 0)
        .unwrap();
    assert!(evidence.tree_closed);
    assert_eq!(evidence.thread_count, 4);
    assert_eq!(evidence.raw_response_count, 4);
    assert_eq!(evidence.usage.total_tokens, 100);
}

#[test]
fn typed_quiet_scan_rejects_descendant_without_terminal_turn_history() {
    let root = thread("root", None);
    let mut child = thread("child", Some("root"));
    child.turns.clear();
    let scan = TreeScan::from_typed_pages(
        &root,
        &[ThreadListResponse {
            data: vec![child.clone()],
            next_cursor: None,
            backwards_cursor: None,
        }],
        &[ThreadLoadedListResponse {
            data: vec![root.id.clone(), child.id.clone()],
            next_cursor: None,
        }],
        &[
            ThreadReadResponse {
                thread: root.clone(),
            },
            ThreadReadResponse { thread: child },
        ],
    );
    assert!(scan.is_err());
}

#[test]
fn typed_quiet_scan_rejects_unloaded_descendant_without_terminal_turn_history() {
    let root = thread("root", None);
    let mut child = thread("child", Some("root"));
    child.turns.clear();
    let scan = TreeScan::from_typed_pages(
        &root,
        &[ThreadListResponse {
            data: vec![child],
            next_cursor: None,
            backwards_cursor: None,
        }],
        &[ThreadLoadedListResponse {
            data: vec![root.id.clone()],
            next_cursor: None,
        }],
        &[ThreadReadResponse {
            thread: root.clone(),
        }],
    );

    assert!(scan.is_err());
}

#[test]
fn tree_collector_rejects_unknown_parent_unfinished_turn_scan_drift_and_usage_drift() {
    let root = thread("root", None);
    let orphan = thread("orphan", Some("missing"));
    let error = TreeEventCollector::new(&root)
        .unwrap()
        .ingest(thread_started(&orphan))
        .unwrap_err();
    assert!(error.to_string().contains("parent"));

    let quiet = root_scan(&root);
    let mut collector = TreeEventCollector::new(&root).unwrap();
    collector.ingest(turn_started("root", "turn-root")).unwrap();
    assert!(
        collector
            .close(&quiet, &quiet, &[], 0)
            .unwrap_err()
            .to_string()
            .contains("terminal")
    );

    let mut in_flight = TreeEventCollector::new(&root).unwrap();
    in_flight.ingest(turn_started("root", "turn-root")).unwrap();
    in_flight
        .ingest(turn_completed("root", "turn-root"))
        .unwrap();
    assert!(
        in_flight
            .close(&quiet, &quiet, &[], 1)
            .unwrap_err()
            .to_string()
            .contains("in-flight")
    );

    let mut usage_drift = TreeEventCollector::new(&root).unwrap();
    usage_drift
        .ingest(turn_started("root", "turn-root"))
        .unwrap();
    usage_drift
        .ingest(raw("root", "turn-root", "response-root", 10))
        .unwrap();
    usage_drift
        .ingest(turn_completed("root", "turn-root"))
        .unwrap();
    let broker = [ResponseCompletedMetadata {
        response_id: "response-root".to_string(),
        usage: Some(ObservedUsage {
            total_tokens: 11,
            input_tokens: 10,
            cached_input_tokens: 0,
            cache_write_input_tokens: 0,
            output_tokens: 1,
            reasoning_output_tokens: 0,
        }),
        actual_model: None,
        deployment_or_fingerprint: None,
    }];
    assert!(
        usage_drift
            .close(&quiet, &quiet, &broker, 0)
            .unwrap_err()
            .to_string()
            .contains("usage differ")
    );
}

#[test]
fn tree_scan_rejects_repeated_cursors_and_guardian_threads() {
    let root = thread("root", None);
    let repeated = [
        ThreadListResponse {
            data: vec![],
            next_cursor: Some("repeat".to_string()),
            backwards_cursor: None,
        },
        ThreadListResponse {
            data: vec![],
            next_cursor: Some("repeat".to_string()),
            backwards_cursor: None,
        },
        ThreadListResponse {
            data: vec![],
            next_cursor: None,
            backwards_cursor: None,
        },
    ];
    let error = TreeScan::from_typed_pages(
        &root,
        &repeated,
        &[ThreadLoadedListResponse {
            data: vec!["root".to_string()],
            next_cursor: None,
        }],
        &[ThreadReadResponse {
            thread: root.clone(),
        }],
    )
    .unwrap_err();
    assert!(error.to_string().contains("repeated"));

    let mut guardian_value = serde_json::to_value(thread("guardian", Some("root"))).unwrap();
    guardian_value["threadSource"] = json!("guardian_review");
    let guardian: Thread = serde_json::from_value(guardian_value).unwrap();
    assert!(
        TreeEventCollector::new(&guardian)
            .unwrap_err()
            .to_string()
            .contains("GuardianReview")
    );
}

#[test]
fn skill_use_requires_a_successful_complete_canonical_read_before_the_final_package() {
    let temp = tempfile::tempdir().unwrap();
    let skill = temp.path().join("SKILL.md");
    std::fs::write(&skill, "synthetic complete skill\n").unwrap();
    let command = format!("cat {}", skill.display());
    let command_item = notification(json!({
        "method": "item/completed",
        "params": {
            "threadId": "root",
            "turnId": "turn",
            "completedAtMs": 1,
            "item": {
                "type": "commandExecution",
                "id": "read-skill",
                "pluginId": null,
                "scriptPath": null,
                "command": command,
                "cwd": temp.path(),
                "processId": null,
                "source": "unifiedExecStartup",
                "status": "completed",
                "commandActions": [],
                "aggregatedOutput": "synthetic complete skill\n",
                "exitCode": 0,
                "durationMs": 1
            }
        }
    }));
    let final_item = notification(json!({
        "method": "item/completed",
        "params": {
            "threadId": "root",
            "turnId": "turn",
            "completedAtMs": 2,
            "item": {"type": "agentMessage", "id": "final", "text": package_json()}
        }
    }));
    let mut tracker = SkillUseTracker::new(&skill, b"synthetic complete skill\n").unwrap();
    tracker.ingest(&command_item).unwrap();
    tracker.ingest(&final_item).unwrap();
    assert_eq!(tracker.finish().unwrap().len(), 64);

    let mut incomplete = SkillUseTracker::new(&skill, b"synthetic complete skill\n").unwrap();
    incomplete.ingest(&final_item).unwrap();
    assert!(
        incomplete
            .finish()
            .unwrap_err()
            .to_string()
            .contains("did not successfully read")
    );
}
