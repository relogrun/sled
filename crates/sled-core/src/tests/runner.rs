use super::support::{
    FakeTools, NoopFold, NoopModel, PanicTools, SuspendTools, create_two_awaiting_slots, temp_dir,
};
use crate::storage::{create_slot, read_message, scan};
use crate::{Call, Message, RuntimeOptions, Status, StepOutcome, ToolSuspension, step};
use serde_json::json;
use std::fs;

#[tokio::test]
async fn runner_rejects_two_awaiting_slots() {
    let dir = temp_dir();
    create_two_awaiting_slots(&dir);

    let err = step(
        &dir,
        &NoopModel,
        &PanicTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap_err()
    .to_string();
    assert!(err.contains("more than one non-terminal file"));
}

#[tokio::test]
async fn pending_tool_uses_injected_executor() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Pending,
        &Message {
            role: "tool".into(),
            summary: "fake".into(),
            call: Some(Call {
                tool: "fake".into(),
                args: json!({}),
            }),
            ..Message::default()
        },
    )
    .unwrap();

    let outcome = step(
        &dir,
        &NoopModel,
        &FakeTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Continue));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].status, Status::Done);
    let msg = read_message(&slots[0].path).unwrap();
    assert_eq!(msg.result.unwrap()["tool"], "fake");
}

#[tokio::test]
async fn pending_tool_with_result_is_closed_without_reexecution() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Pending,
        &Message {
            role: "tool".into(),
            summary: "already done".into(),
            call: Some(Call {
                tool: "fake".into(),
                args: json!({}),
            }),
            result: Some(json!({"ok": true})),
            ..Message::default()
        },
    )
    .unwrap();

    let outcome = step(
        &dir,
        &NoopModel,
        &PanicTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Continue));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Done);
    let msg = read_message(&slots[0].path).unwrap();
    assert_eq!(msg.result.unwrap()["ok"], true);
}

#[tokio::test]
async fn pending_tool_can_suspend_into_tool_awaiting() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Pending,
        &Message {
            role: "tool".into(),
            summary: "ask".into(),
            call: Some(Call {
                tool: "ask_human".into(),
                args: json!({}),
            }),
            ..Message::default()
        },
    )
    .unwrap();

    let outcome = step(
        &dir,
        &NoopModel,
        &SuspendTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Awaiting(_)));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Awaiting);
    assert_eq!(
        slots[0].path.file_name().unwrap(),
        "0001.tool.awaiting.json5"
    );
    let msg = read_message(&slots[0].path).unwrap();
    let suspension = msg.suspension.unwrap();
    assert_eq!(suspension.request["tool"], "ask_human");
    assert!(msg.result.is_none());
}

#[tokio::test]
async fn pending_tool_with_suspension_recovers_to_awaiting_without_reexecution() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Pending,
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

    let outcome = step(
        &dir,
        &NoopModel,
        &PanicTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Awaiting(_)));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Awaiting);
    assert_eq!(
        slots[0].path.file_name().unwrap(),
        "0001.tool.awaiting.json5"
    );
}

#[tokio::test]
async fn filled_user_awaiting_recovers_to_done() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Awaiting,
        &Message {
            role: "user".into(),
            summary: "hello".into(),
            body: "hello".into(),
            ..Message::default()
        },
    )
    .unwrap();

    let outcome = step(
        &dir,
        &NoopModel,
        &PanicTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Continue));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Done);
    assert_eq!(slots[0].path.file_name().unwrap(), "0001.user.done.json5");
}

#[tokio::test]
async fn completed_tool_awaiting_recovers_to_done() {
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
            result: Some(json!({"ok": true, "answer": "human answer"})),
            suspension: Some(ToolSuspension {
                request: json!({"prompt": "answer required"}),
            }),
            ..Message::default()
        },
    )
    .unwrap();

    let outcome = step(
        &dir,
        &NoopModel,
        &PanicTools,
        &NoopFold,
        RuntimeOptions::default(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, StepOutcome::Continue));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Done);
    assert_eq!(slots[0].path.file_name().unwrap(), "0001.tool.done.json5");
    let msg = read_message(&slots[0].path).unwrap();
    assert_eq!(msg.result.unwrap()["answer"], "human answer");
}
