//! Per-invocation authentication for LLM agent invocations.

use std::collections::BTreeMap;

use crate::error::PipelineError;

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
    Custom(BTreeMap<String, String>),
}

impl Auth {
    /// Convert auth configuration to environment variables for injection into a task.
    ///
    /// # Errors
    ///
    /// Returns `Err(PipelineError::Auth)` when `FromEnv` is used but neither
    /// `CLAUDE_CODE_OAUTH_TOKEN` nor `ANTHROPIC_API_KEY` is set in the process
    /// environment.
    pub fn to_env(&self) -> Result<BTreeMap<String, String>, PipelineError> {
        let mut env = BTreeMap::new();
        match self {
            Self::OAuthToken(token) => {
                let _ = env.insert(
                    String::from("CLAUDE_CODE_OAUTH_TOKEN"),
                    token.clone(),
                );
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
    base: BTreeMap<String, String>,
    overlay: Option<&BTreeMap<String, String>>,
) -> Option<BTreeMap<String, String>> {
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
mod tests {
    use super::*;

    #[test]
    fn oauth_token_produces_single_env_var() {
        let auth = Auth::OAuthToken(String::from("tok_123"));
        let env = auth.to_env().unwrap_or_default();
        assert_eq!(env.len(), 1, "should produce exactly one env var");
        assert_eq!(
            env.get("CLAUDE_CODE_OAUTH_TOKEN").map(String::as_str),
            Some("tok_123"),
            "should set CLAUDE_CODE_OAUTH_TOKEN"
        );
    }

    #[test]
    fn api_key_produces_single_env_var() {
        let auth = Auth::ApiKey(String::from("sk-ant-123"));
        let env = auth.to_env().unwrap_or_default();
        assert_eq!(env.len(), 1, "should produce exactly one env var");
        assert_eq!(
            env.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("sk-ant-123"),
            "should set ANTHROPIC_API_KEY"
        );
    }

    #[test]
    fn custom_passes_through() {
        let mut custom = BTreeMap::new();
        let _ = custom.insert(String::from("MY_KEY"), String::from("my_val"));
        let _ = custom.insert(String::from("OTHER"), String::from("stuff"));
        let auth = Auth::Custom(custom);
        let env = auth.to_env().unwrap_or_default();
        assert_eq!(env.len(), 2, "should pass through all custom vars");
        assert_eq!(
            env.get("MY_KEY").map(String::as_str),
            Some("my_val"),
            "custom key preserved"
        );
    }

    #[test]
    fn from_env_returns_result() {
        // In test environment, credentials may or may not be set.
        // We verify the method returns a Result (not panic) in either case.
        let auth = Auth::FromEnv;
        let result = auth.to_env();
        // If no credentials are set, we get Err; if set, we get Ok with entries.
        match result {
            Ok(env) => {
                assert!(!env.is_empty(), "should have at least one credential var");
            }
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("CLAUDE_CODE_OAUTH_TOKEN"),
                    "error should mention missing vars: {msg}"
                );
            }
        }
    }

    #[test]
    fn merge_env_empty_produces_none() {
        let base = BTreeMap::new();
        let result = merge_env(base, None);
        assert!(result.is_none(), "empty maps should produce None");
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion on known-Some value
    fn merge_env_overlay_takes_precedence() {
        let mut base = BTreeMap::new();
        let _ = base.insert(String::from("A"), String::from("1"));
        let _ = base.insert(String::from("B"), String::from("base"));

        let mut overlay = BTreeMap::new();
        let _ = overlay.insert(String::from("B"), String::from("overlay"));
        let _ = overlay.insert(String::from("C"), String::from("3"));

        let result = merge_env(base, Some(&overlay)).unwrap();
        assert_eq!(
            result.get("A").map(String::as_str),
            Some("1"),
            "base keys preserved"
        );
        assert_eq!(
            result.get("B").map(String::as_str),
            Some("overlay"),
            "overlay takes precedence"
        );
        assert_eq!(
            result.get("C").map(String::as_str),
            Some("3"),
            "overlay keys added"
        );
    }
}
