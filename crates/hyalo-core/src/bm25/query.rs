//! Ranked-search query language (iteration 302, DEC-333).
//!
//! Grammar, loosest binding first:
//!
//! ```text
//! query   := and_expr
//! and_expr:= or_expr ( ["AND"] or_expr )*      -- implicit AND
//! or_expr := unary ( "OR" unary )*             -- OR binds tighter than AND
//! unary   := "-" unary | primary               -- negates a term, phrase or group
//! primary := WORD | PREFIX* | "PHRASE" | FIELD:value | "(" and_expr ")"
//! ```
//!
//! A parsed query is compiled once per stemming language present in the
//! corpus: every term leaf becomes the alternatives of its per-language stems
//! (identical stems deduplicated), so a German note is found by its German
//! inflection even when the vault default language is English.
//!
//! Field terms (`title:`, `heading:`, `tag:`, `path:`) are per-document
//! predicates evaluated from index metadata, never from the token stream, so
//! the snapshot format and [`super::TOKENIZER_VERSION`] are unchanged.

use std::borrow::Cow;
use std::collections::HashMap;

use rust_stemmers::Stemmer;

use super::{Bm25InvertedIndex, Bm25Match, StemLanguage, create_stemmer, tokenize};

/// Most dictionary terms one `prefix*` term may expand to. Further terms are
/// dropped (most frequent kept) and [`Bm25InvertedIndex::capped_prefixes`]
/// reports the prefix so the caller can warn.
pub const MAX_PREFIX_EXPANSION: usize = 256;

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

/// Document field a `field:value` term tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Stemmed tokens of the promoted title (property, H1, filename).
    Title,
    /// Stemmed tokens of any heading.
    Heading,
    /// A tag, matched with the same prefix rule as `--tag`.
    Tag,
    /// Case-insensitive substring of the vault-relative path.
    Path,
}

impl FieldKind {
    fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "title" => Some(Self::Title),
            "heading" => Some(Self::Heading),
            "tag" => Some(Self::Tag),
            "path" => Some(Self::Path),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum FieldText {
    Word(String),
    Phrase(String),
}

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
    Phrase(String),
    Field {
        kind: FieldKind,
        value: FieldText,
    },
}

/// Read a quoted phrase body after its opening quote. An unterminated quote
/// runs to the end of the query (the historical behaviour).
fn read_phrase(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) -> String {
    let mut phrase = String::new();
    for (_, c) in chars.by_ref() {
        if c == '"' {
            break;
        }
        phrase.push(c);
    }
    phrase
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

fn lex(query: &str) -> Vec<Lexeme> {
    let mut out = Vec::new();
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
                let phrase = read_phrase(&mut chars);
                if !phrase.is_empty() {
                    out.push(Lexeme::Phrase(phrase));
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
                        let phrase = read_phrase(&mut chars);
                        if !phrase.is_empty() {
                            out.push(Lexeme::Not);
                            out.push(Lexeme::Phrase(phrase));
                        }
                    }
                    Some(&(_, '(')) => out.push(Lexeme::Not),
                    Some(_) => {
                        out.push(Lexeme::Not);
                        // A negated word is always literal: `-or` excludes "or".
                        let (text, start, end) = read_word(query, &mut chars);
                        out.push(classify_word(text, start, end, &mut chars, false));
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
                out.push(classify_word(text, start, end, &mut chars, true));
            }
        }
    }
    out
}

