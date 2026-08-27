use std::fs::File;
use std::fs::{self};
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use serde::Serialize;

mod broker;
mod broker_sse;
mod dump;
mod read_api_key;

pub use broker::BoundProxy;
pub use broker::ExchangeObserver;
pub use broker::ForwardErrorClass;
pub use broker::ForwardResult;
pub use broker::ObservedUsage;
pub use broker::ProxyConfig;
pub use broker::RequestGate;
pub use broker::RequestInspector;
pub use broker::RequestMetadata;
pub use broker::RequestPermit;
pub use broker::RequestTransformConfig;
pub use broker::ResponseCompletedMetadata;
pub use broker::RunningProxy;
pub use broker::TransformedRequestEvidence;
pub use broker::TransformedRequestMetadata;
pub use broker::activate;
pub use broker::bind;
pub use read_api_key::LockedAuthHeader;
pub use read_api_key::local_mock_auth_header;
pub use read_api_key::read_auth_header_from_stdin;

#[cfg(test)]
mod broker_tests;

/// CLI arguments for the proxy.
#[derive(Debug, Clone, Parser)]
#[command(name = "responses-api-proxy", about = "Minimal OpenAI responses proxy")]
pub struct Args {
    /// Port to listen on. If not set, an ephemeral port is used.
    #[arg(long)]
    pub port: Option<u16>,

    /// Path to a JSON file to write startup info (single line). Includes {"port": <u16>}.
    #[arg(long, value_name = "FILE")]
    pub server_info: Option<PathBuf>,

    /// Enable HTTP shutdown endpoint at GET /shutdown.
    #[arg(long)]
    pub http_shutdown: bool,

    /// Absolute URL the proxy should forward requests to (defaults to OpenAI).
    #[arg(long, default_value = "https://api.openai.com/v1/responses")]
    pub upstream_url: String,

    /// Directory where request/response dumps should be written as JSON.
    #[arg(long, value_name = "DIR")]
    pub dump_dir: Option<PathBuf>,
}

#[derive(Serialize)]
struct ServerInfo {
    port: u16,
    pid: u32,
}

struct AllowAllGate {
    request_timeout: Duration,
}

impl RequestGate for AllowAllGate {
    fn before_forward(
        &self,
        _request: &RequestMetadata,
        _transformed: &TransformedRequestMetadata,
    ) -> Result<RequestPermit> {
        Ok(RequestPermit::new(0, Instant::now() + self.request_timeout))
    }

    fn after_forward(&self, _permit: RequestPermit, _result: &ForwardResult) {}
}

struct NoopObserver;

impl ExchangeObserver for NoopObserver {
    fn response_completed(&self, _permit: &RequestPermit, _event: &ResponseCompletedMetadata) {}
}

/// Entry point for the library main, for parity with other crates.
pub fn run_main(args: Args) -> Result<()> {
    let upstream_url = reqwest::Url::parse(&args.upstream_url).context("parsing --upstream-url")?;
    let config = ProxyConfig {
        listen_port: args.port,
        upstream_url,
        dump_dir: args.dump_dir,
        http_shutdown: args.http_shutdown,
        default_request_timeout: None,
        request_transform: None,
    };
    let bound = bind(&config)?;
    if let Some(path) = args.server_info.as_ref() {
        write_server_info(path, bound.addr().port())?;
    }
    let auth = read_auth_header_from_stdin()?;
    let gate = Arc::new(AllowAllGate {
        // Legacy mode historically had no request timeout. Keep that practical
        // behavior while still giving each permit an immutable absolute deadline.
        request_timeout: Duration::from_secs(/*secs*/ 365 * 24 * 60 * 60),
    });
    let proxy = activate(bound, config, auth, gate, Arc::new(NoopObserver))?;
    eprintln!("responses-api-proxy listening on {}", proxy.addr());
    proxy.wait()
}

fn write_server_info(path: &Path, port: u16) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    let info = ServerInfo {
        port,
        pid: std::process::id(),
    };
    let mut data = serde_json::to_string(&info)?;
    data.push('\n');
    let mut file = File::create(path)?;
    std::io::Write::write_all(&mut file, data.as_bytes())?;
    Ok(())
}
