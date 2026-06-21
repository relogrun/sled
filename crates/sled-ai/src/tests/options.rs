use crate::{AnthropicEffort, AnthropicThinking, OpenAiReasoningEffort, Provider};

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
