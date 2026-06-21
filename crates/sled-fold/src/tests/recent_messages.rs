use super::support::temp_dir;
use crate::RecentMessagesFold;
use sled_core::storage::{create_slot, scan};
use sled_core::{Fold, Message, Status};
use std::fs;

#[test]
fn recent_messages_fold_includes_only_last_k_messages() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body".into(),
            ..Message::default()
        },
    )
    .unwrap();
    create_slot(
        &dir,
        2,
        Status::Done,
        &Message {
            role: "assistant".into(),
            summary: "second".into(),
            body: "second-body".into(),
            ..Message::default()
        },
    )
    .unwrap();

    let slots = scan(&dir).unwrap();
    let context = RecentMessagesFold::new(1).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
}

#[test]
fn recent_messages_fold_keeps_empty_open_cursor_outside_limit() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body".into(),
            ..Message::default()
        },
    )
    .unwrap();
    create_slot(
        &dir,
        2,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "second".into(),
            body: "second-body".into(),
            ..Message::default()
        },
    )
    .unwrap();
    create_slot(&dir, 3, Status::Running, &Message::default()).unwrap();

    let slots = scan(&dir).unwrap();
    let context = RecentMessagesFold::new(1).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(context.index.contains("0003 [none] running"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
}
