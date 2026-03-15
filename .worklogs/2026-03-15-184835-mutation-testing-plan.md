# Add mutation testing plan

**Date:** 2026-03-15 18:48
**Scope:** Plans only

## Summary
Added .plans/todo/mutation_testing.md documenting cargo-mutants as the test quality validation step after parser implementation. Runs after all tests pass, injects bugs, checks if tests catch them. Surviving mutants = test gaps. Target >80% kill rate.
