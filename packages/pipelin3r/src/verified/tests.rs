#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(clippy::expect_used, reason = "test setup")]
#![allow(clippy::panic, reason = "test assertions")]
//! Tests for the verified step module.

use std::path::Path;
use std::sync::Arc;

use super::*;

#[test]
fn var_string_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("template.md");
    crate::fs::write(&template_path, "Hello {{NAME}}, output to {{PATH}}").unwrap();

    let step = PromptedStep {
        name: String::from("test"),
        prompt_template: template_path.to_string_lossy().into_owned(),
        vars: vec![
            Var::String {
                placeholder: String::from("{{NAME}}"),
                value: String::from("world"),
            },
            Var::String {
                placeholder: String::from("{{PATH}}"),
                value: String::from("output.json"),
            },
        ],
        inputs: Vec::new(),
        outputs: vec![String::from("output.json")],
    };

    let agent_step = step.resolve(dir.path()).unwrap();
    assert_eq!(
        agent_step.config.prompt,
        "Hello world, output to output.json"
    );
    assert_eq!(agent_step.outputs, vec!["output.json"]);
}

#[test]
fn var_file_resolution() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("template.md");
    crate::fs::write(&template_path, "Spec: {{SPEC}}").unwrap();

    let spec_path = dir.path().join("spec.md");
    crate::fs::write(&spec_path, "RFC 9116 section 4.2").unwrap();

    let step = PromptedStep {
        name: String::from("test"),
        prompt_template: template_path.to_string_lossy().into_owned(),
        vars: vec![Var::File {
            placeholder: String::from("{{SPEC}}"),
            path: String::from("spec.md"),
        }],
        inputs: vec![String::from("spec.md")],
        outputs: Vec::new(),
    };

    let agent_step = step.resolve(dir.path()).unwrap();
    assert_eq!(agent_step.config.prompt, "Spec: RFC 9116 section 4.2");
}

#[test]
fn prompted_step_mixed_vars() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("template.md");
    crate::fs::write(
        &template_path,
        "Package: {{PKG}}\n\nSpec:\n{{SPEC}}\n\nOutput: {{OUT}}",
    )
    .unwrap();

    let spec_path = dir.path().join("spec-reference.md");
    crate::fs::write(&spec_path, "# RFC 9116\nAll the rules.").unwrap();

    let step = PromptedStep {
        name: String::from("resolve"),
        prompt_template: template_path.to_string_lossy().into_owned(),
        vars: vec![
            Var::String {
                placeholder: String::from("{{PKG}}"),
                value: String::from("my-parser"),
            },
            Var::File {
                placeholder: String::from("{{SPEC}}"),
                path: String::from("spec-reference.md"),
            },
            Var::String {
                placeholder: String::from("{{OUT}}"),
                value: String::from("rulings.json"),
            },
        ],
        inputs: vec![String::from("spec-reference.md")],
        outputs: vec![String::from("rulings.json")],
    };

    let agent_step = step.resolve(dir.path()).unwrap();
    assert!(
        agent_step.config.prompt.contains("my-parser"),
        "should contain package name"
    );
    assert!(
        agent_step.config.prompt.contains("# RFC 9116"),
        "should contain spec content"
    );
    assert!(
        agent_step.config.prompt.contains("rulings.json"),
        "should contain output path"
    );
}

#[test]
fn prompted_step_missing_template() {
    let dir = tempfile::tempdir().unwrap();
    let step = PromptedStep {
        name: String::from("test"),
        prompt_template: String::from("/nonexistent/template.md"),
        vars: Vec::new(),
        inputs: Vec::new(),
        outputs: Vec::new(),
    };

    let result = step.resolve(dir.path());
    assert!(result.is_err(), "should fail on missing template");
}

