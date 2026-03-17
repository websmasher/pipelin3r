#[test]
fn regression_no_duplicated_limit3r_implementations() {
    // Regression: shedul3r's db crate used to duplicate limit3r
    // implementations instead of re-exporting. Verify that the source
    // files in this crate contain no `impl` blocks — they should only
    // have comments pointing to limit3r.
    let crate_src = env!("CARGO_MANIFEST_DIR");
    let src_dir = std::path::Path::new(crate_src).join("src");

    let check_files = [
        "rate_limiter.rs",
        "circuit_breaker.rs",
        "bulkhead.rs",
        "retry.rs",
    ];

    for filename in &check_files {
        let path = src_dir.join(filename);
        if !path.exists() {
            continue;
        }
        #[allow(clippy::unwrap_used)] // reason: test code, panics are acceptable
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("impl "),
            "{filename} contains an `impl` block — implementations must live in limit3r, not here"
        );
    }
}
