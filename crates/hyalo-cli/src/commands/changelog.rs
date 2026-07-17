#![allow(clippy::missing_errors_doc)]
//! `hyalo changelog` — Keep a Changelog 1.1.0 release generator.
//!
//! Two deterministic, LLM-free maintenance commands for a `CHANGELOG.md` that
//! follows the [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/)
//! grammar:
//!
//! - [`run_release`] rotates the accumulated `## [Unreleased]` content into a
//!   dated `## [X.Y.Z] - YYYY-MM-DD` section, re-creates an empty `[Unreleased]`
//!   above it, and appends a placeholder footer link-reference definition for
//!   the new version. It refuses to overwrite a version that already exists
//!   (idempotency guard). Defaults to `--dry-run`; writes only with `--apply`.
//! - [`run_add`] appends an entry under a category (`Added`, `Fixed`, …) in the
//!   `## [Unreleased]` section, creating the section/subsection as needed. It
//!   too defaults to `--dry-run`.
//!
//! Both operate purely on the file text (a line-oriented splice) — no schema, no
//! network — and preserve everything outside the region they touch. The date
//! defaults to today (`--date` overrides). Validate the result with
//! `hyalo lint --profile changelog`.

use anyhow::{Context, Result};
use std::path::Path;

use crate::commands::changelog_lint::CATEGORIES;
use crate::output::{CommandOutcome, Format, format_error};

/// Default changelog filename (vault-relative).
pub(crate) const CHANGELOG_FILE: &str = "CHANGELOG.md";

/// Resolve the effective `CHANGELOG.md` path for the changelog commands.
///
/// Precedence:
/// 1. `config_path` — the raw `[changelog] path` value from `.hyalo.toml`,
///    resolved *relative to `config_dir`* (the directory the config lives in).
///    This may point outside the vault `dir` (e.g. `../CHANGELOG.md` for a
///    repo-root changelog when the vault is a docs subdir).
/// 2. Otherwise `dir/CHANGELOG.md` (the historical default).
///
/// The resolved path is validated to stay within `config_dir`'s repo root: an
/// absolute `path`, or one that escapes above `config_dir` after normalization,
/// is rejected — mirroring the `mv` vault-escape guard so a config typo can't
/// make the tool write to an arbitrary location.
///
/// # Errors
/// Returns an error when `config_path` is absolute or traverses above
/// `config_dir`.
pub(crate) fn resolve_changelog_file(
    dir: &Path,
    config_dir: &Path,
    config_path: Option<&str>,
) -> Result<std::path::PathBuf> {
    let Some(raw) = config_path else {
        return Ok(dir.join(CHANGELOG_FILE));
    };
    let raw_norm = raw.replace('\\', "/");
    anyhow::ensure!(
        !Path::new(&raw_norm).is_absolute() && !raw_norm.starts_with('/'),
        "[changelog] path must be relative to the config directory, not absolute: {raw}"
    );
    // Join onto config_dir, then verify the lexically-normalized result does not
    // climb above config_dir (bounded `..` back into config_dir is fine; net
    // escape is not).
    let joined = config_dir.join(&raw_norm);
    let mut depth: i32 = 0;
    let mut escaped = false;
    for comp in Path::new(&raw_norm).components() {
        match comp {
            std::path::Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    escaped = true;
                    break;
                }
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            _ => {
                escaped = true;
                break;
            }
        }
    }
    anyhow::ensure!(
        !escaped,
        "[changelog] path escapes the config directory: {raw}"
    );
    Ok(joined)
}

/// A parsed view of a changelog sufficient for the release/add splices.
struct Changelog {
    /// Raw lines with both the trailing `\n` and any trailing `\r` stripped;
    /// the EOL style is tracked separately via `crlf` and restored on render.
    lines: Vec<String>,
    /// True when every line ending in the source was CRLF (round-tripped on
    /// write). A file with mixed or no CRLF endings renders as LF, so a
    /// same-EOL file never gets silently rewritten to a different style.
    crlf: bool,
    /// True when the source file ended with a trailing newline.
    trailing_newline: bool,
}

