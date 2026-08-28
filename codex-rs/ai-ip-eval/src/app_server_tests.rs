use std::fs;
use std::time::Duration;
use std::time::Instant;

use codex_app_server_protocol::AdditionalContextKind;
use codex_app_server_protocol::AskForApproval;
use codex_app_server_protocol::ConfigReadResponse;
use codex_app_server_protocol::ConfigRequirementsReadResponse;
use pretty_assertions::assert_eq;
use serde_json::json;
use sha2::Digest;
use sha2::Sha256;

use crate::AppServerClient;
use crate::ChildEnvironment;
use crate::ConfigAuditExpectation;
use crate::JsonLineClient;
use crate::app_server::VerifiedExecutable;
use crate::audit_config;
use crate::audit_frozen_config;
use crate::build_shared_config;
use crate::build_thread_start;
use crate::build_turn_start;
use crate::config_read_params;
use crate::initialize_params;
use crate::tests::mission_case;

#[cfg(target_os = "macos")]
#[test]
fn verified_launch_executes_retained_descriptor_after_pathname_replacement() {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("codex-under-test");
    fs::copy("/usr/bin/true", &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let retained = VerifiedExecutable::from_retained(
        fs::File::open(&executable).unwrap(),
        &executable,
        format!("{:x}", Sha256::digest(fs::read(&executable).unwrap())),
    )
    .unwrap();
    fs::rename(&executable, temp.path().join("verified-image")).unwrap();
    fs::copy("/usr/bin/false", &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let home = temp.path().to_str().unwrap();
    let environment = ChildEnvironment::from_environment(
        &BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]),
        home,
        home,
        home,
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let client = AppServerClient::spawn_verified(
            &executable,
            retained,
            &temp.path().join("stderr.log"),
            &environment,
        )
        .await
        .unwrap();
        assert!(
            client
                .close(Duration::from_secs(10))
                .await
                .unwrap()
                .success()
        );
    });
}

#[cfg(target_os = "macos")]
#[test]
fn verified_native_launch_uses_a_private_immutable_descriptor_copy() {
    use std::collections::BTreeMap;
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("native-under-test");
    fs::copy("/usr/bin/true", &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let retained = VerifiedExecutable::from_retained(
        fs::File::open(&executable).unwrap(),
        &executable,
        format!("{:x}", Sha256::digest(fs::read(&executable).unwrap())),
    )
    .unwrap();
    fs::rename(&executable, temp.path().join("verified-native-image")).unwrap();
    fs::copy("/usr/bin/false", &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let home = temp.path().to_str().unwrap();
    let environment = ChildEnvironment::from_environment(
        &BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]),
        home,
        home,
        home,
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let client = AppServerClient::spawn_verified(
            &executable,
            retained,
            &temp.path().join("native-stderr.log"),
            &environment,
        )
        .await
        .unwrap();
        assert!(
            client
                .close(Duration::from_secs(10))
                .await
                .unwrap()
                .success()
        );
    });
}

