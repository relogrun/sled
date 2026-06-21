use crate::config::{ResolvedDialogConfig, build_fold_override};
use crate::profile::Profile;
use anyhow::Result;
use sled_ai::{ModelOptions, Provider, create_model_with_options};
use sled_core::{ContextLimit, Fold, RuntimeOptions, StepOutcome, WriteOptions, run_until_stop};
use std::path::Path;

pub(crate) struct RunOptions {
    provider: Provider,
    model: Option<String>,
    openai_reasoning_effort: Option<sled_ai::OpenAiReasoningEffort>,
    anthropic_effort: Option<sled_ai::AnthropicEffort>,
    anthropic_thinking: Option<sled_ai::AnthropicThinking>,
    openai_compatible_base_url: Option<String>,
    body_mirror: bool,
    context_limit: ContextLimit,
    fold_override: Option<Box<dyn Fold>>,
}

pub(crate) async fn run_dialog(dir: &Path, profile: &Profile, options: RunOptions) -> Result<()> {
    let model = create_model_with_options(
        options.provider,
        ModelOptions {
            model: options.model,
            openai_compatible_base_url: options.openai_compatible_base_url,
            openai_reasoning_effort: options.openai_reasoning_effort,
            anthropic_effort: options.anthropic_effort,
            anthropic_thinking: options.anthropic_thinking,
            temperature: None,
        },
    )?;
    let fold = selected_fold(profile, options.fold_override.as_deref());
    let available_tools = available_tools_prompt(profile);
    match run_until_stop(
        dir,
        model.as_ref(),
        &profile.tools,
        fold,
        RuntimeOptions {
            write_options: WriteOptions {
                body_mirror: options.body_mirror,
            },
            available_tools,
            context_limit: options.context_limit,
        },
    )
    .await?
    {
        StepOutcome::Awaiting(path) => println!("awaiting input: {}", path.display()),
        StepOutcome::Finished(Some(num)) => println!("finished at {num:04}"),
        StepOutcome::Finished(None) => println!("finished"),
        StepOutcome::Continue => unreachable!(),
    }
    Ok(())
}

pub(crate) fn available_tools_prompt(profile: &Profile) -> Option<String> {
    profile.tools.tool_descriptions_prompt()
}

pub(crate) fn run_options_from_resolved_config(config: ResolvedDialogConfig) -> Result<RunOptions> {
    let fold_override = build_fold_override(&config)?;
    Ok(RunOptions {
        provider: config.provider,
        model: config.model,
        openai_reasoning_effort: config.openai_reasoning_effort,
        anthropic_effort: config.anthropic_effort,
        anthropic_thinking: config.anthropic_thinking,
        openai_compatible_base_url: config.openai_compatible_base_url,
        body_mirror: config.body_mirror,
        context_limit: config.context_limit,
        fold_override,
    })
}

pub(crate) fn selected_fold<'a>(
    profile: &'a Profile,
    fold_override: Option<&'a dyn Fold>,
) -> &'a dyn Fold {
    fold_override.unwrap_or(profile.fold.as_ref())
}
