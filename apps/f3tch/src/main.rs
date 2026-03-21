use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use clap::Parser;
use futures::stream::{self, StreamExt};
use serde::Serialize;

/// Simple non-crypto random u32 from system time + thread id.
fn rand_u32() -> u32 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut h);
    std::thread::current().id().hash(&mut h);
    h.finish() as u32
}

#[derive(Parser)]
#[command(name = "f3tch", about = "Fetch a specific file from many domains")]
struct Cli {
    /// File with one domain per line (e.g., Tranco top-1m.csv).
    #[arg(long)]
    domains: PathBuf,

    /// URL path to fetch from each domain (e.g., "/.well-known/security.txt").
    #[arg(long)]
    path: String,

    /// Output directory for downloaded files.
    #[arg(long, default_value = "raw-files")]
    output: PathBuf,

    /// Max domains to process (0 = all).
    #[arg(long, default_value = "100000")]
    limit: usize,

    /// Concurrent requests.
    #[arg(long, default_value = "5000")]
    concurrency: usize,

    /// Timeout per request in seconds.
    #[arg(long, default_value = "5")]
    timeout: u64,

    /// Only save files containing this substring (case-insensitive).
    /// Useful for additional content filtering.
    #[arg(long)]
    must_contain: Option<String>,

    /// Detect soft-404s by also fetching a random nonexistent path.
    /// If both return 200 with the same content, the real path is skipped.
    #[arg(long, default_value = "true")]
    detect_soft_404: bool,
}

#[derive(Serialize)]
struct FetchResult {
    domain: String,
    url: String,
    status: u16,
    size: usize,
    elapsed_ms: u64,
    file: Option<String>,
}