#[cfg(target_os = "macos")]
#[test]
fn suspended_spawn_rejects_private_image_clear_replace_restore_before_resume() {
    use std::collections::BTreeMap;
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("native-under-attack");
    fs::copy("/usr/bin/true", &executable).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let retained = VerifiedExecutable::from_retained(
        fs::File::open(&executable).unwrap(),
        &executable,
        format!("{:x}", Sha256::digest(fs::read(&executable).unwrap())),
    )
    .unwrap();
    let home = temp.path().to_str().unwrap();
    let environment = ChildEnvironment::from_environment(
        &BTreeMap::from([("PATH".to_string(), "/usr/bin:/bin".to_string())]),
        home,
        home,
        home,
    )
    .unwrap();

    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime.block_on(AppServerClient::spawn_verified_with_private_image_hook(
        &executable,
        retained,
        &temp.path().join("attacked-stderr.log"),
        &environment,
        |image, directory| {
            let image = image.to_path_buf();
            let directory = directory.to_path_buf();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let image_c = CString::new(image.as_os_str().as_bytes())?;
                let directory_c = CString::new(directory.as_os_str().as_bytes())?;
                if unsafe { libc::chflags(directory_c.as_ptr(), 0) } != 0
                    || unsafe { libc::chflags(image_c.as_ptr(), 0) } != 0
                {
                    return Err(std::io::Error::last_os_error().into());
                }
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
                let retained_path = directory.join("retained-original");
                fs::rename(&image, &retained_path)?;
                fs::copy("/usr/bin/false", &image)?;
                fs::set_permissions(&image, fs::Permissions::from_mode(0o500))?;
                fs::remove_file(&image)?;
                fs::rename(&retained_path, &image)?;
                Ok(())
            })
            .join()
            .map_err(|_| anyhow::anyhow!("private-image attacker panicked"))??;
            Ok(())
        },
    ));

    assert!(result.is_err());
}

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
fn production_config_composition_rejects_injected_builtin_effective_drift() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.toml");
    let config_bytes = b"model = \"glm-approved\"\n".to_vec();
    fs::write(&config_path, &config_bytes).unwrap();
    let canonical = config_path.canonicalize().unwrap();
    let layer = json!({"model": "glm-approved"});
    let response: ConfigReadResponse = serde_json::from_value(json!({
        "config": {
            "model": "glm-approved",
            "allow_login_shell": false
        },
        "origins": {},
        "layers": [{
            "name": {"type": "user", "file": canonical, "profile": null},
            "version": "v1",
            "config": layer
        }]
    }))
    .unwrap();

    let error = audit_frozen_config(
        &response,
        &ConfigRequirementsReadResponse { requirements: None },
        canonical,
        config_bytes,
        layer,
    )
    .unwrap_err();
    assert!(error.to_string().contains("effective config"));
}

#[test]
fn frozen_config_audit_evidence_is_equal_across_isolated_home_prefixes() {
    let temp = tempfile::tempdir().unwrap();
    let config_bytes = b"model = \"glm-approved\"\n".to_vec();
    let layer = json!({"model": "glm-approved"});
    let audit = |arm: &str| {
        let home = temp.path().join(arm);
        fs::create_dir(&home).unwrap();
        let config_path = home.join("config.toml");
        fs::write(&config_path, &config_bytes).unwrap();
        let canonical = config_path.canonicalize().unwrap();
        let response: ConfigReadResponse = serde_json::from_value(json!({
            "config": {"model": "glm-approved", "allow_login_shell": true},
            "origins": {},
            "layers": [{
                "name": {"type": "user", "file": canonical, "profile": null},
                "version": "v1",
                "config": layer
            }]
        }))
        .unwrap();
        audit_frozen_config(
            &response,
            &ConfigRequirementsReadResponse { requirements: None },
            canonical,
            config_bytes.clone(),
            layer.clone(),
        )
        .unwrap()
    };

    assert_eq!(audit("generic"), audit("candidate"));
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
fn json_line_client_observes_queued_and_streamed_turn_completion_exactly_once() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;
        use tokio::io::BufReader;

        const STATUS: &[u8] = b"{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"root-thread\",\"status\":{\"type\":\"idle\"}}}\n";
        const COMPLETED: &[u8] = b"{\"method\":\"turn/completed\",\"params\":{\"threadId\":\"root-thread\",\"turn\":{\"id\":\"root-turn\",\"items\":[],\"itemsView\":\"full\",\"status\":\"completed\",\"error\":null,\"startedAt\":null,\"completedAt\":null,\"durationMs\":null}}}\n";

        for queued in [true, false] {
            let (client_io, server_io) = tokio::io::duplex(16 * 1024);
            let (client_read, client_write) = tokio::io::split(client_io);
            let (server_read, mut server_write) = tokio::io::split(server_io);
            let server = tokio::spawn(async move {
                if queued {
                    let mut reader = BufReader::new(server_read);
                    let mut request = String::new();
                    reader.read_line(&mut request).await.unwrap();
                    server_write.write_all(STATUS).await.unwrap();
                    server_write.write_all(COMPLETED).await.unwrap();
                    server_write
                        .write_all(b"{\"id\":1,\"result\":{\"ok\":true}}\n")
                        .await
                        .unwrap();
                } else {
                    drop(server_read);
                    server_write.write_all(STATUS).await.unwrap();
                    server_write.write_all(COMPLETED).await.unwrap();
                }
            });
            let mut client = JsonLineClient::new(client_read, client_write);
            if queued {
                let _: serde_json::Value = client
                    .request(
                        "synthetic/request",
                        Some(&json!({})),
                        Duration::from_secs(1),
                    )
                    .await
                    .unwrap();
            }
            let mut observed = Vec::new();
            client
                .wait_for_turn_completion(
                    "root-thread",
                    "root-turn",
                    Duration::from_secs(1),
                    |notification| {
                        observed.push(match notification {
                            codex_app_server_protocol::ServerNotification::ThreadStatusChanged(
                                _,
                            ) => "status",
                            codex_app_server_protocol::ServerNotification::TurnCompleted(_) => {
                                "completed"
                            }
                            other => panic!("unexpected notification: {other:?}"),
                        });
                        Ok(())
                    },
                )
                .await
                .unwrap();
            assert_eq!(observed, vec!["status", "completed"]);
            server.await.unwrap();
        }
    });
}

