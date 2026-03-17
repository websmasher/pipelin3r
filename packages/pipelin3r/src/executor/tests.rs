#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[test]
fn extract_step_name_basic() {
    let yaml = "name: 3_1_implement_tests\ncommand: echo\n";
    assert_eq!(
        extract_step_name(yaml),
        "3-1-implement-tests",
        "should slugify name field"
    );
}

#[test]
fn extract_step_name_missing() {
    let yaml = "command: echo\ntimeout: 5m\n";
    assert_eq!(
        extract_step_name(yaml),
        "unknown",
        "should fallback to unknown when name is missing"
    );
}

#[test]
fn extract_step_name_special_chars() {
    let yaml = "name: Hello World! (v2)\n";
    assert_eq!(
        extract_step_name(yaml),
        "hello-world-v2",
        "should replace non-alphanumeric with dashes, collapsing consecutive dashes"
    );
}

#[test]
fn executor_with_defaults_succeeds() {
    let result = Executor::with_defaults();
    assert!(result.is_ok(), "should create executor with defaults");
}

#[test]
fn mutant_kill_default_provider_returns_set_value() {
    // Mutant kill: executor.rs:126 — default_provider() replaced with None
    let executor = Executor::with_defaults()
        .unwrap_or_else(|_| std::process::abort())
        .with_default_provider(Provider::OpenRouter);
    let provider = executor.default_provider();
    assert!(
        provider.is_some(),
        "default_provider() must return Some after with_default_provider()"
    );
    assert!(
        matches!(provider, Some(Provider::OpenRouter)),
        "default_provider() must return the provider that was set"
    );
}

#[test]
fn is_local_true_for_localhost() {
    let executor = Executor::with_defaults().unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "default URL (localhost:7943) must be detected as local"
    );
}

#[test]
fn is_local_true_for_127() {
    let config = ClientConfig {
        base_url: String::from("http://127.0.0.1:7943"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(executor.is_local(), "127.0.0.1 must be detected as local");
}

#[test]
fn is_local_true_for_ipv6_loopback() {
    let config = ClientConfig {
        base_url: String::from("http://[::1]:7943"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(executor.is_local(), "[::1] must be detected as local");
}

#[test]
fn is_local_false_for_remote_url() {
    let config = ClientConfig {
        base_url: String::from("https://shedul3r.example.com"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        !executor.is_local(),
        "remote URL must not be detected as local"
    );
}

#[test]
fn is_local_false_for_subdomain_bypass() {
    let config = ClientConfig {
        base_url: String::from("http://localhost.evil.com"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        !executor.is_local(),
        "localhost.evil.com must NOT be detected as local (subdomain bypass)"
    );
}

#[test]
fn is_local_true_for_localhost_with_port() {
    let config = ClientConfig {
        base_url: String::from("http://localhost:8080"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "localhost:8080 must be detected as local"
    );
}

#[test]
fn is_local_true_for_localhost_with_path() {
    let config = ClientConfig {
        base_url: String::from("http://localhost/path"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "localhost/path must be detected as local"
    );
}

#[test]
fn is_local_true_for_uppercase_localhost() {
    let config = ClientConfig {
        base_url: String::from("http://LOCALHOST:7943"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "LOCALHOST (uppercase) must be detected as local"
    );
}

#[test]
fn is_local_true_for_mixed_case_localhost() {
    let config = ClientConfig {
        base_url: String::from("http://Localhost:7943"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "Localhost (mixed case) must be detected as local"
    );
}

#[test]
fn is_local_true_for_credentials_localhost() {
    let config = ClientConfig {
        base_url: String::from("http://user:pass@localhost:7943"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        executor.is_local(),
        "user:pass@localhost must be detected as local"
    );
}

#[test]
fn is_local_false_for_credentials_remote() {
    let config = ClientConfig {
        base_url: String::from("http://user:pass@remote.example.com"),
        ..ClientConfig::default()
    };
    let executor = Executor::new(&config).unwrap_or_else(|_| std::process::abort());
    assert!(
        !executor.is_local(),
        "user:pass@remote.example.com must NOT be detected as local"
    );
}

#[test]
fn executor_with_dry_run() {
    let executor = Executor::with_defaults()
        .unwrap_or_else(|_| {
            Executor::new(&ClientConfig::default()).unwrap_or_else(|_| std::process::abort())
        })
        .with_dry_run(PathBuf::from("/tmp/test-dry-run"));

    assert!(
        executor.dry_run_config().is_some(),
        "dry run should be enabled"
    );
}
