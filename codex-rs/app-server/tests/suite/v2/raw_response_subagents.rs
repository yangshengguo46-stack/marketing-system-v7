use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::write_models_cache;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::RawResponseCompletedNotification;
use codex_app_server_protocol::ThreadReadParams;
use codex_app_server_protocol::ThreadReadResponse;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::ThreadStartedNotification;
use codex_app_server_protocol::TokenUsageBreakdown;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use codex_features::Feature;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;
use wiremock::Mock;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path_regex;

const READ_TIMEOUT: Duration = Duration::from_secs(20);
const ROOT_PROMPT: &str = "spawn both synthetic workers";
const CHILD_PROMPT: &str = "spawn a grandchild for synthetic child work";
const SIBLING_PROMPT: &str = "synthetic sibling work";
const GRANDCHILD_PROMPT: &str = "synthetic grandchild work";
const ROOT_CHILD_CALL: &str = "spawn-root-child";
const ROOT_SIBLING_CALL: &str = "spawn-root-sibling";
const CHILD_GRANDCHILD_CALL: &str = "spawn-child-grandchild";

fn request_body(request: &wiremock::Request) -> String {
    let is_zstd = request
        .headers
        .get("content-encoding")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|entry| entry.trim().eq_ignore_ascii_case("zstd"))
        });
    let body = if is_zstd {
        zstd::stream::decode_all(std::io::Cursor::new(&request.body))
            .expect("decode zstd Responses request")
    } else {
        request.body.clone()
    };
    String::from_utf8(body).expect("Responses request body is UTF-8 JSON")
}

