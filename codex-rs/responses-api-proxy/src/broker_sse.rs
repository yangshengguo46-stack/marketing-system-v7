use std::io::Read;
use std::sync::Arc;

use reqwest::header::HeaderMap;
use serde::Deserialize;
use zeroize::Zeroize;
use zeroize::Zeroizing;

use crate::broker::ExchangeObserver;
use crate::broker::ObservedUsage;
use crate::broker::RequestPermit;
use crate::broker::ResponseCompletedMetadata;

const MAX_SSE_EVENT_BYTES: usize = 256 * 1024;

pub(crate) struct ObservedResponseBody<R> {
    inner: R,
    parser: SseMetadataParser,
}

impl<R> ObservedResponseBody<R> {
    pub(crate) fn new(
        inner: R,
        permit: RequestPermit,
        observer: Arc<dyn ExchangeObserver>,
    ) -> Self {
        Self {
            inner,
            parser: SseMetadataParser {
                buffer: Vec::new(),
                permit,
                observer,
            },
        }
    }
}

impl ObservedResponseBody<reqwest::blocking::Response> {
    pub(crate) fn inner_headers(&self) -> &HeaderMap {
        self.inner.headers()
    }
}

impl<R: Read> Read for ObservedResponseBody<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(output)?;
        if read > 0 {
            self.parser.consume(&output[..read]);
        }
        Ok(read)
    }
}

struct SseMetadataParser {
    buffer: Vec<u8>,
    permit: RequestPermit,
    observer: Arc<dyn ExchangeObserver>,
}

impl SseMetadataParser {
    fn consume(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        while let Some((end, delimiter_length)) = next_sse_event_boundary(&self.buffer) {
            let event = Zeroizing::new(
                self.buffer
                    .drain(..end + delimiter_length)
                    .collect::<Vec<_>>(),
            );
            self.inspect_event(&event[..end]);
        }
        if self.buffer.len() > MAX_SSE_EVENT_BYTES {
            self.buffer.zeroize();
            self.buffer.clear();
        }
    }

    fn inspect_event(&self, event: &[u8]) {
        let mut data = Zeroizing::new(Vec::new());
        for line in event.split(|byte| *byte == b'\n') {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let Some(value) = line.strip_prefix(b"data:") else {
                continue;
            };
            let value = value.strip_prefix(b" ").unwrap_or(value);
            if !data.is_empty() {
                data.push(b'\n');
            }
            data.extend_from_slice(value);
        }
        if data.is_empty() || data.len() > MAX_SSE_EVENT_BYTES {
            return;
        }
        let Ok(envelope) = serde_json::from_slice::<SseEnvelope<'_>>(&data) else {
            return;
        };
        if envelope.kind != "response.completed" {
            return;
        }
        let Some(response) = envelope.response else {
            return;
        };
        let usage = response.usage.map(ObservedUsage::from);
        let event = ResponseCompletedMetadata {
            response_id: response.id.to_string(),
            usage,
            actual_model: response.model.map(str::to_string),
            deployment_or_fingerprint: response
                .system_fingerprint
                .or(response.deployment)
                .map(str::to_string),
        };
        self.observer.response_completed(&self.permit, &event);
    }
}

fn next_sse_event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = buffer.windows(2).position(|window| window == b"\n\n");
    let crlf = buffer.windows(4).position(|window| window == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(lf), Some(crlf)) if lf <= crlf => Some((lf, 2)),
        (Some(_), Some(crlf)) => Some((crlf, 4)),
        (Some(lf), None) => Some((lf, 2)),
        (None, Some(crlf)) => Some((crlf, 4)),
        (None, None) => None,
    }
}

impl Drop for SseMetadataParser {
    fn drop(&mut self) {
        self.buffer.zeroize();
    }
}

#[derive(Deserialize)]
struct SseEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(borrow)]
    response: Option<SseCompletedResponse<'a>>,
}

#[derive(Deserialize)]
struct SseCompletedResponse<'a> {
    id: &'a str,
    model: Option<&'a str>,
    system_fingerprint: Option<&'a str>,
    deployment: Option<&'a str>,
    usage: Option<SseUsage>,
}

#[derive(Deserialize)]
struct SseUsage {
    total_tokens: i64,
    input_tokens: i64,
    input_tokens_details: Option<SseInputTokenDetails>,
    output_tokens: i64,
    output_tokens_details: Option<SseOutputTokenDetails>,
}

#[derive(Default, Deserialize)]
struct SseInputTokenDetails {
    cached_tokens: i64,
    #[serde(default)]
    cache_write_tokens: i64,
}

#[derive(Deserialize)]
struct SseOutputTokenDetails {
    reasoning_tokens: i64,
}

impl From<SseUsage> for ObservedUsage {
    fn from(value: SseUsage) -> Self {
        let input_details = value.input_tokens_details.unwrap_or_default();
        Self {
            total_tokens: value.total_tokens,
            input_tokens: value.input_tokens,
            cached_input_tokens: input_details.cached_tokens,
            cache_write_input_tokens: input_details.cache_write_tokens,
            output_tokens: value.output_tokens,
            reasoning_output_tokens: value
                .output_tokens_details
                .map_or(0, |details| details.reasoning_tokens),
        }
    }
}
