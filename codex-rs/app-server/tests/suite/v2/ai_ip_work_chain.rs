use anyhow::Result;
use app_test_support::MockResponsesConfig;
use app_test_support::TestAppServer;
use app_test_support::create_command_execution_sse_response;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::TurnCompletedNotification;
use codex_app_server_protocol::TurnStartParams;
use codex_app_server_protocol::TurnStartResponse;
use codex_app_server_protocol::UserInput;
use core_test_support::TestTargetOs;
use core_test_support::responses;
use core_test_support::skip_if_wine_exec;
use core_test_support::test_target_os;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test]
async fn marketing_work_persists_reopens_and_exposes_rework_to_lead() -> Result<()> {
    skip_if_wine_exec!(
        Ok(()),
        "Native ToolEnvironment currently omits foreign-platform cwd values"
    );
    let server = responses::start_mock_server().await;
    let research = format!(
        "访谈：新人担心培训是否兑现。{}RESEARCH_FULL_BODY_MARKER",
        "访谈观察与待核实部分。".repeat(20)
    );
    let steps = [
        json!({"action":"open","workId":"guild","brief":"公会吸引达人加入","materials":["interview.txt"]}),
        json!({"action":"record","workId":"guild","stage":"research","body":research}),
        json!({"action":"record","workId":"guild","stage":"direction","body":"展示真实培训过程，说明适合谁"}),
        json!({"action":"record","workId":"guild","stage":"draft","body":"完整口播；实拍培训；克制音乐"}),
        json!({"action":"record","workId":"guild","stage":"research","body":"补充：有经验达人更关注结算透明"}),
    ];
    let mut events = Vec::new();
    for (index, arguments) in steps.iter().enumerate() {
        events.push(responses::sse(vec![
            responses::ev_function_call(
                &format!("work-{index}"),
                "marketing_work",
                &arguments.to_string(),
            ),
            responses::ev_completed(&format!("response-{index}")),
        ]));
        if let Some((file, call_id)) = match index {
            0 => Some(("interview.txt", "read-material")),
            1 => Some(("output/marketing-work/guild.json", "read-research")),
            _ => None,
        } {
            let mut command: Vec<String> = match test_target_os() {
                TestTargetOs::Windows => {
                    vec!["cmd.exe".into(), "/d".into(), "/c".into(), "type".into()]
                }
                TestTargetOs::Linux | TestTargetOs::MacOs => vec!["cat".into()],
            };
            command.push(file.into());
            events.push(create_command_execution_sse_response(
                command,
                /*workdir*/ None,
                Some(5000),
                call_id,
            )?);
        }
    }
    events.push(responses::sse(vec![
        // Live providers may send sparse added items before their first delta.
        json!({"type":"response.output_item.added", "item":{"type":"reasoning", "id":"rs-sparse", "status":"in_progress"}}),
        json!({"type":"response.reasoning_summary_part.added", "item_id":"rs-sparse", "summary_index":0, "part":{"type":"summary_text", "text":""}}),
        json!({"type":"response.output_item.done", "item":{"type":"reasoning", "id":"rs-sparse", "summary":[]}}),
        json!({"type":"response.output_item.added", "item":{"type":"message", "id":"done", "role":"assistant", "status":"in_progress"}}),
        json!({"type":"response.output_text.delta", "item_id":"done", "content_index":0, "delta":"Saved"}),
        responses::ev_assistant_message("done", "Saved"),
        responses::ev_completed("done"),
    ]));
    events.push(responses::sse(vec![
        responses::ev_function_call(
            "reopen",
            "marketing_work",
            &json!({"action":"open","workId":"guild"}).to_string(),
        ),
        responses::ev_completed("reopen"),
    ]));
    events.push(responses::sse(vec![
        responses::ev_assistant_message("resumed", "Resumed"),
        responses::ev_completed("resumed"),
    ]));
    let mock = responses::mount_sse_sequence(&server, events).await;
    let codex_home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .with_sandbox_mode("workspace-write")
        .write(codex_home.path())?;
    // Cold native app-server startup on macOS/Windows can exceed the fixture's 10s default.
    let mut app = TestAppServer::builder()
        .with_codex_home(codex_home.path())
        .build_initialized_with_timeout(std::time::Duration::from_secs(60))
        .await?;
    let env = app.auto_env()?;
    env.environment()
        .get_filesystem()
        .write_file(
            &env.selection().cwd.join("interview.txt")?,
            format!(
                "{}MATERIAL_FULL_BODY_MARKER",
                "访谈材料：达人关心培训与结算。".repeat(20)
            )
            .into_bytes(),
            Default::default(),
            /*sandbox*/ None,
        )
        .await?;
    // A second native thread has no first-thread conversation or in-memory work state.
    for task in [
        "Complete this marketing mission",
        "Reopen the saved marketing mission",
    ] {
        let thread = app.start_thread(ThreadStartParams::default()).await?.thread;
        let _: TurnStartResponse = app
            .request(
                |request_id| codex_app_server_protocol::ClientRequest::TurnStart {
                    request_id,
                    params: TurnStartParams {
                        thread_id: thread.id,
                        input: vec![UserInput::Text {
                            text: task.into(),
                            text_elements: vec![],
                        }],
                        ..Default::default()
                    },
                },
            )
            .await?;
        timeout(
            std::time::Duration::from_secs(60),
            app.read_notification::<TurnCompletedNotification>("turn/completed"),
        )
        .await??;
    }
    let requests = mock.requests();
    assert_eq!(requests.len(), 10);
    assert!(
        requests[2]
            .function_call_output_text("read-material")
            .unwrap()
            .contains("MATERIAL_FULL_BODY_MARKER")
    );
    assert!(
        requests[4]
            .function_call_output_text("read-research")
            .unwrap()
            .contains("RESEARCH_FULL_BODY_MARKER")
    );
    let results: Vec<Value> = [1, 3, 5, 6, 7]
        .into_iter()
        .enumerate()
        .map(|(index, request_index)| {
            serde_json::from_str(
                &requests[request_index]
                    .function_call_output_text(&format!("work-{index}"))
                    .unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        results
            .iter()
            .map(|result| result["nextStage"].clone())
            .collect::<Vec<_>>(),
        vec![
            json!("research"),
            json!("direction"),
            json!("draft"),
            Value::Null,
            json!("direction")
        ]
    );
    let reopened: Value =
        serde_json::from_str(&requests[9].function_call_output_text("reopen").unwrap())?;
    assert_eq!(reopened, results[4]);
    assert_eq!(
        reopened["results"][2],
        json!({"preview":"完整口播；实拍培训；克制音乐", "needsReview":true})
    );
    assert_eq!(
        results[1]["results"][0]["preview"],
        &research[..research.floor_char_boundary(90)]
    );
    assert_eq!(reopened["workFile"], "output/marketing-work/guild.json");
    assert!(reopened.to_string().len() <= 900);
    let env = app.auto_env()?;
    let saved: Value = serde_json::from_slice(
        &env.environment()
            .get_filesystem()
            .read_file(
                &env.selection()
                    .cwd
                    .join("output/marketing-work/guild.json")?,
                Default::default(),
                /*sandbox*/ None,
            )
            .await?,
    )?;
    assert_eq!(
        saved,
        json!({
            "brief":"公会吸引达人加入", "materials":["interview.txt"], "startAt":"research",
            "results":[
                {"body":"补充：有经验达人更关注结算透明","needsReview":false},
                {"body":"展示真实培训过程，说明适合谁","needsReview":true},
                {"body":"完整口播；实拍培训；克制音乐","needsReview":true}
            ]
        })
    );
    Ok(())
}
