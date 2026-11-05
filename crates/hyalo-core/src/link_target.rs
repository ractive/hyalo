//! The one canonical split of an authored link target (iter-322, DEC-384).
//!
//! `[[sub/note?x=1#Head|label]]`, `[x](my%20note.md?v=2#intro)` and a
//! frontmatter `[[note#H|label]]` are all `path` + `query` + `fragment` +
//! `alias`. The reader (catalog, case index, discovery) and every writer
//! (`mv`, `links fix`, `links auto`) take their spans from this one function,
//! so a spelling the reader resolves is a spelling a writer can find and
//! rewrite — they used to split on `#` only, on `#`/`?`, and on neither.

use std::borrow::Cow;

/// The four pieces of an authored target, each a slice of the input.
///
/// `query`, `fragment` and `alias` keep their leading `?`, `#` and `|`, so
/// `path` + `query` + `fragment` + `alias` is the input again, byte for byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetParts<'a> {
    /// Everything before the first `?` or `#`, as written (not trimmed).
    pub path: &'a str,
    /// `?query` including the `?`, or `""`.
    pub query: &'a str,
    /// `#fragment` including the `#`, or `""`.
    pub fragment: &'a str,
    /// `|alias` including the `|`, or `""`.
    pub alias: &'a str,
}

impl<'a> TargetParts<'a> {
    /// Split a target that may carry a `|alias` (a wikilink as written).
    ///
    /// The alias is separated first because it may itself hold a `#` or `?`
    /// (`path#frag|Some #hashtag label`).
    #[must_use]
    pub fn split_with_alias(raw: &'a str) -> Self {
        let (before, alias) = match raw.find('|') {
            Some(idx) => (&raw[..idx], &raw[idx..]),
            None => (raw, ""),
        };
        Self {
            alias,
            ..Self::split(before)
        }
    }

    /// Split a target without an alias (`Link::target`, a markdown destination).
    ///
    /// The fragment runs to the end, so a `?` after the `#` belongs to it; a
    /// `?` before the `#` opens the query.
    #[must_use]
    pub fn split(raw: &'a str) -> Self {
        let (head, fragment) = match raw.find('#') {
            Some(idx) => (&raw[..idx], &raw[idx..]),
            None => (raw, ""),
        };
        let (path, query) = match head.find('?') {
            Some(idx) => (&head[..idx], &head[idx..]),
            None => (head, ""),
        };
        Self {
            path,
            query,
            fragment,
            alias: "",
        }
    }

    /// The path as the reader resolves it: trimmed (`[[a #H]]` leaves `a `),
    /// backslashes read as `/`, percent-escapes decoded.
    #[must_use]
    pub fn resolution_path(&self) -> Cow<'a, str> {
        resolution_path(self.path)
    }
}

/// Trim, read `\` as `/` and percent-decode a target's path portion. Borrows
/// when none of that changes anything.
#[must_use]
pub fn resolution_path(path: &str) -> Cow<'_, str> {
    let trimmed = path.trim();
    let mut out: Cow<'_, str> = if trimmed.contains('\\') {
        Cow::Owned(trimmed.replace('\\', "/"))
    } else {
        Cow::Borrowed(trimmed)
    };
    if let Some(decoded) = crate::discovery::percent_decode_path(&out) {
        out = Cow::Owned(decoded);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_every_piece_and_reassembles() {
        for raw in [
            "note",
            "note#H",
            "note?x=1",
            "note?x=1#H",
            "note?#H|a",
            "a#x|Label #tag",
            "a|Label",
            "my%20note.md?v=2#intro",
            "",
        ] {
            let p = TargetParts::split_with_alias(raw);
            assert_eq!(
                format!("{}{}{}{}", p.path, p.query, p.fragment, p.alias),
                raw
            );
        }
        let p = TargetParts::split_with_alias("sub/note?x=1#H?z|al#ias");
        assert_eq!(
            (p.path, p.query, p.fragment, p.alias),
            ("sub/note", "?x=1", "#H?z", "|al#ias")
        );
    }

    #[test]
    fn resolution_path_trims_decodes_and_reads_backslashes() {
        assert_eq!(TargetParts::split("a #H").resolution_path(), "a");
        assert_eq!(
            TargetParts::split("my%20note?q").resolution_path(),
            "my note"
        );
        assert_eq!(resolution_path("sub\\note"), "sub/note");
        assert!(matches!(resolution_path("plain"), Cow::Borrowed(_)));
        assert_eq!(resolution_path("100%done"), "100%done");
    }
}
