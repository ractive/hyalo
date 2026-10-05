//! Ranked-search query language (iteration 302, DEC-333).
//!
//! Grammar, loosest binding first:
//!
//! ```text
//! query   := and_expr
//! and_expr:= or_expr ( ["AND"] or_expr )*      -- implicit AND
//! or_expr := unary ( "OR" unary )*             -- OR binds tighter than AND
//! unary   := "-" unary | primary               -- negates a term, phrase or group
//! primary := WORD | PREFIX* | "PHRASE" | "(" and_expr ")"
//! ```
//!
//! A parsed query is compiled once per stemming language present in the
//! corpus: every term leaf becomes the alternatives of its per-language stems
//! (identical stems deduplicated), so a German note is found by its German
//! inflection even when the vault default language is English.
//!
//! Every leaf searches the token stream: there are no field terms
//! (DEC-366). A `name:value` token such as `title:x`, `std::fs` or a URL is a
//! plain word; structure is selected with `find`'s flags (`--title`,
//! `--section`, `--tag`, `--glob`).

use std::collections::HashMap;

use rust_stemmers::Stemmer;

use super::{Bm25InvertedIndex, Bm25Match, StemLanguage, create_stemmer, tokenize};

/// Most dictionary terms one `prefix*` term may expand to. Further terms are
/// dropped (most frequent kept) and [`Bm25InvertedIndex::capped_prefixes`]
/// reports the prefix so the caller can warn.
pub const MAX_PREFIX_EXPANSION: usize = 256;

/// How many top-ranked candidates the proximity bonus re-scores (DEC-338).
pub const PROXIMITY_CANDIDATES: usize = 200;

/// Deepest parenthesis nesting accepted; bounds the recursive parser.
const MAX_GROUP_DEPTH: usize = 64;

/// A malformed query: unbalanced parenthesis, empty group, bare `*`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuerySyntaxError {
    message: String,
}

impl QuerySyntaxError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for QuerySyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QuerySyntaxError {}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Lexeme {
    Open,
    Close,
    Or,
    And,
    Not,
    /// A plain word with its byte span in the query (for did-you-mean rewrites).
    Word {
        text: String,
        start: usize,
        end: usize,
    },
    /// A quoted phrase, its `~N` slop (0 when absent), and the byte offset
    /// of its content's first character in the query (for did-you-mean
    /// rewrites of a misspelled word inside the phrase — DEC-357).
    Phrase(String, u32, usize),
}

/// Largest accepted phrase slop; a larger `~N` is clamped to it.
pub const MAX_PHRASE_SLOP: u32 = 64;

/// Non-fatal issues detected while parsing a query (UX-6 / DEC-358):
/// malformed-but-recoverable input gets a `-q`-proof warning instead of a
/// silent reinterpretation that returns a plausible-looking but wrong
/// answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryWarnings {
    /// `OR` appeared with no left or right operand (`a OR`, `OR a`) and was
    /// dropped rather than widening the match the way a present `OR` does.
    pub dangling_operator: bool,
    /// A `"` was opened but the query ended before a matching `"` closed it;
    /// the historical behaviour (run the phrase to the end of the query)
    /// still applies, this only reports that it happened.
    pub unterminated_quote: bool,
    /// Each `~N` that exceeded [`MAX_PHRASE_SLOP`], as typed (every one was
    /// clamped to it).
    pub clamped_slops: Vec<u32>,
    /// Words with a `*` that is not the trailing prefix marker (`*foo`,
    /// `sn*p`): the `*` there is a literal character, not a wildcard.
    pub misplaced_wildcard_terms: Vec<String>,
}

/// Read an optional `~N` slop suffix directly after a closing quote (DEC-338).
/// A `~` without digits is consumed and means 0. Returns the (possibly
/// clamped) slop and, when clamping happened, the value as typed.
fn read_slop(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> (u32, Option<u32>) {
    if !matches!(chars.peek(), Some(&(_, '~'))) {
        return (0, None);
    }
    chars.next();
    let mut slop: u32 = 0;
    let mut saw_digit = false;
    while let Some(&(_, c)) = chars.peek() {
        let Some(digit) = c.to_digit(10) else {
            break;
        };
        saw_digit = true;
        slop = slop.saturating_mul(10).saturating_add(digit);
        chars.next();
    }
    if !saw_digit {
        return (0, None);
    }
    if slop > MAX_PHRASE_SLOP {
        (MAX_PHRASE_SLOP, Some(slop))
    } else {
        (slop, None)
    }
}

/// `true` when the upcoming (unconsumed) chars are a `~` immediately
/// followed by an alphabetic character -- a malformed slop suffix
/// (`"a b"~abc`), not a word that happens to follow a phrase. Called only
/// right after a *closing* `"` (review fix: the original check scanned
/// every `"` in the query, including an *opening* one, so `"~home dir"` --
/// an ordinary phrase whose content starts with `~` -- was rejected before
/// it was even lexed).
fn peek_malformed_slop(chars: &std::iter::Peekable<std::str::CharIndices<'_>>) -> bool {
    let mut lookahead = chars.clone();
    matches!(lookahead.next(), Some((_, '~')))
        && matches!(lookahead.next(), Some((_, c)) if c.is_alphabetic())
}

/// Read a quoted phrase body after its opening quote. An unterminated quote
/// runs to the end of the query (the historical behaviour); the returned
/// `bool` says whether a closing `"` was actually found.
fn read_phrase(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> (String, bool) {
    let mut phrase = String::new();
    for (_, c) in chars.by_ref() {
        if c == '"' {
            return (phrase, true);
        }
        phrase.push(c);
    }
    (phrase, false)
}

/// Read one word. A `(` inside a word is literal and so is the `)` that
/// closes it (`main()`, `f(x)`); an unmatched `)` ends the word and closes a group.
fn read_word(
    query: &str,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> (String, usize, usize) {
    let start = chars.peek().map_or(query.len(), |&(i, _)| i);
    let mut end = start;
    let mut inner = 0usize;
    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() || c == '"' {
            break;
        }
        if c == '(' {
            if i == start {
                break;
            }
            inner += 1;
        } else if c == ')' {
            if inner == 0 {
                break;
            }
            inner -= 1;
        }
        end = i + c.len_utf8();
        chars.next();
    }
    (query[start..end].to_owned(), start, end)
}

/// Error text shared by every malformed-slop site (plain and negated phrase).
const MALFORMED_SLOP_MESSAGE: &str =
    "'~' after a phrase takes a number of extra words, e.g. \"a b\"~3 -- not a word";

fn lex(query: &str) -> Result<(Vec<Lexeme>, QueryWarnings), QuerySyntaxError> {
    let mut out = Vec::new();
    let mut warnings = QueryWarnings::default();
    let mut chars = query.char_indices().peekable();
    while let Some(&(_, ch)) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        match ch {
            '(' => {
                chars.next();
                out.push(Lexeme::Open);
            }
            ')' => {
                chars.next();
                out.push(Lexeme::Close);
            }
            '"' => {
                chars.next();
                let content_start = chars.peek().map_or(query.len(), |&(i, _)| i);
                let (phrase, terminated) = read_phrase(&mut chars);
                // Review fix (MUST-FIX 1): checked only right after a
                // *closing* quote was actually found, on the unconsumed
                // chars that follow it -- not by scanning the whole query
                // for any `"`, which misfired on an *opening* quote whose
                // phrase content happened to start with `~` (`"~home
                // dir"`).
                if terminated && peek_malformed_slop(&chars) {
                    return Err(QuerySyntaxError::new(MALFORMED_SLOP_MESSAGE));
                }
                let (slop, clamped_from) = read_slop(&mut chars);
                warnings.unterminated_quote |= !terminated;
                warnings.clamped_slops.extend(clamped_from);
                if !phrase.is_empty() {
                    out.push(Lexeme::Phrase(phrase, slop, content_start));
                }
            }
            '-' => {
                chars.next();
                match chars.peek() {
                    // A lone `-` (or one directly before `)`) negates nothing.
                    None => {}
                    Some(&(_, c)) if c.is_whitespace() || c == ')' => {}
                    // `-""` negates an empty phrase, i.e. nothing: emit no
                    // `Not`, or it would negate whatever term comes next.
                    Some(&(_, '"')) => {
                        chars.next();
                        let content_start = chars.peek().map_or(query.len(), |&(i, _)| i);
                        let (phrase, terminated) = read_phrase(&mut chars);
                        if terminated && peek_malformed_slop(&chars) {
                            return Err(QuerySyntaxError::new(MALFORMED_SLOP_MESSAGE));
                        }
                        let (slop, clamped_from) = read_slop(&mut chars);
                        warnings.unterminated_quote |= !terminated;
                        warnings.clamped_slops.extend(clamped_from);
                        if !phrase.is_empty() {
                            out.push(Lexeme::Not);
                            out.push(Lexeme::Phrase(phrase, slop, content_start));
                        }
                    }
                    Some(&(_, '(')) => out.push(Lexeme::Not),
                    Some(_) => {
                        out.push(Lexeme::Not);
                        // A negated word is always literal: `-or` excludes "or".
                        let (text, start, end) = read_word(query, &mut chars);
                        out.push(classify_word(text, start, end, false));
                    }
                }
            }
            _ => {
                let (text, start, end) = read_word(query, &mut chars);
                if text.is_empty() {
                    // Defensive: never loop without consuming input.
                    chars.next();
                    continue;
                }
                out.push(classify_word(text, start, end, true));
            }
        }
    }
    Ok((out, warnings))
}

/// `OR`/`AND` (case-insensitive) are operators unless negated; everything
/// else is a word, a `name:value` token included (DEC-366).
fn classify_word(text: String, start: usize, end: usize, keywords: bool) -> Lexeme {
    if keywords && text.eq_ignore_ascii_case("or") {
        return Lexeme::Or;
    }
    if keywords && text.eq_ignore_ascii_case("and") {
        return Lexeme::And;
    }
    Lexeme::Word { text, start, end }
}

// ---------------------------------------------------------------------------
// Parser → raw (unstemmed) AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum RawNode {
    And(Vec<RawNode>),
    Or(Vec<RawNode>),
    Not(Box<RawNode>),
    Word {
        text: String,
        start: usize,
        end: usize,
    },
    /// Content, slop, and the byte offset of the content's first character
    /// in the query (DEC-357).
    Phrase(String, u32, usize),
}

