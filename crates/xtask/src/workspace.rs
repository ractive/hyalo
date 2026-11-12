//! Shared workspace-path helper.
//!
//! Previously lived in `ac_fidelity.rs`, which was deleted; the three quality
//! gates that use it have nothing to do with acceptance criteria.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Locate the workspace root: the nearest ancestor of `CARGO_MANIFEST_DIR`
/// (set by `cargo run`), else of the current directory, whose `Cargo.toml`
/// has a `[workspace]` table.
///
/// Fails when there is none (L-8): a gate run from the wrong directory used to
/// fall back to `.` and read the wrong tree, or an empty one, and the gates
/// that tolerate a missing path went green on it.
pub fn workspace_root() -> Result<PathBuf> {
    let start = match std::env::var_os("CARGO_MANIFEST_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().context("reading the current directory")?,
    };
    find_workspace_root(&start)
}

fn find_workspace_root(start: &Path) -> Result<PathBuf> {
    for dir in start.ancestors() {
        let candidate = dir.join("Cargo.toml");
        if !candidate.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&candidate)
            .with_context(|| format!("reading {}", candidate.display()))?;
        if has_workspace_table(&content) {
            return Ok(dir.to_path_buf());
        }
    }
    bail!(
        "no Cargo.toml with a [workspace] table at or above {} — run the gate from inside the \
         repository, or through `cargo run -p xtask` / with CARGO_MANIFEST_DIR set",
        start.display()
    )
}

/// `true` when `manifest` parses as TOML and holds a top-level `workspace`
/// table (a comment or a string mentioning `[workspace]` does not count).
fn has_workspace_table(manifest: &str) -> bool {
    manifest
        .parse::<toml::Table>()
        .is_ok_and(|table| table.get("workspace").is_some_and(toml::Value::is_table))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_or_string_is_not_a_workspace_table() {
        assert!(has_workspace_table("[workspace]\nmembers = []\n"));
        assert!(has_workspace_table("[workspace.dependencies]\nx = \"1\"\n"));
        assert!(!has_workspace_table(
            "# [workspace]\n[package]\nname = \"x\"\n"
        ));
        assert!(!has_workspace_table(
            "[package]\ndescription = \"[workspace]\"\n"
        ));
        assert!(!has_workspace_table("not toml ["));
    }

    #[test]
    fn walking_up_finds_the_workspace_and_a_stray_directory_fails() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        let nested = dir.path().join("crates/x");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        assert_eq!(find_workspace_root(&nested).unwrap(), dir.path());

        let stray = tempfile::tempdir().unwrap();
        // The temp directory may itself sit under a workspace on exotic
        // setups; only assert the error shape when nothing is found above.
        if let Err(error) = find_workspace_root(stray.path()) {
            assert!(error.to_string().contains("no Cargo.toml"));
        }
    }
}