impl Changelog {
    fn parse(content: &str) -> Self {
        // Only treat the file as CRLF when *every* newline is preceded by
        // `\r` — a mixed-EOL file (or one with none) renders as LF so we
        // never normalize a file's line endings as an unintended side effect
        // of a `release`/`add` splice.
        let newline_count = content.matches('\n').count();
        let crlf_count = content.matches("\r\n").count();
        let crlf = newline_count > 0 && newline_count == crlf_count;
        let trailing_newline = content.ends_with('\n');
        let lines: Vec<String> = content
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
            .collect();
        // `split('\n')` on a trailing-newline string yields a final empty
        // element; drop it so line indices map cleanly.
        let mut lines = lines;
        if trailing_newline {
            lines.pop();
        }
        Self {
            lines,
            crlf,
            trailing_newline,
        }
    }

    fn render(&self) -> String {
        let sep = if self.crlf { "\r\n" } else { "\n" };
        let mut out = self.lines.join(sep);
        if self.trailing_newline {
            out.push_str(sep);
        }
        out
    }

    /// Index of the `## [<label>]` heading line (case-insensitive on label),
    /// or `None`.
    fn version_heading_index(&self, label: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| heading_label(l).is_some_and(|found| found.eq_ignore_ascii_case(label)))
    }

    /// Index of the first version heading line (any `## [...]`), or `None`.
    fn first_version_heading_index(&self) -> Option<usize> {
        self.lines.iter().position(|l| heading_label(l).is_some())
    }

    /// Whether a version with `label` already appears as a heading.
    fn has_version(&self, label: &str) -> bool {
        self.version_heading_index(label).is_some()
    }
}

/// If `line` is a `## [label] …` heading, return the bracketed label.
fn heading_label(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("## ")?;
    let rest = rest.strip_prefix('[')?;
    let close = rest.find(']')?;
    Some(&rest[..close])
}

/// Parse the label of a footer link-reference definition line `[label]: url`.
fn link_def_label(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix('[')?;
    let close = rest.find("]:")?;
    let label = &rest[..close];
    if label.is_empty() || label.contains('[') {
        return None;
    }
    Some(label)
}

/// Validate an `X.Y.Z` numeric semver core.
fn is_semver(s: &str) -> bool {
    let mut it = s.split('.');
    let ok =
        |p: Option<&str>| p.is_some_and(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_digit()));
    let a = ok(it.next());
    let b = ok(it.next());
    let c = ok(it.next());
    a && b && c && it.next().is_none()
}

/// `YYYY-MM-DD` shape check.
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
}

/// A short display name for the changelog file used in messages and JSON
/// output: the file name (e.g. `CHANGELOG.md`) so output stays stable
/// regardless of where the file physically lives.
fn changelog_display(path: &Path) -> String {
    path.file_name().map_or_else(
        || CHANGELOG_FILE.to_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// `hyalo changelog release <X.Y.Z> [--date YYYY-MM-DD] [--apply]`.
///
/// Rotates `## [Unreleased]` content into a new dated version section. Returns
/// the outcome and an optional exit-code override (1 when a dry run would
/// change the file, matching the `okf index` / `madr toc` CI convention).
pub fn run_release(
    changelog_file: &Path,
    version: &str,
    date: Option<&str>,
    apply: bool,
    active_profiles: &[String],
    format: Format,
) -> Result<(CommandOutcome, Option<i32>)> {
    let display = changelog_display(changelog_file);
    if !is_semver(version) {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("`{version}` is not a valid MAJOR.MINOR.PATCH version"),
                Some(version),
                Some("pass a semver like 1.2.0"),
                None,
            )),
            None,
        ));
    }
    let date_owned = match date {
        Some(d) => {
            if !is_iso_date(d) {
                return Ok((
                    CommandOutcome::UserError(format_error(
                        format,
                        &format!("`{d}` is not a valid YYYY-MM-DD date"),
                        Some(d),
                        Some("pass a date like 2026-07-17"),
                        None,
                    )),
                    None,
                ));
            }
            d.to_owned()
        }
        None => hyalo_core::schema::today_iso8601(),
    };

    let full = changelog_file.to_path_buf();
    if !full.is_file() {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("{display} not found"),
                Some(&display),
                Some("create a CHANGELOG.md with an `## [Unreleased]` section first"),
                None,
            )),
            None,
        ));
    }
    let old_content =
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {display}"))?;
    let mut cl = Changelog::parse(&old_content);

    // Idempotency guard: refuse to release an already-present version.
    if cl.has_version(version) {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("version `[{version}]` already exists in {display}"),
                Some(version),
                Some("bump to a new version, or remove the existing section first"),
                None,
            )),
            None,
        ));
    }

    let Some(unreleased_idx) = cl.version_heading_index("Unreleased") else {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("no `## [Unreleased]` section found in {display}"),
                None,
                Some("add an `## [Unreleased]` section to accumulate entries into"),
                None,
            )),
            None,
        ));
    };

    rotate_unreleased(&mut cl, unreleased_idx, version, &date_owned);
    upsert_release_link_ref(&mut cl, version);

    let new_content = cl.render();
    let changed = new_content != old_content;
    if apply && changed {
        hyalo_core::fs_util::atomic_write(&full, new_content.as_bytes())
            .with_context(|| format!("failed to write {display}"))?;
    }

    let payload = serde_json::json!({
        "command": "changelog release",
        "apply": apply,
        "file": display,
        "version": version,
        "date": date_owned,
        "changed": changed,
        "hint": crate::commands::profile_lint_hint("changelog", active_profiles, "validate the rotated changelog"),
    });
    let exit_override = if !apply && changed { Some(1) } else { None };
    Ok((
        CommandOutcome::success_with_total(payload.to_string(), u64::from(changed)),
        exit_override,
    ))
}