fn classify_word(
    text: String,
    start: usize,
    end: usize,
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    keywords: bool,
) -> Lexeme {
    if keywords && text.eq_ignore_ascii_case("or") {
        return Lexeme::Or;
    }
    if keywords && text.eq_ignore_ascii_case("and") {
        return Lexeme::And;
    }
    if let Some((name, rest)) = text.split_once(':')
        && let Some(kind) = FieldKind::parse(name)
    {
        if !rest.is_empty() {
            return Lexeme::Field {
                kind,
                value: FieldText::Word(rest.to_owned()),
            };
        }
        if matches!(chars.peek(), Some(&(_, '"'))) {
            chars.next();
            let phrase = read_phrase(chars);
            return Lexeme::Field {
                kind,
                value: FieldText::Phrase(phrase),
            };
        }
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
    Phrase(String),
    Field {
        kind: FieldKind,
        value: FieldText,
    },
}

struct Parser {
    lexemes: Vec<Lexeme>,
    pos: usize,
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
                // `AND` is whitespace; an `OR` with no left operand is ignored.
                Some(Lexeme::And | Lexeme::Or) => self.pos += 1,
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
            // A trailing `OR` (end, `)` or `AND`) has no right operand: ignored.
            if matches!(self.peek(), None | Some(Lexeme::Close | Lexeme::And)) {
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
            Lexeme::Phrase(text) => Ok(RawNode::Phrase(text)),
            Lexeme::Field { kind, value } => Ok(RawNode::Field { kind, value }),
            // Unreachable from parse_and/parse_or, which consume these first.
            Lexeme::Close | Lexeme::Or | Lexeme::And | Lexeme::Not => Err(QuerySyntaxError::new(
                "'-' must be followed by a term, phrase or group",
            )),
        }
    }
}

fn parse(query: &str) -> Result<RawNode, QuerySyntaxError> {
    let mut parser = Parser {
        lexemes: lex(query),
        pos: 0,
    };
    Ok(RawNode::And(parser.parse_and(0)?))
}

// ---------------------------------------------------------------------------
// Compiled (stemmed) AST
// ---------------------------------------------------------------------------

/// What a field term compares against.
#[derive(Debug, Clone, PartialEq)]
enum FieldValue {
    /// Alternative stemmed token sequences (one per query language, deduped).
    Sequences(Vec<Vec<String>>),
    /// Lowercase prefix tested against each stemmed token (`title:conf*`).
    Prefix(Vec<String>),
    /// Literal text: a tag query or a lowercase path substring.
    Literal(String),
}

/// A compiled `field:value` predicate.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldTerm {
    kind: FieldKind,
    value: FieldValue,
}

/// Index metadata a field term is evaluated against.
pub struct FieldDocument<'a> {
    /// Promoted title (frontmatter, first H1, filename stem).
    pub title: Cow<'a, str>,
    /// Every heading text in the document.
    pub headings: Vec<&'a str>,
    /// Frontmatter tags.
    pub tags: &'a [String],
    /// The document's stemming language.
    pub language: StemLanguage,
}

/// Supplies [`FieldDocument`]s by vault-relative path. `path:` never needs one.
pub trait FieldSource {
    /// Metadata for `rel_path`, or `None` when unknown (field terms then fail).
    fn field_document(&self, rel_path: &str) -> Option<FieldDocument<'_>>;
}

/// A [`FieldSource`] that knows no document: only `path:` terms can match.
pub struct NoFields;

impl FieldSource for NoFields {
    fn field_document(&self, _rel_path: &str) -> Option<FieldDocument<'_>> {
        None
    }
}

impl FieldTerm {
    /// The field this term tests.
    #[must_use]
    pub fn kind(&self) -> FieldKind {
        self.kind
    }

    fn matches_tokens(&self, tokens: &[String]) -> bool {
        match &self.value {
            FieldValue::Sequences(alternatives) => alternatives.iter().any(|seq| {
                !seq.is_empty()
                    && tokens
                        .windows(seq.len())
                        .any(|window| window == seq.as_slice())
            }),
            FieldValue::Prefix(candidates) => tokens
                .iter()
                .any(|t| candidates.iter().any(|p| t.starts_with(p.as_str()))),
            FieldValue::Literal(_) => false,
        }
    }

