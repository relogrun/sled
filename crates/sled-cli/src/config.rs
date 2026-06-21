use crate::args::{ContextArgs, DialogArgs};
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use sled_ai::{
    AnthropicEffort, AnthropicThinking, OpenAiReasoningEffort, Provider,
    default_context_window_tokens, default_model,
};
use sled_core::storage::durable_write;
use sled_core::{ContextLimit, DEFAULT_CONTEXT_RATIO, DEFAULT_CONTEXT_WINDOW_TOKENS, Fold};
use sled_fold::{RecentBytesFold, RecentMessagesFold, RecentTokensFold};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct DialogConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) openai: Option<OpenAiConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) anthropic: Option<AnthropicConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) openai_compatible: Option<OpenAiCompatibleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recent_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recent_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) recent_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_window_tokens: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) context_ratio: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) body_mirror: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenAiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AnthropicConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) thinking: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct OpenAiCompatibleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) base_url: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedDialogConfig {
    pub(crate) provider: Provider,
    pub(crate) model: Option<String>,
    pub(crate) openai_reasoning_effort: Option<OpenAiReasoningEffort>,
    pub(crate) anthropic_effort: Option<AnthropicEffort>,
    pub(crate) anthropic_thinking: Option<AnthropicThinking>,
    pub(crate) openai_compatible_base_url: Option<String>,
    pub(crate) recent_messages: Option<usize>,
    pub(crate) recent_bytes: Option<usize>,
    pub(crate) recent_tokens: Option<usize>,
    pub(crate) context_limit: ContextLimit,
    pub(crate) body_mirror: bool,
}

#[derive(Clone, Default)]
pub(crate) struct DialogOptionOverrides {
    pub(crate) provider: Option<Provider>,
    pub(crate) model: Option<String>,
    pub(crate) openai_reasoning: Option<OpenAiReasoningEffort>,
    pub(crate) anthropic_effort: Option<AnthropicEffort>,
    pub(crate) anthropic_thinking: Option<AnthropicThinking>,
    pub(crate) openai_compatible_base_url: Option<String>,
    pub(crate) all: bool,
    pub(crate) recent_messages: Option<usize>,
    pub(crate) recent_bytes: Option<usize>,
    pub(crate) recent_tokens: Option<usize>,
    pub(crate) context_window_tokens: Option<usize>,
    pub(crate) context_ratio: Option<f32>,
    pub(crate) body_mirror: Option<bool>,
}

impl From<DialogArgs> for DialogOptionOverrides {
    fn from(args: DialogArgs) -> Self {
        Self {
            provider: args.provider.provider,
            model: args.provider.model,
            openai_reasoning: args.provider.openai_reasoning,
            anthropic_effort: args.provider.anthropic_effort,
            anthropic_thinking: args.provider.anthropic_thinking,
            openai_compatible_base_url: args.provider.openai_compatible_base_url,
            all: args.fold.all,
            recent_messages: args.fold.recent_messages,
            recent_bytes: args.fold.recent_bytes,
            recent_tokens: args.fold.recent_tokens,
            context_window_tokens: args.context.context_window_tokens,
            context_ratio: args.context.context_ratio,
            body_mirror: body_mirror_override(args.body_mirror),
        }
    }
}

impl From<ContextArgs> for DialogOptionOverrides {
    fn from(args: ContextArgs) -> Self {
        Self {
            context_window_tokens: args.context_window_tokens,
            context_ratio: args.context_ratio,
            ..Self::default()
        }
    }
}

pub(crate) fn resolve_dialog_config(
    mut config: DialogConfig,
    overrides: DialogOptionOverrides,
) -> Result<ResolvedDialogConfig> {
    apply_dialog_option_overrides(&mut config, overrides)?;
    let provider = configured_provider(&config)?;
    let model =
        provider_model(&config, provider).or_else(|| default_model(provider).map(str::to_string));

    Ok(ResolvedDialogConfig {
        provider,
        model: model.clone(),
        openai_reasoning_effort: provider_openai_reasoning_effort(&config, provider)?,
        anthropic_effort: provider_anthropic_effort(&config, provider)?,
        anthropic_thinking: provider_anthropic_thinking(&config, provider)?,
        openai_compatible_base_url: config
            .openai_compatible
            .as_ref()
            .and_then(|config| config.base_url.clone()),
        recent_messages: config.recent_messages,
        recent_bytes: config.recent_bytes,
        recent_tokens: config.recent_tokens,
        context_limit: resolved_context_limit(&config, provider, model.as_deref())?,
        body_mirror: config.body_mirror.unwrap_or(false),
    })
}

