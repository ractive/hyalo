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

/// GitHub registers at most this many annotations of each type (`error`,
/// `warning`, `notice`) per workflow *step*. Beyond the cap, further
/// annotations of that type are silently dropped from the PR check — hyalo
/// still emits every workflow command, but GitHub will not surface them.
/// When a run exceeds this for errors or warnings we append a truncation
/// `::notice::` so the reader knows findings were hidden (iter-186).
const GITHUB_ANNOTATION_CAP: u64 = 10;

/// One error/warning annotation, collected before emission so the full set can
/// be sorted deterministically by `(path, line, rule)`. Which findings GitHub
/// registers under its per-type cap depends on emission order, so a stable
/// order makes the surfaced subset reproducible across runs (iter-186 —
/// root cause of the iter-171 evidence flakiness).
struct Annotation {
    /// `"error"` or `"warning"` — the workflow command name.
    command: &'static str,
    /// Repo-root-relative file path (already prefixed).
    file: String,
    /// 1-based line, or `None` for a file-level annotation (sorts first).
    line: Option<u64>,
    /// Rule id used as the annotation `title` (may be empty).
    rule: String,
    /// The (un-escaped) message text.
    message: String,
}

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
/// Error/warning annotations are emitted in a deterministic order — sorted by
/// `(path, line, rule)` — so the subset GitHub keeps under its per-type cap is
/// stable across runs. Fix-mode `::notice` annotations follow, in file order.
///
/// `path_prefix` is prepended (with a `/` separator, always forward-slash for
/// portability) to each vault-relative file path so annotations resolve against
/// the repo root. Pass an empty string when the vault dir *is* the CWD.
#[must_use]
pub fn render(payload: &serde_json::Value, path_prefix: &str) -> String {
    let mut out = String::new();
    let prefix = normalize_prefix(path_prefix);

    // Fix-mode payloads carry `total_fixed` / `total_remaining` (see
    // `ExtLintFixOutput`). In that mode, would-be-fixed violations are rendered
    // as `::notice` annotations with a `[fixable]` title prefix so the PR check
    // is visibly different from a plain lint run (df-own-kb U6): a reviewer can
    // tell at a glance which findings `--fix` would resolve versus which remain.
    let is_fix_mode = payload.get("total_fixed").is_some();

    // Collect error/warning annotations first so they can be sorted before
    // emission (iter-186). Fix-mode `::notice` lines are written directly in
    // file order below — they are informational and not subject to the same
    // determinism/cap concern as the error/warning annotations GitHub gates.
    let mut annotations: Vec<Annotation> = Vec::new();

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
                            annotations.push(Annotation {
                                command: if severity == "error" {
                                    "error"
                                } else {
                                    "warning"
                                },
                                file: full_path.clone(),
                                line,
                                rule: rule.to_owned(),
                                message: message.to_owned(),
                            });
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
                    annotations.push(Annotation {
                        command: if severity == "error" {
                            "error"
                        } else {
                            "warning"
                        },
                        file: full_path.clone(),
                        line: None,
                        rule: String::new(),
                        message: message.to_owned(),
                    });
                }
            }
        }
    }

    // Deterministic emission order: sort by (path, line, rule) so which
    // annotations GitHub registers under its per-type cap is stable across runs
    // (iter-186). File-level annotations (line == None) sort before lined ones
    // for the same path.
    annotations.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.rule.cmp(&b.rule))
    });
    for a in &annotations {
        write_command(&mut out, a.command, &a.file, a.line, &a.rule, &a.message);
    }

    // Fix-mode `::notice` annotations (would-be-fixed violations), in file
    // order. Kept separate from the sorted error/warning stream above.
    if is_fix_mode && let Some(files) = payload.get("files").and_then(|f| f.as_array()) {
        for file_entry in files {
            let file = file_entry
                .get("file")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("?");
            let full_path = if file == ".hyalo.toml" {
                file.to_owned()
            } else {
                join_path(&prefix, file)
            };
            // Fix-mode: annotate would-be-fixed violations as `::notice` with a
            // `[fixable]` title prefix so they read distinctly from the errors
            // and warnings that remain (df-own-kb U6).
            if let Some(fixed_groups) = file_entry.get("fixed_groups").and_then(|fg| fg.as_array())
            {
                for group in fixed_groups {
                    let rule = group
                        .get("rule")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    let title = if rule.is_empty() {
                        "[fixable]".to_owned()
                    } else {
                        format!("[fixable] {rule}")
                    };
                    let violations = group.get("violations").and_then(|v| v.as_array());
                    match violations {
                        Some(vs) if !vs.is_empty() => {
                            for v in vs {
                                let line = v
                                    .get("line")
                                    .and_then(serde_json::Value::as_u64)
                                    .filter(|&l| l > 0);
                                let message = v
                                    .get("message")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("");
                                write_notice(&mut out, &full_path, line, &title, message);
                            }
                        }
                        // Groups with no per-violation detail (e.g. SCHEMA
                        // fixes counted via re-validation) still get one
                        // file-level notice so the fix is visible.
                        _ => {
                            let count = group
                                .get("count")
                                .and_then(serde_json::Value::as_u64)
                                .unwrap_or(0);
                            let message = format!("{count} violation(s) would be fixed");
                            write_notice(&mut out, &full_path, None, &title, &message);
                        }
                    }
                }
            }
        }
    }

    // Trailing summary line. Fix-mode gets a distinct `N fixable, M remaining`
    // shape so `--fix --dry-run` output is never byte-identical to a plain lint
    // run (df-own-kb U6); read-only keeps the `N errors, M warnings` shape.
    if is_fix_mode {
        let fixable = payload
            .get("total_fixed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let remaining = payload
            .get("total_remaining")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let _ = write!(
            out,
            "{fixable} {}, {remaining} remaining",
            plural(fixable, "fixable", "fixable"),
        );
        return out;
    }

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

    // Truncation honesty (iter-186): GitHub registers at most
    // `GITHUB_ANNOTATION_CAP` annotations of each type per step. When this run
    // emitted more errors or warnings than that, the PR check silently drops
    // the overflow — so emit a `::notice::` stating the true totals and the cap
    // GitHub will apply. Stay quiet when nothing is hidden (both under the cap).
    if errors > GITHUB_ANNOTATION_CAP || warnings > GITHUB_ANNOTATION_CAP {
        let mut over = Vec::new();
        if errors > GITHUB_ANNOTATION_CAP {
            over.push(format!("{errors} {}", plural(errors, "error", "errors")));
        }
        if warnings > GITHUB_ANNOTATION_CAP {
            over.push(format!(
                "{warnings} {}",
                plural(warnings, "warning", "warnings")
            ));
        }
        let _ = writeln!(
            out,
            "::notice::hyalo emitted {} but GitHub registers at most {GITHUB_ANNOTATION_CAP} annotations of each type per step; the rest are not shown on the PR. Run `hyalo lint --strict` locally (or see the lint-kb-full check on main) for the full list.",
            over.join(" and "),
        );
    }

    let _ = write!(
        out,
        "{errors} {}, {warnings} {} in {files_with_issues} {}",
        plural(errors, "error", "errors"),
        plural(warnings, "warning", "warnings"),
        plural(files_with_issues, "file", "files"),
    );

    out
}

