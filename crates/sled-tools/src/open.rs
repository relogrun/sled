use crate::{Tool, ToolContext};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use sled_core::{ToolResult, read_message};

pub struct OpenTool;

#[async_trait]
impl Tool for OpenTool {
    fn name(&self) -> &'static str {
        "open"
    }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<ToolResult> {
        let nums = args["slots"].as_array().cloned().unwrap_or_default();
        let sections: Vec<Value> = nums
            .iter()
            .filter_map(|num| num.as_u64())
            .map(
                |num| match ctx.slots.iter().find(|slot| slot.num as u64 == num) {
                    Some(slot) => match read_message(&slot.path) {
                        Ok(msg) => json!({
                            "slot": num,
                            "ok": true,
                            "role": if msg.role.is_empty() {
                                slot.role.clone().unwrap_or_else(|| "none".into())
                            } else {
                                msg.role
                            },
                            "summary": msg.summary,
                            "body": msg.body,
                            "call": msg.call,
                            "result": msg.result,
                            "suspension": msg.suspension,
                        }),
                        Err(err) => json!({"slot": num, "ok": false, "error": err.to_string()}),
                    },
                    None => json!({"slot": num, "ok": false, "error": "no such slot"}),
                },
            )
            .collect();
        Ok(ToolResult::completed(
            json!({"ok": true, "sections": sections}),
        ))
    }
}