struct Parser {
    lexemes: Vec<Lexeme>,
    pos: usize,
    /// Accumulated while parsing; `lex()`'s findings seed it, parsing adds
    /// `dangling_operator` (UX-6 / DEC-358).
    warnings: QueryWarnings,
}

impl Parser {
    fn peek(&self) -> Option<&Lexeme> {
        self.lexemes.get(self.pos)
    }

    fn parse_and(&mut self, depth: usize) -> Result<Vec<RawNode>, QuerySyntaxError> {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                None if depth > 0 => {
                    return Err(QuerySyntaxError::new(
                        "unbalanced parenthesis: a '(' is never closed",
                    ));
                }
                None => break,
                Some(Lexeme::Close) if depth > 0 => break,
                Some(Lexeme::Close) => {
                    return Err(QuerySyntaxError::new(
                        "unbalanced parenthesis: ')' without a matching '('",
                    ));
                }
                // `AND` is whitespace and never dangling. An `OR` reaching
                // here has no left operand (`OR a`, or a run of `OR OR`) --
                // ignored, same as a trailing one in `parse_or`.
                Some(Lexeme::And) => self.pos += 1,
                Some(Lexeme::Or) => {
                    self.warnings.dangling_operator = true;
                    self.pos += 1;
                }
                Some(_) => items.push(self.parse_or(depth)?),
            }
        }
        Ok(items)
    }

    fn parse_or(&mut self, depth: usize) -> Result<RawNode, QuerySyntaxError> {
        let mut alternatives = vec![self.parse_unary(depth)?];
        while matches!(self.peek(), Some(Lexeme::Or)) {
            while matches!(self.peek(), Some(Lexeme::Or)) {
                self.pos += 1;
            }
            // A trailing `OR` (end, `)` or `AND`) has no right operand:
            // ignored, and flagged (UX-6 / DEC-358) -- `a OR` matched
            // everything just like `a` alone, which is not what a reader
            // typing `OR` expects.
            if matches!(self.peek(), None | Some(Lexeme::Close | Lexeme::And)) {
                self.warnings.dangling_operator = true;
                break;
            }
            alternatives.push(self.parse_unary(depth)?);
        }
        Ok(if alternatives.len() == 1 {
            alternatives.remove(0)
        } else {
            RawNode::Or(alternatives)
        })
    }

    /// Consecutive `-` are counted iteratively (never recursed) and collapse
    /// to one `Not` or none; more than [`MAX_GROUP_DEPTH`] in a row is an error.
    fn parse_unary(&mut self, depth: usize) -> Result<RawNode, QuerySyntaxError> {
        let mut negations = 0usize;
        while matches!(self.peek(), Some(Lexeme::Not)) {
            self.pos += 1;
            negations += 1;
            if negations > MAX_GROUP_DEPTH {
                return Err(QuerySyntaxError::new(format!(
                    "more than {MAX_GROUP_DEPTH} consecutive '-' negations"
                )));
            }
        }
        let operand = self.parse_primary(depth)?;
        Ok(if negations % 2 == 1 {
            RawNode::Not(Box::new(operand))
        } else {
            operand
        })
    }

    fn parse_primary(&mut self, depth: usize) -> Result<RawNode, QuerySyntaxError> {
        let Some(lexeme) = self.lexemes.get(self.pos).cloned() else {
            return Err(QuerySyntaxError::new("'-' must be followed by a term"));
        };
        self.pos += 1;
        match lexeme {
            Lexeme::Open => {
                if depth + 1 > MAX_GROUP_DEPTH {
                    return Err(QuerySyntaxError::new(format!(
                        "parentheses nested deeper than {MAX_GROUP_DEPTH} levels"
                    )));
                }
                let items = self.parse_and(depth + 1)?;
                // parse_and(depth > 0) only returns at a `)`.
                self.pos += 1;
                if items.is_empty() {
                    return Err(QuerySyntaxError::new(
                        "empty group: '()' contains no search term",
                    ));
                }
                Ok(RawNode::And(items))
            }
            Lexeme::Word { text, start, end } => Ok(RawNode::Word { text, start, end }),
            Lexeme::Phrase(text, slop, content_start) => {
                Ok(RawNode::Phrase(text, slop, content_start))
            }
            // Unreachable from parse_and/parse_or, which consume these first.
            Lexeme::Close | Lexeme::Or | Lexeme::And | Lexeme::Not => Err(QuerySyntaxError::new(
                "'-' must be followed by a term, phrase or group",
            )),
        }
    }
}

fn parse(query: &str) -> Result<(RawNode, QueryWarnings), QuerySyntaxError> {
    // UX-6 / DEC-358: a malformed `~N` slop (`"a b"~abc`) is detected inside
    // `lex()` itself, right after the *closing* quote it actually follows
    // (review fix: a query-wide scan for `"` + `~` + a letter also matched
    // an *opening* quote, so `"~home dir"` -- an ordinary phrase whose
    // content starts with `~` -- was rejected before it was ever lexed).
    let (lexemes, lex_warnings) = lex(query)?;
    let mut parser = Parser {
        lexemes,
        pos: 0,
        warnings: lex_warnings,
    };
    let root = RawNode::And(parser.parse_and(0)?);
    Ok((root, parser.warnings))
}

// ---------------------------------------------------------------------------
// Compiled (stemmed) AST
// ---------------------------------------------------------------------------

/// A node of the compiled query.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Node {
    And(Vec<Node>),
    Or(Vec<Node>),
    Not(Box<Node>),
    /// One query token: alternative stems, one per query language (deduped).
    Term(Vec<String>),
    /// Lowercase prefix matched against the stemmed dictionary: the raw
    /// prefix first, then its per-language stems and their longest common
    /// prefixes with the raw form, tried only when the raw prefix matches nothing.
    Prefix(Vec<String>),
    /// Alternative stem sequences, one per query language, and the slop:
    /// tokens in order with at most that many extra positions (DEC-338).
    Phrase(Vec<Vec<String>>, u32),
}

/// A positive plain query word, kept for did-you-mean.
#[derive(Debug, Clone)]
struct QueryWord {
    /// The word as written.
    raw: String,
    /// Byte span of `raw` in the query string.
    start: usize,
    end: usize,
    /// Per-language stems of the single token the word produced.
    stems: Vec<String>,
}

/// A positive word shaped like one of the removed field terms (`title:x`,
/// `heading:x`, `tag:x`, `path:x`; DEC-366). It is searched as plain words;
/// the hint layer uses it to point a zero-result query at the matching flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyFieldTerm {
    /// The field name, lowercased: `title`, `heading`, `tag` or `path`.
    pub field: String,
    /// The text after the colon, as written.
    pub value: String,
    /// Byte span of the whole token in the query string.
    start: usize,
    end: usize,
}

/// The removed field-term names (DEC-366), recognised only for the
/// migration hint.
const LEGACY_FIELDS: [&str; 4] = ["title", "heading", "tag", "path"];

/// `Some` when `text` is `name:value` with `name` a removed field term.
fn legacy_field_term(text: &str, start: usize, end: usize) -> Option<LegacyFieldTerm> {
    let (name, value) = text.split_once(':')?;
    let field = name.to_ascii_lowercase();
    (LEGACY_FIELDS.contains(&field.as_str()) && !value.is_empty()).then(|| LegacyFieldTerm {
        field,
        value: value.to_owned(),
        start,
        end,
    })
}

/// One normalized query shared by indexed scoring, disk fallback and snippets.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub(super) root: Option<Node>,
    words: Vec<QueryWord>,
    source: String,
    /// Field weights and proximity bonus used when scoring (DEC-337/338).
    params: super::SearchSettings,
    /// Non-fatal issues found while parsing (UX-6 / DEC-358).
    warnings: QueryWarnings,
    /// Positive words shaped like a removed field term (DEC-366).
    legacy_fields: Vec<LegacyFieldTerm>,
}

fn dedup<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut out: Vec<T> = Vec::with_capacity(items.len());
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

/// Per-language token sequences for `text`, deduped. Tokenization splits the
/// same way in every language; only the stems differ.
fn sequences(text: &str, stemmers: &[&Stemmer]) -> Vec<Vec<String>> {
    dedup(
        stemmers
            .iter()
            .map(|stemmer| tokenize(text, stemmer))
            .filter(|tokens| !tokens.is_empty())
            .collect(),
    )
}

/// Alternatives at each token position of `text`, deduped per position.
fn positional_alternatives(text: &str, stemmers: &[&Stemmer]) -> Vec<Vec<String>> {
    let per_language: Vec<Vec<String>> = stemmers.iter().map(|s| tokenize(text, s)).collect();
    let width = per_language.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|i| {
            dedup(
                per_language
                    .iter()
                    .filter_map(|t| t.get(i).cloned())
                    .collect(),
            )
        })
        .collect()
}

/// Candidate prefixes for `raw*`: the raw lowercase prefix first, then for
/// each query language its stem and the longest common prefix of raw and
/// stem (at least three characters). The fallbacks are used only when the raw
/// prefix matches no dictionary stem, so `configuration*` still finds
/// `configur` (the stem of "configuration").
fn prefix_candidates(raw: &str, stemmers: &[&Stemmer]) -> Vec<String> {
    let mut out = vec![raw.to_owned()];
    for stemmer in stemmers {
        let stem = stemmer.stem(raw).into_owned();
        let common: String = raw
            .chars()
            .zip(stem.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a)
            .collect();
        for candidate in [stem, common] {
            if candidate.chars().count() >= 3 && !out.contains(&candidate) {
                out.push(candidate);
            }
        }
    }
    out
}

/// Whitespace-delimited raw words inside quoted phrase `text`, each with its
/// byte span local to `text` (DEC-357): `"stale indx"` yields `("stale",
/// 0, 5)` and `("indx", 6, 10)`. A phrase has no parenthesis/field syntax to
/// account for, unlike [`read_word`], so plain whitespace splitting suffices.
fn phrase_raw_words(text: &str) -> Vec<(&str, usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut last_end = 0usize;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                out.push((&text[s..i], s, i));
            }
        } else if start.is_none() {
            start = Some(i);
        }
        last_end = i + c.len_utf8();
    }
    if let Some(s) = start {
        out.push((&text[s..last_end], s, last_end));
    }
    out
}

fn group(nodes: Vec<Node>, make: fn(Vec<Node>) -> Node) -> Option<Node> {
    match nodes.len() {
        0 => None,
        1 => nodes.into_iter().next(),
        _ => Some(make(nodes)),
    }
}

