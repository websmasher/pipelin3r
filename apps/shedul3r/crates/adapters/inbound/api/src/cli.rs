//! CLI transport: clap argument parsing and command execution.
//!
//! When the binary is invoked with a subcommand (`execute` or `status`),
//! it runs in CLI mode instead of starting the HTTP daemon.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use domain_types::{EnvironmentMap, SchedulrError, TaskRequest};

use crate::state::{AppState, build_app_state};

/// Schedulr CLI — task execution engine.
#[derive(Debug, Parser)]
#[command(name = "shedul3r", about = "Task execution engine")]
pub struct Cli {
    /// Port for daemon mode (default: 7943).
    #[arg(long)]
    pub port: Option<u16>,

    /// Subcommand to run. If omitted, starts the HTTP daemon.
    #[command(subcommand)]
    pub command: Option<Commands>,
}

/// Available CLI subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Execute a task definition.
    Execute {
        /// YAML task definition (inline string or file path).
        #[arg(short = 't', long = "task")]
        task: String,

        /// Input piped to subprocess stdin.
        #[arg(short = 'i', long = "input")]
        input: Option<String>,

        /// Override limiter key.
        #[arg(short = 'k', long = "limiter-key")]
        limiter_key: Option<String>,

        /// Working directory for subprocess.
        #[arg(short = 'w', long = "workdir")]
        workdir: Option<PathBuf>,

        /// Max execution time in milliseconds.
        #[arg(long = "timeout")]
        timeout: Option<u64>,

        /// Extra environment variables (KEY=VALUE, repeatable).
        #[arg(short = 'e', long = "env")]
        env: Vec<String>,
    },
    /// Show scheduler status.
    Status,
}

/// Runs the CLI command, returning the process exit code.
///
/// # Panics
///
/// Panics if the tokio runtime cannot be created (fatal startup error).
pub fn run_cli(cli: Cli) -> i32 {
    #[allow(clippy::expect_used)] // Startup: tokio runtime creation failure is unrecoverable
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    rt.block_on(run_cli_async(cli))
}

/// Async implementation of CLI command dispatch.
async fn run_cli_async(cli: Cli) -> i32 {
    let state = build_app_state();

    match cli.command {
        Some(Commands::Execute {
            task,
            input,
            limiter_key,
            workdir,
            timeout,
            env,
        }) => run_execute(state, task, input, limiter_key, workdir, timeout, env).await,
        Some(Commands::Status) => run_status(&state),
        None => 0, // unreachable in practice — caller checks command.is_some()
    }
}

/// Parse `KEY=VALUE` strings into an [`EnvironmentMap`].
///
/// Each string is split on the first `=` sign. Strings without `=` are
/// silently skipped.
fn parse_env_vars(env_args: &[String]) -> Option<EnvironmentMap> {
    if env_args.is_empty() {
        return None;
    }

    let map: EnvironmentMap = env_args
        .iter()
        .filter_map(|s| {
            let (key, value) = s.split_once('=')?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect();

    if map.is_empty() { None } else { Some(map) }
}

/// Read task YAML from a file path or treat as inline YAML.
///
/// If the string is a path to an existing file, reads its contents.
/// Otherwise, assumes the string is inline YAML.
fn resolve_task_yaml(task_arg: &str) -> String {
    let path = std::path::Path::new(task_arg);
    if path.exists() && path.is_file() {
        #[allow(clippy::disallowed_methods)] // CLI needs to read task YAML files from disk
        let content = std::fs::read_to_string(path);
        content.unwrap_or_else(|_| task_arg.to_owned())
    } else {
        task_arg.to_owned()
    }
}

/// Execute a task via the engine and print CLI-formatted output.
#[allow(clippy::too_many_arguments)] // CLI dispatch collects all flags
async fn run_execute(
    state: Arc<AppState>,
    task: String,
    input: Option<String>,
    limiter_key: Option<String>,
    workdir: Option<PathBuf>,
    timeout: Option<u64>,
    env: Vec<String>,
) -> i32 {
    let environment = parse_env_vars(&env);
    let task_yaml = resolve_task_yaml(&task);

    let request = TaskRequest {
        task: task_yaml,
        input,
        limiter_key,
        environment,
        working_directory: workdir,
        timeout_ms: timeout,
    };

    match state.engine.execute(request).await {
        Ok(response) => {
            let elapsed_ms = response.metadata.elapsed.as_millis();
            #[allow(clippy::print_stdout)] // CLI output: intentional user-facing display
            {
                println!("Success: {}", response.success);
                println!("Output: {}", response.output);
                println!("Exit code: {}", response.metadata.exit_code);
                println!("Duration: {elapsed_ms}ms");
            }
            0
        }
        Err(SchedulrError::TaskDefinition(msg)) => {
            #[allow(clippy::print_stderr)] // CLI error: intentional user-facing error display
            {
                eprintln!("ERROR: {msg}");
            }
            1
        }
        Err(other) => {
            #[allow(clippy::print_stderr)] // CLI error: intentional user-facing error display
            {
                eprintln!("ERROR: {other}");
            }
            1
        }
    }
}

/// Print scheduler status in CLI format.
fn run_status(state: &AppState) -> i32 {
    let status = state.engine.status();
    #[allow(clippy::print_stdout)] // CLI output: intentional user-facing display
    {
        println!("Active tasks: {}", status.active_tasks);
        println!("Pending tasks: {}", status.pending_tasks);
        println!("Started at: {}", status.started_at);
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_env_vars_empty() {
        assert!(
            parse_env_vars(&[]).is_none(),
            "empty vec should return None"
        );
    }

    #[test]
    fn parse_env_vars_single() {
        let vars = vec!["KEY=value".to_owned()];
        let map = parse_env_vars(&vars);
        assert!(map.is_some(), "should parse one var");
        let m = map.unwrap_or_default();
        assert_eq!(m.get("KEY").map(String::as_str), Some("value"));
    }

    #[test]
    fn parse_env_vars_value_with_equals() {
        let vars = vec!["DB_URL=postgres://host=foo".to_owned()];
        let map = parse_env_vars(&vars);
        assert!(map.is_some(), "should handle = in value");
        let m = map.unwrap_or_default();
        assert_eq!(
            m.get("DB_URL").map(String::as_str),
            Some("postgres://host=foo")
        );
    }

    #[test]
    fn parse_env_vars_no_equals_skipped() {
        let vars = vec!["NOEQUALS".to_owned()];
        assert!(parse_env_vars(&vars).is_none(), "no = should be skipped");
    }
}
