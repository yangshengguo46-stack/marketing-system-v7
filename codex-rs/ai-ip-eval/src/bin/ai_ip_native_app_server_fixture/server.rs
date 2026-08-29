use std::fs;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde_json::json;

use crate::wire;

pub(crate) fn run() -> anyhow::Result<()> {
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
    let mut effective_config = config.clone();
    effective_config["allow_login_shell"] = json!(true);
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
    let late_generic_target_skill_read = !candidate
        && eval_root
            .join("generic-target-skill-read-quiet-window")
            .is_file();
    let mut late_generic_target_skill_read_scheduled = false;
    let mut root = wire::thread(thread_id, None, None, candidate, &eval_root);
    let mut child = wire::thread(
        child_id,
        Some(thread_id),
        Some("subagent"),
        candidate,
        &eval_root,
    );
    let stdin = BufReader::new(std::io::stdin().lock());
    let output = wire::stdout();

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
                "config": effective_config,
                "origins": {},
                "layers": [{
                    "name": {"type": "user", "file": codex_home.join("config.toml"), "profile": null},
                    "version": "v1",
                    "config": config
                }]
            }),
            Some("configRequirements/read") => json!({"requirements": null}),
            Some("skills/list") => {
                assert_eq!(message["params"]["forceReload"], json!(true));
                let cwd = std::path::PathBuf::from(message["params"]["cwds"][0].as_str().unwrap());
                let skills = if candidate {
                    vec![json!({
                        "name": codex_ai_ip_runtime::LEAD_SKILL_NAME,
                        "description": "Synthetic native Lead Skill",
                        "path": codex_home
                            .join("skills")
                            .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                            .join("SKILL.md"),
                        "scope": "user",
                        "enabled": true
                    })]
                } else {
                    Vec::new()
                };
                json!({"data": [{"cwd": cwd, "skills": skills, "errors": []}]})
            }
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
                let package = if candidate {
                    let mut package: serde_json::Value =
                        serde_json::from_str(&wire::package_json()).unwrap();
                    package["publishableContent"]["title"] = json!("Refined synthetic title");
                    package["publishableContent"]["body"] = json!("Refined synthetic body");
                    package.to_string()
                } else {
                    wire::package_json()
                };
                let final_item = json!({
                    "type": "agentMessage",
                    "id": "final-package",
                    "text": package
                });
                let generic_target_skill_read =
                    !candidate && eval_root.join("generic-target-skill-read").is_file();
                let skill_item = (candidate || generic_target_skill_read).then(|| {
                    let skill_path = codex_home
                        .join("skills")
                        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                        .join("SKILL.md");
                    let skill_bytes_path = if candidate {
                        skill_path.clone()
                    } else {
                        eval_root
                            .join("candidate-home/.codex/skills")
                            .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                            .join("SKILL.md")
                    };
                    json!({
                        "type": "commandExecution",
                        "id": "lead-skill-read",
                        "pluginId": null,
                        "scriptPath": null,
                        "command": format!("cat {}", skill_path.display()),
                        "cwd": eval_root,
                        "processId": null,
                        "status": "completed",
                        "commandActions": [],
                        "aggregatedOutput": fs::read_to_string(&skill_bytes_path).unwrap(),
                        "exitCode": 0,
                        "durationMs": 1
                    })
                });
                wire::send(
                    &output,
                    &json!({"id": request_id, "result": {"turn": wire::turn(turn_id, "inProgress")}}),
                );
                wire::send(
                    &output,
                    &json!({"method": "turn/started", "params": {"threadId": thread_id, "turn": wire::turn(turn_id, "inProgress")}}),
                );
                wire::send(
                    &output,
                    &json!({"method": "thread/started", "params": {"thread": child}}),
                );
                wire::send(
                    &output,
                    &json!({"method": "turn/started", "params": {"threadId": child_id, "turn": wire::turn(&child_turn_id, "inProgress")}}),
                );
                std::thread::sleep(Duration::from_millis(200));
                if let Some(skill_item) = skill_item.as_ref() {
                    wire::send(
                        &output,
                        &json!({
                            "method": "item/completed",
                            "params": {
                                "threadId": thread_id,
                                "turnId": turn_id,
                                "completedAtMs": 1787616000000_i64,
                                "item": skill_item
                            }
                        }),
                    );
                }
                wire::call_broker(
                    &output,
                    &config,
                    thread_id,
                    turn_id,
                    None,
                    candidate,
                    &home,
                    &codex_home,
                );
                wire::call_broker(
                    &output,
                    &config,
                    child_id,
                    &child_turn_id,
                    Some(thread_id),
                    candidate,
                    &home,
                    &codex_home,
                );
                wire::send(
                    &output,
                    &json!({"method": "turn/completed", "params": {"threadId": child_id, "turn": wire::turn(&child_turn_id, "completed")}}),
                );
                wire::send(
                    &output,
                    &json!({
                        "method": "item/completed",
                        "params": {
                            "threadId": thread_id,
                            "turnId": turn_id,
                            "completedAtMs": 1787616000001_i64,
                            "item": final_item.clone()
                        }
                    }),
                );
                let mut root_items = Vec::new();
                if let Some(skill_item) = skill_item {
                    root_items.push(skill_item);
                }
                root_items.push(final_item);
                wire::send(
                    &output,
                    &json!({"method": "turn/completed", "params": {"threadId": thread_id, "turn": wire::completed_turn(turn_id, root_items.clone())}}),
                );
                child["turns"] = json!([wire::turn(&child_turn_id, "completed")]);
                root["turns"] = json!([wire::completed_turn(turn_id, root_items)]);
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
            Some("thread/read") => {
                let requested_thread_id = message["params"]["threadId"].as_str().unwrap();
                if late_generic_target_skill_read
                    && !late_generic_target_skill_read_scheduled
                    && requested_thread_id == child_id
                {
                    late_generic_target_skill_read_scheduled = true;
                    let late_output = output.clone();
                    let skill_path = codex_home
                        .join("skills")
                        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                        .join("SKILL.md");
                    let skill_bytes_path = eval_root
                        .join("candidate-home/.codex/skills")
                        .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                        .join("SKILL.md");
                    let skill_bytes = fs::read_to_string(skill_bytes_path).unwrap();
                    let root_thread_id = thread_id.to_string();
                    let late_cwd = eval_root.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(250));
                        wire::send(
                            &late_output,
                            &json!({
                                "method": "item/completed",
                                "params": {
                                    "threadId": root_thread_id,
                                    "turnId": "turn-generic",
                                    "completedAtMs": 1787616000002_i64,
                                    "item": {
                                        "type": "commandExecution",
                                        "id": "late-lead-skill-read",
                                        "pluginId": null,
                                        "scriptPath": null,
                                        "command": format!("cat {}", skill_path.display()),
                                        "cwd": late_cwd,
                                        "processId": null,
                                        "status": "completed",
                                        "commandActions": [],
                                        "aggregatedOutput": skill_bytes,
                                        "exitCode": 0,
                                        "durationMs": 1
                                    }
                                }
                            }),
                        );
                    });
                }
                json!({
                    "thread": if requested_thread_id == thread_id { &root } else { &child }
                })
            }
            _ => {
                wire::send(
                    &output,
                    &json!({"id": request_id, "error": {"code": -32601, "message": "unsupported"}}),
                );
                continue;
            }
        };
        wire::send(&output, &json!({"id": request_id, "result": result}));
    }
    Ok(())
}
