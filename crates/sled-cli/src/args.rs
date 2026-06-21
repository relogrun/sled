use clap::{Parser, Subcommand};
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
        #[arg(long, help = "Save provider override")]
        provider: Option<Provider>,
        #[arg(long, help = "Save model override for the selected provider")]
        model: Option<String>,
        #[arg(long = "openai-reasoning", help = "Save OpenAI reasoning effort")]
        openai_reasoning: Option<OpenAiReasoningEffort>,
        #[arg(long = "anthropic-effort", help = "Save Anthropic effort")]
        anthropic_effort: Option<AnthropicEffort>,
        #[arg(long = "anthropic-thinking", help = "Save Anthropic thinking mode")]
        anthropic_thinking: Option<AnthropicThinking>,
        #[arg(
            long = "openai-compatible-base-url",
            help = "Save base URL for openai-compatible providers"
        )]
        openai_compatible_base_url: Option<String>,
        #[arg(long, help = "Clear saved fold selection and use full message context")]
        all: bool,
        #[arg(
            long = "recent-messages",
            help = "Save limit to the last n message bodies"
        )]
        recent_messages: Option<usize>,
        #[arg(
            long = "recent-bytes",
            help = "Save byte budget for newest body sections"
        )]
        recent_bytes: Option<usize>,
        #[arg(
            long = "recent-tokens",
            help = "Save estimated token budget for newest body sections"
        )]
        recent_tokens: Option<usize>,
        #[arg(
            long = "context-window-tokens",
            help = "Save model context window token limit"
        )]
        context_window_tokens: Option<usize>,
        #[arg(
            long = "context-ratio",
            help = "Save max ratio of the model context window used by input"
        )]
        context_ratio: Option<f32>,
        #[arg(long, help = "Save markdown body mirrors as enabled")]
        body_mirror: bool,
    },
    Run {
        dir: PathBuf,
        #[arg(long, help = "Provider to use (default: openai)")]
        provider: Option<Provider>,
        #[arg(
            long,
            help = "Model override for the selected provider (defaults: openai=gpt-5.4-mini, anthropic=claude-sonnet-4-6; openai-compatible requires one)"
        )]
        model: Option<String>,
        #[arg(
            long = "openai-reasoning",
            help = "OpenAI reasoning effort for this run"
        )]
        openai_reasoning: Option<OpenAiReasoningEffort>,
        #[arg(long = "anthropic-effort", help = "Anthropic effort for this run")]
        anthropic_effort: Option<AnthropicEffort>,
        #[arg(
            long = "anthropic-thinking",
            help = "Anthropic thinking mode for this run"
        )]
        anthropic_thinking: Option<AnthropicThinking>,
        #[arg(
            long = "openai-compatible-base-url",
            help = "Base URL for openai-compatible providers"
        )]
        openai_compatible_base_url: Option<String>,
        #[arg(long, help = "Use full message context (default)")]
        all: bool,
        #[arg(long = "recent-messages", help = "Use only the last n message bodies")]
        recent_messages: Option<usize>,
        #[arg(
            long = "recent-bytes",
            help = "Use a byte budget for newest body sections"
        )]
        recent_bytes: Option<usize>,
        #[arg(
            long = "recent-tokens",
            help = "Use an estimated token budget for newest body sections"
        )]
        recent_tokens: Option<usize>,
        #[arg(
            long = "context-window-tokens",
            help = "Model context window token limit"
        )]
        context_window_tokens: Option<usize>,
        #[arg(
            long = "context-ratio",
            help = "Max ratio of the model context window used by input"
        )]
        context_ratio: Option<f32>,
        #[arg(
            long,
            help = "Write readable .done.md mirrors beside JSON5 files (default: off)"
        )]
        body_mirror: bool,
    },
    Status {
        dir: PathBuf,
    },
    Context {
        dir: PathBuf,
        #[arg(
            long = "context-window-tokens",
            help = "Model context window token limit"
        )]
        context_window_tokens: Option<usize>,
        #[arg(
            long = "context-ratio",
            help = "Max ratio of the model context window used by input"
        )]
        context_ratio: Option<f32>,
    },
}
