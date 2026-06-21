use crate::rows::{context_from_selected, context_rows};
use anyhow::Result;
use sled_core::{Context, Fold, Slot};
use tracing::debug;

#[derive(Clone, Debug)]
pub struct RecentTokensFold {
    pub budget: usize,
}

impl RecentTokensFold {
    pub fn new(budget: usize) -> Self {
        Self { budget }
    }
}

impl Fold for RecentTokensFold {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        debug!(
            slots = slots.len(),
            budget = self.budget,
            "assembling token-budgeted model context"
        );
        let rows = context_rows(slots)?;
        let mut selected = vec![false; rows.len()];

        let mut used = 0usize;
        for (idx, row) in rows.iter().enumerate().rev() {
            if row.empty_open_slot {
                selected[idx] = true;
                continue;
            }
            let section_tokens = estimate_tokens(row.body_section.len());
            if used + section_tokens > self.budget {
                break;
            }
            used += section_tokens;
            selected[idx] = true;
        }

        Ok(context_from_selected(&rows, &selected))
    }
}

fn estimate_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}
