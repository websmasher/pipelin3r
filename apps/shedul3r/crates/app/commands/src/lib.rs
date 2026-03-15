//! Application use cases and business logic commands.
//!
//! This crate contains the command/use-case layer of the hexagonal architecture.
//! It orchestrates domain types via port traits — never importing concrete
//! adapters directly.

mod execute;
mod parser;

pub use execute::TaskEngine;
pub use parser::parse_task_definition;
