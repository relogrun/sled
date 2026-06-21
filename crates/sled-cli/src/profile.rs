use sled_core::Fold;
use sled_fold::AllFold;
use sled_tools::ToolRegistry;

pub struct Profile {
    pub fold: Box<dyn Fold>,
    pub tools: ToolRegistry,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            fold: Box::new(AllFold),
            tools: ToolRegistry::with_defaults(),
        }
    }
}
