use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use sled_core::{Call, Context, Model, Reply};
use std::io::{self, Write};
use tracing::{debug, info, warn};

#[derive(Clone, Copy, Debug)]
pub enum Provider {
    Operator,
    OpenAi,
    Anthropic,
}

impl std::str::FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "operator" => Self::Operator,
            "openai" => Self::OpenAi,
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
            Self::Anthropic => "anthropic",
        })
    }
}

pub fn create_model(provider: Provider, model: Option<String>) -> Result<Box<dyn Model>> {
    match provider {
        Provider::Operator => {
            info!(provider = %provider, "creating model client");
            Ok(Box::new(OperatorModel))
        }
        Provider::OpenAi => {
            let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;
            let model = model.unwrap_or_else(|| "gpt-5.5".into());
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(OpenAiModel {
                client: Client::new(),
                api_key,
                model,
            }))
        }
        Provider::Anthropic => {
            let api_key =
                std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is required")?;
            let model = model.unwrap_or_else(|| "claude-sonnet-4-6".into());
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(AnthropicModel {
                client: Client::new(),
                api_key,
                model,
            }))
        }
    }
}

pub struct OperatorModel;

#[async_trait]
impl Model for OperatorModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(
            provider = "operator",
            "waiting for operator assistant input"
        );
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
}

#[async_trait]
impl Model for OpenAiModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = "openai", model = %self.model, "sending model request");
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
        let response: Value = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user}
                ]
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        info!(provider = "openai", model = %self.model, "received model response");
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("empty OpenAI response: {response}"))?;
        parse_reply(text)
    }
}

pub struct AnthropicModel {
    client: Client,
    api_key: String,
    model: String,
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
        let response: Value = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({
                "model": self.model,
                "max_tokens": 4096,
                "system": system,
                "messages": [{"role": "user", "content": user}]
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        info!(provider = "anthropic", model = %self.model, "received model response");
        let text = response["content"][0]["text"]
            .as_str()
            .ok_or_else(|| anyhow!("empty Anthropic response: {response}"))?;
        parse_reply(text)
    }
}

fn parse_reply(text: &str) -> Result<Reply> {
    debug!(response_bytes = text.len(), "parsing model response");
    let clean = text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let value: Value = match serde_json::from_str(clean) {
        Ok(value) => value,
        Err(err) => {
            warn!(error = %err, response_bytes = text.len(), "model returned non-JSON");
            return Err(err).with_context(|| format!("model returned non-JSON: {text}"));
        }
    };

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

fn shorten(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
