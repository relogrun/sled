use crate::{ModelOptions, Provider, create_model_with_options};

fn model_error(options: ModelOptions) -> String {
    match create_model_with_options(Provider::OpenAiCompatible, options) {
        Ok(_) => panic!("expected model creation to fail"),
        Err(err) => err.to_string(),
    }
}

#[test]
fn openai_compatible_requires_model_and_base_url() {
    let missing_all = model_error(ModelOptions::default());
    assert_eq!(
        missing_all,
        "--openai-compatible-base-url or _config.openai_compatible.base_url is required"
    );

    let missing_model = model_error(ModelOptions {
        openai_compatible_base_url: Some("https://example.com/v1".into()),
        ..ModelOptions::default()
    });
    assert_eq!(
        missing_model,
        "--model or _config.openai_compatible.model is required"
    );

    let blank_model = model_error(ModelOptions {
        model: Some(" ".into()),
        openai_compatible_base_url: Some("https://example.com/v1".into()),
        ..ModelOptions::default()
    });
    assert_eq!(
        blank_model,
        "--model or _config.openai_compatible.model is required"
    );
}