#[test]
fn prompted_step_missing_var_file() {
    let dir = tempfile::tempdir().unwrap();
    let template_path = dir.path().join("template.md");
    crate::fs::write(&template_path, "Content: {{DATA}}").unwrap();

    let step = PromptedStep {
        name: String::from("test"),
        prompt_template: template_path.to_string_lossy().into_owned(),
        vars: vec![Var::File {
            placeholder: String::from("{{DATA}}"),
            path: String::from("missing.json"),
        }],
        inputs: Vec::new(),
        outputs: Vec::new(),
    };

    let result = step.resolve(dir.path());
    assert!(result.is_err(), "should fail on missing Var::File target");
}

#[test]
fn breaker_script_pass() {
    let breaker = Breaker::Script {
        name: String::from("json-check"),
        func: Arc::new(|_path: &Path| Ok(())),
    };

    if let Breaker::Script { func, .. } = &breaker {
        let result = func(Path::new("/tmp"));
        assert!(result.is_ok(), "should pass");
    } else {
        panic!("expected Script variant");
    }
}

#[test]
fn breaker_script_fail() {
    let breaker = Breaker::Script {
        name: String::from("json-check"),
        func: Arc::new(|_path: &Path| Err(String::from("Invalid JSON at line 42"))),
    };

    if let Breaker::Script { func, .. } = &breaker {
        let result = func(Path::new("/tmp"));
        assert!(result.is_err(), "should fail");
        assert_eq!(result.unwrap_err(), "Invalid JSON at line 42");
    } else {
        panic!("expected Script variant");
    }
}

#[test]
fn breaker_debug_impl() {
    let script = Breaker::Script {
        name: String::from("check"),
        func: Arc::new(|_: &Path| Ok(())),
    };
    let debug_str = format!("{script:?}");
    assert!(debug_str.contains("Script"), "debug should show variant");

    let agent = Breaker::Agent {
        name: String::from("review"),
        step: PromptedStep {
            name: String::from("review-agent"),
            prompt_template: String::from("template.md"),
            vars: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        },
    };
    let agent_debug = format!("{agent:?}");
    assert!(
        agent_debug.contains("Agent"),
        "debug should show Agent variant"
    );
}

#[test]
fn copy_dir_recursive_preserves_structure() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();

    // Create a nested directory structure.
    let research = src.path().join("research");
    let rust_dir = research.join("rust");
    crate::fs::create_dir_all(&rust_dir).unwrap();
    crate::fs::write(&research.join("overview.md"), "top-level overview").unwrap();
    crate::fs::write(&rust_dir.join("lib1.json"), r#"{"name":"lib1"}"#).unwrap();
    crate::fs::write(&rust_dir.join("lib2.json"), r#"{"name":"lib2"}"#).unwrap();

    // Copy the "research" directory input.
    let dst_research = dst.path().join("research");
    super::orchestrator::copy_dir_recursive(&research, &dst_research).unwrap();

    // Verify structure is preserved.
    assert!(dst_research.join("overview.md").is_file(), "top-level file");
    assert!(
        dst_research.join("rust").join("lib1.json").is_file(),
        "nested file 1"
    );
    assert!(
        dst_research.join("rust").join("lib2.json").is_file(),
        "nested file 2"
    );

    // Verify content.
    let content = crate::fs::read_to_string(&dst_research.join("overview.md")).unwrap();
    assert_eq!(content, "top-level overview");
}

#[test]
fn verified_step_result_converged() {
    let result = VerifiedStepResult {
        converged: true,
        iterations: 1,
        final_output_dir: std::path::PathBuf::from("/tmp/final"),
        name: String::from("09-resolve"),
    };
    assert!(result.require_converged().is_ok());
}

#[test]
fn verified_step_result_not_converged() {
    let result = VerifiedStepResult {
        converged: false,
        iterations: 3,
        final_output_dir: std::path::PathBuf::from("/tmp/final"),
        name: String::from("09-resolve"),
    };
    let err = result.require_converged().unwrap_err();
    let err_msg = format!("{err}");
    assert!(
        err_msg.contains("09-resolve"),
        "error should include step name"
    );
}
