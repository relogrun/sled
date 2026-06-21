use anyhow::Result;
use std::path::PathBuf;

pub(crate) fn system_prompt(
    system: Option<String>,
    system_file: Option<PathBuf>,
) -> Result<Option<String>> {
    match (system, system_file) {
        (Some(prompt), None) => Ok(Some(prompt)),
        (None, Some(path)) => Ok(Some(std::fs::read_to_string(path)?)),
        (None, None) => Ok(None),
        (Some(_), Some(_)) => unreachable!("clap prevents conflicting init prompt options"),
    }
}
