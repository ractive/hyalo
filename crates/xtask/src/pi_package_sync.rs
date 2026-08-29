//! Gate — pi-package / vendored-copy sync check.
//!
//! `crates/hyalo-cli/src/commands/init.rs` embeds the pi integration files
//! (`hyalo` and `hyalo-tidy` skills, the `hyalo.ts` extension, `package.json`)
//! via `include_str!`. Those files must live *inside* `crates/hyalo-cli/`
//! because `cargo package`/`cargo publish` build the verify tarball with only
//! the crate directory on disk — an `include_str!` reaching outside the crate
//! (as it used to, pointing at the top-level `pi-package/`) fails that build.
//! This broke the `hyalo-cli` 0.21.0 crates.io publish on 2026-08-29.
//!
//! The fix vendors byte-identical copies under
//! `crates/hyalo-cli/templates/pi/`. The top-level `pi-package/` directory
//! stays canonical — it is the exact layout `pi install
//! git:github.com/ractive/hyalo` consumes, and cannot move or gain a
//! symlink (Windows checkouts don't support them). This gate keeps the two
//! copies from drifting apart: it fails CI if any vendored file differs from
//! its `pi-package/` counterpart, or if `pi-package/{skills,extensions}` or
//! `pi-package/package.json` gains a file with no vendored counterpart.
//! Run `just sync-pi-package` to fix drift.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::workspace::workspace_root;

