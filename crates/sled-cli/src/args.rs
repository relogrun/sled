use clap::{Args, Parser, Subcommand};
use sled_ai::{AnthropicEffort, AnthropicThinking, OpenAiReasoningEffort, Provider};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "sled")]
#[command(about = "File-based AI dialog runner")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    Init {
        dir: PathBuf,
        #[arg(long, conflicts_with = "system_file")]
        system: Option<String>,
        #[arg(long = "system-file")]
        system_file: Option<PathBuf>,
    },
    Say {
        dir: PathBuf,
        text: String,
        #[arg(long, help = "Run immediately after writing the user message")]
        run: bool,
        #[arg(
            long,
            help = "Write readable .done.md mirrors beside JSON5 files (default: off)"
        )]
        body_mirror: bool,
    },
    Config {
        dir: PathBuf,
        #[command(flatten)]
        options: DialogArgs,
    },
    Run {
        dir: PathBuf,
        #[command(flatten)]
        options: DialogArgs,
    },
    Status {
        dir: PathBuf,
    },
    Context {
        dir: PathBuf,
        #[command(flatten)]
        context: ContextArgs,
    },
}

#[derive(Args, Clone, Default)]
pub(crate) struct DialogArgs {
    #[command(flatten)]
    pub(crate) provider: ProviderArgs,
    #[arg(long, help = "Fold pipeline, such as all or recent-tokens:50000")]
    pub(crate) fold: Option<String>,
    #[command(flatten)]
    pub(crate) context: ContextLimitArgs,
    #[arg(
        long,
        help = "Write readable .done.md mirrors beside JSON5 files (default: off)"
    )]
    pub(crate) body_mirror: bool,
}

#[derive(Args, Clone, Default)]
pub(crate) struct ProviderArgs {
    #[arg(long, help = "Provider to use")]
    pub(crate) provider: Option<Provider>,
    #[arg(long, help = "Model override for the selected provider")]
    pub(crate) model: Option<String>,
    #[arg(long = "openai-reasoning", help = "OpenAI reasoning effort")]
    pub(crate) openai_reasoning: Option<OpenAiReasoningEffort>,
    #[arg(long = "anthropic-effort", help = "Anthropic effort")]
    pub(crate) anthropic_effort: Option<AnthropicEffort>,
    #[arg(long = "anthropic-thinking", help = "Anthropic thinking mode")]
    pub(crate) anthropic_thinking: Option<AnthropicThinking>,
    #[arg(
        long = "openai-compatible-base-url",
        help = "Base URL for openai-compatible providers"
    )]
    pub(crate) openai_compatible_base_url: Option<String>,
}

#[derive(Args, Clone, Default)]
pub(crate) struct ContextArgs {
    #[arg(long, help = "Fold pipeline, such as all or recent-tokens:50000")]
    pub(crate) fold: Option<String>,
    #[command(flatten)]
    pub(crate) context: ContextLimitArgs,
}

#[derive(Args, Clone, Default)]
pub(crate) struct ContextLimitArgs {
    #[arg(
        long = "context-window-tokens",
        help = "Model context window token limit"
    )]
    pub(crate) context_window_tokens: Option<usize>,
    #[arg(
        long = "context-ratio",
        help = "Max ratio of the model context window used by input"
    )]
    pub(crate) context_ratio: Option<f32>,
}
