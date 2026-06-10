use crate::assemble;
use anyhow::Result;
use sled_core::{Context, Fold, Slot};

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
        assemble::assemble_recent_bytes(slots, self.budget)
    }
}
