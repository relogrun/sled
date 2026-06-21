use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Running,
    Pending,
    Awaiting,
    Done,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Running => "running",
            Status::Pending => "pending",
            Status::Awaiting => "awaiting",
            Status::Done => "done",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "running" => Self::Running,
            "pending" => Self::Pending,
            "awaiting" => Self::Awaiting,
            "done" => Self::Done,
            _ => return None,
        })
    }

    pub fn terminal(self) -> bool {
        self == Self::Done
    }
}

#[derive(Clone, Debug)]
pub struct Slot {
    pub num: u32,
    pub role: Option<String>,
    pub status: Status,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<Call>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspension: Option<ToolSuspension>,
}

impl Message {
    pub fn filled(&self) -> bool {
        !self.body.is_empty()
            || self.call.is_some()
            || self.result.is_some()
            || self.suspension.is_some()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Call {
    pub tool: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSuspension {
    pub request: Value,
}

#[derive(Clone, Debug)]
pub struct Context {
    pub index: String,
    pub bodies: String,
}

#[derive(Clone, Debug)]
pub struct ModelInput {
    pub system: String,
    pub context: Context,
}

pub trait Fold: Send + Sync {
    fn assemble(&self, slots: &[Slot]) -> Result<Context>;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    pub body_mirror: bool,
}

#[derive(Clone, Debug)]
pub enum Reply {
    Final {
        text: String,
        summary: String,
        wait_user: bool,
    },
    Tool {
        call: Call,
        summary: String,
    },
}

#[async_trait]
pub trait Model: Send + Sync {
    async fn complete(&self, system: &str, context: &Context) -> Result<Reply>;
}

#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, dialog_dir: &Path, slots: &[Slot], call: &Call) -> Result<ToolResult>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ToolResult {
    Completed(Value),
    Suspended(Value),
}

impl ToolResult {
    pub fn completed(value: Value) -> Self {
        Self::Completed(value)
    }

    pub fn suspended(request: Value) -> Self {
        Self::Suspended(request)
    }
}