    fn matches(
        &self,
        rel_path: &str,
        source: &dyn FieldSource,
        stemmers: &mut HashMap<StemLanguage, Stemmer>,
    ) -> bool {
        if self.kind == FieldKind::Path {
            return match &self.value {
                FieldValue::Literal(needle) => rel_path.to_lowercase().contains(needle.as_str()),
                _ => false,
            };
        }
        let Some(doc) = source.field_document(rel_path) else {
            return false;
        };
        match self.kind {
            FieldKind::Tag => match &self.value {
                FieldValue::Literal(query) => doc
                    .tags
                    .iter()
                    .any(|tag| crate::filter::tag_matches(tag, query)),
                _ => false,
            },
            FieldKind::Title | FieldKind::Heading => {
                let stemmer = stemmers
                    .entry(doc.language)
                    .or_insert_with(|| create_stemmer(doc.language));
                if self.kind == FieldKind::Title {
                    self.matches_tokens(&tokenize(&doc.title, stemmer))
                } else {
                    doc.headings
                        .iter()
                        .any(|heading| self.matches_tokens(&tokenize(heading, stemmer)))
                }
            }
            FieldKind::Path => false,
        }
    }
}

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
    /// Alternative consecutive stem sequences, one per query language.
    Phrase(Vec<Vec<String>>),
    Field(FieldTerm),
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

