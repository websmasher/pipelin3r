# Fix clippy errors in shedul3r-rs-sdk package

**Date:** 2026-03-17 16:58
**Task:** Fix all clippy errors from guardrail3 tightening

## Goal
Zero clippy errors when running `cargo clippy -p shedul3r-rs-sdk --all-targets`.

## Approach

### 1. Type aliases for `type_complexity`
- `client/mod.rs`: Add `pub type EnvironmentMap = BTreeMap<String, String>;` and use in TaskPayload
- `bundle/mod.rs`: Add `type BundleFileRef<'a> = (&'a str, &'a [u8]);` and use in upload_bundle
- `bundle/tests.rs`: Use `BundleFileRef` type alias for local variables
- `client/tests_regression.rs`: Add type alias for `(SocketAddr, JoinHandle<()>)` return type

### 2. `#[allow]` for disallowed_methods
- `client/mod.rs` `Client::new()`: allow `reqwest::Client::builder` (SDK core functionality)
- `client/mod.rs` `http_call()`: allow `reqwest::Response::json` (thin HTTP wrapper)
- `bundle/mod.rs` `upload_bundle()`: allow `reqwest::Response::json` (thin HTTP wrapper)
- `client/tests.rs`: allow `std::fs::write` on test functions
- `client/tests_regression.rs`: allow `std::fs::write` and `serde_json::from_value` on test functions

### 3. Type alias for test mock return types
- `client/tests_regression.rs` `spawn_http_mock`: type alias for `(SocketAddr, JoinHandle<()>)`
- `bundle/tests.rs` `spawn_http_mock`: same

## Files to Modify
- `packages/shedul3r-rs-sdk/src/client/mod.rs`
- `packages/shedul3r-rs-sdk/src/client/tests.rs`
- `packages/shedul3r-rs-sdk/src/client/tests_regression.rs`
- `packages/shedul3r-rs-sdk/src/bundle/mod.rs`
- `packages/shedul3r-rs-sdk/src/bundle/tests.rs`
