#![allow(clippy::unwrap_used, reason = "test assertions")]
#![allow(
    clippy::disallowed_methods,
    reason = "test helper — filesystem setup/teardown"
)]
#![allow(
    clippy::type_complexity,
    reason = "test code: explicit types for clarity"
)]

use super::*;

#[test]
fn filter_files_reduces_count() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    // Write 3 input files.
    let input_a = base.join("a.txt");
    let input_b = base.join("b.txt");
    let input_c = base.join("c.txt");
    std::fs::write(&input_a, b"keep-a").unwrap();
    std::fs::write(&input_b, b"skip-b").unwrap();
    std::fs::write(&input_c, b"keep-c").unwrap();

    let out_dir = base.join("out");

    let result = TransformBuilder::new("filter-test")
        .input_file(&input_a)
        .input_file(&input_b)
        .input_file(&input_c)
        .apply(move |inputs| {
            let outputs: Vec<(PathBuf, Vec<u8>)> = inputs
                .into_iter()
                .filter(|(_, content)| content.starts_with(b"keep"))
                .map(|(path, content)| {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    (out_dir.join(name), content)
                })
                .collect();
            Ok(outputs)
        })
        .execute()
        .unwrap();

    assert_eq!(result.files_read, 3, "should read all 3 input files");
    assert_eq!(
        result.files_written, 2,
        "should write only 2 filtered files"
    );

    // Verify the right files were written.
    assert!(
        base.join("out").join("a.txt").exists(),
        "a.txt should exist in output"
    );
    assert!(
        !base.join("out").join("b.txt").exists(),
        "b.txt should be filtered out"
    );
    assert!(
        base.join("out").join("c.txt").exists(),
        "c.txt should exist in output"
    );
}

#[test]
fn modify_content_uppercase() {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path();

    let input = base.join("hello.txt");
    std::fs::write(&input, b"hello world").unwrap();

    let output_path = base.join("out").join("hello.txt");
    let output_path_clone = output_path.clone();

    let result = TransformBuilder::new("uppercase-test")
        .input_file(&input)
        .apply(move |inputs| {
            let outputs: Vec<(PathBuf, Vec<u8>)> = inputs
                .into_iter()
                .map(|(_, content)| {
                    let upper: Vec<u8> = content.iter().map(u8::to_ascii_uppercase).collect();
                    (output_path_clone.clone(), upper)
                })
                .collect();
            Ok(outputs)
        })
        .execute()
        .unwrap();

    assert_eq!(result.files_read, 1, "should read 1 file");
    assert_eq!(result.files_written, 1, "should write 1 file");

    let written = std::fs::read(&output_path).unwrap();
    assert_eq!(written, b"HELLO WORLD", "content should be uppercased");
}

#[test]
fn empty_inputs_returns_empty() {
    let result = TransformBuilder::new("empty-test")
        .apply(|inputs| {
            assert!(inputs.is_empty(), "should receive no inputs");
            Ok(Vec::new())
        })
        .execute()
        .unwrap();

    assert_eq!(result.files_read, 0, "should read 0 files");
    assert_eq!(result.files_written, 0, "should write 0 files");
}

#[test]
fn missing_apply_returns_error() {
    let result = TransformBuilder::new("no-apply").execute();
    assert!(result.is_err(), "should fail without apply function");
    let msg = result.unwrap_or_else(|e| {
        // Verify error message, then return a dummy.
        assert!(
            e.to_string().contains("no apply function"),
            "error should mention missing apply: {e}"
        );
        TransformResult {
            files_read: 0,
            files_written: 0,
        }
    });
    assert_eq!(msg.files_read, 0, "dummy result");
}

#[test]
fn input_files_bulk_add() {
    let builder = TransformBuilder::new("bulk").input_files(&[
        PathBuf::from("/a"),
        PathBuf::from("/b"),
        PathBuf::from("/c"),
    ]);

    assert_eq!(
        builder.input_files.len(),
        3,
        "should add all files via input_files"
    );
}
