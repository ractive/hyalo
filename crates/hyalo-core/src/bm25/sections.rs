//! Section-granular BM25 scoring (iteration 303, DEC-334).
//!
//! Scores and snippets a query not against a whole document but against each
//! of its outline sections independently: a flat, non-overlapping partition
//! of the body at every heading boundary (plus a synthesized pre-heading
//! preamble). Shares the compiled query AST, prefix expansion and IDF
//! formula with file-level scoring (`query.rs`) so a section hit and a file
//! hit never disagree about what a query means — only about what text it is
//! evaluated against.
//!
//! A query's `Not` and `Field` nodes are file-level predicates already
//! applied by the caller (a section is only ever scored inside a file that
//! already matched them), so [`SectionScorer::collect`] prunes them away
//! before touching any file: see [`prune`].

use std::collections::{HashMap, VecDeque};

use rust_stemmers::Stemmer;

use super::query;
use super::{Bm25InvertedIndex, CompiledQuery, StemLanguage, create_stemmer, tokenize};
use crate::heading::SectionRange;
use crate::types::{ContentMatch, OutlineSection};

// BM25 tuning constants — identical to `Bm25InvertedIndex::{K1,B}` (query.rs).
const K1: f64 = 1.2;
const B: f64 = 0.75;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Where a section sits in its file.
///
/// Lines are 1-based and file-absolute, the same numbering as
/// [`OutlineSection::line`].
#[derive(Debug, Clone, PartialEq)]
pub struct SectionSpan {
    /// Heading text, or `None` for the pre-heading preamble.
    pub heading: Option<String>,
    /// ATX heading level (one through six), `0` for the preamble.
    pub level: u8,
    /// The heading line itself (preamble: the first body line).
    pub line_start: usize,
    /// Inclusive: the line before the next heading of any level, or the
    /// file's last line.
    pub line_end: usize,
    /// Heading path from the outline, ending with this heading
    /// (e.g. `["H1", "H2"]`); empty for the preamble.
    pub path: Vec<String>,
}

/// One section scored above zero by [`SectionScorer::finish`].
#[derive(Debug, Clone)]
pub struct SectionHit {
    /// Where this section sits in its file.
    pub span: SectionSpan,
    /// BM25 score restricted to this section's text.
    pub score: f64,
    /// Best (at most three) snippet lines inside this section.
    pub matches: Vec<ContentMatch>,
}

/// One file's eligible sections with the per-section statistics scoring needs.
///
/// Built by [`SectionScorer::collect`]; consumed by [`SectionScorer::finish`].
pub struct FileSections {
    sections: Vec<SectionStat>,
    body_offset: usize,
}

impl FileSections {
    fn empty(body_offset: usize) -> Self {
        Self {
            sections: Vec::new(),
            body_offset,
        }
    }

    /// Number of frontmatter lines before the body (0 without frontmatter).
    ///
    /// `body-relative line = file line - body_offset`.
    #[must_use]
    pub fn body_offset(&self) -> usize {
        self.body_offset
    }
}

/// Per-section statistics kept during [`SectionScorer::collect`].
///
/// `term_tf` and `alt_hit` are indexed by [`SectionScorer::term_index`] and
/// the flattened phrase-alternative list respectively, so their size is
/// bounded by the *query*, not the document — memory stays
/// O(sections × query terms) regardless of document length.
struct SectionStat {
    span: SectionSpan,
    token_count: usize,
    term_tf: Vec<u32>,
    alt_hit: Vec<bool>,
    matches: Vec<ContentMatch>,
}

// ---------------------------------------------------------------------------
// Pruned query tree
// ---------------------------------------------------------------------------

/// The compiled query with `Not` and `Field` nodes dropped, `Prefix` nodes
/// expanded against the corpus, and every term interned to a small integer.
#[derive(Debug, Clone)]
enum PrunedNode {
    And(Vec<PrunedNode>),
    Or(Vec<PrunedNode>),
    /// Any-of these term ids (a plain term, or an expanded prefix).
    Terms(Vec<usize>),
    /// Index into [`SectionScorer::phrase_alt_groups`].
    Phrase(usize),
}

