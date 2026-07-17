//! GitHub Actions workflow-command rendering for `hyalo lint`.
//!
//! When `hyalo lint --format github` runs inside a GitHub Actions job, each
//! violation is emitted as a [workflow command] so the finding renders as an
//! inline annotation on the PR diff — no `jq` glue (which would violate the
//! no-polyglot-tooling rule) required.
//!
//! Output shape, one line per violation:
//!
//! ```text
//! ::error file=<repo-root-relative path>,line=<line>,title=<RULE_ID>::<message>
//! ::warning file=...,line=...,title=...::...
//! ```
//!
//! followed by a single plain-text summary line so the job log stays readable:
//!
//! ```text
//! N errors, M warnings in K files
//! ```
//!
//! [workflow command]: https://docs.github.com/en/actions/using-workflows/workflow-commands-for-github-actions
//!
//! ## Path resolution
//!
//! GitHub resolves annotation `file=` paths against the **workspace root** (the
//! checked-out repo), not the hyalo vault dir. The lint payload carries
//! vault-relative paths, so the renderer prefixes each with `path_prefix` — the
//! vault dir expressed relative to the current working directory. CI is assumed
//! to run `hyalo lint` from the repository root; the README documents this.

use std::fmt::Write as _;

/// Render the extended lint JSON payload as GitHub Actions workflow commands
/// plus a trailing summary line.
///
/// `payload` is the `results` value produced by the lint command — the
/// serialized `ExtLintOutput` (or its `--fix` variant), i.e. an object with a
/// `files` array whose entries carry `rule_groups[].violations[]` (read-only
/// lint) or `remaining_groups[].violations[]` (fix mode — `fixed_groups` are
/// omitted from annotations since they're no longer problems), plus the
/// top-level `errors` / `warnings` / `files_with_violations` counters.
///
/// `path_prefix` is prepended (with a `/` separator, always forward-slash for
/// portability) to each vault-relative file path so annotations resolve against
/// the repo root. Pass an empty string when the vault dir *is* the CWD.
#[must_use]
pub fn render(payload: &serde_json::Value, path_prefix: &str) -> String {
    let mut out = String::new();
    let prefix = normalize_prefix(path_prefix);

    if let Some(files) = payload.get("files").and_then(|f| f.as_array()) {
        for file_entry in files {
            let file = file_entry
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            // `.hyalo.toml` view-lint findings (see `validate_views`) are reported
            // as the literal filename `.hyalo.toml`, which always lives at the
            // config root (repo root in the common case where CI runs from
            // there) — never inside the vault dir. Every other entry is
            // vault-relative and needs the prefix; this one must not get it, or
            // the annotation points at a path that doesn't exist.
            let full_path = if file == ".hyalo.toml" {
                file.to_owned()
            } else {
                join_path(&prefix, file)
            };

            // New shape (read-only lint): violations grouped by rule under
            // `rule_groups`. Fix-mode (`--fix`/`--fix --dry-run`) instead groups
            // *unresolved* violations under `remaining_groups` — same per-group
            // shape, different key — with `fixed_groups` for violations already
            // resolved (nothing to annotate there since the PR diff no longer
            // shows them as problems).
            let groups_key = if file_entry.get("rule_groups").is_some() {
                "rule_groups"
            } else {
                "remaining_groups"
            };
            if let Some(rule_groups) = file_entry.get(groups_key).and_then(|rg| rg.as_array()) {
                for group in rule_groups {
                    let rule = group
                        .get("rule")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    let severity = group
                        .get("severity")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("warn");
                    if let Some(violations) = group.get("violations").and_then(|v| v.as_array()) {
                        for v in violations {
                            // line 0 (or absent) means "no specific line" — GitHub
                            // attaches such annotations to the top of the file.
                            let line = v
                                .get("line")
                                .and_then(serde_json::Value::as_u64)
                                .filter(|&l| l > 0);
                            let message = v
                                .get("message")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("");
                            write_command(&mut out, severity, &full_path, line, rule, message);
                        }
                    }
                }
            } else if let Some(violations) = file_entry.get("violations").and_then(|v| v.as_array())
            {
                // Legacy flat shape (e.g. `.hyalo.toml` view violations).
                for v in violations {
                    let severity = v
                        .get("severity")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("warn");
                    let message = v
                        .get("message")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    write_command(&mut out, severity, &full_path, None, "", message);
                }
            }
        }
    }

    // Trailing summary line, using the same field-name fallbacks the text
    // renderer uses so both read-only and `--fix` payloads are covered.
    let errors = payload
        .get("errors")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let warnings = payload
        .get("warnings")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let files_with_issues = payload
        .get("files_with_violations")
        .or_else(|| payload.get("files_with_issues"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let _ = write!(
        out,
        "{errors} {}, {warnings} {} in {files_with_issues} {}",
        plural(errors, "error", "errors"),
        plural(warnings, "warning", "warnings"),
        plural(files_with_issues, "file", "files"),
    );

    out
}

/// Pick the singular or plural word based on `n`.
fn plural<'a>(n: u64, singular: &'a str, plural: &'a str) -> &'a str {
    if n == 1 { singular } else { plural }
}

/// Write one `::error`/`::warning` workflow command line to `out`.
fn write_command(
    out: &mut String,
    severity: &str,
    file: &str,
    line: Option<u64>,
    rule: &str,
    message: &str,
) {
    let command = if severity == "error" {
        "error"
    } else {
        "warning"
    };
    let _ = write!(out, "::{command} file={}", escape_property(file));
    if let Some(l) = line {
        let _ = write!(out, ",line={l}");
    }
    if !rule.is_empty() {
        let _ = write!(out, ",title={}", escape_property(rule));
    }
    let _ = writeln!(out, "::{}", escape_data(message));
}

/// Escape a workflow-command *message* (the part after `::`).
///
/// Per the workflow-command spec: `%` → `%25`, `\r` → `%0D`, `\n` → `%0A`.
/// Order matters — `%` must be escaped first so the `%` in later replacements
/// is not double-escaped. Message text ultimately originates from linted file
/// content (body text, property values), so after the spec escapes are applied
/// any *other* raw control bytes (e.g. ANSI escape sequences) are stripped via
/// [`crate::output::sanitize_control_chars`] — the same defense other CLI
/// output paths use — to prevent terminal/log injection in the Actions job log.
/// This must run after the `\r`/`\n` replacement above, since
/// `sanitize_control_chars` would otherwise silently drop a raw `\r` instead of
/// letting it become `%0D`.
#[must_use]
pub fn escape_data(s: &str) -> String {
    let escaped = s
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A");
    crate::output::sanitize_control_chars(&escaped)
}

/// Escape a workflow-command *property* value (e.g. `file=`, `title=`).
///
/// Properties additionally escape `:` → `%3A` and `,` → `%2C` on top of the
/// data escapes, since those characters are the property delimiters.
#[must_use]
pub fn escape_property(s: &str) -> String {
    escape_data(s).replace(':', "%3A").replace(',', "%2C")
}

/// Normalize a path prefix: strip a leading `./`, convert backslashes to
/// forward slashes, and drop any trailing slash so [`join_path`] can add
/// exactly one separator.
fn normalize_prefix(prefix: &str) -> String {
    let p = prefix.replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.trim_end_matches('/').to_owned()
}

/// Join a normalized prefix with a vault-relative file path using a forward
/// slash. An empty prefix (vault dir == CWD) returns the file unchanged (minus
/// any `./`).
fn join_path(prefix: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    let file = file.strip_prefix("./").unwrap_or(&file);
    if prefix.is_empty() {
        file.to_owned()
    } else {
        format!("{prefix}/{file}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn escape_data_handles_percent_and_newlines() {
        // Percent must be escaped first so later replacements aren't double-escaped.
        assert_eq!(escape_data("100% done"), "100%25 done");
        assert_eq!(escape_data("line1\nline2"), "line1%0Aline2");
        assert_eq!(escape_data("a\r\nb"), "a%0D%0Ab");
        assert_eq!(escape_data("mix %\r\n%"), "mix %25%0D%0A%25");
        assert_eq!(escape_data("plain text"), "plain text");
    }

    #[test]
    fn escape_property_also_escapes_colon_and_comma() {
        assert_eq!(escape_property("a:b,c"), "a%3Ab%2Cc");
        // Colon/comma escapes compose with the data escapes.
        assert_eq!(escape_property("50%,x:y"), "50%25%2Cx%3Ay");
        // Windows-style path separators are left to the caller (join_path); a
        // bare colon in a drive-less path is still escaped.
        assert_eq!(escape_property("notes/a.md"), "notes/a.md");
    }

    #[test]
    fn normalize_prefix_strips_dot_slash_and_trailing_slash() {
        assert_eq!(normalize_prefix("./kb/"), "kb");
        assert_eq!(normalize_prefix("kb"), "kb");
        assert_eq!(normalize_prefix("sub\\kb/"), "sub/kb");
        assert_eq!(normalize_prefix("."), ".");
        assert_eq!(normalize_prefix(""), "");
    }

    #[test]
    fn join_path_prefixes_relative_paths() {
        assert_eq!(join_path("kb", "notes/a.md"), "kb/notes/a.md");
        assert_eq!(join_path("", "notes/a.md"), "notes/a.md");
        assert_eq!(join_path("", "./notes/a.md"), "notes/a.md");
        assert_eq!(join_path("sub/kb", "a.md"), "sub/kb/a.md");
        // Backslash file paths are normalized to forward slashes.
        assert_eq!(join_path("kb", "notes\\a.md"), "kb/notes/a.md");
    }

    #[test]
    fn render_emits_error_and_warning_commands_with_prefix() {
        let payload = json!({
            "files": [
                {
                    "file": "notes/a.md",
                    "rule_groups": [
                        {
                            "rule": "HYALO001",
                            "severity": "error",
                            "violations": [
                                {"line": 3, "message": "missing required property \"title\""}
                            ]
                        },
                        {
                            "rule": "SCHEMA",
                            "severity": "warn",
                            "violations": [
                                {"line": 1, "message": "no 'type' property"}
                            ]
                        }
                    ]
                }
            ],
            "errors": 1,
            "warnings": 1,
            "files_with_violations": 1
        });
        let out = render(&payload, "kb");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "::error file=kb/notes/a.md,line=3,title=HYALO001::missing required property \"title\""
        );
        assert_eq!(
            lines[1],
            "::warning file=kb/notes/a.md,line=1,title=SCHEMA::no 'type' property"
        );
        assert_eq!(lines[2], "1 error, 1 warning in 1 file");
    }

    #[test]
    fn render_clean_payload_is_summary_only() {
        let payload = json!({
            "files": [],
            "errors": 0,
            "warnings": 0,
            "files_with_violations": 0
        });
        let out = render(&payload, "");
        assert_eq!(out, "0 errors, 0 warnings in 0 files");
    }

    #[test]
    fn render_escapes_message_and_omits_zero_line() {
        let payload = json!({
            "files": [
                {
                    "file": "a.md",
                    "rule_groups": [
                        {
                            "rule": "MD013",
                            "severity": "error",
                            "violations": [
                                {"line": 0, "message": "line, with: comma%and\nnewline"}
                            ]
                        }
                    ]
                }
            ],
            "errors": 1,
            "warnings": 0,
            "files_with_violations": 1
        });
        let out = render(&payload, "");
        let first = out.lines().next().unwrap();
        // line=0 is omitted (annotations without a line attach to the file top).
        // Commas/colons/percent/newline in the *message* use data-escaping only.
        assert_eq!(
            first,
            "::error file=a.md,title=MD013::line, with: comma%25and%0Anewline"
        );
    }

    #[test]
    fn render_handles_legacy_flat_violations() {
        // `.hyalo.toml` view-lint findings are always reported at the config
        // root (repo root in the common case), never inside the vault dir, so
        // the vault-dir prefix must NOT apply here even though every other file
        // entry gets prefixed with "kb".
        let payload = json!({
            "files": [
                {
                    "file": ".hyalo.toml",
                    "violations": [
                        {"severity": "warn", "message": "view 'x' has no narrowing filter"}
                    ]
                }
            ],
            "errors": 0,
            "warnings": 1,
            "files_with_issues": 1
        });
        let out = render(&payload, "kb");
        let first = out.lines().next().unwrap();
        assert_eq!(
            first,
            "::warning file=.hyalo.toml::view 'x' has no narrowing filter"
        );
    }

    /// Fix-mode (`--fix`/`--fix --dry-run`) payloads group unresolved
    /// violations under `remaining_groups`, not `rule_groups`. The renderer
    /// must annotate those too — `fixed_groups` are intentionally skipped since
    /// they're no longer problems.
    #[test]
    fn render_handles_fix_mode_remaining_groups() {
        let payload = json!({
            "files": [
                {
                    "file": "notes/a.md",
                    "fixed_groups": [
                        {"rule": "MD047", "count": 1, "violations": []}
                    ],
                    "remaining_groups": [
                        {
                            "rule": "MD013",
                            "severity": "warn",
                            "violations": [
                                {"line": 5, "message": "Line length is 90 characters, expected no more than 80"}
                            ]
                        }
                    ],
                    "conflicts": []
                }
            ],
            "errors": 0,
            "warnings": 1,
            "files_with_violations": 1
        });
        let out = render(&payload, "");
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(
            lines[0],
            "::warning file=notes/a.md,line=5,title=MD013::Line length is 90 characters, expected no more than 80"
        );
        assert_eq!(lines[1], "0 errors, 1 warning in 1 file");
    }

    /// A fix-mode payload with zero remaining violations (everything fixed)
    /// produces no annotations at all — matches `render_clean_payload_is_summary_only`.
    #[test]
    fn render_fix_mode_all_fixed_is_summary_only() {
        let payload = json!({
            "files": [
                {
                    "file": "notes/a.md",
                    "fixed_groups": [
                        {"rule": "MD047", "count": 1, "violations": []}
                    ],
                    "remaining_groups": [],
                    "conflicts": []
                }
            ],
            "errors": 0,
            "warnings": 0,
            "files_with_violations": 0
        });
        let out = render(&payload, "");
        assert_eq!(out, "0 errors, 0 warnings in 0 files");
    }

    /// Raw control bytes (e.g. an ANSI escape sequence smuggled in via file
    /// content) are stripped from messages after the workflow-command escapes
    /// are applied, so they can't inject terminal/log control sequences into
    /// the Actions job log. `\r`/`\n`/`%` still round-trip through the spec's
    /// percent-escaping rather than being silently dropped.
    #[test]
    fn escape_data_strips_other_control_bytes_but_keeps_percent_escapes() {
        // ESC (0x1B) starts a raw ANSI sequence — must be stripped.
        let with_ansi = "\u{1b}[31mred\u{1b}[0m text";
        assert_eq!(escape_data(with_ansi), "[31mred[0m text");
        // \r and \n still become %0D / %0A, not stripped.
        assert_eq!(escape_data("a\r\nb"), "a%0D%0Ab");
        // A bell character (0x07) is stripped too.
        assert_eq!(escape_data("beep\u{7}beep"), "beepbeep");
    }
}
