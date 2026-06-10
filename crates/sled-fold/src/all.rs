use crate::assemble;
use anyhow::Result;
use sled_core::{Context, Fold, Slot};

#[derive(Clone, Debug, Default)]
pub struct AllFold;

impl Fold for AllFold {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        assemble::assemble(slots, None)
    }
}
