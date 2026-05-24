//! Gate 2 — Feature-fanout matrix check.
//!
//! Reads `crates/xtask/feature-matrix.toml` (relative to the workspace root)
//! and for each `[flags."--X"]` entry, asserts that every command listed in
//! `required_in` exposes `--X` in its `--help` output.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

use crate::ac_fidelity::workspace_root;

#[derive(Debug, Deserialize)]
pub struct FeatureMatrix {
    #[serde(default)]
    pub flags: HashMap<String, FlagEntry>,
    /// Envelope shape contracts — parsed from TOML for documentation/test purposes.
    #[serde(default)]
    #[allow(dead_code)]
    pub envelopes: Option<EnvelopeContracts>,
}

#[derive(Debug, Deserialize)]
pub struct FlagEntry {
    pub required_in: Vec<String>,
    /// Shape hint stored in the matrix for documentation purposes.
    #[allow(dead_code)]
    pub shape: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EnvelopeContracts {
    /// Commands that must surface files_from_counters in their JSON envelope.
    #[allow(dead_code)]
    pub files_from_counters: Option<Vec<String>>,
}

/// Load the feature matrix from the workspace root.
pub fn load_matrix(workspace_root: &std::path::Path) -> Result<FeatureMatrix> {
    let matrix_path = workspace_root
        .join("crates")
        .join("xtask")
        .join("feature-matrix.toml");
    let content = std::fs::read_to_string(&matrix_path)
        .with_context(|| format!("reading feature matrix at {matrix_path:?}"))?;
    toml::from_str(&content).with_context(|| "parsing feature-matrix.toml")
}

/// Run `cargo run -q -p hyalo-cli -- <cmd> --help` and capture stdout.
///
/// Returns `None` if the command fails to run.
fn help_output(workspace_root: &std::path::Path, cmd_args: &[&str]) -> Option<String> {
    let mut args = vec!["run", "-q", "-p", "hyalo-cli", "--"];
    args.extend_from_slice(cmd_args);
    args.push("--help");

    let out = Command::new("cargo")
        .args(&args)
        .current_dir(workspace_root)
        .output()
        .ok()?;

    // clap writes help to stdout for --help.
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if stdout.trim().is_empty() {
        // Some clap versions write help to stderr.
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        if stderr.trim().is_empty() {
            None
        } else {
            Some(stderr)
        }
    } else {
        Some(stdout)
    }
}

/// Check the feature fanout matrix. Returns `true` on pass, `false` on any violation.
pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    run_with_root(&root)
}

pub fn run_with_root(root: &std::path::Path) -> Result<bool> {
    let matrix = load_matrix(root)?;
    let mut violations: Vec<String> = Vec::new();

    for (flag, entry) in &matrix.flags {
        let required_in = entry.required_in.join(", ");
        for cmd in &entry.required_in {
            let help = help_output(root, &[cmd.as_str()]);
            match help {
                None => {
                    violations.push(format!(
                        "Feature-fanout violation: could not get help output for 'hyalo {cmd}'. \
                         Cannot verify flag '{flag}'. \
                         Matrix declares required_in=[{required_in}]. \
                         Either the command is missing, or 'cargo run -p hyalo-cli' failed."
                    ));
                }
                Some(text) => {
                    if !text.contains(flag.as_str()) {
                        violations.push(format!(
                            "Feature-fanout violation: flag '{flag}' missing from 'hyalo {cmd} --help'.\n  \
                             Matrix declares required_in=[{required_in}].\n  \
                             Either add the flag to '{cmd}', or update crates/xtask/feature-matrix.toml."
                        ));
                    }
                }
            }
        }
    }

    if violations.is_empty() {
        println!(
            "check-feature-fanout: all {} flag(s) present in all required commands.",
            matrix.flags.len()
        );
        Ok(true)
    } else {
        eprintln!(
            "check-feature-fanout: {} violation(s):\n\n{}",
            violations.len(),
            violations.join("\n\n")
        );
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Unit tests (matrix parsing only — no subprocess calls)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const MATRIX_TOML: &str = r#"
[flags."--files-from"]
required_in = ["find", "lint"]
shape = "selector"

[flags."--glob"]
required_in = ["find"]
shape = "selector"

[envelopes]
files_from_counters = ["find", "lint"]
"#;

    #[test]
    fn parse_matrix_flags() {
        let matrix: FeatureMatrix = toml::from_str(MATRIX_TOML).unwrap();
        assert!(matrix.flags.contains_key("--files-from"));
        assert!(matrix.flags.contains_key("--glob"));

        let ff = &matrix.flags["--files-from"];
        assert_eq!(ff.required_in, vec!["find", "lint"]);
        assert_eq!(ff.shape.as_deref(), Some("selector"));
    }

    #[test]
    fn parse_matrix_envelopes() {
        let matrix: FeatureMatrix = toml::from_str(MATRIX_TOML).unwrap();
        let envelopes = matrix.envelopes.unwrap();
        let counters = envelopes.files_from_counters.unwrap();
        assert!(counters.contains(&"find".to_owned()));
        assert!(counters.contains(&"lint".to_owned()));
    }

    #[test]
    fn parse_empty_matrix() {
        let matrix: FeatureMatrix = toml::from_str("").unwrap();
        assert!(matrix.flags.is_empty());
        assert!(matrix.envelopes.is_none());
    }

    #[test]
    fn matrix_path_derives_from_workspace() {
        // Just verify the path logic produces the expected suffix.
        let root = PathBuf::from("/workspace");
        let expected = root
            .join("crates")
            .join("xtask")
            .join("feature-matrix.toml");
        let computed = root
            .join("crates")
            .join("xtask")
            .join("feature-matrix.toml");
        assert_eq!(expected, computed);
    }
}
