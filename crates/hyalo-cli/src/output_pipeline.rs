use anyhow::Result;

use crate::commands::files_from::FilesFromCounters;
use crate::hints::{FilesFromCounterSummary, HintContext, generate_hints_with_counters};
use crate::output::{CommandOutcome, Format, apply_jq_filter_result, build_envelope_value};

/// Error message for `--count` on non-list commands (shared across match arms).
///
/// Deliberately has no "Error: " prefix baked in — callers route it through
/// [`crate::output::format_error`] so it renders correctly under both
/// `--format text` (which adds the prefix) and `--format json`.
pub(crate) const COUNT_UNSUPPORTED_ERROR: &str = "--count is only supported for list commands (find, tags summary, properties summary, backlinks, lint)";

/// Encapsulates the post-command output pipeline: jq filtering, hint generation,
/// and envelope wrapping.
pub(crate) struct OutputPipeline<'a> {
    /// Format the user requested.
    pub user_format: Format,
    /// Optional jq filter expression (operates on the full envelope).
    pub jq_filter: Option<&'a str>,
    /// Optional hint context for drill-down commands.
    pub hint_ctx: Option<&'a HintContext>,
    /// Print only the total count as a bare integer.
    pub count: bool,
    /// When `--files-from` was used, inject skip counters into the envelope.
    pub files_from_counters: Option<FilesFromCounters>,
    /// Path prefix for `--format github` annotations: the vault dir expressed
    /// relative to CWD, prepended to each vault-relative file path so GitHub
    /// resolves annotations against the repo root. Empty when the vault dir is
    /// the CWD. Only consulted when `user_format == Format::Github`.
    pub github_path_prefix: String,
}

/// Inject `files_missing`, `files_skipped_non_md`, and `files_skipped_outside_vault`
/// counters into the envelope's `results` object when `--files-from` was used.
///
/// When `counters` is `None` (no `--files-from` on this invocation), the envelope is
/// left untouched and the fields are omitted entirely. When `counters` is `Some`, all
/// three fields are written — including zero values — so consumers can reliably
/// distinguish "used `--files-from`, zero skips" from "`--files-from` was not used".
///
/// For commands where `results` is an object (e.g. lint), counters are inserted
/// directly into that object.  For commands where `results` is an array (e.g. find),
/// the array is promoted to `{"files": [...], "files_missing": N, ...}` so that
/// `jq '.results | keys'` returns the counter names alongside `"files"`.
fn inject_files_from_counters(
    envelope: &mut serde_json::Value,
    counters: Option<&FilesFromCounters>,
) {
    let Some(c) = counters else { return };
    let Some(envelope_obj) = envelope.as_object_mut() else {
        return;
    };
    let Some(results) = envelope_obj.get_mut("results") else {
        return;
    };
    if let Some(obj) = results.as_object_mut() {
        // results is already an object (e.g. lint) — insert directly.
        obj.insert(
            "files_missing".to_owned(),
            serde_json::json!(c.files_missing),
        );
        obj.insert(
            "files_skipped_non_md".to_owned(),
            serde_json::json!(c.files_skipped_non_md),
        );
        obj.insert(
            "files_skipped_outside_vault".to_owned(),
            serde_json::json!(c.files_skipped_outside_vault),
        );
    } else if results.is_array() {
        // results is a bare array (e.g. find) — promote to an object so counter
        // fields are addressable via `.results | keys`.
        let arr = results.take();
        *results = serde_json::json!({
            "files": arr,
            "files_missing": c.files_missing,
            "files_skipped_non_md": c.files_skipped_non_md,
            "files_skipped_outside_vault": c.files_skipped_outside_vault,
        });
    }
}

