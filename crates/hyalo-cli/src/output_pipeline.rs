use anyhow::Result;

use crate::hints::{HintContext, generate_hints};
use crate::output::{
    CommandOutcome, Format, apply_jq_filter_result, format_success, format_with_hints,
};

/// Encapsulates the post-command output pipeline: jq filtering, hint generation,
/// and format conversion.
pub(crate) struct OutputPipeline<'a> {
    /// Format the user requested (may differ from effective_format).
    pub user_format: Format,
    /// Optional jq filter expression.
    pub jq_filter: Option<&'a str>,
    /// Optional hint context for drill-down commands.
    pub hint_ctx: Option<&'a HintContext>,
    /// Whether --hints is active (but may not have a hint_ctx for this command).
    pub hints_active: bool,
}

impl OutputPipeline<'_> {
    /// Process a command result through the output pipeline.
    /// Prints output to stdout/stderr and returns the exit code.
    pub fn finalize(&self, result: Result<CommandOutcome>) -> i32 {
        match result {
            Ok(CommandOutcome::Success(output)) => {
                if let Some(filter) = self.jq_filter {
                    // Parse the JSON output we forced above, then apply the user filter.
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
                    match apply_jq_filter_result(filter, &value) {
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
                } else if let Some(ctx) = self.hint_ctx {
                    // Re-parse the output to generate hints, then format with them.
                    let value: serde_json::Value = if let Ok(v) = serde_json::from_str(&output) {
                        v
                    } else {
                        // Should not happen since effective_format is forced to JSON,
                        // but fall through to plain output if it does.
                        println!("{output}");
                        return 0;
                    };
                    let hints = generate_hints(ctx, &value);
                    let formatted = format_with_hints(self.user_format, &value, &hints);
                    println!("{formatted}");
                } else if self.hints_active {
                    // --hints forced JSON internally but this command has no hint
                    // generator.  Convert back to the user-requested format.
                    match serde_json::from_str::<serde_json::Value>(&output) {
                        Ok(value) => {
                            println!("{}", format_success(self.user_format, &value));
                        }
                        Err(_) => println!("{output}"),
                    }
                } else {
                    println!("{output}");
                }
                0
            }
            Ok(CommandOutcome::UserError(output)) => {
                eprintln!("{output}");
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
