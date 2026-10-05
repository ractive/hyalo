//! Tokenizer v4 (iteration 304, DEC-336) and the field-aware document
//! token stream BM25F scores (DEC-337).
//!
//! Pipeline per word, identical for queries and documents:
//!
//! 1. **Words.** A word is a maximal run of letters, digits and combining
//!    marks, where a single `_` or `-` between two such characters joins the
//!    run (`get_user_name`, `error-handling`).
//! 2. **CJK.** A word holding a scriptio-continua character (CJK ideographs,
//!    Hiragana/Katakana, Hangul) keeps the v2 pipeline: CJK segments become
//!    overlapping character bigrams, other segments a folded, stemmed token.
//! 3. **Diacritic folding.** Other words are NFKD-decomposed and their
//!    combining marks dropped before anything else (`résumé` → `resume`,
//!    `Nöte` → `Note`, `ﬁle` → `file`).
//! 4. **Identifier splitting.** The folded word splits at `_`/`-` and at
//!    camelCase boundaries (a lowercase letter followed by an uppercase one,
//!    and an uppercase run followed by an uppercase letter that starts a
//!    lowercase run of two or more: `HTMLParser` → `HTML` `Parser`, while
//!    `APIs` stays whole). Digits never split (`utf8`, `sha256sum`). A word
//!    with two or more parts emits the joined whole first, then each part:
//!    `getUserName` → `getusernam get user name`. A single-part word emits
//!    one token, exactly as v3 did.
//! 5. Each token is lowercased and stemmed with the document language.

use std::borrow::Cow;
use std::sync::OnceLock;

use rust_stemmers::Stemmer;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Bumped whenever [`tokenize`]'s output for the same input text changes in a
/// way that makes previously persisted tokens stale. Readers compare it with
/// [`crate::index::IndexEntry::bm25_tokenizer_version`] and the persisted
/// index's own stamp, and re-tokenize on mismatch (DEC-094 / F-2).
///
/// v1: whole-alphanumeric-run tokens. v2: scriptio-continua runs become
/// overlapping character bigrams. v3 (iter-243 BUG-4): the indexed raw body
/// includes code-fence delimiter and `%%` lines, matching the disk path.
/// v4 (iter-304, DEC-336): diacritic folding, identifier splitting (whole
/// plus parts), the optional `[search] code_blocks = "skip"` filter, and a
/// field-aware stream (title, headings, tags) for BM25F.
pub const TOKENIZER_VERSION: u32 = 4;

// ---------------------------------------------------------------------------
// Search settings (process-global, like `discovery::set_link_aliases`)
// ---------------------------------------------------------------------------

/// BM25F field weights from `[search.weights]` (DEC-337).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldWeights {
    /// Weight of a term occurrence in the promoted title.
    pub title: f64,
    /// Weight of a term occurrence on a heading line.
    pub headings: f64,
    /// Weight of a term in a frontmatter tag or alias.
    pub tags: f64,
    /// Weight of every other body occurrence.
    pub body: f64,
}

impl Default for FieldWeights {
    fn default() -> Self {
        Self {
            title: 3.0,
            headings: 2.0,
            tags: 2.0,
            body: 1.0,
        }
    }
}

/// Effective `[search]` settings that change tokenization or scoring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchSettings {
    /// `[search] code_blocks = "skip"`: fenced code blocks (delimiters
    /// included) contribute no tokens. Inline code is always indexed.
    pub skip_code_blocks: bool,
    /// BM25F field weights.
    pub weights: FieldWeights,
    /// `[search] proximity_bonus` (DEC-338); `0` disables the bonus.
    pub proximity_bonus: f64,
}

/// Default `[search] proximity_bonus`.
pub const DEFAULT_PROXIMITY_BONUS: f64 = 0.5;

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            skip_code_blocks: false,
            weights: FieldWeights::default(),
            proximity_bonus: DEFAULT_PROXIMITY_BONUS,
        }
    }
}

static SEARCH_SETTINGS: OnceLock<SearchSettings> = OnceLock::new();