struct Compiler<'a> {
    stemmers: &'a [&'a Stemmer],
    words: Vec<QueryWord>,
    /// Words with a `*` that is not the trailing prefix marker (UX-6 /
    /// DEC-358): `*foo`, `sn*p`.
    misplaced_wildcards: Vec<String>,
    /// Positive `title:x`-shaped words (DEC-366).
    legacy_fields: Vec<LegacyFieldTerm>,
}

impl Compiler<'_> {
    fn compile(&mut self, node: RawNode, positive: bool) -> Result<Option<Node>, QuerySyntaxError> {
        Ok(match node {
            RawNode::And(children) => {
                let mut out = Vec::new();
                for child in children {
                    out.extend(self.compile(child, positive)?);
                }
                group(out, Node::And)
            }
            RawNode::Or(children) => {
                let mut out = Vec::new();
                for child in children {
                    out.extend(self.compile(child, positive)?);
                }
                group(out, Node::Or)
            }
            RawNode::Not(child) => self
                .compile(*child, !positive)?
                .map(|n| Node::Not(Box::new(n))),
            RawNode::Word { text, start, end } => self.word(&text, start, end, positive)?,
            RawNode::Phrase(text, slop, content_start) => {
                // UX-1 / DEC-357: a misspelled word inside a phrase used to
                // get no suggestion at all, because only the plain-word path
                // (`self.word`, below) ever registered a `QueryWord` for
                // did-you-mean. Each simple single-token word in the phrase
                // gets the same registration here, at its absolute byte span
                // in the original query so a correction can be spliced back
                // into `query.source`.
                if positive {
                    for (raw, local_start, local_end) in phrase_raw_words(&text) {
                        let words = super::tokenizer::query_words(raw, self.stemmers);
                        if let [only] = words.as_slice()
                            && only.whole.is_none()
                            && only.parts.len() == 1
                        {
                            self.words.push(QueryWord {
                                raw: raw.to_owned(),
                                start: content_start + local_start,
                                end: content_start + local_end,
                                stems: only.parts[0].clone(),
                            });
                        }
                    }
                }
                let mut alternatives = sequences(&text, self.stemmers);
                if alternatives.iter().all(|seq| seq.len() == 1) && !alternatives.is_empty() {
                    Some(Node::Term(alternatives.drain(..).flatten().collect()))
                } else {
                    (!alternatives.is_empty()).then_some(Node::Phrase(alternatives, slop))
                }
            }
        })
    }

    fn word(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
        positive: bool,
    ) -> Result<Option<Node>, QuerySyntaxError> {
        if positive && let Some(legacy) = legacy_field_term(text, start, end) {
            self.legacy_fields.push(legacy);
        }
        if let Some(body) = text.strip_suffix('*') {
            let body = body.trim_end_matches('*');
            let parts: Vec<String> = body
                .split(|c: char| !c.is_alphanumeric())
                .filter(|p| !p.is_empty())
                .map(str::to_lowercase)
                .collect();
            let Some((last, leading)) = parts.split_last() else {
                return Err(QuerySyntaxError::new(format!(
                    "'{text}': a prefix term needs at least one letter or digit before '*'"
                )));
            };
            let mut nodes: Vec<Node> = leading
                .iter()
                .flat_map(|part| positional_alternatives(part, self.stemmers))
                .map(Node::Term)
                .collect();
            nodes.push(Node::Prefix(prefix_candidates(last, self.stemmers)));
            return Ok(group(nodes, Node::And));
        }
        // UX-6 / DEC-358: `*` only means "prefix wildcard" as the very last
        // character of a word (handled above). A `*` anywhere else -- a
        // leading `*foo` or an interior `sn*p` -- is tokenized as a literal
        // character and silently dropped, which usually is not what the
        // query meant.
        if text.contains('*') {
            self.misplaced_wildcards.push(text.to_owned());
        }
        let words = super::tokenizer::query_words(text, self.stemmers);
        if positive
            && let [only] = words.as_slice()
            && only.whole.is_none()
            && only.parts.len() == 1
        {
            self.words.push(QueryWord {
                raw: text.to_owned(),
                start,
                end,
                stems: only.parts[0].clone(),
            });
        }
        // An identifier (`getUserName`, `error-handling`) matches its joined
        // whole OR all of its parts (DEC-336): the whole ranks exact uses
        // first, the parts keep prose spellings ("error handling") matching.
        let nodes = words
            .into_iter()
            .filter_map(|word| {
                let parts = group(word.parts.into_iter().map(Node::Term).collect(), Node::And);
                match (word.whole, parts) {
                    (Some(whole), Some(parts)) => Some(Node::Or(vec![Node::Term(whole), parts])),
                    (Some(whole), None) => Some(Node::Term(whole)),
                    (None, parts) => parts,
                }
            })
            .collect();
        Ok(group(nodes, Node::And))
    }
}

impl CompiledQuery {
    /// Parse and compile `query` once per stemming language in `languages`
    /// (deduplicated; an empty slice means English). Term leaves match any of
    /// their per-language stems.
    ///
    /// # Errors
    /// Returns a [`QuerySyntaxError`] for an unbalanced parenthesis, an empty
    /// group `()`, or a bare `*`.
    pub fn parse(query: &str, languages: &[StemLanguage]) -> Result<Self, QuerySyntaxError> {
        let mut unique: Vec<StemLanguage> = Vec::new();
        for language in languages {
            if !unique.contains(language) {
                unique.push(*language);
            }
        }
        if unique.is_empty() {
            unique.push(StemLanguage::default());
        }
        let stemmers: Vec<Stemmer> = unique.into_iter().map(create_stemmer).collect();
        let refs: Vec<&Stemmer> = stemmers.iter().collect();
        Self::parse_with_stemmers(query, &refs)
    }

    /// Lenient single-language compile: a syntax error yields an empty query
    /// (matching nothing). Prefer [`CompiledQuery::parse`] for user input.
    #[must_use]
    pub fn new(query: &str, language: StemLanguage) -> Self {
        Self::parse(query, &[language]).unwrap_or_else(|_| Self::empty(query))
    }

    pub(super) fn lenient_with_stemmer(query: &str, stemmer: &Stemmer) -> Self {
        Self::parse_with_stemmers(query, &[stemmer]).unwrap_or_else(|_| Self::empty(query))
    }

    fn empty(query: &str) -> Self {
        Self {
            root: None,
            words: Vec::new(),
            source: query.to_owned(),
            params: super::search_settings(),
            warnings: QueryWarnings::default(),
            legacy_fields: Vec::new(),
        }
    }

    fn parse_with_stemmers(query: &str, stemmers: &[&Stemmer]) -> Result<Self, QuerySyntaxError> {
        let (raw, mut warnings) = parse(query)?;
        let mut compiler = Compiler {
            stemmers,
            words: Vec::new(),
            misplaced_wildcards: Vec::new(),
            legacy_fields: Vec::new(),
        };
        let root = compiler.compile(raw, true)?;
        warnings.misplaced_wildcard_terms = compiler.misplaced_wildcards;
        Ok(Self {
            root,
            words: compiler.words,
            source: query.to_owned(),
            params: super::search_settings(),
            warnings,
            legacy_fields: compiler.legacy_fields,
        })
    }

    /// Positive words shaped like a removed field term (`title:x`; DEC-366),
    /// in query order. They are searched as plain words.
    #[must_use]
    pub fn legacy_field_terms(&self) -> &[LegacyFieldTerm] {
        &self.legacy_fields
    }

    /// The query text with every [`Self::legacy_field_terms`] token removed
    /// and whitespace collapsed: what is left to search once those tokens
    /// become flags. Empty when nothing else remains.
    #[must_use]
    pub fn without_legacy_field_terms(&self) -> String {
        let mut out = String::with_capacity(self.source.len());
        let mut cursor = 0;
        for term in &self.legacy_fields {
            if let Some(kept) = self.source.get(cursor..term.start) {
                out.push_str(kept);
            }
            out.push(' ');
            cursor = term.end;
        }
        out.push_str(self.source.get(cursor..).unwrap_or_default());
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Non-fatal issues found while parsing this query (UX-6 / DEC-358):
    /// dangling operators, an unterminated quote, clamped slops, and words
    /// whose `*` is not the trailing prefix marker. The hint layer warns
    /// for each, `-q`-proof.
    #[must_use]
    pub fn warnings(&self) -> &QueryWarnings {
        &self.warnings
    }

    /// Replace the scoring parameters (field weights, proximity bonus). A
    /// compiled query starts with the process's effective `[search]` settings.
    #[must_use]
    pub fn with_params(mut self, params: super::SearchSettings) -> Self {
        self.params = params;
        self
    }

    /// The scoring parameters this query uses.
    #[must_use]
    pub fn params(&self) -> &super::SearchSettings {
        &self.params
    }

    /// The query's required ("Must") text groups for proximity (DEC-338):
    /// each direct child of the top-level AND (or the root itself) made only
    /// of positive text leaves. A group containing a negation is not
    /// positional and is skipped.
    pub(super) fn must_groups(&self) -> Vec<&Node> {
        fn pure_text(node: &Node) -> bool {
            match node {
                Node::Term(_) | Node::Prefix(_) | Node::Phrase(..) => true,
                Node::And(c) | Node::Or(c) => c.iter().all(pure_text),
                Node::Not(_) => false,
            }
        }
        match &self.root {
            Some(Node::And(children)) => children.iter().filter(|n| pure_text(n)).collect(),
            Some(node) if pure_text(node) => vec![node],
            _ => Vec::new(),
        }
    }

    /// `true` when the query has a positive (non-negated) leaf: a word,
    /// prefix or phrase that can rank a document. A query without one (only
    /// negations) matches nothing.
    #[must_use]
    pub fn has_positive_leaf(&self) -> bool {
        fn walk(node: &Node, positive: bool) -> bool {
            match node {
                Node::And(c) | Node::Or(c) => c.iter().any(|n| walk(n, positive)),
                Node::Not(n) => walk(n, !positive),
                _ => positive,
            }
        }
        self.root.as_ref().is_some_and(|n| walk(n, true))
    }

    fn visit_positive_text<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        fn walk<'a>(node: &'a Node, positive: bool, f: &mut dyn FnMut(&'a Node)) {
            match node {
                Node::And(c) | Node::Or(c) => c.iter().for_each(|n| walk(n, positive, f)),
                Node::Not(n) => walk(n, !positive, f),
                Node::Term(_) | Node::Prefix(_) | Node::Phrase(..) if positive => f(node),
                _ => {}
            }
        }
        if let Some(root) = &self.root {
            walk(root, true, f);
        }
    }

