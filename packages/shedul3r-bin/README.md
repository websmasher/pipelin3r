# shedul3r

Task execution server with built-in rate limiting, circuit breaking, retry, and concurrency control.

## Installation

```bash
# Recommended: prebuilt binary (instant, no compilation)
cargo binstall shedul3r

# Or download directly from GitHub Releases
curl -L https://github.com/websmasher/pipelin3r/releases/latest/download/shedul3r-linux-x86_64.tar.gz | tar xz
```

> **Note:** `cargo install shedul3r` will NOT work — this crate is a stub for `cargo binstall`. The real binary is distributed via [GitHub Releases](https://github.com/websmasher/pipelin3r/releases).

## Usage

```bash
# Start the server
shedul3r --port 7943

# Execute a task via CLI
shedul3r execute -t task.yaml

# Execute with stdin input
shedul3r execute -t task.yaml -i "input data"
```

## Features

- REST API for task submission (`POST /api/tasks`)
- Bundle upload/download for remote file transfer
- Rate limiting (fixed-window token bucket)
- Circuit breaking (count-based sliding window)
- Bulkhead (semaphore-based concurrency limiting)
- Retry with exponential backoff
- Environment variable injection per task

## Client SDK

Use [shedul3r-rs-sdk](https://crates.io/crates/shedul3r-rs-sdk) to interact with shedul3r from Rust:

```bash
cargo add shedul3r-rs-sdk
```

## License

MIT