fn intern(term_index: &mut HashMap<String, usize>, term: &str) -> usize {
    if let Some(&id) = term_index.get(term) {
        return id;
    }
    let id = term_index.len();
    term_index.insert(term.to_owned(), id);
    id
}

fn collapse(nodes: Vec<PrunedNode>, make: fn(Vec<PrunedNode>) -> PrunedNode) -> Option<PrunedNode> {
    match nodes.len() {
        0 => None,
        1 => nodes.into_iter().next(),
        _ => Some(make(nodes)),
    }
}

/// Drop `Not`/`Field` nodes, expand `Prefix` nodes via `corpus`, collapse
/// empty `And`/`Or` groups, and intern every surviving term.
///
/// A `Prefix` leaf that expands to no dictionary term is kept as an
/// always-false `Terms([])` leaf, not dropped — dropping it would silently
/// satisfy an `And` branch the query never intended to relax.
fn prune(
    node: &query::Node,
    corpus: &Bm25InvertedIndex,
    term_index: &mut HashMap<String, usize>,
    alt_specs: &mut Vec<Vec<String>>,
    alt_term_ids: &mut Vec<Vec<usize>>,
    phrase_alt_groups: &mut Vec<Vec<usize>>,
) -> Option<PrunedNode> {
    match node {
        query::Node::And(children) => {
            let out: Vec<PrunedNode> = children
                .iter()
                .filter_map(|c| {
                    prune(
                        c,
                        corpus,
                        term_index,
                        alt_specs,
                        alt_term_ids,
                        phrase_alt_groups,
                    )
                })
                .collect();
            collapse(out, PrunedNode::And)
        }
        query::Node::Or(children) => {
            let out: Vec<PrunedNode> = children
                .iter()
                .filter_map(|c| {
                    prune(
                        c,
                        corpus,
                        term_index,
                        alt_specs,
                        alt_term_ids,
                        phrase_alt_groups,
                    )
                })
                .collect();
            collapse(out, PrunedNode::Or)
        }
        query::Node::Not(_) | query::Node::Field(_) => None,
        query::Node::Term(stems) => {
            let ids = stems.iter().map(|s| intern(term_index, s)).collect();
            Some(PrunedNode::Terms(ids))
        }
        query::Node::Prefix(candidates) => {
            let expanded = corpus.expand_prefix(candidates);
            let ids = expanded.iter().map(|t| intern(term_index, t)).collect();
            Some(PrunedNode::Terms(ids))
        }
        query::Node::Phrase(alternatives) => {
            let leaf_idx = phrase_alt_groups.len();
            let mut group = Vec::with_capacity(alternatives.len());
            for seq in alternatives {
                let ids: Vec<usize> = seq.iter().map(|t| intern(term_index, t)).collect();
                let alt_idx = alt_specs.len();
                alt_specs.push(seq.clone());
                alt_term_ids.push(ids);
                group.push(alt_idx);
            }
            phrase_alt_groups.push(group);
            Some(PrunedNode::Phrase(leaf_idx))
        }
    }
}

fn flatten_leaves(node: &PrunedNode, out: &mut Vec<PrunedNode>) {
    match node {
        PrunedNode::And(children) | PrunedNode::Or(children) => {
            for n in children {
                flatten_leaves(n, out);
            }
        }
        PrunedNode::Terms(ids) => out.push(PrunedNode::Terms(ids.clone())),
        PrunedNode::Phrase(idx) => out.push(PrunedNode::Phrase(*idx)),
    }
}

fn tfnorm(tf: f64, dl: f64, avgdl: f64) -> f64 {
    (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * dl / avgdl.max(f64::MIN_POSITIVE)))
}

/// `true` when `window`'s trailing `seq.len()` tokens equal `seq`.
fn window_ends_with(window: &VecDeque<String>, seq: &[String]) -> bool {
    let len = seq.len();
    if len == 0 || window.len() < len {
        return false;
    }
    window.iter().skip(window.len() - len).eq(seq.iter())
}

// ---------------------------------------------------------------------------
// SectionScorer
// ---------------------------------------------------------------------------