    /// Every `prefix*` candidate list in the query, positive or negated.
    fn prefixes(&self) -> Vec<&[String]> {
        fn walk<'a>(node: &'a Node, out: &mut Vec<&'a [String]>) {
            match node {
                Node::And(c) | Node::Or(c) => c.iter().for_each(|n| walk(n, out)),
                Node::Not(n) => walk(n, out),
                Node::Prefix(p) => out.push(p),
                _ => {}
            }
        }
        let mut out = Vec::new();
        if let Some(root) = &self.root {
            walk(root, &mut out);
        }
        out
    }
}

/// Whether `positions` (one ascending list per phrase token) contain the
/// tokens in order with at most `slop` extra positions between the first and
/// the last token. `slop == 0` is the exact adjacent phrase.
pub(super) fn phrase_in_positions(positions: &[&[u32]], slop: u32) -> bool {
    let Some((first, rest)) = positions.split_first() else {
        return false;
    };
    if positions.iter().any(|p| p.is_empty()) {
        return false;
    }
    #[allow(clippy::cast_possible_truncation)]
    let extra_allowed = u64::from(slop);
    for &start in *first {
        // Greedy earliest successor per token minimises the span for a
        // fixed start; when it runs out, no later start can succeed either.
        let mut prev = start;
        for list in rest {
            let idx = list.partition_point(|&p| p <= prev);
            match list.get(idx) {
                Some(&p) => prev = p,
                None => return false,
            }
        }
        let span = u64::from(prev - start);
        if span.saturating_sub(rest.len() as u64) <= extra_allowed {
            return true;
        }
    }
    false
}

/// Whether `seq` occurs in `tokens` in order with at most `slop` extra tokens.
pub(super) fn seq_in_tokens(tokens: &[String], seq: &[String], slop: u32) -> bool {
    if seq.is_empty() {
        return false;
    }
    if slop == 0 {
        return tokens.windows(seq.len()).any(|w| w == seq);
    }
    let positions: Vec<Vec<u32>> = seq
        .iter()
        .map(|t| {
            tokens
                .iter()
                .enumerate()
                .filter(|(_, tok)| *tok == t)
                .filter_map(|(i, _)| u32::try_from(i).ok())
                .collect()
        })
        .collect();
    let refs: Vec<&[u32]> = positions.iter().map(Vec::as_slice).collect();
    phrase_in_positions(&refs, slop)
}

/// Smallest number of extra positions in a window holding at least one
/// position from every list (`lists` each ascending). `None` when a list is
/// empty. A window `[lo, hi]` over `k` lists costs `(hi - lo + 1) - k`,
/// floored at 0, so adjacent terms cost 0 — the same unit as phrase slop.
pub(super) fn min_window(lists: &[Vec<u32>]) -> Option<u64> {
    if lists.is_empty() || lists.iter().any(Vec::is_empty) {
        return None;
    }
    let mut events: Vec<(u32, usize)> = lists
        .iter()
        .enumerate()
        .flat_map(|(g, l)| l.iter().map(move |&p| (p, g)))
        .collect();
    events.sort_unstable();
    let k = lists.len();
    let mut counts = vec![0usize; k];
    let mut covered = 0usize;
    let mut best: Option<u64> = None;
    let mut lo = 0usize;
    for hi in 0..events.len() {
        let g = events[hi].1;
        if counts[g] == 0 {
            covered += 1;
        }
        counts[g] += 1;
        while covered == k {
            let width = u64::from(events[hi].0 - events[lo].0) + 1;
            let cost = width.saturating_sub(k as u64);
            best = Some(best.map_or(cost, |b| b.min(cost)));
            let gl = events[lo].1;
            counts[gl] -= 1;
            if counts[gl] == 0 {
                covered -= 1;
            }
            lo += 1;
        }
    }
    best
}

/// Lexeme-level check used to explain an empty query made only of operators.
pub(super) fn operator_only(query: &str) -> bool {
    // Called only once a query has already compiled successfully elsewhere
    // (see `find`'s BM25-empty-result path), so a lex error here would be
    // unreachable in practice; still handled rather than unwrapped.
    let Ok((lexemes, _)) = lex(query) else {
        return false;
    };
    !lexemes.is_empty()
        && lexemes
            .iter()
            .all(|l| matches!(l, Lexeme::Or | Lexeme::And))
}

// ---------------------------------------------------------------------------
// Snippet selection
// ---------------------------------------------------------------------------

/// One positive query token for snippet coverage: its stem alternatives or a prefix.
#[derive(Debug, Clone, PartialEq)]
enum SnippetGroup {
    Stems(Vec<String>),
    Prefix(Vec<String>),
}

impl SnippetGroup {
    fn hit(&self, token: &str) -> bool {
        match self {
            Self::Stems(stems) => stems.iter().any(|s| s == token),
            Self::Prefix(candidates) => candidates.iter().any(|p| token.starts_with(p.as_str())),
        }
    }
}

/// Token tests for every text leaf under `node` (a required group).
fn collect_snippet_tests(node: &Node, out: &mut Vec<SnippetGroup>) {
    match node {
        Node::And(c) | Node::Or(c) => c.iter().for_each(|n| collect_snippet_tests(n, out)),
        Node::Term(stems) => out.push(SnippetGroup::Stems(stems.clone())),
        Node::Prefix(p) => out.push(SnippetGroup::Prefix(p.clone())),
        Node::Phrase(alternatives, _) => {
            out.push(SnippetGroup::Stems(dedup(
                alternatives.iter().flatten().cloned().collect(),
            )));
        }
        Node::Not(_) => {}
    }
}

/// Positive leaves compiled for snippet qualification and coverage counting.
#[derive(Debug, Clone, Default)]
pub(super) struct SnippetMatcher {
    groups: Vec<SnippetGroup>,
    phrases: Vec<(Vec<String>, u32)>,
    singles: Vec<SnippetGroup>,
    /// Token tests of each required text group, for the min-window
    /// preference between equally covering lines (DEC-338).
    must: Vec<Vec<SnippetGroup>>,
}

impl SnippetMatcher {
    pub(super) fn from_compiled(query: &CompiledQuery) -> Self {
        let mut matcher = Self::default();
        query.visit_positive_text(&mut |node| match node {
            Node::Term(stems) => {
                let group = SnippetGroup::Stems(stems.clone());
                matcher.singles.push(group.clone());
                matcher.push_group(group);
            }
            Node::Prefix(prefix) => {
                let group = SnippetGroup::Prefix(prefix.clone());
                matcher.singles.push(group.clone());
                matcher.push_group(group);
            }
            Node::Phrase(alternatives, slop) => {
                let width = alternatives.iter().map(Vec::len).max().unwrap_or(0);
                for i in 0..width {
                    let stems = dedup(
                        alternatives
                            .iter()
                            .filter_map(|s| s.get(i).cloned())
                            .collect(),
                    );
                    matcher.push_group(SnippetGroup::Stems(stems));
                }
                matcher
                    .phrases
                    .extend(alternatives.iter().map(|seq| (seq.clone(), *slop)));
            }
            _ => {}
        });
        let groups = query.must_groups();
        if groups.len() >= 2 {
            matcher.must = groups
                .into_iter()
                .map(|node| {
                    let mut tests = Vec::new();
                    collect_snippet_tests(node, &mut tests);
                    tests
                })
                .collect();
        }
        matcher
    }

    /// Extra positions of the smallest window on `tokens` covering every
    /// required group; 0 when the query has fewer than two groups, and
    /// `u64::MAX` when the line misses a group.
    pub(super) fn window(&self, tokens: &[String]) -> u64 {
        if self.must.is_empty() {
            return 0;
        }
        let lists: Vec<Vec<u32>> = self
            .must
            .iter()
            .map(|tests| {
                tokens
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| tests.iter().any(|g| g.hit(t)))
                    .filter_map(|(i, _)| u32::try_from(i).ok())
                    .collect()
            })
            .collect();
        min_window(&lists).unwrap_or(u64::MAX)
    }

    fn push_group(&mut self, group: SnippetGroup) {
        if !self.groups.contains(&group) {
            self.groups.push(group);
        }
    }

    /// Distinct positive query tokens a qualifying line covers.
    pub(super) fn max_coverage(&self) -> usize {
        self.groups.len()
    }

    /// Number of distinct positive query tokens on the line, or zero when the
    /// line holds no complete positive leaf (a term, a prefix hit or a phrase).
    pub(super) fn coverage(&self, tokens: &[String]) -> usize {
        let qualifies = self.singles.iter().any(|g| tokens.iter().any(|t| g.hit(t)))
            || self
                .phrases
                .iter()
                .any(|(seq, slop)| seq_in_tokens(tokens, seq, *slop));
        if !qualifies {
            return 0;
        }
        self.groups
            .iter()
            .filter(|g| tokens.iter().any(|t| g.hit(t)))
            .count()
    }
}

// ---------------------------------------------------------------------------
// Evaluation over postings
// ---------------------------------------------------------------------------

/// Dense document set over `0..len` doc ids.
#[derive(Debug, Clone)]
pub(super) struct DocSet {
    words: Vec<u64>,
    len: usize,
}

impl DocSet {
    fn empty(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    fn full(len: usize) -> Self {
        let mut set = Self {
            words: vec![u64::MAX; len.div_ceil(64)],
            len,
        };
        set.trim();
        set
    }

    fn trim(&mut self) {
        let tail = self.len % 64;
        if tail != 0
            && let Some(last) = self.words.last_mut()
        {
            *last &= (1u64 << tail) - 1;
        }
    }

    fn insert(&mut self, id: u32) {
        let id = id as usize;
        if id < self.len {
            self.words[id / 64] |= 1u64 << (id % 64);
        }
    }

    pub(super) fn contains(&self, id: u32) -> bool {
        let id = id as usize;
        id < self.len && self.words[id / 64] & (1u64 << (id % 64)) != 0
    }

    fn intersect(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= b;
        }
    }

    fn union(&mut self, other: &Self) {
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    fn negate(&mut self) {
        for w in &mut self.words {
            *w = !*w;
        }
        self.trim();
    }

    fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(i, &word)| {
            let mut bits = word;
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros() as usize;
                bits &= bits - 1;
                u32::try_from(i * 64 + bit).ok()
            })
        })
    }
}

