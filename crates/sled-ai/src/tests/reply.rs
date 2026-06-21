use crate::reply::{parse_reply, sled_reply_json_schema};
use sled_core::Reply;

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
