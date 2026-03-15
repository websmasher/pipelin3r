# Implement SDK bundle client and wire remote execution into pipelin3r

**Date:** 2026-03-15 13:07
**Task:** Replace stub bundle methods in shedul3r-rs-sdk with real HTTP implementations; add remote mode to pipelin3r Executor.

## Goal
1. SDK bundle.rs has working upload_bundle (multipart POST), download_file (GET), delete_bundle (DELETE)
2. pipelin3r Executor gains `with_remote()` flag; when enabled, bundles are uploaded before task submission and outputs downloaded after completion

## Approach

### Task 1: SDK bundle client

1. Replace stubs in `packages/shedul3r-rs-sdk/src/bundle.rs` with real implementations
2. `upload_bundle`: build `reqwest::multipart::Form`, POST to `/api/bundles`, deserialize `{id, path}` response
3. `download_file`: GET `/api/bundles/{id}/files/{path}`, return bytes
4. `delete_bundle`: DELETE `/api/bundles/{id}`, expect 204
5. Add serde_json dep to SDK Cargo.toml (needed for deserializing upload response)
6. Add test that upload_bundle builds the correct URL (unit test, no server)

### Task 2: Wire remote bundles into pipelin3r

1. Add `remote: bool` field to Executor, `with_remote()` builder method
2. Add `files()` accessor to Bundle so upload can access file data
3. In agent.rs `execute()`: when remote=true AND bundle present:
   - Upload bundle files via SDK
   - Set working_directory to remote path
   - After task completes, download expected outputs
   - Delete remote bundle
4. Same for batch execution path

## Files to Modify
- `packages/shedul3r-rs-sdk/src/bundle.rs` -- replace stubs with real HTTP
- `packages/shedul3r-rs-sdk/Cargo.toml` -- add serde_json
- `packages/pipelin3r/src/bundle.rs` -- add files() accessor
- `packages/pipelin3r/src/executor.rs` -- add remote flag
- `packages/pipelin3r/src/agent.rs` -- wire remote upload/download into execute paths
