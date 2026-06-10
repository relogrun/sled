use async_trait::async_trait;
use serde_json::{Value, json};
use sled_cli::{Profile, run_cli};
use sled_fold::AllFold;
use sled_tools::{Tool, ToolContext, ToolRegistry, ToolResult};

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &'static str {
        "echo"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::completed(json!({
            "ok": true,
            "echo": args,
        })))
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut tools = ToolRegistry::with_defaults();
    tools.register(EchoTool);

    run_cli(Profile {
        fold: Box::new(AllFold),
        tools,
        protocol_prompt: Some(
            "The custom `echo` tool returns its JSON arguments unchanged.".into(),
        ),
    })
    .await
}
