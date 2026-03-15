# shedul3r-rs-sdk

Rust client SDK for the [shedul3r](https://github.com/3jane/pipelin3r) task execution server.

## Installation

```sh
cargo add shedul3r-rs-sdk
```

## Quick Start

```rust
use shedul3r_rs_sdk::{Client, TaskPayload};

let client = Client::with_defaults()?;

let payload = TaskPayload {
    task: String::from("name: my-task\ncommand: echo hello"),
    input: String::from("world"),
    working_directory: None,
    environment: None,
    limiter_key: None,
    timeout_ms: None,
};

let result = client.submit_task(&payload).await?;
assert!(result.success);
println!("{}", result.output);
```

## Features

- **Task submission** with configurable HTTP timeouts (default 45 minutes)
- **File-poll recovery** -- races HTTP response against disk polling for long-running tasks where connections may drop
- **Bundle upload/download** -- transfer file bundles to and from the server for remote execution
- **Typed errors** -- `SdkError` enum with `Http`, `Json`, `PollTimeout` variants (no `anyhow`)
- Configurable base URL, poll interval, initial delay, and max poll duration via `ClientConfig`

## License

MIT
