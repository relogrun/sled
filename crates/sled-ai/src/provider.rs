use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Operator,
    OpenAi,
    OpenAiCompatible,
    Anthropic,
}

impl std::str::FromStr for Provider {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "operator" => Self::Operator,
            "openai" => Self::OpenAi,
            "openai-compatible" => Self::OpenAiCompatible,
            "anthropic" => Self::Anthropic,
            other => bail!("unknown provider: {other}"),
        })
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Operator => "operator",
            Self::OpenAi => "openai",
            Self::OpenAiCompatible => "openai-compatible",
            Self::Anthropic => "anthropic",
        })
    }
}

pub fn default_model(provider: Provider) -> Option<&'static str> {
    match provider {
        Provider::OpenAi => Some("gpt-5.4-mini"),
        Provider::Anthropic => Some("claude-sonnet-4-6"),
        Provider::Operator | Provider::OpenAiCompatible => None,
    }
}

pub fn default_context_window_tokens(provider: Provider, model: Option<&str>) -> Option<usize> {
    let model = model?.trim().to_ascii_lowercase();
    match provider {
        Provider::OpenAi => openai_context_window_tokens(&model),
        Provider::Anthropic => anthropic_context_window_tokens(&model),
        Provider::Operator | Provider::OpenAiCompatible => None,
    }
}

fn openai_context_window_tokens(model: &str) -> Option<usize> {
    if model.starts_with("gpt-5.4-mini") {
        return Some(400_000);
    }
    if model.starts_with("gpt-5.5") || model.starts_with("gpt-5.4") {
        return Some(1_000_000);
    }
    if model.starts_with("gpt-4.1") {
        return Some(1_000_000);
    }
    if model.starts_with("gpt-4o") || model.starts_with("gpt-4.5") {
        return Some(128_000);
    }
    if model.starts_with("gpt-4-turbo") {
        return Some(128_000);
    }
    if model.starts_with("gpt-4-32k") {
        return Some(32_768);
    }
    if model == "gpt-4" || model.starts_with("gpt-4-") {
        return Some(8_192);
    }
    None
}

fn anthropic_context_window_tokens(model: &str) -> Option<usize> {
    if model.starts_with("claude-fable-5")
        || model.starts_with("claude-mythos-5")
        || model.starts_with("claude-opus-4-8")
        || model.starts_with("claude-sonnet-4-6")
    {
        return Some(1_000_000);
    }
    if model.starts_with("claude-haiku-4-5") {
        return Some(200_000);
    }
    None
}
