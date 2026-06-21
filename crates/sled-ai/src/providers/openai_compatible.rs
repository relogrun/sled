use crate::Provider;
use crate::http::{RequestDiagnostics, send_model_request_with_retry};
use crate::reply::parse_reply;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use sled_core::{Context, Model, Reply};
use tracing::{debug, info};

pub(crate) struct OpenAiCompatibleModel {
    client: Client,
    api_key: String,
    model: String,
    endpoint: String,
    temperature: Option<f32>,
}

impl OpenAiCompatibleModel {
    pub(crate) fn new(
        api_key: String,
        model: String,
        endpoint: String,
        temperature: Option<f32>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            endpoint,
            temperature,
        }
    }
}

#[async_trait]
impl Model for OpenAiCompatibleModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = "openai-compatible", model = %self.model, "sending model request");
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
        let response: Value = send_model_request_with_retry(
            Provider::OpenAiCompatible,
            &self.model,
            diagnostics,
            || {
                self.client
                    .post(&self.endpoint)
                    .bearer_auth(&self.api_key)
                    .json(&payload)
            },
        )
        .await?
        .json()
        .await?;

        info!(provider = "openai-compatible", model = %self.model, "received model response");
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| anyhow!("empty OpenAI-compatible response: {response}"))?;
        parse_reply(text)
    }
}

pub(crate) fn chat_completions_endpoint(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/chat/completions") {
        trimmed.into()
    } else {
        format!("{trimmed}/chat/completions")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
