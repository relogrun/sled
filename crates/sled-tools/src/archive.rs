use crate::{Tool, ToolContext};
use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::ToolResult;
use sled_core::storage::read_message;
use std::fs;
use std::path::{Path, PathBuf};

pub struct ArchiveTool;

#[async_trait]
impl Tool for ArchiveTool {
    fn name(&self) -> &'static str {
        "archive"
    }

    fn description(&self) -> Option<&'static str> {
        Some(
            "Inspect compacted dialog archive under archive/. Args: {\"op\":\"list\"} lists compact manifests without full summaries; {\"op\":\"read\",\"id\":\"0001-0042\"} reads one manifest; {\"op\":\"read_slots\",\"slots\":[1,2]} reads archived slot messages. Use list first, then read only the manifest or slots needed to recover old details.",
        )
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult> {
        let value = match args["op"].as_str() {
            Some("list") => list_manifests(&ctx.dialog_dir)?,
            Some("read") => read_manifest(&ctx.dialog_dir, args["id"].as_str())?,
            Some("read_slots") => read_slots(&ctx.dialog_dir, args["slots"].as_array())?,
            Some(op) => json!({"ok": false, "error": format!("unknown archive op: {op}")}),
            None => json!({"ok": false, "error": "archive op is required"}),
        };
        Ok(ToolResult::completed(value))
    }
}

fn list_manifests(dialog_dir: &Path) -> Result<Value> {
    let dir = archive_compacts_dir(dialog_dir);
    if !dir.exists() {
        return Ok(json!({"ok": true, "manifests": []}));
    }

    let mut paths = json5_paths(&dir)?;
    paths.sort();
    let manifests = paths
        .iter()
        .map(|path| match read_json_value(path) {
            Ok(manifest) => compact_manifest_summary(&manifest),
            Err(err) => json!({
                "ok": false,
                "path": display_relative(dialog_dir, path),
                "error": err.to_string()
            }),
        })
        .collect::<Vec<_>>();
    Ok(json!({"ok": true, "manifests": manifests}))
}

fn read_manifest(dialog_dir: &Path, id: Option<&str>) -> Result<Value> {
    let Some(id) = id else {
        return Ok(json!({"ok": false, "error": "archive manifest id is required"}));
    };
    if !valid_archive_id(id) {
        return Ok(json!({"ok": false, "id": id, "error": "invalid archive manifest id"}));
    }
    let path = archive_compacts_dir(dialog_dir).join(format!("{id}.json5"));
    match read_json_value(&path) {
        Ok(manifest) => Ok(json!({"ok": true, "id": id, "manifest": manifest})),
        Err(err) => Ok(json!({"ok": false, "id": id, "error": err.to_string()})),
    }
}

fn read_slots(dialog_dir: &Path, slots: Option<&Vec<Value>>) -> Result<Value> {
    let Some(slots) = slots else {
        return Ok(json!({"ok": false, "error": "archive slots array is required"}));
    };
    let sections = slots
        .iter()
        .filter_map(|slot| slot.as_u64())
        .map(|slot| read_archived_slot(dialog_dir, slot))
        .collect::<Vec<_>>();
    Ok(json!({"ok": true, "sections": sections}))
}

fn read_archived_slot(dialog_dir: &Path, slot: u64) -> Value {
    match archived_slot_path(dialog_dir, slot) {
        Ok(Some(path)) => match read_message(&path) {
            Ok(msg) => json!({
                "slot": slot,
                "ok": true,
                "path": display_relative(dialog_dir, &path),
                "role": if msg.role.is_empty() {
                    role_from_archived_slot_path(&path).unwrap_or_else(|| "none".into())
                } else {
                    msg.role
                },
                "summary": msg.summary,
                "body": msg.body,
                "call": msg.call,
                "result": msg.result,
                "suspension": msg.suspension,
                "compact": msg.compact,
            }),
            Err(err) => json!({"slot": slot, "ok": false, "error": err.to_string()}),
        },
        Ok(None) => json!({"slot": slot, "ok": false, "error": "no archived slot"}),
        Err(err) => json!({"slot": slot, "ok": false, "error": err.to_string()}),
    }
}

fn compact_manifest_summary(manifest: &Value) -> Value {
    json!({
        "ok": true,
        "id": manifest.get("id").cloned().unwrap_or(Value::Null),
        "from_slot": manifest.get("from_slot").cloned().unwrap_or(Value::Null),
        "to_slot": manifest.get("to_slot").cloned().unwrap_or(Value::Null),
        "slots": manifest.get("slots").cloned().unwrap_or(Value::Null),
        "compact_slot": manifest.get("compact_slot").cloned().unwrap_or(Value::Null),
        "description": manifest.get("description").cloned().unwrap_or(Value::Null),
        "provider": manifest.get("provider").cloned().unwrap_or(Value::Null),
        "model": manifest.get("model").cloned().unwrap_or(Value::Null),
        "summary_tokens": manifest.get("summary_tokens").cloned().unwrap_or(Value::Null),
        "source_estimated_tokens": manifest.get("source_estimated_tokens").cloned().unwrap_or(Value::Null),
    })
}

fn read_json_value(path: &Path) -> Result<Value> {
    let text =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))
}

fn archived_slot_path(dialog_dir: &Path, slot: u64) -> Result<Option<PathBuf>> {
    let dir = archive_slots_dir(dialog_dir);
    if !dir.exists() {
        return Ok(None);
    }
    let prefix = format!("{slot:04}.");
    let mut matches = json5_paths(&dir)?
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    matches.sort();
    if matches.len() > 1 {
        bail!("multiple archived files for slot {slot}");
    }
    Ok(matches.pop())
}

fn json5_paths(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("could not read {}", dir.display()))? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".json5"))
        {
            paths.push(path);
        }
    }
    Ok(paths)
}

fn archive_slots_dir(dialog_dir: &Path) -> PathBuf {
    dialog_dir.join("archive").join("slots")
}

fn archive_compacts_dir(dialog_dir: &Path) -> PathBuf {
    dialog_dir.join("archive").join("compacts")
}

fn valid_archive_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn role_from_archived_slot_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json5")?;
    let mut parts = stem.split('.');
    parts.next()?;
    let role = parts.next()?;
    Some(role.to_string())
}

fn display_relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .display()
        .to_string()
}
