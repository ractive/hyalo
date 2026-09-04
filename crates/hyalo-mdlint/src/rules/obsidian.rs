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

use hyalo_core::links::inert_link_zones;

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
/// True only for a **single** leading `#` immediately followed by a valid tag
/// token (`##todo` is not a tag in Obsidian, and `## Heading` is a real
/// heading). Trailing prose after the tag is fine — `#todo call the vet` is
/// still a tag line, which is exactly the shape MD018 would rewrite into an
/// H1 sentence.
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
    is_obsidian_tag_token(token)
}

/// Whether the URL MD034 flagged at `column` (1-based, Unicode-scalar) on
/// `line` actually sits inside link markup — a markdown link/image
/// destination, an autolink, or a wikilink — rather than in plain prose.
///
/// Reuses [`inert_link_zones`], the same span map the auto-linker uses to
/// decide where it must not inject `[[…]]`, so "text that is already markup"
/// has one definition across the codebase. A *bare* URL is itself an inert
/// zone there, and such a zone starts exactly at the URL; markup that
/// *contains* a URL always opens earlier (`[`, `<`, `!`). So the test is
/// simply: is the offset covered by a zone that began before it?
#[must_use]
pub fn url_is_inside_link_markup(line: &str, column: usize) -> bool {
    let Some(offset) = scalar_col_to_byte_offset(line, column) else {
        // An out-of-range column means the diagnostic and the line do not
        // match up; suppressing on a guess would hide real findings.
        return false;
    };
    inert_link_zones(line)
        .into_iter()
        .any(|(start, end)| start < offset && offset < end)
}

/// Whether the link starting at `column` (1-based, Unicode-scalar) on `line`
/// has an image for its link text — `[![alt](img)](url)` or `[![[img]]](url)`.
///
/// MD042 calls such a link empty because it only concatenates `Text`/`Code`
/// descendants. The badge idiom is deliberate markup, not a broken link, so
/// the diagnostic is suppressed; a genuinely empty `[](url)` or `[ ](url)`
/// still fires because neither starts its label with an image.
#[must_use]
pub fn link_text_is_image(line: &str, column: usize) -> bool {
    let Some(offset) = scalar_col_to_byte_offset(line, column) else {
        return false;
    };
    let rest = &line[offset..];
    let Some(label) = rest.strip_prefix('[') else {
        return false;
    };
    // Covers both `![alt](img)` and the Obsidian embed `![[img]]`.
    label.trim_start().starts_with("![")
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
        ] {
            assert!(is_obsidian_tag_line(good), "{good:?} should be a tag line");
        }
        for bad in [
            "##todo",
            "#1",
            "#!bang",
            "#Heading typo",
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
    }

    #[test]
    fn multibyte_columns_resolve_to_the_right_offset() {
        let line = "日本語 [![](i.png)](https://example.com/)";
        let col = line[..line.find("https").expect("url")].chars().count() + 1;
        assert!(url_is_inside_link_markup(line, col));
    }
}
