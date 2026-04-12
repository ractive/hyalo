//! BM25 ranked search with multi-language stemming support.
//!
//! This module provides:
//! - [`StemLanguage`]: enum of supported stemming languages with [`parse_language`]
//! - [`resolve_language`]: language precedence logic (frontmatter > CLI > config > English)
//! - [`tokenize`]: Unicode-aware tokenization + stemming pipeline
//! - [`Bm25InvertedIndex`]: serializable in-memory BM25 index built from [`DocumentInput`] values

use std::collections::{HashMap, HashSet};

use rust_stemmers::{Algorithm, Stemmer};
use serde::{Deserialize, Serialize};

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

/// Create a [`Stemmer`] for the given language.
///
/// This is the public API for creating stemmers — `StemLanguage::to_algorithm` is
/// `pub(crate)` and not available outside `hyalo-core`.
pub fn create_stemmer(lang: StemLanguage) -> Stemmer {
    Stemmer::create(lang.to_algorithm())
}

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
// Corpus types
// ---------------------------------------------------------------------------

/// Input for building a corpus from pre-tokenized data (e.g. stored in the snapshot index).
pub struct PreTokenizedInput {
    /// Relative path that uniquely identifies the document.
    pub rel_path: String,
    /// Already-stemmed tokens for this document (title + body, combined).
    pub tokens: Vec<String>,
}

/// Input data for a single document added to a [`Bm25InvertedIndex`].
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
/// Applies the same tokenization pipeline as [`Bm25InvertedIndex::build`]: Unicode-aware
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

/// A ranked search result returned by [`Bm25InvertedIndex::search`] and
/// [`Bm25InvertedIndex::score`].
pub struct Bm25Match {
    /// Relative path of the matched document.
    pub rel_path: String,
    /// BM25 relevance score (higher is more relevant).
    pub score: f64,
}

/// Parsed query with positive search terms and negative exclusion terms.
struct ParsedQuery {
    /// Positive stemmed terms to search for.
    positive: Vec<String>,
    /// Negative terms to exclude (stemmed). A document containing any of these is filtered out.
    negative: HashSet<String>,
}

// ---------------------------------------------------------------------------
// Query parsing
// ---------------------------------------------------------------------------

/// Parse a query string into positive and negative terms.
///
/// Terms prefixed with `-` are negation terms (docs containing them are excluded).
/// All other terms contribute to BM25 relevance scoring.
fn parse_query(query: &str, stemmer: &Stemmer) -> ParsedQuery {
    let mut positive = Vec::new();
    let mut negative = HashSet::new();

    for word in query.split_whitespace() {
        if let Some(neg) = word.strip_prefix('-') {
            if !neg.is_empty() {
                let tokens = tokenize(neg, stemmer);
                for t in tokens {
                    negative.insert(t);
                }
            }
        } else {
            let tokens = tokenize(word, stemmer);
            positive.extend(tokens);
        }
    }

    ParsedQuery { positive, negative }
}

// ---------------------------------------------------------------------------
// BM25 inverted index
// ---------------------------------------------------------------------------

/// A (doc_id, term_frequency) pair stored in the posting list.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Posting {
    doc_id: u32,
    term_freq: u32,
}

/// Serializable BM25 inverted index built from a collection of documents.
///
/// Build once with [`Bm25InvertedIndex::build`] or [`Bm25InvertedIndex::build_from_tokens`],
/// then call [`Bm25InvertedIndex::search`] or [`Bm25InvertedIndex::score`] as many times as
/// needed. Because this type is `Serialize + Deserialize`, it can be persisted in the snapshot
/// index and reused across invocations — avoiding the O(N·doc) corpus rebuild on every query.
///
/// ## BM25 scoring formula
///
/// ```text
/// score(q, d) = Σ IDF(t) × (tf(t,d) × (k1 + 1)) / (tf(t,d) + k1 × (1 - b + b × |d|/avgdl))
///
/// where:
///   IDF(t) = ln(1 + (N - n(t) + 0.5) / (n(t) + 0.5))
///   k1 = 1.2, b = 0.75
/// ```
///
/// ## Query syntax
///
/// - `term1 term2` — all terms contribute to BM25 relevance score; docs matching
///   more terms rank higher (implicit relevance OR with additive scoring)
/// - `-term` — exclude documents containing this term (negation)
///
/// Examples: `"rust programming"`, `"rust -javascript"`, `"search -draft"`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25InvertedIndex {
    /// Term → list of (doc_id, term_frequency) postings, sorted by doc_id.
    postings: HashMap<String, Vec<Posting>>,
    /// Number of tokens in each document (indexed by doc_id).
    doc_lengths: Vec<u32>,
    /// doc_id → relative path mapping.
    doc_paths: Vec<String>,
    /// Per-document token sets for negation query support.
    doc_token_sets: Vec<HashSet<String>>,
    /// Average document length (pre-computed).
    avgdl: f32,
}

impl Bm25InvertedIndex {
    // BM25 tuning constants
    const K1: f64 = 1.2;
    const B: f64 = 0.75;

