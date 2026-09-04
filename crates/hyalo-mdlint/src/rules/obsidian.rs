//! Obsidian-grammar guards for the stock `MD*` rules (iteration 263).
//!
//! The upstream `mdbook-lint-rulesets` rules are written for mdBook, whose
//! Markdown has no `#tag` grammar and whose link scanner is line-based. On a
//! real Obsidian vault that produces *destructive* autofixes, so hyalo
//! post-processes the upstream diagnostics rather than forking the rules:
//!
//! - [`is_obsidian_tag_line`] — MD018 (`no-missing-space-atx`) reads a
//!   line-leading `#todo` as a malformed heading and "fixes" it to `# todo`,
//!   silently turning a tag into an H1 (dogfood v0.22.0 BUG-3, 162 proposals
//!   on the Obsidian Hub vault).
//! - [`url_is_inside_link_markup`] — MD034 (`no-bare-urls`) skips `[...](...)`
//!   with a character scan that cannot see the nested brackets in the badge
//!   idiom `[![](img.png)](https://…)`, so it wraps a *link destination* in
//!   angle brackets and breaks the link (BUG-9, 209 proposals).
//! - [`link_text_is_image`] — MD042 (`no-empty-links`) collects only `Text`
//!   and `Code` children, so a link whose text is an image reads as empty
//!   (BUG-9, 55 hits).
//!
//! Everything here works on the *body* text with the same 1-based line
//! numbering the upstream rules use, so a diagnostic can be checked without
//! re-parsing the document.

use std::collections::HashMap;

/// Whether `token` is a valid Obsidian tag body (the text after the `#`).
///
/// Obsidian's grammar (DEC-271): a tag is made of letters, digits, `_`, `-`,
/// `/` or non-ASCII word characters, and must contain at least one character
/// that is not an ASCII digit — `#1984` is a number, not a tag. An empty
/// token is not a tag.
///
/// Kept public so a future dedicated tag rule (`find --tag` parity, tag
/// linting) reuses exactly the grammar MD018 is exempted against instead of
/// growing a second, subtly different definition.
#[must_use]
pub fn is_obsidian_tag_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    let mut has_non_digit = false;
    for c in token.chars() {
        let ok = c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/') || {
            // Non-ASCII word characters (`#日本語`, `#übung`). Anything
            // ASCII that got here is punctuation and disqualifies the token.
            !c.is_ascii() && (c.is_alphanumeric() || c == '\u{200d}')
        };
        if !ok {
            return false;
        }
        if !c.is_ascii_digit() {
            has_non_digit = true;
        }
    }
    has_non_digit
}

/// Whether `line` is an Obsidian tag line rather than a malformed ATX heading.
///
/// True for a **single** leading `#` immediately followed by a valid tag
/// token — `##todo` is not a tag in Obsidian, and `## Heading` is a real
/// heading. Trailing prose after the tag is fine (`#todo call the vet` is
/// still a tag line), with one deliberate exception, which is the whole
/// difficulty of the rule: `#Heading typo` parses as the tag `#Heading`
/// followed by prose, and is also exactly what a missing space after a `#`
/// looks like. DEC-271 breaks the tie by capitalization — a token that is a
/// **plain capitalized ASCII word** (initial upper-case letter, then letters
/// only: no digit, `-`, `_`, `/`, no non-ASCII) *and* is followed by more
/// text on the line reads as a heading and stays flagged. `#Project/alpha
/// notes`, `#todo call the vet` and a bare `#Someday` are tags.
///
/// The bias is deliberate: a missed heading typo leaves the file untouched,
/// while a mis-fixed tag silently rewrites the author's content.
///
/// Leading indentation is ignored, matching MD018's own `trim_start`.
#[must_use]
pub fn is_obsidian_tag_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix('#') else {
        return false;
    };
    // A second `#` means an ATX heading (or nothing Obsidian recognises).
    if rest.starts_with('#') {
        return false;
    }
    let token = rest
        .split(|c: char| c.is_whitespace())
        .next()
        .unwrap_or_default();
    if !is_obsidian_tag_token(token) {
        return false;
    }
    let has_trailing_text = !rest[token.len()..].trim().is_empty();
    !(has_trailing_text && looks_like_a_heading_word(token))
}