/// Query resolved against one corpus for per-section scoring.
///
/// Built by [`Bm25InvertedIndex::section_scorer`]; reused across every file
/// of the corpus so prefix expansion and IDF are computed once.
pub struct SectionScorer {
    root: Option<PrunedNode>,
    /// Flattened `Terms`/`Phrase` leaves of `root`, duplicates preserved —
    /// scoring sums every leaf independently, the same way file-level
    /// scoring sums every [`query::CompiledQuery`] unit.
    leaves: Vec<PrunedNode>,
    term_index: HashMap<String, usize>,
    idf_by_index: Vec<f64>,
    /// Per phrase-alternative token sequence, used to detect occurrences
    /// while streaming (the sliding window compares against these).
    alt_specs: Vec<Vec<String>>,
    /// Per phrase-alternative term ids, parallel to `alt_specs`.
    alt_term_ids: Vec<Vec<usize>>,
    /// Per `Phrase` leaf (indexed by the `usize` a [`PrunedNode::Phrase`]
    /// carries), the alternatives' indices into `alt_specs`/`alt_term_ids`.
    phrase_alt_groups: Vec<Vec<usize>>,
    max_phrase_len: usize,
    matcher: query::SnippetMatcher,
}

impl Bm25InvertedIndex {
    /// Prefix expansion (capped exactly like file-level scoring, via
    /// `expand_prefix`) and IDF (`N` = this corpus's doc count, `n(t)` = this
    /// corpus's document frequency) come from this corpus.
    #[must_use]
    pub fn section_scorer(&self, query: &CompiledQuery) -> SectionScorer {
        let mut term_index: HashMap<String, usize> = HashMap::new();
        let mut alt_specs: Vec<Vec<String>> = Vec::new();
        let mut alt_term_ids: Vec<Vec<usize>> = Vec::new();
        let mut phrase_alt_groups: Vec<Vec<usize>> = Vec::new();

        let root = query.root.as_ref().and_then(|r| {
            prune(
                r,
                self,
                &mut term_index,
                &mut alt_specs,
                &mut alt_term_ids,
                &mut phrase_alt_groups,
            )
        });

        let mut leaves = Vec::new();
        if let Some(r) = &root {
            flatten_leaves(r, &mut leaves);
        }

        #[allow(clippy::cast_precision_loss)]
        let n = self.doc_count() as f64;
        let mut idf_by_index = vec![0.0_f64; term_index.len()];
        for (term, &id) in &term_index {
            let nt = self.postings.get(term).map_or(0, Vec::len);
            #[allow(clippy::cast_precision_loss)]
            let nt_f = nt as f64;
            idf_by_index[id] = (1.0 + (n - nt_f + 0.5) / (nt_f + 0.5)).ln();
        }

        let max_phrase_len = alt_specs.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let matcher = query::SnippetMatcher::from_compiled(query);

        SectionScorer {
            root,
            leaves,
            term_index,
            idf_by_index,
            alt_specs,
            alt_term_ids,
            phrase_alt_groups,
            max_phrase_len,
            matcher,
        }
    }
}

/// Per-section meta (level, heading, heading path) alongside each boundary
/// in `starts`, built once from the file's outline.
struct SectionMeta {
    level: u8,
    heading: Option<String>,
    path: Vec<String>,
}

impl SectionMeta {
    fn label(&self) -> String {
        self.heading.as_ref().map_or_else(String::new, |h| {
            format!("{} {h}", "#".repeat(usize::from(self.level)))
        })
    }
}

impl SectionScorer {
    /// `false` when the query has no positive text leaf left after pruning.
    #[must_use]
    pub fn has_text_terms(&self) -> bool {
        self.root.is_some()
    }