    /// Builds a BM25 index from `docs`.
    ///
    /// Each document is tokenized with its own [`StemLanguage`]. No stemmer is
    /// stored in the index — pass a [`Stemmer`] to [`search`](Self::search) or
    /// [`score`](Self::score) at query time.
    pub fn build(docs: Vec<DocumentInput>) -> Self {
        let pre_tokenized: Vec<PreTokenizedInput> = docs
            .into_iter()
            .map(|doc| {
                let stemmer = Stemmer::create(doc.language.to_algorithm());
                let combined = format!("{} {}", doc.title, doc.body);
                let tokens = tokenize(&combined, &stemmer);
                PreTokenizedInput {
                    rel_path: doc.rel_path,
                    tokens,
                }
            })
            .collect();
        Self::build_from_tokens(pre_tokenized)
    }

    /// Builds a BM25 index from pre-tokenized documents (e.g. stored in the snapshot index).
    ///
    /// Each document's tokens are already stemmed — no further tokenization is applied.
    pub fn build_from_tokens(docs: Vec<PreTokenizedInput>) -> Self {
        let n = docs.len();
        let mut postings: HashMap<String, Vec<Posting>> = HashMap::new();
        let mut doc_lengths: Vec<u32> = Vec::with_capacity(n);
        let mut doc_paths: Vec<String> = Vec::with_capacity(n);
        let mut doc_token_sets: Vec<HashSet<String>> = Vec::with_capacity(n);

        for (doc_id, doc) in docs.into_iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let doc_id = doc_id as u32;
            let token_count = doc.tokens.len();

            // Build per-document term-frequency map.
            let mut tf: HashMap<&str, u32> = HashMap::new();
            for token in &doc.tokens {
                *tf.entry(token.as_str()).or_insert(0) += 1;
            }

            // Insert into postings lists.
            for (term, freq) in tf {
                postings.entry(term.to_owned()).or_default().push(Posting {
                    doc_id,
                    term_freq: freq,
                });
            }

            let token_set: HashSet<String> = doc.tokens.into_iter().collect();
            #[allow(clippy::cast_possible_truncation)]
            doc_lengths.push(token_count as u32);
            doc_paths.push(doc.rel_path);
            doc_token_sets.push(token_set);
        }

        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let avgdl: f32 = if n == 0 {
            256.0
        } else {
            let total: u64 = doc_lengths.iter().map(|&l| u64::from(l)).sum();
            // avgdl precision loss is acceptable — this is a BM25 normalisation factor.
            (total as f64 / n as f64) as f32
        };

