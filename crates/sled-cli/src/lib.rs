use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sled_ai::{ModelOptions, Provider, create_model_with_options, default_model};
use sled_core::Fold;
use sled_core::{
    StepOutcome, WriteOptions, durable_write, preview_model_input, run_until_stop_with_options,
    say_with_options, status_report, write_default_system_config, write_system_prompt,
};
use sled_fold::{AllFold, RecentBytesFold, RecentMessagesFold};
use sled_tools::ToolRegistry;
use std::fs;
use std::path::{Path, PathBuf};
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
        #[arg(long, help = "Save provider override")]
        provider: Option<Provider>,
        #[arg(long, help = "Save model override for the selected provider")]
        model: Option<String>,
        #[arg(
            long = "openai-compatible-base-url",
            help = "Save base URL for openai-compatible providers"
        )]
        openai_compatible_base_url: Option<String>,
        #[arg(long, help = "Clear saved context limits and use full message context")]
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
        #[arg(long, help = "Save markdown body mirrors as enabled")]
        body_mirror: bool,
    },
    Run {
        dir: PathBuf,
        #[arg(long, help = "Provider to use (default: openai)")]
        provider: Option<Provider>,
        #[arg(
            long,
            help = "Model override for the selected provider (defaults: openai=gpt-5.5, anthropic=claude-sonnet-4-6; openai-compatible requires one)"
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

pub struct Profile {
    pub fold: Box<dyn Fold>,
    pub tools: ToolRegistry,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct DialogConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openai: Option<ProviderModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anthropic: Option<ProviderModelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openai_compatible: Option<OpenAiCompatibleConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recent_messages: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recent_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_mirror: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ProviderModelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct OpenAiCompatibleConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_url: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            fold: Box::new(AllFold),
            tools: ToolRegistry::with_defaults(),
        }
    }
}

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
            let _ = build_fold_override(&resolved)?;
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
            let overrides = DialogOptionOverrides {
                provider,
                model,
                openai_compatible_base_url,
                all,
                recent_messages,
                recent_bytes,
                body_mirror: body_mirror_override(body_mirror),
            };
            let (config, _) = read_resolved_dialog_config(&dir, overrides)?;
            run_dialog(&dir, &profile, run_options_from_resolved_config(config)?).await?;
        }
        Command::Status { dir } => {
            print!("{}", status_report(&dir)?);
        }
        Command::Context { dir } => {
            std::fs::create_dir_all(&dir)?;
            let (config, _) = read_resolved_dialog_config(&dir, DialogOptionOverrides::default())?;
            let fold_override = build_fold_override(&config)?;
            let fold = selected_fold(&profile, fold_override.as_deref());
            let (system, context) = preview_model_input(&dir, fold)?;
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
    body_mirror: bool,
    fold_override: Option<Box<dyn Fold>>,
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

#[derive(Clone, Default)]
struct DialogOptionOverrides {
    provider: Option<Provider>,
    model: Option<String>,
    openai_compatible_base_url: Option<String>,
    all: bool,
    recent_messages: Option<usize>,
    recent_bytes: Option<usize>,
    body_mirror: Option<bool>,
}

async fn run_dialog(dir: &Path, profile: &Profile, options: RunOptions) -> Result<()> {
    let model = create_model_with_options(
        options.provider,
        ModelOptions {
            model: options.model,
            openai_compatible_base_url: options.openai_compatible_base_url,
            temperature: None,
        },
    )?;
    let fold = selected_fold(profile, options.fold_override.as_deref());
    match run_until_stop_with_options(
        dir,
        model.as_ref(),
        &profile.tools,
        fold,
        WriteOptions {
            body_mirror: options.body_mirror,
        },
    )
    .await?
    {
        StepOutcome::NeedsInput(path) => println!("needs input: {}", path.display()),
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
    let provider = configured_provider(&config)?;

    Ok(ResolvedDialogConfig {
        provider,
        model: provider_model(&config, provider)
            .or_else(|| default_model(provider).map(str::to_string)),
        openai_compatible_base_url: config
            .openai_compatible
            .as_ref()
            .and_then(|config| config.base_url.clone()),
        recent_messages: config.recent_messages,
        recent_bytes: config.recent_bytes,
        body_mirror: config.body_mirror.unwrap_or(false),
    })
}

fn read_dialog_config(dir: &Path) -> Result<DialogConfig> {
    let path = dir.join("_config.json5");
    if !path.exists() {
        return Ok(DialogConfig::default());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("could not read {}", path.display()))?;
    json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))
}

fn write_dialog_config(dir: &Path, config: &DialogConfig) -> Result<()> {
    let path = dir.join("_config.json5");
    durable_write(&path, serde_json::to_string_pretty(config)?.as_bytes())
}