/// Rotate the `[Unreleased]` section at `unreleased_idx` into a new dated
/// version section, leaving a fresh empty `[Unreleased]` above it.
///
/// The content between the `## [Unreleased]` heading and the next `## ` heading
/// (or the footer link-ref block / EOF) is the accumulated body. We relabel the
/// existing heading to the new version and insert a fresh empty `[Unreleased]`
/// heading above it.
fn rotate_unreleased(cl: &mut Changelog, unreleased_idx: usize, version: &str, date: &str) {
    // Relabel the existing `## [Unreleased]` line to the new dated version.
    cl.lines[unreleased_idx] = format!("## [{version}] - {date}");

    // Insert a fresh `## [Unreleased]` block above it (heading + blank line).
    let fresh = vec!["## [Unreleased]".to_owned(), String::new()];
    for (offset, line) in fresh.into_iter().enumerate() {
        cl.lines.insert(unreleased_idx + offset, line);
    }
    // Ensure a blank line separates the new Unreleased block from the dated
    // section that now follows it (the dated heading sits at unreleased_idx+2).
    // `fresh` already added a trailing blank at unreleased_idx+1, so the layout
    // is: [Unreleased] / "" / [version] — correct.
}

/// Insert or refresh the footer link-reference definition for `version`.
///
/// Appended just after the `[Unreleased]` definition (if any) or at the end of
/// the footer link-ref block; a placeholder URL is used because the compare URL
/// is repo-specific. Idempotent per version (guarded by the release check).
fn upsert_release_link_ref(cl: &mut Changelog, version: &str) {
    let def = format!("[{version}]: TBD");

    // Find the footer link-ref block: contiguous trailing definition lines.
    // Prefer to insert right after the `[Unreleased]` definition so ordering
    // (newest first) is preserved.
    if let Some(idx) = cl
        .lines
        .iter()
        .position(|l| link_def_label(l).is_some_and(|lbl| lbl.eq_ignore_ascii_case("Unreleased")))
    {
        cl.lines.insert(idx + 1, def);
        return;
    }

    // No Unreleased def: append after the last link-ref definition, else EOF.
    let last_def = cl.lines.iter().rposition(|l| link_def_label(l).is_some());
    if let Some(idx) = last_def {
        cl.lines.insert(idx + 1, def);
    } else {
        // Append a blank separator + the definition at EOF.
        if cl.lines.last().is_some_and(|l| !l.trim().is_empty()) {
            cl.lines.push(String::new());
        }
        cl.lines.push(def);
    }
}

