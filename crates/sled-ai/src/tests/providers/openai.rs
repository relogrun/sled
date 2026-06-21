use crate::OpenAiReasoningEffort;
use crate::providers::openai::{
    openai_response_reply, openai_response_text, openai_responses_payload,
};
use serde_json::json;
use sled_core::Reply;

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