/// Whether `token` reads as the first word of a prose heading: an initial
/// ASCII upper-case letter followed by ASCII letters only.
///
/// Anything a tag namespace would carry — a digit, `-`, `_`, `/`, or a
/// non-ASCII character — disqualifies it, because heading prose does not
/// start with `Project/alpha` but tags routinely do.
fn looks_like_a_heading_word(token: &str) -> bool {
    let mut chars = token.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_uppercase() && chars.all(|c| c.is_ascii_alphabetic())
}

/// Whether the URL MD034 flagged at `column` (1-based, Unicode-scalar) on
/// `line` actually sits inside link markup — a markdown link/image
/// destination, an autolink, or a wikilink — rather than in plain prose.
///
/// Deliberately does **not** reuse `hyalo_core::links::inert_link_zones`.
/// That map answers a different question ("may the auto-linker inject
/// `[[…]]` here?") and matches a link label with the *first* closing `]`, so
/// on `[![](img.png)](url)` it ends the label at the image's `]` and the
/// destination falls outside every zone — precisely the shape BUG-9 is
/// about. [`link_markup_spans`] matches brackets by nesting instead.
#[must_use]
pub fn url_is_inside_link_markup(line: &str, column: usize) -> bool {
    let Some(offset) = scalar_col_to_byte_offset(line, column) else {
        // An out-of-range column means the diagnostic and the line do not
        // match up; suppressing on a guess would hide real findings.
        return false;
    };
    link_markup_spans(line)
        .into_iter()
        .any(|(start, end)| start < offset && offset < end)
}

