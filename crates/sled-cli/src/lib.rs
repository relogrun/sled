use anyhow::Result;
use clap::{Parser, Subcommand};
use sled_ai::{ModelOptions, Provider, create_model_with_options, default_model};
use sled_core::{
    DEFAULT_SYSTEM_PROMPT, DialogConfig, StepOutcome, SystemConfig, WriteOptions,
    preview_model_input, read_dialog_config, run_until_stop_with_options, say_with_options,
    status_report, write_default_system_config, write_dialog_config, write_system_config,
};
use sled_fold::{AllFold, RecentBytesFold, RecentMessagesFold};
use sled_tools::ToolRegistry;
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Parser)]
#[command(name = "sled")]
#[command(about = "File-based AI dialog runner")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
        #[arg(long, help = "Save provider override (default if unset: openai)")]
        provider: Option<Provider>,
        #[arg(
            long,
            help = "Save model override (defaults: openai=gpt-5.5, anthropic=claude-sonnet-4-6; openai-compatible requires one)"
        )]
        model: Option<String>,
        #[arg(
            long = "openai-compatible-base-url",
            help = "Save base URL for openai-compatible providers"
        )]
        openai_compatible_base_url: Option<String>,
        #[arg(
            long,
            help = "Clear context limits and use full message context (default)"
        )]
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
        #[arg(long, help = "Save markdown body mirrors as enabled (default: off)")]
        body_mirror: bool,
    },
    Run {
        dir: PathBuf,
        #[arg(long, help = "Provider to use (default: openai)")]
        provider: Option<Provider>,
        #[arg(
            long,
            help = "Model override (defaults: openai=gpt-5.5, anthropic=claude-sonnet-4-6; openai-compatible requires one)"
        )]
        model: Option<String>,
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
    },
}

pub async fn run_cli() -> Result<()> {
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
                write_system_config(&dir, &SystemConfig { prompt })?;
            } else {
                write_default_system_config(&dir)?;
            }
            let config =
                resolve_dialog_config(DialogConfig::default(), DialogOptionOverrides::default())?;
            write_dialog_config_if_missing(&dir, &config)?;
            println!("initialized {}", dir.display());
        }
        Command::Say {
            dir,
            text,
            run,
            body_mirror,
        } => {
            std::fs::create_dir_all(&dir)?;
            let config = read_or_create_dialog_config(
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
                run_dialog(&dir, run_options_from_resolved_config(config)?).await?;
            }
        }
        Command::Config {
            dir,
            provider,
            model,
            openai_compatible_base_url,
            all,
            recent_messages,
            recent_bytes,
            body_mirror,
        } => {
            std::fs::create_dir_all(&dir)?;
            let mut config = read_dialog_config(&dir)?;
            apply_dialog_option_overrides(
                &mut config,
                DialogOptionOverrides {
                    provider,
                    model,
                    openai_compatible_base_url,
                    all,
                    recent_messages,
                    recent_bytes,
                    body_mirror: body_mirror_override(body_mirror),
                },
            )?;
            let resolved = resolve_dialog_config(config.clone(), DialogOptionOverrides::default())?;
            validate_run_config(&resolved)?;
            let _ = build_fold(&resolved)?;
            write_dialog_config(&dir, &config)?;
            println!("wrote {}", dir.join("_config.json5").display());
        }
        Command::Run {
            dir,
            provider,
            model,
            openai_compatible_base_url,
            all,
            recent_messages,
            recent_bytes,
            body_mirror,
        } => {
            std::fs::create_dir_all(&dir)?;
            let (config, config_exists) = read_resolved_dialog_config(
                &dir,
                DialogOptionOverrides {
                    provider,
                    model,
                    openai_compatible_base_url,
                    all,
                    recent_messages,
                    recent_bytes,
                    body_mirror: body_mirror_override(body_mirror),
                },
            )?;
            validate_run_config(&config)?;
            if !config_exists {
                write_dialog_config(&dir, &dialog_config_from_resolved(&config))?;
            }
            run_dialog(&dir, run_options_from_resolved_config(config)?).await?;
        }
        Command::Status { dir } => {
            print!("{}", status_report(&dir)?);
        }
        Command::Context { dir } => {
            std::fs::create_dir_all(&dir)?;
            let config = read_or_create_dialog_config(&dir, DialogOptionOverrides::default())?;
            let fold = build_fold(&config)?;
            let (system, context) =
                preview_model_input(&dir, DEFAULT_SYSTEM_PROMPT, fold.as_ref())?;
            println!("=== system ===\n{}\n", system);
            println!("=== index ===\n{}", context.index);
            println!("=== bodies ===\n{}", context.bodies);
        }
    }

    Ok(())
}

