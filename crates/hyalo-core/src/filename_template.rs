//! Filename-template parsing and matching.
//!
//! Templates are simple strings with `{placeholder}` tokens. The supported
//! placeholders are:
//!
//!   - `{n}`     — a numeric sequence (one or more digits)
//!   - `{n:0W}`  — a zero-padded numeric sequence of minimum width `W` (e.g.
//!     `{n:04}` matches/produces `0001`, `0042`, `1234`); more than `W` digits
//!     are still accepted so the sequence can grow past the pad width.
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
    /// `{n}` — a numeric sequence (one or more digits). The `pad` field carries
    /// the minimum zero-pad width requested via `{n:0W}` (0 == no padding).
    N { pad: usize },
    /// `{slug}` — a kebab-case slug (letters, digits, `-`, `_`).
    Slug,
    /// `{date}` — an ISO 8601 date (YYYY-MM-DD).
    Date,
}

impl Placeholder {
    /// Parse a placeholder name (the text between `{` and `}`). Recognizes the
    /// bare forms `n`/`slug`/`date` plus the padded-number form `n:0W` (e.g.
    /// `n:04`), where `W` is a positive decimal width.
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "n" => Some(Self::N { pad: 0 }),
            "slug" => Some(Self::Slug),
            "date" => Some(Self::Date),
            _ => {
                // Padded-number form: `n:0W` (leading zero is mandatory so the
                // intent — zero-padding — is explicit and unambiguous).
                let spec = name.strip_prefix("n:")?;
                let width = spec.strip_prefix('0')?;
                let pad: usize = width.parse().ok()?;
                // `n:0` (width 0) is meaningless; require at least width 1.
                if pad == 0 {
                    None
                } else {
                    Some(Self::N { pad })
                }
            }
        }
    }
}

