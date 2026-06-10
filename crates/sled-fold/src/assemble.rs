use anyhow::Result;
use sled_core::{Context, Message, Slot, read_message};
use tracing::debug;

struct ContextRow {
    index_line: String,
    body_section: String,
    empty_open_slot: bool,
}

pub(crate) fn assemble(slots: &[Slot], recent_messages: Option<usize>) -> Result<Context> {
    debug!(
        slots = slots.len(),
        recent_messages, "assembling model context"
    );
    let rows = context_rows(slots)?;
    let recent_from = recent_messages.map(|k| message_row_count(&rows).saturating_sub(k));
    let mut index = String::new();
    let mut bodies = String::new();
    let mut message_idx = 0usize;

    for row in rows {
        let is_message = !row.empty_open_slot;
        let include = row.empty_open_slot
            || recent_from.is_none_or(|from| !is_message || message_idx >= from);
        if is_message {
            message_idx += 1;
        }
        if !include {
            continue;
        }
        index.push_str(&row.index_line);
        bodies.push_str(&row.body_section);
    }

    Ok(Context { index, bodies })
}

pub(crate) fn assemble_recent_bytes(slots: &[Slot], budget: usize) -> Result<Context> {
    debug!(
        slots = slots.len(),
        budget, "assembling byte-budgeted model context"
    );
    let rows = context_rows(slots)?;
    let mut selected = vec![false; rows.len()];

    let mut used = 0usize;
    for (idx, row) in rows.iter().enumerate().rev() {
        if row.empty_open_slot {
            selected[idx] = true;
            continue;
        }
        let section_len = row.body_section.len();
        if used + section_len > budget {
            break;
        }
        used += section_len;
        selected[idx] = true;
    }

    Ok(Context {
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
    })
}

fn context_rows(slots: &[Slot]) -> Result<Vec<ContextRow>> {
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

fn message_row_count(rows: &[ContextRow]) -> usize {
    rows.iter().filter(|row| !row.empty_open_slot).count()
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
