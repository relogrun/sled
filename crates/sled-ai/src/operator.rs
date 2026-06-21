use anyhow::Result;
use async_trait::async_trait;
use sled_core::{Call, Context, Model, Reply};
use std::io::{self, Write};
use tracing::info;

pub struct OperatorModel;

#[async_trait]
impl Model for OperatorModel {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply> {
        info!(provider = "operator", "reading operator assistant input");
        println!(
            "\n=== context ===\n[system]\n{}\n\n[index]\n{}\n[bodies]\n{}",
            system, context.index, context.bodies
        );
        println!("answer as assistant:");
        println!("  final <text>");
        println!("  wait <text>");
        println!("  tool {{\"tool\":\"read\",\"args\":{{\"paths\":[\"Cargo.toml\"]}}}}");
        loop {
            print!("> ");
            io::stdout().flush()?;
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            let line = line.trim();
            if let Some(text) = line.strip_prefix("final ") {
                return Ok(Reply::Final {
                    text: text.into(),
                    summary: shorten(text, 80),
                    wait_user: false,
                });
            }
            if let Some(text) = line.strip_prefix("wait ") {
                return Ok(Reply::Final {
                    text: text.into(),
                    summary: shorten(text, 80),
                    wait_user: true,
                });
            }
            if let Some(raw) = line.strip_prefix("tool ") {
                match serde_json::from_str::<Call>(raw) {
                    Ok(call) => {
                        let summary = format!("call {}", call.tool);
                        return Ok(Reply::Tool { call, summary });
                    }
                    Err(err) => println!("could not parse json: {err}"),
                }
            }
        }
    }
}

fn shorten(text: &str, limit: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
