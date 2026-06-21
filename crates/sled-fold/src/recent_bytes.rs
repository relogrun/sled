use crate::rows::{context_from_selected, context_rows};
use anyhow::Result;
use sled_core::{Context, Fold, Slot};
use tracing::debug;

#[derive(Clone, Debug)]
pub struct RecentBytesFold {
    pub budget: usize,
}

impl RecentBytesFold {
    pub fn new(budget: usize) -> Self {
        Self { budget }
    }
}

impl Fold for RecentBytesFold {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        debug!(
            slots = slots.len(),
            budget = self.budget,
            "assembling byte-budgeted model context"
        );
        let rows = context_rows(slots)?;
        let mut selected = vec![false; rows.len()];

        let mut used = 0usize;
        for (idx, row) in rows.iter().enumerate().rev() {
            if row.empty_open_slot {
                selected[idx] = true;
                continue;
            }
            let section_len = row.body_section.len();
            if used + section_len > self.budget {
                break;
            }
            used += section_len;
            selected[idx] = true;
        }

        Ok(context_from_selected(&rows, &selected))
    }
}
