use crate::Provider;
use anyhow::{Result, anyhow};
use reqwest::{
    RequestBuilder, Response, StatusCode,
    header::{HeaderMap, RETRY_AFTER},
};
use std::fmt;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

const MODEL_HTTP_MAX_ATTEMPTS: usize = 3;

pub(crate) async fn send_model_request_with_retry(
    provider: Provider,
    model: &str,
    diagnostics: RequestDiagnostics,
    build: impl Fn() -> RequestBuilder,
) -> Result<Response> {
    for attempt in 1..=MODEL_HTTP_MAX_ATTEMPTS {
        match build().send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    return Ok(response);
                }

                if should_retry_status(status) && attempt < MODEL_HTTP_MAX_ATTEMPTS {
                    let delay = retry_after(response.headers())
                        .unwrap_or_else(|| retry_backoff(attempt))
                        .min(Duration::from_secs(10));
                    warn!(
                        provider = %provider,
                        model = %model,
                        status = status.as_u16(),
                        attempt,
                        max_attempts = MODEL_HTTP_MAX_ATTEMPTS,
                        delay_ms = delay.as_millis() as u64,
                        request = %diagnostics,
                        "transient model HTTP status; retrying"
                    );
                    sleep(delay).await;
                    continue;
                }

                return Err(model_http_status_error(response, diagnostics).await);
            }
            Err(error) => {
                if should_retry_error(&error) && attempt < MODEL_HTTP_MAX_ATTEMPTS {
                    let delay = retry_backoff(attempt);
                    warn!(
                        provider = %provider,
                        model = %model,
                        error = %error,
                        attempt,
                        max_attempts = MODEL_HTTP_MAX_ATTEMPTS,
                        delay_ms = delay.as_millis() as u64,
                        request = %diagnostics,
                        "transient model request error; retrying"
                    );
                    sleep(delay).await;
                    continue;
                }

                return Err(anyhow!("{error}; request diagnostics: {diagnostics}"));
            }
        }
    }

    unreachable!("retry loop always returns");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RequestDiagnostics {
    payload_bytes: usize,
    system_bytes: usize,
    index_bytes: usize,
    bodies_bytes: usize,
    auth_header_bytes: usize,
}

impl RequestDiagnostics {
    pub(crate) fn new(
        payload_bytes: usize,
        system_bytes: usize,
        index_bytes: usize,
        bodies_bytes: usize,
        auth_header_bytes: usize,
    ) -> Self {
        Self {
            payload_bytes,
            system_bytes,
            index_bytes,
            bodies_bytes,
            auth_header_bytes,
        }
    }
}

impl fmt::Display for RequestDiagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "payload_bytes={}, system_bytes={}, index_bytes={}, bodies_bytes={}, auth_header_bytes={}",
            self.payload_bytes,
            self.system_bytes,
            self.index_bytes,
            self.bodies_bytes,
            self.auth_header_bytes
        )
    }
}

async fn model_http_status_error(
    response: Response,
    diagnostics: RequestDiagnostics,
) -> anyhow::Error {
    let status = response.status();
    let url = response.url().to_string();
    let body = response.text().await.unwrap_or_default();
    let body = single_line_truncated(&body, 500);
    if body.is_empty() {
        anyhow!("HTTP status {status} for url ({url}); request diagnostics: {diagnostics}")
    } else {
        anyhow!(
            "HTTP status {status} for url ({url}); response body: {body}; request diagnostics: {diagnostics}"
        )
    }
}

pub(crate) fn should_retry_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::REQUEST_TIMEOUT
            | StatusCode::CONFLICT
            | StatusCode::TOO_MANY_REQUESTS
            | StatusCode::INTERNAL_SERVER_ERROR
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
}

fn should_retry_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

pub(crate) fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(Duration::from_secs(seconds))
}

fn retry_backoff(attempt: usize) -> Duration {
    let shift = attempt.saturating_sub(1).min(4) as u32;
    let base_ms = 250_u64.saturating_mul(1_u64 << shift);
    let jitter_ms = 37_u64.saturating_mul(attempt as u64);
    Duration::from_millis(base_ms + jitter_ms)
}

fn single_line_truncated(value: &str, limit: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
