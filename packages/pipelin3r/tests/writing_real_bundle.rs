//! Integration tests for the writing preset using a real Steady Parent bundle.
#![allow(
    unused_crate_dependencies,
    reason = "integration test: deps used by lib not by test binary"
)]
#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test code: direct fs access for fixture copying and assertions"
)]

use std::path::{Path, PathBuf};

use pipelin3r::{
    AgentConfig, DEFAULT_CRITIC_PROMPT, DEFAULT_REWRITER_PROMPT, Executor, WritingStepConfig,
    build_writing_step, run_writing_step,
};

use serde_json as _;
use shedul3r_rs_sdk as _;
use tempfile as _;
use thiserror as _;
use toml as _;
use tracing as _;

const FIXTURE_NAME: &str = "explosive-aggression";

#[test]
fn build_writing_step_accepts_real_steady_parent_bundle_fixture() {
    let input_dir = fixture_input_dir(FIXTURE_NAME);
    let writer_prompt = std::fs::read_to_string(input_dir.join("prompt.md")).unwrap();

    let config = WritingStepConfig::new(
        input_dir,
        writer_prompt,
        DEFAULT_CRITIC_PROMPT,
        DEFAULT_REWRITER_PROMPT,
    );

    let step = build_writing_step(&config, AgentConfig::new("defaults", "")).unwrap();

    assert_eq!(step.name, "writing");
    assert_eq!(
        step.doer.inputs,
        vec![
            String::from("article.json"),
            String::from("ctas.json"),
            String::from("links.json"),
            String::from("mailing.json"),
            String::from("prompt.md"),
            String::from("sources"),
            String::from("sources.json"),
        ]
    );
    assert_eq!(step.doer.outputs, vec![String::from("output/draft.md")]);
}

#[tokio::test]
async fn real_steady_parent_bundle_runs_in_dry_run() {
    let fixture_dir = fixture_root(FIXTURE_NAME);
    let input_dir = fixture_dir.join("input");
    let expected_dir = fixture_dir.join("expected");
    assert!(input_dir.is_dir(), "fixture input must exist");
    assert!(
        expected_dir.join("article.mdx").is_file(),
        "fixture expected output must exist"
    );

    let temp = tempfile::tempdir().unwrap();
    let work_dir = temp.path().join("bundle");
    copy_dir_recursive(&input_dir, &work_dir);

    let capture_dir = temp.path().join("capture");
    let executor = Executor::with_defaults()
        .unwrap()
        .with_dry_run(capture_dir.clone());

    let writer_prompt = std::fs::read_to_string(work_dir.join("prompt.md")).unwrap();
    let config = WritingStepConfig::new(
        work_dir.clone(),
        writer_prompt,
        DEFAULT_CRITIC_PROMPT,
        DEFAULT_REWRITER_PROMPT,
    );

    let result = run_writing_step(&executor, &config, AgentConfig::new("defaults", ""))
        .await
        .unwrap();

    assert!(
        !result.final_output_dir.as_os_str().is_empty(),
        "dry-run should still produce a final output directory for inspection"
    );

    assert_file_exists(
        &result.final_output_dir.join("draft.md"),
        "final draft placeholder",
    );
    assert_file_exists(
        &work_dir.join("writing").join("iter-0").join("prompt.md"),
        "prompt copied into iter-0",
    );
    assert_file_exists(
        &work_dir
            .join("writing")
            .join("iter-0")
            .join("sources")
            .join("5-year-olds-explosive-temper-hitting.md"),
        "real source bundle copied into iter-0",
    );

    let writer_meta = std::fs::read_to_string(
        capture_dir
            .join("writing-writer")
            .join("0")
            .join("meta.json"),
    )
    .unwrap();
    assert!(
        writer_meta.contains("prompt.md"),
        "writer capture should list prompt.md in workDirFiles"
    );
    assert!(
        writer_meta.contains("sources/5-year-olds-explosive-temper-hitting.md"),
        "writer capture should list nested source files from the real bundle"
    );
}

fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("steady-parent")
        .join(name)
}

fn fixture_input_dir(name: &str) -> PathBuf {
    fixture_root(name).join("input")
}

fn assert_file_exists(path: &Path, context: &str) {
    assert!(
        path.is_file(),
        "{context}: {} should be a file",
        path.display()
    );
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();

    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let entry_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &dst_path);
        } else {
            let _ = std::fs::copy(&entry_path, &dst_path).unwrap();
        }
    }
}
