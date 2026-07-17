//! MADR conformance lint rules (`hyalo lint --profile madr`).
//!
//! The advisory half of the MADR profile. The MADR-4 hard rules (typed
//! frontmatter, status pattern, required sections) are already covered by the
//! schema pass once the `madr` fragment is overlaid and the `adr` type is bound
//! to the decisions subtree. What lives here are the checks the schema layer
//! cannot express and that should stay *advisory* — so both rules below are
//! **warn**:
//!
//! - `MADR-SUPERSEDE-RESOLVE` — `status: superseded by ADR-0123` warns when no
//!   `0123-*.md` exists in the same ADR directory (a dangling supersede).
//! - `MADR-DUPLICATE-NUMBER` — an ADR filename `NNNN-slug.md` warns when another
//!   file in the same directory shares the `NNNN` prefix (colliding numbers).
//!
//! Rules dispatch only on files whose *effective* type is `adr` (via the path
//! binding or explicit frontmatter). Both touch the filesystem only to stat the
//! ADR file's own directory — no vault-wide walk, no network.

use std::path::Path;

/// A single MADR advisory finding, in the shape the lint pipeline converts into
/// an `InternalViolation`.
pub(crate) struct MadrFinding {
    pub(crate) rule_id: &'static str,
    /// 1-based line within the file, for display.
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// Rule IDs exposed by the MADR profile. Kept in one place so the catalog
/// (`lint-rules list`) and the runtime stay in lock-step; the parity test in
/// this module asserts every emitted id is listed here.
#[cfg(test)]
pub(crate) const MADR_RULE_IDS: &[&str] = &["MADR-SUPERSEDE-RESOLVE", "MADR-DUPLICATE-NUMBER"];

/// Run every enabled MADR rule against one ADR file.
///
/// * `rel_path` — vault-relative path (used to derive the ADR number).
/// * `full_path` — absolute path, used to resolve sibling ADR files.
/// * `effective_type` — the resolved type (explicit frontmatter or path
///   binding). Rules run only when this is `adr`.
/// * `status` — the frontmatter `status` value, if any.
/// * `is_enabled` — predicate deciding whether a given rule id runs (honors
///   `[lint.rules]` overrides and `--rule`/`--rule-prefix` filters).
pub(crate) fn run_madr_rules(
    rel_path: &str,
    full_path: &Path,
    effective_type: Option<&str>,
    status: Option<&str>,
    is_enabled: &dyn Fn(&str) -> bool,
) -> Vec<MadrFinding> {
    let mut out = Vec::new();

    // Only ADR files participate.
    if !matches!(effective_type, Some(t) if t.eq_ignore_ascii_case("adr")) {
        return out;
    }

    if is_enabled("MADR-SUPERSEDE-RESOLVE")
        && let Some(target) = status.and_then(parse_superseded_target)
        && !supersede_target_exists(full_path, target)
    {
        out.push(MadrFinding {
            rule_id: "MADR-SUPERSEDE-RESOLVE",
            line: 1,
            message: format!(
                "status references ADR-{target:04} but no `{target:04}-*.md` exists in this ADR directory (dangling supersede)"
            ),
        });
    }

    if is_enabled("MADR-DUPLICATE-NUMBER")
        && let Some(number) = adr_number(rel_path)
        && let Some(dup) = duplicate_number_sibling(full_path, number)
    {
        out.push(MadrFinding {
            rule_id: "MADR-DUPLICATE-NUMBER",
            line: 1,
            message: format!(
                "ADR number {number:04} is also used by `{dup}` in the same directory (numbers must be unique)"
            ),
        });
    }

    out
}

/// Parse `superseded by ADR-0123` (case-insensitive) into the target number.
/// Returns `None` for any other status value.
fn parse_superseded_target(status: &str) -> Option<u32> {
    let lower = status.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("superseded by adr-")?;
    // The pattern in the schema guarantees 4 digits, but be lenient here.
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// The 4-digit-padded number that a supersede target would resolve to (e.g.
/// `123` → `0123`). Kept as a helper so the `{target:04}` formatting stays in
/// one place for the existence check and the message.
///
/// Excludes `full_path` itself: a file whose own number happens to equal its
/// supersede target (a malformed but possible `status: superseded by
/// ADR-NNNN` self-reference on ADR `NNNN`) must not be treated as resolving
/// its own dangling reference.
fn supersede_target_exists(full_path: &Path, target: u32) -> bool {
    let Some(dir) = full_path.parent() else {
        return false;
    };
    let self_name = full_path.file_name().and_then(|n| n.to_str());
    number_prefix_exists(dir, target, self_name)
}

/// Extract the leading `NNNN` ADR number from a file's basename (`0007-x.md` →
/// `7`). Returns `None` when the basename does not start with a digit run
/// followed by `-`.
fn adr_number(rel_path: &str) -> Option<u32> {
    let name = Path::new(rel_path).file_name()?.to_str()?;
    let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    // Require a `-` separator after the number so `12.md` (no slug) doesn't count
    // and we match the `NNNN-slug.md` convention.
    if !name[digits.len()..].starts_with('-') {
        return None;
    }
    digits.parse().ok()
}

/// Does any file in `dir`, other than `exclude_name`, have a basename starting
/// with `<number>-` (numerically equal, ignoring zero-padding, e.g. both `7-`
/// and `0007-`)?
fn number_prefix_exists(dir: &Path, number: u32, exclude_name: Option<&str>) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if Some(name) == exclude_name {
            continue;
        }
        if !name.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        if let Some(n) = adr_number(name)
            && n == number
        {
            return true;
        }
    }
    false
}

