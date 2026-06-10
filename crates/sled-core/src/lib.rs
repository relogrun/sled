use anyhow::{Context as _, Result, anyhow, bail};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

pub const DEFAULT_SYSTEM_PROMPT: &str = concat!(
    include_str!("../../../prompts/system.md"),
    "\n\n",
    include_str!("../../../prompts/context_format.md"),
    "\n\n",
    include_str!("../../../prompts/reply_protocol.md")
);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Running,
    Pending,
    NeedsInput,
    Done,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Pending => "pending",
            Status::NeedsInput => "needs-input",
            Status::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "running" => Self::Running,
            "pending" => Self::Pending,
            "needs-input" => Self::NeedsInput,
            "done" => Self::Done,
            _ => return None,
        })
    }

    pub fn terminal(self) -> bool {
        self == Self::Done
    }
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub num: u32,
    pub role: Option<String>,
    pub status: Status,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<Call>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension: Option<ToolSuspension>,
}

impl Message {
    pub fn filled(&self) -> bool {
        !self.body.is_empty()
            || self.call.is_some()
            || self.result.is_some()
            || self.suspension.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MessageWriteFormat {
    Json,
    JsonWithMarkdownMirror,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Call {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSuspension {
    pub request: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SystemConfig {
    #[serde(default)]
    pub prompt: String,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub index: String,
    pub bodies: String,
}

pub trait Fold: Send + Sync {
    fn assemble(&self, slots: &[Slot]) -> Result<Context>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    pub body_mirror: bool,
}

#[derive(Clone, Debug)]
pub enum Reply {
    Final {
        text: String,
        summary: String,
        wait_user: bool,
    },
    Tool {
        call: Call,
        summary: String,
    },
}

#[async_trait]
pub trait Model: Send + Sync {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply>;
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, slots: &[Slot], call: &Call) -> Result<ToolResult>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolResult {
    Completed(Value),
    Suspended(Value),
}

impl ToolResult {
    pub fn completed(value: Value) -> Self {
        Self::Completed(value)
    }

    pub fn suspended(request: Value) -> Self {
        Self::Suspended(request)
    }
}

#[derive(Clone, Debug)]
pub enum StepOutcome {
    Continue,
    NeedsInput(PathBuf),
    Finished(Option<u32>),
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

fn write_message_with_format(path: &Path, msg: &Message, format: MessageWriteFormat) -> Result<()> {
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

fn mirror_file_name(path: &Path, msg: &Message) -> Result<String> {
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

fn tmp_path(path: &Path) -> PathBuf {
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
        Status::NeedsInput => Some("user".into()),
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

pub fn read_system_config(dir: &Path) -> Result<SystemConfig> {
    let json5_path = dir.join("_system.json5");
    let json_path = dir.join("_system.json");
    let path = if json5_path.exists() {
        Some(json5_path)
    } else if json_path.exists() {
        Some(json_path)
    } else {
        None
    };

    let Some(path) = path else {
        return Ok(SystemConfig::default());
    };
    let text = fs::read_to_string(&path)?;
    json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))
}

pub fn write_default_system_config(dir: &Path) -> Result<()> {
    let path = dir.join("_system.json5");
    if path.exists() {
        return Ok(());
    }
    write_system_config(dir, &SystemConfig::default())
}

pub fn write_system_config(dir: &Path, config: &SystemConfig) -> Result<()> {
    let path = dir.join("_system.json5");
    durable_write(&path, serde_json::to_string_pretty(&config)?.as_bytes())?;
    Ok(())
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

pub fn preview_model_input(
    dir: &Path,
    default_system: &str,
    fold: &dyn Fold,
) -> Result<(String, Context)> {
    let system_config = read_system_config(dir)?;
    let mut slots = scan(dir)?;
    validate_single_open(&slots)?;

    if slots.iter().all(|slot| slot.status.terminal())
        && let Some(last) = slots.last()
    {
        let msg = read_message(&last.path).unwrap_or_default();
        let role = message_or_slot_role(&msg, last);
        if role == "user" || role == "tool" {
            slots.push(Slot {
                num: last.num + 1,
                role: None,
                status: Status::Running,
                path: slot_path(dir, last.num + 1, None, Status::Running),
            });
        }
    }

    Ok((
        resolve_system_prompt(default_system, &system_config),
        fold.assemble(&slots)?,
    ))
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

fn is_assistant_role(role: &str) -> bool {
    role == "assistant"
}

pub async fn step(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    system: &str,
    fold: &dyn Fold,
) -> Result<StepOutcome> {
    step_with_options(dir, model, tools, system, fold, WriteOptions::default()).await
}

pub async fn step_with_options(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    system: &str,
    fold: &dyn Fold,
    write_options: WriteOptions,
) -> Result<StepOutcome> {
    debug!(dir = %dir.display(), "runner step start");
    fs::create_dir_all(dir)?;
    let system_config = read_system_config(dir)?;
    let slots = scan(dir)?;
    let open = validate_single_open(&slots)?;

    let Some(slot) = open else {
        match slots.last() {
            None => {
                info!(dir = %dir.display(), "empty dialog, creating initial needs-input slot");
                write_default_system_config(dir)?;
                let path = create_slot_with_options(
                    dir,
                    1,
                    Status::NeedsInput,
                    &Message::default(),
                    write_options,
                )?;
                return Ok(StepOutcome::NeedsInput(path));
            }
            Some(last) => {
                let msg = read_message(&last.path)?;
                if is_assistant_role(&msg.role) {
                    info!(slot = last.num, "dialog finished at assistant message");
                    return Ok(StepOutcome::Finished(Some(last.num)));
                }
                info!(
                    last_slot = last.num,
                    last_role = %msg.role,
                    "creating next assistant running slot"
                );
                create_slot_with_options(
                    dir,
                    last.num + 1,
                    Status::Running,
                    &Message::default(),
                    write_options,
                )?;
                return Ok(StepOutcome::Continue);
            }
        }
    };

    match slot.status {
        Status::NeedsInput => {
            info!(slot = slot.num, path = %slot.path.display(), "user input requested");
            Ok(StepOutcome::NeedsInput(slot.path.clone()))
        }
        Status::Pending => {
            info!(slot = slot.num, "processing pending tool slot");
            let mut msg = read_message(&slot.path)?;
            if msg.result.is_some() {
                info!(slot = slot.num, "recovering completed pending tool slot");
                set_status(dir, slot, Status::Done)?;
                return Ok(StepOutcome::Continue);
            }
            if msg.suspension.is_some() {
                info!(slot = slot.num, "recovering suspended pending tool slot");
                let path = set_status(dir, slot, Status::NeedsInput)?;
                return Ok(StepOutcome::NeedsInput(path));
            }
            let call = msg
                .call
                .clone()
                .ok_or_else(|| anyhow!("pending slot without call: {}", slot.path.display()))?;
            match tools.execute(&slots, &call).await? {
                ToolResult::Completed(result) => {
                    msg.result = Some(result);
                    write_message_with_options(&slot.path, &msg, write_options)?;
                    set_status(dir, slot, Status::Done)?;
                    Ok(StepOutcome::Continue)
                }
                ToolResult::Suspended(request) => {
                    msg.suspension = Some(ToolSuspension {
                        request,
                        answer: None,
                    });
                    write_message_with_options(&slot.path, &msg, write_options)?;
                    let path = set_status(dir, slot, Status::NeedsInput)?;
                    Ok(StepOutcome::NeedsInput(path))
                }
            }
        }
        Status::Running => {
            info!(slot = slot.num, "processing running assistant slot");
            let msg = read_message(&slot.path).unwrap_or_default();
            if msg.filled() {
                info!(slot = slot.num, "recovering filled running slot");
                let next = if msg.call.is_some() && msg.result.is_none() {
                    Status::Pending
                } else {
                    Status::Done
                };
                set_status(dir, slot, next)?;
                return Ok(StepOutcome::Continue);
            }

            let context = fold.assemble(&slots)?;
            let system = resolve_system_prompt(system, &system_config);
            match model.complete(&system, &context).await? {
                Reply::Final {
                    text,
                    summary,
                    wait_user,
                } => {
                    info!(
                        slot = slot.num,
                        wait_user, "assistant returned final response"
                    );
                    write_message_with_options(
                        &slot.path,
                        &Message {
                            role: "assistant".into(),
                            summary,
                            body: text,
                            ..Message::default()
                        },
                        write_options,
                    )?;
                    set_status(dir, slot, Status::Done)?;
                    if wait_user {
                        create_slot_with_options(
                            dir,
                            slot.num + 1,
                            Status::NeedsInput,
                            &Message::default(),
                            write_options,
                        )?;
                    }
                    Ok(StepOutcome::Continue)
                }
                Reply::Tool { call, summary } => {
                    info!(slot = slot.num, tool = %call.tool, "assistant requested tool");
                    write_message_with_options(
                        &slot.path,
                        &Message {
                            role: "tool".into(),
                            summary,
                            call: Some(call),
                            ..Message::default()
                        },
                        write_options,
                    )?;
                    set_status(dir, slot, Status::Pending)?;
                    Ok(StepOutcome::Continue)
                }
            }
        }
        Status::Done => unreachable!(),
    }
}

pub async fn run_until_stop(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    system: &str,
    fold: &dyn Fold,
) -> Result<StepOutcome> {
    run_until_stop_with_options(dir, model, tools, system, fold, WriteOptions::default()).await
}

pub async fn run_until_stop_with_options(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    system: &str,
    fold: &dyn Fold,
    write_options: WriteOptions,
) -> Result<StepOutcome> {
    info!(dir = %dir.display(), "runner started");
    loop {
        match step_with_options(dir, model, tools, system, fold, write_options).await? {
            StepOutcome::Continue => continue,
            other => return Ok(other),
        }
    }
}

pub fn say(dir: &Path, text: &str) -> Result<PathBuf> {
    say_with_options(dir, text, WriteOptions::default())
}

pub fn say_with_options(dir: &Path, text: &str, write_options: WriteOptions) -> Result<PathBuf> {
    info!(dir = %dir.display(), "adding user message");
    fs::create_dir_all(dir)?;
    write_default_system_config(dir)?;
    let slots = scan(dir)?;
    let open = validate_single_open(&slots)?;

    if let Some(slot) = open {
        if slot.status != Status::NeedsInput {
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
        if is_tool_needs_input(slot, &existing) {
            let Some(suspension) = &mut existing.suspension else {
                bail!(
                    "cannot answer tool needs-input without suspension: {}",
                    slot.path.display()
                );
            };
            suspension.answer = Some(Value::String(text.into()));
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

fn is_tool_needs_input(slot: &Slot, msg: &Message) -> bool {
    slot.status == Status::NeedsInput
        && (msg.role == "tool" || slot.role.as_deref() == Some("tool"))
        && msg.filled()
}

fn json_tool_answer(text: &str) -> Value {
    serde_json::json!({
        "ok": true,
        "answer": text,
    })
}

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

pub fn resolve_system_prompt(default_system: &str, config: &SystemConfig) -> String {
    if config.prompt.trim().is_empty() {
        default_system.to_string()
    } else {
        format!("{}\n\n{}", default_system, config.prompt.trim())
    }
}

fn shorten(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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
        async fn execute(&self, _slots: &[Slot], call: &Call) -> Result<ToolResult> {
            Ok(ToolResult::completed(
                json!({"ok": true, "tool": call.tool}),
            ))
        }
    }

    struct PanicTools;

    #[async_trait]
    impl ToolExecutor for PanicTools {
        async fn execute(&self, _slots: &[Slot], _call: &Call) -> Result<ToolResult> {
            unreachable!("tool should not be executed")
        }
    }

    struct SuspendTools;

    #[async_trait]
    impl ToolExecutor for SuspendTools {
        async fn execute(&self, _slots: &[Slot], call: &Call) -> Result<ToolResult> {
            Ok(ToolResult::suspended(json!({
                "tool": call.tool,
                "prompt": "answer required"
            })))
        }
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
        let path =
            say_with_options(&dir, "hello\nworld", WriteOptions { body_mirror: true }).unwrap();
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
        let needs_input = create_slot(&dir, 2, Status::NeedsInput, &Message::default()).unwrap();

        assert_eq!(running.file_name().unwrap(), "0001.running.json5");
        assert_eq!(
            needs_input.file_name().unwrap(),
            "0002.user.needs-input.json5"
        );
    }

    #[test]
    fn rejects_two_open_slots() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        create_slot(&dir, 1, Status::NeedsInput, &Message::default()).unwrap();
        create_slot(&dir, 2, Status::Pending, &Message::default()).unwrap();
        let slots = scan(&dir).unwrap();
        assert!(validate_single_open(&slots).is_err());
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

        let outcome = step(&dir, &NoopModel, &FakeTools, "system", &NoopFold)
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

        let outcome = step(&dir, &NoopModel, &PanicTools, "system", &NoopFold)
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::Continue));

        let slots = scan(&dir).unwrap();
        assert_eq!(slots[0].status, Status::Done);
        let msg = read_message(&slots[0].path).unwrap();
        assert_eq!(msg.result.unwrap()["ok"], true);
    }

    #[tokio::test]
    async fn pending_tool_can_suspend_into_tool_needs_input() {
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

        let outcome = step(&dir, &NoopModel, &SuspendTools, "system", &NoopFold)
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::NeedsInput(_)));

        let slots = scan(&dir).unwrap();
        assert_eq!(slots[0].status, Status::NeedsInput);
        assert_eq!(
            slots[0].path.file_name().unwrap(),
            "0001.tool.needs-input.json5"
        );
        let msg = read_message(&slots[0].path).unwrap();
        let suspension = msg.suspension.unwrap();
        assert_eq!(suspension.request["tool"], "ask_human");
        assert!(suspension.answer.is_none());
        assert!(msg.result.is_none());
    }

    #[tokio::test]
    async fn pending_tool_with_suspension_recovers_to_needs_input_without_reexecution() {
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
                    answer: None,
                }),
                ..Message::default()
            },
        )
        .unwrap();

        let outcome = step(&dir, &NoopModel, &PanicTools, "system", &NoopFold)
            .await
            .unwrap();
        assert!(matches!(outcome, StepOutcome::NeedsInput(_)));

        let slots = scan(&dir).unwrap();
        assert_eq!(slots[0].status, Status::NeedsInput);
        assert_eq!(
            slots[0].path.file_name().unwrap(),
            "0001.tool.needs-input.json5"
        );
    }

    #[test]
    fn say_answers_suspended_tool_needs_input() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        create_slot(
            &dir,
            1,
            Status::NeedsInput,
            &Message {
                role: "tool".into(),
                summary: "ask".into(),
                call: Some(Call {
                    tool: "ask_human".into(),
                    args: json!({}),
                }),
                suspension: Some(ToolSuspension {
                    request: json!({"prompt": "answer required"}),
                    answer: None,
                }),
                ..Message::default()
            },
        )
        .unwrap();

        let path = say(&dir, "human answer").unwrap();
        assert_eq!(path.file_name().unwrap(), "0001.tool.done.json5");

        let msg = read_message(&path).unwrap();
        let suspension = msg.suspension.unwrap();
        assert_eq!(suspension.answer.unwrap(), json!("human answer"));
        assert_eq!(msg.result.unwrap()["answer"], "human answer");
    }
}
