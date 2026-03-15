//! Pure function transforms (stub).
//!
//! Will be fleshed out when pipeline steps need deterministic data transformations
//! (e.g. JSON reshaping, filtering, aggregation) without LLM involvement.

/// Builder for a pure-function transform step (stub).
pub struct TransformBuilder {
    _name: String,
}

impl TransformBuilder {
    /// Create a new transform builder.
    pub fn new(name: &str) -> Self {
        Self {
            _name: String::from(name),
        }
    }
}
