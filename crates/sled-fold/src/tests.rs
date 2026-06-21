use super::*;
use sled_core::storage::{create_slot, scan};
use sled_core::{Fold, Message, Status};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let seq = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("sled-fold-test-{id}-{seq}"))
}

#[test]
fn all_fold_includes_all_bodies() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
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
            body: "second-body".into(),
            ..Message::default()
        },
    )
    .unwrap();

    let slots = scan(&dir).unwrap();
    let context = AllFold.assemble(&slots).unwrap();
    assert!(context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
}

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

#[test]
fn recent_bytes_fold_keeps_latest_sections_that_fit_budget() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body-that-is-too-long-to-fit-after-newest-section".into(),
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
    let context = RecentBytesFold::new(64).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
    assert!(context.bodies.len() <= 64);
}

#[test]
fn recent_bytes_fold_keeps_empty_open_cursor_outside_budget() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body-that-is-too-long-to-fit-after-newest-section".into(),
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
    create_slot(&dir, 3, Status::Running, &Message::default()).unwrap();

    let slots = scan(&dir).unwrap();
    let context = RecentBytesFold::new(64).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(context.index.contains("0003 [none] running"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
}

#[test]
fn recent_tokens_fold_keeps_latest_sections_that_fit_budget() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body-that-is-too-long-to-fit-after-newest-section".into(),
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
    let context = RecentTokensFold::new(16).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
    assert!(context.bodies.len().div_ceil(4) <= 16);
}

#[test]
fn recent_tokens_fold_keeps_empty_open_cursor_outside_budget() {
    let dir = temp_dir();
    fs::create_dir_all(&dir).unwrap();
    create_slot(
        &dir,
        1,
        Status::Done,
        &Message {
            role: "user".into(),
            summary: "first".into(),
            body: "first-body-that-is-too-long-to-fit-after-newest-section".into(),
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
    create_slot(&dir, 3, Status::Running, &Message::default()).unwrap();

    let slots = scan(&dir).unwrap();
    let context = RecentTokensFold::new(16).assemble(&slots).unwrap();
    assert!(!context.index.contains("first"));
    assert!(context.index.contains("second"));
    assert!(context.index.contains("0003 [none] running"));
    assert!(!context.bodies.contains("first-body"));
    assert!(context.bodies.contains("second-body"));
}
