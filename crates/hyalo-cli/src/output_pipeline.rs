use anyhow::Result;

use crate::commands::files_from::FilesFromCounters;
use crate::hints::{FilesFromCounterSummary, HintContext, generate_hints_with_counters};
use crate::output::{CommandOutcome, Envelope, Format, evaluate_jq_isolated, output_value};

/// Error message for `--count` on non-list commands (shared across match arms).
///
/// Re-exported from [`crate::list_commands`], the single source of truth for
/// which commands emit a `total`. Do not hand-write the command list here —
/// see that module's docs (iter-192).
pub(crate) use crate::list_commands::count_unsupported_error;

/// Encapsulates the post-command output pipeline: jq filtering, hint generation,
/// and envelope wrapping.
pub(crate) struct OutputPipeline<'a> {
    /// Format the user requested for *results*.
    pub user_format: Format,
    /// Format used for error envelopes (L-5).
    ///
    /// Normally identical to [`Self::user_format`]. They diverge for `read`,
    /// whose results default to raw text (you want the note's markdown, not a
    /// JSON string) even when stdout is a pipe — but whose *errors* must still
    /// follow the ambient piped-JSON default, exactly like every sibling
    /// command, so a script can parse them.
    pub error_format: Format,
    /// Optional jq filter expression (operates on the full envelope).
    pub jq_filter: Option<&'a str>,
    /// Optional hint context for drill-down commands.
    pub hint_ctx: Option<&'a HintContext>,
    /// Print only the total count as a bare integer.
    pub count: bool,
    pub projection: crate::prepared::Projection,
    pub internal_report: bool,
    /// When `--files-from` was used, inject skip counters into the envelope.
    pub files_from_counters: Option<FilesFromCounters>,
    /// Path prefix for `--format github` annotations: the vault dir expressed
    /// relative to CWD, prepended to each vault-relative file path so GitHub
    /// resolves annotations against the repo root. Empty when the vault dir is
    /// the CWD. Only consulted when `user_format == Format::Github`.
    pub github_path_prefix: String,
}

