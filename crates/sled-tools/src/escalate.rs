use crate::{Tool, ToolContext};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::ToolResult;

pub struct EscalateTool;

#[async_trait]
impl Tool for EscalateTool {
    fn name(&self) -> &'static str {
        "escalate"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> Result<ToolResult> {
        let reason = args["reason"].as_str().unwrap_or("").trim();
        Ok(ToolResult::suspended(json!({
            "ok": true,
            "tool": "escalate",
            "reason": reason,
        })))
    }
}
