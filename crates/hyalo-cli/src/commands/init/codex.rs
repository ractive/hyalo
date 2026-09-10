//! Project Codex installation. Plugin and embedded assets are checked for drift by xtask.
use super::{
    Report, active_profiles_from_config, capture_installation, ensure_installation_dir,
    publish_captured_installation, relative_within, remove_captured_installation_artifact,
    remove_dir_if_empty,
};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexMode {
    None,
    Local,
    Plugin,
}

const MANAGED: &str = "<!-- hyalo:managed -->";
const YAML_MANAGED: &str = "# hyalo:managed";
const START: &str = "<!-- hyalo:start -->";
const END: &str = "<!-- hyalo:end -->";

struct Skill {
    name: &'static str,
    profile: Option<&'static str>,
    body: &'static str,
    metadata: &'static str,
}

macro_rules! skill {
    ($name:literal, $profile:expr) => {
        Skill {
            name: $name,
            profile: $profile,
            body: include_str!(concat!(
                "../../../templates/codex/skills/",
                $name,
                "/SKILL.md"
            )),
            metadata: include_str!(concat!(
                "../../../templates/codex/skills/",
                $name,
                "/agents/openai.yaml"
            )),
        }
    };
}

const SKILLS: &[Skill] = &[
    skill!("hyalo", None),
    skill!("hyalo-tidy", None),
    skill!("hyalo-okf", Some("okf")),
    skill!("hyalo-madr", Some("madr")),
    skill!("hyalo-skills", Some("skills")),
    skill!("hyalo-changelog", Some("changelog")),
];

// Check every existing component: even an in-root symlink may refer to user-owned
// guidance. Refuse it rather than overwriting the referent or following it on removal.
fn check_path(root: &Path, relative: &str) -> Result<()> {
    let mut path = root.to_path_buf();
    for part in Path::new(relative).components() {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("Codex artifact path is a symlink: {}", path.display());
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("checking {}", path.display())),
        }
    }
    Ok(())
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

fn owned(text: &str) -> bool {
    text.lines()
        .any(|line| line == MANAGED || line == YAML_MANAGED)
}

/// Return byte offsets so surrounding user text, including CRLF, stays unchanged.
fn managed_range(text: &str) -> Result<Option<std::ops::Range<usize>>> {
    let mut start = None;
    let mut range = None;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        match line.trim_end_matches(['\r', '\n']) {
            START => {
                if start.is_some() || range.is_some() {
                    bail!(
                        "AGENTS.md has duplicate or nested hyalo markers; repair them before running init/deinit"
                    );
                }
                start = Some(offset);
            }
            END => {
                let Some(begin) = start.take() else {
                    bail!("AGENTS.md has an unmatched hyalo end marker");
                };
                range = Some(begin..offset + line.len());
            }
            _ => {}
        }
        offset += line.len();
    }
    if start.is_some() {
        bail!("AGENTS.md has an unmatched hyalo start marker");
    }
    Ok(range)
}

pub(super) fn preflight_removal(root: &Path) -> Result<()> {
    check_path(root, "AGENTS.md")?;
    if let Some(text) = read_optional(&root.join("AGENTS.md"))? {
        managed_range(&text)?;
    }
    for skill in SKILLS {
        for suffix in ["SKILL.md", "agents/openai.yaml"] {
            check_path(root, &format!(".agents/skills/{}/{suffix}", skill.name))?;
        }
    }
    Ok(())
}

pub(super) fn preflight(root: &Path) -> Result<()> {
    preflight_removal(root)?;
    check_path(root, ".hyalo.toml")?;
    if let Some(text) = read_optional(&root.join(".hyalo.toml"))? {
        let _: toml::Value =
            toml::from_str(&text).context("invalid .hyalo.toml for Codex setup")?;
    }
    for skill in SKILLS {
        for suffix in ["SKILL.md", "agents/openai.yaml"] {
            let relative = format!(".agents/skills/{}/{suffix}", skill.name);
            if let Some(text) = read_optional(&root.join(&relative))?
                && !owned(&text)
            {
                bail!(
                    "Codex skill conflict: {relative} is not managed by hyalo; move or rename it before init"
                );
            }
        }
    }
    Ok(())
}

fn write_asset(root: &Path, relative: &str, text: &str, report: &mut Report) -> Result<()> {
    let path = root.join(relative);
    let source = capture_installation(root, &path)?;
    write_asset_from_capture(root, relative, text, source, report)
}