/// `hyalo changelog add --category <cat> --message "..." [--apply]`.
///
/// Appends `- <message>` under the `### <category>` subsection of
/// `## [Unreleased]`, creating the section / subsection when missing.
pub fn run_add(
    changelog_file: &Path,
    category: &str,
    message: &str,
    apply: bool,
    active_profiles: &[String],
    format: Format,
) -> Result<(CommandOutcome, Option<i32>)> {
    let display = changelog_display(changelog_file);
    let Some(canonical) = CATEGORIES.iter().find(|c| c.eq_ignore_ascii_case(category)) else {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                &format!("`{category}` is not a Keep a Changelog category"),
                Some(category),
                Some(&format!("use one of: {}", CATEGORIES.join(", "))),
                None,
            )),
            None,
        ));
    };
    if message.trim().is_empty() {
        return Ok((
            CommandOutcome::UserError(format_error(
                format,
                "entry message must not be empty",
                None,
                Some("pass --message \"...\""),
                None,
            )),
            None,
        ));
    }

    let full = changelog_file.to_path_buf();
    let old_content = if full.is_file() {
        std::fs::read_to_string(&full).with_context(|| format!("failed to read {display}"))?
    } else {
        // Fresh changelog skeleton.
        "# Changelog\n\n## [Unreleased]\n".to_owned()
    };
    let mut cl = Changelog::parse(&old_content);

    let unreleased_idx = if let Some(i) = cl.version_heading_index("Unreleased") {
        i
    } else {
        // Insert `## [Unreleased]` above the first version heading, or after the
        // title / at the top.
        let insert_at = cl.first_version_heading_index().unwrap_or(cl.lines.len());
        cl.lines.insert(insert_at, String::new());
        cl.lines.insert(insert_at, "## [Unreleased]".to_owned());
        insert_at
    };

    insert_entry(&mut cl, unreleased_idx, canonical, message.trim());

    let new_content = cl.render();
    let changed = new_content != old_content;
    if apply && changed {
        hyalo_core::fs_util::atomic_write(&full, new_content.as_bytes())
            .with_context(|| format!("failed to write {display}"))?;
    }

    let payload = serde_json::json!({
        "command": "changelog add",
        "apply": apply,
        "file": display,
        "category": canonical,
        "message": message.trim(),
        "changed": changed,
        "hint": crate::commands::profile_lint_hint("changelog", active_profiles, "validate the changelog"),
    });
    let exit_override = if !apply && changed { Some(1) } else { None };
    Ok((
        CommandOutcome::success_with_total(payload.to_string(), u64::from(changed)),
        exit_override,
    ))
}

