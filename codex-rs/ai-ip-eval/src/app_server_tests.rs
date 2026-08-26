use std::fs;
use std::time::Duration;

use codex_app_server_protocol::AdditionalContextKind;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ConfigReadResponse;
use codex_app_server_protocol::ConfigRequirementsReadResponse;
use pretty_assertions::assert_eq;
use serde_json::json;

use crate::ConfigAuditExpectation;
use crate::JsonLineClient;
use crate::audit_config;
use crate::build_shared_config;
use crate::build_thread_start;
use crate::build_turn_start;
use crate::config_read_params;
use crate::initialize_params;
use crate::tests::mission_case;

#[test]
fn typed_requests_are_byte_identical_across_arms_and_strictly_project_the_mission() {
    let temp = tempfile::tempdir().unwrap();
    let case_dir = temp.path().canonicalize().unwrap();
    let generic_thread =
        build_thread_start("glm-approved", "ai-ip-proof-broker", &case_dir).unwrap();
    let candidate_thread =
        build_thread_start("glm-approved", "ai-ip-proof-broker", &case_dir).unwrap();
    assert_eq!(generic_thread, candidate_thread);
    assert_eq!(generic_thread.approval_policy, Some(AskForApproval::Never));
    assert_eq!(generic_thread.permissions.as_deref(), Some("ai-ip-eval"));
    assert_eq!(generic_thread.ephemeral, Some(false));
    assert!(generic_thread.experimental_raw_events);

    let generic_turn = build_turn_start("root-thread", &mission_case()).unwrap();
    let candidate_turn = build_turn_start("root-thread", &mission_case()).unwrap();
    assert_eq!(generic_turn, candidate_turn);
    assert_eq!(generic_turn.additional_context.as_ref().unwrap().len(), 1);
    let context = &generic_turn.additional_context.as_ref().unwrap()["ai_ip_evaluation"];
    assert_eq!(context.kind, AdditionalContextKind::Untrusted);
    assert_eq!(
        generic_turn.output_schema,
        Some(codex_ai_ip_runtime::content_package_schema().unwrap())
    );
    assert_eq!(generic_turn.approval_policy, Some(AskForApproval::Never));
    assert_eq!(generic_turn.permissions.as_deref(), Some("ai-ip-eval"));
}

#[test]
fn initialization_and_config_read_handshake_are_exact() {
    let initialize = initialize_params();
    assert_eq!(initialize.client_info.name, "codex_ai_ip_eval");
    assert_eq!(initialize.client_info.title, None);
    assert!(initialize.capabilities.unwrap().experimental_api);

    let temp = tempfile::tempdir().unwrap();
    let canonical = temp.path().canonicalize().unwrap();
    let read = config_read_params(&canonical).unwrap();
    assert!(read.include_layers);
    assert_eq!(read.cwd.as_deref(), canonical.to_str());
}

#[test]
fn shared_config_builder_freezes_the_proof_controls_without_a_secret_or_auth_source() {
    let config = build_shared_config("glm-approved", 43123).unwrap();
    let text = std::str::from_utf8(&config.bytes).unwrap();
    assert!(!text.contains('\r'));
    assert!(text.ends_with('\n'));
    assert_eq!(config.layer_json["model"], json!("glm-approved"));
    assert_eq!(config.layer_json["approvals_reviewer"], json!("user"));
    assert_eq!(
        config.layer_json["default_permissions"],
        json!("ai-ip-eval")
    );
    assert_eq!(
        config.layer_json["features"]["guardian_approval"],
        json!(false)
    );
    assert_eq!(
        config.layer_json["features"]["guardianv2"]["enabled"],
        json!(false)
    );
    assert_eq!(
        config.layer_json["permissions"]["ai-ip-eval"]["network"]["enabled"],
        json!(false)
    );
    let provider = &config.layer_json["model_providers"]["ai-ip-proof-broker"];
    assert_eq!(provider["name"], json!("OpenAI"));
    assert_eq!(provider["request_max_retries"], json!(0));
    assert_eq!(provider["stream_max_retries"], json!(0));
    assert_eq!(provider["supports_websockets"], json!(false));
    for forbidden in [
        "env_key",
        "bearer",
        "headers",
        "auth.json",
        "[mcp",
        "[plugins",
    ] {
        assert!(!text.to_ascii_lowercase().contains(forbidden));
    }
}

