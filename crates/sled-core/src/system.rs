use crate::storage::durable_write;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub(crate) const DEFAULT_SYSTEM_PROMPT: &str = concat!(
    include_str!("../../../prompts/system.md"),
    "\n\n",
    include_str!("../../../prompts/context_format.md"),
    "\n\n",
    include_str!("../../../prompts/reply_protocol.md")
);

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DialogSystemPromptFile {
    #[serde(default)]
    prompt: String,
}

pub fn read_dialog_system_prompt(dir: &Path) -> Result<String> {
    let path = dir.join("_system.json5");
    if !path.exists() {
        return Ok(String::new());
    }
    let text = fs::read_to_string(&path)?;
    let file: DialogSystemPromptFile =
        json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))?;
    Ok(file.prompt)
}

pub fn ensure_dialog_system_prompt(dir: &Path) -> Result<()> {
    let path = dir.join("_system.json5");
    if path.exists() {
        return Ok(());
    }
    write_dialog_system_prompt(dir, "")
}

fn write_dialog_system_prompt(dir: &Path, prompt: &str) -> Result<()> {
    let path = dir.join("_system.json5");
    durable_write(&path, dialog_system_prompt_json5(prompt)?.as_bytes())?;
    Ok(())
}

pub fn set_dialog_system_prompt(dir: &Path, prompt: impl Into<String>) -> Result<()> {
    let prompt = prompt.into();
    write_dialog_system_prompt(dir, &prompt)
}

fn dialog_system_prompt_json5(prompt: &str) -> Result<String> {
    Ok(format!(
        "// Dialog-specific system prompt fragment.\n\
         // sled always prepends its internal protocol prompt before this fragment.\n\
         {}\n",
        serde_json::to_string_pretty(&DialogSystemPromptFile {
            prompt: prompt.into()
        })?
    ))
}

pub(crate) fn build_system_prompt(dialog_prompt: &str, available_tools: Option<&str>) -> String {
    let mut parts = vec![system_section("Sled Protocol", DEFAULT_SYSTEM_PROMPT)];
    if let Some(tools) = available_tools
        && !tools.trim().is_empty()
    {
        parts.push(system_section("Available Tools", tools));
    }
    if !dialog_prompt.trim().is_empty() {
        parts.push(system_section("Dialog Instructions", dialog_prompt));
    }
    parts.join("\n\n")
}

fn system_section(title: &str, body: &str) -> String {
    format!("=== {title} ===\n{}", body.trim())
}
