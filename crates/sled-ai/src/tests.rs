use crate::http::{RequestDiagnostics, retry_after, should_retry_status};
use crate::reply::{parse_reply, sled_reply_json_schema};
use crate::*;
use reqwest::{
    StatusCode,
    header::{HeaderMap, HeaderValue, RETRY_AFTER},
};
use sled_core::Reply;
use std::time::Duration;

#[test]
fn parses_openai_compatible_provider() {
    let provider = "openai-compatible".parse::<Provider>().unwrap();
    assert!(matches!(provider, Provider::OpenAiCompatible));
    assert_eq!(provider.to_string(), "openai-compatible");
}

#[test]
fn parses_openai_reasoning_effort() {
    let effort = "low".parse::<OpenAiReasoningEffort>().unwrap();

    assert_eq!(effort, OpenAiReasoningEffort::Low);
    assert_eq!(effort.to_string(), "low");
}

#[test]
fn parses_anthropic_effort_and_thinking() {
    assert_eq!(
        "xhigh".parse::<AnthropicEffort>().unwrap(),
        AnthropicEffort::XHigh
    );
    assert_eq!(
        "adaptive".parse::<AnthropicThinking>().unwrap(),
        AnthropicThinking::Adaptive
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
fn parses_tool_reply_with_args_json() {
    let text = r#"{
  "type": "tool",
  "text": "",
  "summary": "probe f(14)",
  "wait_user": false,
  "tool": "probe",
  "args_json": "{\"x\":14}"
}"#;

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
fn rejects_args_json_that_is_not_an_object() {
    let text = r#"{
  "type": "tool",
  "text": "",
  "summary": "bad args",
  "wait_user": false,
  "tool": "probe",
  "args_json": "[1,2,3]"
}"#;

    let err = format!("{:#}", parse_reply(text).unwrap_err());
    assert!(err.contains("args_json must contain a JSON object"));
}

#[test]
fn sled_reply_schema_uses_string_encoded_args() {
    let schema = sled_reply_json_schema();

    assert_eq!(schema["properties"]["args_json"]["type"], "string");
    assert!(schema["properties"]["args"].is_null());
    assert_eq!(schema["additionalProperties"], false);
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