fn write_asset_from_capture(
    root: &Path,
    relative: &str,
    text: &str,
    source: Option<hyalo_core::rooted::CapturedInput>,
    report: &mut Report,
) -> Result<()> {
    write_asset_from_capture_with_before_publish(root, relative, text, source, report, || Ok(()))
}

fn write_asset_from_capture_with_before_publish(
    root: &Path,
    relative: &str,
    text: &str,
    source: Option<hyalo_core::rooted::CapturedInput>,
    report: &mut Report,
    before_publish: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let path = root.join(relative);
    let old = source
        .as_ref()
        .map(hyalo_core::rooted::CapturedInput::bytes)
        .transpose()?
        .map(String::from_utf8)
        .transpose()
        .context("installation artifact is not UTF-8")?;
    if old.as_deref() == Some(text) {
        report.push("unchanged", relative);
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        ensure_installation_dir(root, parent, report)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    before_publish()?;
    publish_captured_installation(root, &path, source, text.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    report.push(if old.is_some() { "updated" } else { "created" }, relative);
    Ok(())
}

pub(super) fn install(root: &Path, mode: CodexMode, report: &mut Report) -> Result<()> {
    let config_path = root.join(".hyalo.toml");
    let raw = fs::read_to_string(&config_path).context("reading Codex vault configuration")?;
    let config: toml::Value = toml::from_str(&raw).context("parsing Codex vault configuration")?;
    let configured_dir = config
        .get("dir")
        .and_then(toml::Value::as_str)
        .unwrap_or(".");
    if Path::new(configured_dir).is_absolute()
        || relative_within(root, &root.join(configured_dir)).is_none()
    {
        bail!("Codex vault dir must stay inside the project root: {configured_dir}");
    }
    let active = active_profiles_from_config(&config_path);
    // The retained config, not auto-detection, is authoritative on repeated init.
    report.dir = Some(configured_dir.to_owned());
    if mode == CodexMode::Plugin {
        remove_skills(root, report)?;
        report.notes.push("Codex plugin mode: install the Hyalo plugin separately; project skills were removed to avoid duplicate discovery. Use --codex-plugin on subsequent init runs.".to_owned());
    } else {
        for skill in SKILLS
            .iter()
            .filter(|s| s.profile.is_none_or(|p| active.iter().any(|a| a == p)))
        {
            write_asset(
                root,
                &format!(".agents/skills/{}/SKILL.md", skill.name),
                skill.body,
                report,
            )?;
            write_asset(
                root,
                &format!(".agents/skills/{}/agents/openai.yaml", skill.name),
                skill.metadata,
                report,
            )?;
        }
        report.notes.push("Codex project skills installed. If you also install the Hyalo plugin, rerun init --codex --codex-plugin to avoid duplicate skills.".to_owned());
    }
    let skill_location = if mode == CodexMode::Plugin {
        "Use the installed Hyalo plugin's skills."
    } else {
        "Use the hyalo and hyalo-tidy skills in .agents/skills/."
    };
    // JSON quoting prevents paths with backticks/newlines from turning into instructions.
    let quoted_dir = serde_json::to_string(configured_dir)?;
    let mut block = format!(
        "{START}\nHyalo manages the markdown vault at {quoted_dir}, relative to this project's .hyalo.toml.\n\
         {skill_location}\n\
         Use the hyalo CLI for knowledgebase search, reading, frontmatter, tags, tasks, and links.\n\
         Run hyalo config to inspect the effective vault; file arguments are vault-relative.\n\
         Run commands from the project root containing .hyalo.toml or inside the configured vault.\n\
         Other project subdirectories do not inherit this vault configuration; return to the project root first.\n\
         Use normal editing tools for body prose; run hyalo lint on changed markdown before handoff.\n\
         An audit request authorizes inspection; apply repairs only within the user's requested scope.\n"
    );
    for profile in &active {
        if SKILLS.iter().any(|s| s.profile == Some(profile.as_str())) {
            writeln!(
                block,
                "Active profile: {profile}; consult the hyalo-{profile} skill for its workflow."
            )?;
        }
    }
    writeln!(block, "{END}")?;
    let agents_source = capture_installation(root, &root.join("AGENTS.md"))?;
    let old = agents_source
        .as_ref()
        .map(hyalo_core::rooted::CapturedInput::bytes)
        .transpose()?
        .map(String::from_utf8)
        .transpose()
        .context("AGENTS.md is not UTF-8")?
        .unwrap_or_default();
    let updated = if let Some(range) = managed_range(&old)? {
        format!("{}{}{}", &old[..range.start], block, &old[range.end..])
    } else if old.is_empty() || old.ends_with('\n') {
        format!("{old}{block}")
    } else {
        // Prepend rather than changing the final newline of user-owned text.
        format!("{block}{old}")
    };
    write_asset_from_capture(root, "AGENTS.md", &updated, agents_source, report)?;
    if root.join("AGENTS.override.md").exists() {
        report.push_detail("warning", "AGENTS.override.md", "Codex prefers this file over AGENTS.md; merge the Hyalo guidance into it manually if wanted");
    }
    report.notes.push("Start a new Codex session to load the project guidance. Post-edit lint is skill guidance, not an automatic hook.".to_owned());
    Ok(())
}

fn remove_skills(root: &Path, report: &mut Report) -> Result<()> {
    for skill in SKILLS {
        for suffix in ["SKILL.md", "agents/openai.yaml"] {
            let relative = format!(".agents/skills/{}/{suffix}", skill.name);
            let path = root.join(&relative);
            if let Some(source) = capture_installation(root, &path)? {
                let text = String::from_utf8(source.bytes()?)
                    .with_context(|| format!("{relative} is not UTF-8"))?;
                if owned(&text) {
                    remove_captured_installation_artifact(root, &path, source)
                        .with_context(|| format!("removing {relative}"))?;
                    report.push("removed", &relative);
                } else {
                    report.push_detail("skipped", &relative, "not managed by hyalo");
                }
            }
        }
        for relative in [
            format!(".agents/skills/{}/agents", skill.name),
            format!(".agents/skills/{}", skill.name),
        ] {
            remove_dir_if_empty(&root.join(&relative), &relative, report)?;
        }
    }
    remove_dir_if_empty(&root.join(".agents/skills"), ".agents/skills/", report)?;
    remove_dir_if_empty(&root.join(".agents"), ".agents/", report)?;
    Ok(())
}

pub(super) fn remove(root: &Path, report: &mut Report) -> Result<()> {
    remove_skills(root, report)?;
    let agents_path = root.join("AGENTS.md");
    if let Some(source) = capture_installation(root, &agents_path)?
        && let text = String::from_utf8(source.bytes()?).context("AGENTS.md is not UTF-8")?
        && let Some(range) = managed_range(&text)?
    {
        let stripped = format!("{}{}", &text[..range.start], &text[range.end..]);
        if stripped.is_empty() {
            remove_captured_installation_artifact(root, &agents_path, source)
                .context("removing empty AGENTS.md")?;
            report.push("removed", "AGENTS.md");
        } else {
            write_asset_from_capture(root, "AGENTS.md", &stripped, Some(source), report)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_preserve_surrounding_bytes_and_reject_ambiguous_input() {
        let original = format!("user\r\n{START}\r\nmanaged\r\n{END}\r\ntail\r\n");
        let range = managed_range(&original).unwrap().unwrap();
        assert_eq!(
            format!("{}{}", &original[..range.start], &original[range.end..]),
            "user\r\ntail\r\n"
        );
        for text in [
            START.to_owned(),
            END.to_owned(),
            format!("{START}\n{START}\n{END}"),
            format!("{START}\n{END}\n{START}\n{END}"),
        ] {
            assert!(managed_range(&text).is_err());
        }
        assert!(
            managed_range("Mention <!-- hyalo:start --> in prose")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn codex_asset_publication_failure_retains_created_directory_effects() {
        let root = tempfile::tempdir().unwrap();
        let scope = super::super::resolve_scope(Some("."), root.path());
        let mut report = Report::new("init", &scope, Some(".".to_owned()));
        let relative = ".agents/skills/hyalo/agents/openai.yaml";
        let error = write_asset_from_capture_with_before_publish(
            root.path(),
            relative,
            "managed",
            None,
            &mut report,
            || anyhow::bail!("injected publication failure"),
        )
        .unwrap_err();

        assert!(error.to_string().contains("injected publication failure"));
        assert!(!root.path().join(relative).exists());
        let effects = report.effects();
        for directory in [
            ".agents",
            ".agents/skills",
            ".agents/skills/hyalo",
            ".agents/skills/hyalo/agents",
        ] {
            let expected = dunce::canonicalize(root.path().join(directory)).unwrap();
            assert!(
                effects.paths.iter().any(|effect| {
                    effect.state == crate::commands::apply::EffectState::Committed
                        && dunce::canonicalize(&effect.file).ok().as_deref()
                            == Some(expected.as_path())
                }),
                "missing directory effect for {directory}: {:?}",
                effects.paths
            );
        }
    }
}