/// Run the gate: `Ok(true)` when the vendored copies match `pi-package/`
/// exactly, `Ok(false)` on any mismatch (details printed to stderr).
pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let pi_package = root.join("pi-package");
    let vendored = root
        .join("crates")
        .join("hyalo-cli")
        .join("templates")
        .join("pi");

    let mut pairs: Vec<(PathBuf, PathBuf)> = Vec::new();

    // skills/*/SKILL.md
    let skills_dir = pi_package.join("skills");
    match std::fs::read_dir(&skills_dir) {
        Ok(entries) => {
            for entry in entries.filter_map(|e| e.ok()) {
                let skill_dir = entry.path();
                let skill_md = skill_dir.join("SKILL.md");
                if skill_md.is_file() {
                    let name = skill_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    pairs.push((
                        skill_md,
                        vendored.join("skills").join(&name).join("SKILL.md"),
                    ));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("check-pi-package-sync: {skills_dir:?} not found, skipping");
        }
        Err(e) => return Err(e).with_context(|| format!("reading {skills_dir:?}")),
    }

    // extensions/*.ts
    let extensions_dir = pi_package.join("extensions");
    match std::fs::read_dir(&extensions_dir) {
        Ok(entries) => {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ts") {
                    let name = path.file_name().unwrap_or_default();
                    pairs.push((path.clone(), vendored.join("extensions").join(name)));
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("check-pi-package-sync: {extensions_dir:?} not found, skipping");
        }
        Err(e) => return Err(e).with_context(|| format!("reading {extensions_dir:?}")),
    }

    // package.json
    let package_json = pi_package.join("package.json");
    if package_json.is_file() {
        pairs.push((package_json, vendored.join("package.json")));
    }

    if pairs.is_empty() {
        eprintln!("check-pi-package-sync: no pi-package files found to check");
        return Ok(false);
    }

    let mut all_ok = true;
    let mut checked = 0usize;
    for (source, vendored_copy) in &pairs {
        checked += 1;
        if !vendored_copy.is_file() {
            all_ok = false;
            eprintln!(
                "check-pi-package-sync: {} has no vendored counterpart at {} — run `just sync-pi-package`",
                display_rel(&root, source),
                display_rel(&root, vendored_copy),
            );
            continue;
        }
        let source_bytes = std::fs::read(source).with_context(|| format!("reading {source:?}"))?;
        let vendored_bytes =
            std::fs::read(vendored_copy).with_context(|| format!("reading {vendored_copy:?}"))?;
        if source_bytes != vendored_bytes {
            all_ok = false;
            eprintln!(
                "check-pi-package-sync: {} differs from vendored copy {} — run `just sync-pi-package`",
                display_rel(&root, source),
                display_rel(&root, vendored_copy),
            );
        }
    }

    // Reverse direction: a vendored file whose canonical source was deleted
    // or renamed would otherwise go stale forever (review finding on
    // PR #289). Walk the vendored tree with the same file selection the
    // forward pass uses and demand a source for each.
    let expected: std::collections::HashSet<&PathBuf> = pairs
        .iter()
        .map(|(_, vendored_copy)| vendored_copy)
        .collect();
    for orphan in vendored_files(&vendored)? {
        if !expected.contains(&orphan) {
            all_ok = false;
            eprintln!(
                "check-pi-package-sync: vendored {} has no source under pi-package/ — delete it (and any include_str! of it)",
                display_rel(&root, &orphan),
            );
        }
    }

    if all_ok {
        println!("check-pi-package-sync: {checked} pi-package file(s) match their vendored copies");
    }
    Ok(all_ok)
}

/// The vendored files the gate is responsible for: `skills/*/SKILL.md`,
/// `extensions/*.ts`, and `package.json` under `templates/pi/` — the same
/// selection as the forward pass, so the two directions cannot disagree on
/// scope. A missing directory yields an empty list.
fn vendored_files(vendored: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    let read = |dir: &Path| -> Result<Vec<PathBuf>> {
        match std::fs::read_dir(dir) {
            Ok(entries) => Ok(entries.filter_map(|e| e.ok().map(|e| e.path())).collect()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e).with_context(|| format!("reading {dir:?}")),
        }
    };
    for skill_dir in read(&vendored.join("skills"))? {
        let skill_md = skill_dir.join("SKILL.md");
        if skill_md.is_file() {
            found.push(skill_md);
        }
    }
    for path in read(&vendored.join("extensions"))? {
        if path.extension().and_then(|e| e.to_str()) == Some("ts") {
            found.push(path);
        }
    }
    let package_json = vendored.join("package.json");
    if package_json.is_file() {
        found.push(package_json);
    }
    Ok(found)
}

fn display_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn display_rel_strips_root() {
        let root = Path::new("/repo");
        let path = Path::new("/repo/pi-package/package.json");
        assert_eq!(display_rel(root, path), "pi-package/package.json");
    }

    #[test]
    fn display_rel_falls_back_to_absolute_outside_root() {
        let root = Path::new("/repo");
        let path = Path::new("/elsewhere/file.txt");
        assert_eq!(display_rel(root, path), "/elsewhere/file.txt");
    }

    #[test]
    fn vendored_files_selects_same_scope_as_forward_pass() {
        let dir = tempfile::tempdir().expect("tempdir");
        let v = dir.path();
        fs::create_dir_all(v.join("skills/hyalo")).expect("mkdir");
        fs::create_dir_all(v.join("extensions")).expect("mkdir");
        fs::write(v.join("skills/hyalo/SKILL.md"), "s").expect("write");
        fs::write(v.join("skills/hyalo/notes.txt"), "ignored").expect("write");
        fs::write(v.join("extensions/hyalo.ts"), "t").expect("write");
        fs::write(v.join("extensions/README.md"), "ignored").expect("write");
        fs::write(v.join("package.json"), "{}").expect("write");
        let mut found = vendored_files(v).expect("walk");
        found.sort();
        let mut expected = vec![
            v.join("extensions/hyalo.ts"),
            v.join("package.json"),
            v.join("skills/hyalo/SKILL.md"),
        ];
        expected.sort();
        assert_eq!(found, expected);
        // A missing tree is empty, not an error.
        assert!(vendored_files(&v.join("nope")).expect("walk").is_empty());
    }

    #[test]
    fn detects_identical_and_diverging_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"same").expect("write a");
        fs::write(&b, b"same").expect("write b");
        assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());

        fs::write(&b, b"different").expect("write b");
        assert_ne!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    }
}