/// Install the effective `[search]` settings for this process.
///
/// Idempotent-once, like [`crate::discovery::set_link_aliases`]: the CLI calls
/// it right after config resolution, before any command runs, which keeps the
/// settings off the signature of every scanner, index builder and scorer.
pub fn set_search_settings(settings: SearchSettings) {
    let _ = SEARCH_SETTINGS.set(settings);
}

/// The `[search]` settings in effect (defaults when none were installed).
#[must_use]
pub fn search_settings() -> SearchSettings {
    SEARCH_SETTINGS.get().copied().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Word splitting
// ---------------------------------------------------------------------------

/// Returns `true` for codepoints from scripts conventionally written without
/// spaces between words ("scriptio continua"): CJK ideographs (including
/// compatibility/extension blocks), Hiragana, Katakana, and Hangul syllables.
pub(crate) fn is_scriptio_continua(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF   // Hiragana + Katakana
        | 0x31F0..=0x31FF // Katakana Phonetic Extensions
        | 0x2E80..=0x2EFF // CJK Radicals Supplement
        | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xAC00..=0xD7A3 // Hangul Syllables
        | 0x20000..=0x2A6DF // CJK Unified Ideographs Extension B
    )
}

fn is_word_char(c: char) -> bool {
    if c.is_ascii() {
        c.is_ascii_alphanumeric()
    } else {
        c.is_alphanumeric() || is_combining_mark(c)
    }
}

fn is_joiner(c: char) -> bool {
    c == '_' || c == '-'
}

/// Call `f` with every word of `text` (see the module docs).
fn for_each_word(text: &str, mut f: impl FnMut(&str)) {
    let mut start: Option<usize> = None;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if is_word_char(c) {
            if start.is_none() {
                start = Some(i);
            }
            continue;
        }
        if let Some(s) = start {
            if is_joiner(c) && chars.peek().is_some_and(|&(_, next)| is_word_char(next)) {
                continue;
            }
            f(&text[s..i]);
            start = None;
        }
    }
    if let Some(s) = start {
        f(&text[s..]);
    }
}

/// NFKD-decompose `word` and drop its combining marks.
fn fold(word: &str) -> String {
    word.nfkd().filter(|c| !is_combining_mark(*c)).collect()
}

/// Accent-fold and lowercase `word` the way the tokenizer does before
/// stemming (DEC-336), so a `prefix*` or a `terms PREFIX` typed with accents
/// or capitals meets the folded dictionary (`Résum` → `resum`).
pub(crate) fn fold_lower(word: &str) -> String {
    if word.is_ascii() {
        word.to_ascii_lowercase()
    } else {
        fold(word).to_lowercase()
    }
}

fn stem_lower(part: &str, stemmer: &Stemmer) -> String {
    if part.is_ascii() {
        stemmer.stem(&part.to_ascii_lowercase()).into_owned()
    } else {
        stemmer.stem(&part.to_lowercase()).into_owned()
    }
}

/// Split one separator-free piece at camelCase boundaries.
fn split_camel<'a>(piece: &'a str, out: &mut Vec<&'a str>) {
    let chars: Vec<(usize, char)> = piece.char_indices().collect();
    let mut start = 0usize;
    for k in 1..chars.len() {
        let (byte, cur) = chars[k];
        let prev = chars[k - 1].1;
        let boundary = if prev.is_lowercase() && cur.is_uppercase() {
            true
        } else if prev.is_uppercase() && cur.is_uppercase() {
            // `HTMLParser`: split before the `P` when a lowercase run of at
            // least two follows, so `APIs` and `URLs` stay whole.
            chars[k + 1..]
                .iter()
                .take_while(|(_, c)| c.is_lowercase())
                .count()
                >= 2
        } else {
            false
        };
        if boundary {
            out.push(&piece[start..byte]);
            start = byte;
        }
    }
    if start < piece.len() {
        out.push(&piece[start..]);
    }
}