        Self {
            postings,
            doc_lengths,
            doc_paths,
            doc_token_sets,
            avgdl,
        }
    }

    /// Build a `Bm25InvertedIndex` from `IndexEntry` values stored in a snapshot.
    ///
    /// Returns `None` if no entries have `bm25_tokens` set (i.e. the index was built
    /// without `bm25_tokenize = true`).
    pub fn build_from_entries(entries: &[crate::index::IndexEntry]) -> Option<Self> {
        let docs: Vec<PreTokenizedInput> = entries
            .iter()
            .filter_map(|e| {
                e.bm25_tokens.as_ref().map(|tokens| PreTokenizedInput {
                    rel_path: e.rel_path.clone(),
                    tokens: tokens.clone(),
                })
            })
            .collect();

        if docs.is_empty() {
            return None;
        }
        Some(Self::build_from_tokens(docs))
    }

    /// Returns the top `limit` matches for `query`, ranked by BM25 score (highest first).
    ///
    /// Returns an empty vec when `query` produces no tokens or has no matches.
    pub fn search(&self, query: &str, stemmer: &Stemmer, limit: usize) -> Vec<Bm25Match> {
        self.ranked_matches(query, stemmer)
            .into_iter()
            .take(limit)
            .collect()
    }

    /// Returns **all** matches for `query`, ranked by BM25 score (highest first).
    ///
    /// Returns an empty vec when `query` produces no tokens or has no matches.
    pub fn score(&self, query: &str, stemmer: &Stemmer) -> Vec<Bm25Match> {
        self.ranked_matches(query, stemmer)
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Compute BM25 scores for all documents matching any positive term.
    fn ranked_matches(&self, query: &str, stemmer: &Stemmer) -> Vec<Bm25Match> {
        let parsed = parse_query(query, stemmer);
        if parsed.positive.is_empty() {
            return Vec::new();
        }

        #[allow(clippy::cast_precision_loss)]
        let n = self.doc_paths.len() as f64;
        let avgdl = f64::from(self.avgdl);
        let mut scores: Vec<f64> = vec![0.0; self.doc_paths.len()];

        for term in &parsed.positive {
            let Some(posting_list) = self.postings.get(term) else {
                continue;
            };

            // IDF = ln(1 + (N - n(t) + 0.5) / (n(t) + 0.5))
            #[allow(clippy::cast_precision_loss)]
            let nt = posting_list.len() as f64;
            let idf = (1.0 + (n - nt + 0.5) / (nt + 0.5)).ln();

            for p in posting_list {
                let doc_id = p.doc_id as usize;
                let tf = f64::from(p.term_freq);
                let dl = f64::from(self.doc_lengths[doc_id]);
                // BM25 term weight
                let tf_norm = (tf * (Self::K1 + 1.0))
                    / (tf + Self::K1 * (1.0 - Self::B + Self::B * dl / avgdl));
                scores[doc_id] += idf * tf_norm;
            }
        }

        // Collect non-zero scores, apply negation filter, sort descending.
        let mut matches: Vec<Bm25Match> = scores
            .into_iter()
            .enumerate()
            .filter_map(|(doc_id, score)| {
                if score <= 0.0 {
                    return None;
                }
                // Negation filter: skip docs that contain any excluded term.
                if !parsed.negative.is_empty()
                    && parsed
                        .negative
                        .iter()
                        .any(|neg| self.doc_token_sets[doc_id].contains(neg))
                {
                    return None;
                }
                Some(Bm25Match {
                    rel_path: self.doc_paths[doc_id].clone(),
                    score,
                })
            })
            .collect();

        matches.sort_unstable_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        matches
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
    // Bm25InvertedIndex
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("rust programming", &stemmer, 10);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("run", &stemmer, 10);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("rust", &stemmer, 10);
        assert!(results.len() >= 2);
        assert_eq!(results[0].rel_path, "many.md");
    }

    #[test]
    fn test_bm25_corpus_empty_query() {
        let docs = vec![doc("a.md", "Title", "Some body text.")];
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("", &stemmer, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_corpus_no_matches() {
        let docs = vec![doc("a.md", "Title", "Some body text.")];
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("xyzzy42quux", &stemmer, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_corpus_single_doc() {
        let docs = vec![doc(
            "single.md",
            "Only document",
            "This is the only document.",
        )];
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.search("document", &stemmer, 10);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        // "quick" matches only doc a, so score() should return exactly 1 result.
        let all = index.score("quick", &stemmer);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        // "programming -python" should return rust.md and go.md but NOT python.md
        let results = index.score("programming -python", &stemmer);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        // Only negation terms → no positive terms → empty results
        let results = index.score("-text", &stemmer);
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
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.score("love -running", &stemmer);
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
        let index = Bm25InvertedIndex::build_from_tokens(docs);
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.score("rust", &stemmer);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "a.md");
    }

    #[test]
    fn test_bm25_serde_round_trip() {
        let docs = vec![
            doc("rust.md", "Rust", "Rust is a systems programming language."),
            doc("python.md", "Python", "Python is a scripting language."),
        ];
        let index = Bm25InvertedIndex::build(docs);
        let stemmer = make_stemmer(StemLanguage::English);

        // Serialize to MessagePack and back.
        let bytes = rmp_serde::to_vec_named(&index).expect("serialize");
        let restored: Bm25InvertedIndex = rmp_serde::from_slice(&bytes).expect("deserialize");

        // Verify search results are identical after round-trip.
        let before = index.score("rust", &stemmer);
        let after = restored.score("rust", &stemmer);
        assert_eq!(before.len(), after.len());
        assert_eq!(before[0].rel_path, after[0].rel_path);
        assert!((before[0].score - after[0].score).abs() < f64::EPSILON);
    }

    #[test]
    fn test_bm25_build_from_entries() {
        use crate::index::IndexEntry;
        use indexmap::IndexMap;

        let entries = vec![
            IndexEntry {
                rel_path: "a.md".to_owned(),
                modified: String::new(),
                properties: IndexMap::new(),
                tags: Vec::new(),
                sections: Vec::new(),
                tasks: Vec::new(),
                links: Vec::new(),
                bm25_tokens: Some(vec!["rust".to_owned(), "program".to_owned()]),
                bm25_language: Some("english".to_owned()),
            },
            IndexEntry {
                rel_path: "b.md".to_owned(),
                modified: String::new(),
                properties: IndexMap::new(),
                tags: Vec::new(),
                sections: Vec::new(),
                tasks: Vec::new(),
                links: Vec::new(),
                bm25_tokens: None, // No tokens — should be skipped
                bm25_language: None,
            },
        ];

        let index = Bm25InvertedIndex::build_from_entries(&entries);
        assert!(index.is_some(), "should build from entries with tokens");
        let index = index.unwrap();
        let stemmer = make_stemmer(StemLanguage::English);
        let results = index.score("rust", &stemmer);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rel_path, "a.md");
    }

    #[test]
    fn test_bm25_build_from_entries_none_when_no_tokens() {
        use crate::index::IndexEntry;
        use indexmap::IndexMap;

        let entries = vec![IndexEntry {
            rel_path: "a.md".to_owned(),
            modified: String::new(),
            properties: IndexMap::new(),
            tags: Vec::new(),
            sections: Vec::new(),
            tasks: Vec::new(),
            links: Vec::new(),
            bm25_tokens: None,
            bm25_language: None,
        }];

        assert!(
            Bm25InvertedIndex::build_from_entries(&entries).is_none(),
            "no tokens → should return None"
        );
    }
}
