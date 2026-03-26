//! Execution helpers for agent invocations: dry-run capture, work-dir
//! transport, and duration formatting.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use shedul3r_rs_sdk::TaskPayload;

use super::AgentResult;
use crate::auth::EnvironmentMap;
use crate::bundle::validate_path;
use crate::error::PipelineError;
use crate::executor::extract_step_name;

/// Validate that a work directory path is safe and well-formed.
///
/// Rejects empty paths, relative paths, non-existent paths, non-directories,
/// and paths containing parent (`..`) components.
///
/// # Errors
/// Returns `PipelineError::Config` if the path fails any check.
pub fn validate_work_dir(path: &Path) -> Result<(), PipelineError> {
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

/// Write a dry-run capture for a single invocation.
#[allow(
    clippy::disallowed_types,
    reason = "published library: avoiding parking_lot dependency to minimize dependency tree"
)]
pub fn execute_dry_run_capture(
    dry_run_mutex: &std::sync::Mutex<crate::executor::DryRunConfig>,
    task_yaml: &str,
    prompt: &str,
    work_dir: Option<&Path>,
    expected_outputs: &[String],
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
        "expectedOutputs": expected_outputs,
        "environment": env_keys,
    });
    crate::fs::write(
        &capture_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)
            .map_err(|e| PipelineError::Other(format!("failed to serialize meta: {e}")))?,
    )?;

    tracing::info!("[dry-run] Captured to {}", capture_dir.display());
    let mut output_files = BTreeMap::new();
    if let Some(dir) = work_dir {
        for output_path in expected_outputs {
            validate_path(output_path)?;
            let local_path = dir.join(output_path);
            if let Some(parent) = local_path.parent() {
                crate::fs::create_dir_all(parent)?;
            }
            let placeholder = format!("(dry-run placeholder for {output_path})\n");
            crate::fs::write(&local_path, &placeholder)?;
            let _ = output_files.insert(output_path.clone(), placeholder);
        }
    }

    Ok(AgentResult {
        success: true,
        output: String::from("(dry-run)"),
        output_files,
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

/// Execute a task with work-dir transport: auto-detect local vs remote.
///
/// **Local** (shared filesystem): passes the work-dir path directly to the
/// server as `working_directory`. Output files are already in the work-dir
/// after execution.
///
/// **Remote**: reads all files from the work-dir, uploads them as a bundle,
/// passes the bundle's remote path as `working_directory`, then downloads
/// `expected_outputs` back into the local work-dir and cleans up the bundle.
#[allow(
    clippy::too_many_arguments,
    reason = "flat param list avoids an intermediate struct for a private helper"
)]
#[allow(
    clippy::too_many_lines,
    reason = "work-dir transport has sequential phases that are clearer kept together"
)]
pub async fn execute_with_work_dir(
    client: &shedul3r_rs_sdk::Client,
    remote: bool,
    task_yaml: &str,
    prompt: &str,
    work_dir: Option<&Path>,
    expected_outputs: &[String],
    env: Option<EnvironmentMap>,
) -> Result<AgentResult, PipelineError> {
    // Upload work-dir contents when remote mode is enabled and a work-dir is set.
    // Even an empty work-dir must be uploaded so the remote server has a valid
    // working_directory path. Without this, the local path gets sent to the
    // remote server which rejects it (path doesn't exist there).
    let bundle_handle = if remote {
        if let Some(dir) = work_dir {
            let mut files = read_dir_to_memory(dir)?;
            // Ensure at least one file so the multipart upload creates a
            // remote temp directory even when the work-dir is empty.
            if files.is_empty() {
                files.push((String::from(".gitkeep"), Vec::new()));
            }
            {
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

    tracing::debug!(
        task_yaml_len = task_yaml.len(),
        prompt_len = prompt.len(),
        working_directory = ?payload.working_directory,
        has_env = payload.environment.is_some(),
        env_keys = ?payload.environment.as_ref().map(|e| e.keys().cloned().collect::<Vec<_>>()),
        "submitting task to shedul3r"
    );

    // Wrap execution in a block that always cleans up the bundle.
    let execution_result = async {
        // Use file-poll recovery when local and expected outputs are set.
        // shedul3r sometimes drops HTTP connections for long tasks, but the
        // agent still completes and writes output files. The SDK races the
        // HTTP call against a file poller.
        let first_expected = if remote {
            None
        } else {
            work_dir.and_then(|dir| expected_outputs.first().map(|name| dir.join(name)))
        };

        let result = if remote {
            client.submit_task_poll(&payload).await?
        } else if let Some(ref expected_path) = first_expected {
            client
                .submit_task_with_recovery(&payload, expected_path)
                .await?
        } else {
            client.submit_task(&payload).await?
        };

        tracing::debug!(
            success = result.success,
            output_len = result.output.len(),
            output_preview = %crate::utils::truncate_str(&result.output, 200),
            elapsed = ?result.elapsed,
            exit_code = ?result.exit_code,
            "task response received"
        );

        // Download expected outputs from remote bundle back to local work-dir.
        // Only download if the task succeeded — failed tasks have no output files.
        if result.success {
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
        } // if result.success (download gate)

        // Read expected output files into a map.
        let mut output_files = BTreeMap::new();
        if let Some(dir) = work_dir {
            for output_path in expected_outputs {
                let local_path = dir.join(output_path);
                if local_path.is_file() {
                    match crate::fs::read_to_string(&local_path) {
                        Ok(content) => {
                            let _ = output_files.insert(output_path.clone(), content);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "expected output file {} could not be read: {e}",
                                local_path.display()
                            );
                        }
                    }
                }
            }
        }

        Ok::<AgentResult, PipelineError>(AgentResult {
            success: result.success,
            output: result.output,
            output_files,
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

/// Format a `Duration` as a human-readable timeout string for task YAML.
pub fn format_duration(d: Duration) -> String {
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
