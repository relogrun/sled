use crate::rows::context_rows;
use anyhow::Result;
use sled_core::{Context, Fold, Slot};
use tracing::debug;

#[derive(Clone, Debug, Default)]
pub struct AllFold;

impl Fold for AllFold {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        debug!(slots = slots.len(), "assembling full model context");
        let rows = context_rows(slots)?;
        Ok(Context {
            index: rows.iter().map(|row| row.index_line.as_str()).collect(),
            bodies: rows.iter().map(|row| row.body_section.as_str()).collect(),
        })
    }
}
