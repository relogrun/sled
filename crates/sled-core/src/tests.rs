use crate::model_input::{body_sections, estimate_tokens, fit_model_input};
use crate::storage::{MessageWriteFormat, mirror_file_name, tmp_path, write_message_with_format};
use crate::system::{DEFAULT_SYSTEM_PROMPT, resolve_system_prompt};
use crate::*;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

struct NoopModel;

#[async_trait]
impl Model for NoopModel {
    async fn complete(&self, _system: &str, _context: &Context) -> Result<Reply> {
        unreachable!("model should not be called in this test")
    }
}

struct NoopFold;

impl Fold for NoopFold {
    fn assemble(&self, _slots: &[Slot]) -> Result<Context> {
        Ok(Context {
            index: String::new(),
            bodies: String::new(),
        })
    }
}

struct FakeTools;

#[async_trait]
impl ToolExecutor for FakeTools {
    async fn execute(
        &self,
        _dialog_dir: &Path,
        _slots: &[Slot],
        call: &Call,
    ) -> Result<ToolResult> {
        Ok(ToolResult::completed(
            json!({"ok": true, "tool": call.tool}),
        ))
    }
}

struct PanicTools;

#[async_trait]
impl ToolExecutor for PanicTools {
    async fn execute(
        &self,
        _dialog_dir: &Path,
        _slots: &[Slot],
        _call: &Call,
    ) -> Result<ToolResult> {
        unreachable!("tool should not be executed")
    }
}

struct SuspendTools;

#[async_trait]
impl ToolExecutor for SuspendTools {
    async fn execute(
        &self,
        _dialog_dir: &Path,
        _slots: &[Slot],
        call: &Call,
    ) -> Result<ToolResult> {
        Ok(ToolResult::suspended(json!({
            "tool": call.tool,
            "prompt": "answer required"
        })))
    }
}

#[test]
fn context_limit_keeps_newest_body_sections() {
    let first = "--- 0001 [user] ---\nfirst first first\n\n";
    let second = "--- 0002 [assistant] ---\nsecond second second\n\n";
    let third = "--- 0003 [user] ---\nthird third third\n\n";
    let input = ModelInput {
            system: String::new(),
            context: Context {
                index: "0001 [user] done - first\n0002 [assistant] done - second\n0003 [user] done - third\n".into(),
                bodies: format!("{first}{second}{third}"),
            },
        };
    let budget = estimate_tokens(input.context.index.len() + second.len() + third.len());

    let limited = fit_model_input(
        input,
        ContextLimit {
            context_window_tokens: budget,
            context_ratio: 1.0,
        },
    )
    .unwrap();

    assert_eq!(
        limited.context.index,
        "0001 [user] done - first\n0002 [assistant] done - second\n0003 [user] done - third\n"
    );
    assert!(!limited.context.bodies.contains("first first"));
    assert!(limited.context.bodies.contains("second second"));
    assert!(limited.context.bodies.contains("third third"));
}

#[test]
fn context_limit_rejects_oversized_system_and_index() {
    let err = fit_model_input(
        ModelInput {
            system: "system text that is too large".into(),
            context: Context {
                index: "index text that is too large".into(),
                bodies: "--- 0001 [user] ---\nbody\n\n".into(),
            },
        },
        ContextLimit {
            context_window_tokens: 1,
            context_ratio: 1.0,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("model input exceeds context budget even without bodies"));
}

#[test]
fn context_limit_rejects_oversized_newest_body_section() {
    let err = fit_model_input(
        ModelInput {
            system: String::new(),
            context: Context {
                index: "0001 [user] done - first\n".into(),
                bodies: "--- 0001 [user] ---\nthis latest body section is too large\n\n".into(),
            },
        },
        ContextLimit {
            context_window_tokens: 8,
            context_ratio: 1.0,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("newest body section exceeds context budget"));
}

#[test]
fn body_sections_ignore_markdown_rules_inside_body_text() {
    let bodies = "--- 0001 [user] ---\nfirst\n--- not a sled section\nstill first\n\n--- 0002 [assistant] ---\nsecond\n\n";
    let sections = body_sections(bodies);

    assert_eq!(sections.len(), 2);
    assert!(sections[0].contains("--- not a sled section"));
    assert!(sections[1].contains("second"));
}

fn temp_dir() -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sled-core-test-{id}-{seq}"))
}

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
fn open_slot_filenames_match_known_roles() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();

    let running = create_slot(&dir, 1, Status::Running, &Message::default()).unwrap();
    let awaiting = create_slot(&dir, 2, Status::Awaiting, &Message::default()).unwrap();

    assert_eq!(running.file_name().unwrap(), "0001.running.json5");
    assert_eq!(awaiting.file_name().unwrap(), "0002.user.awaiting.json5");
}

#[test]
fn system_prompt_is_internal_prompt_plus_system_config() {
    let system = resolve_system_prompt(
        &SystemConfig {
            prompt: "Dialog prompt.".into(),
        },
        &SystemPromptFragments::default(),
    );

    assert!(system.starts_with("=== Sled Protocol ===\n"));
    assert!(system.contains(DEFAULT_SYSTEM_PROMPT));
    assert!(system.ends_with("=== Dialog Instructions ===\nDialog prompt."));
}

