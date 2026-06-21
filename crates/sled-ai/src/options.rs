use anyhow::{Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenAiReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
}

impl std::str::FromStr for OpenAiReasoningEffort {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            other => bail!("unknown reasoning effort: {other}"),
        })
    }
}

impl std::fmt::Display for OpenAiReasoningEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnthropicEffort {
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl std::str::FromStr for AnthropicEffort {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::XHigh,
            "max" => Self::Max,
            other => bail!("unknown Anthropic effort: {other}"),
        })
    }
}

impl std::fmt::Display for AnthropicEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnthropicThinking {
    Off,
    Adaptive,
}

impl std::str::FromStr for AnthropicThinking {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(match value {
            "off" => Self::Off,
            "adaptive" => Self::Adaptive,
            other => bail!("unknown Anthropic thinking mode: {other}"),
        })
    }
}

impl std::fmt::Display for AnthropicThinking {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Off => "off",
            Self::Adaptive => "adaptive",
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ModelOptions {
    pub model: Option<String>,
    pub openai_compatible_base_url: Option<String>,
    pub openai_reasoning_effort: Option<OpenAiReasoningEffort>,
    pub anthropic_effort: Option<AnthropicEffort>,
    pub anthropic_thinking: Option<AnthropicThinking>,
    pub temperature: Option<f32>,
}
