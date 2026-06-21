use anyhow::Result;
use sled_core::{Context, Message, Slot, read_message};

pub(crate) struct ContextRow {
    pub(crate) index_line: String,
    pub(crate) body_section: String,
    pub(crate) empty_open_slot: bool,
}

pub(crate) fn context_rows(slots: &[Slot]) -> Result<Vec<ContextRow>> {
    slots
        .iter()
        .map(|slot| {
            let msg = read_slot_message(slot)?;
            let role = message_or_slot_role(&msg, slot);
            Ok(ContextRow {
                index_line: index_line(slot, &role, &msg),
                body_section: body_section(slot, &role, &msg),
                empty_open_slot: !slot.status.terminal() && !msg.filled(),
            })
        })
        .collect()
}

pub(crate) fn message_row_count(rows: &[ContextRow]) -> usize {
    rows.iter().filter(|row| !row.empty_open_slot).count()
}

pub(crate) fn context_from_selected(rows: &[ContextRow], selected: &[bool]) -> Context {
    Context {
        index: rows
            .iter()
            .zip(selected.iter())
            .filter(|(_, selected)| **selected)
            .map(|(row, _)| row.index_line.as_str())
            .collect(),
        bodies: rows
            .iter()
            .zip(selected.iter())
            .filter(|(_, selected)| **selected)
            .map(|(row, _)| row.body_section.as_str())
            .collect(),
    }
}

fn read_slot_message(slot: &Slot) -> Result<Message> {
    if slot.path.exists() {
        read_message(&slot.path)
    } else {
        Ok(Message::default())
    }
}

fn message_or_slot_role(msg: &Message, slot: &Slot) -> String {
    if !msg.role.is_empty() {
        msg.role.clone()
    } else {
        slot.role.clone().unwrap_or_else(|| "none".into())
    }
}

fn empty_as<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn index_line(slot: &Slot, role: &str, msg: &Message) -> String {
    format!(
        "{:04} [{}] {} - {}\n",
        slot.num,
        role,
        slot.status.as_str(),
        empty_as(&msg.summary, "(no summary)")
    )
}

fn body_section(slot: &Slot, role: &str, msg: &Message) -> String {
    let mut section = String::new();
    section.push_str(&format!("--- {:04} [{}] ---\n", slot.num, role));
    if !msg.body.is_empty() {
        section.push_str(&msg.body);
        section.push('\n');
    }
    if let Some(call) = &msg.call {
        section.push_str(&format!("call: {} {}\n", call.tool, call.args));
    }
    if let Some(result) = &msg.result {
        section.push_str(&format!("result: {}\n", result));
    }
    if let Some(suspension) = &msg.suspension {
        section.push_str(&format!("suspension_request: {}\n", suspension.request));
    }
    section.push('\n');
    section
}
