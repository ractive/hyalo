//! Generate and verify the TypeScript contracts derived from Rust models.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use walkdir::WalkDir;

use crate::workspace::workspace_root;

const GENERATED_DIR: &str = "npm/hyalo/src/generated";
const PUBLIC_TYPES: &str = "npm/hyalo/src/types.ts";

/// Regenerate all derived declarations in a temporary directory and compare
/// them byte-for-byte with the committed package sources.
pub fn check() -> Result<bool> {
    let root = workspace_root()?;
    let before = inventory(&root.join(GENERATED_DIR))?;
    let temp = tempfile::tempdir().context("creating temporary TypeScript export directory")?;
    let generated = temp.path().join("generated");
    generate_rust_types(&root, &generated)?;
    write_barrel(&generated)?;

    let expected = inventory(&generated)?;
    let actual = inventory(&root.join(GENERATED_DIR))?;
    let mut ok = compare_inventory(GENERATED_DIR, &expected, &actual);

    let expected_public = public_types_source();
    let actual_public = fs::read(root.join(PUBLIC_TYPES)).unwrap_or_default();
    if actual_public != expected_public.as_bytes() {
        eprintln!("check-ts-types: {PUBLIC_TYPES} is stale or missing");
        ok = false;
    }

    let after = inventory(&root.join(GENERATED_DIR))?;
    if before != after {
        bail!(
            "check-ts-types mutated committed generated sources; TS_RS_EXPORT_DIR was not isolated"
        );
    }

    if ok {
        println!(
            "check-ts-types: {} generated declaration(s) match committed LF-normalized sources",
            actual.len()
        );
    } else {
        eprintln!("check-ts-types: run `cargo run -p xtask -- generate-ts-types`");
    }
    Ok(ok)
}

/// Refresh the committed declarations and both generated barrels.
pub fn generate() -> Result<bool> {
    let root = workspace_root()?;
    let temp = tempfile::tempdir().context("creating temporary TypeScript export directory")?;
    let generated = temp.path().join("generated");
    generate_rust_types(&root, &generated)?;
    write_barrel(&generated)?;

    let destination = root.join(GENERATED_DIR);
    fs::create_dir_all(&destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    let expected = inventory(&generated)?;
    let actual = inventory(&destination)?;
    for stale in actual.keys().filter(|path| !expected.contains_key(*path)) {
        fs::remove_file(destination.join(stale))
            .with_context(|| format!("removing stale {GENERATED_DIR}/{}", stale.display()))?;
    }
    for (path, bytes) in &expected {
        let target = destination.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&target, bytes).with_context(|| format!("writing {}", target.display()))?;
    }
    fs::write(root.join(PUBLIC_TYPES), public_types_source())
        .with_context(|| format!("writing {PUBLIC_TYPES}"))?;
    println!(
        "generate-ts-types: wrote {} declaration(s) and {PUBLIC_TYPES}",
        expected.len()
    );
    Ok(true)
}

fn generate_rust_types(root: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output).with_context(|| format!("creating {}", output.display()))?;
    for package in ["hyalo-core", "hyalo-cli"] {
        let status = Command::new("cargo")
            .args([
                "test",
                "-q",
                "-p",
                package,
                "--lib",
                "export_bindings_",
                "--",
                "--test-threads=1",
            ])
            .current_dir(root)
            .env("TS_RS_EXPORT_DIR", output)
            .env("TS_RS_IMPORT_EXTENSION", "js")
            .env("TS_RS_LARGE_INT", "number")
            .status()
            .with_context(|| format!("running ts-rs export tests for {package}"))?;
        if !status.success() {
            bail!("ts-rs export tests for {package} failed with {status}");
        }
    }
    // The unformatted ts-rs exporter leaves spaces after field separators.
    // Normalize only freshly generated files, so the drift check still rejects
    // edits to committed declarations instead of silently normalizing them.
    for (path, bytes) in inventory(output)? {
        let source = String::from_utf8(bytes)
            .with_context(|| format!("generated {} is not UTF-8", path.display()))?;
        let mut normalized = String::with_capacity(source.len());
        for line in source.lines() {
            normalized.push_str(line.trim_end_matches([' ', '\t']));
            normalized.push('\n');
        }
        let target = output.join(path);
        fs::write(&target, normalized)
            .with_context(|| format!("normalizing {}", target.display()))?;
    }
    Ok(())
}

