//! Filename-template parsing and matching.
//!
//! Templates are simple strings with `{placeholder}` tokens. The supported
//! placeholders are:
//!
//!   - `{n}`     — a numeric sequence (one or more digits)
//!   - `{slug}`  — a kebab-case slug (letters, digits, `-`, `_`)
//!   - `{date}`  — an ISO 8601 date (YYYY-MM-DD)
//!
//! A template like `iterations/iteration-{n}-{slug}.md` matches paths such as
//! `iterations/iteration-101-bm25.md`. Matching is used by `hyalo lint --fix`
//! to infer a document's `type` when its frontmatter lacks one.
//!
//! The parser is shared with `hyalo types set --filename-template`.

use std::path::Path;

use crate::util::is_iso8601_date;

/// A single parsed segment of a filename template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    /// A literal chunk of the template (matched byte-for-byte).
    Literal(String),
    /// A named placeholder (e.g. `n`, `slug`, `date`).
    Placeholder(Placeholder),
}

/// Supported placeholder kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placeholder {
    /// `{n}` — a numeric sequence (one or more digits).
    N,
    /// `{slug}` — a kebab-case slug (letters, digits, `-`, `_`).
    Slug,
    /// `{date}` — an ISO 8601 date (YYYY-MM-DD).
    Date,
}

impl Placeholder {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "n" => Some(Self::N),
            "slug" => Some(Self::Slug),
            "date" => Some(Self::Date),
            _ => None,
        }
    }
}

/// A parsed filename template.
#[derive(Debug, Clone)]
pub struct FilenameTemplate {
    segments: Vec<Segment>,
}

impl FilenameTemplate {
    /// Parse a template string. Returns an error if a `{...}` placeholder
    /// is unknown or the braces are unbalanced.
    pub fn parse(template: &str) -> Result<Self, ParseError> {
        let mut segments: Vec<Segment> = Vec::new();
        let mut literal = String::new();
        let mut chars = template.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            if c == '{' {
                // Flush any pending literal.
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                // Find the matching `}`.
                let rest = &template[i + 1..];
                let Some(end) = rest.find('}') else {
                    return Err(ParseError::UnbalancedBrace(i));
                };
                let name = &rest[..end];
                let Some(ph) = Placeholder::from_name(name) else {
                    return Err(ParseError::UnknownPlaceholder(name.to_owned()));
                };
                segments.push(Segment::Placeholder(ph));
                // Advance the iterator past the closing brace.
                // We consumed `{`, the placeholder name, and `}` (end+1 bytes after `{`).
                let target = i + 1 + end;
                while let Some(&(next_i, _)) = chars.peek() {
                    if next_i > target {
                        break;
                    }
                    chars.next();
                }
            } else if c == '}' {
                return Err(ParseError::UnbalancedBrace(i));
            } else {
                literal.push(c);
            }
        }

        if !literal.is_empty() {
            segments.push(Segment::Literal(literal));
        }

        Ok(Self { segments })
    }

    /// Convert the template to a glob pattern suitable for use with `--glob`.
    ///
    /// Each placeholder is replaced with the most permissive wildcard that still
    /// constrains the character class:
    ///
    /// - `{n}`    -> `[0-9][0-9]*`  (one or more digits)
    /// - `{slug}` -> `*`             (any characters - glob has no slug-char class)
    /// - `{date}` -> `[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]` (YYYY-MM-DD digits only)
    ///
    /// Literal segments are passed through unchanged.
    pub fn to_glob(&self) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                Segment::Placeholder(Placeholder::N) => out.push_str("[0-9][0-9]*"),
                Segment::Placeholder(Placeholder::Slug) => out.push('*'),
                Segment::Placeholder(Placeholder::Date) => {
                    out.push_str("[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]");
                }
            }
        }
        out
    }

    /// Returns `true` if the given relative path matches this template.
    ///
    /// Path separators are normalized to `/` for matching, so templates can
    /// be written with forward slashes and still match paths produced on
    /// Windows.
    pub fn matches(&self, rel_path: &str) -> bool {
        let normalized: String = rel_path.replace('\\', "/");
        self.match_segments(&normalized, 0, 0)
    }

    /// Returns `true` if the given `Path` matches this template. The path is
    /// converted to a slash-normalized string first.
    pub fn matches_path(&self, rel_path: &Path) -> bool {
        let Some(s) = rel_path.to_str() else {
            return false;
        };
        self.matches(s)
    }

    /// Backtracking match: try to consume `input[pos..]` against `segments[seg_idx..]`.
    fn match_segments(&self, input: &str, seg_idx: usize, pos: usize) -> bool {
        if seg_idx >= self.segments.len() {
            return pos == input.len();
        }
        match &self.segments[seg_idx] {
            Segment::Literal(lit) => {
                if input[pos..].starts_with(lit.as_str()) {
                    self.match_segments(input, seg_idx + 1, pos + lit.len())
                } else {
                    false
                }
            }
            Segment::Placeholder(ph) => {
                // Greedy match that respects the placeholder's character class,
                // with backtracking when the tail fails.
                let slice = &input[pos..];
                let max = max_placeholder_len(*ph, slice);
                let min = min_placeholder_len(*ph);
                if max < min {
                    return false;
                }
                // Try longest first (greedy) to keep common cases fast.
                let mut len = max;
                loop {
                    if self.match_segments(input, seg_idx + 1, pos + len) {
                        return true;
                    }
                    if len == min {
                        return false;
                    }
                    len -= 1;
                }
            }
        }
    }
}