    fn hit(&self, term_tf: &[u32], alt_hit: &[bool]) -> bool {
        fn walk(
            node: &PrunedNode,
            scorer: &SectionScorer,
            term_tf: &[u32],
            alt_hit: &[bool],
        ) -> bool {
            match node {
                PrunedNode::And(children) => {
                    children.iter().all(|n| walk(n, scorer, term_tf, alt_hit))
                }
                PrunedNode::Or(children) => {
                    children.iter().any(|n| walk(n, scorer, term_tf, alt_hit))
                }
                PrunedNode::Terms(ids) => ids.iter().any(|&id| term_tf[id] > 0),
                PrunedNode::Phrase(leaf_idx) => scorer.phrase_alt_groups[*leaf_idx]
                    .iter()
                    .any(|&alt_idx| alt_hit[alt_idx]),
            }
        }
        self.root
            .as_ref()
            .is_some_and(|r| walk(r, self, term_tf, alt_hit))
    }

    fn score_section(&self, stat: &SectionStat, avgdl: f64) -> f64 {
        #[allow(clippy::cast_precision_loss)]
        let dl = stat.token_count as f64;
        let mut score = 0.0;
        for leaf in &self.leaves {
            match leaf {
                PrunedNode::Terms(ids) => {
                    for &id in ids {
                        let tf = stat.term_tf[id];
                        if tf == 0 {
                            continue;
                        }
                        score += self.idf_by_index[id] * tfnorm(f64::from(tf), dl, avgdl);
                    }
                }
                PrunedNode::Phrase(leaf_idx) => {
                    for &alt_idx in &self.phrase_alt_groups[*leaf_idx] {
                        if !stat.alt_hit[alt_idx] {
                            continue;
                        }
                        for &term_id in &self.alt_term_ids[alt_idx] {
                            let tf = stat.term_tf[term_id];
                            if tf == 0 {
                                continue;
                            }
                            score += self.idf_by_index[term_id] * tfnorm(f64::from(tf), dl, avgdl);
                        }
                    }
                }
                PrunedNode::And(_) | PrunedNode::Or(_) => {
                    debug_assert!(false, "flatten_leaves must never emit And/Or");
                }
            }
        }
        score
    }

    /// Stream one file once. `outline` is the file's `OutlineSection` list
    /// (index entry sections). `scope`: `None` means every section eligible;
    /// `Some(ranges)` means only sections whose `line_start` lies in one of
    /// the ranges are eligible (the preamble is then never eligible, since
    /// every range starts at an actual heading line).
    ///
    /// # Errors
    /// Propagates I/O and vault-boundary errors from opening and reading
    /// `rel_path`. A file over the size cap or with an empty body yields no
    /// sections rather than an error.
    pub fn collect(
        &self,
        dir: &std::path::Path,
        rel_path: &str,
        language: StemLanguage,
        outline: &[OutlineSection],
        scope: Option<&[SectionRange]>,
    ) -> anyhow::Result<FileSections> {
        use crate::scanner::{MAX_BODY_LINE_BYTES, MAX_FILE_SIZE, read_line_capped};

        let path = super::resolve_document_path(dir, rel_path)?;
        let file = std::fs::File::open(&path)?;
        if file.metadata()?.len() > MAX_FILE_SIZE {
            return Ok(FileSections::empty(0));
        }
        let mut reader = std::io::BufReader::new(file);
        let framed = crate::frontmatter::read_frame_for_body(&mut reader, MAX_BODY_LINE_BYTES)?;
        if framed.bytes().is_empty() {
            return Ok(FileSections::empty(0));
        }
        let fm_lines = if framed.frame().frontmatter().is_some() {
            memchr::memchr_iter(b'\n', framed.bytes()).count()
                + usize::from(!framed.bytes().ends_with(b"\n"))
        } else {
            0
        };
        let body_first_line = if fm_lines == 0 { 1 } else { fm_lines + 1 };

        // Flat, non-overlapping section boundaries: index 0 is the
        // synthesized preamble, followed by one entry per outline heading.
        let headed: Vec<&OutlineSection> = outline.iter().filter(|s| s.heading.is_some()).collect();
        let mut starts: Vec<usize> = Vec::with_capacity(headed.len() + 1);
        starts.push(body_first_line);
        starts.extend(headed.iter().map(|s| s.line));

        let mut metas: Vec<SectionMeta> = Vec::with_capacity(starts.len());
        metas.push(SectionMeta {
            level: 0,
            heading: None,
            path: Vec::new(),
        });
        let mut stack: Vec<(u8, String)> = Vec::new();
        for h in &headed {
            let heading = h.heading.clone().unwrap_or_default();
            while stack.last().is_some_and(|(level, _)| *level >= h.level) {
                stack.pop();
            }
            stack.push((h.level, heading.clone()));
            let path = stack.iter().map(|(_, text)| text.clone()).collect();
            metas.push(SectionMeta {
                level: h.level,
                heading: Some(heading),
                path,
            });
        }

        let eligible: Vec<bool> = starts
            .iter()
            .map(|&line| scope.is_none_or(|ranges| crate::heading::in_scope(ranges, line)))
            .collect();

        let term_count = self.term_index.len();
        let alt_count = self.alt_specs.len();

        let mut collector = Collector {
            scorer: self,
            starts: &starts,
            metas: &metas,
            eligible: &eligible,
            cursor: 0,
            window: VecDeque::new(),
            tokens: 0,
            term_tf: vec![0; term_count],
            alt_hit: vec![false; alt_count],
            best: Vec::with_capacity(4),
            stats: Vec::new(),
        };

        let stemmer = create_stemmer(language);
        let mut last_line = body_first_line;

        if fm_lines == 0
            && framed.first_line_complete()
            && let Ok(first_line) = std::str::from_utf8(framed.bytes())
        {
            let trimmed = first_line.trim_end_matches(['\r', '\n']);
            collector.feed(trimmed, body_first_line, &stemmer);
            last_line = body_first_line;
        }

        let mut line = fm_lines.max(1);
        let mut buf = String::new();
        loop {
            buf.clear();
            let (n, outcome) = read_line_capped(&mut reader, &mut buf, MAX_BODY_LINE_BYTES)?;
            if n == 0 {
                break;
            }
            line += 1;
            if outcome == crate::scanner::LineOutcome::Complete {
                let trimmed = buf.trim_end_matches(['\r', '\n']);
                collector.feed(trimmed, line, &stemmer);
                last_line = line;
            }
        }

        collector.finish_last(last_line);

        Ok(FileSections {
            sections: collector.stats,
            body_offset: fm_lines,
        })
    }