#[derive(Serialize)]
struct Manifest {
    total_domains: usize,
    fetched: usize,
    saved: usize,
    errors: usize,
    elapsed_secs: f64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Read domains.
    let raw = tokio::fs::read_to_string(&cli.domains).await.expect("cannot read domains file");
    let domains: Vec<String> = raw
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            // Handle CSV format (rank,domain)
            let domain = if let Some((_rank, dom)) = line.split_once(',') {
                dom.trim()
            } else {
                line
            };
            if domain.is_empty() {
                return None;
            }
            Some(domain.to_string())
        })
        .take(if cli.limit == 0 { usize::MAX } else { cli.limit })
        .collect();

    let total = domains.len();
    tracing::info!("Loaded {total} domains, fetching {}", cli.path);

    // Create output dir.
    tokio::fs::create_dir_all(&cli.output).await.expect("cannot create output dir");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(cli.timeout))
        .redirect(reqwest::redirect::Policy::limited(3))
        .danger_accept_invalid_certs(true)
        .build()
        .expect("cannot build HTTP client");

    let saved_count = Arc::new(AtomicUsize::new(0));
    let error_count = Arc::new(AtomicUsize::new(0));
    let fetched_count = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();

    let path_template = cli.path.clone();
    let output_dir = cli.output.clone();
    let must_contain = cli.must_contain.map(|s| s.to_lowercase());
    let detect_soft_404 = cli.detect_soft_404;

    let results: Vec<Option<FetchResult>> = stream::iter(domains.into_iter().enumerate())
        .map(|(idx, domain)| {
            let client = client.clone();
            let path = path_template.clone();
            let out_dir = output_dir.clone();
            let saved = Arc::clone(&saved_count);
            let errors = Arc::clone(&error_count);
            let fetched = Arc::clone(&fetched_count);
            let must_contain = must_contain.clone();

            async move {
                let url = format!("https://{domain}{path}");
                let req_start = Instant::now();

                let response = match client.get(&url).send().await {
                    Ok(r) => r,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        let done = fetched.fetch_add(1, Ordering::Relaxed) + 1;
                        if done % 5000 == 0 {
                            let s = saved.load(Ordering::Relaxed);
                            let e = errors.load(Ordering::Relaxed);
                            tracing::info!("{done}/{} fetched, {s} saved, {e} errors", total);
                        }
                        return None;
                    }
                };

                let status = response.status().as_u16();
                let elapsed_ms = req_start.elapsed().as_millis() as u64;

                if status != 200 {
                    errors.fetch_add(1, Ordering::Relaxed);
                    let done = fetched.fetch_add(1, Ordering::Relaxed) + 1;
                    if done % 5000 == 0 {
                        let s = saved.load(Ordering::Relaxed);
                        let e = errors.load(Ordering::Relaxed);
                        tracing::info!("{done}/{} fetched, {s} saved, {e} errors", total);
                    }
                    return None;
                }

                let body = match response.bytes().await {
                    Ok(b) => b,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        fetched.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                let size = body.len();

                // Skip empty or too large.
                if size == 0 || size > 1_000_000 {
                    fetched.fetch_add(1, Ordering::Relaxed);
                    return None;
                }

                // Reject binary files (images, archives, executables, etc.).
                if infer::get(&body).is_some() {
                    errors.fetch_add(1, Ordering::Relaxed);
                    fetched.fetch_add(1, Ordering::Relaxed);
                    return None;
                }

                // Reject non-UTF-8 content.
                let text = match std::str::from_utf8(&body) {
                    Ok(t) => t,
                    Err(_) => {
                        errors.fetch_add(1, Ordering::Relaxed);
                        fetched.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                };

                // Reject HTML, JSON, XML — servers returning 200 for error pages.
                let prefix = text.get(..500).unwrap_or(text).to_lowercase();
                let is_markup = prefix.contains("<!doctype")
                    || prefix.contains("<html")
                    || prefix.contains("<head")
                    || prefix.contains("<?xml");
                let is_json = text.trim_start().starts_with('{')
                    || text.trim_start().starts_with('[');
                if is_markup || is_json {
                    errors.fetch_add(1, Ordering::Relaxed);
                    fetched.fetch_add(1, Ordering::Relaxed);
                    return None;
                }

                // Detect soft-404: fetch a random path and check if it
                // returns the same content (server returns 200 for everything).
                if detect_soft_404 {
                    let probe_url = format!(
                        "https://{domain}/.well-known/{:x}{:x}.txt",
                        rand_u32(), rand_u32()
                    );
                    if let Ok(probe_resp) = client.get(&probe_url).send().await {
                        if probe_resp.status().as_u16() == 200 {
                            if let Ok(probe_body) = probe_resp.bytes().await {
                                if probe_body == body {
                                    // Same response for real and random path = soft-404.
                                    errors.fetch_add(1, Ordering::Relaxed);
                                    fetched.fetch_add(1, Ordering::Relaxed);
                                    return None;
                                }
                            }
                        }
                    }
                }

                // Optional content filter.
                if let Some(ref needle) = must_contain {
                    if !text.to_lowercase().contains(needle.as_str()) {
                        errors.fetch_add(1, Ordering::Relaxed);
                        fetched.fetch_add(1, Ordering::Relaxed);
                        return None;
                    }
                }

                // Save file.
                let file_num = saved.fetch_add(1, Ordering::Relaxed) + 1;
                let filename = format!("{file_num:06}.txt");
                let file_path = out_dir.join(&filename);
                if tokio::fs::write(&file_path, &body).await.is_err() {
                    return None;
                }

                let done = fetched.fetch_add(1, Ordering::Relaxed) + 1;
                if done % 5000 == 0 {
                    let s = saved.load(Ordering::Relaxed);
                    let e = errors.load(Ordering::Relaxed);
                    tracing::info!("{done}/{} fetched, {s} saved, {e} errors", total);
                }

                Some(FetchResult {
                    domain,
                    url,
                    status,
                    size,
                    elapsed_ms,
                    file: Some(filename),
                })
            }
        })
        .buffer_unordered(cli.concurrency)
        .collect()
        .await;

    let elapsed = start.elapsed();
    let saved_final = saved_count.load(Ordering::Relaxed);
    let error_final = error_count.load(Ordering::Relaxed);

    // Write manifest.
    let manifest = Manifest {
        total_domains: total,
        fetched: total,
        saved: saved_final,
        errors: error_final,
        elapsed_secs: elapsed.as_secs_f64(),
    };
    let manifest_path = cli.output.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    tokio::fs::write(&manifest_path, &manifest_json).await.ok();

    // Write results index (only successful fetches).
    let index: Vec<&FetchResult> = results.iter().flatten().collect();
    let index_path = cli.output.join("index.json");
    let index_json = serde_json::to_string_pretty(&index).unwrap_or_default();
    tokio::fs::write(&index_path, &index_json).await.ok();

    tracing::info!(
        "Done: {saved_final} saved, {error_final} errors out of {total} domains in {:.1}s",
        elapsed.as_secs_f64()
    );
}
