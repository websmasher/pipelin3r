//! t3str CLI -- test extraction and execution tool.

#![allow(unused_crate_dependencies)] // bin crate gets false positives from workspace deps

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use t3str_domain_types::{Language, T3strError};

/// t3str -- multi-language test extraction and execution.
#[derive(Debug, Parser)]
#[command(name = "t3str", version, about)]
struct Cli {
    /// Output format.
    #[arg(long, default_value = "json")]
    format: OutputFormat,

    /// Subcommand to execute.
    #[command(subcommand)]
    command: Command,
}

/// Output format for results.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OutputFormat {
    /// JSON output (default, for pipeline consumption).
    Json,
    /// Human-readable output (for local development).
    Human,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Discover and extract test information from a repository.
    Extract {
        /// Path to the repository directory.
        #[arg(long)]
        repo: PathBuf,
        /// Programming language of the repository.
        #[arg(long)]
        lang: LanguageArg,
        /// Optional topic filter to find relevant tests.
        #[arg(long)]
        topic: Option<String>,
    },
    /// Execute tests in a repository and parse results.
    Run {
        /// Path to the repository directory.
        #[arg(long)]
        repo: PathBuf,
        /// Programming language of the repository.
        #[arg(long)]
        lang: LanguageArg,
        /// Optional test filter expression.
        #[arg(long)]
        filter: Option<String>,
    },
}

/// Wrapper around [`Language`] that implements [`clap::ValueEnum`].
///
/// The domain `Language` type lives in `t3str-domain-types` and doesn't
/// derive clap traits. This wrapper bridges the gap for CLI argument parsing.
#[derive(Debug, Clone, Copy)]
struct LanguageArg(Language);

impl clap::ValueEnum for LanguageArg {
    fn value_variants<'a>() -> &'a [Self] {
        /// All supported language variants for clap.
        static VARIANTS: &[LanguageArg] = &[
            LanguageArg(Language::Rust),
            LanguageArg(Language::Python),
            LanguageArg(Language::Go),
            LanguageArg(Language::Javascript),
            LanguageArg(Language::Php),
            LanguageArg(Language::Csharp),
            LanguageArg(Language::Ruby),
            LanguageArg(Language::Java),
            LanguageArg(Language::Elixir),
        ];
        VARIANTS
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        let value = match self.0 {
            Language::Rust => clap::builder::PossibleValue::new("rust"),
            Language::Python => clap::builder::PossibleValue::new("python"),
            Language::Go => clap::builder::PossibleValue::new("go"),
            Language::Javascript => clap::builder::PossibleValue::new("javascript")
                .alias("js")
                .alias("typescript")
                .alias("ts"),
            Language::Php => clap::builder::PossibleValue::new("php"),
            Language::Csharp => clap::builder::PossibleValue::new("csharp")
                .alias("c#")
                .alias("dotnet"),
            Language::Ruby => clap::builder::PossibleValue::new("ruby"),
            Language::Java => clap::builder::PossibleValue::new("java"),
            Language::Elixir => clap::builder::PossibleValue::new("elixir"),
        };
        Some(value)
    }
}

fn main() -> std::process::ExitCode {
    init_tracing();

    let cli = Cli::parse();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(err) => {
            report_error(&err.to_string());
            return std::process::ExitCode::FAILURE;
        }
    };

    let result = rt.block_on(run_command(&cli));

    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            report_error(&err.to_string());
            std::process::ExitCode::FAILURE
        }
    }
}

/// Initialize the tracing subscriber with env-filter support.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")); // CLI default: only warnings

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// Report an error message to stderr.
#[allow(clippy::print_stderr)] // Error reporting to stderr is expected for CLI
fn report_error(msg: &str) {
    eprintln!("Error: {msg}");
}

/// Dispatch the parsed CLI command to the appropriate handler.
async fn run_command(cli: &Cli) -> Result<(), T3strError> {
    match &cli.command {
        Command::Extract { repo, lang, topic } => {
            run_extract(repo, lang.0, topic.as_deref(), cli.format)
        }
        Command::Run { repo, lang, filter } => {
            run_execute(repo, lang.0, filter.as_deref(), cli.format).await
        }
    }
}

/// Run test extraction and output results.
fn run_extract(
    repo: &std::path::Path,
    language: Language,
    topic: Option<&str>,
    format: OutputFormat,
) -> Result<(), T3strError> {
    let discoverer = t3str_extract::TreeSitterDiscoverer;
    let results = t3str_commands::ExtractCommand::run(&discoverer, repo, language, topic)?;

    output_extract_results(&results, format)
}

/// Format and print extraction results.
#[allow(clippy::print_stdout)] // CLI output is the primary interface
fn output_extract_results(
    results: &[t3str_domain_types::TestFile],
    format: OutputFormat,
) -> Result<(), T3strError> {
    match format {
        OutputFormat::Json => {
            let json =
                serde_json::to_string_pretty(results).map_err(|e| T3strError::ParseFailed {
                    format: "json".into(),
                    reason: e.to_string(),
                })?;
            println!("{json}");
        }
        OutputFormat::Human => {
            for file in results {
                println!(
                    "{}: {} functions",
                    file.path.display(),
                    file.functions.len()
                );
            }
        }
    }
    Ok(())
}

/// Run test execution and output results.
async fn run_execute(
    repo: &std::path::Path,
    language: Language,
    filter: Option<&str>,
    format: OutputFormat,
) -> Result<(), T3strError> {
    let executor = t3str_run::ProcessTestExecutor;
    let suite = t3str_commands::RunCommand::run(&executor, repo, language, filter).await?;

    output_run_results(&suite, format)
}

/// Format and print test execution results.
#[allow(clippy::print_stdout)] // CLI output is the primary interface
fn output_run_results(
    suite: &t3str_domain_types::TestSuite,
    format: OutputFormat,
) -> Result<(), T3strError> {
    match format {
        OutputFormat::Json => {
            let json =
                serde_json::to_string_pretty(suite).map_err(|e| T3strError::ParseFailed {
                    format: "json".into(),
                    reason: e.to_string(),
                })?;
            println!("{json}");
        }
        OutputFormat::Human => {
            println!(
                "Tests: {} total, {} passed, {} failed, {} skipped",
                suite.summary.total,
                suite.summary.passed,
                suite.summary.failed,
                suite.summary.skipped,
            );
        }
    }
    Ok(())
}