struct RunOptions {
    provider: Provider,
    model: Option<String>,
    openai_compatible_base_url: Option<String>,
    fold: Box<dyn sled_core::Fold>,
    body_mirror: bool,
}

#[derive(Clone, Debug)]
struct ResolvedDialogConfig {
    provider: Provider,
    model: Option<String>,
    openai_compatible_base_url: Option<String>,
    recent_messages: Option<usize>,
    recent_bytes: Option<usize>,
    body_mirror: bool,
}

#[derive(Default)]
struct DialogOptionOverrides {
    provider: Option<Provider>,
    model: Option<String>,
    openai_compatible_base_url: Option<String>,
    all: bool,
    recent_messages: Option<usize>,
    recent_bytes: Option<usize>,
    body_mirror: Option<bool>,
}

async fn run_dialog(dir: &PathBuf, options: RunOptions) -> Result<()> {
    let model = create_model_with_options(
        options.provider,
        ModelOptions {
            model: options.model,
            openai_compatible_base_url: options.openai_compatible_base_url,
        },
    )?;
    let tools = ToolRegistry::with_defaults();
    match run_until_stop_with_options(
        dir,
        model.as_ref(),
        &tools,
        DEFAULT_SYSTEM_PROMPT,
        options.fold.as_ref(),
        WriteOptions {
            body_mirror: options.body_mirror,
        },
    )
    .await?
    {
        StepOutcome::Input(path) => println!("input requested: {}", path.display()),
        StepOutcome::Finished(Some(num)) => println!("finished at {num:04}"),
        StepOutcome::Finished(None) => println!("finished"),
        StepOutcome::Continue => unreachable!(),
    }
    Ok(())
}

fn resolve_dialog_config(
    mut config: DialogConfig,
    overrides: DialogOptionOverrides,
) -> Result<ResolvedDialogConfig> {
    apply_dialog_option_overrides(&mut config, overrides)?;
    let provider = match config.provider.as_deref() {
        Some(provider) => provider.parse()?,
        None => Provider::OpenAi,
    };

    Ok(ResolvedDialogConfig {
        provider,
        model: config
            .model
            .clone()
            .or_else(|| default_model(provider).map(str::to_string)),
        openai_compatible_base_url: config.openai_compatible_base_url.clone(),
        recent_messages: config.recent_messages,
        recent_bytes: config.recent_bytes,
        body_mirror: config.body_mirror.unwrap_or(false),
    })
}

fn read_or_create_dialog_config(
    dir: &PathBuf,
    overrides: DialogOptionOverrides,
) -> Result<ResolvedDialogConfig> {
    let (resolved, file_exists) = read_resolved_dialog_config(dir, overrides)?;
    if !file_exists {
        write_dialog_config(dir, &dialog_config_from_resolved(&resolved))?;
    }
    Ok(resolved)
}

fn read_resolved_dialog_config(
    dir: &PathBuf,
    overrides: DialogOptionOverrides,
) -> Result<(ResolvedDialogConfig, bool)> {
    let path = dir.join("_config.json5");
    let file_exists = path.exists();
    let config = if file_exists {
        read_dialog_config(dir)?
    } else {
        DialogConfig::default()
    };
    let resolved = resolve_dialog_config(config, overrides)?;
    Ok((resolved, file_exists))
}

fn write_dialog_config_if_missing(dir: &PathBuf, config: &ResolvedDialogConfig) -> Result<()> {
    if !dir.join("_config.json5").exists() {
        write_dialog_config(dir, &dialog_config_from_resolved(config))?;
    }
    Ok(())
}

fn dialog_config_from_resolved(config: &ResolvedDialogConfig) -> DialogConfig {
    DialogConfig {
        provider: Some(config.provider.to_string()),
        model: config.model.clone(),
        openai_compatible_base_url: config.openai_compatible_base_url.clone(),
        recent_messages: config.recent_messages,
        recent_bytes: config.recent_bytes,
        body_mirror: Some(config.body_mirror),
    }
}