/// Tokenize one word segment that mixes CJK and other scripts (v2 rules,
/// with diacritic folding for the non-CJK segments).
fn tokenize_mixed(run: &str, stemmer: &Stemmer, out: &mut Vec<String>) {
    let chars: Vec<char> = run.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let seg_is_cjk = is_scriptio_continua(chars[i]);
        let mut j = i + 1;
        while j < chars.len() && is_scriptio_continua(chars[j]) == seg_is_cjk {
            j += 1;
        }
        let segment = &chars[i..j];
        if seg_is_cjk {
            if let [only] = segment {
                out.push(only.to_string());
            } else {
                out.extend(
                    segment
                        .windows(2)
                        .map(|pair| pair.iter().collect::<String>()),
                );
            }
        } else {
            let word: String = segment.iter().collect();
            for piece in fold(&word)
                .split(|c: char| !c.is_alphanumeric())
                .filter(|p| !p.is_empty())
            {
                out.push(stem_lower(piece, stemmer));
            }
        }
        i = j;
    }
}

/// Tokenize one word into `out`. Returns `true` when the word was an
/// identifier and its joined whole was pushed first, followed by its parts.
fn tokenize_word(word: &str, stemmer: &Stemmer, out: &mut Vec<String>) -> bool {
    // Fast path: plain lowercase ASCII prose words are one token.
    if word
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        out.push(stemmer.stem(word).into_owned());
        return false;
    }
    if !word.is_ascii() && word.chars().any(is_scriptio_continua) {
        for part in word.split(is_joiner).filter(|p| !p.is_empty()) {
            tokenize_mixed(part, stemmer, out);
        }
        return false;
    }
    let folded: Cow<'_, str> = if word.is_ascii() {
        Cow::Borrowed(word)
    } else {
        Cow::Owned(fold(word))
    };
    let mut parts: Vec<&str> = Vec::new();
    for piece in folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|p| !p.is_empty())
    {
        split_camel(piece, &mut parts);
    }
    match parts.as_slice() {
        [] => false,
        [only] => {
            out.push(stem_lower(only, stemmer));
            false
        }
        _ => {
            let whole: String = parts.concat();
            out.push(stem_lower(&whole, stemmer));
            out.extend(parts.iter().map(|p| stem_lower(p, stemmer)));
            true
        }
    }
}

/// Tokenize `text` into `out` (appending).
pub(crate) fn tokenize_into(text: &str, stemmer: &Stemmer, out: &mut Vec<String>) {
    for_each_word(text, |word| {
        tokenize_word(word, stemmer, out);
    });
}

/// Tokenize `text`: words, CJK bigrams, diacritic folding, identifier
/// splitting, lowercasing and stemming — see the module docs.
pub fn tokenize(text: &str, stemmer: &Stemmer) -> Vec<String> {
    let mut out = Vec::new();
    tokenize_into(text, stemmer, &mut out);
    out
}

/// Tokens of one query word, per position, across query languages.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct QueryWordTokens {
    /// Stem alternatives of the joined identifier, when the word has parts.
    pub(crate) whole: Option<Vec<String>>,
    /// Stem alternatives at each part position (one position for a plain word).
    pub(crate) parts: Vec<Vec<String>>,
}

/// Split query text into words and tokenize each with every stemmer,
/// keeping the identifier structure (whole vs parts) the compiler needs.
pub(crate) fn query_words(text: &str, stemmers: &[&Stemmer]) -> Vec<QueryWordTokens> {
    let mut out = Vec::new();
    for_each_word(text, |word| {
        let mut whole: Option<Vec<String>> = None;
        let mut parts: Vec<Vec<String>> = Vec::new();
        for stemmer in stemmers {
            let mut tokens = Vec::new();
            let is_identifier = tokenize_word(word, stemmer, &mut tokens);
            let rest = if is_identifier && !tokens.is_empty() {
                let first = tokens.remove(0);
                let alternatives = whole.get_or_insert_with(Vec::new);
                if !alternatives.contains(&first) {
                    alternatives.push(first);
                }
                tokens
            } else {
                tokens
            };
            for (i, token) in rest.into_iter().enumerate() {
                if parts.len() <= i {
                    parts.push(Vec::new());
                }
                if !parts[i].contains(&token) {
                    parts[i].push(token);
                }
            }
        }
        if !parts.is_empty() || whole.is_some() {
            out.push(QueryWordTokens { whole, parts });
        }
    });
    out
}

