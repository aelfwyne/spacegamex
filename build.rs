//! Stamps the binary with its build time and git commit, so the game can
//! show exactly which build is running (title screen and top bar).

use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn main() {
    let stamp = run("date", &["+%Y-%m-%d %H:%M"]).unwrap_or_else(|| "unknown time".into());
    let mut git = run("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "nogit".into());
    // Uncommitted changes in the build get a "+".
    if run("git", &["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty()) {
        git.push('+');
    }
    println!("cargo:rustc-env=BUILD_STAMP={stamp}");
    println!("cargo:rustc-env=BUILD_GIT={git}");
    // Re-stamp whenever the code or the checked-out commit changes.
    for p in ["src", "Cargo.toml", "build.rs", ".git/HEAD", ".git/refs/heads", ".git/index"] {
        println!("cargo:rerun-if-changed={p}");
    }
}