fn run_options_from_resolved_config(config: ResolvedDialogConfig) -> Result<RunOptions> {
    validate_run_config(&config)?;
    let fold = build_fold(&config)?;
    Ok(RunOptions {
        provider: config.provider,
        model: config.model,
        openai_compatible_base_url: config.openai_compatible_base_url,
        fold,
        body_mirror: config.body_mirror,
    })
}

fn validate_run_config(config: &ResolvedDialogConfig) -> Result<()> {
    if !matches!(config.provider, Provider::OpenAiCompatible) {
        return Ok(());
    }
    if config
        .model
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        anyhow::bail!("--model or _config.model is required for openai-compatible");
    }
    if config
        .openai_compatible_base_url
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        anyhow::bail!(
            "--openai-compatible-base-url or _config.openai_compatible_base_url is required"
        );
    }
    Ok(())
}

fn apply_dialog_option_overrides(
    config: &mut sled_core::DialogConfig,
    overrides: DialogOptionOverrides,
) -> Result<()> {
    let DialogOptionOverrides {
        provider,
        model,
        openai_compatible_base_url,
        all,
        recent_messages,
        recent_bytes,
        body_mirror,
    } = overrides;

    if let Some(provider) = provider {
        config.provider = Some(provider.to_string());
    }
    if let Some(model) = model {
        config.model = Some(model);
    }
    if let Some(openai_compatible_base_url) = openai_compatible_base_url {
        config.openai_compatible_base_url = Some(openai_compatible_base_url);
    }

    let fold_overrides = usize::from(all)
        + usize::from(recent_messages.is_some())
        + usize::from(recent_bytes.is_some());
    if fold_overrides > 1 {
        anyhow::bail!(
            "--all, --recent-messages, and --recent-bytes select different folds; use only one"
        );
    }
    if all {
        config.recent_messages = None;
        config.recent_bytes = None;
    } else if recent_messages.is_some() {
        config.recent_bytes = None;
    } else if recent_bytes.is_some() {
        config.recent_messages = None;
    }

    if let Some(recent_messages) = recent_messages {
        config.recent_messages = Some(recent_messages);
    }
    if let Some(recent_bytes) = recent_bytes {
        config.recent_bytes = Some(recent_bytes);
    }
    if let Some(body_mirror) = body_mirror {
        config.body_mirror = Some(body_mirror);
    }

    Ok(())
}

fn build_fold(config: &ResolvedDialogConfig) -> Result<Box<dyn sled_core::Fold>> {
    match (config.recent_messages, config.recent_bytes) {
        (Some(_), Some(_)) => {
            anyhow::bail!("recent_messages and recent_bytes select different folds; use only one")
        }
        (Some(k), None) => Ok(Box::new(RecentMessagesFold::new(k))),
        (None, Some(budget)) => Ok(Box::new(RecentBytesFold::new(budget))),
        (None, None) => Ok(Box::new(AllFold)),
    }
}

fn body_mirror_override(body_mirror: bool) -> Option<bool> {
    if body_mirror { Some(true) } else { None }
}

fn system_prompt(system: Option<String>, system_file: Option<PathBuf>) -> Result<Option<String>> {
    match (system, system_file) {
        (Some(prompt), None) => Ok(Some(prompt)),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(path)?)),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => unreachable!("clap prevents conflicting init prompt options"),
    }
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let seq = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("sled-cli-test-{id}-{seq}"))
    }

    #[test]
    fn write_dialog_config_if_missing_does_not_replace_existing_config() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();
        write_dialog_config(
            &dir,
            &DialogConfig {
                recent_messages: Some(3),
                ..DialogConfig::default()
            },
        )
        .unwrap();

        let resolved =
            resolve_dialog_config(DialogConfig::default(), DialogOptionOverrides::default())
                .unwrap();
        write_dialog_config_if_missing(&dir, &resolved).unwrap();

        let config = read_dialog_config(&dir).unwrap();
        assert_eq!(config.recent_messages, Some(3));
    }

    #[test]
    fn openai_compatible_run_config_requires_model_and_base_url() {
        let mut config = ResolvedDialogConfig {
            provider: Provider::OpenAiCompatible,
            model: None,
            openai_compatible_base_url: None,
            recent_messages: None,
            recent_bytes: None,
            body_mirror: false,
        };
        assert!(validate_run_config(&config).is_err());

        config.model = Some("openai/gpt-4o-mini".into());
        assert!(validate_run_config(&config).is_err());

        config.openai_compatible_base_url = Some("https://openrouter.ai/api/v1".into());
        assert!(validate_run_config(&config).is_ok());
    }
}
