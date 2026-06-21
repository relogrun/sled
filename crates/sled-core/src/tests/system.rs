use super::support::temp_dir;
use crate::system::{DEFAULT_SYSTEM_PROMPT, build_system_prompt};
use crate::{ensure_dialog_system_prompt, read_dialog_system_prompt, set_dialog_system_prompt};
use std::fs;

#[test]
fn system_prompt_is_internal_prompt_plus_dialog_prompt() {
    let system = build_system_prompt("Dialog prompt.", None);

    assert!(system.starts_with("=== Sled Protocol ===\n"));
    assert!(system.contains(DEFAULT_SYSTEM_PROMPT));
    assert!(system.ends_with("=== Dialog Instructions ===\nDialog prompt."));
}

#[test]
fn system_prompt_sections_are_ordered() {
    let system = build_system_prompt("Dialog prompt.", Some("Tool prompt."));

    let sled = system.find("=== Sled Protocol ===").unwrap();
    let tools = system.find("=== Available Tools ===").unwrap();
    let dialog = system.find("=== Dialog Instructions ===").unwrap();

    assert!(sled < tools);
    assert!(tools < dialog);
    assert!(system.contains("=== Available Tools ===\nTool prompt."));
}

#[test]
fn dialog_system_prompt_writer_includes_fragment_comment() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    set_dialog_system_prompt(&dir, "Dialog prompt.").unwrap();

    let text = fs::read_to_string(dir.join("_system.json5")).unwrap();
    assert!(text.starts_with("// Dialog-specific system prompt fragment."));
    assert!(text.contains("sled always prepends its internal protocol prompt"));
    assert_eq!(read_dialog_system_prompt(&dir).unwrap(), "Dialog prompt.");
}

#[test]
fn ensure_dialog_system_prompt_creates_file_without_overwriting_existing_prompt() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();

    ensure_dialog_system_prompt(&dir).unwrap();
    assert!(dir.join("_system.json5").exists());
    assert_eq!(read_dialog_system_prompt(&dir).unwrap(), "");

    set_dialog_system_prompt(&dir, "Dialog prompt.").unwrap();
    ensure_dialog_system_prompt(&dir).unwrap();
    assert_eq!(read_dialog_system_prompt(&dir).unwrap(), "Dialog prompt.");
}
