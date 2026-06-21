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
