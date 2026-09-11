//! Resolve runnable Cargo artifacts from Cargo's compiler-artifact messages.

use anyhow::{Context, Result, bail};
use clap::Args;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Args, Debug, Default)]
pub struct ArtifactArgs {
    /// Build this target triple. Runtime gates reject a non-host target honestly.
    #[arg(long, value_name = "TRIPLE")]
    pub target: Option<String>,
}

#[derive(Debug)]
pub struct CargoArtifact {
    pub executable: PathBuf,
    pub host: String,
    pub requested_target: Option<String>,
}

#[derive(Deserialize)]
struct CompilerArtifact {
    reason: String,
    target: CargoTarget,
    executable: Option<PathBuf>,
}

#[derive(Deserialize)]
struct CargoTarget {
    name: String,
    kind: Vec<String>,
}

pub fn build_hyalo(root: &Path, args: &ArtifactArgs, release: bool) -> Result<CargoArtifact> {
    let requested_target = args
        .target
        .clone()
        .or_else(|| std::env::var("CARGO_BUILD_TARGET").ok());
    let host = rustc_host(root)?;
    if requested_target
        .as_deref()
        .is_some_and(|target| target != host)
    {
        bail!(
            "runtime check unavailable: requested Cargo target {} differs from native host {host}; cross-compilation is not runtime evidence",
            requested_target.as_deref().unwrap_or_default()
        );
    }

    let mut command = Command::new("cargo");
    command.args([
        "build",
        "--message-format=json-render-diagnostics",
        "-p",
        "hyalo-cli",
        "--bin",
        "hyalo",
    ]);
    if release {
        command.arg("--release");
    }
    if let Some(target) = &requested_target {
        command.args(["--target", target]);
    }
    let output = command
        .current_dir(root)
        .output()
        .context("running Cargo to resolve the hyalo executable")?;
    if !output.status.success() {
        bail!(
            "Cargo could not build the hyalo executable ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let executable = parse_hyalo_executable(&output.stdout)?;
    if !executable.is_file() {
        bail!(
            "Cargo reported {}, but that executable does not exist",
            executable.display()
        );
    }
    let expected_name = format!("hyalo{}", std::env::consts::EXE_SUFFIX);
    if executable.file_name().and_then(|name| name.to_str()) != Some(expected_name.as_str()) {
        bail!(
            "Cargo reported unexpected binary name {}; expected {expected_name}",
            executable.display()
        );
    }
    Ok(CargoArtifact {
        executable,
        host,
        requested_target,
    })
}

fn rustc_host(root: &Path) -> Result<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .current_dir(root)
        .output()
        .context("querying rustc host triple")?;
    if !output.status.success() {
        bail!("rustc -vV failed with {}", output.status);
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .context("rustc -vV did not report a host triple")
}

fn parse_hyalo_executable(stdout: &[u8]) -> Result<PathBuf> {
    let mut executable = None;
    for line in stdout.split(|byte| *byte == b'\n') {
        let Ok(message) = serde_json::from_slice::<CompilerArtifact>(line) else {
            continue;
        };
        if message.reason == "compiler-artifact"
            && message.target.name == "hyalo"
            && message.target.kind.iter().any(|kind| kind == "bin")
            && message.executable.is_some()
        {
            executable = message.executable;
        }
    }
    executable.context("Cargo emitted no executable artifact for the hyalo binary")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_platform_specific_cargo_executable_verbatim() {
        for path in ["/tmp/custom/release/hyalo", r"C:\custom\release\hyalo.exe"] {
            let line = format!(
                "{{\"reason\":\"compiler-artifact\",\"target\":{{\"name\":\"hyalo\",\"kind\":[\"bin\"]}},\"executable\":{}}}\n",
                serde_json::to_string(path).unwrap()
            );
            assert_eq!(
                parse_hyalo_executable(line.as_bytes()).unwrap(),
                PathBuf::from(path)
            );
        }
    }

    #[test]
    fn ignores_non_binary_and_missing_artifacts() {
        let input = br#"{"reason":"compiler-artifact","target":{"name":"hyalo_cli","kind":["lib"]},"executable":null}
"#;
        assert!(parse_hyalo_executable(input).is_err());
    }
}
