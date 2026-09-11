//! Capability gate derived from the real Clap tree and runtime behavior.

use anyhow::{Context, Result};
use hyalo_cli::describe_invocation;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};

use crate::artifact::{ArtifactArgs, build_hyalo};
use crate::workspace::workspace_root;

#[derive(Debug, Deserialize)]
pub struct FeatureMatrix {
    #[serde(default)]
    pub invocations: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub flags: BTreeMap<String, FlagEntry>,
    #[serde(default)]
    pub envelopes: Option<EnvelopeContracts>,
}

#[derive(Debug, Deserialize)]
pub struct FlagEntry {
    pub required_in: Vec<String>,
    pub shape: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EnvelopeContracts {
    pub files_from_counters: Option<Vec<String>>,
}

pub fn load_matrix(workspace_root: &Path) -> Result<FeatureMatrix> {
    let matrix_path = workspace_root.join("crates/xtask/feature-matrix.toml");
    let content = std::fs::read_to_string(&matrix_path)
        .with_context(|| format!("reading feature matrix at {matrix_path:?}"))?;
    toml::from_str(&content).context("parsing feature-matrix.toml")
}

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let matrix = load_matrix(&root)?;
    let mut violations = descriptor_violations(&matrix);
    let artifact = build_hyalo(&root, &ArtifactArgs::default(), false)?;
    violations.extend(runtime_violations(&artifact.executable)?);

