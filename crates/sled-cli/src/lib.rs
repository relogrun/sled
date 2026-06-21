mod args;
mod config;
mod init;
mod logging;
mod profile;
mod run;

use crate::args::{Cli, Command};
use crate::config::{
    DialogOptionOverrides, apply_dialog_option_overrides, body_mirror_override,
    build_fold_override, read_dialog_config, read_resolved_dialog_config, resolve_dialog_config,
    write_dialog_config,
};
use crate::init::system_prompt;
use crate::logging::init_logging;
use crate::run::{
    run_dialog, run_options_from_resolved_config, selected_fold, system_prompt_fragments,
};
use anyhow::Result;
use clap::Parser;
use sled_core::{
    WriteOptions, preview_model_input_with_limit, say_with_options, status_report,
    write_default_system_config, write_system_prompt,
};

pub use profile::Profile;

pub async fn run_default_cli() -> Result<()> {
    run_cli(Profile::default()).await
}

pub async fn run_cli(profile: Profile) -> Result<()> {
    dotenvy::dotenv().ok();
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Command::Init {
            dir,
            system,
            system_file,
        } => {
            std::fs::create_dir_all(&dir)?;
            if let Some(prompt) = system_prompt(system, system_file)? {
                write_system_prompt(&dir, prompt)?;
            } else {
                write_default_system_config(&dir)?;
            }
            println!("initialized {}", dir.display());
        }
        Command::Say {
            dir,
            text,
            run,
            body_mirror,
        } => {
            std::fs::create_dir_all(&dir)?;
            let (config, _) = read_resolved_dialog_config(
                &dir,
                DialogOptionOverrides {
                    body_mirror: body_mirror_override(body_mirror),
                    ..DialogOptionOverrides::default()
                },
            )?;
            let body_mirror = config.body_mirror;
            let path = say_with_options(&dir, &text, WriteOptions { body_mirror })?;
            println!("wrote {}", path.display());
            if run {
                run_dialog(&dir, &profile, run_options_from_resolved_config(config)?).await?;
            }
        }
        Command::Config {
            dir,
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
        } => {
            std::fs::create_dir_all(&dir)?;
            let mut config = read_dialog_config(&dir)?;
            apply_dialog_option_overrides(
                &mut config,
                DialogOptionOverrides {
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
                    body_mirror: body_mirror_override(body_mirror),
                },
            )?;
            let resolved = resolve_dialog_config(config.clone(), DialogOptionOverrides::default())?;
            let _ = build_fold_override(&resolved)?;
            write_dialog_config(&dir, &config)?;
            println!("wrote {}", dir.join("_config.json5").display());
        }
        Command::Run {
            dir,
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
        } => {
            std::fs::create_dir_all(&dir)?;
            let overrides = DialogOptionOverrides {
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
                body_mirror: body_mirror_override(body_mirror),
            };
            let (config, _) = read_resolved_dialog_config(&dir, overrides)?;
            run_dialog(&dir, &profile, run_options_from_resolved_config(config)?).await?;
        }
        Command::Status { dir } => {
            print!("{}", status_report(&dir)?);
        }
        Command::Context {
            dir,
            context_window_tokens,
            context_ratio,
        } => {
            std::fs::create_dir_all(&dir)?;
            let (config, _) = read_resolved_dialog_config(
                &dir,
                DialogOptionOverrides {
                    context_window_tokens,
                    context_ratio,
                    ..DialogOptionOverrides::default()
                },
            )?;
            let fold_override = build_fold_override(&config)?;
            let fold = selected_fold(&profile, fold_override.as_deref());
            let system_fragments = system_prompt_fragments(&profile);
            let input = preview_model_input_with_limit(
                &dir,
                fold,
                &system_fragments,
                config.context_limit,
            )?;
            println!("{}\n", input.system);
            println!("=== index ===\n{}", input.context.index);
            println!("=== bodies ===\n{}", input.context.bodies);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