    /// Score every collected section. `avgdl` is the mean token length over
    /// *all* collected sections of *all* files passed in (zero-token
    /// sections excluded from the mean). Returns only hits, sorted by score
    /// descending, then file ascending, then `line_start` ascending.
    // Takes `files` by value so spans and snippets move into the hits.
    #[must_use]
    pub fn finish(&self, files: Vec<(String, FileSections)>) -> Vec<(String, SectionHit)> {
        if self.root.is_none() {
            return Vec::new();
        }

        let mut total: u64 = 0;
        let mut count: u64 = 0;
        for (_, fs) in &files {
            for sec in &fs.sections {
                if sec.token_count > 0 {
                    total += sec.token_count as u64;
                    count += 1;
                }
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let avgdl = if count > 0 {
            total as f64 / count as f64
        } else {
            1.0
        };

        let mut out: Vec<(String, SectionHit)> = Vec::new();
        for (path, fs) in files {
            for sec in fs.sections {
                if !self.hit(&sec.term_tf, &sec.alt_hit) {
                    continue;
                }
                let score = self.score_section(&sec, avgdl);
                out.push((
                    path.clone(),
                    SectionHit {
                        span: sec.span,
                        score,
                        matches: sec.matches,
                    },
                ));
            }
        }

        out.sort_by(|a, b| {
            b.1.score
                .partial_cmp(&a.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.span.line_start.cmp(&b.1.span.line_start))
        });
        out
    }
}

/// Mutable streaming state for one [`Bm25InvertedIndex::collect`] call.
struct Collector<'a> {
    scorer: &'a SectionScorer,
    starts: &'a [usize],
    metas: &'a [SectionMeta],
    eligible: &'a [bool],
    cursor: usize,
    window: VecDeque<String>,
    tokens: usize,
    term_tf: Vec<u32>,
    alt_hit: Vec<bool>,
    best: Vec<(usize, ContentMatch)>,
    stats: Vec<SectionStat>,
}

impl Collector<'_> {
    /// Process one raw (already CRLF-trimmed) body line at file-absolute `line`.
    fn feed(&mut self, raw: &str, line: usize, stemmer: &Stemmer) {
        while self.cursor + 1 < self.starts.len() && line >= self.starts[self.cursor + 1] {
            let line_end = self.starts[self.cursor + 1] - 1;
            self.finalize(line_end);
            self.cursor += 1;
        }
        if !self.eligible[self.cursor] {
            return;
        }

        let tokens = tokenize(raw, stemmer);
        for tok in &tokens {
            self.tokens += 1;
            if let Some(&id) = self.scorer.term_index.get(tok.as_str()) {
                self.term_tf[id] += 1;
            }
            self.window.push_back(tok.clone());
            if self.window.len() > self.scorer.max_phrase_len {
                self.window.pop_front();
            }
            for (alt_idx, seq) in self.scorer.alt_specs.iter().enumerate() {
                if !self.alt_hit[alt_idx] && window_ends_with(&self.window, seq) {
                    self.alt_hit[alt_idx] = true;
                }
            }
        }

        let count = self.scorer.matcher.coverage(&tokens);
        if count == 0 || (self.best.len() == 3 && self.best[2].0 >= count) {
            return;
        }
        self.best.push((
            count,
            ContentMatch {
                line,
                section: self.metas[self.cursor].label(),
                text: raw.to_owned(),
            },
        ));
        self.best
            .sort_by_key(|(count, m)| (std::cmp::Reverse(*count), m.line));
        self.best.truncate(3);
    }

