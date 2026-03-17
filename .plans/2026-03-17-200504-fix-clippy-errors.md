# Fix all clippy errors in pipelin3r workspace

**Date:** 2026-03-17 20:05
**Task:** Fix all clippy errors across the pipelin3r workspace

## Goal
Zero clippy errors from `cargo clippy --workspace --all-targets`.

## Approach

### Files and fixes needed:

1. **command/mod.rs** (line 19): `type_complexity` on `Option<BTreeMap<String, String>>` - add type alias
2. **image_gen/client.rs**:
   - Lines 1,10,13: `doc_markdown` - backtick `OpenRouter`
   - Lines 17,58: `redundant_pub_crate` - change to `pub`
   - Line 79: `items_after_statements` - move `use base64::Engine as _` to top of function
   - Lines 98: `doc_markdown` - backtick `mime_type` and `base64_data`
   - Line 99: `type_complexity` on return type - add `#[allow]`
   - Line 137,177: `redundant_pub_crate` on `send_image_request`/`fetch_cost` - change to `pub`
   - Line 155: `unwrap_or_default` - disallowed method, add allow
3. **image_gen/mod.rs**:
   - Lines 240,248: `disallowed_methods` for `std::fs::create_dir_all` and `std::fs::write` - add `#[allow]`
4. **image_gen/tests.rs**:
   - Lines 87,89,110: `indexing_slicing` - use `.get()` instead
5. **validate/action.rs** (lines 23-26): `type_complexity` on `FunctionFix` - add type alias
6. **validate/report.rs**:
   - Line 54: `missing_const_for_fn` on `pass()` - add `const`
   - Line 74: `missing_const_for_fn` on `fail()` - add `const`
7. **validate/tests.rs**:
   - Lines 257,285,307,335: `disallowed_methods` for `std::fs::remove_dir_all` - add `#[allow]`