/// One normalized query shared by indexed scoring, disk fallback and snippets.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub(super) root: Option<Node>,
    words: Vec<QueryWord>,
    source: String,
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
            RawNode::Phrase(text) => {
                let mut alternatives = sequences(&text, self.stemmers);
                if alternatives.iter().all(|seq| seq.len() == 1) && !alternatives.is_empty() {
                    Some(Node::Term(alternatives.drain(..).flatten().collect()))
                } else {
                    (!alternatives.is_empty()).then_some(Node::Phrase(alternatives))
                }
            }
            RawNode::Field { kind, value } => self.field(kind, value)?.map(Node::Field),
        })
    }

    fn word(
        &mut self,
        text: &str,
        start: usize,
        end: usize,
        positive: bool,
    ) -> Result<Option<Node>, QuerySyntaxError> {
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
        let positions = positional_alternatives(text, self.stemmers);
        if positive && positions.len() == 1 {
            self.words.push(QueryWord {
                raw: text.to_owned(),
                start,
                end,
                stems: positions[0].clone(),
            });
        }
        Ok(group(
            positions.into_iter().map(Node::Term).collect(),
            Node::And,
        ))
    }

    fn field(
        &self,
        kind: FieldKind,
        value: FieldText,
    ) -> Result<Option<FieldTerm>, QuerySyntaxError> {
        let (text, quoted) = match value {
            FieldText::Word(text) => (text, false),
            FieldText::Phrase(text) => (text, true),
        };
        let value = match kind {
            FieldKind::Tag => {
                let tag = text.trim().trim_start_matches('#');
                if tag.is_empty() {
                    return Ok(None);
                }
                FieldValue::Literal(tag.to_owned())
            }
            FieldKind::Path => {
                if text.is_empty() {
                    return Ok(None);
                }
                FieldValue::Literal(text.to_lowercase())
            }
            FieldKind::Title | FieldKind::Heading => {
                if !quoted && let Some(body) = text.strip_suffix('*') {
                    let prefix = body.trim_end_matches('*').to_lowercase();
                    if !prefix.chars().any(char::is_alphanumeric) {
                        return Err(QuerySyntaxError::new(format!(
                            "'{text}': a prefix term needs at least one letter or digit before '*'"
                        )));
                    }
                    FieldValue::Prefix(prefix_candidates(&prefix, self.stemmers))
                } else {
                    let alternatives = sequences(&text, self.stemmers);
                    if alternatives.is_empty() {
                        return Ok(None);
                    }
                    FieldValue::Sequences(alternatives)
                }
            }
        };
        Ok(Some(FieldTerm { kind, value }))
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
        }
    }

    fn parse_with_stemmers(query: &str, stemmers: &[&Stemmer]) -> Result<Self, QuerySyntaxError> {
        let raw = parse(query)?;
        let mut compiler = Compiler {
            stemmers,
            words: Vec::new(),
        };
        let root = compiler.compile(raw, true)?;
        Ok(Self {
            root,
            words: compiler.words,
            source: query.to_owned(),
        })
    }

    /// `true` when the query has a positive (non-negated) leaf of any kind.
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

    /// `true` when a positive leaf searches body text (term, prefix, phrase).
    /// A query of field terms only ranks nothing: every hit scores 0.
    #[must_use]
    pub fn has_text_terms(&self) -> bool {
        let mut found = false;
        self.visit_positive_text(&mut |_| found = true);
        found
    }

    /// `true` when the query uses a field term anywhere.
    #[must_use]
    pub fn has_field_terms(&self) -> bool {
        fn walk(node: &Node) -> bool {
            match node {
                Node::And(c) | Node::Or(c) => c.iter().any(walk),
                Node::Not(n) => walk(n),
                Node::Field(_) => true,
                _ => false,
            }
        }
        self.root.as_ref().is_some_and(walk)
    }

    fn visit_positive_text<'a>(&'a self, f: &mut dyn FnMut(&'a Node)) {
        fn walk<'a>(node: &'a Node, positive: bool, f: &mut dyn FnMut(&'a Node)) {
            match node {
                Node::And(c) | Node::Or(c) => c.iter().for_each(|n| walk(n, positive, f)),
                Node::Not(n) => walk(n, !positive, f),
                Node::Term(_) | Node::Prefix(_) | Node::Phrase(_) if positive => f(node),
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

/// Lexeme-level check used to explain an empty query made only of operators.
pub(super) fn operator_only(query: &str) -> bool {
    let lexemes = lex(query);
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

/// Positive leaves compiled for snippet qualification and coverage counting.
#[derive(Debug, Clone, Default)]
pub(super) struct SnippetMatcher {
    groups: Vec<SnippetGroup>,
    phrases: Vec<Vec<String>>,
    singles: Vec<SnippetGroup>,
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
            Node::Phrase(alternatives) => {
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
                matcher.phrases.extend(alternatives.iter().cloned());
            }
            _ => {}
        });
        matcher
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
            || self.phrases.iter().any(|seq| {
                !seq.is_empty() && tokens.windows(seq.len()).any(|w| w == seq.as_slice())
            });
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
    fields: &'a dyn FieldSource,
    stemmers: HashMap<StemLanguage, Stemmer>,
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

    fn phrase_docs(&self, seq: &[String]) -> DocSet {
        let mut set = DocSet::empty(self.len());
        for id in self.index.docs_with_all_terms(seq) {
            if self.index.has_phrase_at_positions(seq, id) {
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
            Node::Phrase(alternatives) => {
                let units: Vec<_> = alternatives
                    .iter()
                    .map(|seq| {
                        (
                            seq.iter().map(String::as_str).collect(),
                            self.phrase_docs(seq),
                        )
                    })
                    .collect();
                self.leaf(units.into_iter(), positive)
            }
            Node::Field(term) => {
                let mut set = DocSet::empty(self.len());
                for (id, path) in self.index.doc_paths.iter().enumerate() {
                    if term.matches(path, self.fields, &mut self.stemmers)
                        && let Ok(id) = u32::try_from(id)
                    {
                        set.insert(id);
                    }
                }
                set
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
    /// when it matches any dictionary term, else every stem-derived fallback
    /// that does (`configuration*` → `configur`).
    fn effective_prefixes<'p>(&self, candidates: &'p [String]) -> Vec<&'p str> {
        let matches = |p: &str| self.postings.keys().any(|t| t.starts_with(p));
        match candidates.split_first() {
            Some((raw, _)) if matches(raw) => vec![raw.as_str()],
            Some((_, fallbacks)) => fallbacks
                .iter()
                .map(String::as_str)
                .filter(|p| matches(p))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Documents for which `node`, read with positive polarity, evaluates
    /// true — the file-level verdict of one leaf (section scoring uses it for
    /// field terms and negated leaves, DEC-334).
    pub(super) fn node_docs(&self, node: &Node, fields: &dyn FieldSource) -> DocSet {
        let mut evaluator = Evaluator {
            index: self,
            fields,
            stemmers: HashMap::new(),
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

    /// Score a compiled query whose field terms resolve through `fields`.
    ///
    /// A document matches when the query tree evaluates true for it. Its score
    /// sums BM25 contributions of every positive text leaf it satisfies; a
    /// match through field terms or negations alone scores 0. A query with no
    /// positive leaf at all matches nothing.
    #[must_use]
    pub fn score_with_fields(
        &self,
        query: &CompiledQuery,
        fields: &dyn FieldSource,
    ) -> Vec<Bm25Match> {
        let Some(root) = &query.root else {
            return Vec::new();
        };
        if !query.has_positive_leaf() {
            return Vec::new();
        }
        let mut evaluator = Evaluator {
            index: self,
            fields,
            stemmers: HashMap::new(),
            units: Vec::new(),
        };
        let admitted = evaluator.eval(root, true);

        #[allow(clippy::cast_precision_loss)]
        let n = self.doc_paths.len() as f64;
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
                    let tf = f64::from(p.term_freq);
                    let dl = f64::from(self.doc_lengths[p.doc_id as usize]);
                    let tf_norm = (tf * (Self::K1 + 1.0))
                        / (tf + Self::K1 * (1.0 - Self::B + Self::B * dl / self.avgdl));
                    if let Some(score) = scores.get_mut(&p.doc_id) {
                        *score += idf * tf_norm;
                    }
                }
            }
        }
        let mut matches: Vec<Bm25Match> = scores
            .into_iter()
            .map(|(doc_id, score)| Bm25Match {
                rel_path: self.doc_paths[doc_id as usize].clone(),
                score,
            })
            .collect();
        // Score descending, then path: deterministic across index and disk.
        matches.sort_unstable_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.rel_path.cmp(&b.rel_path))
        });
        matches
    }

    /// Did-you-mean for each positive query word none of whose stems occur in
    /// the dictionary: up to three close terms (Jaro-Winkler ≥ 0.85 or
    /// Levenshtein ≤ 2), most similar first with document frequency as the
    /// tie-break. Candidates are dictionary stems, not surface words.
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
            // (term, docs, edit similarity, Jaro-Winkler), best probe each.
            let mut candidates: Vec<(&str, usize, f64, f64)> = self
                .postings
                .iter()
                .filter(|(term, _)| !probes.contains(&term.as_str()))
                .filter_map(|(term, posts)| {
                    let (edit, jw) = probes
                        .iter()
                        .filter(|p| close_enough(p, term))
                        .map(|p| {
                            (
                                strsim::normalized_levenshtein(p, term),
                                strsim::jaro_winkler(p, term),
                            )
                        })
                        .fold(None, |best: Option<(f64, f64)>, s| {
                            Some(best.map_or(s, |b| if s > b { s } else { b }))
                        })?;
                    Some((term.as_str(), posts.len(), edit, jw))
                })
                .collect();
            if candidates.is_empty() {
                continue;
            }
            // Fewest edits first (normalised Levenshtein), then Jaro-Winkler;
            // document frequency only breaks ties.
            candidates.sort_unstable_by(|a, b| {
                b.2.partial_cmp(&a.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal))
                    .then_with(|| b.1.cmp(&a.1))
                    .then_with(|| a.0.cmp(b.0))
            });
            candidates.truncate(SUGGEST_MAX_CANDIDATES);
            out.push(TermSuggestion {
                term: word.raw.clone(),
                candidates: candidates
                    .into_iter()
                    .map(|(term, docs, _, _)| TermCandidate {
                        term: term.to_owned(),
                        docs,
                    })
                    .collect(),
            });
        }
        out
    }
}

