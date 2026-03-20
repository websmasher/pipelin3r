# Add t3str binary release workflow

## Summary

Added GitHub Actions workflow to build and release t3str binaries on push to production, mirroring the existing shedul3r release workflow.

## Decisions

- **Same target matrix as shedul3r**: linux-gnu, linux-musl (static), linux-arm64, darwin-x86_64, darwin-arm64. The worker uses the musl build.
- **Separate tag namespace**: Uses `t3str-v*` tags to avoid collision with `shedul3r-v*` tags. This is important because GitHub's `/releases/latest` only returns one release — with separate tags, we use the API to find the latest per-prefix.
- **Version from workspace Cargo.toml**: Extracts version from `apps/t3str/Cargo.toml` `[workspace.package]` section.

## Key Files

- `.github/workflows/release-t3str.yml` — CI workflow for t3str binary releases

## Next Steps

- Push to production branch to trigger first t3str release
- Rebuild claude-worker Docker image to pick up t3str from install-deps.sh
