use super::support::{create_two_awaiting_slots, temp_dir};
use crate::storage::{create_slot, read_message};
use crate::{Call, Message, Status, ToolSuspension, WriteOptions, say, say_with_options};
use serde_json::json;
use std::fs;

#[test]
fn say_creates_done_user_message() {
    let dir = temp_dir();
    let path = say(&dir, "hello").unwrap();
    assert_eq!(path.file_name().unwrap(), "0001.user.done.json5");
    let msg = read_message(&path).unwrap();
    assert_eq!(msg.role, "user");
    assert_eq!(msg.body, "hello");
}

#[test]
fn say_with_body_mirror_option_writes_markdown_mirror() {
    let dir = temp_dir();
    let path = say_with_options(&dir, "hello\nworld", WriteOptions { body_mirror: true }).unwrap();
    assert_eq!(path.file_name().unwrap(), "0001.user.done.json5");
    assert_eq!(
        fs::read_to_string(dir.join("0001.user.done.md")).unwrap(),
        "hello\nworld"
    );
}

#[test]
fn say_rejects_two_awaiting_slots() {
    let dir = temp_dir();
    create_two_awaiting_slots(&dir);

    let err = say(&dir, "answer").unwrap_err().to_string();
    assert!(err.contains("more than one non-terminal file"));
}

#[test]
fn say_answers_suspended_tool_awaiting() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Awaiting,
        &Message {
            role: "tool".into(),
            summary: "ask".into(),
            call: Some(Call {
                tool: "ask_human".into(),
                args: json!({}),
            }),
            suspension: Some(ToolSuspension {
                request: json!({"prompt": "answer required"}),
            }),
            ..Message::default()
        },
    )
    .unwrap();

    let path = say(&dir, "human answer").unwrap();
    assert_eq!(path.file_name().unwrap(), "0001.tool.done.json5");

    let msg = read_message(&path).unwrap();
    let suspension = msg.suspension.unwrap();
    assert_eq!(suspension.request["prompt"], "answer required");
    assert_eq!(msg.result.unwrap()["answer"], "human answer");
}