impl OutputPipeline<'_> {
    /// One-line human-readable summary of `--files-from` input paths that were
    /// dropped before linting, or `None` when every input resolved (or
    /// `--files-from` was not used).
    ///
    /// Shared verbatim between `--format text` (`note: …` on stderr) and
    /// `--format github` (`::notice::…` on stdout) so both formats report the
    /// same dropped-path story a `--format json` consumer already sees in the
    /// `files_missing` / `files_skipped_*` envelope fields (UX-B).
    fn skip_summary(&self) -> Option<String> {
        let c = self.files_from_counters.as_ref()?;
        let mut parts: Vec<String> = Vec::new();
        if c.files_missing > 0 {
            let noun = if c.files_missing == 1 {
                "path"
            } else {
                "paths"
            };
            parts.push(format!("{} input {noun} missing", c.files_missing));
        }
        if c.files_skipped_non_md > 0 {
            parts.push(format!("{} non-markdown skipped", c.files_skipped_non_md));
        }
        if c.files_skipped_outside_vault > 0 {
            parts.push(format!(
                "{} outside vault skipped",
                c.files_skipped_outside_vault
            ));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(", "))
        }
    }

    /// Process a command result through the output pipeline.
    /// Prints output to stdout/stderr and returns the exit code.
    pub fn finalize(&self, result: Result<CommandOutcome>) -> i32 {
        match result {
            Ok(CommandOutcome::Success { output, total }) => {
                // --count: print bare total and exit early.
                if self.count {
                    if let Some(n) = total {
                        println!("{n}");
                        return 0;
                    }
                    eprintln!(
                        "{}",
                        crate::output::format_error(
                            self.user_format,
                            COUNT_UNSUPPORTED_ERROR,
                            None,
                            None,
                            None,
                        )
                    );
                    return 2;
                }

                // Commands always produce JSON internally.
                let value: serde_json::Value = match serde_json::from_str(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = crate::output::format_error(
                            self.user_format,
                            "internal error: failed to parse command JSON output",
                            None,
                            None,
                            Some(&e.to_string()),
                        );
                        eprintln!("{msg}");
                        return 2;
                    }
                };

                // `--format github`: render lint violations as GitHub Actions
                // workflow commands (inline PR annotations) plus a summary line.
                // This bypasses the JSON envelope, hints, and jq entirely — those
                // are rejected for `github` upstream. `github` is lint-only, so
                // `value` here is always the extended lint payload.
                if self.user_format == Format::Github {
                    let rendered =
                        crate::commands::lint_github::render(&value, &self.github_path_prefix);
                    println!("{rendered}");
                    // Surface dropped `--files-from` input paths as a GitHub
                    // Actions `::notice::` so a diff-scoped CI run shows in the
                    // job log that some inputs never reached lint (UX-B).
                    if let Some(summary) = self.skip_summary() {
                        println!("::notice::{summary}");
                    }
                    return 0;
                }

                // Generate hints when a context is available. Pass through
                // the `--files-from` counters so iter-143's counter-aware
                // hints can fire (the envelope merge for the JSON shape
                // happens later, so `value` doesn't carry them yet).
                let hints = if let Some(ctx) = self.hint_ctx {
                    let counters =
                        self.files_from_counters
                            .as_ref()
                            .map(|c| FilesFromCounterSummary {
                                files_missing: c.files_missing,
                                files_skipped_outside_vault: c.files_skipped_outside_vault,
                            });
                    generate_hints_with_counters(ctx, &value, total, counters)
                } else {
                    Vec::new()
                };

                if let Some(filter) = self.jq_filter {
                    // Build the full envelope first so jq can address any field.
                    let mut envelope = build_envelope_value(&value, total, &hints);
                    inject_files_from_counters(&mut envelope, self.files_from_counters.as_ref());
                    match apply_jq_filter_result(filter, &envelope) {
                        Ok(filtered) => println!("{filtered}"),
                        Err(e) => {
                            let msg = crate::output::format_error(
                                self.user_format,
                                "jq filter failed",
                                None,
                                None,
                                Some(&e),
                            );
                            eprintln!("{msg}");
                            return 1;
                        }
                    }
                } else {
                    let mut envelope = build_envelope_value(&value, total, &hints);
                    inject_files_from_counters(&mut envelope, self.files_from_counters.as_ref());
                    let formatted = crate::output::format_prebuilt_envelope(
                        self.user_format,
                        &envelope,
                        total,
                        &hints,
                        &value,
                    );
                    println!("{formatted}");
                    // Surface dropped `--files-from` input paths as a stderr
                    // note so a diff-scoped `--format text` run shows that some
                    // inputs never reached lint — matching the `::notice::` the
                    // github format emits (UX-B). JSON stays as-is (the counters
                    // are already injected into the envelope above).
                    if self.user_format == Format::Text
                        && let Some(summary) = self.skip_summary()
                    {
                        eprintln!("note: {summary}");
                    }
                    // In text mode, when a list command returns zero results, emit a
                    // notice on stderr so the user knows the command ran successfully.
                    // Only fires for list commands (total is Some) with empty arrays.
                    if self.user_format == Format::Text
                        && total == Some(0)
                        && value.as_array().is_some_and(Vec::is_empty)
                    {
                        eprintln!("No results");
                    }
                }
                0
            }
            Ok(CommandOutcome::RawOutput(output)) => {
                if self.count {
                    eprintln!(
                        "{}",
                        crate::output::format_error(
                            self.user_format,
                            COUNT_UNSUPPORTED_ERROR,
                            None,
                            None,
                            None,
                        )
                    );
                    return 2;
                }
                // Raw output bypasses the JSON pipeline — print directly to stdout.
                // Used by the `read` command for text-format content output.
                // println! matches pre-refactor behavior: the content string already ends
                // with '\n', and the extra newline from println! preserves empty-line
                // endings (e.g. `--lines :2` where line 2 is blank).
                //
                // Sanitized here (unlike the JSON pipeline above) because RawOutput
                // is raw file body content that never passes through format_success/
                // format_envelope — without this, a vault file containing ANSI escapes
                // or other control bytes would inject them straight into the terminal.
                println!("{}", crate::output::sanitize_control_chars(&output));
                0
            }
            Ok(CommandOutcome::UserError(output)) => {
                // UserError strings are always formatted as JSON internally (effective_format=Json).
                // When the user requested text format, re-format the error as human-readable text.
                let displayed = if self.user_format == Format::Text {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&output) {
                        let error = v["error"].as_str().unwrap_or("unknown error");
                        let path = v["path"].as_str();
                        let hint = v["hint"].as_str();
                        let cause = v["cause"].as_str();
                        crate::output::format_error(Format::Text, error, path, hint, cause)
                    } else {
                        output
                    }
                } else {
                    output
                };
                eprintln!("{displayed}");
                1
            }
            Err(e) => {
                let msg = crate::output::format_error(
                    self.user_format,
                    &e.to_string(),
                    None,
                    None,
                    e.chain()
                        .nth(1)
                        .map(std::string::ToString::to_string)
                        .as_deref(),
                );
                eprintln!("{msg}");
                2
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline_with_counters(counters: FilesFromCounters) -> OutputPipeline<'static> {
        OutputPipeline {
            user_format: Format::Text,
            jq_filter: None,
            hint_ctx: None,
            count: false,
            files_from_counters: Some(counters),
            github_path_prefix: String::new(),
        }
    }

    #[test]
    fn skip_summary_singular_missing_path() {
        let pipeline = pipeline_with_counters(FilesFromCounters {
            files_missing: 1,
            files_skipped_non_md: 0,
            files_skipped_outside_vault: 0,
        });
        assert_eq!(
            pipeline.skip_summary().as_deref(),
            Some("1 input path missing")
        );
    }

    #[test]
    fn skip_summary_plural_missing_paths() {
        let pipeline = pipeline_with_counters(FilesFromCounters {
            files_missing: 2,
            files_skipped_non_md: 0,
            files_skipped_outside_vault: 0,
        });
        assert_eq!(
            pipeline.skip_summary().as_deref(),
            Some("2 input paths missing")
        );
    }

    #[test]
    fn skip_summary_combines_all_counters() {
        let pipeline = pipeline_with_counters(FilesFromCounters {
            files_missing: 1,
            files_skipped_non_md: 3,
            files_skipped_outside_vault: 1,
        });
        assert_eq!(
            pipeline.skip_summary().as_deref(),
            Some("1 input path missing, 3 non-markdown skipped, 1 outside vault skipped")
        );
    }

    #[test]
    fn skip_summary_none_when_no_counters() {
        let pipeline = OutputPipeline {
            user_format: Format::Text,
            jq_filter: None,
            hint_ctx: None,
            count: false,
            files_from_counters: None,
            github_path_prefix: String::new(),
        };
        assert_eq!(pipeline.skip_summary(), None);
    }

    #[test]
    fn skip_summary_none_when_all_zero() {
        let pipeline = pipeline_with_counters(FilesFromCounters {
            files_missing: 0,
            files_skipped_non_md: 0,
            files_skipped_outside_vault: 0,
        });
        assert_eq!(pipeline.skip_summary(), None);
    }
}
