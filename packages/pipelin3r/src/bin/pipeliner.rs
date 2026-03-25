//! CLI wrapper for reusable pipelin3r presets.

use base64 as _;
use limit3r as _;
use reqwest as _;
use serde as _;
use serde_json as _;
use std::env;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use pipelin3r::{
    AgentConfig, Auth, DEFAULT_CRITIC_PROMPT, DEFAULT_REWRITER_PROMPT, Executor, RetryConfig,
    WritingStepConfig, run_writing_step,
};
use shedul3r_rs_sdk::ClientConfig;
use tempfile as _;
use thiserror as _;
use toml as _;
use tracing as _;

/// Parsed command-line options for the `write` subcommand.
#[derive(Debug)]
struct WriteOptions {
    /// Working directory supplied by the caller.
    work_dir: PathBuf,
    /// Writer prompt content.
    writer_prompt: String,
    /// Critic prompt content.
    critic_prompt: String,
    /// Rewriter prompt content.
    rewriter_prompt: String,
    /// Whether to run `ProseSmasher`.
    use_prosemasher: bool,
    /// Step name under the working directory.
    name: String,
    /// Maximum fixer iterations.
    max_iterations: usize,
    /// shedul3r base URL.
    shedul3r_url: String,
    /// Optional OAuth token.
    oauth_token: Option<String>,
    /// Optional dry-run capture directory.
    dry_run_capture_dir: Option<PathBuf>,
}

/// Entry point.
#[allow(
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::disallowed_methods,
    reason = "CLI binary needs user-facing stdout/stderr output and process exit codes"
)]
fn main() {
    if let Err(error) = real_main() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// Real main that returns errors instead of exiting.
#[tokio::main(flavor = "current_thread")]
#[allow(
    clippy::print_stdout,
    reason = "CLI success path reports the final step location to stdout"
)]
async fn real_main() -> Result<(), String> {
    let collected_args: Vec<String> = env::args().skip(1).collect();
    let mut args = collected_args.iter();
    let Some(subcommand) = args.next() else {
        return Err(usage());
    };

    if subcommand != "write" {
        return Err(usage());
    }

    let options = parse_write_options(args.cloned().collect::<Vec<_>>().as_slice())?;
    let mut executor = build_executor(&options)?;
    if let Some(capture_dir) = &options.dry_run_capture_dir {
        executor = executor.with_dry_run(capture_dir.clone());
    }

    let writing_config = WritingStepConfig {
        name: options.name.clone(),
        work_dir: options.work_dir.clone(),
        writer_prompt: options.writer_prompt,
        critic_prompt: options.critic_prompt,
        rewriter_prompt: options.rewriter_prompt,
        use_prosemasher: options.use_prosemasher,
        max_iterations: options.max_iterations,
    };

    let result = run_writing_step(&executor, &writing_config, agent_defaults()).await;
    match result {
        Ok(step_result) => {
            println!(
                "writing step '{}' finished: converged={} iterations={} final={}",
                writing_config.name,
                step_result.converged,
                step_result.iterations,
                step_result.final_output_dir.display()
            );
            Ok(())
        }
        Err(error) => Err(format!("writing step failed: {error}")),
    }
}

