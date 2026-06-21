use crate::providers::anthropic::{
    anthropic_messages_payload, anthropic_response_reply, anthropic_response_text,
};
use crate::{AnthropicEffort, AnthropicThinking};
use serde_json::json;
use sled_core::Reply;

#[test]
fn builds_anthropic_payload_with_effort_and_adaptive_thinking() {
    let payload = anthropic_messages_payload(
        "claude-sonnet-4-6",
        "system prompt",
        "user context",
        Some(AnthropicEffort::Medium),
        Some(AnthropicThinking::Adaptive),
    );

    assert_eq!(payload["model"], "claude-sonnet-4-6");
    assert_eq!(payload["output_config"]["effort"], "medium");
    assert_eq!(payload["thinking"]["type"], "adaptive");
    assert_eq!(payload["tool_choice"]["type"], "tool");
    assert_eq!(payload["tool_choice"]["name"], "sled_reply");
    assert_eq!(payload["tools"][0]["name"], "sled_reply");
    assert_eq!(payload["tools"][0]["strict"], true);
}

#[test]
fn extracts_anthropic_text_after_thinking_blocks() {
    let response = json!({
        "content": [
            {"type": "thinking", "thinking": "hidden summary"},
            {"type": "text", "text": "{\"type\":\"final\",\"text\":\"ok\"}"}
        ]
    });

    assert_eq!(
        anthropic_response_text(&response).as_deref(),
        Some("{\"type\":\"final\",\"text\":\"ok\"}")
    );
}

#[test]
fn extracts_anthropic_sled_reply_tool_use() {
    let response = json!({
        "content": [
            {
                "type": "tool_use",
                "name": "sled_reply",
                "input": {
                    "type": "tool",
                    "text": "",
                    "summary": "probe x",
                    "wait_user": false,
                    "tool": "probe",
                    "args_json": "{\"x\":14}"
                }
            }
        ]
    });

    match anthropic_response_reply(&response).unwrap().unwrap() {
        Reply::Tool { call, summary } => {
            assert_eq!(call.tool, "probe");
            assert_eq!(call.args["x"], 14);
            assert_eq!(summary, "probe x");
        }
        other => panic!("expected tool reply, got {other:?}"),
    }
}
