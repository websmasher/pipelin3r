# Bump Pipeliner To 0.1.1

**Date:** 2026-03-26 13:59
**Scope:** `packages/pipelin3r/Cargo.toml`, `packages/pipelin3r/prompts/writing/writer.md`, `packages/pipelin3r/prompts/writing/rewriter.md`

## Summary
Bumped the `pipelin3r` package version from `0.1.0` to `0.1.1` and grouped it with the still-uncommitted prompt-wrapper fix for generic writing artifacts. Rebuilt the release binary so downstream callers can identify a new patch version instead of relying only on the git hash.

## Context & Problem
After the previous cleanup commit, the CLI exposed `--version`, but the semantic version still remained at `0.1.0`. The user called out the obvious problem: if the point of versioning is to distinguish changed binaries, the package version must also move when the shipped behavior changes. At the same time, the worktree still held an uncommitted fix that made the writing wrappers artifact-generic instead of article-specific, so it made sense to ship both together as a patch release.

## Decisions Made

### Bump the crate version to `0.1.1`
- **Chose:** Update `packages/pipelin3r/Cargo.toml` from `0.1.0` to `0.1.1`.
- **Why:** This is a behavior-changing patch release for the CLI and prompt wrappers, not just an internal rebuild. Downstream repos need a new semantic version to refer to.
- **Alternatives considered:**
  - Leave the version at `0.1.0` and rely only on the embedded git hash — rejected because it makes version numbers meaningless for consumers.
  - Jump to a minor release — rejected because the change is still a patch-level fix to an existing feature line.

### Ship the prompt-wrapper fix in the same patch release
- **Chose:** Include the pending `writer.md` / `rewriter.md` changes in the same commit as the version bump.
- **Why:** Those changes are part of the actual runtime behavior that motivated the version change. Leaving them uncommitted would produce a misleading `0.1.1` build that does not correspond to the fixed behavior.
- **Alternatives considered:**
  - Version-bump only and leave prompt changes dirty — rejected because it would immediately invalidate the new version number.
  - Make a second follow-up commit for prompts — rejected because the worktree already contained the fix and the user explicitly wanted the version to reflect changed behavior.

## Architectural Notes
- The semantic version now advances independently from the embedded git metadata added in the prior commit.
- `pipeliner --version` therefore reports both:
  - semantic version (`0.1.1`)
  - source identity (`ac83d66` before this commit, plus dirty/clean state)
- The prompt-wrapper changes keep the generic writing preset aligned with non-article artifacts such as `answer.md`.

## Information Sources
- `packages/pipelin3r/Cargo.toml` — package version source of truth
- `packages/pipelin3r/prompts/writing/writer.md`
- `packages/pipelin3r/prompts/writing/rewriter.md`
- `target/release/pipeliner --version`
- `cargo test -p pipelin3r presets::writing::tests -- --nocapture`
- `.worklogs/2026-03-26-134453-writing-cli-version-and-cleanup.md`

## Open Questions / Future Considerations
- If the expectation is truly “bump on every shipped change,” that should probably become an explicit release policy or hook, not just a manual convention.
- The Quora remote replay still needs live end-to-end validation after the prompt-wrapper fix; this worklog only covers shipping the fix and version bump together.

## Key Files for Context
- `packages/pipelin3r/Cargo.toml` — semantic version source of truth
- `packages/pipelin3r/prompts/writing/writer.md` — generic writer artifact contract
- `packages/pipelin3r/prompts/writing/rewriter.md` — generic rewriter artifact contract
- `.worklogs/2026-03-26-134453-writing-cli-version-and-cleanup.md` — prior CLI version-surface work

## Next Steps / Continuation Plan
1. Commit this version bump and prompt-wrapper fix together, then rebuild `target/release/pipeliner` so `--version` reports the new clean commit.
2. Re-run the real Quora bundle against the remote worker and confirm whether `iter-1/output/answer.md` is still empty after the artifact-generic prompt fix.
3. If the team wants stronger guarantees, add an explicit release/versioning policy document or a pre-commit/pre-release check that refuses behavior-changing commits without a package version bump.
