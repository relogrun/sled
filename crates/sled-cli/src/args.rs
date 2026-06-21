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
    #[command(about = "Create a dialog directory and optional dialog system prompt")]
    Init {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[arg(
            long,
            conflicts_with = "system_file",
            help = "Dialog-specific system prompt text"
        )]
        system: Option<String>,
        #[arg(
            long = "system-file",
            value_name = "PATH",
            help = "Read dialog-specific system prompt text from a file"
        )]
        system_file: Option<PathBuf>,
    },
    #[command(about = "Append a user message or answer the current awaiting slot")]
    Say {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[arg(value_name = "TEXT", help = "Message text")]
        text: String,
        #[arg(long, help = "Run immediately after writing the user message")]
        run: bool,
        #[arg(
            long,
            help = "Write readable .done.md mirrors beside JSON5 files (default: off)"
        )]
        body_mirror: bool,
    },
    #[command(about = "Create or update dialog-local _config.json5")]
    Config {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[command(flatten)]
        options: DialogArgs,
    },
    #[command(about = "Continue the dialog until finished, awaiting input, or error")]
    Run {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[command(flatten)]
        options: DialogArgs,
    },
    #[command(about = "Summarize active done slots into one compact message and archive originals")]
    Compact {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[command(flatten)]
        options: CompactArgs,
    },
    #[command(about = "Print current dialog status and latest message")]
    Status {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
    },
    #[command(about = "Print the model input assembled from system prompt, index, and bodies")]
    Context {
        #[arg(value_name = "DIR", help = "Dialog directory")]
        dir: PathBuf,
        #[command(flatten)]
        context: ContextArgs,
    },
}

#[derive(Args, Clone, Default)]
pub(crate) struct DialogArgs {
    #[command(flatten)]
    pub(crate) provider: ProviderArgs,
    #[arg(
        long,
        value_name = "PIPELINE",
        help = "Fold pipeline: all | recent-messages:N | recent-tokens:N"
    )]
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
    #[arg(
        long,
        value_name = "PROVIDER",
        help = "Provider: operator | openai | openai-compatible | anthropic"
    )]
    pub(crate) provider: Option<Provider>,
    #[arg(long, value_name = "MODEL", help = "Model for the selected provider")]
    pub(crate) model: Option<String>,
    #[arg(
        long = "openai-reasoning",
        value_name = "EFFORT",
        help = "OpenAI reasoning effort: minimal | low | medium | high"
    )]
    pub(crate) openai_reasoning: Option<OpenAiReasoningEffort>,
    #[arg(
        long = "anthropic-effort",
        value_name = "EFFORT",
        help = "Anthropic effort: low | medium | high | xhigh | max"
    )]
    pub(crate) anthropic_effort: Option<AnthropicEffort>,
    #[arg(
        long = "anthropic-thinking",
        value_name = "MODE",
        help = "Anthropic thinking mode: off | adaptive"
    )]
    pub(crate) anthropic_thinking: Option<AnthropicThinking>,
    #[arg(
        long = "openai-compatible-base-url",
        value_name = "URL",
        help = "Base URL for openai-compatible provider"
    )]
    pub(crate) openai_compatible_base_url: Option<String>,
}

#[derive(Args, Clone, Default)]
pub(crate) struct ContextArgs {
    #[arg(
        long,
        value_name = "PIPELINE",
        help = "Fold pipeline: all | recent-messages:N | recent-tokens:N"
    )]
    pub(crate) fold: Option<String>,
    #[command(flatten)]
    pub(crate) context: ContextLimitArgs,
}

#[derive(Args, Clone, Default)]
pub(crate) struct ContextLimitArgs {
    #[arg(
        long = "context-window-tokens",
        value_name = "TOKENS",
        help = "Model context window token limit; known models have defaults"
    )]
    pub(crate) context_window_tokens: Option<usize>,
    #[arg(
        long = "context-ratio",
        value_name = "RATIO",
        help = "Max ratio of the model context window used by input; default 0.8"
    )]
    pub(crate) context_ratio: Option<f32>,
}

#[derive(Args, Clone, Default)]
pub(crate) struct CompactArgs {
    #[command(flatten)]
    pub(crate) provider: ProviderArgs,
    #[command(flatten)]
    pub(crate) context: ContextLimitArgs,
    #[arg(
        long = "from-slot",
        value_name = "N",
        help = "First active done slot to compact; default is the first active done slot"
    )]
    pub(crate) from_slot: Option<u32>,
    #[arg(
        long = "to-slot",
        value_name = "N",
        help = "Compact through active done slot N; exclusive with --keep-recent and --keep-recent-tokens"
    )]
    pub(crate) to_slot: Option<u32>,
    #[arg(
        long = "keep-recent",
        value_name = "N",
        help = "Compact all but the last N active done slots; exclusive with --to-slot and --keep-recent-tokens"
    )]
    pub(crate) keep_recent: Option<usize>,
    #[arg(
        long = "keep-recent-tokens",
        value_name = "TOKENS",
        help = "Compact all but newest active done bodies fitting this estimated token budget; exclusive with --to-slot and --keep-recent"
    )]
    pub(crate) keep_recent_tokens: Option<usize>,
    #[arg(
        long = "summary-tokens",
        value_name = "TOKENS",
        default_value_t = 2000,
        help = "Target compact summary size in estimated tokens"
    )]
    pub(crate) summary_tokens: usize,
}