fn write_barrel(generated: &Path) -> Result<()> {
    let inventory = inventory(generated)?;
    let mut source =
        String::from("// Generated by `cargo run -p xtask -- generate-ts-types`; do not edit.\n\n");
    for path in inventory.keys() {
        if path.components().count() != 1 || path.extension() != Some(OsStr::new("ts")) {
            bail!("unexpected generated declaration path: {}", path.display());
        }
        let stem = path
            .file_stem()
            .and_then(OsStr::to_str)
            .context("generated declaration has a non-UTF-8 name")?;
        source.push_str(&format!("export type {{ {stem} }} from \"./{stem}.js\";\n"));
    }
    fs::write(generated.join("index.ts"), source).context("writing generated TypeScript barrel")
}

fn public_types_source() -> String {
    String::from(
        "// Generated by `cargo run -p xtask -- generate-ts-types`; do not edit.\n\n\
export type * from \"./generated/index.js\";\n\n\
import type { ConfigResult } from \"./generated/ConfigResult.js\";\n\
import type { FileObject } from \"./generated/FileObject.js\";\n\
import type { FindArgs } from \"./generated/FindArgs.js\";\n\
import type { GlobalArgs } from \"./generated/GlobalArgs.js\";\n\
import type { ReadArgs } from \"./generated/ReadArgs.js\";\n\
import type { ReadResult } from \"./generated/ReadResult.js\";\n\
import type { SummaryArgs } from \"./generated/SummaryArgs.js\";\n\
import type { VaultSummary } from \"./generated/VaultSummary.js\";\n\n\
type ApiGlobals = Partial<Omit<GlobalArgs, \"format\" | \"jq\" | \"count\" | \"hints\" | \"no_hints\">>;\n\
export type FindOptions = ApiGlobals & Partial<Omit<FindArgs, \"filenames_only\" | \"filenames0\" | \"strict\">>;\n\
export type ReadOptions = ApiGlobals & Partial<ReadArgs>;\n\
export type SummaryOptions = ApiGlobals & Partial<SummaryArgs>;\n\
export type ConfigOptions = ApiGlobals;\n\n\
export type FindResult = Array<FileObject>;\n\
export type SummaryResult = Omit<VaultSummary, \"dir\">;\n\
export type { ConfigResult, ReadResult };\n",
    )
}

fn inventory(dir: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    if !dir.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut files = BTreeMap::new();
    for entry in WalkDir::new(dir).sort_by_file_name() {
        let entry = entry.with_context(|| format!("walking {}", dir.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension() != Some(OsStr::new("ts")) {
            bail!(
                "unexpected non-TypeScript file in generated inventory: {}",
                path.display()
            );
        }
        let relative = path
            .strip_prefix(dir)
            .context("generated inventory path escaped its root")?
            .to_path_buf();
        files.insert(
            relative,
            fs::read(path).with_context(|| format!("reading {}", path.display()))?,
        );
    }
    Ok(files)
}

fn compare_inventory(
    label: &str,
    expected: &BTreeMap<PathBuf, Vec<u8>>,
    actual: &BTreeMap<PathBuf, Vec<u8>>,
) -> bool {
    let mut ok = true;
    for path in expected.keys() {
        match actual.get(path) {
            None => {
                eprintln!("check-ts-types: missing {label}/{}", path.display());
                ok = false;
            }
            Some(bytes) if bytes != &expected[path] => {
                eprintln!("check-ts-types: drift in {label}/{}", path.display());
                ok = false;
            }
            Some(_) => {}
        }
    }
    for path in actual.keys() {
        if !expected.contains_key(path) {
            eprintln!("check-ts-types: stale extra {label}/{}", path.display());
            ok = false;
        }
    }
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_comparison_rejects_missing_changed_and_extra_files() {
        let expected = BTreeMap::from([
            (PathBuf::from("A.ts"), b"a\n".to_vec()),
            (PathBuf::from("B.ts"), b"b\n".to_vec()),
        ]);
        assert!(compare_inventory("generated", &expected, &expected));
        assert!(!compare_inventory(
            "generated",
            &expected,
            &BTreeMap::from([
                (PathBuf::from("A.ts"), b"changed\n".to_vec()),
                (PathBuf::from("C.ts"), b"extra\n".to_vec()),
            ]),
        ));
    }
}
