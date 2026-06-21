mod http;
mod operator;
mod options;
mod provider;
mod providers;
mod reply;

use anyhow::{Context as _, Result, bail};
use sled_core::Model;
use tracing::info;

pub use operator::OperatorModel;
pub use options::{AnthropicEffort, AnthropicThinking, ModelOptions, OpenAiReasoningEffort};
pub use provider::{Provider, default_context_window_tokens, default_model};

pub fn create_model_with_options(
    provider: Provider,
    options: ModelOptions,
) -> Result<Box<dyn Model>> {
    match provider {
        Provider::Operator => {
            info!(provider = %provider, "creating model client");
            Ok(Box::new(OperatorModel))
        }
        Provider::OpenAi => {
            let api_key = std::env::var("OPENAI_API_KEY").context("OPENAI_API_KEY is required")?;
            let model = options.model.unwrap_or_else(|| {
                default_model(provider)
                    .expect("OpenAI has a default")
                    .into()
            });
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(providers::openai::OpenAiResponsesModel::new(
                api_key,
                model,
                options.openai_reasoning_effort,
                options.temperature,
            )))
        }
        Provider::OpenAiCompatible => {
            let base_url = required_non_empty(
                options.openai_compatible_base_url,
                "--openai-compatible-base-url or _config.openai_compatible.base_url is required",
            )?;
            let model = required_non_empty(
                options.model,
                "--model or _config.openai_compatible.model is required",
            )?;
            let api_key = std::env::var("SLED_OPENAI_COMPAT_API_KEY")
                .context("SLED_OPENAI_COMPAT_API_KEY is required")?;
            let endpoint = providers::openai_compatible::chat_completions_endpoint(&base_url);
            info!(provider = %provider, model = %model, endpoint = %endpoint, "creating model client");
            Ok(Box::new(
                providers::openai_compatible::OpenAiCompatibleModel::new(
                    api_key,
                    model,
                    endpoint,
                    options.temperature,
                ),
            ))
        }
        Provider::Anthropic => {
            let api_key =
                std::env::var("ANTHROPIC_API_KEY").context("ANTHROPIC_API_KEY is required")?;
            let model = options.model.unwrap_or_else(|| {
                default_model(provider)
                    .expect("Anthropic has a default")
                    .into()
            });
            info!(provider = %provider, model = %model, "creating model client");
            Ok(Box::new(providers::anthropic::AnthropicModel::new(
                api_key,
                model,
                options.anthropic_effort,
                options.anthropic_thinking,
                options.temperature,
            )))
        }
    }
}

fn required_non_empty(value: Option<String>, message: &'static str) -> Result<String> {
    let value = value.context(message)?;
    let value = value.trim();
    if value.is_empty() {
        bail!(message);
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests;
