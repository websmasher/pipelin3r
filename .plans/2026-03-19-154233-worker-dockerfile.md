# Replace railpack.json with Dockerfile for claude-worker

**Date:** 2026-03-19 15:42
**Task:** Single Dockerfile with all language runtimes baked in

## What goes in the image

| Runtime | Source | Why not apt |
|---------|--------|-------------|
| Node 22 | nodesource or official tarball | Need 22, apt has older |
| Python 3 | apt `python3 python3-pip python3-venv` | apt is fine |
| Go 1.22 | official tarball | apt too old |
| Rust | rustup | not in apt |
| PHP 8.3+ | static-php-cli binary or sury PPA | apt has 8.2 |
| Composer | phar download | not in apt |
| Ruby 3.x | apt `ruby ruby-bundler` | apt is fine |
| .NET SDK 9+10 | dotnet-install.sh | not in apt |
| gcc/make | apt `build-essential pkg-config libssl-dev` | |
| git/curl | apt | |
| shedul3r | GitHub release tarball | custom binary |
| gh CLI | GitHub release tarball | |
| Claude Code | npm `@anthropic-ai/claude-code` | |

## Files to change
- `claude-worker/Dockerfile` — NEW, replaces railpack.json
- `claude-worker/railpack.json` — DELETE (Railway uses Dockerfile when present)
- `claude-worker/scripts/install-deps.sh` — KEEP but simplify (just shedul3r + gh, runtimes in Dockerfile)
- `claude-worker/entrypoint.sh` — minor PATH updates

## Dockerfile structure
```dockerfile
FROM debian:bookworm-slim

# System packages (one layer, cached)
RUN apt-get update && apt-get install -y ...

# Node 22 (for Claude Code)
# Go 1.22
# Rust via rustup
# PHP 8.3 static binary
# Composer
# .NET SDK
# shedul3r + gh + claude

COPY entrypoint.sh scripts/ package.json ./
RUN npm install

ENTRYPOINT ["./entrypoint.sh"]
```
