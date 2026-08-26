use std::io::Cursor;
use std::io::Read;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::header::AUTHORIZATION;
use reqwest::header::CONTENT_LENGTH;
use reqwest::header::HOST;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderName;
use reqwest::header::HeaderValue;
use reqwest::redirect::Policy;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tiny_http::Header;
use tiny_http::Method;
use tiny_http::Request;
use tiny_http::Response;
use tiny_http::Server;
use tiny_http::StatusCode;
use zeroize::Zeroize;
use zeroize::Zeroizing;

use crate::LockedAuthHeader;
use crate::broker_sse::ObservedResponseBody;
use crate::dump::ExchangeDumper;

const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(/*millis*/ 20);

#[derive(Clone)]
pub struct ProxyConfig {
    pub listen_port: Option<u16>,
    pub upstream_url: Url,
    pub dump_dir: Option<PathBuf>,
    pub http_shutdown: bool,
    pub default_request_timeout: Option<Duration>,
    pub request_transform: Option<RequestTransformConfig>,
}

#[derive(Clone)]
pub struct RequestTransformConfig {
    pub max_output_tokens: u64,
    pub max_body_bytes: usize,
    pub inspector: Arc<dyn RequestInspector>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedRequestMetadata {
    pub content_length: usize,
    pub sha256: [u8; 32],
    pub evidence: TransformedRequestEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformedRequestEvidence {
    pub raw_sha256: [u8; 32],
    pub normalized_sha256: [u8; 32],
    pub normalized_base_commitment: [u8; 32],
    pub treatment_diff_commitment: Option<[u8; 32]>,
}

/// Inspects one short-lived, post-limit-injection JSON request in memory.
///
/// Implementations must return only body-free digest evidence, must not retain
/// `post_injection_body`, and are called synchronously before [`RequestGate`].
pub trait RequestInspector: Send + Sync + 'static {
    fn inspect(&self, post_injection_body: &Value) -> Result<TransformedRequestEvidence>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestMetadata {
    pub method: String,
    pub path: String,
    pub window_id: Option<String>,
    pub parent_thread_id: Option<String>,
    pub is_subagent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedUsage {
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub cache_write_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseCompletedMetadata {
    pub response_id: String,
    pub usage: Option<ObservedUsage>,
    pub actual_model: Option<String>,
    pub deployment_or_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardErrorClass {
    DeadlineExceeded,
    Upstream,
    Downstream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardResult {
    Completed { status: u16 },
    Failed { class: ForwardErrorClass },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestPermit {
    attempt_id: u64,
    deadline: Instant,
}

impl RequestPermit {
    pub fn new(attempt_id: u64, deadline: Instant) -> Self {
        Self {
            attempt_id,
            deadline,
        }
    }

    pub fn attempt_id(&self) -> u64 {
        self.attempt_id
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }
}

/// Makes the synchronous, pre-forward authorization decision for one request.
///
/// `before_forward` runs after bounded transformation and before network I/O.
/// It receives no body and returns a permit with a non-extendable deadline.
/// `after_forward` is called exactly once for each returned permit, after the
/// upstream exchange has completed or failed, and may be called concurrently.
pub trait RequestGate: Send + Sync + 'static {
    fn before_forward(
        &self,
        request: &RequestMetadata,
        transformed: &TransformedRequestMetadata,
    ) -> Result<RequestPermit>;

    fn after_forward(&self, permit: RequestPermit, result: &ForwardResult);
}

/// Receives body-free metadata for each parsed `response.completed` SSE event.
///
/// Calls occur synchronously while an authorized response is streamed and may
/// be concurrent. Implementations must not assume that a successful HTTP status
/// guarantees an event; the evaluator reconciles observer events separately.
pub trait ExchangeObserver: Send + Sync + 'static {
    fn response_completed(&self, permit: &RequestPermit, event: &ResponseCompletedMetadata);
}

/// Loopback listener that is bound but does not accept requests until activated.
pub struct BoundProxy {
    listener: TcpListener,
    addr: SocketAddr,
}

impl BoundProxy {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Activated proxy handle with explicit wait and bounded shutdown operations.
pub struct RunningProxy {
    addr: SocketAddr,
    shutdown_tx: Option<mpsc::SyncSender<()>>,
    completion_rx: mpsc::Receiver<Result<()>>,
    join: Option<JoinHandle<()>>,
}

impl RunningProxy {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn wait(mut self) -> Result<()> {
        let result = self
            .completion_rx
            .recv()
            .context("proxy completion channel closed")?;
        self.join_thread()?;
        result
    }

    pub fn shutdown_with_timeout(mut self, timeout: Duration) -> Result<()> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        let result = self
            .completion_rx
            .recv_timeout(timeout)
            .context("timed out waiting for proxy shutdown")?;
        self.join_thread()?;
        result
    }

    fn join_thread(&mut self) -> Result<()> {
        let Some(join) = self.join.take() else {
            return Err(anyhow!("proxy thread already joined"));
        };
        join.join()
            .map_err(|_| anyhow!("proxy thread panicked while shutting down"))
    }
}

/// Validates the upstream and binds a loopback listener without accepting requests.
pub fn bind(config: &ProxyConfig) -> Result<BoundProxy> {
    validate_upstream_url(config)?;
    let addr = SocketAddr::from(([127, 0, 0, 1], config.listen_port.unwrap_or(0)));
    let listener = TcpListener::bind(addr).with_context(|| format!("failed to bind {addr}"))?;
    let addr = listener
        .local_addr()
        .context("failed to read bound proxy address")?;
    Ok(BoundProxy { listener, addr })
}

/// Starts accepting requests on a previously bound listener.
///
/// Callers must finish config, sandbox, and credential preflight before passing
/// the locked header. The gate and observer can be called concurrently and see
/// only bounded body-free metadata.
pub fn activate(
    bound: BoundProxy,
    config: ProxyConfig,
    auth: LockedAuthHeader,
    gate: Arc<dyn RequestGate>,
    observer: Arc<dyn ExchangeObserver>,
) -> Result<RunningProxy> {
    validate_upstream_url(&config)?;
    let server = Server::from_listener(bound.listener, None)
        .map_err(|error| anyhow!("creating HTTP proxy server: {error}"))?;
    let client = Client::builder()
        .timeout(None::<Duration>)
        .redirect(Policy::none())
        .no_proxy()
        .build()
        .context("building upstream HTTP client")?;
    let forward = Arc::new(ForwardConfig::new(config.upstream_url.clone(), auth)?);
    let dump = config
        .dump_dir
        .clone()
        .map(ExchangeDumper::new)
        .transpose()
        .context("creating proxy dump directory")?
        .map(Arc::new);
    let runtime = Arc::new(RuntimeConfig {
        client,
        forward,
        dump,
        request_transform: config.request_transform.clone(),
        default_request_timeout: config.default_request_timeout,
        gate,
        observer,
    });
    let (shutdown_tx, shutdown_rx) = mpsc::sync_channel(1);
    let (completion_tx, completion_rx) = mpsc::sync_channel(1);
    let http_shutdown = config.http_shutdown;
    let addr = bound.addr;
    let join = std::thread::spawn(move || {
        let result = serve(server, runtime, http_shutdown, shutdown_rx);
        let _ = completion_tx.send(result);
    });
    Ok(RunningProxy {
        addr,
        shutdown_tx: Some(shutdown_tx),
        completion_rx,
        join: Some(join),
    })
}

struct ForwardConfig {
    upstream_url: Url,
    host_header: HeaderValue,
    auth_header: HeaderValue,
}

impl ForwardConfig {
    fn new(upstream_url: Url, auth: LockedAuthHeader) -> Result<Self> {
        let host = upstream_host_header(&upstream_url)?;
        let host_header = HeaderValue::from_str(&host).context("constructing upstream Host")?;
        let mut auth_header = HeaderValue::from_static(auth.as_str());
        auth_header.set_sensitive(true);
        Ok(Self {
            upstream_url,
            host_header,
            auth_header,
        })
    }
}

struct RuntimeConfig {
    client: Client,
    forward: Arc<ForwardConfig>,
    dump: Option<Arc<ExchangeDumper>>,
    request_transform: Option<RequestTransformConfig>,
    default_request_timeout: Option<Duration>,
    gate: Arc<dyn RequestGate>,
    observer: Arc<dyn ExchangeObserver>,
}

fn serve(
    server: Server,
    runtime: Arc<RuntimeConfig>,
    http_shutdown: bool,
    shutdown_rx: mpsc::Receiver<()>,
) -> Result<()> {
    let mut workers = Vec::new();
    loop {
        match shutdown_rx.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let Some(request) = server
            .recv_timeout(ACCEPT_POLL_INTERVAL)
            .context("accepting proxy request")?
        else {
            reap_finished(&mut workers)?;
            continue;
        };
        if http_shutdown && request.method() == &Method::Get && request.url() == "/shutdown" {
            let _ = request.respond(Response::new_empty(StatusCode(200)));
            break;
        }
        let runtime = runtime.clone();
        workers.push(std::thread::spawn(move || {
            if let Err(error) = forward_request(request, &runtime) {
                eprintln!("responses-api-proxy forwarding error: {error:#}");
            }
        }));
        reap_finished(&mut workers)?;
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| anyhow!("proxy request worker panicked"))?;
    }
    Ok(())
}

fn reap_finished(workers: &mut Vec<JoinHandle<()>>) -> Result<()> {
    let mut index = 0;
    while index < workers.len() {
        if workers[index].is_finished() {
            workers
                .swap_remove(index)
                .join()
                .map_err(|_| anyhow!("proxy request worker panicked"))?;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn forward_request(mut request: Request, runtime: &RuntimeConfig) -> Result<()> {
    let method = request.method().clone();
    let path = request.url().to_string();
    if method != Method::Post || path != "/v1/responses" {
        let _ = request.respond(Response::new_empty(StatusCode(403)));
        return Ok(());
    }
    let request_metadata = request_metadata(&request, &path);
    let raw_body = match read_body(&mut request, runtime.request_transform.as_ref()) {
        Ok(body) => body,
        Err(RequestBodyError::TooLarge) => {
            let _ = request.respond(Response::new_empty(StatusCode(413)));
            return Ok(());
        }
        Err(RequestBodyError::Read(error)) => return Err(error),
    };
    let exchange_dump = runtime.dump.as_ref().and_then(|dump| {
        dump.dump_request(&method, &path, request.headers(), &raw_body)
            .map_err(|error| {
                eprintln!("responses-api-proxy failed to dump request: {error}");
                error
            })
            .ok()
    });
    let prepared = match prepare_body(raw_body, runtime.request_transform.as_ref()) {
        Ok(prepared) => prepared,
        Err(_) => {
            let _ = request.respond(Response::new_empty(StatusCode(400)));
            return Ok(());
        }
    };
    let permit = match runtime
        .gate
        .before_forward(&request_metadata, &prepared.metadata)
    {
        Ok(permit) => permit,
        Err(_) => {
            let _ = request.respond(Response::new_empty(StatusCode(403)));
            return Ok(());
        }
    };
    let Some(timeout) = remaining_timeout(&permit, runtime.default_request_timeout) else {
        let result = ForwardResult::Failed {
            class: ForwardErrorClass::DeadlineExceeded,
        };
        runtime.gate.after_forward(permit, &result);
        let _ = request.respond(Response::new_empty(StatusCode(504)));
        return Ok(());
    };
    let headers = upstream_headers(&request, &runtime.forward, prepared.bytes.len())?;
    let body = reqwest::blocking::Body::new(ZeroizingCursor::new(prepared.bytes));
    let upstream = runtime
        .client
        .post(runtime.forward.upstream_url.clone())
        .headers(headers)
        .timeout(timeout)
        .body(body)
        .send();
    let upstream = match upstream {
        Ok(response) => response,
        Err(error) => {
            let class = if error.is_timeout() || Instant::now() >= permit.deadline() {
                ForwardErrorClass::DeadlineExceeded
            } else {
                ForwardErrorClass::Upstream
            };
            let status = if class == ForwardErrorClass::DeadlineExceeded {
                StatusCode(504)
            } else {
                StatusCode(502)
            };
            let result = ForwardResult::Failed { class };
            runtime.gate.after_forward(permit, &result);
            let _ = request.respond(Response::new_empty(status));
            return Ok(());
        }
    };
    let status = upstream.status();
    let headers = response_headers(upstream.headers());
    let content_length = upstream
        .content_length()
        .and_then(|length| usize::try_from(length).ok());
    let observed = ObservedResponseBody::new(upstream, permit.clone(), runtime.observer.clone());
    let response_body: Box<dyn Read + Send> = if let Some(exchange_dump) = exchange_dump {
        let response_headers = observed.inner_headers().clone();
        Box::new(exchange_dump.tee_response_body(status.as_u16(), &response_headers, observed))
    } else {
        Box::new(observed)
    };
    let response = Response::new(
        StatusCode(status.as_u16()),
        headers,
        response_body,
        content_length,
        None,
    );
    let respond_result = request.respond(response);
    let result = if respond_result.is_ok() {
        ForwardResult::Completed {
            status: status.as_u16(),
        }
    } else if Instant::now() >= permit.deadline() {
        ForwardResult::Failed {
            class: ForwardErrorClass::DeadlineExceeded,
        }
    } else {
        ForwardResult::Failed {
            class: ForwardErrorClass::Downstream,
        }
    };
    runtime.gate.after_forward(permit, &result);
    Ok(())
}

fn request_metadata(request: &Request, path: &str) -> RequestMetadata {
    let window_id = incoming_header(request, "x-codex-window-id");
    let parent_thread_id = incoming_header(request, "x-codex-parent-thread-id");
    let is_subagent = incoming_header(request, "x-openai-subagent").is_some();
    RequestMetadata {
        method: request.method().as_str().to_string(),
        path: path.to_string(),
        window_id,
        parent_thread_id,
        is_subagent,
    }
}

fn incoming_header(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .iter()
        .find(|header| header.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str().to_string())
}

enum RequestBodyError {
    TooLarge,
    Read(anyhow::Error),
}

fn read_body(
    request: &mut Request,
    transform: Option<&RequestTransformConfig>,
) -> std::result::Result<Zeroizing<Vec<u8>>, RequestBodyError> {
    let mut body = Zeroizing::new(Vec::new());
    if let Some(transform) = transform {
        let read_limit = transform.max_body_bytes.saturating_add(1);
        let mut reader = request
            .as_reader()
            .take(u64::try_from(read_limit).unwrap_or(u64::MAX));
        reader
            .read_to_end(&mut body)
            .map_err(|error| RequestBodyError::Read(error.into()))?;
        if body.len() > transform.max_body_bytes {
            return Err(RequestBodyError::TooLarge);
        }
    } else {
        request
            .as_reader()
            .read_to_end(&mut body)
            .map_err(|error| RequestBodyError::Read(error.into()))?;
    }
    Ok(body)
}

struct PreparedBody {
    bytes: Vec<u8>,
    metadata: TransformedRequestMetadata,
}

fn prepare_body(
    mut raw_body: Zeroizing<Vec<u8>>,
    transform: Option<&RequestTransformConfig>,
) -> Result<PreparedBody> {
    let raw_sha256 = sha256(&raw_body);
    let Some(transform) = transform else {
        let bytes = std::mem::take(&mut *raw_body);
        return Ok(PreparedBody {
            metadata: TransformedRequestMetadata {
                content_length: bytes.len(),
                sha256: raw_sha256,
                evidence: TransformedRequestEvidence {
                    raw_sha256,
                    normalized_sha256: raw_sha256,
                    normalized_base_commitment: raw_sha256,
                    treatment_diff_commitment: None,
                },
            },
            bytes,
        });
    };
    let mut value =
        SensitiveJson::new(serde_json::from_slice(&raw_body).context("parsing request JSON")?);
    let object = value
        .as_mut()
        .as_object_mut()
        .ok_or_else(|| anyhow!("Responses request body must be a JSON object"))?;
    if object.contains_key("max_output_tokens") {
        return Err(anyhow!(
            "caller must not provide max_output_tokens when transform is active"
        ));
    }
    object.insert(
        "max_output_tokens".to_string(),
        Value::Number(transform.max_output_tokens.into()),
    );
    let evidence = transform.inspector.inspect(value.as_ref())?;
    let bytes = serde_json::to_vec(value.as_ref()).context("serializing transformed request")?;
    let transformed_sha256 = sha256(&bytes);
    Ok(PreparedBody {
        metadata: TransformedRequestMetadata {
            content_length: bytes.len(),
            sha256: transformed_sha256,
            evidence,
        },
        bytes,
    })
}

fn upstream_headers(
    request: &Request,
    forward: &ForwardConfig,
    content_length: usize,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for header in request.headers() {
        let lower = header.field.as_str().to_ascii_lowercase();
        if matches!(lower.as_str(), "authorization" | "host" | "content-length") {
            continue;
        }
        let Ok(name) = HeaderName::from_bytes(lower.as_bytes()) else {
            continue;
        };
        if let Ok(value) = HeaderValue::from_bytes(header.value.as_bytes()) {
            headers.append(name, value);
        }
    }
    headers.insert(AUTHORIZATION, forward.auth_header.clone());
    headers.insert(HOST, forward.host_header.clone());
    headers.insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&content_length.to_string())
            .context("constructing transformed Content-Length")?,
    );
    Ok(headers)
}

fn response_headers(headers: &HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "content-length" | "transfer-encoding" | "connection" | "trailer" | "upgrade"
            )
        })
        .filter_map(|(name, value)| Header::from_bytes(name.as_str(), value.as_bytes()).ok())
        .collect()
}

fn remaining_timeout(
    permit: &RequestPermit,
    default_timeout: Option<Duration>,
) -> Option<Duration> {
    let now = Instant::now();
    let permit_remaining = permit.deadline().checked_duration_since(now)?;
    Some(default_timeout.map_or(permit_remaining, |limit| permit_remaining.min(limit)))
}

fn validate_upstream_url(config: &ProxyConfig) -> Result<()> {
    let url = &config.upstream_url;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(anyhow!("upstream URL must use http or https"));
    }
    if url.host_str().is_none() {
        return Err(anyhow!("upstream URL must include a host"));
    }
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return Err(anyhow!(
            "upstream URL must not include userinfo or a fragment"
        ));
    }
    if url.scheme() == "http" {
        let host = url
            .host_str()
            .ok_or_else(|| anyhow!("upstream URL must include a host"))?;
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());
        if !loopback {
            return Err(anyhow!("plain HTTP upstream must be loopback"));
        }
    }
    if config.request_transform.is_some() && url.query().is_some() {
        return Err(anyhow!(
            "transformed evaluator upstream URL must not include a query"
        ));
    }
    Ok(())
}

