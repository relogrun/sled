use crate::rows::{context_from_selected, context_rows};
use anyhow::Result;
use sled_core::{Context, Fold, Slot, select_newest_sections_to_fit};
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
        let budgeted_sections = rows
            .iter()
            .filter(|row| !row.empty_open_slot)
            .map(|row| row.body_section.len());
        let budgeted_selection = select_newest_sections_to_fit(0, budgeted_sections, self.budget);
        let mut budgeted_selection = budgeted_selection.into_iter();
        let selected = rows
            .iter()
            .map(|row| {
                if row.empty_open_slot {
                    true
                } else {
                    budgeted_selection.next().unwrap_or(false)
                }
            })
            .collect::<Vec<_>>();
        debug_assert!(budgeted_selection.next().is_none());
        Ok(context_from_selected(&rows, &selected))
    }
}
