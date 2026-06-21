use crate::{Tool, ToolContext, ToolRegistry};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::{Call, ToolExecutor, ToolResult};
use std::path::Path;

#[tokio::test]
async fn defaults_include_escalate_as_suspending_tool() {
    let registry = ToolRegistry::with_defaults();
    let ctx = ToolContext {
        dialog_dir: "dialog".into(),
        slots: Vec::new(),
    };
    let result = registry
        .execute(
            &ctx,
            &Call {
                tool: "escalate".into(),
                args: json!({"reason": "need a decision"}),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        result,
        ToolResult::suspended(json!({
            "ok": true,
            "tool": "escalate",
            "reason": "need a decision"
        }))
    );
}

struct DialogDirTool;

#[async_trait]
impl Tool for DialogDirTool {
    fn name(&self) -> &'static str {
        "dialog_dir"
    }

    async fn execute(&self, ctx: &ToolContext, _args: Value) -> Result<ToolResult> {
        Ok(ToolResult::completed(json!({
            "dialog_dir": ctx.dialog_dir,
        })))
    }
}

#[tokio::test]
async fn tool_executor_context_includes_dialog_dir() {
    let mut registry = ToolRegistry::new();
    registry.register(DialogDirTool);
    let result = <ToolRegistry as ToolExecutor>::execute(
        &registry,
        Path::new("runs/example/dialog"),
        &[],
        &Call {
            tool: "dialog_dir".into(),
            args: json!({}),
        },
    )
    .await
    .unwrap();

    assert_eq!(
        result,
        ToolResult::completed(json!({
            "dialog_dir": "runs/example/dialog",
        }))
    );
}

struct DescribedTool;

#[async_trait]
impl Tool for DescribedTool {
    fn name(&self) -> &'static str {
        "described"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Use for tests.")
    }

    async fn execute(&self, _ctx: &ToolContext, _args: Value) -> Result<ToolResult> {
        Ok(ToolResult::completed(json!({})))
    }
}

struct OtherDescribedTool;

#[async_trait]
impl Tool for OtherDescribedTool {
    fn name(&self) -> &'static str {
        "other"
    }

    fn description(&self) -> Option<&'static str> {
        Some("Use for other tests.")
    }

    async fn execute(&self, _ctx: &ToolContext, _args: Value) -> Result<ToolResult> {
        Ok(ToolResult::completed(json!({})))
    }
}

#[test]
fn tool_descriptions_prompt_includes_tool_descriptions() {
    let mut registry = ToolRegistry::new();
    registry.register(DescribedTool);

    let fragment = registry.tool_descriptions_prompt().unwrap();

    assert!(fragment.contains("Tool `described`:"));
    assert!(fragment.contains("Use for tests."));
}

#[test]
fn tool_descriptions_prompt_preserves_registration_order() {
    let mut registry = ToolRegistry::new();
    registry.register(OtherDescribedTool);
    registry.register(DescribedTool);

    let fragment = registry.tool_descriptions_prompt().unwrap();

    assert!(fragment.find("Tool `other`:").unwrap() < fragment.find("Tool `described`:").unwrap());
}