/// The query with the best single substitution applied: the first suggested
/// word replaced by its most frequent candidate. `None` without suggestions.
#[must_use]
pub fn corrected_query(query: &CompiledQuery, suggestions: &[TermSuggestion]) -> Option<String> {
    let suggestion = suggestions.first()?;
    let candidate = suggestion.candidates.first()?;
    let word = query.words.iter().find(|w| w.raw == suggestion.term)?;
    let mut out = String::with_capacity(query.source.len());
    out.push_str(query.source.get(..word.start)?);
    out.push_str(&candidate.term);
    out.push_str(query.source.get(word.end..)?);
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
                Node::Not(Box::new(Node::Phrase(vec![vec!["d1".into(), "e1".into()]]))),
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
        for bad in ["*", "**", "-*", "(*)", "title:*"] {
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
            tokens: many,
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

    struct MapFields(HashMap<&'static str, (&'static str, Vec<&'static str>, Vec<String>)>);

    impl FieldSource for MapFields {
        fn field_document(&self, rel_path: &str) -> Option<FieldDocument<'_>> {
            self.0
                .get(rel_path)
                .map(|(title, headings, tags)| FieldDocument {
                    title: Cow::Borrowed(*title),
                    headings: headings.clone(),
                    tags,
                    language: StemLanguage::English,
                })
        }
    }

    #[test]
    fn field_terms_are_predicates() {
        let index = corpus(&[
            ("iterations/a.md", "alpha links"),
            ("notes/b.md", "beta links"),
            ("notes/c.md", "gamma"),
        ]);
        let fields = MapFields(HashMap::from([
            (
                "iterations/a.md",
                (
                    "Iteration Planning",
                    vec!["Install steps"],
                    vec!["iteration".into()],
                ),
            ),
            (
                "notes/b.md",
                ("Running notes", vec!["Usage"], vec!["project/alpha".into()]),
            ),
            ("notes/c.md", ("Gamma", vec![], vec![])),
        ]));
        let run = |q: &str| -> Vec<(String, f64)> {
            let mut out: Vec<_> = index
                .score_with_fields(&compile(q), &fields)
                .into_iter()
                .map(|m| (m.rel_path, m.score))
                .collect();
            out.sort_by(|a, b| a.0.cmp(&b.0));
            out
        };
        let paths = |q: &str| run(q).into_iter().map(|(p, _)| p).collect::<Vec<_>>();
        assert_eq!(paths("title:iterations"), vec!["iterations/a.md"]);
        assert_eq!(paths("title:run"), vec!["notes/b.md"]);
        assert_eq!(paths("title:\"running notes\""), vec!["notes/b.md"]);
        assert_eq!(paths("title:\"notes running\""), Vec::<String>::new());
        assert_eq!(paths("title:iter*"), vec!["iterations/a.md"]);
        assert_eq!(paths("heading:install"), vec!["iterations/a.md"]);
        assert_eq!(paths("tag:project"), vec!["notes/b.md"]);
        assert_eq!(paths("tag:proj"), Vec::<String>::new());
        assert_eq!(paths("path:ITERATIONS/"), vec!["iterations/a.md"]);
        assert_eq!(paths("path:notes/ -tag:project"), vec!["notes/c.md"]);
        assert_eq!(
            paths("tag:iteration OR title:gamma"),
            vec!["iterations/a.md", "notes/c.md"]
        );
        // Field-only queries score 0; mixed queries rank by the text leaves.
        assert!(run("path:notes/").iter().all(|(_, s)| *s == 0.0));
        let mixed = run("links path:notes/");
        assert_eq!(mixed.len(), 1);
        assert!(mixed[0].1 > 0.0);
        // Unknown prefixes and URLs stay plain terms.
        assert!(!compile("foo:bar").has_field_terms());
        assert!(!compile("https://example.com").has_field_terms());
        assert!(compile("TITLE:x").has_field_terms());
        // Without a field source only path: can match.
        assert_eq!(index.score_compiled(&compile("tag:iteration")).len(), 0);
    }

    #[test]
    fn multi_language_terms_are_alternatives() {
        let docs = vec![
            DocumentInput {
                rel_path: "de.md".into(),
                title: String::new(),
                body: "Die Häuser stehen am See".into(),
                language: StemLanguage::German,
            },
            DocumentInput {
                rel_path: "en.md".into(),
                title: String::new(),
                body: "houses by the lake".into(),
                language: StemLanguage::English,
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
            Parser { lexemes, pos: 0 }.parse_and(0)
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
    fn dictionary_lists_terms_by_frequency() {
        let index = corpus(&[("a.md", "link links linking"), ("b.md", "link alpha")]);
        let all = index.dictionary(None);
        assert_eq!(all[0], ("link", 2));
        assert_eq!(index.dictionary(Some("AL")), vec![("alpha", 1)]);
        let got = index.dictionary(Some("zz"));
        assert!(got.is_empty(), "expected empty, got {got:?}");
    }
}
