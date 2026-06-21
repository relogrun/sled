use crate::http::{RequestDiagnostics, send_model_request_with_retry};
use crate::reply::{parse_reply, parse_reply_value, sled_reply_json_schema};
use crate::{AnthropicEffort, AnthropicThinking, Provider};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use sled_core::{Context, Model, Reply};
use tracing::{debug, info};

pub(crate) struct AnthropicModel {
    client: Client,
    api_key: String,
    model: String,
    effort: Option<AnthropicEffort>,
    thinking: Option<AnthropicThinking>,
    temperature: Option<f32>,
}

impl AnthropicModel {
    pub(crate) fn new(
        api_key: String,
        model: String,
        effort: Option<AnthropicEffort>,
        thinking: Option<AnthropicThinking>,
        temperature: Option<f32>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            effort,
            thinking,
            temperature,
        }
    }
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
        let mut payload =
            anthropic_messages_payload(&self.model, system, &user, self.effort, self.thinking);
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
        anthropic_response_reply(&response)
            .ok_or_else(|| anyhow!("empty Anthropic response: {response}"))?
    }
}

pub(crate) fn anthropic_messages_payload(
    model: &str,
    system: &str,
    user: &str,
    effort: Option<AnthropicEffort>,
    thinking: Option<AnthropicThinking>,
) -> Value {
    let mut payload = json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": [{"role": "user", "content": user}],
        "tools": [anthropic_sled_reply_tool()],
        "tool_choice": {
            "type": "tool",
            "name": "sled_reply"
        }
    });
    if let Some(effort) = effort {
        payload["output_config"] = json!({ "effort": effort.to_string() });
    }
    if thinking == Some(AnthropicThinking::Adaptive) {
        payload["thinking"] = json!({ "type": "adaptive" });
    }
    payload
}

fn anthropic_sled_reply_tool() -> Value {
    json!({
        "name": "sled_reply",
        "description": "Return exactly one sled dialog reply: either a final answer or one sled tool call.",
        "input_schema": sled_reply_json_schema(),
        "strict": true
    })
}

pub(crate) fn anthropic_response_reply(response: &Value) -> Option<Result<Reply>> {
    for content in response["content"].as_array()? {
        if content["type"]
            .as_str()
            .is_some_and(|kind| kind == "tool_use")
            && content["name"]
                .as_str()
                .is_some_and(|name| name == "sled_reply")
        {
            return Some(parse_reply_value(&content["input"]));
        }
    }

    anthropic_response_text(response).map(|text| parse_reply(&text))
}

pub(crate) fn anthropic_response_text(response: &Value) -> Option<String> {
    let mut texts = Vec::new();
    for content in response["content"].as_array()? {
        if content["type"].as_str().is_some_and(|kind| kind == "text") {
            if let Some(text) = content["text"].as_str() {
                texts.push(text);
            }
        }
    }
    let text = texts.join("\n");
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}
