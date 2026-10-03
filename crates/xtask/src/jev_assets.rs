//! Reproducible Jev bundle and shared-reference distribution.
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn run(sync: bool) -> Result<bool> {
    let root = crate::workspace::workspace_root()?;
    let package = root.join("npm/jev");
    ensure!(
        package.join("node_modules/@typesafe-ai/sdk").is_dir(),
        "run `bun install --frozen-lockfile` in npm/jev first"
    );
    let temporary = tempfile::tempdir()?;
    let bundle = temporary.path().join("jev.mjs");
    let status = Command::new("bun")
        .args(["--no-install", "--no-env-file", "run", "build.ts"])
        .current_dir(&package)
        .env("HYALO_JEV_BUNDLE_OUT", &bundle)
        .status()
        .context("building Jev bundle; Bun 1.4.2 is required for development")?;
    ensure!(status.success(), "Jev bundle build failed");
    let script = fs::read(bundle)?;
    let reference = fs::read(root.join("plugins/hyalo/skills/hyalo-tidy/references/jev.md"))?;
    let mut valid = true;
    for (name, content) in [
        ("scripts/jev.mjs", &script),
        ("references/jev.md", &reference),
    ] {
        // The crate embeds these assets from `templates/jev/` alone (DEC-344);
        // the pi/codex template trees carry no copy, and their mirror gates
        // skip `hyalo-tidy/{scripts,references}` accordingly.
        for base in [
            "plugins/hyalo/skills/hyalo-tidy",
            "pi-package/skills/hyalo-tidy",
        ] {
            let path = root.join(base).join(name);
            if sync {
                fs::create_dir_all(path.parent().context("asset has no parent")?)?;
                fs::write(&path, content)?;
            }
            if fs::read(&path).ok().as_ref() != Some(content) {
                eprintln!("Jev resource drift: {}", path.display());
                valid = false;
            }
        }
        let path = root
            .join("crates/hyalo-cli/templates/jev")
            .join(name.rsplit('/').next().context("asset has no filename")?);
        if sync {
            fs::create_dir_all(path.parent().context("asset has no parent")?)?;
            fs::write(&path, content)?;
        }
        if fs::read(&path).ok().as_ref() != Some(content) {
            eprintln!("Jev resource drift: {}", path.display());
            valid = false;
        }
    }
    // The mirror gates skip every file under `hyalo-tidy/{scripts,references}`
    // (DEC-344), so this gate must also own the directory listings: a stray
    // file there would otherwise ship unchecked.
    let mut listings: Vec<(String, &[&str])> = Vec::new();
    for base in [
        "plugins/hyalo/skills/hyalo-tidy",
        "pi-package/skills/hyalo-tidy",
    ] {
        listings.push((format!("{base}/scripts"), &["jev.mjs"]));
        listings.push((format!("{base}/references"), &["jev.md"]));
    }
    for base in [
        "crates/hyalo-cli/templates/pi/skills/hyalo-tidy",
        "crates/hyalo-cli/templates/codex/skills/hyalo-tidy",
    ] {
        listings.push((format!("{base}/scripts"), &[]));
        listings.push((format!("{base}/references"), &[]));
    }
    listings.push((
        "crates/hyalo-cli/templates/jev".to_owned(),
        &["jev.md", "jev.mjs"],
    ));
    for (dir, expected) in &listings {
        for stray in unexpected_entries(&root.join(dir), expected)? {
            eprintln!(
                "Jev resource directory holds an unexpected entry: {}",
                stray.display()
            );
            valid = false;
        }
    }
    if valid {
        println!("Jev helper and reference match across all skill distributions");
    }
    Ok(valid)
}

/// Entries of `dir` whose names are not in `expected`; a missing directory has none.
fn unexpected_entries(dir: &Path, expected: &[&str]) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).with_context(|| format!("reading {}", dir.display())),
    };
    let mut stray = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("reading {}", dir.display()))?;
        let name = entry.file_name();
        if !expected.iter().any(|allowed| name == **allowed) {
            stray.push(entry.path());
        }
    }
    stray.sort();
    Ok(stray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_entries_flags_strays_and_tolerates_missing_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let scripts = dir.path().join("scripts");
        fs::create_dir(&scripts).expect("mkdir");
        fs::write(scripts.join("jev.mjs"), "bundle").expect("write");
        assert!(
            unexpected_entries(&scripts, &["jev.mjs"])
                .expect("list")
                .is_empty()
        );
        fs::write(scripts.join("extra.mjs"), "stray").expect("write");
        fs::create_dir(scripts.join("nested")).expect("mkdir");
        assert_eq!(
            unexpected_entries(&scripts, &["jev.mjs"]).expect("list"),
            vec![scripts.join("extra.mjs"), scripts.join("nested")]
        );
        assert_eq!(
            unexpected_entries(&scripts, &[]).expect("list").len(),
            3,
            "a directory that must not exist reports everything in it"
        );
        assert!(
            unexpected_entries(&dir.path().join("missing"), &[])
                .expect("list")
                .is_empty()
        );
    }
}
