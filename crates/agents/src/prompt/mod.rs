//! Prompt Builder Module
//!
//! Dynamic modular assembly and progressive disclosure for System Prompt
//! construction, reducing prompt token consumption by 30-50%.

pub mod builder;
pub mod cache;

pub use builder::{PromptBuilder, PromptComponents, SkillLevelDesc, ToolDefinition};
pub use builder::model_presets;
pub use cache::{PromptCache, PromptCacheConfig};