fn upstream_host_header(url: &Url) -> Result<String> {
    let raw_host = url
        .host_str()
        .ok_or_else(|| anyhow!("upstream URL must include a host"))?;
    let host = if raw_host.contains(':') {
        format!("[{raw_host}]")
    } else {
        raw_host.to_string()
    };
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    })
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

struct SensitiveJson {
    value: Value,
}

impl SensitiveJson {
    fn new(value: Value) -> Self {
        Self { value }
    }

    fn as_ref(&self) -> &Value {
        &self.value
    }

    fn as_mut(&mut self) -> &mut Value {
        &mut self.value
    }
}

impl Drop for SensitiveJson {
    fn drop(&mut self) {
        zeroize_json(std::mem::replace(&mut self.value, Value::Null));
    }
}

fn zeroize_json(value: Value) {
    match value {
        Value::String(mut value) => value.zeroize(),
        Value::Array(values) => {
            for value in values {
                zeroize_json(value);
            }
        }
        Value::Object(values) => {
            for (mut key, value) in values {
                key.zeroize();
                zeroize_json(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

struct ZeroizingCursor {
    cursor: Cursor<Vec<u8>>,
}

impl ZeroizingCursor {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            cursor: Cursor::new(bytes),
        }
    }
}

impl Read for ZeroizingCursor {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(buffer)
    }
}

impl Drop for ZeroizingCursor {
    fn drop(&mut self) {
        self.cursor.get_mut().zeroize();
    }
}
