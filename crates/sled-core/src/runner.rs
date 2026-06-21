use crate::model_input::{ContextLimit, ModelInputOptions, assemble_model_input_from_slots};
use crate::storage::{
    create_slot_with_options, read_message, scan, set_status, validate_single_open,
    write_message_with_options,
};
use crate::system::ensure_dialog_system_prompt;
use crate::{
    Fold, Message, Model, Reply, Slot, Status, ToolExecutor, ToolResult, ToolSuspension,
    WriteOptions,
};
use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Clone, Debug)]
pub enum StepOutcome {
    Continue,
    Awaiting(PathBuf),
    Finished(Option<u32>),
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeOptions {
    pub write_options: WriteOptions,
    pub available_tools: Option<String>,
    pub context_limit: ContextLimit,
}

pub async fn step(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    fold: &dyn Fold,
    options: RuntimeOptions,
) -> Result<StepOutcome> {
    step_with_runtime_options(dir, model, tools, fold, &options).await
}

async fn step_with_runtime_options(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    fold: &dyn Fold,
    options: &RuntimeOptions,
) -> Result<StepOutcome> {
    debug!(dir = %dir.display(), "runner step start");
    fs::create_dir_all(dir)?;
    let slots = scan(dir)?;
    let open = validate_single_open(&slots)?;

    let Some(slot) = open else {
        match slots.last() {
            None => {
                info!(dir = %dir.display(), "empty dialog, creating initial awaiting slot");
                ensure_dialog_system_prompt(dir)?;
                let path = create_slot_with_options(
                    dir,
                    1,
                    Status::Awaiting,
                    &Message::default(),
                    options.write_options,
                )?;
                return Ok(StepOutcome::Awaiting(path));
            }
            Some(last) => {
                let msg = read_message(&last.path)?;
                if is_assistant_role(&msg.role) {
                    info!(slot = last.num, "dialog finished at assistant message");
                    return Ok(StepOutcome::Finished(Some(last.num)));
                }
                info!(
                    last_slot = last.num,
                    last_role = %msg.role,
                    "creating next assistant running slot"
                );
                create_slot_with_options(
                    dir,
                    last.num + 1,
                    Status::Running,
                    &Message::default(),
                    options.write_options,
                )?;
                return Ok(StepOutcome::Continue);
            }
        }
    };

    match slot.status {
        Status::Awaiting => {
            let msg = read_message(&slot.path).unwrap_or_default();
            if is_filled_user_awaiting(slot, &msg) || is_completed_tool_awaiting(slot, &msg) {
                info!(slot = slot.num, "recovering filled awaiting slot");
                set_status(dir, slot, Status::Done)?;
                return Ok(StepOutcome::Continue);
            }
            info!(slot = slot.num, path = %slot.path.display(), "user input requested");
            Ok(StepOutcome::Awaiting(slot.path.clone()))
        }
        Status::Pending => {
            info!(slot = slot.num, "processing pending tool slot");
            let mut msg = read_message(&slot.path)?;
            if msg.result.is_some() {
                info!(slot = slot.num, "recovering completed pending tool slot");
                set_status(dir, slot, Status::Done)?;
                return Ok(StepOutcome::Continue);
            }
            if msg.suspension.is_some() {
                info!(slot = slot.num, "recovering suspended pending tool slot");
                let path = set_status(dir, slot, Status::Awaiting)?;
                return Ok(StepOutcome::Awaiting(path));
            }
            let call = msg
                .call
                .clone()
                .ok_or_else(|| anyhow!("pending slot without call: {}", slot.path.display()))?;
            match tools.execute(dir, &slots, &call).await? {
                ToolResult::Completed(result) => {
                    msg.result = Some(result);
                    write_message_with_options(&slot.path, &msg, options.write_options)?;
                    set_status(dir, slot, Status::Done)?;
                    Ok(StepOutcome::Continue)
                }
                ToolResult::Suspended(request) => {
                    msg.suspension = Some(ToolSuspension { request });
                    write_message_with_options(&slot.path, &msg, options.write_options)?;
                    let path = set_status(dir, slot, Status::Awaiting)?;
                    Ok(StepOutcome::Awaiting(path))
                }
            }
        }
        Status::Running => {
            info!(slot = slot.num, "processing running assistant slot");
            let msg = read_message(&slot.path).unwrap_or_default();
            if msg.filled() {
                info!(slot = slot.num, "recovering filled running slot");
                let next = if msg.call.is_some() && msg.result.is_none() {
                    Status::Pending
                } else {
                    Status::Done
                };
                set_status(dir, slot, next)?;
                return Ok(StepOutcome::Continue);
            }

            let input = assemble_model_input_from_slots(
                dir,
                &slots,
                fold,
                ModelInputOptions {
                    available_tools: options.available_tools.clone(),
                    context_limit: options.context_limit,
                },
            )?;
            match model.complete(&input.system, &input.context).await? {
                Reply::Final {
                    text,
                    summary,
                    wait_user,
                } => {
                    info!(
                        slot = slot.num,
                        wait_user, "assistant returned final response"
                    );
                    write_message_with_options(
                        &slot.path,
                        &Message {
                            role: "assistant".into(),
                            summary,
                            body: text,
                            ..Message::default()
                        },
                        options.write_options,
                    )?;
                    set_status(dir, slot, Status::Done)?;
                    if wait_user {
                        create_slot_with_options(
                            dir,
                            slot.num + 1,
                            Status::Awaiting,
                            &Message::default(),
                            options.write_options,
                        )?;
                    }
                    Ok(StepOutcome::Continue)
                }
                Reply::Tool { call, summary } => {
                    info!(slot = slot.num, tool = %call.tool, "assistant requested tool");
                    write_message_with_options(
                        &slot.path,
                        &Message {
                            role: "tool".into(),
                            summary,
                            call: Some(call),
                            ..Message::default()
                        },
                        options.write_options,
                    )?;
                    set_status(dir, slot, Status::Pending)?;
                    Ok(StepOutcome::Continue)
                }
            }
        }
        Status::Done => unreachable!(),
    }
}

pub async fn run_until_stop(
    dir: &Path,
    model: &dyn Model,
    tools: &dyn ToolExecutor,
    fold: &dyn Fold,
    options: RuntimeOptions,
) -> Result<StepOutcome> {
    info!(dir = %dir.display(), "runner started");
    loop {
        match step_with_runtime_options(dir, model, tools, fold, &options).await? {
            StepOutcome::Continue => continue,
            other => return Ok(other),
        }
    }
}

fn is_assistant_role(role: &str) -> bool {
    role == "assistant"
}

fn is_completed_tool_awaiting(slot: &Slot, msg: &Message) -> bool {
    slot.status == Status::Awaiting
        && (msg.role == "tool" || slot.role.as_deref() == Some("tool"))
        && msg.result.is_some()
}

fn is_filled_user_awaiting(slot: &Slot, msg: &Message) -> bool {
    slot.status == Status::Awaiting
        && (msg.role == "user" || slot.role.as_deref() == Some("user"))
        && !msg.body.is_empty()
}
