use anyhow::Result;
use sled_core::{Context, Fold, Slot};

pub trait FoldTransform: Send + Sync {
    fn apply(&self, context: Context) -> Result<Context>;
}

pub struct FoldPipeline {
    source: Box<dyn Fold>,
    transforms: Vec<Box<dyn FoldTransform>>,
}

impl FoldPipeline {
    pub fn new(source: Box<dyn Fold>) -> Self {
        Self {
            source,
            transforms: Vec::new(),
        }
    }

    pub fn then(mut self, transform: Box<dyn FoldTransform>) -> Self {
        self.transforms.push(transform);
        self
    }
}

impl Fold for FoldPipeline {
    fn assemble(&self, slots: &[Slot]) -> Result<Context> {
        let mut context = self.source.assemble(slots)?;
        for transform in &self.transforms {
            context = transform.apply(context)?;
        }
        Ok(context)
    }
}