/// Byte spans of the link markup on `line`: markdown links and images
/// (`[label](dest)`, `![alt](src)`, including the reference forms
/// `[text][ref]`), Obsidian wikilinks and embeds (`[[…]]`, `![[…]]`), and
/// autolinks / raw HTML (`<…>`).
///
/// Brackets are matched by **nesting**, so the label of `[![](img)](url)` is
/// the whole `[![](img)]` and the span covers the destination too. Escapes
/// are not honoured (`\[` counts as a bracket) — the map is used to decide
/// "is this position inside markup", where treating bracket-shaped text as
/// markup errs toward reporting less, which is the safe direction for an
/// autofix.
fn link_markup_spans(line: &str) -> Vec<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut close_of: HashMap<usize, usize> = HashMap::new();
    if bytes.contains(&b'[') {
        let mut stack: Vec<usize> = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'[' => stack.push(i),
                b']' => {
                    if let Some(open) = stack.pop() {
                        close_of.insert(open, i);
                    }
                }
                _ => {}
            }
        }
    }

    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                if let Some(&close) = close_of.get(&i) {
                    // `![alt](src)` — start the span at the `!` so the whole
                    // image is one region.
                    let start = if i > 0 && bytes[i - 1] == b'!' {
                        i - 1
                    } else {
                        i
                    };
                    let mut end = close + 1;
                    match bytes.get(end) {
                        // Inline destination `(...)`, nesting-aware so a
                        // parenthesised URL does not truncate the span.
                        Some(&b'(') => {
                            if let Some(paren) = matching_paren(bytes, end) {
                                end = paren + 1;
                            }
                        }
                        // Full reference form `[text][ref]`.
                        Some(&b'[') => {
                            if let Some(&ref_close) = close_of.get(&end) {
                                end = ref_close + 1;
                            }
                        }
                        _ => {}
                    }
                    spans.push((start, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            b'<' => {
                // Autolink `<https://…>` or a raw HTML tag: either way the
                // URL inside it is not bare.
                if let Some(rel) = line[i + 1..].find('>') {
                    let end = i + 1 + rel + 1;
                    spans.push((i, end));
                    i = end;
                    continue;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    spans
}

/// Byte index of the `)` closing the `(` at `open`, counting nesting.
///
/// `None` when the parenthesis never closes on this line.
fn matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                // `depth` is always ≥ 1 here for a caller that passes the
                // index of a `(`; the guard keeps a misuse from underflowing.
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the MD042 diagnostic reported at `column` (1-based,
/// Unicode-scalar) on `line` is really the badge idiom
/// `[![alt](img)](url)` — an image used as a link's text.
///
/// MD042 emits *two* diagnostics for that one construct, and both are
/// suppressed here:
///
/// - on the enclosing **link**, "Found empty link", because the rule only
///   concatenates `Text`/`Code` descendants and so never sees the image;
/// - on the **image**, "Found image with empty alt text", because in this
///   position the alt text would duplicate the link it wraps — the link, not
///   the image, is what a reader follows.
///
/// A standalone `![](img.png)` still gets the alt-text warning, and a
/// genuinely empty `[](url)` or `[ ](url)` still gets the empty-link error:
/// neither has an image at the start of a link label.
#[must_use]
pub fn link_text_is_image(line: &str, column: usize) -> bool {
    let Some(offset) = scalar_col_to_byte_offset(line, column) else {
        return false;
    };
    // The link diagnostic points at the `[` opening the label. Covers both
    // `![alt](img)` and the Obsidian embed `![[img]]`.
    if let Some(label) = line[offset..].strip_prefix('[')
        && label.trim_start().starts_with("![")
    {
        return true;
    }
    // The image diagnostic points at the `!`; it is link text exactly when a
    // link label opens immediately before it.
    line[offset..].starts_with("![") && line[..offset].ends_with('[')
}

/// 1-based Unicode-scalar column → byte offset within `line`.
///
/// `None` when the column is past the end of the line (or zero), which the
/// callers treat as "cannot judge, keep the diagnostic".
fn scalar_col_to_byte_offset(line: &str, column: usize) -> Option<usize> {
    let idx = column.checked_sub(1)?;
    if idx == 0 {
        return Some(0);
    }
    line.char_indices().nth(idx).map(|(byte, _)| byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_tokens_follow_obsidian_grammar() {
        for good in ["todo", "todo/next", "2024-goals", "日本語", "a", "x_y-z/w"] {
            assert!(is_obsidian_tag_token(good), "{good} should be a tag");
        }
        for bad in ["", "1", "2024", "!bang", "a b", "a.b", "a,b", "a#b"] {
            assert!(!is_obsidian_tag_token(bad), "{bad} should not be a tag");
        }
    }

    #[test]
    fn tag_lines_are_recognised() {
        for good in [
            "#todo",
            "#todo/next",
            "#2024-goals",
            "#日本語",
            "  #todo",
            "#todo trailing text",
            "#todo\t and more",
            // Capitalized, but namespaced or bare — still a tag.
            "#Project/alpha notes",
            "#Someday",
        ] {
            assert!(is_obsidian_tag_line(good), "{good:?} should be a tag line");
        }
        for bad in [
            "##todo",
            "#1",
            "#2024 review",
            "#!bang",
            "#Heading typo",
            "#Standalone typo",
            "# todo",
            "#",
            "text #todo",
            "###",
        ] {
            assert!(
                !is_obsidian_tag_line(bad),
                "{bad:?} should not be a tag line"
            );
        }
    }

    #[test]
    fn urls_inside_link_destinations_are_markup() {
        let line = "[![](img.png)](https://example.com/y.png)";
        let col = line.find("https").expect("url present") + 1;
        assert!(url_is_inside_link_markup(line, col));
    }

    #[test]
    fn a_bare_url_after_a_link_is_still_bare() {
        let line = "see [docs](https://a.example/) and https://b.example/ too";
        let bare = line.rfind("https://b").expect("bare url present") + 1;
        assert!(!url_is_inside_link_markup(line, bare));
        let inside = line.find("https://a").expect("dest present") + 1;
        assert!(url_is_inside_link_markup(line, inside));
    }

    #[test]
    fn image_link_text_is_not_empty() {
        let line = "[![](img.png)](https://example.com/y.png)";
        assert!(link_text_is_image(line, 1));
        assert!(!link_text_is_image("[](https://example.com/)", 1));
        assert!(!link_text_is_image("[ ](https://example.com/)", 1));
        assert!(link_text_is_image("prefix [![alt](i.png)](u)", 8));
        // The image half of the same construct (column of the `!`).
        assert!(link_text_is_image(line, 2));
        // A standalone image keeps its empty-alt warning.
        assert!(!link_text_is_image("![](img.png)", 1));
    }

    #[test]
    fn multibyte_columns_resolve_to_the_right_offset() {
        let line = "日本語 [![](i.png)](https://example.com/)";
        let col = line[..line.find("https").expect("url")].chars().count() + 1;
        assert!(url_is_inside_link_markup(line, col));
    }
}
