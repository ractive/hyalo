//! BM25 ranked search with multi-language stemming support.
//!
//! This module provides:
//! - [`StemLanguage`]: enum of supported stemming languages with [`parse_language`]
//! - [`resolve_language`]: language precedence logic (frontmatter > CLI > config > English)
//! - [`tokenize`]: Unicode-aware tokenization + stemming pipeline
//! - [`Bm25Corpus`]: in-memory BM25 index built from [`DocumentInput`] values

use std::collections::HashSet;

use bm25::{EmbedderBuilder, Scorer, Tokenizer};
use rust_stemmers::{Algorithm, Stemmer};

// ---------------------------------------------------------------------------
// Language
// ---------------------------------------------------------------------------

/// A stemming language supported by the [`rust_stemmers`] crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StemLanguage {
    Arabic,
    Danish,
    Dutch,
    #[default]
    English,
    Finnish,
    French,
    German,
    Greek,
    Hungarian,
    Italian,
    Norwegian,
    Portuguese,
    Romanian,
    Russian,
    Spanish,
    Swedish,
    Tamil,
    Turkish,
}

impl StemLanguage {
    pub(crate) fn to_algorithm(self) -> Algorithm {
        match self {
            Self::Arabic => Algorithm::Arabic,
            Self::Danish => Algorithm::Danish,
            Self::Dutch => Algorithm::Dutch,
            Self::English => Algorithm::English,
            Self::Finnish => Algorithm::Finnish,
            Self::French => Algorithm::French,
            Self::German => Algorithm::German,
            Self::Greek => Algorithm::Greek,
            Self::Hungarian => Algorithm::Hungarian,
            Self::Italian => Algorithm::Italian,
            Self::Norwegian => Algorithm::Norwegian,
            Self::Portuguese => Algorithm::Portuguese,
            Self::Romanian => Algorithm::Romanian,
            Self::Russian => Algorithm::Russian,
            Self::Spanish => Algorithm::Spanish,
            Self::Swedish => Algorithm::Swedish,
            Self::Tamil => Algorithm::Tamil,
            Self::Turkish => Algorithm::Turkish,
        }
    }

    /// Returns the lowercase canonical name for this language variant.
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Arabic => "arabic",
            Self::Danish => "danish",
            Self::Dutch => "dutch",
            Self::English => "english",
            Self::Finnish => "finnish",
            Self::French => "french",
            Self::German => "german",
            Self::Greek => "greek",
            Self::Hungarian => "hungarian",
            Self::Italian => "italian",
            Self::Norwegian => "norwegian",
            Self::Portuguese => "portuguese",
            Self::Romanian => "romanian",
            Self::Russian => "russian",
            Self::Spanish => "spanish",
            Self::Swedish => "swedish",
            Self::Tamil => "tamil",
            Self::Turkish => "turkish",
        }
    }
}

