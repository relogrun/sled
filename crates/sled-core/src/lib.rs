mod model_input;
mod runner;
mod say;
mod status;
mod storage;
mod system;
mod types;

pub use model_input::{
    ContextLimit, DEFAULT_CONTEXT_RATIO, DEFAULT_CONTEXT_WINDOW_TOKENS,
    assemble_model_input_from_slots, assemble_model_input_from_slots_with_limit,
    preview_model_input, preview_model_input_with_limit,
};
pub use runner::{
    StepOutcome, run_until_stop, run_until_stop_with_options,
    run_until_stop_with_options_and_fragments, run_until_stop_with_options_fragments_and_limit,
    step, step_with_options, step_with_options_and_fragments,
    step_with_options_fragments_and_limit,
};
pub use say::{say, say_with_options};
pub use status::status_report;
pub use storage::{
    create_slot, create_slot_with_options, durable_write, read_message, scan, set_status,
    slot_path, validate_single_open, write_message, write_message_with_options,
};
pub use system::{
    SystemConfig, SystemPromptFragments, read_system_config, write_default_system_config,
    write_system_config, write_system_prompt,
};
pub use types::{
    Call, Context, Fold, Message, Model, ModelInput, Reply, Slot, Status, ToolExecutor, ToolResult,
    ToolSuspension, WriteOptions,
};

#[cfg(test)]
mod tests;
