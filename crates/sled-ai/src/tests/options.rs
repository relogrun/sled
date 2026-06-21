use crate::{
    AnthropicEffort, AnthropicThinking, OpenAiReasoningEffort, Provider,
    default_context_window_tokens,
};

#[test]
fn parses_openai_compatible_provider() {
    let provider = "openai-compatible".parse::<Provider>().unwrap();
    assert!(matches!(provider, Provider::OpenAiCompatible));
    assert_eq!(provider.to_string(), "openai-compatible");
}

#[test]
fn parses_openai_reasoning_effort() {
    let effort = "low".parse::<OpenAiReasoningEffort>().unwrap();

    assert_eq!(effort, OpenAiReasoningEffort::Low);
    assert_eq!(effort.to_string(), "low");
}

#[test]
fn parses_anthropic_effort_and_thinking() {
    assert_eq!(
        "xhigh".parse::<AnthropicEffort>().unwrap(),
        AnthropicEffort::XHigh
    );
    assert_eq!(
        "adaptive".parse::<AnthropicThinking>().unwrap(),
        AnthropicThinking::Adaptive
    );
}

#[test]
fn known_openai_models_have_context_window_defaults() {
    assert_eq!(
        default_context_window_tokens(Provider::OpenAi, Some("gpt-5.4")),
        Some(1_000_000)
    );
    assert_eq!(
        default_context_window_tokens(Provider::OpenAi, Some("gpt-5.4-mini")),
        Some(400_000)
    );
    assert_eq!(
        default_context_window_tokens(Provider::OpenAi, Some("gpt-4.1-mini")),
        Some(1_000_000)
    );
    assert_eq!(
        default_context_window_tokens(Provider::OpenAi, Some("gpt-4o")),
        Some(128_000)
    );
}

#[test]
fn known_anthropic_models_have_context_window_defaults() {
    assert_eq!(
        default_context_window_tokens(Provider::Anthropic, Some("claude-sonnet-4-6")),
        Some(1_000_000)
    );
    assert_eq!(
        default_context_window_tokens(Provider::Anthropic, Some("claude-haiku-4-5")),
        Some(200_000)
    );
}

#[test]
fn unknown_models_have_no_context_window_default() {
    assert_eq!(
        default_context_window_tokens(Provider::OpenAi, Some("custom-model")),
        None
    );
    assert_eq!(
        default_context_window_tokens(Provider::OpenAiCompatible, Some("gpt-4o")),
        None
    );
}
