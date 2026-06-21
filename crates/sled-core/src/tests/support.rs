use crate::storage::create_slot;
use crate::{
    Call, Context, Fold, Message, Model, Reply, Slot, Status, ToolExecutor, ToolResult,
    ToolSuspension,
};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) struct NoopModel;

#[async_trait]
impl Model for NoopModel {
    async fn complete(&self, _system: &str, _context: &Context) -> Result<Reply> {
        unreachable!("model should not be called in this test")
    }
}

pub(crate) struct NoopFold;

impl Fold for NoopFold {
    fn assemble(&self, _slots: &[Slot]) -> Result<Context> {
        Ok(Context {
            index: String::new(),
            bodies: String::new(),
        })
    }
}

pub(crate) struct FakeTools;

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

pub(crate) struct PanicTools;

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

pub(crate) struct SuspendTools;

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

pub(crate) fn temp_dir() -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sled-core-test-{id}-{seq}"))
}

pub(crate) fn create_two_awaiting_slots(dir: &Path) {
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
