use crate::{EscalateTool, HttpGetTool, OpenTool, ReadTool};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::{Call, Slot, ToolExecutor, ToolResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct ToolContext {
    pub dialog_dir: PathBuf,
    pub slots: Vec<Slot>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(OpenTool);
        registry.register(ReadTool);
        registry.register(HttpGetTool::default());
        registry.register(EscalateTool);
        registry
    }

    pub fn register<T>(&mut self, tool: T)
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().into(), Box::new(tool));
    }

    pub async fn execute(&self, ctx: &ToolContext, call: &Call) -> Result<ToolResult> {
        info!(tool = %call.tool, "executing tool");
        let Some(tool) = self.tools.get(&call.tool) else {
            warn!(tool = %call.tool, "unknown tool requested");
            return Ok(ToolResult::completed(
                json!({"ok": false, "error": format!("unknown tool: {}", call.tool)}),
            ));
        };
        tool.execute(ctx, call.args.clone()).await
    }
}

#[async_trait]
impl ToolExecutor for ToolRegistry {
    async fn execute(&self, dialog_dir: &Path, slots: &[Slot], call: &Call) -> Result<ToolResult> {
        let ctx = ToolContext {
            dialog_dir: dialog_dir.to_path_buf(),
            slots: slots.to_vec(),
        };
        self.execute(&ctx, call).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