/// A set of index terms scored together for the documents in `docs`.
struct ScoringUnit<'a> {
    terms: Vec<&'a str>,
    docs: DocSet,
}

struct Evaluator<'a> {
    index: &'a Bm25InvertedIndex,
    units: Vec<ScoringUnit<'a>>,
}

impl<'a> Evaluator<'a> {
    fn len(&self) -> usize {
        self.index.doc_paths.len()
    }

    fn term_docs(&self, term: &str) -> DocSet {
        let mut set = DocSet::empty(self.len());
        for p in self.index.postings.get(term).into_iter().flatten() {
            set.insert(p.doc_id);
        }
        set
    }

    fn phrase_docs(&self, seq: &[String], slop: u32) -> DocSet {
        let mut set = DocSet::empty(self.len());
        for id in self.index.docs_with_all_terms(seq) {
            if self.index.has_phrase_at_positions(seq, id, slop) {
                set.insert(id);
            }
        }
        set
    }

    fn eval(&mut self, node: &'a Node, positive: bool) -> DocSet {
        match node {
            Node::And(children) => {
                let mut acc = DocSet::full(self.len());
                for child in children {
                    let set = self.eval(child, positive);
                    acc.intersect(&set);
                }
                acc
            }
            Node::Or(children) => {
                let mut acc = DocSet::empty(self.len());
                for child in children {
                    let set = self.eval(child, positive);
                    acc.union(&set);
                }
                acc
            }
            Node::Not(child) => {
                let mut set = self.eval(child, !positive);
                set.negate();
                set
            }
            Node::Term(stems) => {
                let units: Vec<_> = stems
                    .iter()
                    .map(|s| (vec![s.as_str()], self.term_docs(s)))
                    .collect();
                self.leaf(units.into_iter(), positive)
            }
            Node::Prefix(candidates) => {
                let expanded = self.index.expand_prefix(candidates);
                let units: Vec<_> = expanded
                    .into_iter()
                    .map(|t| (vec![t], self.term_docs(t)))
                    .collect();
                self.leaf(units.into_iter(), positive)
            }
            Node::Phrase(alternatives, slop) => {
                let units: Vec<_> = alternatives
                    .iter()
                    .map(|seq| {
                        (
                            seq.iter().map(String::as_str).collect(),
                            self.phrase_docs(seq, *slop),
                        )
                    })
                    .collect();
                self.leaf(units.into_iter(), positive)
            }
        }
    }

    /// Union a text leaf's units; positive leaves keep their units for scoring.
    fn leaf(
        &mut self,
        units: impl Iterator<Item = (Vec<&'a str>, DocSet)>,
        positive: bool,
    ) -> DocSet {
        let mut acc = DocSet::empty(self.len());
        for (terms, docs) in units {
            acc.union(&docs);
            if positive {
                self.units.push(ScoringUnit { terms, docs });
            }
        }
        acc
    }
}

/// A dictionary term close to a query term that has no postings.
#[derive(Debug, Clone, PartialEq)]
pub struct TermCandidate {
    /// The (stemmed) dictionary term.
    pub term: String,
    /// Number of documents containing it.
    pub docs: usize,
}

/// Did-you-mean candidates for one query term with no postings.
#[derive(Debug, Clone, PartialEq)]
pub struct TermSuggestion {
    /// The query word as written.
    pub term: String,
    /// Up to three closest dictionary stems, most similar first (document
    /// frequency breaks ties).
    pub candidates: Vec<TermCandidate>,
}

/// Jaro-Winkler floor for a did-you-mean candidate.
const SUGGEST_MIN_JARO_WINKLER: f64 = 0.85;
/// Levenshtein ceiling for a did-you-mean candidate.
const SUGGEST_MAX_LEVENSHTEIN: usize = 2;
/// Candidates reported per missing term.
const SUGGEST_MAX_CANDIDATES: usize = 3;

fn close_enough(query: &str, candidate: &str) -> bool {
    let longer = query.chars().count().max(candidate.chars().count());
    let shorter = query.chars().count().min(candidate.chars().count());
    // Very short dictionary tokens (`a`, `is`) are similar to everything.
    if shorter < 3 {
        return false;
    }
    let lev = strsim::levenshtein(query, candidate);
    (lev <= SUGGEST_MAX_LEVENSHTEIN && lev * 3 <= longer)
        || strsim::jaro_winkler(query, candidate) >= SUGGEST_MIN_JARO_WINKLER
}

impl Bm25InvertedIndex {
    /// The prefixes a `prefix*` term actually expands with: the raw prefix
    /// (when it matches any dictionary term) *union* every stem-derived
    /// fallback that does (`configuration*` → `configur`) — BUG-10 /
    /// DEC-355's sibling fix. The two used to be mutually exclusive: the
    /// fallback only fired when the raw prefix matched *nothing*, so a
    /// single typo stem sharing the raw prefix (`configurationon`, a
    /// one-document misspelling) silently absorbed the whole search and the
    /// well-known stem `configur` was never tried.
    fn effective_prefixes<'p>(&self, candidates: &'p [String]) -> Vec<&'p str> {
        let matches = |p: &str| self.postings.keys().any(|t| t.starts_with(p));
        let Some((raw, fallbacks)) = candidates.split_first() else {
            return Vec::new();
        };
        let mut out: Vec<&str> = Vec::new();
        if matches(raw) {
            out.push(raw.as_str());
        }
        out.extend(fallbacks.iter().map(String::as_str).filter(|p| matches(p)));
        out
    }

    /// Documents for which `node`, read with positive polarity, evaluates
    /// true — the file-level verdict of one leaf (section scoring uses it for
    /// negated leaves, DEC-334).
    pub(super) fn node_docs(&self, node: &Node) -> DocSet {
        let mut evaluator = Evaluator {
            index: self,
            units: Vec::new(),
        };
        evaluator.eval(node, true)
    }

    /// Doc id of every document path.
    pub(super) fn doc_ids(&self) -> HashMap<&str, u32> {
        self.doc_paths
            .iter()
            .enumerate()
            .filter_map(|(id, path)| u32::try_from(id).ok().map(|id| (path.as_str(), id)))
            .collect()
    }

    /// Every dictionary term a `prefix*` candidate list matches, uncapped.
    fn prefix_matches(&self, candidates: &[String]) -> Vec<(&str, usize)> {
        let prefixes = self.effective_prefixes(candidates);
        self.postings
            .iter()
            .filter(|(term, _)| prefixes.iter().any(|p| term.starts_with(p)))
            .map(|(term, posts)| (term.as_str(), posts.len()))
            .collect()
    }

    /// Dictionary terms a `prefix*` term expands to (most frequent first,
    /// then alphabetical), capped at [`MAX_PREFIX_EXPANSION`].
    pub(super) fn expand_prefix(&self, candidates: &[String]) -> Vec<&str> {
        let mut terms = self.prefix_matches(candidates);
        terms.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        terms.truncate(MAX_PREFIX_EXPANSION);
        terms.into_iter().map(|(t, _)| t).collect()
    }

    /// Prefixes in `query` matching more than [`MAX_PREFIX_EXPANSION`]
    /// dictionary terms, with the number of terms each matched.
    #[must_use]
    pub fn capped_prefixes(&self, query: &CompiledQuery) -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for candidates in query.prefixes() {
            let Some(raw) = candidates.first() else {
                continue;
            };
            let count = self.prefix_matches(candidates).len();
            if count > MAX_PREFIX_EXPANSION && !out.iter().any(|(p, _)| p == raw) {
                out.push((raw.clone(), count));
            }
        }
        out
    }

    /// Dictionary terms (stems) with their document frequency, most frequent
    /// first then alphabetical. `prefix` (lowercased) narrows the listing.
    #[must_use]
    pub fn dictionary(&self, prefix: Option<&str>) -> Vec<(&str, usize)> {
        let prefix = prefix.map(str::to_lowercase);
        let mut terms: Vec<(&str, usize)> = self
            .postings
            .iter()
            .filter(|(term, _)| prefix.as_deref().is_none_or(|p| term.starts_with(p)))
            .map(|(term, posts)| (term.as_str(), posts.len()))
            .collect();
        terms.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        terms
    }

    /// Score a compiled query.
    ///
    /// A document matches when the query tree evaluates true for it. Its score
    /// sums BM25 contributions of every positive leaf it satisfies. A query
    /// with no positive leaf at all matches nothing.
    #[must_use]
    pub fn score_compiled(&self, query: &CompiledQuery) -> Vec<Bm25Match> {
        let Some(root) = &query.root else {
            return Vec::new();
        };
        if !query.has_positive_leaf() {
            return Vec::new();
        }
        let mut evaluator = Evaluator {
            index: self,
            units: Vec::new(),
        };
        let admitted = evaluator.eval(root, true);

        #[allow(clippy::cast_precision_loss)]
        let n = self.doc_paths.len() as f64;
        let weights = query.params.weights;
        let mut scores: HashMap<u32, f64> = admitted.iter().map(|id| (id, 0.0)).collect();
        for unit in &evaluator.units {
            for term in &unit.terms {
                let Some(postings) = self.postings.get(*term) else {
                    continue;
                };
                #[allow(clippy::cast_precision_loss)]
                let nt = postings.len() as f64;
                let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();
                for p in postings {
                    if !admitted.contains(p.doc_id) || !unit.docs.contains(p.doc_id) {
                        continue;
                    }
                    let tf = p.weighted_tf(&weights);
                    if tf <= 0.0 {
                        continue;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let dl = self.bm25_len(p.doc_id as usize) as f64;
                    let tf_norm = (tf * (Self::K1 + 1.0))
                        / (tf + Self::K1 * (1.0 - Self::B + Self::B * dl / self.avgdl));
                    if let Some(score) = scores.get_mut(&p.doc_id) {
                        *score += idf * tf_norm;
                    }
                }
            }
        }
        let mut ranked: Vec<(u32, f64)> = scores.into_iter().collect();
        let by_score = |a: &(u32, f64), b: &(u32, f64)| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| self.doc_paths[a.0 as usize].cmp(&self.doc_paths[b.0 as usize]))
        };
        // Score descending, then path: deterministic across index and disk.
        ranked.sort_unstable_by(by_score);
        if self.apply_proximity(query, &mut ranked) {
            ranked.sort_unstable_by(by_score);
        }
        ranked
            .into_iter()
            .map(|(doc_id, score)| Bm25Match {
                rel_path: self.doc_paths[doc_id as usize].clone(),
                score,
            })
            .collect()
    }

    /// Index terms a required group's leaves stand for in this corpus.
    fn group_terms<'s>(&'s self, node: &'s Node, out: &mut Vec<&'s str>) {
        match node {
            Node::And(c) | Node::Or(c) => c.iter().for_each(|n| self.group_terms(n, out)),
            Node::Term(stems) => out.extend(stems.iter().map(String::as_str)),
            Node::Prefix(candidates) => out.extend(self.expand_prefix(candidates)),
            Node::Phrase(alternatives, _) => {
                out.extend(alternatives.iter().flatten().map(String::as_str));
            }
            Node::Not(_) => {}
        }
    }

    /// Proximity bonus (DEC-338): with two or more required text groups,
    /// multiply each of the top [`PROXIMITY_CANDIDATES`] scores (by current
    /// rank) by `1 + bonus / (1 + w)`, where `w` is the extra positions of the
    /// smallest window holding a stream occurrence of every group. A document
    /// that matches a group only through tags gets no bonus. Returns whether
    /// any score changed.
    fn apply_proximity(&self, query: &CompiledQuery, ranked: &mut [(u32, f64)]) -> bool {
        let bonus = query.params.proximity_bonus;
        if bonus <= 0.0 || !bonus.is_finite() {
            return false;
        }
        let groups = query.must_groups();
        if groups.len() < 2 {
            return false;
        }
        let group_terms: Vec<Vec<&str>> = groups
            .iter()
            .map(|node| {
                let mut terms = Vec::new();
                self.group_terms(node, &mut terms);
                terms.sort_unstable();
                terms.dedup();
                terms
            })
            .collect();
        let mut changed = false;
        for (doc_id, score) in ranked.iter_mut().take(PROXIMITY_CANDIDATES) {
            let lists: Vec<Vec<u32>> = group_terms
                .iter()
                .map(|terms| {
                    let mut positions: Vec<u32> = terms
                        .iter()
                        .flat_map(|t| self.positions_of(t, *doc_id).iter().copied())
                        .collect();
                    positions.sort_unstable();
                    positions.dedup();
                    positions
                })
                .collect();
            if let Some(window) = min_window(&lists) {
                #[allow(clippy::cast_precision_loss)]
                let factor = 1.0 + bonus / (1.0 + window as f64);
                *score *= factor;
                changed = true;
            }
        }
        changed
    }

    /// Every positive query word none of whose stems occur in the
    /// dictionary at all -- "has zero postings", independent of whether a
    /// close dictionary term exists to suggest (review fix, SHOULD-FIX 5):
    /// [`Self::suggest`] silently drops a word with no close candidate
    /// either, so a caller using `suggest()`'s output as a stand-in for
    /// "this word has no postings" (the "Try OR" hint did) wrongly treated
    /// a hopeless word like `qqqzzz` as if it had matches.
    #[must_use]
    pub fn words_without_postings(&self, query: &CompiledQuery) -> Vec<String> {
        query
            .words
            .iter()
            .filter(|word| !word.stems.iter().any(|s| self.postings.contains_key(s)))
            .map(|word| word.raw.clone())
            .collect()
    }

    /// Did-you-mean for each positive query word none of whose stems occur in
    /// the dictionary: up to three close terms (Jaro-Winkler ≥ 0.85 or
    /// Levenshtein ≤ 2), ordered by Damerau-Levenshtein distance (transposition-
    /// aware, so `snapshto` → `snapshot` is 1 edit, not the 2 plain Levenshtein
    /// counts) then document frequency descending, then alphabetically for
    /// determinism (UX-1 / DEC-357). Candidates are dictionary stems, not
    /// surface words.
    #[must_use]
    pub fn suggest(&self, query: &CompiledQuery) -> Vec<TermSuggestion> {
        let mut out: Vec<TermSuggestion> = Vec::new();
        for word in &query.words {
            if word.stems.iter().any(|s| self.postings.contains_key(s)) {
                continue;
            }
            if out.iter().any(|s| s.term == word.raw) {
                continue;
            }
            let lowered = word.raw.to_lowercase();
            let mut probes: Vec<&str> = vec![lowered.as_str()];
            probes.extend(word.stems.iter().map(String::as_str));
            let probes = dedup(probes);
            // (term, docs, best-probe Damerau-Levenshtein distance).
            let mut candidates: Vec<(&str, usize, usize)> = self
                .postings
                .iter()
                .filter(|(term, _)| !probes.contains(&term.as_str()))
                .filter_map(|(term, posts)| {
                    let distance = probes
                        .iter()
                        .filter(|p| close_enough(p, term))
                        .map(|p| strsim::damerau_levenshtein(p, term))
                        .min()?;
                    Some((term.as_str(), posts.len(), distance))
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            candidates.sort_unstable_by(|a, b| {
                a.2.cmp(&b.2)
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.cmp(b.0))
            });
            candidates.truncate(SUGGEST_MAX_CANDIDATES);
            out.push(TermSuggestion {
                term: word.raw.clone(),
                candidates: candidates
                    .into_iter()
                    .map(|(term, docs, _)| TermCandidate {
                        term: term.to_owned(),
                        docs,
                    })
                    .collect(),
            });
        }
        out
    }
}