impl OutputPipeline<'_> {
    pub(crate) fn plain(format: Format, jq: Option<&str>) -> OutputPipeline<'_> {
        OutputPipeline {
            user_format: format,
            error_format: crate::error::transport_error_format(format),
            jq_filter: jq,
            hint_ctx: None,
            count: false,
            projection: crate::prepared::Projection::Standard,
            internal_report: false,
            files_from_counters: None,
            github_path_prefix: String::new(),
        }
    }

    pub(crate) fn finalize_config(&self, outcome: CommandOutcome) -> i32 {
        let mut report = ExecutionReport::from_result(Ok(outcome));
        report.payload = report.payload.map(|payload| match payload {
            ReportPayload::Value(value, _) => ReportPayload::Envelope(value),
            ReportPayload::Text(mut text) => {
                if text.ends_with('\n') {
                    text.pop();
                }
                ReportPayload::Text(text)
            }
            other => other,
        });
        self.write_report(
            report,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        )
    }

    pub(crate) fn finalize_with_effects(
        &self,
        outcome: CommandOutcome,
        effects: crate::commands::apply::ApplyReport,
    ) -> i32 {
        let mut report = ExecutionReport::from_result(Ok(outcome));
        report.effects = Some(effects);
        self.write_report(
            report,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        )
    }

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

    /// Rendering completes before stdout publication. Effects remain owned by
    /// the report until both output channels have been finalized.
    pub fn finalize(&self, result: Result<CommandOutcome>) -> i32 {
        self.finalize_to(
            result,
            &mut std::io::stdout().lock(),
            &mut std::io::stderr().lock(),
        )
    }

    fn finalize_to(
        &self,
        result: Result<CommandOutcome>,
        stdout: &mut dyn std::io::Write,
        stderr: &mut dyn std::io::Write,
    ) -> i32 {
        self.write_report(ExecutionReport::from_result(result), stdout, stderr)
    }

    fn write_report(
        &self,
        report: ExecutionReport,
        stdout: &mut dyn std::io::Write,
        stderr: &mut dyn std::io::Write,
    ) -> i32 {
        // Drain warnings before rendering: even a renderer failure must leave
        // the typed error envelope as the final framed stderr object.
        let _ = stderr.write_all(crate::warn::take_summary().as_bytes());
        let effects = report.effects.clone();
        let completeness = report.completeness;
        let rendered = self.render(report);
        let rendered = match rendered {
            Ok(rendered) => rendered,
            Err(mut diagnostic) => {
                diagnostic.effects = effects;
                if completeness == Completeness::Incomplete && diagnostic.hint.is_none() {
                    diagnostic.hint = Some(
                        "execution was incomplete; inspect the effect report before retrying"
                            .into(),
                    );
                }
                let text = format!("{}\n", diagnostic.render(self.error_format));
                let _ = stderr.write_all(text.as_bytes());
                return 2;
            }
        };
        let write_result = stderr
            .write_all(&rendered.stderr)
            .and_then(|()| stdout.write_all(&rendered.stdout))
            .and_then(|()| stdout.flush());
        if let Err(error) = write_result {
            let broken = error.kind() == std::io::ErrorKind::BrokenPipe;
            if !broken
                || effects
                    .as_ref()
                    .is_some_and(|report| !report.committed_paths().is_empty())
            {
                let mut diagnostic = crate::output::user_diagnostic(
                    self.error_format,
                    "failed to write output",
                    None,
                    Some("inspect committed paths before retrying"),
                    Some(&error.to_string()),
                );
                diagnostic.category = Some("output_failure");
                diagnostic.effects = effects;
                let _ = writeln!(stderr, "{}", diagnostic.render(self.error_format));
            }
            return if broken {
                crate::broken_pipe::BROKEN_PIPE_EXIT_CODE
            } else {
                2
            };
        }
        rendered.code
    }

    fn render(
        &self,
        report: ExecutionReport,
    ) -> std::result::Result<RenderedReport, Box<crate::output::UserDiagnostic>> {
        let mut rendered = RenderedReport {
            stdout: Vec::new(),
            stderr: Vec::new(),
            code: report.code,
        };
        for diagnostic in report.diagnostics {
            rendered.stderr.extend_from_slice(
                format!("{}\n", diagnostic.render(self.error_format)).as_bytes(),
            );
        }
        let Some(payload) = report.payload else {
            return Ok(rendered);
        };
        match payload {
            ReportPayload::Envelope(envelope) => {
                let formatted = if let Some(filter) = self.jq_filter {
                    evaluate_jq_isolated(filter, &envelope).map_err(|error| {
                        let mut diagnostic = crate::output::user_diagnostic(
                            self.error_format,
                            "jq filter failed",
                            None,
                            None,
                            Some(&error),
                        );
                        diagnostic.category = Some("output_failure");
                        diagnostic
                    })?
                } else {
                    serde_json::to_string_pretty(&envelope)
                        .map_err(|error| crate::output::UserDiagnostic::new(error.to_string()))?
                };
                rendered.stdout = format!("{formatted}\n").into_bytes();
            }
            ReportPayload::Bytes(bytes) => rendered.stdout = bytes,
            ReportPayload::Text(text) => {
                rendered.stdout = crate::output::sanitize_control_chars(&text).into_bytes();
                if !text.is_empty() {
                    rendered.stdout.push(b'\n');
                }
            }
            ReportPayload::Value(value, total) => {
                if self.count {
                    let count = total.ok_or_else(|| {
                        crate::output::user_diagnostic(
                            self.error_format,
                            count_unsupported_error(),
                            None,
                            None,
                            None,
                        )
                    })?;
                    rendered.stdout = format!("{count}\n").into_bytes();
                    return Ok(rendered);
                }
                if self.projection != crate::prepared::Projection::Standard {
                    for file in value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("file").and_then(serde_json::Value::as_str))
                    {
                        if self.projection == crate::prepared::Projection::Filenames0 {
                            rendered.stdout.extend_from_slice(file.as_bytes());
                            rendered.stdout.push(0);
                        } else {
                            rendered.stdout.extend_from_slice(
                                crate::output::sanitize_control_chars(file).as_bytes(),
                            );
                            rendered.stdout.push(b'\n');
                        }
                    }
                    return Ok(rendered);
                }
                if self.user_format == Format::Github {
                    rendered.stdout = format!(
                        "{}\n",
                        hyalo_mdlint::profiles::github::render(&value, &self.github_path_prefix)
                    )
                    .into_bytes();
                    if let Some(summary) = self.skip_summary() {
                        rendered
                            .stdout
                            .extend_from_slice(format!("::notice::{summary}\n").as_bytes());
                    }
                    return Ok(rendered);
                }
                let hints = self.hint_ctx.map_or_else(Vec::new, |ctx| {
                    let counters =
                        self.files_from_counters
                            .as_ref()
                            .map(|c| FilesFromCounterSummary {
                                files_missing: c.files_missing,
                                files_skipped_outside_vault: c.files_skipped_outside_vault,
                            });
                    generate_hints_with_counters(ctx, &value, total, counters)
                });
                let envelope =
                    Envelope::from_result(&value, total, &hints, self.files_from_counters.as_ref());
                let envelope = if self.internal_report {
                    let effects = report.effects.as_ref().ok_or_else(|| {
                        crate::output::UserDiagnostic::new("internal mutation report unavailable")
                    })?;
                    output_value(&crate::output::MutationReportEnvelope { envelope, effects })
                } else {
                    output_value(&envelope)
                };
                let formatted = if let Some(filter) = self.jq_filter {
                    evaluate_jq_isolated(filter, &envelope).map_err(|error| {
                        let mut diagnostic = crate::output::user_diagnostic(
                            self.error_format,
                            "jq filter failed",
                            None,
                            None,
                            Some(&error),
                        );
                        diagnostic.category = Some("output_failure");
                        diagnostic
                    })?
                } else {
                    if self.user_format == Format::Text
                        && total == Some(0)
                        && value.as_array().is_some_and(Vec::is_empty)
                    {
                        let notice = self.hint_ctx.map_or_else(
                            || "No results".to_owned(),
                            crate::hints::zero_result_notice,
                        );
                        rendered
                            .stderr
                            .extend_from_slice(format!("{notice}\n").as_bytes());
                    }
                    crate::output::format_prebuilt_envelope(
                        self.user_format,
                        &envelope,
                        total,
                        &hints,
                        &value,
                    )
                };
                rendered.stdout = format!("{formatted}\n").into_bytes();
                if self.user_format == Format::Text
                    && let Some(summary) = self.skip_summary()
                {
                    rendered
                        .stderr
                        .extend_from_slice(format!("note: {summary}\n").as_bytes());
                }
            }
        }
        Ok(rendered)
    }
}