    if violations.is_empty() {
        println!(
            "check-feature-fanout: PASS {} flags across {} Clap descriptors plus runtime nested/count/output/empty/preflight probes",
            matrix.flags.len(),
            matrix.invocations.len()
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

fn descriptor_violations(matrix: &FeatureMatrix) -> Vec<String> {
    let mut violations = Vec::new();
    let mut descriptors = BTreeMap::new();
    for (label, invocation) in &matrix.invocations {
        let mut argv = vec!["hyalo".to_owned()];
        argv.extend(invocation.iter().cloned());
        match describe_invocation(&argv) {
            Ok(descriptor) => {
                let actual_path = descriptor["command"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ");
                if actual_path != *label {
                    violations.push(format!(
                        "descriptor {label:?} resolved to canonical path {actual_path:?}"
                    ));
                }
                descriptors.insert(label.as_str(), descriptor);
            }
            Err(error) => violations.push(format!(
                "descriptor {label:?} could not parse {:?}: {error:#}",
                invocation
            )),
        }
    }
    for (flag, entry) in &matrix.flags {
        for label in &entry.required_in {
            let Some(descriptor) = descriptors.get(label.as_str()) else {
                violations.push(format!("flag {flag} names unknown invocation {label:?}"));
                continue;
            };
            let present = descriptor["options"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|option| {
                    option["long"]
                        .as_str()
                        .is_some_and(|long| format!("--{long}") == *flag)
                });
            if !present {
                violations.push(format!(
                    "Clap descriptor for {label:?} lacks required {flag} ({})",
                    entry.shape.as_deref().unwrap_or("unlabelled shape")
                ));
            }
        }
    }
    if let Some(commands) = matrix
        .envelopes
        .as_ref()
        .and_then(|contracts| contracts.files_from_counters.as_ref())
    {
        for label in commands {
            let Some(descriptor) = descriptors.get(label.as_str()) else {
                violations.push(format!(
                    "files_from_counters names unknown invocation {label:?}"
                ));
                continue;
            };
            if descriptor["capabilities"]["targets"] == "none" {
                violations.push(format!(
                    "files_from_counters names target-free invocation {label:?}"
                ));
            }
        }
    }
    violations.extend(capability_violations(&descriptors));
    violations
}

fn capability_violations(descriptors: &BTreeMap<&str, serde_json::Value>) -> Vec<String> {
    let expected = [
        (
            "find",
            "batch",
            true,
            false,
            "empty_result",
            &["text", "json"][..],
        ),
        (
            "read",
            "single",
            false,
            false,
            "cardinality_error",
            &["text", "json"][..],
        ),
        (
            "task read",
            "single",
            false,
            false,
            "cardinality_error",
            &["text", "json"][..],
        ),
        (
            "task toggle",
            "batch",
            false,
            true,
            "empty_result",
            &["text", "json"][..],
        ),
        (
            "views run",
            "batch",
            true,
            false,
            "empty_result",
            &["text", "json"][..],
        ),
        (
            "lint",
            "batch",
            true,
            false,
            "empty_result",
            &["text", "json", "github"][..],
        ),
    ];
    let mut violations = Vec::new();
    for (label, targets, count, writes, empty, modes) in expected {
        let Some(descriptor) = descriptors.get(label) else {
            violations.push(format!("missing capability descriptor {label:?}"));
            continue;
        };
        let capabilities = &descriptor["capabilities"];
        let actual_modes = capabilities["output_modes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        if capabilities["targets"] != targets
            || capabilities["count"] != count
            || capabilities["writes"] != writes
            || capabilities["empty_input"] != empty
            || actual_modes != modes
        {
            violations.push(format!(
                "capability mismatch for {label:?}: got {}",
                capabilities
            ));
        }
    }
    violations
}

fn runtime_violations(executable: &Path) -> Result<Vec<String>> {
    let tmp = tempfile::tempdir().context("creating feature-fanout runtime fixture")?;
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n")?;
    let note = tmp.path().join("note.md");
    let original = b"---\ntitle: Note\nstatus: draft\n---\n- [ ] task\n";
    std::fs::write(&note, original)?;
    std::fs::write(tmp.path().join("empty.txt"), "")?;
    let mut violations = Vec::new();

    for (label, args) in [
        (
            "nested task read",
            vec![
                "task",
                "read",
                "note.md",
                "--all",
                "--format",
                "json",
                "--no-hints",
            ],
        ),
        (
            "text output",
            vec!["read", "note.md", "--format", "text", "--no-hints"],
        ),
        (
            "json output",
            vec!["read", "note.md", "--format", "json", "--no-hints"],
        ),
        (
            "github output",
            vec!["lint", "note.md", "--format", "github", "--no-hints"],
        ),
    ] {
        let output = invoke(executable, tmp.path(), &args)?;
        if !output.status.success() {
            violations.push(format!(
                "runtime {label} failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let count = invoke(executable, tmp.path(), &["find", "--count", "--no-hints"])?;
    if !count.status.success() || count.stdout != b"1\n" {
        violations.push(format!(
            "runtime count expected 1, got status={} stdout={:?}",
            count.status,
            String::from_utf8_lossy(&count.stdout)
        ));
    }
    let empty_batch = invoke(
        executable,
        tmp.path(),
        &["find", "--files-from", "empty.txt", "--count", "--no-hints"],
    )?;
    if !empty_batch.status.success() || empty_batch.stdout != b"0\n" {
        violations.push("batch empty selection did not return a successful zero count".into());
    }
    let empty_single = invoke(
        executable,
        tmp.path(),
        &[
            "read",
            "--files-from",
            "empty.txt",
            "--format",
            "json",
            "--no-hints",
        ],
    )?;
    if empty_single.status.code() != Some(1)
        || !String::from_utf8_lossy(&empty_single.stderr)
            .contains("requires exactly one resolved file from --files-from")
    {
        violations
            .push("single-target empty selection did not return the cardinality error".into());
    }
    let rejected_mutation = invoke(
        executable,
        tmp.path(),
        &[
            "set",
            "note.md",
            "--property",
            "status=done",
            "--count",
            "--no-hints",
        ],
    )?;
    if rejected_mutation.status.code() != Some(1) || std::fs::read(&note)? != original {
        violations.push(
            "mutation output preflight did not reject --count before preserving source bytes"
                .into(),
        );
    }
    Ok(violations)
}

fn invoke(executable: &Path, cwd: &Path, args: &[&str]) -> Result<Output> {
    Command::new(executable)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("running hyalo {}", args.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MATRIX: &str = r#"
[invocations]
find = ["find"]
read = ["read", "note.md"]
"task read" = ["task", "read", "note.md", "--all"]
"task toggle" = ["task", "toggle", "note.md", "--all"]
"views run" = ["views", "run", "saved"]
lint = ["lint"]

[flags."--files-from"]
required_in = ["find", "read"]
shape = "selector"
"#;

    #[test]
    fn real_descriptors_satisfy_a_small_matrix() {
        let matrix: FeatureMatrix = toml::from_str(MATRIX).unwrap();
        assert!(descriptor_violations(&matrix).is_empty());
    }

    #[test]
    fn controlled_bad_matrix_is_rejected() {
        let matrix: FeatureMatrix =
            toml::from_str(&MATRIX.replace("--files-from", "--definitely-not-a-real-option"))
                .unwrap();
        assert!(!descriptor_violations(&matrix).is_empty());
    }
}
