//! Explicit status for two historic gate names that were never implemented.
//!
//! A non-zero result is deliberate: these lexical analyses have no defined
//! contract and therefore cannot be cited as behavioral coverage.

use anyhow::Result;
use clap::Args;

/// Shared arg shape for stubs. Accepts (and ignores) `--since <REF>` so the
/// ralph-loop harness can call these uniformly alongside the real gates.
#[derive(Args)]
pub struct LegacyGateArgs {
    /// Historic compatibility argument. No analysis is performed.
    #[arg(long, value_name = "REF")]
    #[allow(dead_code)]
    pub since: Option<String>,
}

/// Stub for `check-dead-primitives`.
///
// allow-todo: iter-142b
pub fn check_dead_primitives() -> Result<bool> {
    eprintln!(
        "check-dead-primitives: UNSUPPORTED — no behavioral contract is defined; this command is not a passing quality gate"
    );
    Ok(false)
}

/// Stub for `check-todo-annotations`.
///
// allow-todo: iter-142b
pub fn check_todo_annotations() -> Result<bool> {
    eprintln!(
        "check-todo-annotations: UNSUPPORTED — no behavioral contract is defined; this command is not a passing quality gate"
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_legacy_names_cannot_report_success() {
        assert!(!check_dead_primitives().unwrap());
        assert!(!check_todo_annotations().unwrap());
    }
}
