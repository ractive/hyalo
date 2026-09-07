//! Freshness gate for the self-contained Pi API runtime.

use anyhow::{Context, Result, bail};
use std::{fs, process::Command};

pub fn run() -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let package = root.join("npm/hyalo");
    let script = package.join("scripts/build.mjs");
    let canonical_dir = root.join("pi-package/lib");
    if !script.is_file() || !package.join("node_modules/esbuild").is_dir() {
        bail!("npm API build dependencies are missing; run `npm ci` in npm/hyalo");
    }

    let temp = tempfile::tempdir().context("creating temporary Pi runtime build directory")?;
    let generated = temp.path().join("hyalo-api.js");
    let dist = temp.path().join("dist");
    let status = Command::new("node")
        .arg(&script)
        .current_dir(&package)
        .env("HYALO_DIST_DIR", &dist)
        .env("HYALO_PI_BUNDLE_OUT", &generated)
        .status()
        .context("running npm API build for Pi runtime freshness check")?;
    if !status.success() {
        bail!("npm API build failed with {status}");
    }

    for name in ["hyalo-api.js", "hyalo-api.d.ts"] {
        let expected_path = temp.path().join(name);
        let canonical = canonical_dir.join(name);
        let expected = fs::read(&expected_path).with_context(|| {
            format!(
                "reading freshly built Pi runtime {}",
                expected_path.display()
            )
        })?;
        let actual = match fs::read(&canonical) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("check-pi-runtime: missing {}", canonical.display());
                return Ok(false);
            }
            Err(error) => return Err(error).context("reading committed Pi runtime"),
        };
        if actual != expected {
            eprintln!(
                "check-pi-runtime: {} is stale; run `npm run build` in npm/hyalo and `just sync-pi-package`",
                canonical.display()
            );
            return Ok(false);
        }
    }
    println!("check-pi-runtime: committed runtime and declaration match a fresh isolated build");
    Ok(true)
}
