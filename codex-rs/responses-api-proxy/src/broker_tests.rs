use crate::Args;
use crate::ExchangeObserver;
use crate::ForwardErrorClass;
use crate::ForwardResult;
use crate::ObservedUsage;
use crate::ProxyConfig;
use crate::RequestGate;
use crate::RequestInspector;
use crate::RequestMetadata;
use crate::RequestPermit;
use crate::RequestTransformConfig;
use crate::ResponseCompletedMetadata;
use crate::TransformedRequestEvidence;
use crate::TransformedRequestMetadata;
use crate::activate;
use crate::bind;
use crate::read_api_key::locked_auth_header_for_test;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use pretty_assertions::assert_eq;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde_json::Value;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use tiny_http::Header;
use tiny_http::Response;
use tiny_http::Server;

const TEST_TIMEOUT: Duration = Duration::from_secs(/*secs*/ 3);

#[derive(Debug)]
struct CapturedRequest {
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl CapturedRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

#[derive(Default)]
struct RecordingInspector;

impl RequestInspector for RecordingInspector {
    fn inspect(&self, body: &Value) -> Result<TransformedRequestEvidence> {
        if body.get("max_output_tokens") != Some(&json!(321)) {
            bail!("transform did not inject the frozen output limit");
        }
        Ok(TransformedRequestEvidence {
            raw_sha256: [1; 32],
            normalized_sha256: [2; 32],
            normalized_base_commitment: [3; 32],
            treatment_diff_commitment: Some([4; 32]),
        })
    }
}

struct RecordingGate {
    reject: bool,
    permit_lifetime: Duration,
    requests: Mutex<Vec<RequestMetadata>>,
    transformed: Mutex<Vec<TransformedRequestMetadata>>,
    results: Mutex<Vec<ForwardResult>>,
}

impl RecordingGate {
    fn allow(permit_lifetime: Duration) -> Self {
        Self {
            reject: false,
            permit_lifetime,
            requests: Mutex::new(Vec::new()),
            transformed: Mutex::new(Vec::new()),
            results: Mutex::new(Vec::new()),
        }
    }

    fn reject() -> Self {
        Self {
            reject: true,
            ..Self::allow(TEST_TIMEOUT)
        }
    }
}

impl RequestGate for RecordingGate {
    fn before_forward(
        &self,
        request: &RequestMetadata,
        transformed: &TransformedRequestMetadata,
    ) -> Result<RequestPermit> {
        self.requests.lock().unwrap().push(request.clone());
        self.transformed.lock().unwrap().push(transformed.clone());
        if self.reject {
            bail!("test gate rejected request");
        }
        Ok(RequestPermit::new(7, Instant::now() + self.permit_lifetime))
    }

    fn after_forward(&self, _permit: RequestPermit, result: &ForwardResult) {
        self.results.lock().unwrap().push(result.clone());
    }
}

#[derive(Default)]
struct RecordingObserver {
    events: Mutex<Vec<ResponseCompletedMetadata>>,
}

impl ExchangeObserver for RecordingObserver {
    fn response_completed(&self, _permit: &RequestPermit, event: &ResponseCompletedMetadata) {
        self.events.lock().unwrap().push(event.clone());
    }
}

fn proxy_config(upstream_url: Url) -> ProxyConfig {
    ProxyConfig {
        listen_port: None,
        upstream_url,
        dump_dir: None,
        http_shutdown: false,
        default_request_timeout: Some(TEST_TIMEOUT),
        request_transform: Some(RequestTransformConfig {
            max_output_tokens: 321,
            max_body_bytes: 16 * 1024,
            inspector: Arc::new(RecordingInspector),
        }),
    }
}

fn start_proxy(
    config: ProxyConfig,
    gate: Arc<RecordingGate>,
    observer: Arc<RecordingObserver>,
) -> crate::RunningProxy {
    let bound = bind(&config).unwrap();
    activate(
        bound,
        config,
        locked_auth_header_for_test("test-token"),
        gate,
        observer,
    )
    .unwrap()
}

fn client() -> Client {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
        .timeout(TEST_TIMEOUT)
        .build()
        .unwrap()
}

fn spawn_upstream(
    status: u16,
    headers: Vec<Header>,
    body: String,
    delay: Duration,
) -> (Url, mpsc::Receiver<CapturedRequest>) {
    let server = Server::http("127.0.0.1:0").unwrap();
    let addr = server.server_addr().to_ip().unwrap();
    let (captured_tx, captured_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let Some(mut request) = server.recv_timeout(TEST_TIMEOUT).unwrap() else {
            return;
        };
        let mut request_body = Vec::new();
        request.as_reader().read_to_end(&mut request_body).unwrap();
        let captured = CapturedRequest {
            headers: request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.field.as_str().to_string(),
                        header.value.as_str().to_string(),
                    )
                })
                .collect(),
            body: request_body,
        };
        let _ = captured_tx.send(captured);
        if !delay.is_zero() {
            thread::sleep(delay);
        }
        let response = headers.into_iter().fold(
            Response::from_string(body).with_status_code(status),
            tiny_http::Response::with_header,
        );
        let _ = request.respond(response);
    });
    (
        Url::parse(&format!("http://{addr}/v1/responses")).unwrap(),
        captured_rx,
    )
}

