use crate::{ArchiveTool, Tool, ToolContext, ToolRegistry};
use serde_json::{Value, json};
use sled_core::{Message, ToolResult};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[tokio::test]
async fn archive_lists_compact_manifests_without_full_summary() {
    let dir = temp_dialog_dir("list");
    write_manifest(
        &dir,
        "0001-0002",
        json!({
            "id": "0001-0002",
            "from_slot": 1,
            "to_slot": 2,
            "slots": [1, 2],
            "compact_slot": 1,
            "description": "old setup",
            "summary": "full compact summary",
            "provider": "openai",
            "model": "gpt-5.4-mini",
            "summary_tokens": 2000,
            "source_estimated_tokens": 123
        }),
    );

    let result = execute_archive(&dir, json!({"op": "list"})).await;

    assert_eq!(
        result,
        ToolResult::completed(json!({
            "ok": true,
            "manifests": [{
                "ok": true,
                "id": "0001-0002",
                "from_slot": 1,
                "to_slot": 2,
                "slots": [1, 2],
                "compact_slot": 1,
                "description": "old setup",
                "provider": "openai",
                "model": "gpt-5.4-mini",
                "summary_tokens": 2000,
                "source_estimated_tokens": 123
            }]
        }))
    );
}

#[tokio::test]
async fn archive_reads_manifest_by_id() {
    let dir = temp_dialog_dir("read-manifest");
    let manifest = json!({
        "id": "0001-0002",
        "from_slot": 1,
        "to_slot": 2,
        "summary": "full compact summary"
    });
    write_manifest(&dir, "0001-0002", manifest.clone());

    let result = execute_archive(&dir, json!({"op": "read", "id": "0001-0002"})).await;

    assert_eq!(
        result,
        ToolResult::completed(json!({
            "ok": true,
            "id": "0001-0002",
            "manifest": manifest
        }))
    );
}

#[tokio::test]
async fn archive_reads_archived_slots() {
    let dir = temp_dialog_dir("read-slots");
    let slots_dir = dir.join("archive").join("slots");
    fs::create_dir_all(&slots_dir).unwrap();
    let message = Message {
        role: "user".into(),
        summary: "cat fact".into(),
        body: "Cats can be black.".into(),
        ..Message::default()
    };
    fs::write(
        slots_dir.join("0001.user.done.json5"),
        serde_json::to_string_pretty(&message).unwrap(),
    )
    .unwrap();

    let result = execute_archive(&dir, json!({"op": "read_slots", "slots": [1, 2]})).await;

    assert_eq!(
        result,
        ToolResult::completed(json!({
            "ok": true,
            "sections": [
                {
                    "slot": 1,
                    "ok": true,
                    "path": "archive/slots/0001.user.done.json5",
                    "role": "user",
                    "summary": "cat fact",
                    "body": "Cats can be black.",
                    "call": null,
                    "result": null,
                    "suspension": null,
                    "compact": null
                },
                {
                    "slot": 2,
                    "ok": false,
                    "error": "no archived slot"
                }
            ]
        }))
    );
}

#[test]
fn defaults_include_archive_tool_description() {
    let registry = ToolRegistry::with_defaults();
    let prompt = registry.tool_descriptions_prompt().unwrap();

    assert!(prompt.contains("Tool `archive`:"));
    assert!(prompt.contains("{\"op\":\"list\"}"));
}

async fn execute_archive(dir: &PathBuf, args: Value) -> ToolResult {
    let ctx = ToolContext {
        dialog_dir: dir.clone(),
        slots: Vec::new(),
    };
    ArchiveTool.execute(&ctx, args).await.unwrap()
}

fn write_manifest(dir: &PathBuf, id: &str, manifest: Value) {
    let manifests_dir = dir.join("archive").join("compacts");
    fs::create_dir_all(&manifests_dir).unwrap();
    fs::write(
        manifests_dir.join(format!("{id}.json5")),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
}

fn temp_dialog_dir(name: &str) -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("sled-tools-archive-{name}-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}
