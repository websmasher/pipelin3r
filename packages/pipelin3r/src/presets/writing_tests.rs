#![allow(clippy::unwrap_used, reason = "test assertions")]

use super::*;

#[test]
fn discover_workspace_inputs_excludes_step_dir() {
    let dir = tempfile::tempdir().unwrap();
    crate::fs::write(&dir.path().join("brief.md"), "brief").unwrap();
    crate::fs::create_dir_all(&dir.path().join("research")).unwrap();
    crate::fs::create_dir_all(&dir.path().join("writing")).unwrap();

    let inputs = discover_workspace_inputs(dir.path(), "writing").unwrap();

    assert_eq!(
        inputs,
        vec![String::from("brief.md"), String::from("research")]
    );
}

#[test]
fn build_writing_step_uses_workspace_entries() {
    let dir = tempfile::tempdir().unwrap();
    crate::fs::write(&dir.path().join("brief.md"), "brief").unwrap();
    crate::fs::create_dir_all(&dir.path().join("research")).unwrap();

    let mut config = WritingStepConfig::new(dir.path().to_path_buf(), "write", "critic", "rewrite");
    config.use_prosemasher = true;

    let step = build_writing_step(&config, AgentConfig::new("defaults", "")).unwrap();

    assert_eq!(step.name, "writing");
    assert_eq!(
        step.doer.inputs,
        vec![String::from("brief.md"), String::from("research")]
    );
    assert_eq!(
        step.doer.outputs,
        vec![artifact_output_path(DEFAULT_ARTIFACT_PATH)]
    );
    assert_eq!(step.breakers.len(), 2);
    assert_eq!(
        step.fixer.outputs,
        vec![artifact_output_path(DEFAULT_ARTIFACT_PATH)]
    );
    assert!(
        step.fixer.inputs.contains(&String::from(ISSUES_INPUT_PATH)),
        "fixer should receive merged issues"
    );
    assert!(
        step.fixer
            .inputs
            .contains(&artifact_input_path(DEFAULT_ARTIFACT_PATH)),
        "fixer should receive the staged current draft"
    );
    assert!(
        step.fixer
            .inputs
            .contains(&String::from(CRITIC_REPORT_INPUT_PATH)),
        "fixer should receive the structured critic report"
    );
    assert!(
        step.fixer
            .inputs
            .contains(&String::from(PROSESMASHER_REPORT_PATH)),
        "fixer should receive the prosemasher report when enabled"
    );
}

#[test]
fn writing_config_enables_prosemasher_by_default() {
    let dir = tempfile::tempdir().unwrap();
    let config = WritingStepConfig::new(dir.path().to_path_buf(), "write", "critic", "rewrite");
    assert!(config.use_prosemasher);
}

#[test]
fn prosemasher_clean_report_passes() {
    let report = serde_json::json!({
        "success": true,
        "failed": 0,
        "failures": [],
    });
    assert!(prosemasher_report_is_clean(&report));
}

#[test]
fn prosemasher_non_empty_issues_fails() {
    let report = serde_json::json!({
        "success": false,
        "failed": 1,
        "failures": [{ "message": "too much passive voice" }],
    });
    assert!(!prosemasher_report_is_clean(&report));
}