fn sse_header() -> Header {
    Header::from_bytes("content-type", "text/event-stream").unwrap()
}

fn post_json(addr: SocketAddr, body: Value) -> reqwest::blocking::Response {
    client()
        .post(format!("http://{addr}/v1/responses"))
        .header("authorization", "Bearer caller-secret")
        .header("host", "attacker.invalid")
        .header("x-codex-window-id", "root-thread:0")
        .header("x-codex-parent-thread-id", "parent-thread")
        .header("x-openai-subagent", "collab_spawn")
        .json(&body)
        .send()
        .unwrap()
}

#[test]
fn transform_headers_and_sse_observation_are_end_to_end() {
    let completed = concat!(
        "event: response.completed\r\n",
        "data: {\"type\":\"response.completed\",\"response\":{",
        "\"id\":\"resp-1\",\"model\":\"glm-revision-1\",",
        "\"system_fingerprint\":\"deployment-7\",",
        "\"usage\":{\"input_tokens\":10,",
        "\"input_tokens_details\":{\"cached_tokens\":2,\"cache_write_tokens\":1},",
        "\"output_tokens\":5,\"output_tokens_details\":{\"reasoning_tokens\":3},",
        "\"total_tokens\":15},",
        "\"output\":[{\"secret_body\":\"observer must never receive this\"}]}}\r\n\r\n"
    );
    let (upstream_url, captured_rx) = spawn_upstream(
        200,
        vec![sse_header()],
        completed.to_string(),
        Duration::ZERO,
    );
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let observer = Arc::new(RecordingObserver::default());
    let proxy = start_proxy(proxy_config(upstream_url), gate.clone(), observer.clone());

    let response = post_json(proxy.addr(), json!({"model": "test", "input": []}));
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.text().unwrap(), completed);

    let captured = captured_rx.recv_timeout(TEST_TIMEOUT).unwrap();
    assert_eq!(captured.header("authorization"), Some("Bearer test-token"));
    assert_ne!(captured.header("host"), Some("attacker.invalid"));
    let upstream_body: Value = serde_json::from_slice(&captured.body).unwrap();
    assert_eq!(upstream_body["max_output_tokens"], json!(321));
    let expected_content_length = captured.body.len().to_string();
    assert_eq!(
        captured.header("content-length"),
        Some(expected_content_length.as_str())
    );

    assert_eq!(
        gate.requests.lock().unwrap().as_slice(),
        &[RequestMetadata {
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            window_id: Some("root-thread:0".to_string()),
            parent_thread_id: Some("parent-thread".to_string()),
            is_subagent: true,
        }]
    );
    let transformed = gate.transformed.lock().unwrap();
    assert_eq!(transformed.len(), 1);
    assert_eq!(transformed[0].content_length, captured.body.len());
    assert_eq!(transformed[0].evidence.raw_sha256, [1; 32]);
    drop(transformed);
    assert_eq!(
        gate.results.lock().unwrap().as_slice(),
        &[ForwardResult::Completed { status: 200 }]
    );
    assert_eq!(
        observer.events.lock().unwrap().as_slice(),
        &[ResponseCompletedMetadata {
            response_id: "resp-1".to_string(),
            usage: Some(ObservedUsage {
                total_tokens: 15,
                input_tokens: 10,
                cached_input_tokens: 2,
                cache_write_input_tokens: 1,
                output_tokens: 5,
                reasoning_output_tokens: 3,
            }),
            actual_model: Some("glm-revision-1".to_string()),
            deployment_or_fingerprint: Some("deployment-7".to_string()),
        }]
    );

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn gate_rejection_does_not_reach_upstream() {
    let (upstream_url, captured_rx) = spawn_upstream(
        200,
        Vec::new(),
        "should not be reached".to_string(),
        Duration::ZERO,
    );
    let gate = Arc::new(RecordingGate::reject());
    let observer = Arc::new(RecordingObserver::default());
    let proxy = start_proxy(proxy_config(upstream_url), gate.clone(), observer.clone());

    let response = post_json(proxy.addr(), json!({"model": "test"}));
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(
        captured_rx
            .recv_timeout(Duration::from_millis(100))
            .is_err()
    );
    assert!(gate.results.lock().unwrap().is_empty());
    assert!(observer.events.lock().unwrap().is_empty());

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn only_the_exact_responses_ingress_path_reaches_the_gate() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    drop(listener);
    let upstream_url = Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap();
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let proxy = start_proxy(
        proxy_config(upstream_url),
        gate.clone(),
        Arc::new(RecordingObserver::default()),
    );

    let get = client()
        .get(format!("http://{}/v1/responses", proxy.addr()))
        .send()
        .unwrap();
    let wrong_path = client()
        .post(format!("http://{}/v1/responses?debug=1", proxy.addr()))
        .body("{}")
        .send()
        .unwrap();

    assert_eq!(get.status(), StatusCode::FORBIDDEN);
    assert_eq!(wrong_path.status(), StatusCode::FORBIDDEN);
    assert!(gate.requests.lock().unwrap().is_empty());
    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn legacy_mode_forwards_non_json_without_transforming_it() {
    let original = b"legacy opaque body".to_vec();
    let (upstream_url, captured_rx) =
        spawn_upstream(200, Vec::new(), "ok".to_string(), Duration::ZERO);
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let mut config = proxy_config(upstream_url);
    config.request_transform = None;
    let proxy = start_proxy(config, gate.clone(), Arc::new(RecordingObserver::default()));

    let response = client()
        .post(format!("http://{}/v1/responses", proxy.addr()))
        .body(original.clone())
        .send()
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        captured_rx.recv_timeout(TEST_TIMEOUT).unwrap().body,
        original
    );
    let metadata = gate.transformed.lock().unwrap();
    assert_eq!(metadata[0].sha256, metadata[0].evidence.raw_sha256);
    assert_eq!(
        metadata[0].evidence.raw_sha256,
        metadata[0].evidence.normalized_sha256
    );
    drop(metadata);
    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn transformed_upstream_urls_are_loopback_http_or_query_free_https() {
    let non_loopback_http = proxy_config(Url::parse("http://example.com/v1/responses").unwrap());
    let queried = proxy_config(Url::parse("https://example.com/v1/responses?debug=1").unwrap());

    assert!(bind(&non_loopback_http).is_err());
    assert!(bind(&queried).is_err());
}

#[test]
fn malformed_conflicting_and_oversized_transforms_fail_before_gate() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    drop(listener);
    let upstream_url = Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap();
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let observer = Arc::new(RecordingObserver::default());
    let mut config = proxy_config(upstream_url);
    config.request_transform.as_mut().unwrap().max_body_bytes = 64;
    let proxy = start_proxy(config, gate.clone(), observer);
    let url = format!("http://{}/v1/responses", proxy.addr());

    for body in [
        "not-json".to_string(),
        "[]".to_string(),
        json!({"max_output_tokens": 1}).to_string(),
        json!({"max_output_tokens": "1"}).to_string(),
    ] {
        let response = client().post(&url).body(body).send().unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
    let response = client()
        .post(&url)
        .body(json!({"input": "x".repeat(128)}).to_string())
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(gate.requests.lock().unwrap().is_empty());

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn upstream_failure_records_attempt_without_completion() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let upstream_addr = listener.local_addr().unwrap();
    drop(listener);
    let upstream_url = Url::parse(&format!("http://{upstream_addr}/v1/responses")).unwrap();
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let observer = Arc::new(RecordingObserver::default());
    let proxy = start_proxy(proxy_config(upstream_url), gate.clone(), observer.clone());

    let response = post_json(proxy.addr(), json!({"model": "test"}));
    assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(
        gate.results.lock().unwrap().as_slice(),
        &[ForwardResult::Failed {
            class: ForwardErrorClass::Upstream,
        }]
    );
    assert!(observer.events.lock().unwrap().is_empty());

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn redirects_are_forwarded_without_following() {
    let target = Server::http("127.0.0.1:0").unwrap();
    let target_addr = target.server_addr().to_ip().unwrap();
    let (target_tx, target_rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let request = target.recv_timeout(Duration::from_millis(300)).unwrap();
        target_tx.send(request.is_some()).unwrap();
    });

    let location = Header::from_bytes(
        "location",
        format!("http://{target_addr}/captured").as_bytes(),
    )
    .unwrap();
    let (upstream_url, _) = spawn_upstream(307, vec![location], String::new(), Duration::ZERO);
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let proxy = start_proxy(
        proxy_config(upstream_url),
        gate.clone(),
        Arc::new(RecordingObserver::default()),
    );

    let response = post_json(proxy.addr(), json!({"model": "test"}));
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert!(!target_rx.recv_timeout(TEST_TIMEOUT).unwrap());
    assert_eq!(
        gate.results.lock().unwrap().as_slice(),
        &[ForwardResult::Completed { status: 307 }]
    );

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn permit_deadline_bounds_the_upstream_exchange() {
    let (upstream_url, _) = spawn_upstream(
        200,
        Vec::new(),
        "late".to_string(),
        Duration::from_millis(250),
    );
    let gate = Arc::new(RecordingGate::allow(Duration::from_millis(50)));
    let proxy = start_proxy(
        proxy_config(upstream_url),
        gate.clone(),
        Arc::new(RecordingObserver::default()),
    );
    let started = Instant::now();

    let response = post_json(proxy.addr(), json!({"model": "test"}));
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    assert!(started.elapsed() < Duration::from_secs(/*secs*/ 1));
    assert_eq!(
        gate.results.lock().unwrap().as_slice(),
        &[ForwardResult::Failed {
            class: ForwardErrorClass::DeadlineExceeded,
        }]
    );

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();
}

#[test]
fn shutdown_waits_for_in_flight_request_to_finish() {
    let (upstream_url, captured_rx) = spawn_upstream(
        200,
        Vec::new(),
        "done".to_string(),
        Duration::from_millis(120),
    );
    let gate = Arc::new(RecordingGate::allow(TEST_TIMEOUT));
    let proxy = start_proxy(
        proxy_config(upstream_url),
        gate,
        Arc::new(RecordingObserver::default()),
    );
    let addr = proxy.addr();
    let request = thread::spawn(move || post_json(addr, json!({"model": "test"})).status());
    captured_rx.recv_timeout(TEST_TIMEOUT).unwrap();
    let shutdown_started = Instant::now();

    proxy.shutdown_with_timeout(TEST_TIMEOUT).unwrap();

    assert!(shutdown_started.elapsed() >= Duration::from_millis(80));
    assert_eq!(request.join().unwrap(), StatusCode::OK);
}

#[test]
fn wait_does_not_initiate_shutdown() {
    let (upstream_url, _) = spawn_upstream(200, Vec::new(), String::new(), Duration::ZERO);
    let mut config = proxy_config(upstream_url);
    config.http_shutdown = true;
    let proxy = start_proxy(
        config,
        Arc::new(RecordingGate::allow(TEST_TIMEOUT)),
        Arc::new(RecordingObserver::default()),
    );
    let addr = proxy.addr();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    thread::spawn(move || done_tx.send(proxy.wait()).unwrap());

    assert!(done_rx.recv_timeout(Duration::from_millis(80)).is_err());
    let response = client()
        .get(format!("http://{addr}/shutdown"))
        .send()
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    done_rx.recv_timeout(TEST_TIMEOUT).unwrap().unwrap();
}

#[test]
fn legacy_cli_flags_still_parse() {
    let args = Args::try_parse_from([
        "responses-api-proxy",
        "--port",
        "43210",
        "--server-info",
        "/tmp/server-info.json",
        "--http-shutdown",
        "--upstream-url",
        "https://example.com/v1/responses",
        "--dump-dir",
        "/tmp/dumps",
    ])
    .unwrap();

    assert_eq!(args.port, Some(43210));
    assert!(args.http_shutdown);
    assert_eq!(args.upstream_url, "https://example.com/v1/responses");
    assert_eq!(
        args.server_info.unwrap().to_str(),
        Some("/tmp/server-info.json")
    );
    assert_eq!(args.dump_dir.unwrap().to_str(), Some("/tmp/dumps"));
}
