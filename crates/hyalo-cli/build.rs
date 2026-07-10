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

    // Cargo's default "rerun if any package file changes" is suppressed as
    // soon as we emit any rerun-if-changed, so re-declare crate sources
    // explicitly. Without this, edits under src/ would not refresh the
    // `+dirty` marker computed from `git status --porcelain`. Also watch
    // `.git/index` so staging changes (which also flip dirty state) refresh.
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=build.rs");
    let index = git_dir.join("index");
    if index.exists() {
        println!("cargo:rerun-if-changed={}", index.display());
    }

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

    // `--date=short --format=%cd` instead of `%cs`: `%cs` was only added in
    // git 2.25, and older gits (e.g. inside cross build containers) echo the
    // unknown specifier literally, baking "%cs" into --version output.
    let date = match run_git(&["show", "-s", "--date=short", "--format=%cd", "HEAD"]) {
        Some(s) if is_iso_date(&s) => s,
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

/// `YYYY-MM-DD` shape check — anything else (old-git literal `%cd`, locale
/// formats) degrades to the bare `hyalo <semver>` form instead of leaking
/// garbage into `--version`.
fn is_iso_date(s: &str) -> bool {
    s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8(output.stdout).ok()?;
    Some(s.trim().to_string())
}