/// Write one `::notice` workflow command line to `out`, used for fix-mode
/// would-be-fixed annotations. Mirrors [`write_command`] but always uses the
/// `notice` command (there is no error/warning distinction for a fixable).
fn write_notice(out: &mut String, file: &str, line: Option<u64>, title: &str, message: &str) {
    use std::fmt::Write as _;
    let _ = write!(out, "::notice file={}", escape_property(file));
    if let Some(l) = line {
        let _ = write!(out, ",line={l}");
    }
    if !title.is_empty() {
        let _ = write!(out, ",title={}", escape_property(title));
    }
    let _ = writeln!(out, "::{}", escape_data(message));
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
        // Sorted by (path, line, rule): same path, so line 1 (SCHEMA) precedes
        // line 3 (HYALO001) despite HYALO001 appearing first in the payload.
        assert_eq!(
            lines[0],
            "::warning file=kb/notes/a.md,line=1,title=SCHEMA::no 'type' property"
        );
        assert_eq!(
            lines[1],
            "::error file=kb/notes/a.md,line=3,title=HYALO001::missing required property \"title\""
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

    /// Annotations are emitted sorted by (path, line, rule), regardless of the
    /// order files/groups/violations appear in the payload (iter-186). This
    /// makes the subset GitHub keeps under its per-type cap reproducible.
    #[test]
    fn render_sorts_annotations_by_path_line_rule() {
        let payload = json!({
            "files": [
                {
                    "file": "z/late.md",
                    "rule_groups": [
                        {"rule": "MD013", "severity": "warn",
                         "violations": [{"line": 2, "message": "z2"}]}
                    ]
                },
                {
                    "file": "a/early.md",
                    "rule_groups": [
                        // Two violations on the same line: MD040 must sort
                        // before MD041 by rule id. And line 5 sorts after
                        // line 1 within the same file.
                        {"rule": "MD041", "severity": "warn",
                         "violations": [{"line": 1, "message": "a1-md041"}]},
                        {"rule": "MD040", "severity": "warn",
                         "violations": [{"line": 1, "message": "a1-md040"}]},
                        {"rule": "MD013", "severity": "warn",
                         "violations": [{"line": 5, "message": "a5"}]},
                        // line 0 (file-level) must sort before all lined ones.
                        {"rule": "SCHEMA", "severity": "warn",
                         "violations": [{"line": 0, "message": "a0"}]}
                    ]
                }
            ],
            "errors": 0,
            "warnings": 5,
            "files_with_violations": 2
        });
        let out = render(&payload, "");
        let lines: Vec<&str> = out.lines().collect();
        let messages: Vec<&str> = lines
            .iter()
            .take_while(|l| l.starts_with("::"))
            .map(|l| l.rsplit("::").next().unwrap())
            .collect();
        assert_eq!(messages, vec!["a0", "a1-md040", "a1-md041", "a5", "z2"]);
    }

    /// The sort is stable across payload input order: reversing the file order
    /// yields byte-identical output (iter-171 flakiness root cause).
    #[test]
    fn render_ordering_is_stable_across_input_permutations() {
        let group = |rule: &str, line: u64, msg: &str| {
            json!({"rule": rule, "severity": "warn",
                   "violations": [{"line": line, "message": msg}]})
        };
        let file_a = json!({"file": "a.md", "rule_groups": [group("MD013", 3, "a3")]});
        let file_b = json!({"file": "b.md", "rule_groups": [group("MD013", 1, "b1")]});
        let forward = json!({
            "files": [file_a.clone(), file_b.clone()],
            "errors": 0, "warnings": 2, "files_with_violations": 2
        });
        let reversed = json!({
            "files": [file_b, file_a],
            "errors": 0, "warnings": 2, "files_with_violations": 2
        });
        assert_eq!(render(&forward, ""), render(&reversed, ""));
    }

    /// A truncation `::notice::` appears when warnings exceed the cap, naming
    /// the true total; it is absent at or below the cap (iter-186).
    #[test]
    fn render_emits_truncation_notice_over_cap() {
        let make = |warns: u64| {
            let violations: Vec<_> = (1..=warns)
                .map(|i| json!({"line": i, "message": format!("w{i}")}))
                .collect();
            json!({
                "files": [{
                    "file": "a.md",
                    "rule_groups": [{"rule": "MD013", "severity": "warn", "violations": violations}]
                }],
                "errors": 0,
                "warnings": warns,
                "files_with_violations": 1
            })
        };
        // Exactly at the cap: no notice.
        let out = render(&make(GITHUB_ANNOTATION_CAP), "");
        assert!(
            !out.contains("::notice::"),
            "no truncation notice at the cap, got:\n{out}"
        );
        // Over the cap: a notice naming the true total and the cap.
        let out = render(&make(GITHUB_ANNOTATION_CAP + 5), "");
        let notice = out
            .lines()
            .find(|l| l.starts_with("::notice::"))
            .expect("truncation notice present over the cap");
        assert!(notice.contains("15 warnings"), "got: {notice}");
        assert!(
            notice.contains(&format!("at most {GITHUB_ANNOTATION_CAP}")),
            "got: {notice}"
        );
        // The summary line still reports the true totals.
        assert!(out.contains("0 errors, 15 warnings in 1 file"));
    }

    /// When both errors and warnings exceed the cap, the notice names both.
    #[test]
    fn render_truncation_notice_names_both_types() {
        let group = |rule: &str, sev: &str, n: u64| {
            let vs: Vec<_> = (1..=n)
                .map(|i| json!({"line": i, "message": format!("{rule}-{i}")}))
                .collect();
            json!({"rule": rule, "severity": sev, "violations": vs})
        };
        let payload = json!({
            "files": [{
                "file": "a.md",
                "rule_groups": [
                    group("HYALO001", "error", 11),
                    group("MD013", "warn", 12)
                ]
            }],
            "errors": 11,
            "warnings": 12,
            "files_with_violations": 1
        });
        let out = render(&payload, "");
        let notice = out
            .lines()
            .find(|l| l.starts_with("::notice::"))
            .expect("notice present");
        assert!(notice.contains("11 errors"), "got: {notice}");
        assert!(notice.contains("12 warnings"), "got: {notice}");
        assert!(notice.contains(" and "), "got: {notice}");
    }
}