#[test]
fn config_audit_accepts_only_the_exact_single_user_layer() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let config_bytes =
        b"model = \"glm-approved\"\nmodel_provider = \"ai-ip-proof-broker\"\n".to_vec();
    fs::write(&config_path, &config_bytes).unwrap();
    let canonical_config_path = config_path.canonicalize().unwrap();
    let layer_json = json!({
        "model": "glm-approved",
        "model_provider": "ai-ip-proof-broker"
    });
    let response: ConfigReadResponse = serde_json::from_value(json!({
        "config": {
            "model": "glm-approved",
            "model_provider": "ai-ip-proof-broker"
        },
        "origins": {
            "model": {
                "name": {"type": "user", "file": canonical_config_path, "profile": null},
                "version": "v1"
            }
        },
        "layers": [{
            "name": {"type": "user", "file": canonical_config_path, "profile": null},
            "version": "v1",
            "config": layer_json
        }]
    }))
    .unwrap();
    let requirements = ConfigRequirementsReadResponse { requirements: None };
    let expectation = ConfigAuditExpectation {
        canonical_config_path,
        expected_config_bytes: config_bytes,
        expected_layer_config: layer_json,
        expected_effective_config: serde_json::to_value(&response.config).unwrap(),
    };

    let evidence = audit_config(&response, &requirements, &expectation).unwrap();
    assert_eq!(evidence.effective_config_sha256.len(), 64);
    assert_eq!(evidence.config_layers_sha256.len(), 64);
}

#[test]
fn config_audit_rejects_requirements_profiles_extra_layers_and_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    fs::write(&config_path, b"model = \"glm\"\n").unwrap();
    let canonical = config_path.canonicalize().unwrap();
    let base = json!({
        "config": {"model": "glm"},
        "origins": {},
        "layers": [{
            "name": {"type": "user", "file": canonical, "profile": null},
            "version": "v1",
            "config": {"model": "glm"}
        }]
    });
    let response: ConfigReadResponse = serde_json::from_value(base.clone()).unwrap();
    let expectation = ConfigAuditExpectation {
        canonical_config_path: canonical,
        expected_config_bytes: b"model = \"glm\"\n".to_vec(),
        expected_layer_config: json!({"model": "glm"}),
        expected_effective_config: serde_json::to_value(&response.config).unwrap(),
    };

    let mut profile = base.clone();
    profile["layers"][0]["name"]["profile"] = json!("unsafe");
    let profile: ConfigReadResponse = serde_json::from_value(profile).unwrap();
    assert!(
        audit_config(
            &profile,
            &ConfigRequirementsReadResponse { requirements: None },
            &expectation
        )
        .unwrap_err()
        .to_string()
        .contains("profile")
    );

    let mut extra = base.clone();
    extra["layers"].as_array_mut().unwrap().push(json!({
        "name": {"type": "sessionFlags"},
        "version": "v2",
        "config": {}
    }));
    let extra: ConfigReadResponse = serde_json::from_value(extra).unwrap();
    assert!(
        audit_config(
            &extra,
            &ConfigRequirementsReadResponse { requirements: None },
            &expectation
        )
        .is_err()
    );

    let requirements: ConfigRequirementsReadResponse =
        serde_json::from_value(json!({"requirements": {}})).unwrap();
    assert!(
        audit_config(&response, &requirements, &expectation)
            .unwrap_err()
            .to_string()
            .contains("requirements")
    );

    let mut instruction_drift = base;
    instruction_drift["config"]["developer_instructions"] = json!("unsafe drift");
    let instruction_drift: ConfigReadResponse = serde_json::from_value(instruction_drift).unwrap();
    assert!(
        audit_config(
            &instruction_drift,
            &ConfigRequirementsReadResponse { requirements: None },
            &expectation,
        )
        .unwrap_err()
        .to_string()
        .contains("effective config")
    );

    let secret_path = temp.path().join("secret.toml");
    fs::write(&secret_path, b"env_key = \"SECRET\"\n").unwrap();
    let secret_expectation = ConfigAuditExpectation {
        canonical_config_path: secret_path.canonicalize().unwrap(),
        expected_config_bytes: b"env_key = \"SECRET\"\n".to_vec(),
        expected_layer_config: json!({"env_key": "SECRET"}),
        expected_effective_config: json!({}),
    };
    assert!(
        audit_config(
            &response,
            &ConfigRequirementsReadResponse { requirements: None },
            &secret_expectation
        )
        .unwrap_err()
        .to_string()
        .contains("forbidden")
    );

    fs::write(temp.path().join("auth.json"), b"{}").unwrap();
    assert!(
        audit_config(
            &response,
            &ConfigRequirementsReadResponse { requirements: None },
            &expectation
        )
        .unwrap_err()
        .to_string()
        .contains("auth.json")
    );
}

