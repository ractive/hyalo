//! Freshness gate for the self-contained Pi API runtime.

use anyhow::{Context, Result, bail};
use std::{fs, path::Path, process::Command};

pub fn run() -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let package = root.join("npm/hyalo");
    let script = package.join("scripts/build.mjs");
    let canonical_dir = root.join("pi-package/lib");
    if !script.is_file() || !package.join("node_modules/esbuild").is_dir() {
        bail!("npm API build dependencies are missing; run `npm ci` in npm/hyalo");
    }
    check_installed_tree(&package)?;

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

/// L-9: the bundle is compared byte for byte, so the dependency tree that
/// builds it has to be the locked one. A `node_modules` symlinked from another
/// checkout, or left over from an older lockfile, builds a bundle for other
/// dependency versions and the comparison then measures that tree.
fn check_installed_tree(package: &Path) -> Result<()> {
    let modules = package.join("node_modules");
    let meta =
        fs::symlink_metadata(&modules).with_context(|| format!("reading {}", modules.display()))?;
    if meta.file_type().is_symlink() {
        bail!(
            "{} is a symlink; run `npm ci` in npm/hyalo so the bundle is built from the locked dependencies",
            modules.display()
        );
    }
    let lock = read_lock(&package.join("package-lock.json"))?;
    let installed = read_lock(&modules.join(".package-lock.json")).with_context(|| {
        "node_modules has no .package-lock.json; run `npm ci` in npm/hyalo".to_owned()
    })?;
    let drift = lock_drift(&lock, &installed);
    if !drift.is_empty() {
        bail!(
            "node_modules does not match package-lock.json ({}); run `npm ci` in npm/hyalo",
            drift.join("; ")
        );
    }
    for name in ["esbuild", "typescript"] {
        if let Some(version) =
            installed["packages"][format!("node_modules/{name}")]["version"].as_str()
        {
            println!("check-pi-runtime: {name} {version}");
        }
    }
    Ok(())
}

fn read_lock(path: &Path) -> Result<serde_json::Value> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Differences between the lockfile and the lock npm recorded for the
/// installed tree: a package at another version, installed but not locked, or
/// locked and required (not optional, not a link) but not installed.
fn lock_drift(lock: &serde_json::Value, installed: &serde_json::Value) -> Vec<String> {
    let empty = serde_json::Map::new();
    let locked = lock["packages"].as_object().unwrap_or(&empty);
    let present = installed["packages"].as_object().unwrap_or(&empty);
    let mut drift = Vec::new();
    for (path, entry) in present {
        match locked.get(path) {
            None => drift.push(format!("{path} is installed but not locked")),
            Some(want) if want["version"] != entry["version"] => drift.push(format!(
                "{path} is {} but the lock says {}",
                entry["version"], want["version"]
            )),
            Some(_) => {}
        }
    }
    for (path, want) in locked {
        let required = path.starts_with("node_modules/")
            && !want["optional"].as_bool().unwrap_or(false)
            && !want["link"].as_bool().unwrap_or(false);
        if required && !present.contains_key(path) {
            drift.push(format!("{path} is locked but not installed"));
        }
    }
    drift.sort();
    drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matching_trees_have_no_drift_and_optional_packages_may_be_absent() {
        let lock = json!({"packages": {
            "": {}, "node_modules/a": {"version": "1.0.0"},
            "node_modules/b": {"version": "2.0.0", "optional": true}}});
        let installed = json!({"packages": {"node_modules/a": {"version": "1.0.0"}}});
        assert_eq!(lock_drift(&lock, &installed), Vec::<String>::new());
    }

    #[test]
    fn version_and_membership_drift_is_reported() {
        let lock = json!({"packages": {
            "node_modules/a": {"version": "1.0.0"}, "node_modules/c": {"version": "3.0.0"}}});
        let installed = json!({"packages": {
            "node_modules/a": {"version": "1.1.0"}, "node_modules/d": {"version": "4.0.0"}}});
        let drift = lock_drift(&lock, &installed);
        assert_eq!(drift.len(), 3, "{drift:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_node_modules_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("node_modules")).unwrap();
        let error = check_installed_tree(dir.path()).unwrap_err();
        assert!(error.to_string().contains("symlink"), "{error}");
    }
}