// ---------------------------------------------------------------------------
// Field-aware document tokens (BM25F, DEC-337)
// ---------------------------------------------------------------------------

#[allow(clippy::trivially_copy_pass_by_ref)] // serde requires the by-ref shape
fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

/// One document's forward-index record: the positional title+body stream,
/// which of its positions are title or heading text, and the tag tokens.
///
/// `tokens[..title_len]` is the promoted title; `heading_runs` are
/// half-open `[start, end)` position ranges of heading-line tokens; every
/// other position is body. `tag_tokens` (tags plus frontmatter `aliases`,
/// sorted) carry no position, so phrases only ever span title and body.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocTokens {
    /// Title tokens followed by body tokens, in order.
    pub tokens: Vec<String>,
    /// Number of leading title tokens.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub title_len: u32,
    /// Half-open position ranges of heading-line tokens, ascending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heading_runs: Vec<(u32, u32)>,
    /// Tokens of the tags and aliases, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tag_tokens: Vec<String>,
}

impl From<Vec<String>> for DocTokens {
    /// A body-only record: every token is a body token, no tags.
    fn from(tokens: Vec<String>) -> Self {
        Self {
            tokens,
            ..Self::default()
        }
    }
}

/// Builds [`DocTokens`] line by line, applying `[search] code_blocks`.
///
/// Every path that tokenizes a document (`create-index`, the disk fallback,
/// incremental refreshes) feeds it the same raw body lines with the same
/// heading flags, so their streams are identical.
pub struct DocTokenBuilder<'a> {
    stemmer: &'a Stemmer,
    skip_code: bool,
    fence: crate::scanner::FenceTracker,
    doc: DocTokens,
}

fn position(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

impl<'a> DocTokenBuilder<'a> {
    /// Start a document with its promoted title.
    pub fn new(stemmer: &'a Stemmer, skip_code: bool, title: &str) -> Self {
        let mut doc = DocTokens::default();
        tokenize_into(title, stemmer, &mut doc.tokens);
        doc.title_len = position(doc.tokens.len());
        Self {
            stemmer,
            skip_code,
            fence: crate::scanner::FenceTracker::new(),
            doc,
        }
    }

    /// Whether `raw` is excluded as fenced code (advances the fence state).
    /// Call once per body line, in order, before deciding to tokenize it.
    pub fn skips(&mut self, raw: &str) -> bool {
        self.skip_code && self.fence.process_line(raw)
    }

    /// Feed one raw body line; `heading` marks an outline heading line.
    pub fn line(&mut self, raw: &str, heading: bool) {
        if self.skips(raw) {
            return;
        }
        self.push_line(raw, heading);
    }

    /// Tokenize a line already admitted by [`Self::skips`].
    pub fn push_line(&mut self, raw: &str, heading: bool) {
        let start = position(self.doc.tokens.len());
        tokenize_into(raw, self.stemmer, &mut self.doc.tokens);
        let end = position(self.doc.tokens.len());
        if heading && end > start {
            match self.doc.heading_runs.last_mut() {
                Some(last) if last.1 == start => last.1 = end,
                _ => self.doc.heading_runs.push((start, end)),
            }
        }
    }

    /// Finish with the document's tags and aliases.
    pub fn finish<'t>(mut self, tags: impl IntoIterator<Item = &'t str>) -> DocTokens {
        for tag in tags {
            tokenize_into(tag, self.stemmer, &mut self.doc.tag_tokens);
        }
        self.doc.tag_tokens.sort_unstable();
        self.doc
    }
}

/// Tokenize a whole document from its title, `body` text (the body region
/// only, frontmatter excluded), the 0-based body line indices that are
/// headings (sorted), and its tags plus aliases.
pub fn tokenize_document_text<'t>(
    title: &str,
    body: &str,
    heading_lines: &[usize],
    tags: impl IntoIterator<Item = &'t str>,
    stemmer: &Stemmer,
    skip_code: bool,
) -> DocTokens {
    let mut builder = DocTokenBuilder::new(stemmer, skip_code, title);
    for (i, line) in body.lines().enumerate() {
        builder.line(line, heading_lines.binary_search(&i).is_ok());
    }
    builder.finish(tags)
}

