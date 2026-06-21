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

    fn description(&self) -> Option<&'static str> {
        None
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    order: Vec<String>,
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
        let name = tool.name().to_string();
        if !self.tools.contains_key(&name) {
            self.order.push(name.clone());
        }
        self.tools.insert(name, Box::new(tool));
    }

    pub fn tool_descriptions_prompt(&self) -> Option<String> {
        let descriptions: Vec<(&str, &str)> = self
            .order
            .iter()
            .filter_map(|name| {
                let tool = self.tools.get(name)?;
                Some((tool.name(), tool.description()?))
            })
            .collect();
        if descriptions.is_empty() {
            return None;
        }

        let mut fragment =
            String::from("Use these descriptions as the authoritative tool contracts.\n");
        for (name, description) in descriptions {
            fragment.push_str("\nTool `");
            fragment.push_str(name);
            fragment.push_str("`:\n");
            fragment.push_str(description.trim());
            fragment.push('\n');
        }
        Some(fragment)
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
