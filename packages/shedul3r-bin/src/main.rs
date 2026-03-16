//! Stub crate for cargo-binstall. The real binary is distributed via GitHub Releases.
//! Install with: `cargo binstall shedul3r`

#[allow(clippy::print_stderr, clippy::disallowed_methods)] // reason: stub binary — user-facing error message and exit code
fn main() {
    eprintln!("This is a stub crate for cargo-binstall.");
    eprintln!("Install shedul3r with: cargo binstall shedul3r");
    eprintln!("Or download from: https://github.com/websmasher/pipelin3r/releases");
    std::process::exit(1);
}
