//! Per-invocation authentication for LLM agent invocations.

use std::collections::BTreeMap;

use crate::error::PipelineError;

/// Type alias for environment variable maps (`BTreeMap<String, String>`).
pub type EnvironmentMap = BTreeMap<String, String>;

/// Authentication method for LLM agent invocations.
#[derive(Debug, Clone)]
pub enum Auth {
    /// Claude Code OAuth token (`CLAUDE_CODE_OAUTH_TOKEN` env var).
    OAuthToken(String),
    /// Anthropic API key (`ANTHROPIC_API_KEY` env var).
    ApiKey(String),
    /// Read from current process environment.
    ///
    /// Checks `CLAUDE_CODE_OAUTH_TOKEN` first, then `ANTHROPIC_API_KEY`.
    /// Also forwards `CLAUDE_ACCOUNT` and `CLAUDE_CONFIG_DIR` if set.
    ///
    /// Returns an error if neither credential variable is set.
    FromEnv,
    /// Custom environment variables (passed through as-is).
    Custom(EnvironmentMap),
}

impl Auth {
    /// Convert auth configuration to environment variables for injection into a task.
    ///
    /// # Errors
    ///
    /// Returns `Err(PipelineError::Auth)` when `FromEnv` is used but neither
    /// `CLAUDE_CODE_OAUTH_TOKEN` nor `ANTHROPIC_API_KEY` is set in the process
    /// environment.
    #[allow(
        clippy::disallowed_methods,
        reason = "auth module: reading env vars for credential injection is core functionality"
    )]
    pub fn to_env(&self) -> Result<EnvironmentMap, PipelineError> {
        let mut env = BTreeMap::new();
        match self {
            Self::OAuthToken(token) => {
                let _ = env.insert(String::from("CLAUDE_CODE_OAUTH_TOKEN"), token.clone());
            }
            Self::ApiKey(key) => {
                let _ = env.insert(String::from("ANTHROPIC_API_KEY"), key.clone());
            }
            Self::FromEnv => {
                let has_token = if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
                    let _ = env.insert(String::from("CLAUDE_CODE_OAUTH_TOKEN"), token);
                    true
                } else if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                    let _ = env.insert(String::from("ANTHROPIC_API_KEY"), key);
                    true
                } else {
                    false
                };
                if !has_token {
                    return Err(PipelineError::Auth(String::from(
                        "neither CLAUDE_CODE_OAUTH_TOKEN nor ANTHROPIC_API_KEY is set in the environment",
                    )));
                }
                // Forward Claude account settings if present.
                if let Ok(account) = std::env::var("CLAUDE_ACCOUNT") {
                    let _ = env.insert(String::from("CLAUDE_ACCOUNT"), account);
                }
                if let Ok(config_dir) = std::env::var("CLAUDE_CONFIG_DIR") {
                    let _ = env.insert(String::from("CLAUDE_CONFIG_DIR"), config_dir);
                }
            }
            Self::Custom(custom) => {
                env.clone_from(custom);
            }
        }
        Ok(env)
    }
}

/// Merge two env maps, with `overlay` values taking precedence.
///
/// Returns `None` if the merged result is empty.
pub(crate) fn merge_env(
    base: EnvironmentMap,
    overlay: Option<&EnvironmentMap>,
) -> Option<EnvironmentMap> {
    let mut merged = base;
    if let Some(extra) = overlay {
        for (k, v) in extra {
            let _ = merged.insert(k.clone(), v.clone());
        }
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged)
    }
}

#[cfg(test)]
mod tests;
