use crate::storage::{read_message, scan, validate_single_open};
use crate::{Message, Slot};
use anyhow::Result;
use std::path::Path;
use tracing::debug;

pub fn status_report(dir: &Path) -> Result<String> {
    debug!(dir = %dir.display(), "building status report");
    let slots = scan(dir)?;
    let open = validate_single_open(&slots)?;
    let mut report = String::new();
    report.push_str(&format!("slots: {}\n", slots.len()));
    if let Some(slot) = open {
        report.push_str(&format!("non-terminal: {}\n", slot_file_label(slot)));
    } else {
        report.push_str("non-terminal: none\n");
    }
    if let Some(last) = slots.last() {
        let msg = read_message(&last.path).unwrap_or_default();
        let role = message_or_slot_role(&msg, last);
        report.push_str(&format!(
            "last: {} [{}] {}\n",
            slot_file_label(last),
            role,
            empty_as(&msg.summary, "(no summary)")
        ));
    }
    Ok(report)
}

fn empty_as<'a>(value: &'a str, fallback: &'a str) -> &'a str {
    if value.is_empty() { fallback } else { value }
}

fn message_or_slot_role(msg: &Message, slot: &Slot) -> String {
    if !msg.role.is_empty() {
        canonical_role(&msg.role)
    } else {
        slot.role
            .as_deref()
            .map(canonical_role)
            .unwrap_or_else(|| "none".into())
    }
}

fn canonical_role(role: &str) -> String {
    role.into()
}

fn slot_file_label(slot: &Slot) -> String {
    match slot.role.as_deref() {
        Some(role) => format!(
            "{:04}.{}.{}",
            slot.num,
            canonical_role(role),
            slot.status.as_str()
        ),
        None => format!("{:04}.{}", slot.num, slot.status.as_str()),
    }
}
