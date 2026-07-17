//! Gate — Bundled-skill self-conformance.
//!
//! RB-5 regression guard: every skill template hyalo ships
//! (`crates/hyalo-cli/templates/skill-*.md`) must itself pass the `skills`
//! conformance profile. A profile whose own bundled skill violates the profile
//! is an embarrassment class of bug, so this gate lints each template *as
//! installed* — placed at `.claude/skills/<name>/SKILL.md` inside a scratch
//! vault initialized with `hyalo init --profile skills` — and fails CI on any
//! error-severity finding.
//!
//! Each template is linted in its own scratch vault (keyed on the skill's
//! `name` frontmatter) so two templates that share a `name` (e.g. the `hyalo`
//! and `hyalo` pi variants) don't collide on the same directory.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ac_fidelity::workspace_root;

/// Run the gate: `Ok(true)` when every bundled skill passes, `Ok(false)` when
/// at least one violates the skills profile (details printed to stderr).
pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let templates_dir = root.join("crates").join("hyalo-cli").join("templates");
    let mut skill_files: Vec<PathBuf> = std::fs::read_dir(&templates_dir)
        .with_context(|| format!("reading templates dir {templates_dir:?}"))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("skill-") && n.ends_with(".md"))
        })
        .collect();
    skill_files.sort();

    if skill_files.is_empty() {
        eprintln!("check-bundled-skills: no skill-*.md templates found in {templates_dir:?}");
        return Ok(false);
    }

    let mut all_ok = true;
    let mut checked = 0usize;
    for file in &skill_files {
        let body = std::fs::read_to_string(file)
            .with_context(|| format!("reading skill template {file:?}"))?;
        let name = frontmatter_name(&body)
            .with_context(|| format!("skill template {file:?} has no `name:` frontmatter field"))?;

        let scratch = tempfile::tempdir().context("creating scratch vault")?;
        let vault = scratch.path();
        // Install the skills profile config (writes .hyalo.toml + [scan] include).
        init_skills_profile(&root, vault)?;
        // Place the template as installed: `.claude/skills/<name>/SKILL.md`.
        let skill_dir = vault.join(".claude").join("skills").join(&name);
        std::fs::create_dir_all(&skill_dir)
            .with_context(|| format!("creating skill dir {skill_dir:?}"))?;
        std::fs::write(skill_dir.join("SKILL.md"), &body)
            .context("writing SKILL.md into scratch vault")?;

        let rel = format!(".claude/skills/{name}/SKILL.md");
        let (ok, output) = lint_skill(&root, vault, &rel)?;
        checked += 1;
        if !ok {
            all_ok = false;
            eprintln!(
                "check-bundled-skills: {} violates the skills profile:\n{}",
                file.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                output.trim()
            );
        }
    }

    if all_ok {
        println!("check-bundled-skills: {checked} bundled skill(s) pass the skills profile");
    }
    Ok(all_ok)
}

/// Extract the `name:` scalar from a SKILL.md frontmatter block. Handles the
/// simple `name: value` form used by every bundled template.
fn frontmatter_name(body: &str) -> Option<String> {
    let mut lines = body.lines();
    // First non-empty line must open the frontmatter fence.
    if lines.next()?.trim() != "---" {
        return None;
    }
    for line in lines {
        let t = line.trim();
        if t == "---" {
            break;
        }
        if let Some(rest) = t.strip_prefix("name:") {
            let v = rest.trim().trim_matches(['"', '\'']).to_owned();
            if !v.is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Build a `cargo run` command for the `hyalo` CLI that executes *inside*
/// `vault` (so `init` writes `.hyalo.toml` there and never touches the repo's
/// own config), while still resolving the workspace via `--manifest-path`
/// (`cargo run` runs the target binary in the caller's CWD, which we set to the
/// scratch vault).
fn hyalo_in_vault(root: &Path, vault: &Path) -> Command {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-q",
        "--manifest-path",
        &root.join("Cargo.toml").to_string_lossy(),
        "-p",
        "hyalo-cli",
        "--",
    ])
    .current_dir(vault);
    cmd
}

/// Initialize the skills profile in `vault` via `hyalo init --profile skills`.
fn init_skills_profile(root: &Path, vault: &Path) -> Result<()> {
    let out = hyalo_in_vault(root, vault)
        .args(["--dir", ".", "init", "--profile", "skills"])
        .output()
        .context("running hyalo init --profile skills")?;
    if !out.status.success() {
        anyhow::bail!(
            "hyalo init --profile skills failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Lint a single skill file with the skills profile. Returns `(passed, output)`.
fn lint_skill(root: &Path, vault: &Path, rel: &str) -> Result<(bool, String)> {
    let out = hyalo_in_vault(root, vault)
        .args([
            "--dir",
            ".",
            "lint",
            "--profile",
            "skills",
            "--file",
            rel,
            "--format",
            "text",
        ])
        .output()
        .context("running hyalo lint --profile skills")?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok((out.status.success(), combined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_name_simple() {
        let body = "---\nname: skills\ndescription: x\n---\n# body\n";
        assert_eq!(frontmatter_name(body), Some("skills".to_owned()));
    }

    #[test]
    fn frontmatter_name_quoted() {
        let body = "---\nname: \"hyalo-tidy\"\n---\n";
        assert_eq!(frontmatter_name(body), Some("hyalo-tidy".to_owned()));
    }

    #[test]
    fn frontmatter_name_missing() {
        let body = "---\ndescription: x\n---\n";
        assert_eq!(frontmatter_name(body), None);
    }

    #[test]
    fn frontmatter_name_no_fence() {
        assert_eq!(frontmatter_name("name: skills\n"), None);
    }
}
