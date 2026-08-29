use std::io::BufWriter;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
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

fn loopback_broker_port(base_url: &str) -> u16 {
    assert!(
        !base_url.bytes().any(|byte| byte.is_ascii_control()),
        "loopback broker URL contains an ASCII control character: {base_url:?}"
    );
    let port = base_url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|value| value.strip_suffix("/v1"))
        .unwrap_or_else(|| {
            panic!("broker URL must use exact loopback form http://127.0.0.1:<u16>/v1: {base_url}")
        });
    assert!(
        !port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit()),
        "loopback broker URL has an invalid port: {base_url}"
    );
    port.parse()
        .unwrap_or_else(|_| panic!("loopback broker URL has an invalid port: {base_url}"))
}

fn assert_header_value(value: &str) {
    assert!(
        !value.bytes().any(|byte| byte.is_ascii_control()),
        "broker header value contains an ASCII control character"
    );
}

fn response_header_body(response: &[u8]) -> (&[u8], &[u8]) {
    let header_end = response
        .windows(4)
        .position(|bytes| bytes == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("broker response has no HTTP header terminator"));
    (&response[..header_end], &response[header_end + 4..])
}

fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn trim_ows(mut value: &[u8]) -> &[u8] {
    while matches!(value.first(), Some(b' ' | b'\t')) {
        value = &value[1..];
    }
    while matches!(value.last(), Some(b' ' | b'\t')) {
        value = &value[..value.len() - 1];
    }
    value
}

fn valid_field_value(value: &[u8]) -> bool {
    value
        .iter()
        .all(|byte| *byte == b'\t' || (*byte >= b' ' && *byte != 0x7f))
}

fn assert_response_field(name: &[u8], value: &[u8], kind: &str) {
    assert!(
        !name.is_empty() && name.iter().copied().all(is_tchar),
        "broker response has an invalid {kind} field-name"
    );
    assert!(
        valid_field_value(value),
        "broker response has an invalid {kind} field value"
    );
}

fn valid_chunk_extension_value(value: &[u8]) -> bool {
    if !value.is_empty() && value.iter().copied().all(is_tchar) {
        return true;
    }
    if value.len() < 2 || value.first() != Some(&b'"') || value.last() != Some(&b'"') {
        return false;
    }
    let mut escaped = false;
    for byte in &value[1..value.len() - 1] {
        if escaped {
            if !valid_field_value(std::slice::from_ref(byte)) {
                return false;
            }
            escaped = false;
        } else if *byte == b'\\' {
            escaped = true;
        } else if *byte == b'"' || !valid_field_value(std::slice::from_ref(byte)) {
            return false;
        }
    }
    !escaped
}

fn assert_chunk_extension(extension: &[u8]) {
    let (name, value) = match extension.iter().position(|byte| *byte == b'=') {
        Some(equals) => (
            trim_ows(&extension[..equals]),
            Some(trim_ows(&extension[equals + 1..])),
        ),
        None => (trim_ows(extension), None),
    };
    assert!(
        !name.is_empty() && name.iter().copied().all(is_tchar),
        "chunked broker response has an invalid extension name"
    );
    if let Some(value) = value {
        assert!(
            valid_chunk_extension_value(value),
            "chunked broker response has an invalid extension value"
        );
    }
}

fn split_chunk_extensions(line: &[u8]) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in line.iter().copied().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && byte == b';' {
            parts.push(&line[start..index]);
            start = index + 1;
        }
    }
    assert!(
        !quoted && !escaped,
        "chunked broker response has an unterminated quoted extension"
    );
    parts.push(&line[start..]);
    parts
}

fn decode_chunked_body(mut encoded: &[u8]) -> Vec<u8> {
    fn line(input: &mut &[u8]) -> Vec<u8> {
        let end = input
            .windows(2)
            .position(|bytes| bytes == b"\r\n")
            .unwrap_or_else(|| panic!("chunked broker response has an unterminated line"));
        let line = input[..end].to_vec();
        *input = &input[end + 2..];
        line
    }

    let mut decoded = Vec::new();
    loop {
        let size_line = line(&mut encoded);
        let mut size_parts = split_chunk_extensions(&size_line).into_iter();
        let mut size_token = size_parts
            .next()
            .expect("chunk size line always has one token");
        if size_parts.len() > 0 {
            while matches!(size_token.last(), Some(b' ' | b'\t')) {
                size_token = &size_token[..size_token.len() - 1];
            }
        }
        assert!(
            !size_token.is_empty() && size_token.iter().all(u8::is_ascii_hexdigit),
            "chunked broker response has an invalid chunk size"
        );
        size_parts.for_each(assert_chunk_extension);
        let size = usize::from_str_radix(std::str::from_utf8(size_token).unwrap(), 16)
            .unwrap_or_else(|_| panic!("chunked broker response chunk size overflowed"));
        if size == 0 {
            loop {
                let trailer = line(&mut encoded);
                if trailer.is_empty() {
                    assert!(
                        encoded.is_empty(),
                        "chunked broker response has bytes after its trailers"
                    );
                    return decoded;
                }
                let colon = trailer
                    .iter()
                    .position(|byte| *byte == b':')
                    .unwrap_or_else(|| panic!("chunked broker response has a malformed trailer"));
                let name = &trailer[..colon];
                let value = &trailer[colon + 1..];
                assert_response_field(name, value, "trailer");
            }
        }
        let chunk_end = size
            .checked_add(2)
            .unwrap_or_else(|| panic!("chunked broker response chunk size overflowed"));
        assert!(
            encoded.len() >= chunk_end && &encoded[size..chunk_end] == b"\r\n",
            "chunked broker response has a truncated chunk"
        );
        decoded.extend_from_slice(&encoded[..size]);
        encoded = &encoded[chunk_end..];
    }
}

