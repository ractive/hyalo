//! Codex plugin assets are canonical; embedded copies keep cargo package self-contained.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn files(root: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut paths = BTreeSet::new();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.with_context(|| format!("walking {}", root.display()))?;
        ensure!(
            !entry.file_type().is_symlink(),
            "symlink in Codex package: {}",
            entry.path().display()
        );
        if entry.file_type().is_file() {
            paths.insert(entry.path().strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(paths)
}

pub fn run(sync: bool) -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let source = root.join("plugins/hyalo/skills");
    let embedded = root.join("crates/hyalo-cli/templates/codex/skills");
    let paths = files(&source)?;
    ensure!(!paths.is_empty(), "Codex package has no skills");
    for path in &paths {
        let bytes = fs::read(source.join(path))?;
        if sync {
            let target = embedded.join(path);
            fs::create_dir_all(target.parent().context("asset parent")?)?;
            fs::write(target, &bytes)?;
        }
        ensure!(
            fs::read(embedded.join(path)).ok().as_deref() == Some(bytes.as_slice()),
            "Codex asset drift at {}; run just sync-codex-package",
            path.display()
        );
        if path.file_name().is_some_and(|p| p == "openai.yaml") {
            let metadata: Value = serde_saphyr::from_str(std::str::from_utf8(&bytes)?)?;
            let name = path
                .components()
                .next()
                .context("skill name")?
                .as_os_str()
                .to_string_lossy();
            let interface = &metadata["interface"];
            for field in ["display_name", "short_description", "default_prompt"] {
                ensure!(
                    interface[field].as_str().is_some_and(|v| !v.is_empty()),
                    "{name}: missing {field}"
                );
            }
            let short = interface["short_description"]
                .as_str()
                .context("short description")?;
            ensure!(
                (25..=64).contains(&short.chars().count()),
                "{name}: description length"
            );
            ensure!(
                interface["default_prompt"]
                    .as_str()
                    .is_some_and(|s| s.contains(&format!("${name}"))),
                "{name}: prompt must mention skill"
            );
            ensure!(
                metadata["policy"]["allow_implicit_invocation"].as_bool()
                    == Some(name != "hyalo-tidy"),
                "{name}: invocation policy"
            );
        }
    }
    ensure!(
        paths == files(&embedded)?,
        "orphaned embedded Codex assets; remove stale copies explicitly"
    );
    let plugin: Value = serde_json::from_str(&fs::read_to_string(
        root.join("plugins/hyalo/.codex-plugin/plugin.json"),
    )?)?;
    ensure!(
        plugin["name"] == "hyalo" && plugin["skills"] == "./skills/",
        "invalid Hyalo plugin layout"
    );
    let marketplace: Value = serde_json::from_str(&fs::read_to_string(
        root.join(".agents/plugins/marketplace.json"),
    )?)?;
    ensure!(
        marketplace["plugins"]
            .as_array()
            .is_some_and(|entries| entries
                .iter()
                .any(|e| e["name"] == "hyalo" && e["source"]["path"] == "./plugins/hyalo")),
        "marketplace must point at plugins/hyalo"
    );
    println!(
        "check-codex-package: {} assets match; metadata and marketplace valid",
        paths.len()
    );
    Ok(true)
}