/// Render a sequence number for an `{n}` / `{n:0W}` placeholder: zero-padded to
/// `pad` digits (no padding when `pad == 0`). Reusable by callers that expand a
/// template into a concrete filename (e.g. `hyalo new` auto-numbering).
#[must_use]
pub fn render_number(value: u64, pad: usize) -> String {
    format!("{value:0pad$}")
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
                Segment::Placeholder(Placeholder::N { .. }) => out.push_str("[0-9][0-9]*"),
                Segment::Placeholder(Placeholder::Slug) => out.push('*'),
                Segment::Placeholder(Placeholder::Date) => {
                    out.push_str("[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]");
                }
            }
        }
        out
    }

    /// Returns `true` when the template contains at least one numeric `{n}` or
    /// `{n:0W}` placeholder.
    ///
    /// Used by `--iteration <ID>` resolution to decide which type schemas are
    /// addressable by a natural key: a template like
    /// `iterations/iteration-{n}-{slug}.md` carries an `{n}` slot an iteration
    /// ID can be substituted into, while a purely literal template
    /// (`notes/README.md`) or one with only `{slug}` / `{date}` is not.
    #[must_use]
    pub fn has_n_placeholder(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, Segment::Placeholder(Placeholder::N { .. })))
    }

    /// Build a glob that matches files carrying a specific sequence ID.
    ///
    /// `id` is substituted verbatim into every `{n}` / `{n:0W}` placeholder
    /// slot, and every other placeholder (`{slug}`, `{date}`) becomes `*`, so a
    /// template `iterations/iteration-{n}-{slug}.md` combined with the ID
    /// `"206"` yields `iterations/iteration-206-*.md` — exactly the files for
    /// iteration 206 regardless of slug.
    ///
    /// The ID is inserted as written (not normalised): a bare `01` matches
    /// `iteration-01-*` (zero-padded files), not `iteration-1-*`. A
    /// letter-suffixed ID like `16b` matches only `iteration-16b-*`; a bare
    /// integer like `16` matches `iteration-16-*` and *not* `iteration-16b-*`
    /// (the suffix is a separate identifier — see [`crate::iteration_id`]).
    ///
    /// This is the specific-key counterpart to [`to_glob`]'s permissive
    /// "match any value" expansion; it does not require the ID itself to
    /// satisfy the `{n}` character class, because the goal is matching real
    /// files that may carry letter suffixes the `{n}` matcher itself rejects
    /// (`iteration-195a-…` is a real iteration file even though `195a` is not
    /// a pure digit run).
    #[must_use]
    pub fn to_glob_for_id(&self, id: &str) -> String {
        let mut out = String::new();
        for seg in &self.segments {
            match seg {
                Segment::Literal(s) => out.push_str(s),
                Segment::Placeholder(Placeholder::N { .. }) => out.push_str(id),
                // `{slug}` and `{date}` both become `*` — any value is an
                // acceptable match once the ID pins the `{n}` slot.
                Segment::Placeholder(Placeholder::Slug | Placeholder::Date) => out.push('*'),
            }
        }
        out
    }

    /// Zero-pad width the template requests for the `{n}` slot (`{n:04}`
    /// → `Some(4)`), if any `{n}` placeholder carries one.
    #[must_use]
    pub fn n_pad_width(&self) -> Option<usize> {
        self.segments.iter().find_map(|seg| match seg {
            Segment::Placeholder(Placeholder::N { pad }) => (*pad > 0).then_some(*pad),
            _ => None,
        })
    }

    /// All glob patterns that can address iteration `id` through this
    /// template, most specific first (UX-2, dogfood v0.20.0).
    ///
    /// The primary glob is [`to_glob_for_id`] verbatim. Two fallback
    /// families follow, each only when it can differ from what precedes
    /// it:
    ///
    /// - a **zero-padded** variant — a vault whose historical files are
    ///   `iteration-02-links.md` while the template renders `iteration-{n}`
    ///   (unpadded) leaves `--iteration 2` unmatched even though the
    ///   natural key is obvious. Padding width is `max(2, template pad)`;
    ///   only added when `id` is a bare unpadded integer.
    /// - a **recursive** variant per glob produced above — the template's
    ///   directory becomes `**`, so an archived `iterations/done/
    ///   iteration-02-links.md` is still reachable when the live template
    ///   points at `iterations/`. Only added when the template has
    ///   directory components.
    ///
    /// Consumers union these globs, so order affects only diagnostics
    /// ("resolved globs: …") — never what matches.
    #[must_use]
    pub fn to_glob_variants_for_id(&self, id: &str) -> Vec<String> {
        let mut globs = vec![self.to_glob_for_id(id)];
        let unpadded_int = !id.is_empty()
            && id.starts_with(|c: char| c.is_ascii_digit() && c != '0')
            && id.chars().all(|c| c.is_ascii_digit());
        if unpadded_int {
            let width = self.n_pad_width().map_or(2, |w| w.max(2));
            if width > id.len() {
                let padded = format!("{id:0>width$}");
                let padded_glob = self.to_glob_for_id(&padded);
                if !globs.contains(&padded_glob) {
                    globs.push(padded_glob);
                }
            }
        }
        let mut recursive = Vec::new();
        for g in &globs {
            if let Some((_, base)) = g.rsplit_once('/') {
                let rg = format!("**/{base}");
                if !globs.contains(&rg) && !recursive.contains(&rg) {
                    recursive.push(rg);
                }
            }
        }
        globs.extend(recursive);
        globs
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
        Placeholder::N { .. } => slice.bytes().take_while(u8::is_ascii_digit).count(),
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
        // A padded `{n:0W}` still matches as few as one digit — the pad width is
        // a *rendering* minimum, not a match floor, so an existing `1-x.md` next
        // to `0002-x.md` is still recognized as the same type.
        Placeholder::N { .. } | Placeholder::Slug => 1,
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
                    "unknown placeholder {{{name}}} (supported: n, n:0W, slug, date)"
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

    #[test]
    fn padded_number_parses_and_matches() {
        let t = FilenameTemplate::parse("decisions/{n:04}-{slug}.md").unwrap();
        // Exactly the pad width, more, and fewer digits all match (pad is a
        // rendering minimum, not a match floor).
        assert!(t.matches("decisions/0001-use-postgres.md"));
        assert!(t.matches("decisions/12345-big-number.md"));
        assert!(t.matches("decisions/1-terse.md"));
        assert!(!t.matches("decisions/-no-number.md"));
    }

    #[test]
    fn padded_number_glob_is_permissive() {
        let t = FilenameTemplate::parse("decisions/{n:04}-{slug}.md").unwrap();
        // The glob for a padded number is the same permissive digit-run form.
        assert_eq!(t.to_glob(), "decisions/[0-9][0-9]*-*.md");
    }

    #[test]
    fn has_n_placeholder_detects_numeric_slot() {
        let t = FilenameTemplate::parse("iterations/iteration-{n}-{slug}.md").unwrap();
        assert!(t.has_n_placeholder());
        let t = FilenameTemplate::parse("decisions/{n:04}-{slug}.md").unwrap();
        assert!(t.has_n_placeholder());
    }

    #[test]
    fn has_n_placeholder_false_for_literal_only_or_other_placeholders() {
        assert!(
            !FilenameTemplate::parse("notes/README.md")
                .unwrap()
                .has_n_placeholder()
        );
        assert!(
            !FilenameTemplate::parse("journal/{date}.md")
                .unwrap()
                .has_n_placeholder()
        );
        assert!(
            !FilenameTemplate::parse("notes/{slug}.md")
                .unwrap()
                .has_n_placeholder()
        );
    }

    #[test]
    fn to_glob_for_id_substitutes_id_and_star() {
        let t = FilenameTemplate::parse("iterations/iteration-{n}-{slug}.md").unwrap();
        // Bare integer matches only the same-number files.
        assert_eq!(t.to_glob_for_id("206"), "iterations/iteration-206-*.md");
        // Zero-padded ID is inserted verbatim.
        assert_eq!(t.to_glob_for_id("01"), "iterations/iteration-01-*.md");
        // Letter-suffixed ID matches only the suffixed files.
        assert_eq!(t.to_glob_for_id("16b"), "iterations/iteration-16b-*.md");
    }

    #[test]
    fn to_glob_for_id_for_padded_template_inserts_raw_id() {
        let t = FilenameTemplate::parse("decisions/{n:04}-{slug}.md").unwrap();
        // The ID is inserted verbatim — the pad width is a rendering concern,
        // not applied here, so a vault using padded filenames gets a matching
        // glob only when the caller passes the padded form (e.g. "0206").
        assert_eq!(t.to_glob_for_id("0206"), "decisions/0206-*.md");
        assert_eq!(t.to_glob_for_id("206"), "decisions/206-*.md");
    }

    #[test]
    fn to_glob_for_id_date_placeholder_becomes_star() {
        let t = FilenameTemplate::parse("journal/{date}-{n}.md").unwrap();
        assert_eq!(t.to_glob_for_id("42"), "journal/*-42.md");
    }

    #[test]
    fn to_glob_variants_for_id_pads_and_recurses() {
        let t = FilenameTemplate::parse("iterations/iteration-{n}-{slug}.md").unwrap();
        // Raw form first, then zero-padded, then recursive fallbacks.
        assert_eq!(
            t.to_glob_variants_for_id("2"),
            vec![
                "iterations/iteration-2-*.md".to_owned(),
                "iterations/iteration-02-*.md".to_owned(),
                "**/iteration-2-*.md".to_owned(),
                "**/iteration-02-*.md".to_owned(),
            ]
        );
        // Already-wide ID: no padded duplicate.
        assert_eq!(
            t.to_glob_variants_for_id("206"),
            vec![
                "iterations/iteration-206-*.md".to_owned(),
                "**/iteration-206-*.md".to_owned(),
            ]
        );
        // Letter-suffixed ID: no padding, but recursion still applies.
        assert_eq!(
            t.to_glob_variants_for_id("16b"),
            vec![
                "iterations/iteration-16b-*.md".to_owned(),
                "**/iteration-16b-*.md".to_owned(),
            ]
        );
    }

    #[test]
    fn to_glob_variants_for_id_respects_template_pad_width() {
        let t = FilenameTemplate::parse("decisions/{n:04}-{slug}.md").unwrap();
        assert_eq!(
            t.to_glob_variants_for_id("16"),
            vec![
                "decisions/16-*.md".to_owned(),
                "decisions/0016-*.md".to_owned(),
                "**/16-*.md".to_owned(),
                "**/0016-*.md".to_owned(),
            ]
        );
        // A template with no directory component gains no recursive variant.
        let flat = FilenameTemplate::parse("note-{n}.md").unwrap();
        assert_eq!(
            flat.to_glob_variants_for_id("7"),
            vec!["note-7.md".to_owned(), "note-07.md".to_owned()]
        );
    }

    #[test]
    fn render_number_zero_pads() {
        assert_eq!(render_number(1, 4), "0001");
        assert_eq!(render_number(42, 4), "0042");
        assert_eq!(render_number(12345, 4), "12345");
        assert_eq!(render_number(7, 0), "7");
    }

    #[test]
    fn bad_pad_specs_are_unknown_placeholders() {
        // Missing the mandatory leading zero, non-numeric width, and width 0 are
        // all rejected (they'd be ambiguous or meaningless).
        for bad in ["{n:4}.md", "{n:0x}.md", "{n:0}.md", "{n:}.md"] {
            assert!(
                matches!(
                    FilenameTemplate::parse(bad).unwrap_err(),
                    ParseError::UnknownPlaceholder(_)
                ),
                "expected {bad} to be rejected"
            );
        }
    }
}
