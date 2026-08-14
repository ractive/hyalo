//! Shared workspace-path helper.
//!
//! Previously lived in `ac_fidelity.rs`, which was deleted; the three quality
//! gates that use it have nothing to do with acceptance criteria.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR` to find
/// the root `Cargo.toml` with `[workspace]`.
pub fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR for xtask is crates/xtask — go two levels up.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Walk up until we find a Cargo.toml containing [workspace].
    let mut dir = manifest_dir.as_path();
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate)
                .with_context(|| format!("reading {candidate:?}"))?;
            if content.contains("[workspace]") {
                return Ok(dir.to_path_buf());
            }
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    // Fallback: current directory.
    Ok(PathBuf::from("."))
}