    /// Close out the section at `self.cursor` with inclusive end `line_end`,
    /// recording it when eligible, then reset per-section scratch state.
    fn finalize(&mut self, line_end: usize) {
        if self.eligible[self.cursor] {
            let meta = &self.metas[self.cursor];
            let span = SectionSpan {
                heading: meta.heading.clone(),
                level: meta.level,
                line_start: self.starts[self.cursor],
                line_end,
                path: meta.path.clone(),
            };
            // The synthesized preamble is emitted only when it holds at
            // least one token; a headed section is kept regardless.
            if self.cursor != 0 || self.tokens > 0 {
                self.stats.push(SectionStat {
                    span,
                    token_count: self.tokens,
                    term_tf: std::mem::replace(
                        &mut self.term_tf,
                        vec![0; self.scorer.term_index.len()],
                    ),
                    alt_hit: std::mem::replace(
                        &mut self.alt_hit,
                        vec![false; self.scorer.alt_specs.len()],
                    ),
                    matches: std::mem::take(&mut self.best)
                        .into_iter()
                        .map(|(_, m)| m)
                        .collect(),
                });
            }
        }
        self.tokens = 0;
        self.term_tf = vec![0; self.scorer.term_index.len()];
        self.alt_hit = vec![false; self.scorer.alt_specs.len()];
        self.best.clear();
        self.window.clear();
    }

