//! Build-time metadata for the `pipeliner` CLI.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let git_sha =
        git_output(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| String::from("unknown"));
    let git_dirty = git_is_dirty().map_or_else(
        || String::from("unknown"),
        |dirty| {
            if dirty {
                "dirty".to_owned()
            } else {
                "clean".to_owned()
            }
        },
    );

    println!("cargo:rustc-env=PIPELIN3R_GIT_SHA={git_sha}");
    println!("cargo:rustc-env=PIPELIN3R_GIT_DIRTY={git_dirty}");
}

fn git_output(args: &[&str]) -> Option<String> {
    #[allow(
        clippy::disallowed_methods,
        reason = "build script needs git metadata embedded into the CLI binary"
    )]
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn git_is_dirty() -> Option<bool> {
    #[allow(
        clippy::disallowed_methods,
        reason = "build script needs git metadata embedded into the CLI binary"
    )]
    let status = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules=dirty", "HEAD", "--"])
        .status()
        .ok()?;
    Some(!status.success())
}
