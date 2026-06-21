use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use sled_core::{Call, Reply};
use tracing::{debug, warn};

pub(crate) fn parse_reply(text: &str) -> Result<Reply> {
    debug!(response_bytes = text.len(), "parsing model response");
    let clean = text.trim();
    let value =
        parse_json_response(clean).with_context(|| format!("model returned non-JSON: {text}"))?;

    parse_reply_value(&value).with_context(|| format!("invalid model response object: {value}"))
}

pub(crate) fn parse_reply_value(value: &Value) -> Result<Reply> {
    match value["type"].as_str() {
        Some("final") => Ok(Reply::Final {
            text: value["text"].as_str().unwrap_or_default().into(),
            summary: shorten(value["summary"].as_str().unwrap_or_default(), 80),
            wait_user: value["wait_user"].as_bool().unwrap_or(false),
        }),
        Some("tool") => Ok(Reply::Tool {
            call: Call {
                tool: value["tool"].as_str().unwrap_or_default().into(),
                args: reply_args(value)?,
            },
            summary: value["summary"].as_str().unwrap_or_default().into(),
        }),
        _ => bail!("unknown model response type"),
    }
}

fn reply_args(value: &Value) -> Result<Value> {
    if let Some(args_json) = value["args_json"].as_str() {
        if args_json.trim().is_empty() {
            return Ok(json!({}));
        }
        let args: Value = serde_json::from_str(args_json)
            .with_context(|| format!("args_json is not valid JSON: {args_json}"))?;
        if !args.is_object() {
            bail!("args_json must contain a JSON object");
        }
        return Ok(args);
    }

    if value.get("args").is_some() {
        return Ok(value["args"].clone());
    }

    Ok(json!({}))
}

pub(crate) fn sled_reply_json_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "type": {
                "type": "string",
                "enum": ["final", "tool"],
                "description": "Use final for assistant text, tool for exactly one tool call."
            },
            "text": {
                "type": "string",
                "description": "Final assistant text. Empty string for tool calls."
            },
            "summary": {
                "type": "string",
                "description": "Short summary, up to 80 characters."
            },
            "wait_user": {
                "type": "boolean",
                "description": "True only when a final answer needs a user reply. False for tool calls."
            },
            "tool": {
                "type": "string",
                "description": "Tool name for tool calls. Empty string for final answers."
            },
            "args_json": {
                "type": "string",
                "description": "A compact JSON object string with tool arguments. Use \"{}\" for final answers."
            }
        },
        "required": ["type", "text", "summary", "wait_user", "tool", "args_json"],
        "additionalProperties": false
    })
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