/// Insert `- message` under `### <category>` within the `[Unreleased]` section
/// (heading at `unreleased_idx`), creating the subsection if it is missing.
fn insert_entry(cl: &mut Changelog, unreleased_idx: usize, category: &str, message: &str) {
    // Bound of the Unreleased section: the earliest of the next `## ` heading,
    // the footer link-reference block, or EOF. Keep-a-Changelog files end with
    // a block of `[label]: url` link-reference definitions; the `[Unreleased]`
    // section is frequently the *last* `## ` section, so without stopping at the
    // footer we would splice a new `### Category` *after* those definitions —
    // producing a non-conformant file that then fails its own lint (RB-4). We
    // therefore also stop at the first footer link-ref line so entries always
    // land inside the section body.
    let section_end = cl
        .lines
        .iter()
        .enumerate()
        .skip(unreleased_idx + 1)
        .find(|(_, l)| l.starts_with("## ") || link_def_label(l).is_some())
        .map_or(cl.lines.len(), |(i, _)| i);

    let cat_heading = format!("### {category}");
    // Locate an existing `### <category>` within the section.
    let cat_idx = cl.lines[unreleased_idx + 1..section_end]
        .iter()
        .position(|l| l.trim() == cat_heading)
        .map(|rel| unreleased_idx + 1 + rel);

    let entry = format!("- {message}");
    if let Some(cat_idx) = cat_idx {
        // Insert after the *complete* last bullet block under this category
        // (before the next heading or a trailing blank line). A bullet block
        // is its `- ` line plus every immediately-following hanging-indent
        // continuation line (non-blank, starts with whitespace) — Keep a
        // Changelog entries routinely wrap prose at ~80 columns, so a bullet's
        // last line is often a continuation, not the `- ` line itself. Blank
        // lines never continue a bullet and are never themselves treated as
        // the insertion point.
        let mut insert_at = cat_idx + 1;
        let mut i = cat_idx + 1;
        while i < section_end {
            let l = &cl.lines[i];
            if l.starts_with("### ") || l.starts_with("## ") {
                break;
            }
            if l.trim_start().starts_with("- ") {
                // Found a new bullet: advance past it and all of its
                // continuation lines (indented, non-blank lines that don't
                // start a new bullet/heading). A nested `- ` item inside a
                // bullet also matches here and is deliberately consumed as
                // its own bullet-with-continuations block — no depth
                // comparison is needed, because each `- ` line ratchets
                // `insert_at` past itself the same way, so the anchor still
                // ends after the last line of the whole list block.
                insert_at = i + 1;
                i += 1;
                while i < section_end {
                    let cont = &cl.lines[i];
                    let is_continuation = !cont.trim().is_empty()
                        && cont.starts_with(char::is_whitespace)
                        && !cont.trim_start().starts_with("- ");
                    if !is_continuation {
                        break;
                    }
                    insert_at = i + 1;
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
        cl.lines.insert(insert_at, entry);
    } else {
        // Create the subsection at the end of the Unreleased section.
        let mut block = Vec::new();
        // Ensure a blank line precedes the new subsection.
        if section_end > unreleased_idx + 1
            && cl
                .lines
                .get(section_end - 1)
                .is_some_and(|l| !l.trim().is_empty())
        {
            block.push(String::new());
        } else if section_end == unreleased_idx + 1 {
            // Nothing between heading and next section: add a blank first.
            block.push(String::new());
        }
        block.push(cat_heading);
        block.push(String::new());
        block.push(entry);
        block.push(String::new());
        for (offset, line) in block.into_iter().enumerate() {
            cl.lines.insert(section_end + offset, line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "\
# Changelog

## [Unreleased]

### Added

- Feature A.

## [1.0.0] - 2020-01-01

### Added

- Initial.

[Unreleased]: https://x/compare/v1.0.0...HEAD
[1.0.0]: https://x/tag/v1.0.0
";

    #[test]
    fn is_semver_cases() {
        assert!(is_semver("1.0.0"));
        assert!(is_semver("10.20.30"));
        assert!(!is_semver("1.0"));
        assert!(!is_semver("v1.0.0"));
        assert!(!is_semver("1.0.0-rc1"));
        assert!(!is_semver("1..0"));
    }

    #[test]
    fn resolve_changelog_file_defaults_to_vault_dir() {
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo/docs");
        let resolved = resolve_changelog_file(dir, config_dir, None).unwrap();
        assert_eq!(resolved, dir.join("CHANGELOG.md"));
    }

    #[test]
    fn resolve_changelog_file_resolves_relative_to_config_dir() {
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo");
        let resolved = resolve_changelog_file(dir, config_dir, Some("CHANGELOG.md")).unwrap();
        assert_eq!(resolved, config_dir.join("CHANGELOG.md"));
    }

    #[test]
    fn resolve_changelog_file_allows_bounded_parent_traversal() {
        // `..` that stays net-non-negative relative to config_dir is fine (it
        // never actually climbs above config_dir).
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo");
        let resolved =
            resolve_changelog_file(dir, config_dir, Some("sub/../CHANGELOG.md")).unwrap();
        assert_eq!(resolved, config_dir.join("sub/../CHANGELOG.md"));
    }

    #[test]
    fn resolve_changelog_file_rejects_absolute_path() {
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo");
        let err = resolve_changelog_file(dir, config_dir, Some("/etc/passwd")).unwrap_err();
        assert!(err.to_string().contains("must be relative"), "got: {err}");
    }

    #[test]
    fn resolve_changelog_file_rejects_net_escape() {
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo");
        let err = resolve_changelog_file(dir, config_dir, Some("../CHANGELOG.md")).unwrap_err();
        assert!(err.to_string().contains("escapes"), "got: {err}");
    }

    #[test]
    fn resolve_changelog_file_rejects_deeper_net_escape() {
        let dir = Path::new("/repo/docs");
        let config_dir = Path::new("/repo");
        let err =
            resolve_changelog_file(dir, config_dir, Some("a/../../CHANGELOG.md")).unwrap_err();
        assert!(err.to_string().contains("escapes"), "got: {err}");
    }

    #[test]
    fn release_rotates_unreleased() {
        let mut cl = Changelog::parse(BASE);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        rotate_unreleased(&mut cl, idx, "1.1.0", "2026-07-17");
        upsert_release_link_ref(&mut cl, "1.1.0");
        let out = cl.render();
        assert!(out.contains("## [1.1.0] - 2026-07-17"));
        assert!(out.contains("## [Unreleased]"));
        // The rotated section keeps the Feature A entry.
        let idx_new = out.find("## [1.1.0]").unwrap();
        assert!(out[idx_new..].contains("Feature A."));
        // A fresh Unreleased appears above the dated section.
        assert!(out.find("## [Unreleased]").unwrap() < idx_new);
        // Link ref inserted for the new version.
        assert!(out.contains("[1.1.0]: TBD"));
    }

    #[test]
    fn add_appends_under_existing_category() {
        let mut cl = Changelog::parse(BASE);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Added", "Feature B.");
        let out = cl.render();
        let unrel = out.find("## [Unreleased]").unwrap();
        let v1 = out.find("## [1.0.0]").unwrap();
        let seg = &out[unrel..v1];
        assert!(seg.contains("- Feature A."));
        assert!(seg.contains("- Feature B."));
        // Feature B is under Unreleased, not 1.0.0.
        assert!(!out[v1..].contains("Feature B."));
    }

    #[test]
    fn add_creates_missing_category() {
        let mut cl = Changelog::parse(BASE);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "A bug.");
        let out = cl.render();
        let unrel = out.find("## [Unreleased]").unwrap();
        let v1 = out.find("## [1.0.0]").unwrap();
        let seg = &out[unrel..v1];
        assert!(seg.contains("### Fixed"));
        assert!(seg.contains("- A bug."));
    }

    /// RB-4: a conformant KaC file whose `[Unreleased]` is the last `## `
    /// section (only the footer link-ref block follows) must get new entries
    /// *inside* the section, never after the link-ref definitions.
    const UNRELEASED_LAST: &str = "\
# Changelog

## [Unreleased]

### Added

- Existing feature.

[Unreleased]: https://x/compare/v1.0.0...HEAD
[1.0.0]: https://x/tag/v1.0.0
";

    #[test]
    fn add_new_category_lands_before_footer_link_refs() {
        let mut cl = Changelog::parse(UNRELEASED_LAST);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "A bug.");
        let out = cl.render();
        let fixed_pos = out.find("### Fixed").expect("Fixed subsection added");
        let footer_pos = out.find("[Unreleased]:").expect("footer preserved");
        assert!(
            fixed_pos < footer_pos,
            "new category must precede the footer link refs:\n{out}"
        );
        // The bug entry lands under Fixed, above the footer.
        let entry_pos = out.find("- A bug.").unwrap();
        assert!(entry_pos < footer_pos, "entry before footer:\n{out}");
        // Footer link refs are intact and not duplicated.
        assert_eq!(out.matches("[Unreleased]:").count(), 1);
        assert_eq!(out.matches("[1.0.0]:").count(), 1);
    }

    #[test]
    fn add_existing_category_lands_before_footer_link_refs() {
        let mut cl = Changelog::parse(UNRELEASED_LAST);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Added", "Second feature.");
        let out = cl.render();
        let entry_pos = out.find("- Second feature.").unwrap();
        let footer_pos = out.find("[Unreleased]:").unwrap();
        assert!(
            entry_pos < footer_pos,
            "appended entry stays inside the section:\n{out}"
        );
    }

    /// LB-5: a category whose last bullet is wrapped across several
    /// hanging-indent continuation lines, with a footer link-ref block at EOF
    /// (the RB-4 layout). `add` must insert the new entry after the *complete*
    /// wrapped bullet — not after its first line — leaving the continuation
    /// lines intact and directly following the `- ` line, and must not
    /// disturb the footer.
    const WRAPPED_LAST_BULLET: &str = "\
# Changelog

## [Unreleased]

### Fixed

- **`--format github` annotations are no longer truncated by the file cap**: the
  regression is now covered by a test that lints 60 files past the default
  50-file cap and asserts all 60 annotations are emitted.

[Unreleased]: https://x/compare/v1.0.0...HEAD
[1.0.0]: https://x/tag/v1.0.0
";

    /// A last bullet that *contains* a nested `- ` sub-list (each nested item
    /// with its own continuation line). The scan has no indentation-depth
    /// comparison — nested items are consumed as their own
    /// bullet-with-continuations blocks — and this test locks in that the
    /// anchor still ends after the whole list block, keeping the nested list
    /// attached to its parent bullet.
    const NESTED_LIST_LAST_BULLET: &str = "\
# Changelog

## [Unreleased]

### Fixed

- **Parent bullet with a nested list**: prose that
  wraps to a continuation line before the sub-list:
  - nested item one, whose text also
    wraps to a deeper continuation line
  - nested item two
  and a closing parent continuation line after the sub-list.

[Unreleased]: https://x/compare/v1.0.0...HEAD
[1.0.0]: https://x/tag/v1.0.0
";

    #[test]
    fn add_after_bullet_with_nested_list_lb5() {
        let mut cl = Changelog::parse(NESTED_LIST_LAST_BULLET);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "New entry.");
        let out = cl.render();

        let parent_pos = out.find("- **Parent bullet").expect("parent present");
        let closing_pos = out
            .find("  and a closing parent continuation line")
            .expect("closing continuation present");
        let new_entry_pos = out.find("- New entry.").expect("new entry present");
        // The whole parent block — nested list included — stays contiguous:
        // nothing is inserted between the parent's first line and its last
        // continuation line.
        assert!(
            !out[parent_pos..closing_pos].contains("- New entry."),
            "new entry must not split the parent bullet's block:\n{out}"
        );
        assert!(
            closing_pos < new_entry_pos,
            "new entry must follow the complete nested-list block:\n{out}"
        );
        let footer_pos = out.find("[Unreleased]:").expect("footer preserved");
        assert!(new_entry_pos < footer_pos, "new entry stays inside section");
        assert!(out.ends_with('\n') && !out.ends_with("\n\n"), "MD047 clean");
    }

    #[test]
    fn add_after_wrapped_last_bullet_lb5() {
        let mut cl = Changelog::parse(WRAPPED_LAST_BULLET);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "New entry.");
        let out = cl.render();

        // The old wrapped bullet is fully intact: its `- ` line is
        // immediately followed by both continuation lines, uninterrupted.
        let old_bullet_pos = out
            .find("- **`--format github` annotations")
            .expect("old bullet present");
        let cont1_pos = out
            .find("  regression is now covered")
            .expect("first continuation present");
        let cont2_pos = out
            .find("  50-file cap and asserts")
            .expect("second continuation present");
        let new_entry_pos = out.find("- New entry.").expect("new entry present");
        assert!(
            old_bullet_pos < cont1_pos && cont1_pos < cont2_pos,
            "old bullet's continuation lines stay in order directly after it:\n{out}"
        );
        // Nothing (in particular not the new entry) is stranded between the
        // bullet's first line and its continuation lines.
        let between = &out[old_bullet_pos..cont2_pos];
        assert!(
            !between.contains("- New entry."),
            "new entry must not split the wrapped bullet:\n{out}"
        );
        // The new entry lands after the complete old bullet block.
        assert!(
            cont2_pos < new_entry_pos,
            "new entry must follow the old bullet's last continuation line:\n{out}"
        );

        // Footer link refs untouched and still at EOF.
        let footer_pos = out.find("[Unreleased]:").expect("footer preserved");
        assert!(new_entry_pos < footer_pos, "new entry stays inside section");
        assert_eq!(out.matches("[Unreleased]:").count(), 1);
        assert_eq!(out.matches("[1.0.0]:").count(), 1);
        assert!(
            out.trim_end().ends_with("[1.0.0]: https://x/tag/v1.0.0"),
            "footer remains the last content:\n{out}"
        );

        // MD047: exactly one trailing newline, no doubled blank lines.
        assert!(out.ends_with('\n') && !out.ends_with("\n\n"), "MD047 clean");
    }

    /// LB-5: multiple wrapped bullets in the same category — including a
    /// blank line between two bullets, the same blank-line separation the
    /// non-wrapped tests already rely on (see `BASE`/`UNRELEASED_LAST`, which
    /// separate bullets/subsections with blank lines) — with the *last*
    /// bullet itself wrapped. The insertion point must land after the last
    /// bullet's final continuation line, and the blank line between the
    /// first two bullets must not be mistaken for the insertion point.
    const MULTI_WRAPPED_BULLETS: &str = "\
# Changelog

## [Unreleased]

### Fixed

- First bug, single line.

- **Second bug, wrapped**: the fix
  spans two continuation lines
  before the next entry.

[Unreleased]: https://x/compare/v1.0.0...HEAD
[1.0.0]: https://x/tag/v1.0.0
";

    #[test]
    fn add_after_multiple_wrapped_bullets_lb5() {
        let mut cl = Changelog::parse(MULTI_WRAPPED_BULLETS);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "Third bug.");
        let out = cl.render();

        let first_pos = out.find("- First bug, single line.").unwrap();
        let second_pos = out.find("- **Second bug, wrapped**").unwrap();
        let last_cont_pos = out.find("  before the next entry.").unwrap();
        let new_pos = out.find("- Third bug.").unwrap();
        let footer_pos = out.find("[Unreleased]:").unwrap();

        assert!(first_pos < second_pos, "bullets stay in original order");
        assert!(
            second_pos < last_cont_pos && last_cont_pos < new_pos,
            "new entry lands after the complete last wrapped bullet:\n{out}"
        );
        assert!(new_pos < footer_pos, "new entry stays inside the section");

        // The blank line between the two original bullets is preserved
        // exactly once, and no new blank line was introduced elsewhere.
        assert!(
            out.contains("- First bug, single line.\n\n- **Second bug, wrapped**"),
            "blank line between first and second bullet preserved:\n{out}"
        );

        assert!(out.ends_with('\n') && !out.ends_with("\n\n"), "MD047 clean");
    }

    #[test]
    fn add_output_is_md047_clean() {
        // The rendered file must end with exactly one trailing newline (MD047).
        let mut cl = Changelog::parse(UNRELEASED_LAST);
        let idx = cl.version_heading_index("Unreleased").unwrap();
        insert_entry(&mut cl, idx, "Fixed", "A bug.");
        let out = cl.render();
        assert!(out.ends_with('\n'), "file ends with a newline");
        assert!(!out.ends_with("\n\n"), "no trailing blank line (MD047)");
    }

    #[test]
    fn parse_render_round_trip_lf() {
        let cl = Changelog::parse(BASE);
        assert_eq!(cl.render(), BASE);
    }

    #[test]
    fn parse_render_round_trip_crlf() {
        let crlf = BASE.replace('\n', "\r\n");
        let cl = Changelog::parse(&crlf);
        assert!(cl.crlf);
        assert_eq!(cl.render(), crlf);
    }

    #[test]
    fn mixed_eol_does_not_render_as_crlf() {
        // A file with only one CRLF newline among many LF newlines must not be
        // detected as a CRLF file — that would silently rewrite every other
        // line ending on the next `release`/`add` splice (Copilot review
        // finding on PR #197).
        let mixed = "# Changelog\r\n\n## [Unreleased]\n";
        let cl = Changelog::parse(mixed);
        assert!(!cl.crlf, "mixed EOLs must not be treated as CRLF");
        // Rendering normalizes to LF (not a byte-identical round trip, but a
        // stable single style rather than reproducing the mix).
        assert!(!cl.render().contains("\r\n"));
    }

    #[test]
    fn no_newlines_does_not_render_as_crlf() {
        let cl = Changelog::parse("# Changelog");
        assert!(!cl.crlf);
    }

    #[test]
    fn heading_label_extraction() {
        assert_eq!(heading_label("## [Unreleased]"), Some("Unreleased"));
        assert_eq!(heading_label("## [1.2.0] - 2020-01-01"), Some("1.2.0"));
        assert_eq!(heading_label("## Nope"), None);
        assert_eq!(heading_label("### [x]"), None);
    }

    #[test]
    fn has_version_detects_existing() {
        let cl = Changelog::parse(BASE);
        assert!(cl.has_version("1.0.0"));
        assert!(!cl.has_version("2.0.0"));
    }
}
