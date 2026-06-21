use crate::storage::{scan, validate_single_open};
use crate::system::{SystemPromptFragments, read_system_config, resolve_system_prompt};
use crate::{Context, Fold, ModelInput, Slot};
use anyhow::{Result, bail};
use std::path::Path;
use tracing::warn;

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 128_000;
pub const DEFAULT_CONTEXT_RATIO: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContextLimit {
    pub context_window_tokens: usize,
    pub context_ratio: f32,
}

impl Default for ContextLimit {
    fn default() -> Self {
        Self {
            context_window_tokens: DEFAULT_CONTEXT_WINDOW_TOKENS,
            context_ratio: DEFAULT_CONTEXT_RATIO,
        }
    }
}

pub fn assemble_model_input_from_slots(
    dir: &Path,
    slots: &[Slot],
    fold: &dyn Fold,
    system_fragments: &SystemPromptFragments,
) -> Result<ModelInput> {
    assemble_model_input_from_slots_with_limit(
        dir,
        slots,
        fold,
        system_fragments,
        ContextLimit::default(),
    )
}

pub fn assemble_model_input_from_slots_with_limit(
    dir: &Path,
    slots: &[Slot],
    fold: &dyn Fold,
    system_fragments: &SystemPromptFragments,
    context_limit: ContextLimit,
) -> Result<ModelInput> {
    let system_config = read_system_config(dir)?;
    fit_model_input(
        ModelInput {
            system: resolve_system_prompt(&system_config, system_fragments),
            context: fold.assemble(slots)?,
        },
        context_limit,
    )
}

pub fn preview_model_input(
    dir: &Path,
    fold: &dyn Fold,
    system_fragments: &SystemPromptFragments,
) -> Result<ModelInput> {
    preview_model_input_with_limit(dir, fold, system_fragments, ContextLimit::default())
}

pub fn preview_model_input_with_limit(
    dir: &Path,
    fold: &dyn Fold,
    system_fragments: &SystemPromptFragments,
    context_limit: ContextLimit,
) -> Result<ModelInput> {
    let slots = scan(dir)?;
    validate_single_open(&slots)?;
    assemble_model_input_from_slots_with_limit(dir, &slots, fold, system_fragments, context_limit)
}

pub(crate) fn fit_model_input(
    input: ModelInput,
    context_limit: ContextLimit,
) -> Result<ModelInput> {
    ensure_context_limit(context_limit)?;
    let budget = context_budget_tokens(context_limit);
    let full_text = model_input_text_len(&input.system, &input.context);
    if estimate_tokens(full_text) <= budget {
        return Ok(input);
    }

    let base_text = input.system.len() + input.context.index.len();
    if estimate_tokens(base_text) > budget {
        bail!(
            "model input exceeds context budget even without bodies: estimated {} tokens, budget {} tokens",
            estimate_tokens(base_text),
            budget
        );
    }

    let sections = body_sections(&input.context.bodies);
    let mut selected = vec![false; sections.len()];
    let mut bodies_len = 0usize;
    for (idx, section) in sections.iter().enumerate().rev() {
        let next_len = bodies_len + section.len();
        if estimate_tokens(base_text + next_len) > budget {
            break;
        }
        bodies_len = next_len;
        selected[idx] = true;
    }
    if !sections.is_empty() && !selected[sections.len() - 1] {
        bail!(
            "newest body section exceeds context budget: estimated {} tokens, budget {} tokens",
            estimate_tokens(base_text + sections.last().map(|section| section.len()).unwrap_or(0)),
            budget
        );
    }
    let kept_sections = selected.iter().filter(|selected| **selected).count();
    warn!(
        estimated_tokens_before = estimate_tokens(full_text),
        estimated_tokens_after = estimate_tokens(base_text + bodies_len),
        budget_tokens = budget,
        body_sections_before = sections.len(),
        body_sections_after = kept_sections,
        body_sections_dropped = sections.len().saturating_sub(kept_sections),
        "model input bodies trimmed to fit context budget"
    );

    Ok(ModelInput {
        system: input.system,
        context: Context {
            index: input.context.index,
            bodies: sections
                .iter()
                .zip(selected.iter())
                .filter(|(_, selected)| **selected)
                .map(|(section, _)| *section)
                .collect(),
        },
    })
}

fn ensure_context_limit(context_limit: ContextLimit) -> Result<()> {
    if context_limit.context_window_tokens == 0 {
        bail!("context_window_tokens must be greater than 0");
    }
    if !(context_limit.context_ratio > 0.0 && context_limit.context_ratio <= 1.0) {
        bail!("context_ratio must be greater than 0 and less than or equal to 1");
    }
    Ok(())
}

fn context_budget_tokens(context_limit: ContextLimit) -> usize {
    ((context_limit.context_window_tokens as f64) * (context_limit.context_ratio as f64)).floor()
        as usize
}

fn model_input_text_len(system: &str, context: &Context) -> usize {
    system.len() + context.index.len() + context.bodies.len()
}

pub(crate) fn estimate_tokens(chars: usize) -> usize {
    chars.div_ceil(4)
}

pub(crate) fn body_sections(bodies: &str) -> Vec<&str> {
    if bodies.is_empty() {
        return Vec::new();
    }

    let mut starts = Vec::new();
    if is_body_section_start(bodies, 0) {
        starts.push(0);
    }
    starts.extend(
        bodies
            .match_indices('\n')
            .map(|(idx, _)| idx + 1)
            .filter(|idx| is_body_section_start(bodies, *idx)),
    );

    if starts.is_empty() {
        return vec![bodies];
    }

    starts
        .iter()
        .enumerate()
        .map(|(idx, start)| {
            let end = starts.get(idx + 1).copied().unwrap_or(bodies.len());
            &bodies[*start..end]
        })
        .collect()
}

fn is_body_section_start(bodies: &str, idx: usize) -> bool {
    let bytes = bodies.as_bytes();
    if idx + 10 > bytes.len() {
        return false;
    }
    bytes[idx..].starts_with(b"--- ")
        && bytes[idx + 4..idx + 8]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && bytes[idx + 8] == b' '
        && bytes[idx + 9] == b'['
}
