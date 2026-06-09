use crate::{Tool, ToolContext};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::fs;

pub struct ReadTool;

#[async_trait]
impl Tool for ReadTool {
    fn name(&self) -> &'static str {
        "read"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> Result<Value> {
        let paths = args["paths"].as_array().cloned().unwrap_or_default();
        let sections: Vec<Value> = paths
            .iter()
            .filter_map(|path| path.as_str())
            .map(|path| match fs::read_to_string(path) {
                Ok(text) => json!({"path": path, "ok": true, "text": text}),
                Err(err) => json!({"path": path, "ok": false, "error": err.to_string()}),
            })
            .collect();
        Ok(json!({"ok": true, "sections": sections}))
    }
}
