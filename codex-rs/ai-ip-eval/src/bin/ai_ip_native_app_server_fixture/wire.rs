use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use serde_json::json;

pub(crate) type SharedOutput = Arc<Mutex<BufWriter<std::io::Stdout>>>;

pub(crate) fn stdout() -> SharedOutput {
    Arc::new(Mutex::new(BufWriter::new(std::io::stdout())))
}

pub(crate) fn send(output: &SharedOutput, value: &serde_json::Value) {
    let mut output = output.lock().unwrap();
    serde_json::to_writer(&mut *output, value).unwrap();
    output.write_all(b"\n").unwrap();
    output.flush().unwrap();
}

pub(crate) fn thread(
    id: &str,
    parent: Option<&str>,
    source: Option<&str>,
    candidate: bool,
    cwd: &Path,
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

pub(crate) fn turn(id: &str, status: &str) -> serde_json::Value {
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

pub(crate) fn completed_turn(id: &str, items: Vec<serde_json::Value>) -> serde_json::Value {
    let mut value = turn(id, "completed");
    value["items"] = json!(items);
    value
}

pub(crate) fn package_json() -> String {
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

fn target_skill_treatment(codex_home: &Path) -> serde_json::Value {
    json!({
        "role": "developer",
        "content": [{
            "type": "input_text",
            "text": "Use the exact native Skill named in treatment metadata."
        }],
        "metadata": {
            "treatmentKind": "nativeSkill",
            "skillName": codex_ai_ip_runtime::LEAD_SKILL_NAME,
            "skillPath": codex_home
                .join("skills")
                .join(codex_ai_ip_runtime::LEAD_SKILL_NAME)
                .join("SKILL.md")
        }
    })
}

pub(crate) fn call_broker(
    output: &SharedOutput,
    config: &serde_json::Value,
    thread_id: &str,
    turn_id: &str,
    parent_id: Option<&str>,
    candidate: bool,
    home: &Path,
    codex_home: &Path,
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
    let mut body = json!({
        "model": "local-mock",
        "instructions": "Return the frozen synthetic package.",
        "input": [{"role": "user", "content": [{"type": "input_text", "text": "frozen mission"}]}],
        "tools": [{"type": "function", "name": "read_file", "description": "Read one file", "parameters": {"type": "object"}}],
        "metadata": {
            "aiIpThreadId": thread_id,
            "aiIpHome": home,
            "aiIpCodexHome": codex_home
        }
    });
    if candidate {
        body["input"]
            .as_array_mut()
            .unwrap()
            .push(target_skill_treatment(codex_home));
    }
    let body = request
        .body(serde_json::to_vec(&body).unwrap())
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