fn is_slug_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// Maximum number of bytes that a placeholder can consume starting at `slice`.
fn max_placeholder_len(ph: Placeholder, slice: &str) -> usize {
    match ph {
        Placeholder::N => slice.bytes().take_while(u8::is_ascii_digit).count(),
        Placeholder::Slug => slice
            .chars()
            .take_while(|c| is_slug_char(*c))
            .map(char::len_utf8)
            .sum(),
        Placeholder::Date => {
            // YYYY-MM-DD is exactly 10 ASCII bytes.
            if slice.len() >= 10 && is_iso8601_date(&slice[..10]) {
                10
            } else {
                0
            }
        }
    }
}

/// Minimum number of bytes required by a placeholder.
fn min_placeholder_len(ph: Placeholder) -> usize {
    match ph {
        Placeholder::N | Placeholder::Slug => 1,
        Placeholder::Date => 10,
    }
}

/// Errors that can occur while parsing a filename template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A `{` with no matching `}` (or a stray `}`).
    UnbalancedBrace(usize),
    /// A placeholder name that isn't recognized.
    UnknownPlaceholder(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnbalancedBrace(pos) => write!(f, "unbalanced brace at byte {pos}"),
            Self::UnknownPlaceholder(name) => {
                write!(
                    f,
                    "unknown placeholder {{{name}}} (supported: n, slug, date)"
                )
            }
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_literal_only() {
        let t = FilenameTemplate::parse("notes/README.md").unwrap();
        assert!(t.matches("notes/README.md"));
        assert!(!t.matches("notes/README2.md"));
    }

    #[test]
    fn parse_and_match_iteration() {
        let t = FilenameTemplate::parse("iterations/iteration-{n}-{slug}.md").unwrap();
        assert!(t.matches("iterations/iteration-101-bm25.md"));
        assert!(t.matches("iterations/iteration-1-a.md"));
        assert!(t.matches("iterations/iteration-42-my-feature.md"));
        assert!(!t.matches("iterations/iteration-.md"));
        assert!(!t.matches("iteration-101-bm25.md"));
        assert!(!t.matches("iterations/other-101-bm25.md"));
    }

    #[test]
    fn date_placeholder_matches_iso8601() {
        let t = FilenameTemplate::parse("journal/{date}.md").unwrap();
        assert!(t.matches("journal/2026-04-13.md"));
        assert!(!t.matches("journal/April-13.md"));
        assert!(!t.matches("journal/2026-4-13.md"));
    }

    #[test]
    fn backslashes_normalized() {
        let t = FilenameTemplate::parse("iterations/iteration-{n}-{slug}.md").unwrap();
        assert!(t.matches(r"iterations\iteration-101-bm25.md"));
    }

    #[test]
    fn unknown_placeholder_errors() {
        let err = FilenameTemplate::parse("x/{foo}.md").unwrap_err();
        assert!(matches!(err, ParseError::UnknownPlaceholder(_)));
    }

    #[test]
    fn unbalanced_brace_errors() {
        assert!(matches!(
            FilenameTemplate::parse("x/{n.md").unwrap_err(),
            ParseError::UnbalancedBrace(_)
        ));
        assert!(matches!(
            FilenameTemplate::parse("x/n}.md").unwrap_err(),
            ParseError::UnbalancedBrace(_)
        ));
    }

    #[test]
    fn backtracking_when_tail_literal_ambiguous() {
        // Slug can greedily consume hyphens; template must still match.
        let t = FilenameTemplate::parse("{slug}-end").unwrap();
        assert!(t.matches("foo-bar-end"));
    }
}
