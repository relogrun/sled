mod model_input;
mod runner;
mod say;
mod status;
mod system;
mod types;

pub mod storage;

pub use model_input::{
    ContextLimit, DEFAULT_CONTEXT_RATIO, DEFAULT_CONTEXT_WINDOW_TOKENS, ModelInputOptions,
    assemble_model_input_from_slots, context_budget_tokens, estimate_tokens, preview_model_input,
    select_newest_sections_to_fit,
};
pub use runner::{RuntimeOptions, StepOutcome, run_until_stop, step};
pub use say::{say, say_with_options};
pub use status::status_report;
pub use system::{
    ensure_dialog_system_prompt, read_dialog_system_prompt, set_dialog_system_prompt,
};
pub use types::{
    Call, Context, Fold, Message, Model, ModelInput, Reply, Slot, Status, ToolExecutor, ToolResult,
    ToolSuspension, WriteOptions,
};

#[cfg(test)]
mod tests;