#[test]
fn system_prompt_sections_are_ordered() {
    let system = resolve_system_prompt(
        &SystemConfig {
            prompt: "Dialog prompt.".into(),
        },
        &SystemPromptFragments::new(Some("Tool prompt.".into())),
    );

    let sled = system.find("=== Sled Protocol ===").unwrap();
    let tools = system.find("=== Available Tools ===").unwrap();
    let dialog = system.find("=== Dialog Instructions ===").unwrap();

    assert!(sled < tools);
    assert!(tools < dialog);
    assert!(system.contains("=== Available Tools ===\nTool prompt."));
}

#[test]
fn system_config_writer_includes_fragment_comment() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    write_system_prompt(&dir, "Dialog prompt.").unwrap();

    let text = fs::read_to_string(dir.join("_system.json5")).unwrap();
    assert!(text.starts_with("// Dialog-specific system prompt fragment."));
    assert!(text.contains("sled always prepends its internal protocol prompt"));
    assert_eq!(read_system_config(&dir).unwrap().prompt, "Dialog prompt.");
}

#[test]
fn default_system_config_does_not_shadow_existing_json_file() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("_system.json"), r#"{"prompt":"Legacy prompt."}"#).unwrap();

    write_default_system_config(&dir).unwrap();

    assert!(!dir.join("_system.json5").exists());
    assert_eq!(read_system_config(&dir).unwrap().prompt, "Legacy prompt.");
}

#[test]
fn rejects_two_open_slots() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(&dir, 1, Status::Awaiting, &Message::default()).unwrap();
    create_slot(&dir, 2, Status::Pending, &Message::default()).unwrap();
    let slots = scan(&dir).unwrap();
    assert!(validate_single_open(&slots).is_err());
}

fn create_two_awaiting_slots(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    create_slot(
        dir,
        1,
        Status::Awaiting,
        &Message {
            role: "user".into(),
            summary: "user input".into(),
            ..Message::default()
        },
    )
    .unwrap();
    create_slot(
        dir,
        2,
        Status::Awaiting,
        &Message {
            role: "tool".into(),
            summary: "tool input".into(),
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
}

#[test]
fn say_rejects_two_awaiting_slots() {
    let dir = temp_dir();
    create_two_awaiting_slots(&dir);

    let err = say(&dir, "answer").unwrap_err().to_string();
    assert!(err.contains("more than one non-terminal file"));
}

#[tokio::test]
async fn runner_rejects_two_awaiting_slots() {
    let dir = temp_dir();
    create_two_awaiting_slots(&dir);

    let err = step(&dir, &NoopModel, &PanicTools, &NoopFold)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("more than one non-terminal file"));
}

#[test]
fn markdown_mirror_writer_keeps_json_body_as_source_of_truth() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("0001.assistant.done.json5");
    let body = "first line\n\n- bullet\n";

    write_message_with_format(
        &path,
        &Message {
            role: "assistant".into(),
            summary: "multiline".into(),
            body: body.into(),
            ..Message::default()
        },
        MessageWriteFormat::JsonWithMarkdownMirror,
    )
    .unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"body\""));
    assert!(!raw.contains("\"body_file\""));

    let mirror_path = dir.join("0001.assistant.done.md");
    assert_eq!(fs::read_to_string(mirror_path).unwrap(), body);

    let msg = read_message(&path).unwrap();
    assert_eq!(msg.body, body);
}

#[test]
fn durable_write_replaces_file_and_removes_temp() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.txt");

    durable_write(&path, b"first").unwrap();
    durable_write(&path, b"second").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    assert!(!tmp_path(&path).exists());
}

#[test]
fn mirror_file_name_uses_done_status_and_md_suffix() {
    let path = Path::new("0002.running.json5");
    let msg = Message {
        role: "assistant".into(),
        ..Message::default()
    };
    assert_eq!(
        mirror_file_name(path, &msg).unwrap(),
        "0002.assistant.done.md"
    );
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

    let outcome = step(&dir, &NoopModel, &FakeTools, &NoopFold).await.unwrap();
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

    let outcome = step(&dir, &NoopModel, &PanicTools, &NoopFold)
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

    let outcome = step(&dir, &NoopModel, &SuspendTools, &NoopFold)
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

    let outcome = step(&dir, &NoopModel, &PanicTools, &NoopFold)
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

    let outcome = step(&dir, &NoopModel, &PanicTools, &NoopFold)
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

    let outcome = step(&dir, &NoopModel, &PanicTools, &NoopFold)
        .await
        .unwrap();
    assert!(matches!(outcome, StepOutcome::Continue));

    let slots = scan(&dir).unwrap();
    assert_eq!(slots[0].status, Status::Done);
    assert_eq!(slots[0].path.file_name().unwrap(), "0001.tool.done.json5");
    let msg = read_message(&slots[0].path).unwrap();
    assert_eq!(msg.result.unwrap()["answer"], "human answer");
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
