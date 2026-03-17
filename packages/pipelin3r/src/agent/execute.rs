//! Execution helpers for agent invocations: dry-run capture, work-dir
//! transport, single-task dispatch, and duration formatting.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use shedul3r_rs_sdk::TaskPayload;

use super::{AgentResult, AgentTask, BatchConfig};
use crate::auth::{EnvironmentMap, merge_env};
use crate::bundle::validate_path;
use crate::error::PipelineError;
use crate::executor::extract_step_name;
use crate::task::{TaskConfig, build_task_yaml};

/// Validate that a work directory path is safe and well-formed.
///
/// Rejects empty paths, relative paths, non-existent paths, non-directories,
/// and paths containing parent (`..`) components.
///
/// # Errors
/// Returns `PipelineError::Config` if the path fails any check.
pub(super) fn validate_work_dir(path: &Path) -> Result<(), PipelineError> {
    if path.as_os_str().is_empty() {
        return Err(PipelineError::Config(String::from(
            "work_dir must not be empty",
        )));
    }
    if !path.is_absolute() {
        return Err(PipelineError::Config(format!(
            "work_dir must be an absolute path: {}",
            path.display()
        )));
    }
    if path == Path::new("/") {
        return Err(PipelineError::Config(String::from(
            "work_dir must not be the filesystem root",
        )));
    }
    for component in path.components() {
        if matches!(component, std::path::Component::ParentDir) {
            return Err(PipelineError::Config(format!(
                "work_dir must not contain '..' components: {}",
                path.display()
            )));
        }
    }
    if !path.exists() {
        return Err(PipelineError::Config(format!(
            "work_dir does not exist: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(PipelineError::Config(format!(
            "work_dir is not a directory: {}",
            path.display()
        )));
    }
    Ok(())
}

/// Count how many results succeeded vs failed.
#[allow(
    clippy::type_complexity,
    reason = "generic function: type parameterization is intentional"
)]
pub(super) fn count_batch_outcomes<T, E>(results: &[Result<T, E>]) -> (usize, usize) {
    let mut succeeded: usize = 0;
    let mut failed: usize = 0;
    for r in results {
        if r.is_ok() {
            succeeded = succeeded.saturating_add(1);
        } else {
            failed = failed.saturating_add(1);
        }
    }
    (succeeded, failed)
}

/// Returns `true` when a batch has both successes and failures (partial failure).
pub(super) const fn is_partial_failure(succeeded: usize, failed: usize) -> bool {
    failed > 0 && succeeded > 0
}

/// Write a dry-run capture for a single invocation.
#[allow(
    clippy::disallowed_types,
    reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
)]
pub(super) fn execute_dry_run_capture(
    dry_run_mutex: &std::sync::Mutex<crate::executor::DryRunConfig>,
    task_yaml: &str,
    prompt: &str,
    work_dir: Option<&Path>,
    env: Option<&EnvironmentMap>,
) -> Result<AgentResult, PipelineError> {
    let mut guard = dry_run_mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let step_name = extract_step_name(task_yaml);
    let index = guard.counters.get(&step_name).copied().unwrap_or(0);
    let next = index.saturating_add(1);
    let _ = guard.counters.insert(step_name.clone(), next);

    let capture_dir = guard.base_dir.join(&step_name).join(index.to_string());
    drop(guard); // Release lock before I/O.

    crate::fs::create_dir_all(&capture_dir)?;
    crate::fs::write(&capture_dir.join("prompt.md"), prompt)?;
    crate::fs::write(&capture_dir.join("task.yaml"), task_yaml)?;

    // Collect environment variable names (redacted — keys only, no values).
    let env_keys: Vec<&str> = env
        .map(|m| m.keys().map(String::as_str).collect())
        .unwrap_or_default();

    // List files in the work directory (relative paths, not contents).
    let work_dir_files: Vec<String> = if let Some(dir) = work_dir {
        collect_relative_paths(dir)?
    } else {
        Vec::new()
    };

    let meta = serde_json::json!({
        "workDir": work_dir.map(|p| p.display().to_string()),
        "workDirFiles": work_dir_files,
        "environment": env_keys,
    });
    crate::fs::write(
        &capture_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)
            .map_err(|e| PipelineError::Other(format!("failed to serialize meta: {e}")))?,
    )?;

    tracing::info!("[dry-run] Captured to {}", capture_dir.display());
    Ok(AgentResult {
        success: true,
        output: String::from("(dry-run)"),
    })
}

/// Recursively collect relative file paths from a directory.
#[allow(
    clippy::type_complexity,
    reason = "return type reflects standard Vec<Result> pattern"
)]
fn collect_relative_paths(base: &Path) -> Result<Vec<String>, PipelineError> {
    let mut paths = Vec::new();
    if !base.is_dir() {
        return Ok(paths);
    }
    let canonical_base = crate::fs::canonicalize(base)?;
    let mut visited = BTreeSet::new();
    let _inserted = visited.insert(canonical_base.clone());
    collect_relative_paths_inner(&canonical_base, base, &mut paths, &mut visited)?;
    paths.sort();
    Ok(paths)
}

