use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use codex_ai_ip_runtime::content_package_schema;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use core_test_support::responses;
use core_test_support::skip_if_no_network;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

// Native app-server cold startup can exceed ten seconds on macOS and Windows.
#[cfg(any(target_os = "macos", windows))]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
#[cfg(not(any(target_os = "macos", windows)))]
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

fn assert_local_refs_resolve(root: &Value, value: &Value) {
    match value {
        Value::Array(values) => {
            for value in values {
                assert_local_refs_resolve(root, value);
            }
        }
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                assert!(
                    reference.starts_with("#/$defs/"),
                    "unexpected local schema reference {reference}"
                );
                assert!(
                    root.pointer(reference.trim_start_matches('#')).is_some(),
                    "dangling local schema reference {reference}"
                );
            }
            for value in object.values() {
                assert_local_refs_resolve(root, value);
            }
        }
        Value::Bool(_) | Value::Null | Value::Number(_) | Value::String(_) => {}
    }
}

#[tokio::test]
async fn turn_start_sends_strict_content_package_schema() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = responses::start_mock_server().await;
    let valid_package = json!({
        "missionSummary": "Synthetic mission",
        "influenceRelation": {
            "subject": "Synthetic subject",
            "subjectKind": "brand",
            "audience": "Synthetic audience",
            "desiredAction": "Review the package"
        },
        "actionFunnel": [{
            "audienceState": "Aware",
            "intendedChange": "Understands the proposal",
            "nextAction": "Review"
        }],
        "strategicJudgment": "Synthetic judgment",
        "publishableContent": {
            "format": "short post",
            "title": null,
            "body": "Synthetic publishable body",
            "productionNotes": []
        },
        "claims": [],
        "openQuestions": [],
        "measurementPlan": {
            "successSignal": "A completed review",
            "collectionMethod": "Review receipt",
            "observationWindow": "One day"
        },
        "readiness": "draft"
    })
    .to_string();
    let body = responses::sse(vec![
        responses::ev_response_created("resp-ai-ip-schema"),
        responses::ev_assistant_message("msg-ai-ip-schema", &valid_package),
        responses::ev_completed("resp-ai-ip-schema"),
    ]);
    let response_mock = responses::mount_sse_once(&server, body).await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri()).write(codex_home.path())?;
    let mut app_server = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(DEFAULT_READ_TIMEOUT)
        .await?;

    let ThreadStartResponse { thread, .. } = app_server
        .start_thread(ThreadStartParams {
            model: Some("mock-model".to_string()),
            ..Default::default()
        })
        .await?;
    let schema = content_package_schema()?;
    let _: TurnStartResponse = app_server
        .request(
            |request_id| codex_app_server_protocol::ClientRequest::TurnStart {
                request_id,
                params: TurnStartParams {
                    thread_id: thread.id,
                    input: vec![UserInput::Text {
                        text: "Produce the synthetic package".to_string(),
                        text_elements: Vec::new(),
                    }],
                    output_schema: Some(schema.clone()),
                    ..Default::default()
                },
            },
        )
        .await?;
    timeout(
        DEFAULT_READ_TIMEOUT,
        app_server.read_notification::<TurnCompletedNotification>("turn/completed"),
    )
    .await??;

    let request = response_mock.single_request();
    let request_body = request.body_json();
    let provider_format = request_body
        .pointer("/text/format")
        .expect("provider request must contain text.format");
    assert_eq!(
        provider_format,
        &json!({
            "name": "codex_output_schema",
            "type": "json_schema",
            "strict": true,
            "schema": schema,
        })
    );
    let provider_schema = provider_format
        .get("schema")
        .expect("provider text format must contain the schema");
    assert!(provider_schema.get("$schema").is_none());
    assert!(provider_schema.get("definitions").is_none());
    assert_local_refs_resolve(provider_schema, provider_schema);
    Ok(())
}
