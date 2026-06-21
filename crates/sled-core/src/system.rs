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
pub struct SystemConfig {
    #[serde(default)]
    pub prompt: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemPromptFragments {
    pub(crate) available_tools: Option<String>,
}

impl SystemPromptFragments {
    pub fn new(available_tools: Option<String>) -> Self {
        Self { available_tools }
    }
}

pub fn read_system_config(dir: &Path) -> Result<SystemConfig> {
    let json5_path = dir.join("_system.json5");
    let json_path = dir.join("_system.json");
    let path = if json5_path.exists() {
        Some(json5_path)
    } else if json_path.exists() {
        Some(json_path)
    } else {
        None
    };

    let Some(path) = path else {
        return Ok(SystemConfig::default());
    };
    let text = fs::read_to_string(&path)?;
    json5::from_str(&text).with_context(|| format!("could not parse {}", path.display()))
}

pub fn write_default_system_config(dir: &Path) -> Result<()> {
    let json5_path = dir.join("_system.json5");
    let json_path = dir.join("_system.json");
    if json5_path.exists() || json_path.exists() {
        return Ok(());
    }
    write_system_config(dir, &SystemConfig::default())
}

pub fn write_system_config(dir: &Path, config: &SystemConfig) -> Result<()> {
    let path = dir.join("_system.json5");
    durable_write(&path, system_config_json5(config)?.as_bytes())?;
    Ok(())
}

pub fn write_system_prompt(dir: &Path, prompt: impl Into<String>) -> Result<()> {
    write_system_config(
        dir,
        &SystemConfig {
            prompt: prompt.into(),
        },
    )
}

fn system_config_json5(config: &SystemConfig) -> Result<String> {
    Ok(format!(
        "// Dialog-specific system prompt fragment.\n\
         // sled always prepends its internal protocol prompt before this fragment.\n\
         {}\n",
        serde_json::to_string_pretty(config)?
    ))
}

pub(crate) fn resolve_system_prompt(
    config: &SystemConfig,
    fragments: &SystemPromptFragments,
) -> String {
    let mut parts = vec![system_section("Sled Protocol", DEFAULT_SYSTEM_PROMPT)];
    if let Some(tools) = &fragments.available_tools
        && !tools.trim().is_empty()
    {
        parts.push(system_section("Available Tools", tools));
    }
    if !config.prompt.trim().is_empty() {
        parts.push(system_section("Dialog Instructions", &config.prompt));
    }
    parts.join("\n\n")
}

fn system_section(title: &str, body: &str) -> String {
    format!("=== {title} ===\n{}", body.trim())
}
