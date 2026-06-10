use crate::{HttpGetTool, OpenTool, ReadTool};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::{Call, Slot, ToolExecutor, ToolResult};
use std::collections::HashMap;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct ToolContext {
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
    async fn execute(&self, slots: &[Slot], call: &Call) -> Result<ToolResult> {
        let ctx = ToolContext {
            slots: slots.to_vec(),
        };
        self.execute(&ctx, call).await
    }
}
