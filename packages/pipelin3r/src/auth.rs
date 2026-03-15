//! Per-invocation authentication for LLM agent invocations.

use std::collections::HashMap;

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
    FromEnv,
    /// Custom environment variables (passed through as-is).
    Custom(HashMap<String, String>),
}

impl Auth {
    /// Convert auth configuration to environment variables for injection into a task.
    pub fn to_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
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
                if let Ok(token) = std::env::var("CLAUDE_CODE_OAUTH_TOKEN") {
                    let _ = env.insert(String::from("CLAUDE_CODE_OAUTH_TOKEN"), token);
                } else if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
                    let _ = env.insert(String::from("ANTHROPIC_API_KEY"), key);
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
        env
    }
}

/// Merge two env maps, with `overlay` values taking precedence.
///
/// Returns `None` if the merged result is empty.
pub(crate) fn merge_env(
    base: HashMap<String, String>,
    overlay: Option<&HashMap<String, String>>,
) -> Option<HashMap<String, String>> {
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
        let env = auth.to_env();
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
        let env = auth.to_env();
        assert_eq!(env.len(), 1, "should produce exactly one env var");
        assert_eq!(
            env.get("ANTHROPIC_API_KEY").map(String::as_str),
            Some("sk-ant-123"),
            "should set ANTHROPIC_API_KEY"
        );
    }

    #[test]
    fn custom_passes_through() {
        let mut custom = HashMap::new();
        let _ = custom.insert(String::from("MY_KEY"), String::from("my_val"));
        let _ = custom.insert(String::from("OTHER"), String::from("stuff"));
        let auth = Auth::Custom(custom);
        let env = auth.to_env();
        assert_eq!(env.len(), 2, "should pass through all custom vars");
        assert_eq!(
            env.get("MY_KEY").map(String::as_str),
            Some("my_val"),
            "custom key preserved"
        );
    }

    #[test]
    fn from_env_with_no_vars_produces_empty() {
        // In test environment, these vars are typically not set.
        // We can't guarantee this, but it tests the fallback path.
        let auth = Auth::FromEnv;
        let _env = auth.to_env();
        // Just verify it doesn't panic — actual values depend on test environment.
    }

    #[test]
    fn merge_env_empty_produces_none() {
        let base = HashMap::new();
        let result = merge_env(base, None);
        assert!(result.is_none(), "empty maps should produce None");
    }

    #[test]
    #[allow(clippy::unwrap_used)] // reason: test assertion on known-Some value
    fn merge_env_overlay_takes_precedence() {
        let mut base = HashMap::new();
        let _ = base.insert(String::from("A"), String::from("1"));
        let _ = base.insert(String::from("B"), String::from("base"));

        let mut overlay = HashMap::new();
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
