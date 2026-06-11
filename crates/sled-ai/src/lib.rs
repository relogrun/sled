use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::{
    Client, RequestBuilder, Response, StatusCode,
    header::{HeaderMap, RETRY_AFTER},
};
use serde_json::{Value, json};
use sled_core::{Call, Context, Model, Reply};
use std::fmt;
use std::io::{self, Write};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

const MODEL_HTTP_MAX_ATTEMPTS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Operator,
    OpenAi,
    OpenAiCompatible,
    Anthropic,
}

impl std::str::FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "operator" => Self::Operator,
            "openai" => Self::OpenAi,
            "openai-compatible" => Self::OpenAiCompatible,
            "anthropic" => Self::Anthropic,
            other => bail!("unknown provider: {other}"),
        })
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Operator => "operator",
            Self::OpenAi => "openai",
            Self::OpenAiCompatible => "openai-compatible",
            Self::Anthropic => "anthropic",
        })
    }
}

pub fn default_model(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::OpenAi => Some("gpt-5.5"),
        Provider::Anthropic => Some("claude-sonnet-4-6"),
        Provider::Operator | Provider::OpenAiCompatible => None,
    }
}

#[derive(Clone, Debug, Default)]
pub struct ModelOptions {
    pub model: Option<String>,
    pub openai_compatible_base_url: Option<String>,
    pub temperature: Option<f32>,
}

pub fn create_model_with_options(
    provider: Provider,
    options: ModelOptions,
) -> Result<Box<dyn Model>> {
    match provider {
        Provider::Operator => {
            info!(provider = %provider, "creating model client");
            Ok(Box::new(OperatorModel))
        }
        Provider::OpenAi => {
            let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;
            let model = options.model.unwrap_or_else(|| {
                default_model(provider)
                    .expect("OpenAI has a default")
                    .into()
            });
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(OpenAiModel {
                client: Client::new(),
                api_key,
                model,
                endpoint: "https://api.openai.com/v1/chat/completions".into(),
                provider,
                temperature: options.temperature,
            }))
        }
        Provider::OpenAiCompatible => {
            let base_url = required_non_empty(
                options.openai_compatible_base_url,
                "--openai-compatible-base-url or _config.openai_compatible.base_url is required",
            )?;
            let model = required_non_empty(
                options.model,
                "--model or _config.openai_compatible.model is required",
            )?;
            let api_key = std::env::var("SLED_OPENAI_COMPAT_API_KEY")
                .context("SLED_OPENAI_COMPAT_API_KEY is required")?;
            let endpoint = chat_completions_endpoint(&base_url);
            info!(provider = %provider, model = %model, endpoint = %endpoint, "creating model client");
            Ok(Box::new(OpenAiModel {
                client: Client::new(),
                api_key,
                model,
                endpoint,
                provider,
                temperature: options.temperature,
            }))
        }
        Provider::Anthropic => {
            let api_key =
                std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is required")?;
            let model = options.model.unwrap_or_else(|| {
                default_model(provider)
                    .expect("Anthropic has a default")
                    .into()
            });
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(AnthropicModel {
                client: Client::new(),
                api_key,
                model,
                temperature: options.temperature,
            }))
        }
    }
}

fn required_non_empty(value: Option<String>, message: &'static str) -> Result<String> {
    let value = value.context(message)?;
    let value = value.trim();
    if value.is_empty() {
        bail!(message);
    }
    Ok(value.to_string())
}

pub struct OperatorModel;

#[async_trait]
impl Model for OperatorModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = "operator", "reading operator assistant input");
        println!(
            "\n=== context ===\n[system]\n{}\n\n[index]\n{}\n[bodies]\n{}",
            system, context.index, context.bodies
        );
        println!("answer as assistant:");
        println!("  final <text>");
        println!("  wait <text>");
        println!("  tool {{\"tool\":\"read\",\"args\":{{\"paths\":[\"Cargo.toml\"]}}}}");
        loop {
            print!("> ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            let line = line.trim();
            if let Some(text) = line.strip_prefix("final ") {
                return Ok(Reply::Final {
                    text: text.into(),
                    summary: shorten(text, 80),
                    wait_user: false,
                });
            }
            if let Some(text) = line.strip_prefix("wait ") {
                return Ok(Reply::Final {
                    text: text.into(),
                    summary: shorten(text, 80),
                    wait_user: true,
                });
            }
            if let Some(raw) = line.strip_prefix("tool ") {
                match serde_json::from_str::<Call>(raw) {
                    Ok(call) => {
                        let summary = format!("call {}", call.tool);
                        return Ok(Reply::Tool { call, summary });
                    }
                    Err(err) => println!("could not parse json: {err}"),
                }
            }
        }
    }
}

