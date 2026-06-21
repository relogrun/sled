use crate::assemble;
use anyhow::Result;
use sled_core::{Context, Fold, Slot};

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
        assemble::assemble_recent_tokens(slots, self.budget)
    }
}
