use crate::storage::{
    create_slot_with_options, read_message, scan, set_status, validate_single_open,
    write_message_with_options,
};
use crate::system::ensure_dialog_system_prompt;
use crate::{Message, Slot, Status, WriteOptions};
use anyhow::{Result, bail};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

pub fn say(dir: &Path, text: &str) -> Result<PathBuf> {
    say_with_options(dir, text, WriteOptions::default())
}

pub fn say_with_options(dir: &Path, text: &str, write_options: WriteOptions) -> Result<PathBuf> {
    info!(dir = %dir.display(), "adding user message");
    fs::create_dir_all(dir)?;
    ensure_dialog_system_prompt(dir)?;
    let slots = scan(dir)?;
    let open = validate_single_open(&slots)?;

    if let Some(slot) = open {
        if slot.status != Status::Awaiting {
            warn!(
                active = %slot.path.display(),
                "cannot add user message while another slot is active"
            );
            bail!(
                "cannot add user message: currently active {}",
                slot.path.display()
            );
        }
        let mut existing = read_message(&slot.path).unwrap_or_default();
        if is_tool_awaiting(slot, &existing) {
            if existing.suspension.is_none() {
                bail!(
                    "cannot answer tool awaiting without suspension: {}",
                    slot.path.display()
                );
            }
            existing.result = Some(json_tool_answer(text));
            write_message_with_options(&slot.path, &existing, write_options)?;
            return set_status(dir, slot, Status::Done);
        }
        let msg = Message {
            role: "user".into(),
            summary: shorten(text, 80),
            body: text.into(),
            ..Message::default()
        };
        write_message_with_options(&slot.path, &msg, write_options)?;
        return set_status(dir, slot, Status::Done);
    }

    let next_num = slots.last().map(|slot| slot.num + 1).unwrap_or(1);
    create_slot_with_options(
        dir,
        next_num,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: shorten(text, 80),
            body: text.into(),
            ..Message::default()
        },
        write_options,
    )
}

fn is_tool_awaiting(slot: &Slot, msg: &Message) -> bool {
    slot.status == Status::Awaiting
        && (msg.role == "tool" || slot.role.as_deref() == Some("tool"))
        && msg.filled()
}

fn json_tool_answer(text: &str) -> Value {
    serde_json::json!({
        "ok": true,
        "answer": text,
    })
}

fn shorten(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