pub struct OpenAiModel {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    provider: Provider,
    temperature: Option<f32>,
}

#[async_trait]
impl Model for OpenAiModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = %self.provider, model = %self.model, "sending model request");
        debug!(
            index_bytes = context.index.len(),
            bodies_bytes = context.bodies.len(),
            system_bytes = system.len(),
            "model request context sizes"
        );
        let user = format!(
            "Dialog index:\n{}\n\nOpened bodies:\n{}",
            context.index, context.bodies
        );
        let mut payload = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user}
            ]
        });
        if let Some(temperature) = self.temperature {
            payload["temperature"] = json!(temperature);
        }
        let diagnostics = RequestDiagnostics::new(
            serde_json::to_vec(&payload)?.len(),
            system.len(),
            context.index.len(),
            context.bodies.len(),
            "Bearer ".len() + self.api_key.len(),
        );
        let response: Value =
            send_model_request_with_retry(self.provider, &self.model, diagnostics, || {
                self.client
                    .post(&self.endpoint)
                    .bearer_auth(&self.api_key)
                    .json(&payload)
            })
            .await?
            .json()
            .await?;

        info!(provider = %self.provider, model = %self.model, "received model response");
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("empty OpenAI-compatible response: {response}"))?;
        parse_reply(text)
    }
}

fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.into()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

pub struct AnthropicModel {
    client: Client,
    api_key: String,
    model: String,
    temperature: Option<f32>,
}

#[async_trait]
impl Model for AnthropicModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = "anthropic", model = %self.model, "sending model request");
        debug!(
            index_bytes = context.index.len(),
            bodies_bytes = context.bodies.len(),
            system_bytes = system.len(),
            "model request context sizes"
        );
        let user = format!(
            "Dialog index:\n{}\n\nOpened bodies:\n{}",
            context.index, context.bodies
        );
        let mut payload = json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": system,
            "messages": [{"role": "user", "content": user}]
        });
        if let Some(temperature) = self.temperature {
            payload["temperature"] = json!(temperature);
        }
        let diagnostics = RequestDiagnostics::new(
            serde_json::to_vec(&payload)?.len(),
            system.len(),
            context.index.len(),
            context.bodies.len(),
            self.api_key.len() + "2023-06-01".len(),
        );
        let response: Value =
            send_model_request_with_retry(Provider::Anthropic, &self.model, diagnostics, || {
                self.client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", &self.api_key)
                    .header("anthropic-version", "2023-06-01")
                    .json(&payload)
            })
            .await?
            .json()
            .await?;

        info!(provider = "anthropic", model = %self.model, "received model response");
        let text = response["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("empty Anthropic response: {response}"))?;
        parse_reply(text)
    }
}

