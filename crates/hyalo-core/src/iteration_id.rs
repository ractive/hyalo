//! Parsing of `--iteration <ID>` natural-key identifiers.
//!
//! Iteration plans (and other sequence-keyed document types) carry their
//! natural key in the filename, encoded by a type schema's
//! `filename_template` `{n}` placeholder: `iterations/iteration-{n}-{slug}.md`.
//! Agents and humans address them by that key — the bare number — not by the
//! slug. This module owns the ID grammar shared by `hyalo find --iteration`
//! and `hyalo set --iteration` (iter-235, agent-ergonomics review finding 5).
//!
//! Grammar (same shape ralph-loop / preflight already use for branch and plan
//! names): one or more digits, optionally followed by one or more letters.
//! Accepted examples: `206` (bare integer), `01` (zero-padded),
//! `16b` / `16ab` (integer + letter suffix). The letter suffix is how
//! sub-iterations disambiguate within the same base number
//! (`iteration-16a-*`, `iteration-16b-*`). A bare integer is *not* a
//! prefix of a longer number — `16` never matches `160` — because the ID
//! is substituted verbatim into the template's literal structure, not
//! matched as a digit run.

use std::fmt;

/// A parsed `--iteration <ID>` natural-key identifier.
///
/// `raw` is the exact string the caller typed (`"206"`, `"01"`, `"16b"`),
/// substituted verbatim into the type schema's `{n}` placeholder slot when
/// resolving files. `digits` and `suffix` are the parsed components, kept for
/// diagnostics and future sorting/normalisation — resolution itself uses the
/// raw form so zero-padding and letter casing are preserved as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IterationId {
    raw: String,
    digits: String,
    suffix: String,
}

/// Errors from [`parse_iteration_id`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IterationIdParseError {
    /// The identifier was empty (or all whitespace).
    Empty,
    /// The identifier was non-empty but had no leading digit (e.g. `"abc"`).
    /// `raw` echoes the offending input. Kept distinct from [`Self::Empty`]
    /// so the error names what was actually wrong instead of misreporting a
    /// non-empty string as an empty one (BUG-3, review of iter-225/226).
    NotNumeric { raw: String },
    /// The identifier contained characters outside the `digits [letters]`
    /// grammar. `raw` echoes the offending input; `reason` names the rule.
    Invalid { raw: String, reason: &'static str },
}

impl fmt::Display for IterationIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(
                f,
                "iteration ID is empty (expected digits optionally followed by letters, e.g. 206, 01, 16b)"
            ),
            Self::NotNumeric { raw } => write!(
                f,
                "iteration ID '{raw}' is not numeric (expected digits optionally followed by letters, e.g. 206, 01, 16b)"
            ),
            Self::Invalid { raw, reason } => {
                write!(
                    f,
                    "invalid iteration ID '{raw}': {reason} (expected digits optionally followed by letters, e.g. 206, 01, 16b)"
                )
            }
        }
    }
}

impl std::error::Error for IterationIdParseError {}

impl IterationId {
    /// The exact identifier string as typed, e.g. `"206"`, `"01"`, `"16b"`.
    ///
    /// This is what gets substituted into the `{n}` placeholder slot of a
    /// type schema's `filename_template` — verbatim, so a zero-padded ID like
    /// `01` matches `iteration-01-*.md` (not `iteration-1-*.md`).
    #[must_use]
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The leading digit run, e.g. `"206"`, `"01"`, `"16"`.
    #[must_use]
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// The trailing letter suffix, e.g. `""`, `""`, `"b"`. Empty when the ID
    /// is a bare integer.
    #[must_use]
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// `true` when the ID carries a letter suffix (`16b`), `false` when it is
    /// a bare integer (`206`, `01`).
    #[must_use]
    pub fn has_suffix(&self) -> bool {
        !self.suffix.is_empty()
    }
}