fn read_resolved_dialog_config(
    dir: &Path,
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

#[cfg(test)]
fn dialog_config_from_overrides(overrides: DialogOptionOverrides) -> Result<DialogConfig> {
    let mut config = DialogConfig::default();
    apply_dialog_option_overrides(&mut config, overrides)?;
    Ok(config)
}

fn run_options_from_resolved_config(config: ResolvedDialogConfig) -> Result<RunOptions> {
    let fold_override = build_fold_override(&config)?;
    Ok(RunOptions {
        provider: config.provider,
        model: config.model,
        openai_compatible_base_url: config.openai_compatible_base_url,
        body_mirror: config.body_mirror,
        fold_override,
    })
}

fn apply_dialog_option_overrides(
    config: &mut DialogConfig,
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
    let active_provider = configured_provider(config)?;
    if let Some(model) = model {
        set_provider_model(config, active_provider, model)?;
    }
    if let Some(openai_compatible_base_url) = openai_compatible_base_url {
        config
            .openai_compatible
            .get_or_insert_with(OpenAiCompatibleConfig::default)
            .base_url = Some(openai_compatible_base_url);
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
                .get_or_insert_with(ProviderModelConfig::default)
                .model = Some(model);
        }
        Provider::Anthropic => {
            config
                .anthropic
                .get_or_insert_with(ProviderModelConfig::default)
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

fn build_fold_override(config: &ResolvedDialogConfig) -> Result<Option<Box<dyn Fold>>> {
    match (config.recent_messages, config.recent_bytes) {
        (Some(_), Some(_)) => {
            anyhow::bail!("recent_messages and recent_bytes select different folds; use only one")
        }
        (Some(k), None) => Ok(Some(Box::new(RecentMessagesFold::new(k)))),
        (None, Some(budget)) => Ok(Some(Box::new(RecentBytesFold::new(budget)))),
        (None, None) => Ok(None),
    }
}

fn selected_fold<'a>(profile: &'a Profile, fold_override: Option<&'a dyn Fold>) -> &'a dyn Fold {
    fold_override.unwrap_or(profile.fold.as_ref())
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
    fn resolving_missing_config_does_not_create_config_file() {
        let dir = temp_dir();
        fs::create_dir_all(&dir).unwrap();

        let (resolved, file_exists) =
            read_resolved_dialog_config(&dir, DialogOptionOverrides::default()).unwrap();

        assert!(!file_exists);
        assert!(matches!(resolved.provider, Provider::OpenAi));
        assert!(!dir.join("_config.json5").exists());
    }

    #[test]
    fn explicit_provider_override_serializes_without_defaults() {
        let config = dialog_config_from_overrides(DialogOptionOverrides {
            provider: Some(Provider::Anthropic),
            ..DialogOptionOverrides::default()
        })
        .unwrap();

        assert_eq!(config.provider.as_deref(), Some("anthropic"));
        assert!(config.openai.is_none());
        assert!(config.anthropic.is_none());
        assert!(config.body_mirror.is_none());
    }

    #[test]
    fn explicit_model_override_serializes_under_selected_provider() {
        let config = dialog_config_from_overrides(DialogOptionOverrides {
            provider: Some(Provider::Anthropic),
            model: Some("claude-test".into()),
            ..DialogOptionOverrides::default()
        })
        .unwrap();

        assert_eq!(
            config.anthropic.and_then(|config| config.model).as_deref(),
            Some("claude-test")
        );
        assert!(config.openai.is_none());
    }

    #[test]
    fn partial_openai_compatible_config_is_valid_as_saved_config() {
        let config = dialog_config_from_overrides(DialogOptionOverrides {
            provider: Some(Provider::OpenAiCompatible),
            ..DialogOptionOverrides::default()
        })
        .unwrap();
        let resolved = resolve_dialog_config(config, DialogOptionOverrides::default()).unwrap();

        assert!(matches!(resolved.provider, Provider::OpenAiCompatible));
        assert!(resolved.model.is_none());
        assert!(resolved.openai_compatible_base_url.is_none());
        assert!(build_fold_override(&resolved).unwrap().is_none());
    }

    #[test]
    fn model_config_is_scoped_to_selected_provider() {
        let resolved = resolve_dialog_config(
            DialogConfig {
                provider: Some("openai".into()),
                openai: Some(ProviderModelConfig {
                    model: Some("gpt-5.5".into()),
                }),
                ..DialogConfig::default()
            },
            DialogOptionOverrides {
                provider: Some(Provider::Anthropic),
                ..DialogOptionOverrides::default()
            },
        )
        .unwrap();

        assert!(matches!(resolved.provider, Provider::Anthropic));
        assert_eq!(resolved.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn model_override_is_saved_under_active_provider() {
        let mut config = DialogConfig {
            provider: Some("anthropic".into()),
            ..DialogConfig::default()
        };

        apply_dialog_option_overrides(
            &mut config,
            DialogOptionOverrides {
                model: Some("claude-test".into()),
                ..DialogOptionOverrides::default()
            },
        )
        .unwrap();

        assert_eq!(
            config.anthropic.and_then(|config| config.model).as_deref(),
            Some("claude-test")
        );
        assert!(config.openai.is_none());
    }
}