/// Return the basename of a *different* sibling ADR file that shares
/// `full_path`'s ADR number, or `None` when the number is unique in the dir.
fn duplicate_number_sibling(full_path: &Path, number: u32) -> Option<String> {
    let dir = full_path.parent()?;
    let self_name = full_path.file_name().and_then(|n| n.to_str());
    let entries = std::fs::read_dir(dir).ok()?;
    let mut hit: Option<String> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if Some(name) == self_name {
            continue;
        }
        if !name.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        if let Some(n) = adr_number(name)
            && n == number
        {
            // Deterministic: report the lexicographically-smallest collision.
            match &hit {
                Some(existing) if existing.as_str() <= name => {}
                _ => hit = Some(name.to_owned()),
            }
        }
    }
    hit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_enabled(_: &str) -> bool {
        true
    }

    #[test]
    fn parse_superseded_variants() {
        assert_eq!(parse_superseded_target("superseded by ADR-0123"), Some(123));
        assert_eq!(parse_superseded_target("Superseded By ADR-0007"), Some(7));
        assert_eq!(parse_superseded_target("accepted"), None);
        assert_eq!(parse_superseded_target("superseded by ADR-xy"), None);
    }

    #[test]
    fn adr_number_extraction() {
        assert_eq!(adr_number("docs/decisions/0007-use-pg.md"), Some(7));
        assert_eq!(adr_number("0123-x.md"), Some(123));
        assert_eq!(adr_number("readme.md"), None);
        assert_eq!(adr_number("12.md"), None, "no slug separator");
    }

    #[test]
    fn non_adr_type_skips_all_rules() {
        let out = run_madr_rules(
            "docs/decisions/0007-x.md",
            Path::new("/vault/docs/decisions/0007-x.md"),
            Some("note"),
            Some("superseded by ADR-9999"),
            &always_enabled,
        );
        assert!(out.is_empty(), "non-adr files must not trigger MADR rules");
    }

    #[test]
    fn dangling_supersede_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("0007-x.md");
        std::fs::write(&file, "body").unwrap();
        let out = run_madr_rules(
            "docs/decisions/0007-x.md",
            &file,
            Some("adr"),
            Some("superseded by ADR-0123"),
            &always_enabled,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "MADR-SUPERSEDE-RESOLVE");
        assert!(out[0].message.contains("0123"));
    }

    #[test]
    fn resolving_supersede_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0007-x.md"), "body").unwrap();
        std::fs::write(dir.join("0123-target.md"), "body").unwrap();
        let out = run_madr_rules(
            "docs/decisions/0007-x.md",
            &dir.join("0007-x.md"),
            Some("adr"),
            Some("superseded by ADR-0123"),
            &always_enabled,
        );
        assert!(
            out.is_empty(),
            "existing target must not warn: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn duplicate_number_warns() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0007-first.md"), "body").unwrap();
        std::fs::write(dir.join("0007-second.md"), "body").unwrap();
        let out = run_madr_rules(
            "docs/decisions/0007-second.md",
            &dir.join("0007-second.md"),
            Some("adr"),
            Some("accepted"),
            &always_enabled,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "MADR-DUPLICATE-NUMBER");
        assert!(out[0].message.contains("0007-first.md"));
    }

    #[test]
    fn unique_number_is_clean() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0007-only.md"), "body").unwrap();
        let out = run_madr_rules(
            "docs/decisions/0007-only.md",
            &dir.join("0007-only.md"),
            Some("adr"),
            Some("accepted"),
            &always_enabled,
        );
        assert!(out.is_empty());
    }

    #[test]
    fn every_emitted_rule_id_is_registered() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("docs/decisions");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("0007-a.md"), "body").unwrap();
        std::fs::write(dir.join("0007-b.md"), "body").unwrap();
        let out = run_madr_rules(
            "docs/decisions/0007-b.md",
            &dir.join("0007-b.md"),
            Some("adr"),
            Some("superseded by ADR-4242"),
            &always_enabled,
        );
        for f in &out {
            assert!(
                MADR_RULE_IDS.contains(&f.rule_id),
                "unregistered rule id emitted: {}",
                f.rule_id
            );
        }
        // Both rules should have fired in this scenario.
        assert_eq!(out.len(), 2);
    }
}