async fn send_model_request_with_retry(
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
struct RequestDiagnostics {
    payload_bytes: usize,
    system_bytes: usize,
    index_bytes: usize,
    bodies_bytes: usize,
    auth_header_bytes: usize,
}

impl RequestDiagnostics {
    fn new(
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

fn should_retry_status(status: StatusCode) -> bool {
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

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
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

fn parse_reply(text: &str) -> Result<Reply> {
    debug!(response_bytes = text.len(), "parsing model response");
    let clean = text.trim();
    let value =
        parse_json_response(clean).with_context(|| format!("model returned non-JSON: {text}"))?;

    match value["type"].as_str() {
        Some("final") => Ok(Reply::Final {
            text: value["text"].as_str().unwrap_or_default().into(),
            summary: shorten(value["summary"].as_str().unwrap_or_default(), 80),
            wait_user: value["wait_user"].as_bool().unwrap_or(false),
        }),
        Some("tool") => Ok(Reply::Tool {
            call: Call {
                tool: value["tool"].as_str().unwrap_or_default().into(),
                args: value["args"].clone(),
            },
            summary: value["summary"].as_str().unwrap_or_default().into(),
        }),
        _ => bail!("unknown model response type: {clean}"),
    }
}

fn parse_json_response(clean: &str) -> Result<Value> {
    let values = extract_json_objects(clean)?;

    let Some(first) = values.first().cloned() else {
        bail!("empty model response");
    };
    if values.iter().all(|value| value == &first) {
        Ok(first)
    } else {
        bail!("model returned multiple different JSON replies");
    }
}

fn extract_json_objects(text: &str) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    let mut index = 0;

    while let Some(relative_start) = text[index..].find('{') {
        let start = index + relative_start;
        let slice = &text[start..];
        let mut stream = serde_json::Deserializer::from_str(slice).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) if value.is_object() => {
                let end = start + stream.byte_offset();
                values.push(value);
                index = end.max(start + 1);
            }
            Some(Ok(_)) => {
                index = start + 1;
            }
            Some(Err(err)) => {
                debug!(error = %err, byte = start, "skipping invalid JSON candidate");
                index = start + 1;
            }
            None => {
                index = start + 1;
            }
        }
    }

    if values.is_empty() {
        warn!(response_bytes = text.len(), "model returned no JSON object");
    }
    Ok(values)
}

fn shorten(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn parses_openai_compatible_provider() {
        let provider = "openai-compatible".parse::<Provider>().unwrap();
        assert!(matches!(provider, Provider::OpenAiCompatible));
        assert_eq!(provider.to_string(), "openai-compatible");
    }

    #[test]
    fn builds_chat_completions_endpoint() {
        assert_eq!(
            chat_completions_endpoint("https://example.com/v1"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://example.com/v1/"),
            "https://example.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_endpoint("https://example.com/v1/chat/completions"),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn retries_only_transient_statuses() {
        assert!(should_retry_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(should_retry_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(should_retry_status(StatusCode::GATEWAY_TIMEOUT));
        assert!(!should_retry_status(StatusCode::BAD_REQUEST));
        assert!(!should_retry_status(StatusCode::UNAUTHORIZED));
        assert!(!should_retry_status(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE
        ));
    }

    #[test]
    fn parses_retry_after_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("2"));

        assert_eq!(retry_after(&headers), Some(Duration::from_secs(2)));
    }

    #[test]
    fn request_diagnostics_are_size_only() {
        let diagnostics = RequestDiagnostics::new(100, 20, 30, 40, 50);

        assert_eq!(
            diagnostics.to_string(),
            "payload_bytes=100, system_bytes=20, index_bytes=30, bodies_bytes=40, auth_header_bytes=50"
        );
    }

    #[test]
    fn parses_repeated_identical_json_reply() {
        let text = r#"{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}
{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}"#;

        match parse_reply(text).unwrap() {
            Reply::Tool { call, summary } => {
                assert_eq!(call.tool, "probe");
                assert_eq!(call.args["x"], 14);
                assert_eq!(summary, "probe f(14)");
            }
            other => panic!("expected tool reply, got {other:?}"),
        }
    }

    #[test]
    fn parses_single_json_reply_in_markdown_fence() {
        let text = r#"```json
{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}
```"#;

        match parse_reply(text).unwrap() {
            Reply::Tool { call, summary } => {
                assert_eq!(call.tool, "probe");
                assert_eq!(call.args["x"], 14);
                assert_eq!(summary, "probe f(14)");
            }
            other => panic!("expected tool reply, got {other:?}"),
        }
    }

    #[test]
    fn parses_single_json_reply_with_surrounding_text() {
        let text = r#"Here is the tool call:
{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}
I will wait for the result."#;

        match parse_reply(text).unwrap() {
            Reply::Tool { call, summary } => {
                assert_eq!(call.tool, "probe");
                assert_eq!(call.args["x"], 14);
                assert_eq!(summary, "probe f(14)");
            }
            other => panic!("expected tool reply, got {other:?}"),
        }
    }

    #[test]
    fn rejects_multiple_different_json_replies() {
        let text = r#"{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}
{"type":"tool","tool":"probe","args":{"x":15},"summary":"probe f(15)"}"#;

        let err = parse_reply(text).unwrap_err().to_string();
        assert!(err.contains("model returned non-JSON"));
    }

    #[test]
    fn rejects_tool_call_plus_final_reply() {
        let text = r#"{"type":"tool","tool":"probe","args":{"x":14},"summary":"probe f(14)"}
{"type":"final","text":"","summary":"waiting for probe result","wait_user":true}"#;

        let err = format!("{:#}", parse_reply(text).unwrap_err());
        assert!(err.contains("model returned multiple different JSON replies"));
    }

    fn model_error(options: ModelOptions) -> String {
        match create_model_with_options(Provider::OpenAiCompatible, options) {
            Ok(_) => panic!("expected model creation to fail"),
            Err(err) => err.to_string(),
        }
    }

    #[test]
    fn openai_compatible_requires_model_and_base_url() {
        let missing_all = model_error(ModelOptions::default());
        assert_eq!(
            missing_all,
            "--openai-compatible-base-url or _config.openai_compatible.base_url is required"
        );

        let missing_model = model_error(ModelOptions {
            openai_compatible_base_url: Some("https://example.com/v1".into()),
            ..ModelOptions::default()
        });
        assert_eq!(
            missing_model,
            "--model or _config.openai_compatible.model is required"
        );

        let blank_model = model_error(ModelOptions {
            model: Some(" ".into()),
            openai_compatible_base_url: Some("https://example.com/v1".into()),
            ..ModelOptions::default()
        });
        assert_eq!(
            blank_model,
            "--model or _config.openai_compatible.model is required"
        );
    }
}
