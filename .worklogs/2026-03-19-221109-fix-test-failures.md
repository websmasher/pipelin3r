# Fix test failures: PHP version, Python deps, C# build, 0-test validation

**Date:** 2026-03-19 22:11
**Scope:** websmasher config.rs, s06_run_parsers.rs, claude-worker Dockerfile

## Summary
Fixed root causes of 0-test results across PHP (phpunit version), Python (missing test deps), C# (MSBuild crash). Added validation: test script fails if test files exist but 0 tests parsed.