/// The query with every suggested word replaced by its top candidate.
/// `None` without suggestions (UX-1 / DEC-357: a query with two misspelled
/// words used to have only the first corrected, so the hinted command still
/// came up empty — `suggestions` already carried both fixes, the rewrite
/// just never applied the second one).
#[must_use]
pub fn corrected_query(query: &CompiledQuery, suggestions: &[TermSuggestion]) -> Option<String> {
    // Review fix (SHOULD-FIX 4): `suggest()` dedups by raw term, so a word
    // repeated in the query (`ostrch ostrch kangroo`) has only ONE
    // `TermSuggestion`, but every occurrence still needs its own edit --
    // `.find()` (singular) stopped at the first `QueryWord` with that raw
    // text and left the rest of the repeats uncorrected.
    let mut edits: Vec<(usize, usize, &str)> = suggestions
        .iter()
        .filter_map(|suggestion| {
            let candidate = suggestion.candidates.first()?;
            Some((suggestion, candidate.term.as_str()))
        })
        .flat_map(|(suggestion, replacement)| {
            query
                .words
                .iter()
                .filter(move |w| w.raw == suggestion.term)
                .map(move |w| (w.start, w.end, replacement))
        })
        .collect();
    if edits.is_empty() {
        return None;
    }
    edits.sort_unstable_by_key(|&(start, ..)| start);
    let mut out = String::with_capacity(query.source.len());
    let mut cursor = 0;
    for (start, end, replacement) in edits {
        out.push_str(query.source.get(cursor..start)?);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(query.source.get(cursor..)?);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25::{DocumentInput, PreTokenizedInput};

    fn en() -> Stemmer {
        create_stemmer(StemLanguage::English)
    }

    fn compile(query: &str) -> CompiledQuery {
        CompiledQuery::parse(query, &[StemLanguage::English]).unwrap()
    }

    fn term(s: &str) -> Node {
        Node::Term(vec![s.to_owned()])
    }

    fn corpus(docs: &[(&str, &str)]) -> Bm25InvertedIndex {
        Bm25InvertedIndex::build(
            docs.iter()
                .map(|(path, body)| DocumentInput {
                    rel_path: (*path).to_owned(),
                    title: String::new(),
                    body: (*body).to_owned(),
                    language: StemLanguage::English,
                    ..Default::default()
                })
                .collect(),
        )
    }

    fn hits(index: &Bm25InvertedIndex, query: &str) -> Vec<String> {
        let mut out: Vec<String> = index
            .score_compiled(&compile(query))
            .into_iter()
            .map(|m| m.rel_path)
            .collect();
        out.sort();
        out
    }

    #[test]
    fn or_binds_tighter_than_implicit_and() {
        let q = compile("a1 b1 OR c1");
        assert_eq!(
            q.root,
            Some(Node::And(vec![
                term("a1"),
                Node::Or(vec![term("b1"), term("c1")])
            ]))
        );
    }

    #[test]
    fn and_keyword_is_whitespace() {
        assert_eq!(compile("x1 AND y1").root, compile("x1 y1").root);
        assert_eq!(compile("x1 and y1").root, compile("x1 y1").root);
    }

    #[test]
    fn parentheses_group_and_nest() {
        let q = compile("(a1 OR b1) -c1");
        assert_eq!(
            q.root,
            Some(Node::And(vec![
                Node::Or(vec![term("a1"), term("b1")]),
                Node::Not(Box::new(term("c1"))),
            ]))
        );
        let nested = compile("((a1 OR (b1 c1)) d1)");
        assert_eq!(
            nested.root,
            Some(Node::And(vec![
                Node::Or(vec![term("a1"), Node::And(vec![term("b1"), term("c1")])]),
                term("d1"),
            ]))
        );
    }

    #[test]
    fn negation_applies_to_groups_and_phrases() {
        let q = compile("a1 -(b1 OR c1) -\"d1 e1\"");
        assert_eq!(
            q.root,
            Some(Node::And(vec![
                term("a1"),
                Node::Not(Box::new(Node::Or(vec![term("b1"), term("c1")]))),
                Node::Not(Box::new(Node::Phrase(
                    vec![vec!["d1".into(), "e1".into()]],
                    0
                ))),
            ]))
        );
    }

    #[test]
    fn unbalanced_and_empty_groups_are_errors() {
        for bad in ["(a b", "a b)", "()", "a ( ) b", "((a)", "a)"] {
            assert!(
                CompiledQuery::parse(bad, &[StemLanguage::English]).is_err(),
                "{bad} should be a syntax error"
            );
        }
        let deep = format!("{}a{}", "(".repeat(100), ")".repeat(100));
        assert!(CompiledQuery::parse(&deep, &[StemLanguage::English]).is_err());
    }

    #[test]
    fn parentheses_inside_a_word_are_literal() {
        assert_eq!(compile("main()").root, Some(term("main")));
        assert_eq!(
            compile("(f(x) OR y1)").root,
            Some(Node::Or(vec![
                Node::And(vec![term("f"), term("x")]),
                term("y1")
            ]))
        );
    }

    #[test]
    fn dangling_operators_are_ignored() {
        assert_eq!(compile("OR a1").root, Some(term("a1")));
        assert_eq!(compile("a1 OR").root, Some(term("a1")));
        assert_eq!(compile("a1 - b1").root, compile("a1 b1").root);
        assert!(compile("").root.is_none());
        assert!(compile("   ").root.is_none());
        assert!(operator_only("AND OR"));
        assert!(!operator_only("a OR"));
    }

    // -- UX-6 / DEC-358: malformed-but-recoverable query warnings --

    #[test]
    fn dangling_or_is_flagged_both_directions() {
        assert!(compile("a1 OR").warnings().dangling_operator);
        assert!(compile("OR a1").warnings().dangling_operator);
        assert!(!compile("a1 OR b1").warnings().dangling_operator);
        assert!(!compile("a1 b1").warnings().dangling_operator);
    }

    #[test]
    fn unterminated_quote_is_flagged() {
        assert!(compile(r#"a1 "b1"#).warnings().unterminated_quote);
        assert!(!compile(r#"a1 "b1""#).warnings().unterminated_quote);
    }

    #[test]
    fn clamped_slop_is_flagged_with_the_original_value() {
        let w = compile(r#""a1 b1"~99"#).warnings().clone();
        assert_eq!(w.clamped_slops, vec![99]);
        let w = compile(r#""a1 b1"~30"#).warnings().clone();
        assert_eq!(w.clamped_slops, Vec::<u32>::new());
    }

    #[test]
    fn misplaced_wildcard_is_flagged() {
        assert_eq!(
            compile("*foo").warnings().misplaced_wildcard_terms,
            vec!["*foo".to_owned()]
        );
        assert_eq!(
            compile("sn*p").warnings().misplaced_wildcard_terms,
            vec!["sn*p".to_owned()]
        );
        // A trailing '*' is the real prefix syntax, not misplaced.
        assert_eq!(
            compile("config*").warnings().misplaced_wildcard_terms,
            Vec::<String>::new()
        );
    }

    #[test]
    fn malformed_phrase_slop_is_a_syntax_error() {
        assert!(CompiledQuery::parse(r#""a b"~abc"#, &[StemLanguage::English]).is_err());
        // A numeric slop, or none at all, is unaffected.
        assert!(CompiledQuery::parse(r#""a b"~3"#, &[StemLanguage::English]).is_ok());
        assert!(CompiledQuery::parse(r#""a b""#, &[StemLanguage::English]).is_ok());
    }

    #[test]
    fn phrase_content_starting_with_tilde_letter_is_not_malformed_slop() {
        // Review MUST-FIX 1: the malformed-slop check used to scan the whole
        // query for any '"' followed by '~' and a letter, which also fired
        // on the *opening* quote of an ordinary phrase whose content starts
        // with '~' -- `"~home dir"` has nothing to do with a slop suffix.
        assert!(CompiledQuery::parse(r#""~home dir""#, &[StemLanguage::English]).is_ok());
        let index = corpus(&[("a.md", "home dir"), ("b.md", "unrelated")]);
        assert_eq!(hits(&index, r#""~home dir""#), vec!["a.md"]);
    }

    #[test]
    fn negated_keyword_is_literal() {
        assert_eq!(
            compile("x1 -or").root,
            Some(Node::And(vec![term("x1"), Node::Not(Box::new(term("or")))]))
        );
    }

    #[test]
    fn prefix_terms_and_bare_star() {
        assert_eq!(
            compile("Conf*").root,
            Some(Node::Prefix(vec!["conf".into()]))
        );
        assert_eq!(
            compile("conf**").root,
            Some(Node::Prefix(vec!["conf".into()]))
        );
        assert_eq!(
            compile("std::f*").root,
            Some(Node::And(vec![term("std"), Node::Prefix(vec!["f".into()])]))
        );
        for bad in ["*", "**", "-*", "(*)"] {
            assert!(
                CompiledQuery::parse(bad, &[StemLanguage::English]).is_err(),
                "{bad}"
            );
        }
        // A quoted star is literal text, and a mid-word star splits.
        assert_eq!(compile("\"conf*\"").root, Some(term("conf")));
    }

    #[test]
    fn prefix_expansion_matches_stems_and_caps() {
        let index = corpus(&[
            ("a.md", "configuration file"),
            ("b.md", "configure the confluence"),
            ("c.md", "nothing here"),
        ]);
        assert_eq!(hits(&index, "conf*"), vec!["a.md", "b.md"]);
        assert_eq!(hits(&index, "confl*"), vec!["b.md"]);
        let got = index.capped_prefixes(&compile("conf*"));
        assert!(got.is_empty(), "expected empty, got {got:?}");

        let many: Vec<String> = (0..300).map(|i| format!("zeta{i:03}")).collect();
        let big = Bm25InvertedIndex::build_from_tokens(vec![PreTokenizedInput {
            rel_path: "big.md".into(),
            tokens: many.into(),
        }]);
        let q = compile("zeta*");
        assert_eq!(big.capped_prefixes(&q), vec![("zeta".to_owned(), 300)]);
        assert_eq!(
            big.expand_prefix(&["zeta".to_owned()]).len(),
            MAX_PREFIX_EXPANSION
        );
        assert_eq!(big.score_compiled(&q).len(), 1);
    }

    #[test]
    fn precedence_changes_mixed_or_results() {
        let index = corpus(&[
            ("1.md", "rust async"),
            ("2.md", "rust tokio"),
            ("3.md", "tokio only"),
            ("4.md", "async only"),
        ]);
        // Old global-OR meaning would have returned all four.
        assert_eq!(hits(&index, "rust async OR tokio"), vec!["1.md", "2.md"]);
        assert_eq!(hits(&index, "(rust OR tokio) -async"), vec!["2.md", "3.md"]);
        assert_eq!(hits(&index, "-(rust OR async) tokio"), vec!["3.md"]);
        // A query with no positive leaf matches nothing.
        let got = hits(&index, "-rust");
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }

    #[test]
    fn legacy_field_terms_are_reported_but_searched_as_words() {
        let q = compile("snapshot title:Dogfood -tag:x OR \"path:y\" HEADING:install");
        let got: Vec<(&str, &str)> = q
            .legacy_field_terms()
            .iter()
            .map(|t| (t.field.as_str(), t.value.as_str()))
            .collect();
        // Negated and quoted tokens are not positive words.
        assert_eq!(got, vec![("title", "Dogfood"), ("heading", "install")]);
        assert_eq!(
            q.without_legacy_field_terms(),
            "snapshot -tag:x OR \"path:y\""
        );
        assert_eq!(compile("foo:bar title:").legacy_field_terms(), &[]);
        assert_eq!(compile("title:x").without_legacy_field_terms(), "");
    }

    #[test]
    fn name_value_tokens_are_plain_words() {
        // DEC-366: there are no field terms. `title:`, `heading:`, `tag:`,
        // `path:` and any other `name:value` token tokenize like `foo:bar`,
        // `std::fs` or a URL always did.
        for (query, words) in [
            ("title:alpha", "title alpha"),
            ("TITLE:x1", "title x1"),
            ("heading:install", "heading install"),
            ("tag:project", "tag project"),
            ("path:notes/", "path notes"),
            ("foo:bar", "foo bar"),
        ] {
            assert_eq!(compile(query).root, compile(words).root, "{query}");
        }
        let index = corpus(&[
            ("a.md", "the title alpha is here"),
            ("b.md", "alpha without the other word"),
        ]);
        assert_eq!(hits(&index, "title:alpha"), vec!["a.md"]);
        assert_eq!(hits(&index, "-title:alpha alpha"), vec!["b.md"]);
        // A quoted value after a colon is an ordinary phrase.
        assert_eq!(
            compile("title:\"alpha beta\"").root,
            compile("title \"alpha beta\"").root
        );
    }

    #[test]
    fn multi_language_terms_are_alternatives() {
        let docs = vec![
            DocumentInput {
                rel_path: "de.md".into(),
                title: String::new(),
                body: "Die Häuser stehen am See".into(),
                language: StemLanguage::German,
                ..Default::default()
            },
            DocumentInput {
                rel_path: "en.md".into(),
                title: String::new(),
                body: "houses by the lake".into(),
                language: StemLanguage::English,
                ..Default::default()
            },
        ];
        let index = Bm25InvertedIndex::build(docs);
        let english_only = CompiledQuery::parse("Häusern", &[StemLanguage::English]).unwrap();
        assert!(index.score_compiled(&english_only).is_empty());
        let both = CompiledQuery::parse(
            "Häusern",
            &[
                StemLanguage::English,
                StemLanguage::German,
                StemLanguage::English,
            ],
        )
        .unwrap();
        let found: Vec<_> = index
            .score_compiled(&both)
            .into_iter()
            .map(|m| m.rel_path)
            .collect();
        assert_eq!(found, vec!["de.md"]);
        // Identical stems are deduplicated within a leaf.
        let q =
            CompiledQuery::parse("2024", &[StemLanguage::English, StemLanguage::German]).unwrap();
        match q.root {
            Some(Node::Term(stems)) => assert_eq!(stems.len(), 1),
            other => panic!("{other:?}"),
        }
        // Snippets share the expansion.
        let matcher = SnippetMatcher::from_compiled(&both);
        let de = create_stemmer(StemLanguage::German);
        assert_eq!(matcher.coverage(&tokenize("Die Häuser", &de)), 1);
    }

    #[test]
    fn snippet_matcher_counts_groups_and_prefixes() {
        let q = compile("conf* rust -python");
        let m = SnippetMatcher::from_compiled(&q);
        assert_eq!(m.max_coverage(), 2);
        assert_eq!(m.coverage(&tokenize("configure rust", &en())), 2);
        assert_eq!(m.coverage(&tokenize("python only", &en())), 0);
        let phrase = SnippetMatcher::from_compiled(&compile("\"memory safety\""));
        assert_eq!(phrase.coverage(&tokenize("memory alone", &en())), 0);
        assert_eq!(phrase.coverage(&tokenize("memory safety", &en())), 2);
    }

    #[test]
    fn did_you_mean_finds_close_terms_and_rewrites() {
        let index = corpus(&[
            ("a.md", "stemming and stemmer"),
            ("b.md", "stemming again"),
            ("c.md", "unrelated words"),
        ]);
        let q = compile("stemmng -zzz unrelated");
        assert!(index.score_compiled(&q).is_empty());
        let suggestions = index.suggest(&q);
        assert_eq!(suggestions.len(), 1, "{suggestions:?}");
        assert_eq!(suggestions[0].term, "stemmng");
        // Similarity ranks first: `stemmer` (2 edits) beats the more
        // frequent but less similar `stem`.
        let ranked: Vec<_> = suggestions[0]
            .candidates
            .iter()
            .map(|c| (c.term.as_str(), c.docs))
            .collect();
        assert_eq!(ranked, vec![("stemmer", 1), ("stem", 2)]);
        assert_eq!(
            corrected_query(&q, &suggestions).as_deref(),
            Some("stemmer -zzz unrelated")
        );
        let got = index.suggest(&compile("zzzzqqq"));
        assert!(got.is_empty(), "expected empty, got {got:?}");
        assert!(corrected_query(&compile("x"), &[]).is_none());
    }

    #[test]
    fn words_without_postings_reports_a_hopeless_word_suggest_drops() {
        // Review fix (SHOULD-FIX 5): `suggest()` silently drops a word with
        // no close dictionary candidate at all (`zzzzqqq`, just confirmed
        // empty above), so a caller cannot use `suggest()`'s output as a
        // stand-in for "this word has zero postings" -- `qqqzzz wwwxxx`
        // must still show up here even though `suggest()` says nothing
        // about either.
        let index = corpus(&[("a.md", "stemming and stemmer")]);
        let q = compile("qqqzzz wwwxxx");
        assert!(index.suggest(&q).is_empty(), "no close candidates exist");
        let mut without = index.words_without_postings(&q);
        without.sort();
        assert_eq!(without, vec!["qqqzzz".to_owned(), "wwwxxx".to_owned()]);

        // A word that does have postings is absent from the list.
        let q2 = compile("stemmer qqqzzz");
        assert_eq!(index.words_without_postings(&q2), vec!["qqqzzz".to_owned()]);
    }

    #[test]
    fn did_you_mean_orders_by_damerau_levenshtein_then_docs() {
        // UX-1 / DEC-357: the query is one adjacent-transposition away from
        // two different dictionary terms (Damerau distance 1 each) and two
        // plain substitutions away from a third (Damerau distance 2). Plain
        // (non-Damerau) Levenshtein rates all three equally far (2 edits,
        // same length), which is exactly the tie the old ranking broke on
        // document frequency rather than true edit distance — so the
        // Damerau-2 term, given far more documents, must NOT outrank either
        // Damerau-1 term; and between the two Damerau-1 terms, the one with
        // more documents must come first.
        fn docs(word: &'static str, n: usize) -> Vec<(String, &'static str)> {
            (0..n).map(|i| (format!("{word}-{i}.md"), word)).collect()
        }
        let mut pairs: Vec<(String, &str)> = Vec::new();
        pairs.extend(docs("qxwyzt", 5)); // Damerau 1 (transpose pos 2,3), more docs
        pairs.extend(docs("qwxytz", 2)); // Damerau 1 (transpose pos 5,6), fewer docs
        pairs.extend(docs("qwxyab", 20)); // Damerau 2 (two substitutions), most docs
        let docs_ref: Vec<(&str, &str)> = pairs.iter().map(|(p, b)| (p.as_str(), *b)).collect();
        let index = corpus(&docs_ref);

        let q = compile("qwxyzt");
        assert!(index.score_compiled(&q).is_empty());
        let suggestions = index.suggest(&q);
        assert_eq!(suggestions.len(), 1, "{suggestions:?}");
        let ranked: Vec<_> = suggestions[0]
            .candidates
            .iter()
            .map(|c| (c.term.as_str(), c.docs))
            .collect();
        assert_eq!(
            ranked,
            vec![("qxwyzt", 5), ("qwxytz", 2), ("qwxyab", 20)],
            "Damerau distance must beat document frequency as the primary key"
        );
    }

    #[test]
    fn corrected_query_fixes_every_misspelled_term() {
        // UX-1 / DEC-357: a query with two misspelled words used to have
        // only the first one corrected, so the hinted command still
        // returned nothing even though `suggestions` carried both fixes.
        let index = corpus(&[
            ("a.md", "ostrich kangaroo"),
            ("b.md", "ostrich only"),
            ("c.md", "kangaroo only"),
        ]);
        let q = compile("ostrch kangroo");
        assert!(index.score_compiled(&q).is_empty());
        let suggestions = index.suggest(&q);
        assert_eq!(suggestions.len(), 2, "{suggestions:?}");
        assert_eq!(
            corrected_query(&q, &suggestions).as_deref(),
            Some("ostrich kangaroo")
        );
        assert!(
            !index
                .score_compiled(&compile("ostrich kangaroo"))
                .is_empty()
        );
    }

    #[test]
    fn corrected_query_fixes_a_repeated_misspelled_word() {
        // Review fix (SHOULD-FIX 4): `suggest()` dedups by raw term, so a
        // word repeated in the query had only one `TermSuggestion`, and
        // `corrected_query`'s old `.find()` (singular) corrected only the
        // *first* occurrence, leaving the second one typo'd:
        // `ostrch ostrch kangroo` -> `'ostrich ostrch kangaroo'`.
        let index = corpus(&[
            ("a.md", "ostrich kangaroo"),
            ("b.md", "ostrich only"),
            ("c.md", "kangaroo only"),
        ]);
        let q = compile("ostrch ostrch kangroo");
        let suggestions = index.suggest(&q);
        assert_eq!(suggestions.len(), 2, "{suggestions:?}");
        assert_eq!(
            corrected_query(&q, &suggestions).as_deref(),
            Some("ostrich ostrich kangaroo")
        );
    }

    #[test]
    fn did_you_mean_reaches_a_misspelled_word_inside_a_phrase() {
        // UX-1 / DEC-357: only the plain-word compile path used to register
        // a `QueryWord` for did-you-mean, so a typo inside a quoted phrase
        // (`"stale indx"`) silently got `suggestions: null` instead of a fix.
        let index = corpus(&[
            ("a.md", "stale index repair"),
            ("b.md", "stale bread"),
            ("c.md", "index of contents"),
        ]);
        let q = compile(r#""stale indx""#);
        assert!(index.score_compiled(&q).is_empty());
        let suggestions = index.suggest(&q);
        assert_eq!(suggestions.len(), 1, "{suggestions:?}");
        assert_eq!(suggestions[0].term, "indx");
        assert_eq!(suggestions[0].candidates[0].term, "index");
        assert_eq!(
            corrected_query(&q, &suggestions).as_deref(),
            Some(r#""stale index""#)
        );
    }

    #[test]
    fn empty_negated_phrase_negates_nothing() {
        let index = corpus(&[("a.md", "snapshot index"), ("b.md", "other words")]);
        assert_eq!(compile("-\"\" snapshot").root, compile("snapshot").root);
        assert_eq!(hits(&index, "-\"\" snapshot"), vec!["a.md"]);
        assert_eq!(hits(&index, "-\"\" -\"\" snapshot"), vec!["a.md"]);
        // 30 000 empty negated phrases neither overflow the stack nor negate.
        let many = format!("{} snapshot", "-\"\"".repeat(30_000));
        assert_eq!(hits(&index, &many), vec!["a.md"]);
    }

    #[test]
    fn consecutive_negations_are_bounded_and_collapse() {
        let parse_lexemes = |n: usize| {
            let mut lexemes = vec![Lexeme::Not; n];
            lexemes.push(Lexeme::Word {
                text: "x1".into(),
                start: 0,
                end: 2,
            });
            Parser {
                lexemes,
                pos: 0,
                warnings: QueryWarnings::default(),
            }
            .parse_and(0)
        };
        let two = parse_lexemes(2).unwrap();
        assert_eq!(
            two,
            vec![RawNode::Word {
                text: "x1".into(),
                start: 0,
                end: 2
            }]
        );
        assert!(matches!(parse_lexemes(3).unwrap()[0], RawNode::Not(_)));
        assert!(parse_lexemes(MAX_GROUP_DEPTH).is_ok());
        assert!(parse_lexemes(MAX_GROUP_DEPTH + 1).is_err());
        assert!(parse_lexemes(100_000).is_err());
        // Deep negated groups hit the parenthesis limit, not the stack.
        let deep = format!("{}a{}", "-(".repeat(10_000), ")".repeat(10_000));
        assert!(CompiledQuery::parse(&deep, &[StemLanguage::English]).is_err());
    }

    #[test]
    fn whole_word_prefix_falls_back_to_its_stem() {
        let index = corpus(&[
            ("a.md", "configuration file"),
            ("b.md", "happiness matters"),
            ("c.md", "nothing"),
        ]);
        // `configuration` is no stem prefix (the stem is `configur`).
        assert_eq!(hits(&index, "configuration*"), vec!["a.md"]);
        assert_eq!(hits(&index, "happiness*"), vec!["b.md"]);
        // The raw prefix wins whenever it matches something.
        assert_eq!(hits(&index, "conf*"), vec!["a.md"]);
        assert!(hits(&index, "zzzzz*").is_empty(), "no fallback hit");
        let m = SnippetMatcher::from_compiled(&compile("configuration*"));
        assert_eq!(m.coverage(&tokenize("configuration here", &en())), 1);
    }

    #[test]
    fn prefix_unions_raw_match_with_stem_fallback() {
        // BUG-10 / DEC-355's sibling fix: a one-document typo that happens
        // to share the raw prefix ("configurationon" starts with
        // "configuration") used to count as "the raw prefix matched
        // something", which suppressed the stem fallback entirely and made
        // `configuration*` miss every document that only has the ordinary
        // word "configuration" (stem `configur`). The two must union.
        let index = corpus(&[
            ("a.md", "configuration file"),
            ("b.md", "configurationon typo"),
            ("c.md", "unrelated"),
        ]);
        let mut found = hits(&index, "configuration*");
        found.sort();
        assert_eq!(
            found,
            vec!["a.md".to_owned(), "b.md".to_owned()],
            "configuration* must find both the stem match (a.md) and the raw-prefix typo match (b.md)"
        );
        // config* already found both via the stem alone; union must not
        // regress it (and must not double-count a term matched by both).
        let mut config_found = hits(&index, "config*");
        config_found.sort();
        assert_eq!(config_found, vec!["a.md".to_owned(), "b.md".to_owned()]);
    }

    #[test]
    fn dictionary_lists_terms_by_frequency() {
        let index = corpus(&[("a.md", "link links linking"), ("b.md", "link alpha")]);
        let all = index.dictionary(None);
        assert_eq!(all[0], ("link", 2));
        assert_eq!(index.dictionary(Some("AL")), vec![("alpha", 1)]);
        let got = index.dictionary(Some("zz"));
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }
}
