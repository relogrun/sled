use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sled_core::storage::{read_message, scan, slot_path, validate_single_open, write_message};
use sled_core::{
    Context, ContextLimit, Message, Model, Reply, Slot, Status, context_budget_tokens,
    estimate_tokens, select_newest_sections_to_fit,
};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SUMMARY_TOKENS: usize = 2000;

const COMPACT_PROMPT: &str = include_str!("prompts/compact.md");

#[derive(Clone, Debug)]
pub struct CompactOptions {
    pub from_slot: Option<u32>,
    pub range_end: CompactRangeEnd,
    pub summary_tokens: usize,
}

impl CompactOptions {
    pub fn validate(&self) -> Result<()> {
        if self.summary_tokens == 0 {
            bail!("--summary-tokens must be greater than 0");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompactRangeEnd {
    ToSlot(u32),
    KeepRecent(usize),
    KeepRecentTokens(usize),
}

#[derive(Clone, Debug)]
pub struct CompactRuntime {
    pub context_limit: ContextLimit,
    pub provider: String,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactManifest {
    pub id: String,
    pub from_slot: u32,
    pub to_slot: u32,
    pub slots: Vec<u32>,
    pub compact_slot: u32,
    pub description: String,
    pub summary: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub summary_tokens: usize,
    pub source_estimated_tokens: usize,
}

#[derive(Clone, Debug)]
struct CompactSource {
    slot: Slot,
    index_line: String,
    body_section: String,
}

pub async fn compact_dialog(
    dir: &Path,
    model: &dyn Model,
    runtime: &CompactRuntime,
    options: CompactOptions,
) -> Result<CompactManifest> {
    options.validate()?;
    let slots = scan(dir)?;
    validate_single_open(&slots)?;
    let sources = compact_sources(&slots)?;
    let selected = select_compact_sources(&sources, &options)?;
    ensure_compact_input_fits(&selected, runtime)?;
    let context = compact_context(&selected, options.summary_tokens);
    let reply = model.complete(COMPACT_PROMPT, &context).await?;
    let (summary, description) = compact_reply_text(reply)?;
    let manifest = build_manifest(&selected, &summary, &description, runtime, &options);
    write_compact_result(dir, &selected, &summary, &manifest)?;
    Ok(manifest)
}

pub fn archive_slots_dir(dir: &Path) -> PathBuf {
    dir.join("archive").join("slots")
}

fn archive_compacts_dir(dir: &Path) -> PathBuf {
    dir.join("archive").join("compacts")
}

fn compact_sources(slots: &[Slot]) -> Result<Vec<CompactSource>> {
    slots
        .iter()
        .filter(|slot| slot.status == Status::Done)
        .map(|slot| {
            let message = read_message(&slot.path)
                .with_context(|| format!("could not read {}", slot.path.display()))?;
            let role = message_or_slot_role(&message, slot);
            Ok(CompactSource {
                slot: slot.clone(),
                index_line: index_line(slot, &role, &message),
                body_section: body_section(slot, &role, &message),
            })
        })
        .collect()
}

fn select_compact_sources<'a>(
    sources: &'a [CompactSource],
    options: &CompactOptions,
) -> Result<Vec<&'a CompactSource>> {
    if sources.is_empty() {
        bail!("no done slots to compact");
    }
    let from_slot = options
        .from_slot
        .unwrap_or_else(|| sources.first().map(|source| source.slot.num).unwrap_or(1));
    let to_slot = match options.range_end {
        CompactRangeEnd::ToSlot(to_slot) => to_slot,
        CompactRangeEnd::KeepRecent(keep_recent) => keep_recent_to_slot(sources, keep_recent)?,
        CompactRangeEnd::KeepRecentTokens(keep_recent_tokens) => {
            keep_recent_tokens_to_slot(sources, keep_recent_tokens)?
        }
    };
    if from_slot > to_slot {
        bail!("compact range is empty: from-slot {from_slot} is after to-slot {to_slot}");
    }
    let selected = sources
        .iter()
        .filter(|source| source.slot.num >= from_slot && source.slot.num <= to_slot)
        .collect::<Vec<_>>();
    if selected.is_empty() {
        bail!("compact range contains no active done slots");
    }
    if selected.iter().any(|source| source.slot.num == 0) {
        bail!("slot 0 cannot be compacted");
    }
    Ok(selected)
}

fn keep_recent_to_slot(sources: &[CompactSource], keep_recent: usize) -> Result<u32> {
    if keep_recent >= sources.len() {
        bail!("compact range is empty: --keep-recent keeps all active done slots");
    }
    Ok(sources[sources.len() - keep_recent - 1].slot.num)
}

fn keep_recent_tokens_to_slot(sources: &[CompactSource], keep_recent_tokens: usize) -> Result<u32> {
    let selected_tail = select_newest_sections_to_fit(
        0,
        sources.iter().map(|source| source.body_section.len()),
        keep_recent_tokens,
    );
    let first_tail_idx = selected_tail.iter().position(|selected| *selected);
    match first_tail_idx {
        Some(0) => {
            bail!("compact range is empty: --keep-recent-tokens keeps all active done slots")
        }
        Some(idx) => Ok(sources[idx - 1].slot.num),
        None => Ok(sources.last().map(|source| source.slot.num).unwrap_or(0)),
    }
}

fn ensure_compact_input_fits(selected: &[&CompactSource], runtime: &CompactRuntime) -> Result<()> {
    let budget = context_budget_tokens(runtime.context_limit)?;
    let context = compact_context(selected, 0);
    let estimated =
        estimate_tokens(COMPACT_PROMPT.len() + context.index.len() + context.bodies.len());
    if estimated > budget {
        bail!(
            "compact input exceeds context budget: estimated {} tokens, budget {} tokens; choose a smaller range",
            estimated,
            budget
        );
    }
    Ok(())
}

fn compact_context(selected: &[&CompactSource], summary_tokens: usize) -> Context {
    let mut index = String::new();
    if summary_tokens > 0 {
        index.push_str(&format!("Target summary tokens: {summary_tokens}\n"));
    }
    index.extend(selected.iter().map(|source| source.index_line.as_str()));
    let bodies = selected
        .iter()
        .map(|source| source.body_section.as_str())
        .collect::<String>();
    Context { index, bodies }
}

fn compact_reply_text(reply: Reply) -> Result<(String, String)> {
    match reply {
        Reply::Final { text, summary, .. } => {
            let text = text.trim().to_string();
            if text.is_empty() {
                bail!("compact model returned an empty summary");
            }
            let description = if summary.trim().is_empty() {
                shorten(&text, 120)
            } else {
                summary
            };
            Ok((text, description))
        }
        Reply::Tool { .. } => bail!("compact model returned a tool call"),
    }
}

fn build_manifest(
    selected: &[&CompactSource],
    summary: &str,
    description: &str,
    runtime: &CompactRuntime,
    options: &CompactOptions,
) -> CompactManifest {
    let from_slot = selected.first().map(|source| source.slot.num).unwrap();
    let to_slot = selected.last().map(|source| source.slot.num).unwrap();
    let compact_slot = from_slot;
    let id = format!("{from_slot:04}-{to_slot:04}");
    let source_len = selected
        .iter()
        .map(|source| source.index_line.len() + source.body_section.len())
        .sum::<usize>();
    CompactManifest {
        id,
        from_slot,
        to_slot,
        slots: selected.iter().map(|source| source.slot.num).collect(),
        compact_slot,
        description: description.to_string(),
        summary: summary.to_string(),
        provider: runtime.provider.clone(),
        model: runtime.model.clone(),
        summary_tokens: options.summary_tokens,
        source_estimated_tokens: estimate_tokens(source_len),
    }
}

fn write_compact_result(
    dir: &Path,
    selected: &[&CompactSource],
    summary: &str,
    manifest: &CompactManifest,
) -> Result<()> {
    let archive_slots = archive_slots_dir(dir);
    let archive_compacts = archive_compacts_dir(dir);
    fs::create_dir_all(&archive_slots)?;
    fs::create_dir_all(&archive_compacts)?;

    for source in selected {
        let archive_path = archive_slots.join(file_name(&source.slot.path)?);
        if archive_path.exists() {
            bail!("archive slot already exists: {}", archive_path.display());
        }
    }
    let manifest_path = archive_compacts.join(format!("{}.json5", manifest.id));
    if manifest_path.exists() {
        bail!(
            "compact manifest already exists: {}",
            manifest_path.display()
        );
    }

    let compact_path = slot_path(dir, manifest.compact_slot, Some("compact"), Status::Done);
    let first_source_path = selected.first().map(|source| source.slot.path.as_path());
    if compact_path.exists() && Some(compact_path.as_path()) != first_source_path {
        bail!("compact slot already exists: {}", compact_path.display());
    }

    for source in selected {
        let archive_path = archive_slots.join(file_name(&source.slot.path)?);
        fs::rename(&source.slot.path, &archive_path).with_context(|| {
            format!(
                "could not archive {} to {}",
                source.slot.path.display(),
                archive_path.display()
            )
        })?;
        if let Some(mirror_path) = mirror_path_for_slot(&source.slot.path) {
            if mirror_path.exists() {
                let archive_mirror_path = archive_slots.join(file_name(&mirror_path)?);
                fs::rename(&mirror_path, &archive_mirror_path).with_context(|| {
                    format!(
                        "could not archive {} to {}",
                        mirror_path.display(),
                        archive_mirror_path.display()
                    )
                })?;
            }
        }
    }

    let compact_message = Message {
        role: "compact".into(),
        summary: manifest.description.clone(),
        body: summary.to_string(),
        compact: Some(json!({
            "id": manifest.id,
            "from_slot": manifest.from_slot,
            "to_slot": manifest.to_slot,
            "archive": "archive/slots"
        })),
        ..Message::default()
    };
    write_message(&compact_path, &compact_message)?;
    let manifest_text = serde_json::to_string_pretty(manifest)?;
    sled_core::storage::durable_write(&manifest_path, manifest_text.as_bytes())?;
    Ok(())
}

fn file_name(path: &Path) -> Result<&std::ffi::OsStr> {
    path.file_name()
        .ok_or_else(|| anyhow::anyhow!("path has no filename: {}", path.display()))
}

fn mirror_path_for_slot(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json5")?;
    Some(path.with_file_name(format!("{stem}.md")))
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
    if let Some(compact) = &msg.compact {
        section.push_str(&format!("compact: {}\n", compact));
    }
    section.push('\n');
    section
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
    use async_trait::async_trait;
    use sled_core::{Reply, WriteOptions};

    struct FakeModel;

    #[async_trait]
    impl Model for FakeModel {
        async fn complete(&self, _system: &str, context: &Context) -> Result<Reply> {
            Ok(Reply::Final {
                text: format!("summary for {}", context.index.lines().count()),
                summary: "compact summary".into(),
                wait_user: false,
            })
        }
    }

    fn temp_dir() -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sled-compact-test-{id}"))
    }

    fn write_done(dir: &Path, num: u32, role: &str, body: &str) {
        let msg = Message {
            role: role.into(),
            summary: body.into(),
            body: body.into(),
            ..Message::default()
        };
        sled_core::storage::create_slot_with_options(
            dir,
            num,
            Status::Done,
            &msg,
            WriteOptions::default(),
        )
        .unwrap();
    }

    fn test_runtime() -> CompactRuntime {
        CompactRuntime {
            context_limit: ContextLimit::default(),
            provider: "operator".into(),
            model: None,
        }
    }

    #[test]
    fn compact_options_reject_zero_summary_tokens() {
        let err = CompactOptions {
            from_slot: None,
            range_end: CompactRangeEnd::ToSlot(1),
            summary_tokens: 0,
        }
        .validate()
        .unwrap_err()
        .to_string();

        assert_eq!(err, "--summary-tokens must be greater than 0");
    }

    #[test]
    fn keep_recent_selects_boundary_before_tail() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        write_done(&dir, 1, "user", "one");
        write_done(&dir, 2, "assistant", "two");
        write_done(&dir, 3, "user", "three");
        let slots = scan(&dir).unwrap();
        let sources = compact_sources(&slots).unwrap();

        let selected = select_compact_sources(
            &sources,
            &CompactOptions {
                from_slot: None,
                range_end: CompactRangeEnd::KeepRecent(1),
                summary_tokens: DEFAULT_SUMMARY_TOKENS,
            },
        )
        .unwrap();

        assert_eq!(
            selected
                .iter()
                .map(|source| source.slot.num)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[tokio::test]
    async fn compact_archives_slots_and_writes_compact_message() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        write_done(&dir, 1, "user", "one");
        write_done(&dir, 2, "assistant", "two");
        write_done(&dir, 3, "user", "three");

        let manifest = compact_dialog(
            &dir,
            &FakeModel,
            &test_runtime(),
            CompactOptions {
                from_slot: None,
                range_end: CompactRangeEnd::ToSlot(2),
                summary_tokens: DEFAULT_SUMMARY_TOKENS,
            },
        )
        .await
        .unwrap();

        assert_eq!(manifest.id, "0001-0002");
        assert!(dir.join("archive/slots/0001.user.done.json5").exists());
        assert!(dir.join("archive/slots/0002.assistant.done.json5").exists());
        assert!(dir.join("archive/compacts/0001-0002.json5").exists());
        assert!(dir.join("0001.compact.done.json5").exists());
        assert!(dir.join("0003.user.done.json5").exists());
        let compact = read_message(&dir.join("0001.compact.done.json5")).unwrap();
        assert_eq!(compact.role, "compact");
        assert!(compact.body.contains("summary for"));
    }
}
