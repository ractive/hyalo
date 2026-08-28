//! Config-level lint checks: saved views and the `[schema]` block itself, plus the `[lint] ignore` globset.
//!
//! Split out of the single 4,005-line `commands/lint.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `commands::lint::...` paths and behaviour are
//! unchanged.

use hyalo_mdlint::schema::{FileLintResult, Severity, VIOLATION_KIND_SCHEMA_MALFORMED, Violation};
use std::path::Path;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate `.hyalo.toml` view definitions and return a pseudo-file lint
/// result when at least one view looks suspicious.
///
/// Current checks:
/// - Views whose only narrowing mechanism is `fields = ["backlinks"]` or
///   similar — `fields` controls display columns, not filtering, so such a
///   view matches every file. The likely intent is `orphan = true`.
///
/// Returns `None` when there is nothing to report.
pub fn validate_views(dir: &Path) -> Option<FileLintResult> {
    // Keys that actually *narrow* the result set.
    const NARROWING_KEYS: &[&str] = &[
        "pattern",
        "regexp",
        "properties",
        "tag",
        "task",
        "sections",
        "file",
        "glob",
        "broken_links",
        "orphan",
        "dead_end",
        "title",
        "language",
    ];

    let toml_path = dir.join(".hyalo.toml");
    let contents = std::fs::read_to_string(&toml_path).ok()?;
    let table: toml::Table = toml::from_str(&contents).ok()?;
    let Some(toml::Value::Table(views_table)) = table.get("views") else {
        return None;
    };

    let mut violations: Vec<Violation> = Vec::new();
    for (name, value) in views_table {
        let Some(view_tbl) = value.as_table() else {
            continue;
        };

        let has_narrowing = view_tbl.iter().any(|(k, v)| {
            if !NARROWING_KEYS.contains(&k.as_str()) {
                return false;
            }
            // Treat `orphan = false` / `dead_end = false` as non-narrowing.
            if matches!(k.as_str(), "orphan" | "dead_end" | "broken_links") {
                return matches!(v, toml::Value::Boolean(true));
            }
            // List-typed narrowing keys with empty values don't narrow either.
            if let toml::Value::Array(a) = v {
                return !a.is_empty();
            }
            true
        });

        let has_fields = view_tbl.contains_key("fields");

        if !has_narrowing && has_fields {
            violations.push(Violation {
                severity: Severity::Warn,
                kind: None,
                message: format!(
                    "view '{name}' has no narrowing filter — `fields` controls display columns only, \
                     not filtering. Did you mean `orphan = true` or `dead_end = true`?"
                ),
            });
        } else if !has_narrowing {
            violations.push(Violation {
                severity: Severity::Warn,
                kind: None,
                message: format!(
                    "view '{name}' has no narrowing filter — add at least one of: \
                     tag, properties, task, orphan, dead_end, broken_links, glob, file, title"
                ),
            });
        }
    }

    if violations.is_empty() {
        None
    } else {
        Some(FileLintResult {
            file: ".hyalo.toml".to_string(),
            violations,
        })
    }
}

/// Validate the `[schema]` block in `.hyalo.toml` in isolation and return a
/// pseudo-file lint result when it fails to parse.
///
/// Review round finding 2: `parse_schema_from_toml` (`crates/hyalo-cli/src/config.rs`)
/// already detects a malformed `[schema]` block (an unknown key from
/// `deny_unknown_fields`, an invalid field combination, ...) and degrades
/// gracefully to "no schema validation" — but the only signal was a
/// `-q`-suppressible stderr warning. That meant one typo'd key in ANY type's
/// property block silently disabled schema enforcement for the WHOLE vault,
/// while `lint --strict` printed a clean "no issues" and exited 0 on a file
/// with a real violation the (silently disabled) schema would have caught.
///
/// This independently re-parses `.hyalo.toml` (mirroring [`validate_views`]'s
/// pattern of representing a config-level problem as a violation on a
/// `.hyalo.toml` pseudo-file) via the same
/// [`crate::config::try_parse_schema_from_toml`] used by the runtime config
/// loader, so the error text — including which key or value is wrong — is
/// identical to the one already sent to stderr; this just also makes it a
/// visible lint-result violation. `strict` controls severity directly (not
/// promoted later like the per-file `schema/*` kinds): `Error` under
/// `--strict` so `lint --strict` exits non-zero, `Warn` otherwise so
/// `--format json` output shows the problem in `results` without also
/// tripping the plain-`lint` exit code — a stricter stance than the
/// per-file warnings needs, since "validation is silently off" is worse than
/// any single file's violation it might have caught.
///
/// Returns `None` when the schema parses cleanly (including "no `[schema]`
/// block at all", which is valid — not every vault uses schema validation).
pub fn validate_schema_config(dir: &Path, strict: bool) -> Option<FileLintResult> {
    let toml_path = dir.join(".hyalo.toml");
    let contents = std::fs::read_to_string(&toml_path).ok()?;
    let table: toml::Table = toml::from_str(&contents).ok()?;
    match crate::config::try_parse_schema_from_toml(table.get("schema")) {
        Ok(_) => None,
        Err(message) => Some(FileLintResult {
            file: ".hyalo.toml".to_string(),
            violations: vec![Violation {
                severity: if strict {
                    Severity::Error
                } else {
                    Severity::Warn
                },
                kind: Some(VIOLATION_KIND_SCHEMA_MALFORMED),
                message,
            }],
        }),
    }
}

/// Compute lint counts from pre-indexed `IndexEntry` properties.
///
/// Used by `hyalo summary` to avoid re-reading files from disk.
/// The `index_entries` iterator yields `(rel_path, properties)` tuples.
/// Build a [`globset::GlobSet`] from `[lint] ignore` patterns for path-based
/// exclusion. Returns `None` when `patterns` is empty or every pattern failed
/// to compile (callers treat `None` as "exclude nothing"). Invalid patterns are
/// reported via [`crate::warn::warn`] so the behaviour matches `hyalo lint`,
/// which uses the same matching rules (`literal_separator(true)`,
/// `backslash_escape(true)`). Paths are matched against their vault-relative
/// form with `/` separators.
pub(crate) fn build_lint_ignore_globset(patterns: &[String]) -> Option<globset::GlobSet> {
    use globset::{GlobBuilder, GlobSetBuilder};
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GlobSetBuilder::new();
    let mut any_ok = false;
    for pat in patterns {
        match GlobBuilder::new(pat)
            .literal_separator(true)
            .backslash_escape(true)
            .build()
        {
            Ok(g) => {
                builder.add(g);
                any_ok = true;
            }
            Err(e) => {
                crate::warn::warn(format!("invalid [lint] ignore pattern {pat:?}: {e}"));
            }
        }
    }
    if !any_ok {
        return None;
    }
    builder.build().ok()
}

/// True when `rel_path` (vault-relative, any separator) is excluded by the
/// `[lint] ignore` glob set. `None` set means "exclude nothing".
pub(crate) fn is_lint_ignored(set: Option<&globset::GlobSet>, rel_path: &str) -> bool {
    set.is_some_and(|s| s.is_match(rel_path.replace('\\', "/")))
}