/// Inner recursive helper for [`collect_relative_paths`].
fn collect_relative_paths_inner(
    canonical_base: &Path,
    current: &Path,
    out: &mut Vec<String>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), PipelineError> {
    let entries = crate::fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let canonical_path = match crate::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("skipping unresolvable path {}: {e}", path.display());
                continue;
            }
        };
        if !canonical_path.starts_with(canonical_base) {
            tracing::warn!(
                "skipping symlink escape: {} resolves outside base {}",
                path.display(),
                canonical_base.display()
            );
            continue;
        }
        if canonical_path.is_dir() {
            // Skip already-visited directories to prevent symlink loops.
            if !visited.insert(canonical_path) {
                continue;
            }
            collect_relative_paths_inner(canonical_base, &path, out, visited)?;
        } else if let Ok(rel) = canonical_path.strip_prefix(canonical_base) {
            out.push(rel.display().to_string());
        }
    }
    Ok(())
}

/// Write a dry-run capture for a batch task.
#[allow(
    clippy::disallowed_types,
    reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
)]
pub(super) fn execute_batch_task_dry_run(
    task: &AgentTask,
    config: &BatchConfig,
    dry_run_mutex: &std::sync::Mutex<crate::executor::DryRunConfig>,
) -> Result<AgentResult, PipelineError> {
    // Validate work_dir before any work happens (matches real-mode behavior).
    if let Some(ref dir) = task.work_dir {
        validate_work_dir(dir)?;
    }

    let prompt = task
        .prompt
        .as_ref()
        .ok_or_else(|| PipelineError::Config(String::from("agent task prompt is required")))?;

    let task_yaml = build_task_yaml(&TaskConfig {
        name: config.name.clone(),
        model: config.model.clone(),
        timeout: config.timeout.clone(),
        provider_id: None,
        max_concurrent: None,
        max_wait: None,
        max_retries: None,
        allowed_tools: config.tools.clone(),
    })?;

    // Resolve auth for meta: task override > batch default.
    let auth_env = if let Some(ref auth) = task.auth {
        Some(auth.to_env()?)
    } else if config.default_auth_env.is_empty() {
        None
    } else {
        Some(config.default_auth_env.clone())
    };

    execute_dry_run_capture(
        dry_run_mutex,
        &task_yaml,
        prompt,
        task.work_dir.as_deref(),
        auth_env.as_ref(),
    )
}

/// Execute a task with work-dir transport: auto-detect local vs remote.
///
/// **Local** (shared filesystem): passes the work-dir path directly to the
/// server as `working_directory`. Output files are already in the work-dir
/// after execution.
///
/// **Remote**: reads all files from the work-dir, uploads them as a bundle,
/// passes the bundle's remote path as `working_directory`, then downloads
/// `expected_outputs` back into the local work-dir and cleans up the bundle.
#[allow(clippy::too_many_arguments)] // reason: flat param list avoids an intermediate struct for a private helper
pub(super) async fn execute_with_work_dir(
    client: &shedul3r_rs_sdk::Client,
    remote: bool,
    task_yaml: &str,
    prompt: &str,
    work_dir: Option<&Path>,
    expected_outputs: &[String],
    env: Option<EnvironmentMap>,
) -> Result<AgentResult, PipelineError> {
    // Upload work-dir contents when remote mode is enabled and a work-dir is set.
    let bundle_handle = if remote {
        if let Some(dir) = work_dir {
            let files = read_dir_to_memory(dir)?;
            if files.is_empty() {
                None
            } else {
                #[allow(
                    clippy::type_complexity,
                    reason = "explicit SDK upload type for clarity"
                )]
                let file_refs: Vec<(&str, &[u8])> = files
                    .iter()
                    .map(|(name, content)| (name.as_str(), content.as_slice()))
                    .collect();
                Some(client.upload_bundle(&file_refs).await?)
            }
        } else {
            None
        }
    } else {
        None
    };

    // Use remote path as working directory when a bundle was uploaded.
    let working_directory = if let Some(ref handle) = bundle_handle {
        Some(handle.remote_path.clone())
    } else {
        work_dir.map(|p| p.display().to_string())
    };

    let payload = TaskPayload {
        task: String::from(task_yaml),
        input: String::from(prompt),
        working_directory,
        environment: env,
        limiter_key: None,
        timeout_ms: None,
    };

    // Wrap execution in a block that always cleans up the bundle.
    let execution_result = async {
        let result = client.submit_task(&payload).await?;

        // Download expected outputs from remote bundle back to local work-dir.
        if let Some(ref handle) = bundle_handle {
            if let Some(dir) = work_dir {
                for output_path in expected_outputs {
                    // Validate each output path to prevent path traversal.
                    validate_path(output_path)?;

                    let bytes = client.download_file(&handle.id, output_path).await?;

                    let local_path = dir.join(output_path);
                    if let Some(parent) = local_path.parent() {
                        tokio::fs::create_dir_all(parent).await?;
                    }
                    tokio::fs::write(&local_path, &bytes).await?;
                }
            }
        }

        Ok::<AgentResult, PipelineError>(AgentResult {
            success: result.success,
            output: result.output,
        })
    }
    .await;

    // Always clean up remote bundle, regardless of success/failure.
    if let Some(ref handle) = bundle_handle {
        if let Err(e) = client.delete_bundle(&handle.id).await {
            tracing::warn!("failed to delete remote bundle {}: {e}", handle.id);
        }
    }

    execution_result
}

