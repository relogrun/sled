use crate::config::{
    DialogConfig, DialogOptionOverrides, OpenAiConfig, apply_dialog_option_overrides,
    build_fold_override, dialog_config_from_overrides, read_resolved_dialog_config,
    resolve_dialog_config,
};
use sled_ai::{AnthropicEffort, AnthropicThinking, OpenAiReasoningEffort, Provider};
use sled_core::{ContextLimit, DEFAULT_CONTEXT_RATIO, DEFAULT_CONTEXT_WINDOW_TOKENS};
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

    let resolved = read_resolved_dialog_config(&dir, DialogOptionOverrides::default()).unwrap();

    assert!(matches!(resolved.provider, Provider::OpenAi));
    assert_eq!(
        resolved.context_limit,
        ContextLimit {
            context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
            context_ratio: DEFAULT_CONTEXT_RATIO,
        }
    );
    assert!(!dir.join("_config.json5").exists());
}

#[test]
fn context_limit_overrides_are_saved_and_resolved() {
    let config = dialog_config_from_overrides(DialogOptionOverrides {
        context_window_tokens: Some(64_000),
        context_ratio: Some(0.75),
        ..DialogOptionOverrides::default()
    })
    .unwrap();
    assert_eq!(config.context_window_tokens, Some(64_000));
    assert_eq!(config.context_ratio, Some(0.75));

    let resolved = resolve_dialog_config(config, DialogOptionOverrides::default()).unwrap();
    assert_eq!(
        resolved.context_limit,
        ContextLimit {
            context_window_tokens: 64_000,
            context_ratio: 0.75,
        }
    );
}

#[test]
fn context_limit_rejects_invalid_ratio() {
    let err = dialog_config_from_overrides(DialogOptionOverrides {
        context_ratio: Some(0.0),
        ..DialogOptionOverrides::default()
    })
    .and_then(|config| resolve_dialog_config(config, DialogOptionOverrides::default()))
    .unwrap_err()
    .to_string();

    assert_eq!(
        err,
        "context_ratio must be greater than 0 and less than or equal to 1"
    );
}

#[test]
fn recent_tokens_override_is_saved_as_fold_selection() {
    let config = dialog_config_from_overrides(DialogOptionOverrides {
        recent_tokens: Some(2048),
        ..DialogOptionOverrides::default()
    })
    .unwrap();

    assert_eq!(config.recent_tokens, Some(2048));
    assert!(config.recent_messages.is_none());
    assert!(config.recent_bytes.is_none());
    let resolved = resolve_dialog_config(config, DialogOptionOverrides::default()).unwrap();
    assert_eq!(resolved.recent_tokens, Some(2048));
    assert!(build_fold_override(&resolved).unwrap().is_some());
}

#[test]
fn fold_selection_overrides_are_mutually_exclusive() {
    let err = dialog_config_from_overrides(DialogOptionOverrides {
        recent_messages: Some(2),
        recent_tokens: Some(2048),
        ..DialogOptionOverrides::default()
    })
    .unwrap_err()
    .to_string();

    assert_eq!(
        err,
        "--all, --recent-messages, --recent-bytes, and --recent-tokens select different folds; use only one"
    );
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
            openai: Some(OpenAiConfig {
                model: Some("gpt-5.5".into()),
                reasoning: None,
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
fn openai_reasoning_override_is_saved_under_openai() {
    let config = dialog_config_from_overrides(DialogOptionOverrides {
        openai_reasoning: Some(OpenAiReasoningEffort::Low),
        ..DialogOptionOverrides::default()
    })
    .unwrap();

    assert_eq!(
        config.openai.and_then(|config| config.reasoning).as_deref(),
        Some("low")
    );
}

#[test]
fn openai_reasoning_override_is_openai_only() {
    let err = dialog_config_from_overrides(DialogOptionOverrides {
        provider: Some(Provider::Anthropic),
        openai_reasoning: Some(OpenAiReasoningEffort::Low),
        ..DialogOptionOverrides::default()
    })
    .unwrap_err()
    .to_string();

    assert_eq!(
        err,
        "--openai-reasoning is not used with provider anthropic"
    );
}

#[test]
fn anthropic_options_are_saved_under_anthropic() {
    let config = dialog_config_from_overrides(DialogOptionOverrides {
        provider: Some(Provider::Anthropic),
        anthropic_effort: Some(AnthropicEffort::Medium),
        anthropic_thinking: Some(AnthropicThinking::Adaptive),
        ..DialogOptionOverrides::default()
    })
    .unwrap();

    let anthropic = config.anthropic.unwrap();
    assert_eq!(anthropic.effort.as_deref(), Some("medium"));
    assert_eq!(anthropic.thinking.as_deref(), Some("adaptive"));
    assert!(config.openai.is_none());
}

#[test]
fn anthropic_options_are_anthropic_only() {
    let err = dialog_config_from_overrides(DialogOptionOverrides {
        anthropic_effort: Some(AnthropicEffort::Low),
        ..DialogOptionOverrides::default()
    })
    .unwrap_err()
    .to_string();

    assert_eq!(err, "--anthropic-effort is not used with provider openai");
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
