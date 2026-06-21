use crate::model_input::{body_sections, estimate_tokens, fit_model_input};
use crate::{Context, ContextLimit, ModelInput};

#[test]
fn context_limit_keeps_newest_body_sections() {
    let first = "--- 0001 [user] ---\nfirst first first\n\n";
    let second = "--- 0002 [assistant] ---\nsecond second second\n\n";
    let third = "--- 0003 [user] ---\nthird third third\n\n";
    let input = ModelInput {
        system: String::new(),
        context: Context {
            index:
                "0001 [user] done - first\n0002 [assistant] done - second\n0003 [user] done - third\n"
                    .into(),
            bodies: format!("{first}{second}{third}"),
        },
    };
    let budget = estimate_tokens(input.context.index.len() + second.len() + third.len());

    let limited = fit_model_input(
        input,
        ContextLimit {
            context_window_tokens: budget,
            context_ratio: 1.0,
        },
    )
    .unwrap();

    assert_eq!(
        limited.context.index,
        "0001 [user] done - first\n0002 [assistant] done - second\n0003 [user] done - third\n"
    );
    assert!(!limited.context.bodies.contains("first first"));
    assert!(limited.context.bodies.contains("second second"));
    assert!(limited.context.bodies.contains("third third"));
}

#[test]
fn context_limit_rejects_oversized_system_and_index() {
    let err = fit_model_input(
        ModelInput {
            system: "system text that is too large".into(),
            context: Context {
                index: "index text that is too large".into(),
                bodies: "--- 0001 [user] ---\nbody\n\n".into(),
            },
        },
        ContextLimit {
            context_window_tokens: 1,
            context_ratio: 1.0,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("model input exceeds context budget even without bodies"));
}

#[test]
fn context_limit_rejects_oversized_newest_body_section() {
    let err = fit_model_input(
        ModelInput {
            system: String::new(),
            context: Context {
                index: "0001 [user] done - first\n".into(),
                bodies: "--- 0001 [user] ---\nthis latest body section is too large\n\n".into(),
            },
        },
        ContextLimit {
            context_window_tokens: 8,
            context_ratio: 1.0,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(err.contains("newest body section exceeds context budget"));
}

#[test]
fn body_sections_ignore_markdown_rules_inside_body_text() {
    let bodies = "--- 0001 [user] ---\nfirst\n--- not a sled section\nstill first\n\n--- 0002 [assistant] ---\nsecond\n\n";
    let sections = body_sections(bodies);

    assert_eq!(sections.len(), 2);
    assert!(sections[0].contains("--- not a sled section"));
    assert!(sections[1].contains("second"));
}
