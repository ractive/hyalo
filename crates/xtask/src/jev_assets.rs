//! Reproducible Jev bundle and shared-reference distribution.
use anyhow::{Context, Result, ensure};
use std::{fs, process::Command};

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
        for base in [
            "plugins/hyalo/skills/hyalo-tidy",
            "pi-package/skills/hyalo-tidy",
            "crates/hyalo-cli/templates/codex/skills/hyalo-tidy",
            "crates/hyalo-cli/templates/pi/skills/hyalo-tidy",
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
    if valid {
        println!("Jev helper and reference match across all skill distributions");
    }
    Ok(valid)
}
