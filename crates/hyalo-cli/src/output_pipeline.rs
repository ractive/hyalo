use anyhow::Result;

use crate::hints::{HintContext, generate_hints};
use crate::output::{
    CommandOutcome, Format, apply_jq_filter_result, build_envelope_value, format_envelope,
};

/// Encapsulates the post-command output pipeline: jq filtering, hint generation,
/// and envelope wrapping.
pub(crate) struct OutputPipeline<'a> {
    /// Format the user requested.
    pub user_format: Format,
    /// Optional jq filter expression (operates on the full envelope).
    pub jq_filter: Option<&'a str>,
    /// Optional hint context for drill-down commands.
    pub hint_ctx: Option<&'a HintContext>,
}

impl OutputPipeline<'_> {
    /// Process a command result through the output pipeline.
    /// Prints output to stdout/stderr and returns the exit code.
    pub fn finalize(&self, result: Result<CommandOutcome>) -> i32 {
        match result {
            Ok(CommandOutcome::Success { output, total }) => {
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

                // Generate hints when a context is available.
                let hints = if let Some(ctx) = self.hint_ctx {
                    generate_hints(ctx, &value)
                } else {
                    Vec::new()
                };

                if let Some(filter) = self.jq_filter {
                    // Build the full envelope first so jq can address any field.
                    let envelope = build_envelope_value(&value, total, &hints);
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
                    let formatted = format_envelope(self.user_format, &value, total, &hints);
                    // In text mode, when the result is an empty array, emit a notice on stderr
                    // so the user knows the command ran but produced no matches.
                    if self.user_format == Format::Text
                        && value.as_array().is_some_and(std::vec::Vec::is_empty)
                    {
                        eprintln!("No files matched");
                    } else {
                        println!("{formatted}");
                    }
                }
                0
            }
            Ok(CommandOutcome::RawOutput(output)) => {
                // Raw output bypasses the JSON pipeline — print directly to stdout.
                // Used by the `read` command for text-format content output.
                // println! matches pre-refactor behavior: the content string already ends
                // with '\n', and the extra newline from println! preserves empty-line
                // endings (e.g. `--lines :2` where line 2 is blank).
                println!("{output}");
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