fn completion(response_id: &str, total_tokens: i64) -> Vec<Value> {
    vec![
        responses::ev_response_created(response_id),
        responses::ev_assistant_message(&format!("msg-{response_id}"), "synthetic done"),
        responses::ev_completed_with_tokens(response_id, total_tokens),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AgentTreeRoute {
    RootInitial,
    RootFinal,
    ChildInitial,
    ChildFinal,
    Grandchild,
    Sibling,
}

#[derive(Clone)]
struct AgentTreeResponder {
    routes: Arc<Mutex<Vec<AgentTreeRoute>>>,
    root_child_args: String,
    root_sibling_args: String,
    grandchild_args: String,
}

impl Respond for AgentTreeResponder {
    fn respond(&self, request: &wiremock::Request) -> ResponseTemplate {
        let body = request_body(request);
        let (route, events) = if body.contains(CHILD_GRANDCHILD_CALL) {
            (AgentTreeRoute::ChildFinal, completion("resp-child-2", 22))
        } else if body.contains(ROOT_CHILD_CALL) {
            (AgentTreeRoute::RootFinal, completion("resp-root-2", 12))
        } else if body.contains(GRANDCHILD_PROMPT) {
            (
                AgentTreeRoute::Grandchild,
                completion("resp-grandchild", 31),
            )
        } else if body.contains(SIBLING_PROMPT) {
            (AgentTreeRoute::Sibling, completion("resp-sibling", 41))
        } else if body.contains(CHILD_PROMPT) {
            (
                AgentTreeRoute::ChildInitial,
                vec![
                    responses::ev_response_created("resp-child-1"),
                    responses::ev_function_call_with_namespace(
                        CHILD_GRANDCHILD_CALL,
                        "collaboration",
                        "spawn_agent",
                        &self.grandchild_args,
                    ),
                    responses::ev_completed_with_tokens("resp-child-1", 21),
                ],
            )
        } else if body.contains(ROOT_PROMPT) {
            (
                AgentTreeRoute::RootInitial,
                vec![
                    responses::ev_response_created("resp-root-1"),
                    responses::ev_function_call_with_namespace(
                        ROOT_CHILD_CALL,
                        "collaboration",
                        "spawn_agent",
                        &self.root_child_args,
                    ),
                    responses::ev_function_call_with_namespace(
                        ROOT_SIBLING_CALL,
                        "collaboration",
                        "spawn_agent",
                        &self.root_sibling_args,
                    ),
                    responses::ev_completed_with_tokens("resp-root-1", 11),
                ],
            )
        } else {
            panic!("unrecognized synthetic agent-tree request")
        };
        self.routes
            .lock()
            .expect("agent-tree route mutex poisoned")
            .push(route);
        let response = ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(responses::sse(events));
        if route == AgentTreeRoute::RootFinal {
            response.set_delay(Duration::from_secs(2))
        } else {
            response
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_events_cover_root_child_grandchild_and_sibling() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let root_child_args = serde_json::to_string(&json!({
        "message": CHILD_PROMPT,
        "task_name": "child",
        "fork_turns": "none"
    }))?;
    let root_sibling_args = serde_json::to_string(&json!({
        "message": SIBLING_PROMPT,
        "task_name": "sibling",
        "fork_turns": "none"
    }))?;
    let grandchild_args = serde_json::to_string(&json!({
        "message": GRANDCHILD_PROMPT,
        "task_name": "grandchild",
        "fork_turns": "none"
    }))?;
    let routes = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("POST"))
        .and(path_regex(".*/responses$"))
        .respond_with(AgentTreeResponder {
            routes: routes.clone(),
            root_child_args,
            root_sibling_args,
            grandchild_args,
        })
        .mount(&server)
        .await;

    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::Collab)
        .enable_feature(Feature::MultiAgentV2)
        .enable_feature(Feature::Goals)
        .enable_feature(Feature::RealtimeConversation)
        .with_root_config(&format!("chatgpt_base_url = \"{}\"", server.uri()))
        .write(codex_home.path())?;
    write_models_cache(codex_home.path()).await?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized()
        .await?;
    let ThreadStartResponse { thread: root, .. } = app_server
        .start_thread(ThreadStartParams {
            model: Some("gpt-5.6-sol".to_string()),
            experimental_raw_events: true,
            ..Default::default()
        })
        .await?;
    let root_turn: TurnStartResponse = app_server
        .request(|request_id| ClientRequest::TurnStart {
            request_id,
            params: TurnStartParams {
                thread_id: root.id.clone(),
                input: vec![UserInput::Text {
                    text: ROOT_PROMPT.to_string(),
                    text_elements: Vec::new(),
                }],
                ..Default::default()
            },
        })
        .await?;

    let expected_usage = HashMap::from([
        ("resp-root-1", 11),
        ("resp-root-2", 12),
        ("resp-child-1", 21),
        ("resp-child-2", 22),
        ("resp-grandchild", 31),
        ("resp-sibling", 41),
    ]);
    let mut started_by_id = HashMap::new();
    let mut raw_by_response = HashMap::new();
    let mut terminal_order = Vec::new();
    let mut terminal_turns = HashMap::new();
    let (child_id, sibling_id, grandchild_id, expected_thread_ids) = loop {
        let message = timeout(READ_TIMEOUT, app_server.read_next_message())
            .await
            .with_context(|| {
                format!(
                    "timed out reading the mixed notification stream after routes {:?}",
                    routes.lock().expect("agent-tree route mutex poisoned")
                )
            })??;
        let JSONRPCMessage::Notification(notification) = message else {
            continue;
        };
        let method = notification.method;
        let Some(params) = notification.params else {
            continue;
        };
        match method.as_str() {
            "thread/started" => {
                let event: ThreadStartedNotification = serde_json::from_value(params)?;
                assert!(
                    started_by_id
                        .insert(event.thread.id.clone(), event.thread)
                        .is_none()
                );
            }
            "rawResponse/completed" => {
                let event: RawResponseCompletedNotification = serde_json::from_value(params)?;
                let expected_total = expected_usage
                    .get(event.response_id.as_str())
                    .with_context(|| format!("unexpected response id {}", event.response_id))?;
                assert_eq!(
                    event.usage,
                    Some(TokenUsageBreakdown {
                        total_tokens: *expected_total,
                        input_tokens: *expected_total,
                        cached_input_tokens: 0,
                        cache_write_input_tokens: 0,
                        output_tokens: 0,
                        reasoning_output_tokens: 0,
                    })
                );
                assert!(
                    raw_by_response
                        .insert(event.response_id.clone(), event)
                        .is_none()
                );
            }
            "turn/completed" => {
                let event: TurnCompletedNotification = serde_json::from_value(params)?;
                if terminal_turns
                    .insert(event.thread_id.clone(), event.turn.id.clone())
                    .is_none()
                {
                    terminal_order.push(event.thread_id.clone());
                }
                if event.thread_id != root.id || event.turn.id != root_turn.turn.id {
                    continue;
                }

                assert_eq!(
                    raw_by_response
                        .keys()
                        .map(String::as_str)
                        .collect::<HashSet<_>>(),
                    expected_usage.keys().copied().collect::<HashSet<_>>(),
                    "all raw completions must already exist when the root terminal arrives"
                );
                let child_id = raw_by_response["resp-child-1"].thread_id.clone();
                let sibling_id = raw_by_response["resp-sibling"].thread_id.clone();
                let grandchild_id = raw_by_response["resp-grandchild"].thread_id.clone();
                let expected_thread_ids = HashSet::from([
                    root.id.clone(),
                    child_id.clone(),
                    sibling_id.clone(),
                    grandchild_id.clone(),
                ]);
                assert_eq!(
                    started_by_id.keys().cloned().collect::<HashSet<_>>(),
                    expected_thread_ids,
                    "all parent-edge notifications must already exist when the root terminal arrives"
                );
                assert_eq!(
                    terminal_turns.keys().cloned().collect::<HashSet<_>>(),
                    expected_thread_ids,
                    "all descendant terminals must already exist when the root terminal arrives"
                );
                assert_eq!(terminal_order.last(), Some(&root.id));
                assert_eq!(started_by_id[&root.id].parent_thread_id, None);
                for (id, expected_parent) in [
                    (child_id.as_str(), Some(root.id.as_str())),
                    (sibling_id.as_str(), Some(root.id.as_str())),
                    (grandchild_id.as_str(), Some(child_id.as_str())),
                ] {
                    assert_eq!(
                        started_by_id[id].parent_thread_id.as_deref(),
                        expected_parent,
                        "thread/started parent edge must match the spawned agent tree before the root terminal"
                    );
                }
                break (child_id, sibling_id, grandchild_id, expected_thread_ids);
            }
            _ => {}
        }
    };

    assert_eq!(raw_by_response["resp-root-1"].thread_id, root.id);
    assert_eq!(raw_by_response["resp-root-2"].thread_id, root.id);
    assert_eq!(raw_by_response["resp-child-2"].thread_id, child_id);
    assert_eq!(
        raw_by_response
            .values()
            .map(|event| event.thread_id.clone())
            .collect::<HashSet<_>>(),
        expected_thread_ids
    );
    for (id, expected_parent) in [
        (child_id.as_str(), Some(root.id.as_str())),
        (sibling_id.as_str(), Some(root.id.as_str())),
        (grandchild_id.as_str(), Some(child_id.as_str())),
    ] {
        assert_eq!(
            started_by_id[id].parent_thread_id.as_deref(),
            expected_parent,
            "thread/started parent edge must match the spawned agent tree"
        );
        let read: ThreadReadResponse = app_server
            .request(|request_id| ClientRequest::ThreadRead {
                request_id,
                params: ThreadReadParams {
                    thread_id: id.to_string(),
                    include_turns: false,
                },
            })
            .await?;
        assert_eq!(read.thread.parent_thread_id.as_deref(), expected_parent);
    }
    assert_eq!(
        terminal_turns.get(&root.id),
        Some(&root_turn.turn.id),
        "root terminal must identify the turn started by the external client"
    );
    for (response_id, event) in &raw_by_response {
        assert_eq!(
            terminal_turns.get(&event.thread_id),
            Some(&event.turn_id),
            "raw response {response_id} must identify its thread's completed turn"
        );
    }
    let observed_routes = routes
        .lock()
        .expect("agent-tree route mutex poisoned")
        .clone();
    assert_eq!(observed_routes.len(), 6);
    assert_eq!(
        observed_routes.into_iter().collect::<HashSet<_>>(),
        HashSet::from([
            AgentTreeRoute::RootInitial,
            AgentTreeRoute::RootFinal,
            AgentTreeRoute::ChildInitial,
            AgentTreeRoute::ChildFinal,
            AgentTreeRoute::Grandchild,
            AgentTreeRoute::Sibling,
        ])
    );

    Ok(())
}
