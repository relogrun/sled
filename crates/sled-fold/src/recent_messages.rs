use crate::rows::{context_rows, message_row_count};
use anyhow::Result;
use sled_core::{Context, Fold, Slot};
use tracing::debug;

#[derive(Clone, Debug)]
pub struct RecentMessagesFold {
    pub k: usize,
}

impl RecentMessagesFold {
    pub fn new(k: usize) -> Self {
        Self { k }
    }
}

impl Fold for RecentMessagesFold {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        debug!(
            slots = slots.len(),
            recent_messages = self.k,
            "assembling message-windowed model context"
        );
        let rows = context_rows(slots)?;
        let recent_from = message_row_count(&rows).saturating_sub(self.k);
        let mut index = String::new();
        let mut bodies = String::new();
        let mut message_idx = 0usize;

        for row in rows {
            let is_message = !row.empty_open_slot;
            let include = row.empty_open_slot || message_idx >= recent_from;
            if is_message {
                message_idx += 1;
            }
            if !include {
                continue;
            }
            index.push_str(&row.index_line);
            bodies.push_str(&row.body_section);
        }

        Ok(Context { index, bodies })
    }
}