/// Domain outcome, diagnostics and committed effects cross the renderer as one
/// report. A rendering failure cannot discard the effects or replace its exit
/// status with a command's findings status.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Completeness {
    Complete,
    Incomplete,
}
struct ExecutionReport {
    payload: Option<ReportPayload>,
    diagnostics: Vec<crate::output::UserDiagnostic>,
    effects: Option<crate::commands::apply::ApplyReport>,
    code: i32,
    completeness: Completeness,
}
enum ReportPayload {
    Envelope(serde_json::Value),
    Value(serde_json::Value, Option<u64>),
    Text(String),
    Bytes(Vec<u8>),
}
struct RenderedReport {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    code: i32,
}
impl ExecutionReport {
    fn from_result(result: Result<CommandOutcome>) -> Self {
        let mut report = Self {
            payload: None,
            diagnostics: Vec::new(),
            effects: None,
            code: 0,
            completeness: Completeness::Complete,
        };
        match result {
            Ok(CommandOutcome::Success {
                output,
                total,
                effects,
                status,
            }) => {
                report.payload = Some(ReportPayload::Value(output, total));
                report.effects = effects;
                report.code = i32::from(status == crate::output::DomainStatus::Findings);
            }
            Ok(CommandOutcome::RawOutput(text)) => report.payload = Some(ReportPayload::Text(text)),
            Ok(CommandOutcome::RawBytes(bytes)) => {
                report.payload = Some(ReportPayload::Bytes(bytes));
            }
            Ok(CommandOutcome::UserError(diagnostic)) => {
                report.completeness = Completeness::Incomplete;
                report.effects.clone_from(&diagnostic.effects);
                report.diagnostics.push(diagnostic);
                report.code = 1;
            }
            Err(error) => {
                report.completeness = Completeness::Incomplete;
                let diagnostic = if let Some(diagnostic) =
                    error.downcast_ref::<crate::output::UserDiagnostic>()
                {
                    report.code = 1;
                    diagnostic.clone()
                } else if let Some(budget) = hyalo_core::frontmatter::as_budget_error(&error) {
                    report.code = 1;
                    crate::output::budget_diagnostic(Format::Json, budget)
                } else if hyalo_core::frontmatter::is_parse_error(&error) {
                    report.code = 1;
                    crate::output::user_diagnostic(
                        Format::Json,
                        "frontmatter input or rewrite was rejected",
                        None,
                        Some("fix the document's frontmatter framing or size, then retry"),
                        Some(&crate::commands::terse_root_cause(&error)),
                    )
                } else if let Some(user) = error.downcast_ref::<hyalo_core::UserFacingError>() {
                    report.code = 1;
                    crate::output::user_diagnostic(
                        Format::Json,
                        &user.message,
                        None,
                        user.hint.as_deref(),
                        user.cause.as_deref(),
                    )
                } else {
                    report.code = 2;
                    crate::output::user_diagnostic(
                        Format::Json,
                        &error.to_string(),
                        None,
                        None,
                        error.chain().nth(1).map(ToString::to_string).as_deref(),
                    )
                };
                report.effects.clone_from(&diagnostic.effects);
                report.diagnostics.push(diagnostic);
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    struct FailingWriter(std::io::ErrorKind);
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(self.0, "injected output failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn committed_outcome() -> CommandOutcome {
        use crate::commands::apply::{ApplyReport, EffectState, IndexDisposition, PathEffect};
        CommandOutcome::success(serde_json::Value::Null)
            .with_status(1)
            .with_apply_report(ApplyReport {
                paths: vec![PathEffect {
                    file: "a.md".into(),
                    state: EffectState::Committed,
                    error: None,
                    category: None,
                }],
                index: IndexDisposition::Updated,
                index_error: None,
            })
    }
    #[test]
    fn output_failures_preserve_effects_and_override_findings() {
        for (kind, expected) in [
            (std::io::ErrorKind::BrokenPipe, 141),
            (std::io::ErrorKind::Other, 2),
        ] {
            let pipeline = OutputPipeline::plain(Format::Json, None);
            let mut stderr = Vec::new();
            assert_eq!(
                pipeline.finalize_to(
                    Ok(committed_outcome()),
                    &mut FailingWriter(kind),
                    &mut stderr
                ),
                expected
            );
            let error: serde_json::Value = serde_json::from_slice(&stderr).unwrap();
            assert_eq!(error["effects"]["paths"][0]["state"], "committed");
            assert_eq!(error["effects"]["index"], "updated");
        }
    }
    #[test]
    fn renderer_failure_preserves_effects_and_count_preserves_findings() {
        let mut pipeline = OutputPipeline::plain(Format::Json, None);
        pipeline.count = true;
        let mut out = Vec::new();
        let mut err = Vec::new();
        assert_eq!(
            pipeline.finalize_to(Ok(committed_outcome()), &mut out, &mut err),
            2
        );
        assert!(out.is_empty());
        let error: serde_json::Value = serde_json::from_slice(&err).unwrap();
        assert_eq!(error["effects"]["paths"][0]["file"], "a.md");
        err.clear();
        assert_eq!(
            pipeline.finalize_to(
                Ok(CommandOutcome::success_with_total(serde_json::Value::Null, 1).with_status(1)),
                &mut out,
                &mut err
            ),
            1
        );
        assert_eq!(out, b"1\n");
    }

    use super::*;

    fn pipeline_with_counters(counters: FilesFromCounters) -> OutputPipeline<'static> {
        OutputPipeline {
            user_format: Format::Text,
            error_format: Format::Text,
            jq_filter: None,
            hint_ctx: None,
            count: false,
            projection: crate::prepared::Projection::Standard,
            internal_report: false,
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
            error_format: Format::Text,
            jq_filter: None,
            hint_ctx: None,
            count: false,
            projection: crate::prepared::Projection::Standard,
            internal_report: false,
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