#[test]
fn json_line_client_drains_queued_notifications_and_restarts_the_quiet_window() {
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
            server_write
                .write_all(b"{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"root-thread\",\"status\":{\"type\":\"idle\"}}}\n")
                .await
                .unwrap();
            server_write
                .write_all(b"{\"id\":1,\"result\":{\"ok\":true}}\n")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(40)).await;
            server_write
                .write_all(b"{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"root-thread\",\"status\":{\"type\":\"idle\"}}}\n")
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(150)).await;
        });
        let mut client = JsonLineClient::new(client_read, client_write);
        let _: serde_json::Value = client
            .request(
                "synthetic/request",
                Some(&json!({})),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        let started = Instant::now();
        let mut observed = 0_u8;
        client
            .observe_until_quiet(
                Duration::from_millis(80),
                Instant::now() + Duration::from_secs(1),
                |notification| {
                    assert!(matches!(
                        notification,
                        codex_app_server_protocol::ServerNotification::ThreadStatusChanged(_)
                    ));
                    observed += 1;
                    Ok(true)
                },
            )
            .await
            .unwrap();

        assert_eq!(observed, 2);
        assert!(started.elapsed() >= Duration::from_millis(110));
        assert!(!client.is_poisoned());
        server.await.unwrap();
    });
}

#[test]
fn json_line_client_observes_notifications_emitted_after_stdin_shutdown() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        use tokio::io::AsyncReadExt;
        use tokio::io::AsyncWriteExt;

        let (client_io, server_io) = tokio::io::duplex(16 * 1024);
        let (client_read, client_write) = tokio::io::split(client_io);
        let (mut server_read, mut server_write) = tokio::io::split(server_io);
        let server = tokio::spawn(async move {
            let mut discarded = Vec::new();
            server_read.read_to_end(&mut discarded).await.unwrap();
            server_write
                .write_all(b"{\"method\":\"thread/status/changed\",\"params\":{\"threadId\":\"root-thread\",\"status\":{\"type\":\"idle\"}}}\n")
                .await
                .unwrap();
        });
        let client = JsonLineClient::new(client_read, client_write);
        let mut observed = 0_u8;
        client
            .shutdown_and_observe_until_eof(
                Instant::now() + Duration::from_secs(1),
                |notification| {
                    assert!(matches!(
                        notification,
                        codex_app_server_protocol::ServerNotification::ThreadStatusChanged(_)
                    ));
                    observed += 1;
                    Ok(())
                },
            )
            .await
            .unwrap();
        assert_eq!(observed, 1);
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