fn resolved_context_limit(
    config: &DialogConfig,
    provider: Provider,
    model: Option<&str>,
) -> Result<ContextLimit> {
    let context_limit = ContextLimit {
        context_window_tokens: config.context_window_tokens.unwrap_or_else(|| {
            default_context_window_tokens(provider, model).unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS)
        }),
        context_ratio: config.context_ratio.unwrap_or(DEFAULT_CONTEXT_RATIO),
    };
    if context_limit.context_window_tokens == 0 {
        anyhow::bail!("context_window_tokens must be greater than 0");
    }
    if !(context_limit.context_ratio > 0.0 && context_limit.context_ratio <= 1.0) {
        anyhow::bail!("context_ratio must be greater than 0 and less than or equal to 1");
    }
    Ok(context_limit)
}

pub(crate) fn read_dialog_config(dir: &Path) -> Result<DialogConfig> {
    let path = dir.join("_config.json5");
    if !path.exists() {
        return Ok(DialogConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))
}

pub(crate) fn write_dialog_config(dir: &Path, config: &DialogConfig) -> Result<()> {
    let path = dir.join("_config.json5");
    durable_write(&path, serde_json::to_string_pretty(config)?.as_bytes())
}

pub(crate) fn read_resolved_dialog_config(
    dir: &Path,
    overrides: DialogOptionOverrides,
) -> Result<ResolvedDialogConfig> {
    resolve_dialog_config(read_dialog_config(dir)?, overrides)
}

pub(crate) fn apply_dialog_option_overrides(
    config: &mut DialogConfig,
    overrides: DialogOptionOverrides,
) -> Result<()> {
    let DialogOptionOverrides {
        provider,
        model,
        openai_reasoning,
        anthropic_effort,
        anthropic_thinking,
        openai_compatible_base_url,
        all,
        recent_messages,
        recent_bytes,
        recent_tokens,
        context_window_tokens,
        context_ratio,
        body_mirror,
    } = overrides;

    if let Some(provider) = provider {
        config.provider = Some(provider.to_string());
    }
    let active_provider = configured_provider(config)?;
    if let Some(model) = model {
        set_provider_model(config, active_provider, model)?;
    }
    if let Some(openai_reasoning) = openai_reasoning {
        set_provider_openai_reasoning(config, active_provider, openai_reasoning)?;
    }
    if let Some(anthropic_effort) = anthropic_effort {
        set_provider_anthropic_effort(config, active_provider, anthropic_effort)?;
    }
    if let Some(anthropic_thinking) = anthropic_thinking {
        set_provider_anthropic_thinking(config, active_provider, anthropic_thinking)?;
    }
    if let Some(openai_compatible_base_url) = openai_compatible_base_url {
        config
            .openai_compatible
            .get_or_insert_with(OpenAiCompatibleConfig::default)
            .base_url = Some(openai_compatible_base_url);
    }

    let fold_overrides = usize::from(all)
        + usize::from(recent_messages.is_some())
        + usize::from(recent_bytes.is_some())
        + usize::from(recent_tokens.is_some());
    if fold_overrides > 1 {
        anyhow::bail!(
            "--all, --recent-messages, --recent-bytes, and --recent-tokens select different folds; use only one"
        );
    }
    if all {
        config.recent_messages = None;
        config.recent_bytes = None;
        config.recent_tokens = None;
    } else if recent_messages.is_some() {
        config.recent_bytes = None;
        config.recent_tokens = None;
    } else if recent_bytes.is_some() {
        config.recent_messages = None;
        config.recent_tokens = None;
    } else if recent_tokens.is_some() {
        config.recent_messages = None;
        config.recent_bytes = None;
    }

    if let Some(recent_messages) = recent_messages {
        config.recent_messages = Some(recent_messages);
    }
    if let Some(recent_bytes) = recent_bytes {
        config.recent_bytes = Some(recent_bytes);
    }
    if let Some(recent_tokens) = recent_tokens {
        config.recent_tokens = Some(recent_tokens);
    }
    if let Some(context_window_tokens) = context_window_tokens {
        config.context_window_tokens = Some(context_window_tokens);
    }
    if let Some(context_ratio) = context_ratio {
        config.context_ratio = Some(context_ratio);
    }
    if let Some(body_mirror) = body_mirror {
        config.body_mirror = Some(body_mirror);
    }

    Ok(())
}

fn configured_provider(config: &DialogConfig) -> Result<Provider> {
    match config.provider.as_deref() {
        Some(provider) => provider.parse(),
        None => Ok(Provider::OpenAi),
    }
}

fn provider_model(config: &DialogConfig, provider: Provider) -> Option<String> {
    match provider {
        Provider::OpenAi => config
            .openai
            .as_ref()
            .and_then(|config| config.model.clone()),
        Provider::Anthropic => config
            .anthropic
            .as_ref()
            .and_then(|config| config.model.clone()),
        Provider::OpenAiCompatible => config
            .openai_compatible
            .as_ref()
            .and_then(|config| config.model.clone()),
        Provider::Operator => None,
    }
}

