//! Reusable high-level presets built on top of pipelin3r primitives.

mod writing;

pub use writing::{
    DEFAULT_CRITIC_PROMPT, DEFAULT_REWRITER_PROMPT, WritingStepConfig, build_writing_step,
    run_writing_step,
};
