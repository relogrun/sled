use super::support::temp_dir;
use crate::storage::{
    MessageWriteFormat, create_slot, durable_write, mirror_file_name, read_message, scan, tmp_path,
    validate_single_open, write_message_with_format,
};
use crate::{Message, Status};
use std::fs;
use std::path::Path;

#[test]
fn open_slot_filenames_match_known_roles() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();

    let running = create_slot(&dir, 1, Status::Running, &Message::default()).unwrap();
    let awaiting = create_slot(&dir, 2, Status::Awaiting, &Message::default()).unwrap();

    assert_eq!(running.file_name().unwrap(), "0001.running.json5");
    assert_eq!(awaiting.file_name().unwrap(), "0002.user.awaiting.json5");
}

#[test]
fn rejects_two_open_slots() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(&dir, 1, Status::Awaiting, &Message::default()).unwrap();
    create_slot(&dir, 2, Status::Pending, &Message::default()).unwrap();
    let slots = scan(&dir).unwrap();
    assert!(validate_single_open(&slots).is_err());
}

#[test]
fn markdown_mirror_writer_keeps_json_body_as_source_of_truth() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("0001.assistant.done.json5");
    let body = "first line\n\n- bullet\n";

    write_message_with_format(
        &path,
        &Message {
            role: "assistant".into(),
            summary: "multiline".into(),
            body: body.into(),
            ..Message::default()
        },
        MessageWriteFormat::JsonWithMarkdownMirror,
    )
    .unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"body\""));
    assert!(!raw.contains("\"body_file\""));

    let mirror_path = dir.join("0001.assistant.done.md");
    assert_eq!(fs::read_to_string(mirror_path).unwrap(), body);

    let msg = read_message(&path).unwrap();
    assert_eq!(msg.body, body);
}

#[test]
fn durable_write_replaces_file_and_removes_temp() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("data.txt");

    durable_write(&path, b"first").unwrap();
    durable_write(&path, b"second").unwrap();

    assert_eq!(fs::read_to_string(&path).unwrap(), "second");
    assert!(!tmp_path(&path).exists());
}

#[test]
fn mirror_file_name_uses_done_status_and_md_suffix() {
    let path = Path::new("0002.running.json5");
    let msg = Message {
        role: "assistant".into(),
        ..Message::default()
    };
    assert_eq!(
        mirror_file_name(path, &msg).unwrap(),
        "0002.assistant.done.md"
    );
}