fn set_provider_model(config: &mut DialogConfig, provider: Provider, model: String) -> Result<()> {
    match provider {
        Provider::OpenAi => {
            config
                .openai
                .get_or_insert_with(OpenAiConfig::default)
                .model = Some(model);
        }
        Provider::Anthropic => {
            config
                .anthropic
                .get_or_insert_with(AnthropicConfig::default)
                .model = Some(model);
        }
        Provider::OpenAiCompatible => {
            config
                .openai_compatible
                .get_or_insert_with(OpenAiCompatibleConfig::default)
                .model = Some(model);
        }
        Provider::Operator => {
            anyhow::bail!("--model is not used with provider operator");
        }
    }
    Ok(())
}

fn provider_anthropic_effort(
    config: &DialogConfig,
    provider: Provider,
) -> Result<Option<AnthropicEffort>> {
    match provider {
        Provider::Anthropic => config
            .anthropic
            .as_ref()
            .and_then(|config| config.effort.as_deref())
            .map(str::parse)
            .transpose(),
        Provider::Operator | Provider::OpenAi | Provider::OpenAiCompatible => Ok(None),
    }
}

fn provider_anthropic_thinking(
    config: &DialogConfig,
    provider: Provider,
) -> Result<Option<AnthropicThinking>> {
    match provider {
        Provider::Anthropic => config
            .anthropic
            .as_ref()
            .and_then(|config| config.thinking.as_deref())
            .map(str::parse)
            .transpose(),
        Provider::Operator | Provider::OpenAi | Provider::OpenAiCompatible => Ok(None),
    }
}

fn set_provider_anthropic_effort(
    config: &mut DialogConfig,
    provider: Provider,
    effort: AnthropicEffort,
) -> Result<()> {
    match provider {
        Provider::Anthropic => {
            config
                .anthropic
                .get_or_insert_with(AnthropicConfig::default)
                .effort = Some(effort.to_string());
        }
        Provider::Operator => {
            anyhow::bail!("--anthropic-effort is not used with provider operator");
        }
        Provider::OpenAi => {
            anyhow::bail!("--anthropic-effort is not used with provider openai");
        }
        Provider::OpenAiCompatible => {
            anyhow::bail!("--anthropic-effort is not used with provider openai-compatible");
        }
    }
    Ok(())
}

fn set_provider_anthropic_thinking(
    config: &mut DialogConfig,
    provider: Provider,
    thinking: AnthropicThinking,
) -> Result<()> {
    match provider {
        Provider::Anthropic => {
            config
                .anthropic
                .get_or_insert_with(AnthropicConfig::default)
                .thinking = Some(thinking.to_string());
        }
        Provider::Operator => {
            anyhow::bail!("--anthropic-thinking is not used with provider operator");
        }
        Provider::OpenAi => {
            anyhow::bail!("--anthropic-thinking is not used with provider openai");
        }
        Provider::OpenAiCompatible => {
            anyhow::bail!("--anthropic-thinking is not used with provider openai-compatible");
        }
    }
    Ok(())
}

fn provider_openai_reasoning_effort(
    config: &DialogConfig,
    provider: Provider,
) -> Result<Option<OpenAiReasoningEffort>> {
    match provider {
        Provider::OpenAi => config
            .openai
            .as_ref()
            .and_then(|config| config.reasoning.as_deref())
            .map(str::parse)
            .transpose(),
        Provider::Operator | Provider::OpenAiCompatible | Provider::Anthropic => Ok(None),
    }
}

fn set_provider_openai_reasoning(
    config: &mut DialogConfig,
    provider: Provider,
    reasoning: OpenAiReasoningEffort,
) -> Result<()> {
    match provider {
        Provider::OpenAi => {
            config
                .openai
                .get_or_insert_with(OpenAiConfig::default)
                .reasoning = Some(reasoning.to_string());
        }
        Provider::Operator => {
            anyhow::bail!("--openai-reasoning is not used with provider operator");
        }
        Provider::OpenAiCompatible => {
            anyhow::bail!("--openai-reasoning is not used with provider openai-compatible");
        }
        Provider::Anthropic => {
            anyhow::bail!("--openai-reasoning is not used with provider anthropic");
        }
    }
    Ok(())
}

pub(crate) fn build_fold_override(config: &ResolvedDialogConfig) -> Result<Option<Box<dyn Fold>>> {
    let fold_overrides = usize::from(config.recent_messages.is_some())
        + usize::from(config.recent_bytes.is_some())
        + usize::from(config.recent_tokens.is_some());
    if fold_overrides > 1 {
        anyhow::bail!(
            "recent_messages, recent_bytes, and recent_tokens select different folds; use only one"
        );
    }
    if let Some(k) = config.recent_messages {
        return Ok(Some(Box::new(RecentMessagesFold::new(k))));
    }
    if let Some(budget) = config.recent_bytes {
        return Ok(Some(Box::new(RecentBytesFold::new(budget))));
    }
    if let Some(budget) = config.recent_tokens {
        return Ok(Some(Box::new(RecentTokensFold::new(budget))));
    }
    Ok(None)
}

pub(crate) fn body_mirror_override(body_mirror: bool) -> Option<bool> {
    if body_mirror { Some(true) } else { None }
}
