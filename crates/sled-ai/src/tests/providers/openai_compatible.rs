use crate::providers::openai_compatible::chat_completions_endpoint;

#[test]
fn builds_chat_completions_endpoint() {
    assert_eq!(
        chat_completions_endpoint("https://example.com/v1"),
        "https://example.com/v1/chat/completions"
    );
    assert_eq!(
        chat_completions_endpoint("https://example.com/v1/"),
        "https://example.com/v1/chat/completions"
    );
    assert_eq!(
        chat_completions_endpoint("https://example.com/v1/chat/completions"),
        "https://example.com/v1/chat/completions"
    );
}