fn decode_broker_response(response: &[u8]) -> Vec<u8> {
    let (head, body) = response_header_body(response);
    let head = std::str::from_utf8(head)
        .unwrap_or_else(|_| panic!("broker response headers are not UTF-8"));
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    let mut status = status_line.split_ascii_whitespace();
    let version = status.next().unwrap_or("");
    let code = status.next().unwrap_or("");
    assert!(
        matches!(version, "HTTP/1.0" | "HTTP/1.1")
            && code.len() == 3
            && code.bytes().all(|byte| byte.is_ascii_digit()),
        "broker response has an invalid final HTTP status line: {status_line}"
    );
    let code: u16 = code.parse().unwrap();
    assert!(
        (200..300).contains(&code),
        "broker response status is not successful: {status_line}"
    );

    let mut content_lengths = Vec::new();
    let mut transfer_encodings = Vec::new();
    for header in lines {
        let (name, value) = header
            .split_once(':')
            .unwrap_or_else(|| panic!("broker response has a malformed header"));
        assert_response_field(name.as_bytes(), value.as_bytes(), "header");
        let value = trim_ows(value.as_bytes());
        if name.eq_ignore_ascii_case("content-length") {
            content_lengths.push(value);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            transfer_encodings.push(value);
        }
    }
    assert!(
        !(content_lengths.is_empty() && transfer_encodings.is_empty()),
        "broker response has no supported body framing"
    );
    assert!(
        content_lengths.is_empty() || transfer_encodings.is_empty(),
        "broker response has conflicting Content-Length and Transfer-Encoding"
    );
    if !content_lengths.is_empty() {
        assert!(
            content_lengths.len() == 1,
            "broker response has ambiguous Content-Length framing"
        );
        let length = content_lengths[0];
        assert!(
            !length.is_empty() && length.iter().all(u8::is_ascii_digit),
            "broker response has an invalid Content-Length"
        );
        let length: usize = std::str::from_utf8(length)
            .unwrap()
            .parse()
            .unwrap_or_else(|_| panic!("broker response has an invalid Content-Length"));
        assert!(
            body.len() == length,
            "broker response body does not match Content-Length"
        );
        body.to_vec()
    } else {
        assert!(
            transfer_encodings.len() == 1 && transfer_encodings[0].eq_ignore_ascii_case(b"chunked"),
            "broker response has unsupported Transfer-Encoding framing"
        );
        decode_chunked_body(body)
    }
}

fn post_broker(base_url: &str, thread_id: &str, parent_id: Option<&str>, body: &[u8]) -> Vec<u8> {
    let port = loopback_broker_port(base_url);
    let window_id = format!("{thread_id}:0");
    assert_header_value(&window_id);
    if let Some(parent_id) = parent_id {
        assert_header_value(parent_id);
    }
    let mut request = format!(
        concat!(
            "POST /v1/responses HTTP/1.1\r\n",
            "Host: 127.0.0.1:{}\r\n",
            "content-type: application/json\r\n",
            "Content-Length: {}\r\n",
            "Connection: close\r\n",
            "x-codex-window-id: {}\r\n"
        ),
        port,
        body.len(),
        window_id
    );
    if let Some(parent_id) = parent_id {
        request.push_str(&format!(
            "x-codex-parent-thread-id: {parent_id}\r\nx-openai-subagent: collab_spawn\r\n"
        ));
    }
    request.push_str("\r\n");
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    decode_broker_response(&response)
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
    let body = serde_json::to_vec(&body).unwrap();
    let response = post_broker(base_url, thread_id, parent_id, &body);
    let response = String::from_utf8(response).unwrap();
    let completed: serde_json::Value = response
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