/// Parse the `write` subcommand options.
#[allow(
    clippy::too_many_lines,
    reason = "manual CLI parsing keeps the binary dependency-free"
)]
fn parse_write_options(args: &[String]) -> Result<WriteOptions, String> {
    let mut work_dir: Option<PathBuf> = None;
    let mut writer_prompt_inline: Option<String> = None;
    let mut writer_prompt_file: Option<PathBuf> = None;
    let mut critic_prompt_inline: Option<String> = None;
    let mut critic_prompt_file: Option<PathBuf> = None;
    let mut rewriter_prompt_inline: Option<String> = None;
    let mut rewriter_prompt_file: Option<PathBuf> = None;
    let mut use_prosemasher = true;
    let mut name = String::from("writing");
    let mut max_iterations: usize = 3;
    let mut shedul3r_url = String::from("http://localhost:7943");
    #[allow(
        clippy::disallowed_methods,
        reason = "CLI binaries are allowed to read process environment for default flags"
    )]
    let mut oauth_token: Option<String> = env::var("CLAUDE_CODE_OAUTH_TOKEN").ok();
    let mut dry_run_capture_dir: Option<PathBuf> = None;

    let mut index = 0usize;
    while let Some(flag) = args.get(index) {
        match flag.as_str() {
            "--workdir" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--workdir requires a value"))?;
                work_dir = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            "--writer-prompt" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--writer-prompt requires a value"))?;
                writer_prompt_inline = Some(value.clone());
                index = index.saturating_add(2);
            }
            "--writer-prompt-file" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--writer-prompt-file requires a value"))?;
                writer_prompt_file = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            "--critic-prompt" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--critic-prompt requires a value"))?;
                critic_prompt_inline = Some(value.clone());
                index = index.saturating_add(2);
            }
            "--critic-prompt-file" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--critic-prompt-file requires a value"))?;
                critic_prompt_file = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            "--rewriter-prompt" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--rewriter-prompt requires a value"))?;
                rewriter_prompt_inline = Some(value.clone());
                index = index.saturating_add(2);
            }
            "--rewriter-prompt-file" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--rewriter-prompt-file requires a value"))?;
                rewriter_prompt_file = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            "--use-prosemasher" => {
                use_prosemasher = true;
                index = index.saturating_add(1);
            }
            "--no-prosemasher" => {
                use_prosemasher = false;
                index = index.saturating_add(1);
            }
            "--name" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--name requires a value"))?;
                name.clone_from(value);
                index = index.saturating_add(2);
            }
            "--max-iterations" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--max-iterations requires a value"))?;
                max_iterations = value
                    .parse::<usize>()
                    .map_err(|e| format!("invalid --max-iterations value '{value}': {e}"))?;
                index = index.saturating_add(2);
            }
            "--shedul3r-url" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--shedul3r-url requires a value"))?;
                shedul3r_url.clone_from(value);
                index = index.saturating_add(2);
            }
            "--oauth-token" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--oauth-token requires a value"))?;
                oauth_token = Some(value.clone());
                index = index.saturating_add(2);
            }
            "--dry-run" => {
                let value = args
                    .get(index.saturating_add(1))
                    .ok_or_else(|| String::from("--dry-run requires a capture directory"))?;
                dry_run_capture_dir = Some(PathBuf::from(value));
                index = index.saturating_add(2);
            }
            "--help" | "-h" => return Err(usage()),
            other => return Err(format!("unknown argument: {other}\n\n{}", usage())),
        }
    }

    let work_dir = work_dir.ok_or_else(|| String::from("--workdir is required"))?;
    let writer_prompt = resolve_prompt(writer_prompt_inline, writer_prompt_file, true)?
        .ok_or_else(|| String::from("writer prompt is required"))?;
    let critic_prompt = resolve_prompt(critic_prompt_inline, critic_prompt_file, false)?
        .unwrap_or_else(|| String::from(DEFAULT_CRITIC_PROMPT));
    let rewriter_prompt = resolve_prompt(rewriter_prompt_inline, rewriter_prompt_file, false)?
        .unwrap_or_else(|| String::from(DEFAULT_REWRITER_PROMPT));

    Ok(WriteOptions {
        work_dir,
        writer_prompt,
        critic_prompt,
        rewriter_prompt,
        use_prosemasher,
        name,
        max_iterations,
        shedul3r_url,
        oauth_token,
        dry_run_capture_dir,
    })
}

/// Resolve a prompt from inline text, a file, or stdin.
#[allow(
    clippy::type_complexity,
    reason = "small CLI helper returning optional prompt content or a user-facing error"
)]
fn resolve_prompt(
    inline: Option<String>,
    file: Option<PathBuf>,
    read_stdin_if_missing: bool,
) -> Result<Option<String>, String> {
    if let Some(text) = inline {
        return Ok(Some(text));
    }
    if let Some(path) = file {
        let content = pipelin3r::fs::read_to_string(&path)
            .map_err(|e| format!("failed to read prompt file {}: {e}", path.display()))?;
        return Ok(Some(content));
    }
    if read_stdin_if_missing {
        let mut buffer = String::new();
        let _ = std::io::stdin()
            .read_to_string(&mut buffer)
            .map_err(|e| format!("failed to read writer prompt from stdin: {e}"))?;
        let trimmed = buffer.trim().to_owned();
        if trimmed.is_empty() {
            return Err(String::from(
                "writer prompt is required via --writer-prompt, --writer-prompt-file, or stdin",
            ));
        }
        return Ok(Some(trimmed));
    }
    Ok(None)
}

/// Build an executor from the CLI options.
fn build_executor(options: &WriteOptions) -> Result<Executor, String> {
    let client_config = ClientConfig {
        base_url: options.shedul3r_url.clone(),
        ..ClientConfig::default()
    };
    let executor =
        Executor::new(&client_config).map_err(|e| format!("failed to create executor: {e}"))?;
    if let Some(token) = &options.oauth_token {
        Ok(executor.with_default_auth(Auth::OAuthToken(token.clone())))
    } else {
        Ok(executor)
    }
}

/// Agent defaults used by the CLI wrapper.
fn agent_defaults() -> AgentConfig {
    AgentConfig {
        name: String::new(),
        prompt: String::new(),
        execution_timeout: Some(Duration::from_secs(1800)),
        provider_id: Some(String::from("claude")),
        max_concurrent: Some(3),
        max_wait: Some(Duration::from_secs(7200)),
        retry: Some(RetryConfig {
            max_retries: 2,
            initial_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
            max_delay: Duration::from_secs(30),
        }),
        ..AgentConfig::new("", "")
    }
}

/// CLI usage text.
fn usage() -> String {
    String::from(
        "Usage:\n  \
         pipeliner write --workdir <dir> [--writer-prompt <text> | --writer-prompt-file <file> | stdin]\n  \
         [--critic-prompt <text> | --critic-prompt-file <file>]\n  \
         [--rewriter-prompt <text> | --rewriter-prompt-file <file>]\n  \
         [--no-prosemasher] [--name <step-name>] [--max-iterations <n>]\n  \
         [--shedul3r-url <url>] [--oauth-token <token>] [--dry-run <capture-dir>]",
    )
}
