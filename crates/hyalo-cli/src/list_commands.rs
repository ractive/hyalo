//! The single source of truth for "which commands are list commands".
//!
//! A *list command* is one whose JSON envelope carries a `total` field, which
//! in turn is what makes `--count` (a shortcut for `--jq '.total'`) meaningful
//! and what the default output cap applies to.
//!
//! Before iter-192 this set was written out by hand in five places — the
//! top-level `long_about` OUTPUT paragraph, the `--count` flag help, the
//! `Default output limits` block, the `OUTPUT SHAPES` note, and the `--count`
//! runtime error — and all five disagreed with each other and with the binary.
//! Every one of those call sites now renders [`LIST_COMMANDS`] through the
//! helpers below, so the enumerations cannot drift apart again.
//!
//! Membership is asserted against the real binary by
//! `tests/e2e/count.rs::list_commands_constant_matches_binary`, so adding a
//! command here that does not actually emit `total` (or forgetting to add one
//! that does) fails the test suite.

/// Commands that emit a `total` in their JSON envelope and accept `--count`.
///
/// Entries are the argv words a user types after `hyalo`, in the order they
/// should be presented to the user. Keep the list ordered from most to least
/// commonly used — it is rendered verbatim into help text.
pub(crate) const LIST_COMMANDS: &[&str] = &[
    "find",
    "lint",
    "tags summary",
    "properties summary",
    "backlinks",
    "types list",
    "views list",
    "lint-rules list",
];

/// Commands that accept `--limit` and cap their output at `default_limit`.
///
/// A strict subset of [`LIST_COMMANDS`]: every command here emits a `total`,
/// but not every `total`-emitting command is *capped*. `types list`,
/// `views list` and `lint-rules list` enumerate small fixed catalogs — they
/// return everything and reject `--limit` outright — yet the "Default output
/// limits" help block used to name them anyway, promising a flag that exits 2
/// (M-8). The two claims now come from two constants.
///
/// Asserted against the real binary by
/// `tests/e2e/count.rs::every_capped_command_accepts_limit` and
/// `tests/e2e/count.rs::capped_commands_are_a_subset_of_list_commands`.
pub(crate) const LIMITED_COMMANDS: &[&str] = &[
    "find",
    "lint",
    "tags summary",
    "properties summary",
    "backlinks",
];

/// Render [`LIMITED_COMMANDS`] as a comma-separated phrase for help text.
pub(crate) fn limited_commands_phrase() -> &'static str {
    static PHRASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PHRASE.get_or_init(|| LIMITED_COMMANDS.join(", "))
}

/// Render [`LIST_COMMANDS`] as a comma-separated phrase for prose help text,
/// e.g. `find, lint, tags summary, ...`.
pub(crate) fn list_commands_phrase() -> &'static str {
    static PHRASE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PHRASE.get_or_init(|| LIST_COMMANDS.join(", "))
}

/// The `--count` / `total` unsupported-command error message.
///
/// Deliberately has no `Error: ` prefix baked in — callers route it through
/// [`crate::output::format_error`] so it renders correctly under both
/// `--format text` (which adds the prefix) and `--format json`.
pub(crate) fn count_unsupported_error() -> &'static str {
    static MSG: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MSG.get_or_init(|| {
        format!(
            "--count is only supported for list commands ({})",
            list_commands_phrase()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_lists_every_command() {
        let phrase = list_commands_phrase();
        for cmd in LIST_COMMANDS {
            assert!(phrase.contains(cmd), "{cmd} missing from {phrase}");
        }
    }

    #[test]
    fn limited_commands_are_a_subset_of_list_commands() {
        for cmd in LIMITED_COMMANDS {
            assert!(
                LIST_COMMANDS.contains(cmd),
                "{cmd} is capped but does not emit a total"
            );
        }
    }

    #[test]
    fn phrase_has_no_duplicates() {
        let mut seen: Vec<&str> = LIST_COMMANDS.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "duplicate entry in LIST_COMMANDS");
    }

    #[test]
    fn count_error_names_every_list_command() {
        let msg = count_unsupported_error();
        assert!(msg.starts_with("--count is only supported for list commands ("));
        for cmd in LIST_COMMANDS {
            assert!(msg.contains(cmd), "{cmd} missing from error message: {msg}");
        }
    }
}
