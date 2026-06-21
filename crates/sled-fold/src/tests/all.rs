use super::support::temp_dir;
use crate::AllFold;
use sled_core::storage::{create_slot, scan};
use sled_core::{Fold, Message, Status};
use std::fs;

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
