use crate::{Message, Slot, Status, WriteOptions};
use anyhow::{Context as _, Result, anyhow, bail};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageWriteFormat {
    Json,
    JsonWithMarkdownMirror,
}

pub fn scan(dir: &Path) -> Result<Vec<Slot>> {
    debug!(dir = %dir.display(), "scanning dialog directory");
    let mut slots = Vec::new();
    for entry in
        fs::read_dir(dir).with_context(|| format!("missing directory {}", dir.display()))?
    {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".json5") else {
            continue;
        };
        let parts: Vec<&str> = stem.split('.').collect();
        let (num, role, status) = match parts.as_slice() {
            [num, status] => (*num, None, *status),
            [num, role, status] => (*num, Some((*role).to_string()), *status),
            _ => {
                continue;
            }
        };
        let (Ok(num), Some(status)) = (num.parse::<u32>(), Status::parse(status)) else {
            continue;
        };
        slots.push(Slot {
            num,
            role,
            status,
            path,
        });
    }
    slots.sort_by_key(|slot| slot.num);
    debug!(dir = %dir.display(), slots = slots.len(), "scan complete");
    Ok(slots)
}

pub fn slot_path(dir: &Path, num: u32, role: Option<&str>, status: Status) -> PathBuf {
    match role {
        Some(role) => dir.join(format!("{num:04}.{role}.{}.json5", status.as_str())),
        None => dir.join(format!("{num:04}.{}.json5", status.as_str())),
    }
}

pub fn read_message(path: &Path) -> Result<Message> {
    let text = fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(Message::default());
    }
    json5::from_str(&text).with_context(|| format!("could not parse JSON5 {}", path.display()))
}

pub fn write_message(path: &Path, msg: &Message) -> Result<()> {
    write_message_with_options(path, msg, WriteOptions::default())
}

pub fn write_message_with_options(path: &Path, msg: &Message, options: WriteOptions) -> Result<()> {
    let format = if options.body_mirror {
        MessageWriteFormat::JsonWithMarkdownMirror
    } else {
        MessageWriteFormat::Json
    };
    write_message_with_format(path, msg, format)
}

pub(crate) fn write_message_with_format(
    path: &Path,
    msg: &Message,
    format: MessageWriteFormat,
) -> Result<()> {
    debug!(
        path = %path.display(),
        role = %msg.role,
        has_call = msg.call.is_some(),
        has_result = msg.result.is_some(),
        "writing message"
    );
    let text = serde_json::to_string_pretty(msg)?;
    durable_write(path, text.as_bytes())?;
    if format == MessageWriteFormat::JsonWithMarkdownMirror && !msg.body.is_empty() {
        write_markdown_mirror(path, msg)?;
    }
    Ok(())
}

fn write_markdown_mirror(path: &Path, msg: &Message) -> Result<()> {
    let mirror_file = mirror_file_name(path, msg)?;
    let mirror_path = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&mirror_file);
    durable_write(&mirror_path, msg.body.as_bytes())
}

pub(crate) fn mirror_file_name(path: &Path, msg: &Message) -> Result<String> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("message path has no filename: {}", path.display()))?;
    let stem = name
        .strip_suffix(".json5")
        .ok_or_else(|| anyhow!("message path is not a JSON5 file: {}", path.display()))?;
    let mut parts = stem.split('.');
    let slot = parts
        .next()
        .ok_or_else(|| anyhow!("message filename has no slot: {}", path.display()))?;
    let role = if !msg.role.is_empty() {
        sanitize_role(&msg.role)
    } else {
        let rest: Vec<&str> = parts.collect();
        match rest.as_slice() {
            [role, _status] => sanitize_role(role),
            [_status] => bail!("message body mirror requires a role: {}", path.display()),
            _ => bail!("invalid message filename: {}", path.display()),
        }
    };
    Ok(format!("{slot}.{role}.done.md"))
}

pub fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = tmp_path(path);
    {
        let mut file =
            File::create(&tmp).with_context(|| format!("could not create {}", tmp.display()))?;
        use std::io::Write as _;
        file.write_all(bytes)
            .with_context(|| format!("could not write {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("could not sync {}", tmp.display()))?;
    }
    durable_rename(&tmp, path)
}

fn durable_rename(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)
        .with_context(|| format!("could not rename {} to {}", from.display(), to.display()))?;
    sync_parent_dir(to)
}

pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("tmp")
        .to_string();
    name.push_str(".tmp");
    path.with_file_name(name)
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = File::open(parent)
        .with_context(|| format!("could not open directory {}", parent.display()))?;
    dir.sync_all()
        .with_context(|| format!("could not sync directory {}", parent.display()))
}

pub fn set_status(dir: &Path, slot: &Slot, status: Status) -> Result<PathBuf> {
    let msg = read_message(&slot.path).unwrap_or_default();
    let role = role_for_path(&msg, status, slot.role.as_deref());
    let new_path = slot_path(dir, slot.num, role.as_deref(), status);
    info!(
        slot = slot.num,
        role = role.as_deref().unwrap_or("none"),
        from = slot.status.as_str(),
        to = status.as_str(),
        old_path = %slot.path.display(),
        new_path = %new_path.display(),
        "renaming slot status"
    );
    durable_rename(&slot.path, &new_path)?;
    Ok(new_path)
}

pub fn create_slot(dir: &Path, num: u32, status: Status, msg: &Message) -> Result<PathBuf> {
    create_slot_with_options(dir, num, status, msg, WriteOptions::default())
}

pub fn create_slot_with_options(
    dir: &Path,
    num: u32,
    status: Status,
    msg: &Message,
    write_options: WriteOptions,
) -> Result<PathBuf> {
    let role = role_for_path(msg, status, None);
    let path = slot_path(dir, num, role.as_deref(), status);
    info!(
        slot = num,
        role = role.as_deref().unwrap_or("none"),
        status = status.as_str(),
        path = %path.display(),
        "creating slot"
    );
    write_message_with_options(&path, msg, write_options)?;
    Ok(path)
}

fn role_for_path(msg: &Message, status: Status, fallback: Option<&str>) -> Option<String> {
    if !msg.role.is_empty() {
        return Some(sanitize_role(&msg.role));
    }
    if let Some(role) = fallback {
        return Some(sanitize_role(role));
    }
    match status {
        Status::Running => None,
        Status::Awaiting => Some("user".into()),
        Status::Pending => Some("tool".into()),
        Status::Done => Some("unknown".into()),
    }
}

fn sanitize_role(role: &str) -> String {
    role.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub fn validate_single_open(slots: &[Slot]) -> Result<Option<&Slot>> {
    let open: Vec<&Slot> = slots
        .iter()
        .filter(|slot| !slot.status.terminal())
        .collect();
    if open.len() > 1 {
        error!(
            open_slots = ?open.iter().map(|slot| slot.path.display().to_string()).collect::<Vec<_>>(),
            "dialog corruption detected"
        );
        bail!(
            "corruption: more than one non-terminal file: {:?}",
            open.iter()
                .map(|slot| slot.path.display().to_string())
                .collect::<Vec<_>>()
        );
    }
    Ok(open.first().copied())
}