/// Type alias for in-memory file pairs: `(relative_path, content)`.
type FilePair = (String, Vec<u8>);

/// Read all files from a directory into memory as `(relative_path, content)` pairs.
///
/// **Limitation:** Empty subdirectories are not included in the output. The
/// bundle upload format only supports file pairs, so empty directories are
/// silently dropped during remote upload. This is acceptable because most
/// workflows create directories as needed when writing output files.
#[allow(
    clippy::type_complexity,
    reason = "return type uses local FilePair alias"
)]
fn read_dir_to_memory(base: &Path) -> Result<Vec<FilePair>, PipelineError> {
    let mut files = Vec::new();
    if !base.is_dir() {
        return Ok(files);
    }
    let canonical_base = crate::fs::canonicalize(base)?;
    let mut visited = BTreeSet::new();
    let _inserted = visited.insert(canonical_base.clone());
    read_dir_to_memory_inner(&canonical_base, base, &mut files, &mut visited)?;
    Ok(files)
}

/// Inner recursive helper for [`read_dir_to_memory`].
fn read_dir_to_memory_inner(
    canonical_base: &Path,
    current: &Path,
    out: &mut Vec<FilePair>,
    visited: &mut BTreeSet<PathBuf>,
) -> Result<(), PipelineError> {
    let entries = crate::fs::read_dir(current)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let canonical_path = match crate::fs::canonicalize(&path) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("skipping unresolvable path {}: {e}", path.display());
                continue;
            }
        };
        if !canonical_path.starts_with(canonical_base) {
            tracing::warn!(
                "skipping symlink escape: {} resolves outside base {}",
                path.display(),
                canonical_base.display()
            );
            continue;
        }
        if canonical_path.is_dir() {
            // Skip already-visited directories to prevent symlink loops.
            if !visited.insert(canonical_path) {
                continue;
            }
            read_dir_to_memory_inner(canonical_base, &path, out, visited)?;
        } else if let Ok(rel) = canonical_path.strip_prefix(canonical_base) {
            let content = crate::fs::read(&path)?;
            out.push((rel.display().to_string(), content));
        }
    }
    Ok(())
}

/// Execute a single task via the SDK client, with work-dir transport.
pub(super) async fn execute_single_task(
    task: &AgentTask,
    config: &BatchConfig,
    client: &shedul3r_rs_sdk::Client,
) -> Result<AgentResult, PipelineError> {
    // Validate work_dir before any work happens.
    if let Some(ref dir) = task.work_dir {
        validate_work_dir(dir)?;
    }

    let prompt = task
        .prompt
        .as_ref()
        .ok_or_else(|| PipelineError::Config(String::from("agent task prompt is required")))?;

    let task_yaml = build_task_yaml(&TaskConfig {
        name: config.name.clone(),
        model: config.model.clone(),
        timeout: config.timeout.clone(),
        provider_id: None,
        max_concurrent: None,
        max_wait: None,
        max_retries: None,
        allowed_tools: config.tools.clone(),
    })?;

    // Resolve auth: task override > batch default.
    let auth_env = if let Some(ref auth) = task.auth {
        auth.to_env()?
    } else {
        config.default_auth_env.clone()
    };

    let env = merge_env(auth_env, None);

    // Execute via the work-dir transport helper.
    execute_with_work_dir(
        client,
        !config.is_local,
        &task_yaml,
        prompt,
        task.work_dir.as_deref(),
        &task.expected_outputs,
        env,
    )
    .await
}

/// Format a `Duration` as a human-readable timeout string for task YAML.
pub(super) fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs.checked_div(3600).unwrap_or(0);
    let remaining = total_secs.saturating_sub(hours.saturating_mul(3600));
    let minutes = remaining.checked_div(60).unwrap_or(0);
    let seconds = remaining.saturating_sub(minutes.saturating_mul(60));

    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h{minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m{seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}