#[test]
fn json_line_client_routes_typed_notifications_and_response_ids() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;

        let (client_io, server_io) = tokio::io::duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, mut server_write) = tokio::io::split(server_io);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(server_read);
            let mut request = String::new();
            reader.read_line(&mut request).await.unwrap();
            let request: serde_json::Value = serde_json::from_str(&request).unwrap();
            assert_eq!(request["id"], json!(1));
            assert_eq!(request["method"], json!("synthetic/request"));
            server_write
                .write_all(b"{\"method\":\"rawResponse/completed\",\"params\":{\"threadId\":\"root-thread\",\"turnId\":\"root-turn\",\"responseId\":\"resp-1\",\"usage\":{\"totalTokens\":2,\"inputTokens\":1,\"cachedInputTokens\":0,\"cacheWriteInputTokens\":0,\"outputTokens\":1,\"reasoningOutputTokens\":0}}}\n")
                .await
                .unwrap();
            server_write
                .write_all(b"{\"id\":1,\"result\":{\"ok\":true}}\n")
                .await
                .unwrap();
        });
        let mut client = JsonLineClient::new(client_read, client_write);
        let response: serde_json::Value = client
            .request(
                "synthetic/request",
                Some(&json!({"typed": true})),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        assert_eq!(response, json!({"ok": true}));
        assert_eq!(client.take_notifications().len(), 1);
        assert!(!client.is_poisoned());
        server.await.unwrap();
    });
}

#[test]
fn json_line_client_replies_method_not_found_then_poisons_unknown_server_requests() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;

        let (client_io, server_io) = tokio::io::duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, mut server_write) = tokio::io::split(server_io);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(server_read);
            let mut initial_request = String::new();
            reader.read_line(&mut initial_request).await.unwrap();
            server_write
                .write_all(b"{\"id\":99,\"method\":\"server/unknown\",\"params\":{}}\n")
                .await
                .unwrap();
            let mut rejection = String::new();
            reader.read_line(&mut rejection).await.unwrap();
            let rejection: serde_json::Value = serde_json::from_str(&rejection).unwrap();
            assert_eq!(rejection["id"], json!(99));
            assert_eq!(rejection["error"]["code"], json!(-32601));
        });
        let mut client = JsonLineClient::new(client_read, client_write);
        let error = client
            .request::<_, serde_json::Value>(
                "synthetic/request",
                Some(&json!({})),
                Duration::from_secs(1),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("unknown App Server request"));
        assert!(client.is_poisoned());
        server.await.unwrap();
    });
}

#[test]
fn json_line_client_fails_closed_on_timeout_wrong_id_invalid_json_and_eof() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;

        async fn run_case(server_reply: Option<&'static [u8]>, expected: &str) {
            let (client_io, server_io) = tokio::io::duplex(4096);
            let (client_read, client_write) = tokio::io::split(client_io);
            let (server_read, mut server_write) = tokio::io::split(server_io);
            let server = tokio::spawn(async move {
                let mut reader = BufReader::new(server_read);
                let mut request = String::new();
                reader.read_line(&mut request).await.unwrap();
                if let Some(reply) = server_reply {
                    server_write.write_all(reply).await.unwrap();
                }
            });
            let mut client = JsonLineClient::new(client_read, client_write);
            let error = client
                .request::<_, serde_json::Value>(
                    "synthetic/request",
                    Some(&json!({})),
                    Duration::from_millis(50),
                )
                .await
                .unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "unexpected error: {error:#}"
            );
            assert!(client.is_poisoned());
            server.await.unwrap();
        }

        run_case(Some(b"{\"id\":2,\"result\":{}}\n"), "response ID").await;
        run_case(Some(b"not-json\n"), "parse App Server JSON").await;
        run_case(None, "EOF").await;

        let (client_io, server_io) = tokio::io::duplex(4096);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (server_read, _server_write) = tokio::io::split(server_io);
        let server = tokio::spawn(async move {
            let mut reader = BufReader::new(server_read);
            let mut request = String::new();
            reader.read_line(&mut request).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        let mut client = JsonLineClient::new(client_read, client_write);
        let error = client
            .request::<_, serde_json::Value>(
                "synthetic/request",
                Some(&json!({})),
                Duration::from_millis(20),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(client.is_poisoned());
        server.await.unwrap();
    });
}