/// Body-relative (0-based) line indices of an outline's headings, given the
/// file line (1-based) of the first body line.
#[must_use]
pub fn heading_body_lines(
    sections: &[crate::types::OutlineSection],
    first_body_line: usize,
) -> Vec<usize> {
    let mut lines: Vec<usize> = sections
        .iter()
        .filter(|s| s.heading.is_some() && s.line >= first_body_line)
        .map(|s| s.line - first_body_line)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25::{StemLanguage, create_stemmer};

    fn en(text: &str) -> Vec<String> {
        tokenize(text, &create_stemmer(StemLanguage::English))
    }

    #[test]
    fn diacritics_fold_before_stemming() {
        assert_eq!(en("résumé"), en("resume"));
        assert_eq!(en("Nöte"), vec!["note"]);
        assert_eq!(en("naïve café"), en("naive cafe"));
        // Decomposed input (NFD) folds the same as precomposed input.
        assert_eq!(en("re\u{301}sume\u{301}"), en("resume"));
        // Compatibility forms decompose too.
        assert_eq!(en("\u{FB01}le"), en("file"));
    }

    #[test]
    fn german_stemmer_agrees_with_folding() {
        let de = create_stemmer(StemLanguage::German);
        assert_eq!(tokenize("Häuser", &de), tokenize("hauser", &de));
        assert_eq!(tokenize("Häuser", &de), tokenize("Hauser", &de));
        let fr = create_stemmer(StemLanguage::French);
        assert_eq!(tokenize("été", &fr), tokenize("ete", &fr));
    }

    #[test]
    fn identifiers_emit_whole_then_parts() {
        assert_eq!(en("getUserName"), vec!["getusernam", "get", "user", "name"]);
        assert_eq!(en("get_user_name"), en("getUserName"));
        assert_eq!(en("get-user-name"), en("getUserName"));
        assert_eq!(en("HTMLParser"), vec!["htmlparser", "html", "parser"]);
        // Plural acronyms and digit runs stay whole.
        assert_eq!(en("APIs"), vec!["api"]);
        assert_eq!(en("utf8 sha256sum"), vec!["utf8", "sha256sum"]);
        // Doubled or leading separators do not join.
        assert_eq!(en("a--b"), vec!["a", "b"]);
        assert_eq!(en("--flag"), vec!["flag"]);
        // Plain prose is unchanged.
        assert_eq!(en("Running faster"), vec!["run", "faster"]);
    }

    #[test]
    fn cjk_bigrams_survive() {
        let tokens = en("日本語Docker");
        assert!(tokens.contains(&"日本".to_owned()));
        assert!(tokens.contains(&"docker".to_owned()));
        // Hangul is never NFKD-decomposed into jamo.
        assert_eq!(en("한국어"), vec!["한국", "국어"]);
    }

    #[test]
    fn query_words_keep_identifier_structure() {
        let s = create_stemmer(StemLanguage::English);
        let words = query_words("getUserName plain", &[&s]);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].whole, Some(vec!["getusernam".to_owned()]));
        assert_eq!(words[0].parts.len(), 3);
        assert_eq!(words[1].whole, None);
        assert_eq!(words[1].parts, vec![vec!["plain".to_owned()]]);
    }

    #[test]
    fn builder_marks_fields_and_skips_code() {
        let s = create_stemmer(StemLanguage::English);
        let body = "# Heading one\ntext here\n```\ncode inside\n```\n`inline` code";
        let doc = tokenize_document_text("My Title", body, &[0], ["rust", "x/y"], &s, true);
        assert_eq!(doc.title_len, 2);
        assert_eq!(doc.heading_runs, vec![(2, 4)]);
        assert!(!doc.tokens.contains(&"insid".to_owned()));
        assert!(doc.tokens.contains(&"inlin".to_owned()));
        assert_eq!(doc.tag_tokens, vec!["rust", "x", "y"]);
        let indexed = tokenize_document_text("My Title", body, &[0], ["rust"], &s, false);
        assert!(indexed.tokens.contains(&"insid".to_owned()));
    }
}