impl fmt::Display for IterationId {
    /// The raw identifier string as typed (`206`, `01`, `16b`).
    ///
    /// Used in user-facing messages where the exact form the caller passed
    /// matters (`"iteration 206 matches multiple files"`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// Parse an `--iteration <ID>` natural-key identifier.
///
/// Grammar: `[0-9]+` (one or more digits) followed by `[a-zA-Z]*` (zero or
/// more letters). An empty (or whitespace-only) input is
/// [`IterationIdParseError::Empty`]; a non-empty input with no leading digit
/// (e.g. `"abc"`) is [`IterationIdParseError::NotNumeric`]; any non-digit/
/// non-letter character after the digits is
/// [`IterationIdParseError::Invalid`].
///
/// ```
/// use hyalo_core::iteration_id::{parse_iteration_id, IterationId};
/// let id = parse_iteration_id("16b").unwrap();
/// assert_eq!(id.raw(), "16b");
/// assert_eq!(id.digits(), "16");
/// assert_eq!(id.suffix(), "b");
/// assert!(id.has_suffix());
/// ```
pub fn parse_iteration_id(s: &str) -> Result<IterationId, IterationIdParseError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(IterationIdParseError::Empty);
    }
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return Err(IterationIdParseError::NotNumeric { raw: s.to_owned() });
    }
    let digits = &s[..i];
    let mut j = i;
    while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
        j += 1;
    }
    if j != bytes.len() {
        return Err(IterationIdParseError::Invalid {
            raw: s.to_owned(),
            reason: "contains characters other than digits (optionally followed by letters)",
        });
    }
    let suffix = &s[i..];
    Ok(IterationId {
        raw: s.to_owned(),
        digits: digits.to_owned(),
        suffix: suffix.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_integer() {
        let id = parse_iteration_id("206").unwrap();
        assert_eq!(id.raw(), "206");
        assert_eq!(id.digits(), "206");
        assert_eq!(id.suffix(), "");
        assert!(!id.has_suffix());
    }

    #[test]
    fn zero_padded_integer() {
        let id = parse_iteration_id("01").unwrap();
        assert_eq!(id.raw(), "01");
        assert_eq!(id.digits(), "01");
        assert_eq!(id.suffix(), "");
        assert!(!id.has_suffix());
    }

    #[test]
    fn single_letter_suffix() {
        let id = parse_iteration_id("16b").unwrap();
        assert_eq!(id.raw(), "16b");
        assert_eq!(id.digits(), "16");
        assert_eq!(id.suffix(), "b");
        assert!(id.has_suffix());
    }

    #[test]
    fn multi_letter_suffix() {
        let id = parse_iteration_id("16ab").unwrap();
        assert_eq!(id.raw(), "16ab");
        assert_eq!(id.suffix(), "ab");
    }

    #[test]
    fn uppercase_suffix_preserved() {
        let id = parse_iteration_id("16B").unwrap();
        assert_eq!(id.raw(), "16B");
        assert_eq!(id.suffix(), "B");
    }

    #[test]
    fn surrounding_whitespace_trimmed() {
        let id = parse_iteration_id("  206  ").unwrap();
        assert_eq!(id.raw(), "206");
    }

    #[test]
    fn empty_rejected() {
        assert_eq!(parse_iteration_id(""), Err(IterationIdParseError::Empty));
        assert_eq!(parse_iteration_id("   "), Err(IterationIdParseError::Empty));
    }

    #[test]
    fn no_leading_digit_rejected_as_not_numeric() {
        // A leading letter has no digit run at all → NotNumeric, not Empty
        // (BUG-3, review of iter-225/226): the input is non-empty, so the
        // error must say so rather than misreporting it as an empty ID.
        assert_eq!(
            parse_iteration_id("b16"),
            Err(IterationIdParseError::NotNumeric {
                raw: "b16".to_owned()
            })
        );
        assert_eq!(
            parse_iteration_id("abc"),
            Err(IterationIdParseError::NotNumeric {
                raw: "abc".to_owned()
            })
        );
        let err = parse_iteration_id("abc").unwrap_err();
        assert!(err.to_string().contains("'abc' is not numeric"), "{err}");
    }

    #[test]
    fn punctuation_rejected() {
        let err = parse_iteration_id("16-b").unwrap_err();
        assert!(matches!(err, IterationIdParseError::Invalid { .. }));
        assert_eq!(
            err.to_string(),
            parse_iteration_id("16-b").unwrap_err().to_string()
        );
        assert!(err.to_string().contains("16-b"));
        assert!(
            err.to_string()
                .contains("digits optionally followed by letters")
        );
    }

    #[test]
    fn dash_rejected() {
        assert!(matches!(
            parse_iteration_id("1-6"),
            Err(IterationIdParseError::Invalid { .. })
        ));
    }

    #[test]
    fn decimal_rejected() {
        assert!(matches!(
            parse_iteration_id("1.6"),
            Err(IterationIdParseError::Invalid { .. })
        ));
    }

    #[test]
    fn error_display_is_human_readable() {
        let err = parse_iteration_id("16-b").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("'16-b'"), "should echo the bad input: {msg}");
        assert!(
            msg.contains("expected digits optionally followed by letters"),
            "should name the grammar: {msg}"
        );
    }
}