/// Parses a language name string (case-insensitive) into a [`StemLanguage`].
///
/// Returns an error for unrecognised language names.
pub fn parse_language(s: &str) -> anyhow::Result<StemLanguage> {
    match s.to_lowercase().as_str() {
        "arabic" => Ok(StemLanguage::Arabic),
        "danish" => Ok(StemLanguage::Danish),
        "dutch" => Ok(StemLanguage::Dutch),
        "english" => Ok(StemLanguage::English),
        "finnish" => Ok(StemLanguage::Finnish),
        "french" => Ok(StemLanguage::French),
        "german" => Ok(StemLanguage::German),
        "greek" => Ok(StemLanguage::Greek),
        "hungarian" => Ok(StemLanguage::Hungarian),
        "italian" => Ok(StemLanguage::Italian),
        "norwegian" => Ok(StemLanguage::Norwegian),
        "portuguese" => Ok(StemLanguage::Portuguese),
        "romanian" => Ok(StemLanguage::Romanian),
        "russian" => Ok(StemLanguage::Russian),
        "spanish" => Ok(StemLanguage::Spanish),
        "swedish" => Ok(StemLanguage::Swedish),
        "tamil" => Ok(StemLanguage::Tamil),
        "turkish" => Ok(StemLanguage::Turkish),
        other => anyhow::bail!("unknown stemming language: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Language resolution
// ---------------------------------------------------------------------------

/// Resolves the stemming language from up to three optional sources.
///
/// Priority: `frontmatter_lang` > `cli_lang` > `config_lang` > [`StemLanguage::English`].
///
/// Each source is a language name string; unrecognised values are silently skipped.
pub fn resolve_language(
    frontmatter_lang: Option<&str>,
    cli_lang: Option<&str>,
    config_lang: Option<&str>,
) -> StemLanguage {
    for lang_str in [frontmatter_lang, cli_lang, config_lang]
        .into_iter()
        .flatten()
    {
        if let Ok(lang) = parse_language(lang_str) {
            return lang;
        }
    }
    StemLanguage::default()
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

/// Tokenizes `text` with Unicode-aware lowercasing, splits on non-alphanumeric chars, and stems
/// each token using `stemmer`.
pub fn tokenize(text: &str, stemmer: &Stemmer) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|word| stemmer.stem(word).into_owned())
        .collect()
}

// ---------------------------------------------------------------------------
// WhitespaceTokenizer (pre-tokenized text pass-through)
// ---------------------------------------------------------------------------

/// A simple tokenizer that splits on ASCII whitespace.
///
/// Used internally so the bm25 crate re-splits our pre-tokenized, space-joined token strings
/// without applying its own stemming or stop-word logic.
#[derive(Default)]
struct WhitespaceTokenizer;

impl Tokenizer for WhitespaceTokenizer {
    fn tokenize(&self, input: &str) -> Vec<String> {
        input.split_whitespace().map(String::from).collect()
    }
}

// ---------------------------------------------------------------------------
// Corpus types
// ---------------------------------------------------------------------------

/// Input for building a corpus from pre-tokenized data (e.g. stored in the snapshot index).
pub struct PreTokenizedInput {
    /// Relative path that uniquely identifies the document.
    pub rel_path: String,
    /// Already-stemmed tokens for this document (title + body, combined).
    pub tokens: Vec<String>,
}

/// Input data for a single document added to a [`Bm25Corpus`].
pub struct DocumentInput {
    /// Relative path that uniquely identifies the document.
    pub rel_path: String,
    /// Title text (from frontmatter or the first H1 heading).
    pub title: String,
    /// Full body text of the document.
    pub body: String,
    /// Stemming language to use for this document's content.
    pub language: StemLanguage,
}

/// Tokenize a [`DocumentInput`] into a [`PreTokenizedInput`].
///
/// Applies the same tokenization pipeline as [`Bm25Corpus::build`]: Unicode-aware
/// lowercasing, split on non-alphanumeric chars, then stemming with the document's
/// declared language. Useful when mixing indexed (pre-tokenized) and unindexed
/// (raw body) documents in a single corpus build.
pub fn tokenize_document(doc: DocumentInput) -> PreTokenizedInput {
    let stemmer = Stemmer::create(doc.language.to_algorithm());
    let combined = format!("{} {}", doc.title, doc.body);
    let tokens = tokenize(&combined, &stemmer);
    PreTokenizedInput {
        rel_path: doc.rel_path,
        tokens,
    }
}

/// A ranked search result returned by [`Bm25Corpus::search`] and [`Bm25Corpus::score`].
pub struct Bm25Match {
    /// Relative path of the matched document.
    pub rel_path: String,
    /// BM25 relevance score (higher is more relevant).
    pub score: f64,
}

/// Parsed query with positive search terms and negative exclusion terms.
struct ParsedQuery {
    /// Positive terms to search for (stemmed, joined with spaces for the embedder).
    positive: String,
    /// Negative terms to exclude (stemmed). A document containing any of these is filtered out.
    negative: HashSet<String>,
}

/// In-memory BM25 index built from a collection of [`DocumentInput`] values.
///
/// Build once with [`Bm25Corpus::build`], then call [`Bm25Corpus::search`] or
/// [`Bm25Corpus::score`] as many times as needed.
///
/// ## Query syntax
///
/// - `term1 term2` — all terms contribute to BM25 relevance score; docs matching
///   more terms rank higher (implicit relevance OR with additive scoring)
/// - `-term` — exclude documents containing this term (negation)
///
/// Examples: `"rust programming"`, `"rust -javascript"`, `"search -draft"`
pub struct Bm25Corpus {
    /// Relative paths ordered by their integer document ID (index into this vec).
    doc_ids: Vec<String>,
    /// Per-document token sets (stemmed) for negation filtering.
    doc_token_sets: Vec<HashSet<String>>,
    scorer: Scorer<usize, u32>,
    embedder: bm25::Embedder<u32, WhitespaceTokenizer>,
    /// Stemmer used to tokenize queries.
    query_stemmer: Stemmer,
}

impl Bm25Corpus {
    /// Builds a BM25 index from `docs`.
    ///
    /// Each document is tokenized with its own [`StemLanguage`]. The `query_language` stemmer is
    /// stored for use during [`Bm25Corpus::search`] and [`Bm25Corpus::score`].
    pub fn build(docs: Vec<DocumentInput>, query_language: StemLanguage) -> Self {
        // Pre-tokenize every document using its declared language.
        let token_lists: Vec<Vec<String>> = docs
            .iter()
            .map(|doc| {
                let stemmer = Stemmer::create(doc.language.to_algorithm());
                let combined = format!("{} {}", doc.title, doc.body);
                tokenize(&combined, &stemmer)
            })
            .collect();
        let pre_tokenized: Vec<String> = token_lists.iter().map(|t| t.join(" ")).collect();

        // Build per-document token sets for negation filtering.
        let doc_token_sets: Vec<HashSet<String>> = token_lists
            .into_iter()
            .map(|tokens| tokens.into_iter().collect())
            .collect();

        // Compute average document length (in stemmed tokens) across the corpus.
        let total_tokens: usize = pre_tokenized
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum();
        // avgdl is an approximation; precision loss from usize→f32 is acceptable here.
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let avgdl: f32 = if docs.is_empty() {
            256.0
        } else {
            (total_tokens as f64 / docs.len() as f64) as f32
        };

        let embedder = EmbedderBuilder::<u32, WhitespaceTokenizer>::with_avgdl(avgdl).build();
        let mut scorer = Scorer::<usize, u32>::new();

        let doc_ids: Vec<String> = docs.into_iter().map(|d| d.rel_path).collect();

        for (idx, pre_tok) in pre_tokenized.iter().enumerate() {
            let embedding = embedder.embed(pre_tok);
            scorer.upsert(&idx, embedding);
        }

        let query_stemmer = Stemmer::create(query_language.to_algorithm());

        Self {
            doc_ids,
            doc_token_sets,
            scorer,
            embedder,
            query_stemmer,
        }
    }

    /// Builds a BM25 index from pre-tokenized documents (e.g. stored in the snapshot index).
    ///
    /// Each document's tokens are already stemmed — no further tokenization is applied.
    /// The `query_language` stemmer is stored for use during [`Bm25Corpus::search`] and
    /// [`Bm25Corpus::score`].
    pub fn build_from_tokens(docs: Vec<PreTokenizedInput>, query_language: StemLanguage) -> Self {
        // Join each document's token list back into a space-separated string so that
        // the internal `WhitespaceTokenizer` can re-split it. This avoids changing the
        // embedder interface while still bypassing the main tokenization pipeline.
        let pre_tokenized: Vec<String> = docs.iter().map(|d| d.tokens.join(" ")).collect();

        let doc_token_sets: Vec<HashSet<String>> = docs
            .iter()
            .map(|d| d.tokens.iter().cloned().collect())
            .collect();

        let total_tokens: usize = pre_tokenized
            .iter()
            .map(|s| s.split_whitespace().count())
            .sum();
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let avgdl: f32 = if docs.is_empty() {
            256.0
        } else {
            (total_tokens as f64 / docs.len() as f64) as f32
        };

        let embedder = EmbedderBuilder::<u32, WhitespaceTokenizer>::with_avgdl(avgdl).build();
        let mut scorer = Scorer::<usize, u32>::new();

        let doc_ids: Vec<String> = docs.into_iter().map(|d| d.rel_path).collect();

        for (idx, pre_tok) in pre_tokenized.iter().enumerate() {
            let embedding = embedder.embed(pre_tok);
            scorer.upsert(&idx, embedding);
        }

        let query_stemmer = Stemmer::create(query_language.to_algorithm());

        Self {
            doc_ids,
            doc_token_sets,
            scorer,
            embedder,
            query_stemmer,
        }
    }

    /// Returns the top `limit` matches for `query`, ranked by BM25 score (highest first).
    ///
    /// Returns an empty vec when `query` produces no tokens or has no matches.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Bm25Match> {
        self.ranked_matches(query).into_iter().take(limit).collect()
    }

    /// Returns **all** matches for `query`, ranked by BM25 score (highest first).
    ///
    /// Returns an empty vec when `query` produces no tokens or has no matches.
    pub fn score(&self, query: &str) -> Vec<Bm25Match> {
        self.ranked_matches(query)
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Parse a query string into positive and negative terms.
    ///
    /// Terms prefixed with `-` are negation terms (docs containing them are excluded).
    /// All other terms contribute to BM25 relevance scoring.
    fn parse_query(&self, query: &str) -> ParsedQuery {
        let mut positive = Vec::new();
        let mut negative = HashSet::new();

        for word in query.split_whitespace() {
            if let Some(neg) = word.strip_prefix('-') {
                if !neg.is_empty() {
                    let stemmed = tokenize(neg, &self.query_stemmer);
                    for t in stemmed {
                        negative.insert(t);
                    }
                }
            } else {
                let stemmed = tokenize(word, &self.query_stemmer);
                positive.extend(stemmed);
            }
        }

        ParsedQuery {
            positive: positive.join(" "),
            negative,
        }
    }

    fn ranked_matches(&self, query: &str) -> Vec<Bm25Match> {
        let parsed = self.parse_query(query);
        if parsed.positive.is_empty() {
            return Vec::new();
        }
        let query_embedding = self.embedder.embed(&parsed.positive);

        self.scorer
            .matches(&query_embedding)
            .into_iter()
            .filter_map(|scored| {
                let rel_path = self.doc_ids.get(scored.id)?;
                // Apply negation filter: skip docs containing any excluded term.
                if !parsed.negative.is_empty()
                    && let Some(token_set) = self.doc_token_sets.get(scored.id)
                    && parsed.negative.iter().any(|neg| token_set.contains(neg))
                {
                    return None;
                }
                Some(Bm25Match {
                    rel_path: rel_path.clone(),
                    score: f64::from(scored.score),
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stemmer(lang: StemLanguage) -> Stemmer {
        Stemmer::create(lang.to_algorithm())
    }

    // ------------------------------------------------------------------
    // tokenize
    // ------------------------------------------------------------------

    #[test]
    fn test_tokenize_english() {
        let stemmer = make_stemmer(StemLanguage::English);
        let tokens = tokenize("running quickly", &stemmer);
        assert_eq!(tokens, vec!["run", "quick"]);
    }

    #[test]
    fn test_tokenize_french() {
        let stemmer = make_stemmer(StemLanguage::French);
        let tokens = tokenize("mangeons rapidement", &stemmer);
        // Just verify we get two non-empty tokens; exact stems depend on the snowball algorithm.
        assert_eq!(tokens.len(), 2);
        assert!(tokens.iter().all(|t| !t.is_empty()));
    }

    #[test]
    fn test_tokenize_splits_on_punctuation() {
        let stemmer = make_stemmer(StemLanguage::English);
        // "hello-world foo_bar" should yield 4 tokens: hello, world, foo, bar
        let tokens = tokenize("hello-world foo_bar", &stemmer);
        assert_eq!(tokens.len(), 4);
    }

    #[test]
    fn test_tokenize_unicode_lowercase() {
        let stemmer = make_stemmer(StemLanguage::English);
        // "ÜBER" lowercases to "über"; the English stemmer leaves it as-is since it's not an
        // ASCII word, but the key assertion is that it is lowercased.
        let tokens = tokenize("ÜBER", &stemmer);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0], tokens[0].to_lowercase());
    }

    // ------------------------------------------------------------------
    // resolve_language / parse_language
    // ------------------------------------------------------------------

    #[test]
    fn test_resolve_language_precedence() {
        // frontmatter wins over cli and config
        assert_eq!(
            resolve_language(Some("french"), Some("german"), Some("spanish")),
            StemLanguage::French
        );
        // cli wins when frontmatter absent
        assert_eq!(
            resolve_language(None, Some("german"), Some("spanish")),
            StemLanguage::German
        );
        // config wins when frontmatter and cli absent
        assert_eq!(
            resolve_language(None, None, Some("spanish")),
            StemLanguage::Spanish
        );
        // falls back to English when all absent
        assert_eq!(resolve_language(None, None, None), StemLanguage::English);
        // invalid frontmatter value falls through to cli
        assert_eq!(
            resolve_language(Some("klingon"), Some("italian"), None),
            StemLanguage::Italian
        );
    }

    #[test]
    fn test_parse_language_valid() {
        let cases = [
            ("arabic", StemLanguage::Arabic),
            ("Danish", StemLanguage::Danish),
            ("DUTCH", StemLanguage::Dutch),
            ("English", StemLanguage::English),
            ("finnish", StemLanguage::Finnish),
            ("French", StemLanguage::French),
            ("german", StemLanguage::German),
            ("greek", StemLanguage::Greek),
            ("Hungarian", StemLanguage::Hungarian),
            ("Italian", StemLanguage::Italian),
            ("norwegian", StemLanguage::Norwegian),
            ("portuguese", StemLanguage::Portuguese),
            ("romanian", StemLanguage::Romanian),
            ("russian", StemLanguage::Russian),
            ("spanish", StemLanguage::Spanish),
            ("Swedish", StemLanguage::Swedish),
            ("Tamil", StemLanguage::Tamil),
            ("Turkish", StemLanguage::Turkish),
        ];
        for (input, expected) in cases {
            assert_eq!(parse_language(input).expect(input), expected, "{input}");
        }
    }

    #[test]
    fn test_parse_language_invalid() {
        assert!(parse_language("klingon").is_err());
        assert!(parse_language("").is_err());
        assert!(parse_language("en").is_err());
    }

    // ------------------------------------------------------------------
    // Bm25Corpus
    // ------------------------------------------------------------------

    fn doc(rel_path: &str, title: &str, body: &str) -> DocumentInput {
        DocumentInput {
            rel_path: rel_path.to_owned(),
            title: title.to_owned(),
            body: body.to_owned(),
            language: StemLanguage::English,
        }
    }

    #[test]
    fn test_bm25_corpus_basic_search() {
        let docs = vec![
            doc(
                "rust.md",
                "Rust programming",
                "Rust is a systems programming language.",
            ),
            doc(
                "python.md",
                "Python programming",
                "Python is a scripting language.",
            ),
            doc(
                "cooking.md",
                "Cooking recipes",
                "How to bake a delicious cake.",
            ),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("rust programming", 10);
        assert!(!results.is_empty(), "expected at least one result");
        // The Rust document should rank first.
        assert_eq!(results[0].rel_path, "rust.md");
    }

    #[test]
    fn test_bm25_corpus_stemming_matches() {
        // "run" should match a doc that contains "running" because stemming normalises both.
        let docs = vec![
            doc(
                "running.md",
                "Running guide",
                "I enjoy running every morning.",
            ),
            doc(
                "cooking.md",
                "Cooking guide",
                "I enjoy cooking every evening.",
            ),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("run", 10);
        assert!(!results.is_empty(), "expected matches via stemming");
        assert_eq!(results[0].rel_path, "running.md");
    }

    #[test]
    fn test_bm25_corpus_relevance_ranking() {
        // The doc with more occurrences of the query term should rank higher.
        let docs = vec![
            doc(
                "many.md",
                "Rust tips",
                "Rust Rust Rust Rust Rust is great for systems programming.",
            ),
            doc(
                "few.md",
                "Languages",
                "Rust is one option among many languages.",
            ),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("rust", 10);
        assert!(results.len() >= 2);
        assert_eq!(results[0].rel_path, "many.md");
    }

    #[test]
    fn test_bm25_corpus_empty_query() {
        let docs = vec![doc("a.md", "Title", "Some body text.")];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_corpus_no_matches() {
        let docs = vec![doc("a.md", "Title", "Some body text.")];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("xyzzy42quux", 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_corpus_single_doc() {
        let docs = vec![doc(
            "single.md",
            "Only document",
            "This is the only document.",
        )];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.search("document", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "single.md");
    }

    #[test]
    fn test_bm25_corpus_score_returns_all() {
        let docs = vec![
            doc("a.md", "Alpha", "The quick brown fox."),
            doc("b.md", "Beta", "The lazy dog slept."),
            doc("c.md", "Gamma", "No matching content here."),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        // "quick" matches only doc a, so score() should return exactly 1 result.
        let all = corpus.score("quick");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].rel_path, "a.md");
    }

    // ------------------------------------------------------------------
    // Negation queries
    // ------------------------------------------------------------------

    #[test]
    fn test_bm25_negation_excludes_matching_docs() {
        let docs = vec![
            doc("rust.md", "Rust", "Rust is a systems programming language."),
            doc(
                "python.md",
                "Python",
                "Python is a scripting programming language.",
            ),
            doc("go.md", "Go", "Go is a compiled programming language."),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        // "programming -python" should return rust.md and go.md but NOT python.md
        let results = corpus.score("programming -python");
        let paths: Vec<&str> = results.iter().map(|r| r.rel_path.as_str()).collect();
        assert!(
            !paths.contains(&"python.md"),
            "python.md should be excluded: {paths:?}"
        );
        assert!(
            paths.contains(&"rust.md"),
            "rust.md should remain: {paths:?}"
        );
        assert!(paths.contains(&"go.md"), "go.md should remain: {paths:?}");
    }

    #[test]
    fn test_bm25_negation_only_returns_empty() {
        let docs = vec![doc("a.md", "Title", "Some body text.")];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        // Only negation terms → no positive terms → empty results
        let results = corpus.score("-text");
        assert!(
            results.is_empty(),
            "negation-only query should return empty"
        );
    }

    #[test]
    fn test_bm25_negation_with_stemming() {
        // "-running" should also exclude docs containing "run" (after stemming both)
        let docs = vec![
            doc("a.md", "Running", "I love running every day."),
            doc("b.md", "Swimming", "I love swimming every day."),
        ];
        let corpus = Bm25Corpus::build(docs, StemLanguage::English);
        let results = corpus.score("love -running");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "b.md");
    }

    #[test]
    fn test_bm25_build_from_tokens() {
        let docs = vec![
            PreTokenizedInput {
                rel_path: "a.md".to_owned(),
                tokens: vec!["rust".to_owned(), "program".to_owned()],
            },
            PreTokenizedInput {
                rel_path: "b.md".to_owned(),
                tokens: vec!["python".to_owned(), "program".to_owned()],
            },
        ];
        let corpus = Bm25Corpus::build_from_tokens(docs, StemLanguage::English);
        let results = corpus.score("rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "a.md");
    }
}