    fn finish_last(&mut self, last_line: usize) {
        self.finalize(last_line);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bm25::DocumentInput;
    use tempfile::TempDir;

    fn write(dir: &TempDir, rel: &str, content: &str) {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    fn sec(level: u8, heading: &str, line: usize) -> OutlineSection {
        OutlineSection {
            level,
            heading: Some(heading.to_owned()),
            line,
            links: Vec::new(),
            tasks: None,
            code_blocks: Vec::new(),
        }
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

    fn compiled(q: &str) -> CompiledQuery {
        CompiledQuery::parse(q, &[StemLanguage::English]).unwrap()
    }

    #[test]
    fn flat_split_with_preamble_and_nested_path() {
        let tmp = TempDir::new().unwrap();
        let body = "Intro text here.\n\
# A\n\
Body of A.\n\
## B\n\
Body of B.\n\
# C\n\
Body of C.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "A", 2), sec(2, "B", 4), sec(1, "C", 6)];

        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("body");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);

        // All four sections ("intro", A, B, C) contain "body" except the
        // preamble (which does not mention "Body"), and B's path is nested.
        let b = hits
            .iter()
            .find(|(_, h)| h.span.heading.as_deref() == Some("B"))
            .unwrap();
        assert_eq!(b.1.span.path, vec!["A".to_owned(), "B".to_owned()]);
        assert_eq!(b.1.span.level, 2);
        assert_eq!(b.1.span.line_start, 4);
        assert_eq!(b.1.span.line_end, 5);

        let a = hits
            .iter()
            .find(|(_, h)| h.span.heading.as_deref() == Some("A"))
            .unwrap();
        assert_eq!(a.1.span.path, vec!["A".to_owned()]);
        assert_eq!(a.1.span.line_end, 3);

        let c = hits
            .iter()
            .find(|(_, h)| h.span.heading.as_deref() == Some("C"))
            .unwrap();
        assert_eq!(c.1.span.path, vec!["C".to_owned()]);
        assert_eq!(c.1.span.line_end, 7);

        // Preamble ("Intro text here.") never mentions "body" so it is
        // present in no hit; confirm no hit carries heading == None here.
        assert!(hits.iter().all(|(_, h)| h.span.heading.is_some()));
    }

    #[test]
    fn crlf_file_splits_correctly() {
        let tmp = TempDir::new().unwrap();
        let body = "# A\r\nalpha\r\n# B\r\nbeta\r\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "A", 1), sec(1, "B", 3)];
        let idx = corpus(&[("doc.md", "alpha beta")]);
        let q = compiled("alpha OR beta");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 2);
        let a = hits.iter().find(|(_, h)| h.span.line_start == 1).unwrap();
        assert_eq!(a.1.span.line_end, 2);
        let b = hits.iter().find(|(_, h)| h.span.line_start == 3).unwrap();
        assert_eq!(b.1.span.line_end, 4);
    }

    #[test]
    fn and_requires_both_terms_in_the_same_section() {
        let tmp = TempDir::new().unwrap();
        let body = "# One\nalpha only here.\n# Two\nalpha and beta together.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 3)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("alpha beta");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("Two"));
    }

    #[test]
    fn or_group_and_negation_and_field_terms_are_section_scoped_correctly() {
        let tmp = TempDir::new().unwrap();
        let body = "# One\nalpha here.\n# Two\nbeta here.\n# Three\ngamma only.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 3), sec(1, "Three", 5)];
        let idx = corpus(&[("doc.md", body)]);

        // OR: both One (alpha) and Two (beta) hit, Three does not.
        let q = compiled("alpha OR beta");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        let mut headings: Vec<_> = hits
            .iter()
            .map(|(_, h)| h.span.heading.clone().unwrap())
            .collect();
        headings.sort();
        assert_eq!(headings, vec!["One".to_owned(), "Two".to_owned()]);

        // Negation ignored at section level: "alpha -gamma" still hits One,
        // even though "beta"/"gamma" are irrelevant to this per-section check.
        let q = compiled("alpha -gamma");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("One"));

        // Field term ignored: "alpha title:x" still hits purely on "alpha".
        let q = compiled("alpha title:x");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("One"));
    }

    #[test]
    fn phrase_spans_lines_within_a_section() {
        let tmp = TempDir::new().unwrap();
        // "quick fox" splits across the line break, but both lines are
        // inside section One.
        let body = "# One\nthe quick\nfox jumps.\n# Two\nnothing related here.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 4)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("\"quick fox\"");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("One"));
    }

    #[test]
    fn phrase_split_across_a_heading_boundary_does_not_hit() {
        let tmp = TempDir::new().unwrap();
        // "quick" ends section One; "fox" begins section Two's body. If the
        // sliding window leaked across the boundary this would wrongly hit.
        let body = "# One\nsomething quick\n# Two\nfox jumps immediately.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 3)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("\"quick fox\"");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn prefix_leaf_uses_corpus_expansion() {
        let tmp = TempDir::new().unwrap();
        let body = "# One\nconfiguration options live here.\n# Two\nnothing relevant.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 3)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("conf*");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("One"));
    }

    #[test]
    fn scope_restricts_eligibility_and_excludes_the_preamble() {
        let tmp = TempDir::new().unwrap();
        let body = "preamble alpha.\n# One\nalpha here.\n# Two\nalpha here too.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 2), sec(1, "Two", 4)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);

        // Scope covering only section One's heading line.
        let scope = [SectionRange { start: 2, end: 3 }];
        let fs = scorer
            .collect(
                tmp.path(),
                "doc.md",
                StemLanguage::English,
                &outline,
                Some(&scope),
            )
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("One"));
    }

    #[test]
    fn shorter_section_scores_higher_for_equal_term_frequency() {
        let tmp = TempDir::new().unwrap();
        let body = "# Short\nalpha.\n# Long\nalpha plus a lot of other padding words here to lengthen this section considerably.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "Short", 1), sec(1, "Long", 3)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("Short"));
        assert!(hits[0].1.score > hits[1].1.score);
    }

    #[test]
    fn snippets_are_scoped_to_their_section() {
        let tmp = TempDir::new().unwrap();
        let body = "# One\nalpha line.\n# Two\nunrelated line.\nanother alpha line.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1), sec(1, "Two", 3)];
        let idx = corpus(&[("doc.md", body)]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 2);
        for (_, hit) in &hits {
            assert_eq!(hit.matches.len(), 1);
            assert!(hit.matches[0].text.contains("alpha"));
            assert!(
                hit.span.line_start <= hit.matches[0].line
                    && hit.matches[0].line <= hit.span.line_end
            );
        }
    }

    #[test]
    fn has_text_terms_false_for_field_only_and_negation_only_queries() {
        let idx = corpus(&[("doc.md", "alpha beta")]);
        for q in ["title:alpha", "-alpha"] {
            let compiled = compiled(q);
            let scorer = idx.section_scorer(&compiled);
            assert!(
                !scorer.has_text_terms(),
                "{q:?} should have no positive text leaf left"
            );
        }
        // Sanity: a normal term query does have text terms.
        let scorer = idx.section_scorer(&compiled("alpha"));
        assert!(scorer.has_text_terms());
    }

    #[test]
    fn zero_token_heading_section_is_kept_but_never_hits() {
        let tmp = TempDir::new().unwrap();
        // "##!!!" is not a valid ATX heading per se, so use a heading whose
        // text tokenizes to nothing: pure punctuation after the markers.
        let body = "# ---\n# Real\nalpha content.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "---", 1), sec(1, "Real", 2)];
        let idx = corpus(&[("doc.md", "alpha content")]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        // The "---" heading section spans only its own heading line, which
        // tokenizes to nothing — zero tokens, never a hit, but still present.
        assert_eq!(fs.sections.len(), 2);
        let empty = fs
            .sections
            .iter()
            .find(|s| s.span.heading.as_deref() == Some("---"))
            .unwrap();
        assert_eq!(empty.token_count, 0);
        let hits = scorer.finish(vec![("doc.md".to_owned(), fs)]);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.span.heading.as_deref(), Some("Real"));
    }

    #[test]
    fn empty_preamble_is_dropped_not_kept() {
        let tmp = TempDir::new().unwrap();
        let body = "# One\nalpha.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 1)];
        let idx = corpus(&[("doc.md", "alpha")]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        assert_eq!(fs.sections.len(), 1);
        assert_eq!(fs.sections[0].span.heading.as_deref(), Some("One"));
    }

    #[test]
    fn body_offset_reports_frontmatter_line_count() {
        let tmp = TempDir::new().unwrap();
        let body = "---\ntitle: X\n---\n# One\nalpha.\n";
        write(&tmp, "doc.md", body);
        let outline = vec![sec(1, "One", 4)];
        let idx = corpus(&[("doc.md", "alpha")]);
        let q = compiled("alpha");
        let scorer = idx.section_scorer(&q);
        let fs = scorer
            .collect(tmp.path(), "doc.md", StemLanguage::English, &outline, None)
            .unwrap();
        assert_eq!(fs.body_offset(), 3);
        assert_eq!(fs.sections[0].span.line_start, 4);
    }
}
