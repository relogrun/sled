use crate::{
    OpenAiReasoningEffort, Provider, RequestDiagnostics, parse_reply, parse_reply_value,
    send_model_request_with_retry, sled_reply_json_schema,
};
use anyhow::{Context as _, Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use sled_core::{Context, Model, Reply};
use tracing::{debug, info};

pub(crate) struct OpenAiResponsesModel {
    client: Client,
    api_key: String,
    model: String,
    openai_reasoning_effort: Option<OpenAiReasoningEffort>,
    temperature: Option<f32>,
}

impl OpenAiResponsesModel {
    pub(crate) fn new(
        api_key: String,
        model: String,
        openai_reasoning_effort: Option<OpenAiReasoningEffort>,
        temperature: Option<f32>,
    ) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            openai_reasoning_effort,
            temperature,
        }
    }
}

#[async_trait]
impl Model for OpenAiResponsesModel {
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
        let payload = openai_responses_payload(
            &self.model,
            system,
            &user,
            self.openai_reasoning_effort,
            self.temperature,
        );
        let diagnostics = RequestDiagnostics::new(
            serde_json::to_vec(&payload)?.len(),
            system.len(),
            context.index.len(),
            context.bodies.len(),
            "Bearer ".len() + self.api_key.len(),
        );
        let response: Value =
            send_model_request_with_retry(Provider::OpenAi, &self.model, diagnostics, || {
                self.client
                    .post("https://api.openai.com/v1/responses")
                    .bearer_auth(&self.api_key)
                    .json(&payload)
            })
            .await?
            .json()
            .await?;

        info!(provider = "openai", model = %self.model, "received model response");
        openai_response_reply(&response)
            .ok_or_else(|| anyhow!("empty OpenAI response: {response}"))?
    }
}

fn openai_responses_payload(
    model: &str,
    system: &str,
    user: &str,
    openai_reasoning_effort: Option<OpenAiReasoningEffort>,
    temperature: Option<f32>,
) -> Value {
    let mut payload = json!({
        "model": model,
        "instructions": system,
        "input": user,
        "tools": [openai_sled_reply_tool()],
        "tool_choice": {
            "type": "function",
            "name": "sled_reply"
        },
        "parallel_tool_calls": false
    });
    if let Some(openai_reasoning_effort) = openai_reasoning_effort {
        payload["reasoning"] = json!({ "effort": openai_reasoning_effort.to_string() });
    }
    if let Some(temperature) = temperature {
        payload["temperature"] = json!(temperature);
    }
    payload
}

fn openai_sled_reply_tool() -> Value {
    json!({
        "type": "function",
        "name": "sled_reply",
        "description": "Return exactly one sled dialog reply: either a final answer or one sled tool call.",
        "parameters": sled_reply_json_schema(),
        "strict": true
    })
}

fn openai_response_reply(response: &Value) -> Option<Result<Reply>> {
    for item in response["output"].as_array()? {
        if item["type"]
            .as_str()
            .is_some_and(|kind| kind == "function_call")
            && item["name"]
                .as_str()
                .is_some_and(|name| name == "sled_reply")
        {
            let Some(arguments) = item["arguments"].as_str() else {
                return Some(Err(anyhow!(
                    "OpenAI sled_reply function call missing arguments"
                )));
            };
            let value: Result<Value> = serde_json::from_str(arguments)
                .with_context(|| format!("OpenAI sled_reply arguments are not JSON: {arguments}"));
            return Some(value.and_then(|value| parse_reply_value(&value)));
        }
    }

    openai_response_text(response).map(|text| parse_reply(&text))
}

fn openai_response_text(response: &Value) -> Option<String> {
    if let Some(text) = response["output_text"].as_str() {
        if !text.trim().is_empty() {
            return Some(text.to_string());
        }
    }

    let mut texts = Vec::new();
    for item in response["output"].as_array()? {
        for content in item["content"].as_array().into_iter().flatten() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_openai_responses_payload_with_reasoning() {
        let payload = openai_responses_payload(
            "gpt-5.4-mini",
            "system prompt",
            "user context",
            Some(OpenAiReasoningEffort::Low),
            None,
        );

        assert_eq!(payload["model"], "gpt-5.4-mini");
        assert_eq!(payload["instructions"], "system prompt");
        assert_eq!(payload["input"], "user context");
        assert_eq!(payload["reasoning"]["effort"], "low");
        assert_eq!(payload["tools"][0]["type"], "function");
        assert_eq!(payload["tools"][0]["name"], "sled_reply");
        assert_eq!(payload["tools"][0]["strict"], true);
        assert_eq!(payload["tool_choice"]["type"], "function");
        assert_eq!(payload["tool_choice"]["name"], "sled_reply");
        assert_eq!(payload["parallel_tool_calls"], false);
        assert!(payload["messages"].is_null());
        assert!(payload["text"].is_null());
        assert!(payload["temperature"].is_null());
    }

    #[test]
    fn openai_responses_payload_omits_temperature_unless_explicit() {
        let omitted = openai_responses_payload("gpt-5.4-mini", "system", "user", None, None);
        let explicit = openai_responses_payload("gpt-5.4-mini", "system", "user", None, Some(1.0));

        assert!(omitted["temperature"].is_null());
        assert_eq!(explicit["temperature"], 1.0);
    }

    #[test]
    fn extracts_openai_output_text() {
        let response = json!({
            "output": [
                {
                    "content": [
                        {"type": "output_text", "text": "{\"type\":\"final\",\"text\":\"ok\"}"}
                    ]
                }
            ]
        });

        assert_eq!(
            openai_response_text(&response).as_deref(),
            Some("{\"type\":\"final\",\"text\":\"ok\"}")
        );
    }

    #[test]
    fn extracts_openai_sled_reply_function_call() {
        let response = json!({
            "output": [
                {
                    "type": "function_call",
                    "name": "sled_reply",
                    "arguments": r#"{
                        "type": "tool",
                        "text": "",
                        "summary": "probe x",
                        "wait_user": false,
                        "tool": "probe",
                        "args_json": "{\"x\":14}"
                    }"#
                }
            ]
        });

        match openai_response_reply(&response).unwrap().unwrap() {
            Reply::Tool { call, summary } => {
                assert_eq!(call.tool, "probe");
                assert_eq!(call.args["x"], 14);
                assert_eq!(summary, "probe x");
            }
            other => panic!("expected tool reply, got {other:?}"),
        }
    }
}
