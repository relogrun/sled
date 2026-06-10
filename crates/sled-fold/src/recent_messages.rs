use crate::assemble;
use anyhow::Result;
use sled_core::{Context, Fold, Slot};

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
        assemble::assemble(slots, Some(self.k))
    }
}
