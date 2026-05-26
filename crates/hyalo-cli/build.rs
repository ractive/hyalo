//! Build script that embeds git provenance (short sha + commit date) into the
//! compiled `hyalo` binary via the `HYALO_BUILD_VERSION_SHA` and
//! `HYALO_BUILD_DATE` env vars (consumed by `env!` in `cli/args.rs`).
//!
//! Failure modes (missing git, not a git repo, shell-out fails) degrade to
//! empty strings rather than panicking — a broken build script breaks every
//! consumer, including `cargo install` from a crates.io tarball.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=CARGO_HYALO_FORCE_NO_GIT");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT");
    println!("cargo:rerun-if-env-changed=GIT_COMMIT_DATE");

    let (sha, date) = resolve_provenance();

    println!("cargo:rustc-env=HYALO_BUILD_VERSION_SHA={sha}");
    println!("cargo:rustc-env=HYALO_BUILD_DATE={date}");
}

fn resolve_provenance() -> (String, String) {
    // Tarball / forced-off path: emit empty strings, skip git entirely.
    if std::env::var_os("CARGO_HYALO_FORCE_NO_GIT").is_some() {
        return (String::new(), String::new());
    }

    // CI/CD hermetic path: caller-supplied values, skip the shell-out.
    if let (Ok(sha), Ok(date)) = (
        std::env::var("GIT_COMMIT"),
        std::env::var("GIT_COMMIT_DATE"),
    ) && !sha.is_empty()
        && !date.is_empty()
    {
        return (sha, date);
    }

    // Resolve the .git directory so worktrees / submodules work.
    let git_dir = match run_git(&["rev-parse", "--git-dir"]) {
        Some(s) if !s.is_empty() => PathBuf::from(s),
        _ => return (String::new(), String::new()),
    };

    // Rerun directives — these paths change on commits / branch switches.
    let head = git_dir.join("HEAD");
    if head.exists() {
        println!("cargo:rerun-if-changed={}", head.display());
    }
    let refs = git_dir.join("refs");
    if refs.exists() {
        println!("cargo:rerun-if-changed={}", refs.display());
    }
    let packed_refs = git_dir.join("packed-refs");
    if packed_refs.exists() {
        println!("cargo:rerun-if-changed={}", packed_refs.display());
    }

    let sha = match run_git(&["rev-parse", "--short=12", "HEAD"]) {
        Some(s) if !s.is_empty() => s,
        _ => return (String::new(), String::new()),
    };

    let date = match run_git(&["show", "-s", "--format=%cs", "HEAD"]) {
        Some(s) if !s.is_empty() => s,
        _ => return (String::new(), String::new()),
    };

    // Append +dirty if there are uncommitted changes.
    let dirty = match run_git(&["status", "--porcelain"]) {
        Some(s) => !s.is_empty(),
        None => false,
    };

    let sha = if dirty { format!("{sha}+dirty") } else { sha };

    (sha, date)
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_string())
}
