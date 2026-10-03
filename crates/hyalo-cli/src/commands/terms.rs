#![allow(clippy::missing_errors_doc)]
//! `hyalo terms` — list BM25 dictionary terms (stemmed tokens) with their
//! document frequency: the number of files whose authored title or body
//! contains the stem.
//!
//! Corpus resolution mirrors the other read commands: an `--index`/
//! `--index-file` snapshot is used directly when its persisted BM25 index is
//! present and current (`tokenizer_version() == TOKENIZER_VERSION`) and no
//! `--glob` narrows the corpus; otherwise a fresh in-memory scan is built
//! (`ScanOptions { bm25_tokenize: true, .. }`), exactly like `create-index`
//! builds its persisted dictionary.

use anyhow::Result;
use hyalo_core::bm25::{Bm25InvertedIndex, TOKENIZER_VERSION, parse_language, resolve_language};
use hyalo_core::index::{ScanOptions, VaultIndex as _};
use serde::Serialize;

use crate::commands::{ScannedIndexOutcome, build_scanned_index};
use crate::dispatch::{CommandContext, resolve_limit};
use crate::output::CommandOutcome;

/// One dictionary term and the number of files whose authored title or body
/// contains it (the BM25 corpus indexes both).
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
pub struct TermEntry {
    /// Stemmed token.
    pub(crate) term: String,
    /// Number of files whose title or body contains this stem.
    pub(crate) docs: usize,
}

/// Build the dictionary from a fresh in-memory scan (`--glob`-filterable),
/// honouring `[scan] exclude` and warning reporting exactly like
/// `create-index`'s disk scan (shared via [`build_scanned_index`]). Returns
/// the caller-facing outcome as-is when file resolution itself failed (e.g.
/// an invalid glob).
fn dictionary_from_disk(
    ctx: &CommandContext<'_>,
    glob: &[String],
    prefix: Option<&str>,
) -> Result<Result<Vec<(String, usize)>, CommandOutcome>> {
    let outcome = build_scanned_index(
        ctx.dir,
        &[],
        glob,
        ctx.effective_format,
        ctx.site_prefix,
        false,
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: true,
            default_language: ctx.config_language,
            frontmatter_link_props: ctx.frontmatter_link_props,
        },
    )?;
    let build = match outcome {
        ScannedIndexOutcome::Index(build) => build,
        ScannedIndexOutcome::Outcome(outcome) => return Ok(Err(outcome)),
    };
    let dict =
        Bm25InvertedIndex::build_from_entries(build.index.entries()).map_or_else(Vec::new, |idx| {
            idx.dictionary(prefix)
                .into_iter()
                .map(|(term, docs)| (term.to_owned(), docs))
                .collect()
        });
    Ok(Ok(dict))
}

/// `hyalo terms` dispatch arm.
pub(crate) fn run(
    ctx: &mut CommandContext<'_>,
    prefix: Option<&str>,
    glob: &[String],
    limit: Option<usize>,
) -> Result<CommandOutcome> {
    // Fast path: an up-to-date snapshot's persisted BM25 dictionary, when no
    // --glob narrows the corpus (a glob needs a fresh scan scoped to the
    // matched files — the persisted index covers the whole vault).
    // The persisted stems are only reusable when every document was stemmed
    // with the language a fresh scan would use now (same guard as `find`): a
    // `[search] language` changed after `create-index`, or mixed
    // `bm25_language` metadata, falls back to the disk scan.
    if glob.is_empty()
        && let Some(snap) = ctx.snapshot_index.as_ref()
        && let Some(bm25) = snap.bm25_index()
        && bm25.tokenizer_version() == TOKENIZER_VERSION
        && bm25.doc_count() == snap.entries().len()
        && bm25.document_paths().all(|path| {
            snap.get(path).is_some_and(|entry| {
                let fm_lang = entry.properties.get("language").and_then(|v| v.as_str());
                entry
                    .bm25_language
                    .as_deref()
                    .and_then(|s| parse_language(s).ok())
                    == Some(resolve_language(fm_lang, None, ctx.config_language))
            })
        })
    {
        let dict: Vec<(String, usize)> = bm25
            .dictionary(prefix)
            .into_iter()
            .map(|(term, docs)| (term.to_owned(), docs))
            .collect();
        return finish(dict, limit, ctx);
    }

    match dictionary_from_disk(ctx, glob, prefix)? {
        Ok(dict) => finish(dict, limit, ctx),
        Err(outcome) => Ok(outcome),
    }
}

fn finish(
    dict: Vec<(String, usize)>,
    limit: Option<usize>,
    ctx: &CommandContext<'_>,
) -> Result<CommandOutcome> {
    let total = dict.len() as u64;
    let mut dict = dict;
    if let Some(n) = resolve_limit(limit, ctx.config_default_limit, ctx.programmatic_output) {
        dict.truncate(n);
    }
    let results: Vec<TermEntry> = dict
        .into_iter()
        .map(|(term, docs)| TermEntry { term, docs })
        .collect();
    Ok(CommandOutcome::success_with_total(
        serde_json::to_value(&results)?,
        total,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyalo_core::index::ScannedIndex;

    fn build_test_index(dir: &std::path::Path) -> ScannedIndex {
        let files = hyalo_core::discovery::discover_files(dir).unwrap();
        let pairs: Vec<(std::path::PathBuf, String)> = files
            .into_iter()
            .map(|p| {
                let rel = hyalo_core::discovery::relative_path(dir, &p);
                (p, rel)
            })
            .collect();
        ScannedIndex::build(
            &pairs,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: true,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap()
        .index
    }

    #[test]
    fn dictionary_sorted_by_docs_then_term() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.md"), "running runner race\n").unwrap();
        std::fs::write(tmp.path().join("b.md"), "running race\n").unwrap();
        let index = build_test_index(tmp.path());
        let bm25 = Bm25InvertedIndex::build_from_entries(index.entries()).unwrap();
        let dict = bm25.dictionary(None);
        // "run" (stemmed from running/runner) appears in both files; "race" in
        // both; both should sort ahead of singleton terms, and "race" < "run"
        // alphabetically at equal doc count.
        let (_, top_docs) = dict.first().expect("dictionary must not be empty");
        assert_eq!(*top_docs, 2, "{dict:?}");
    }

    #[test]
    fn dictionary_prefix_filters_and_lowercases() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.md"), "Running jumping\n").unwrap();
        let index = build_test_index(tmp.path());
        let bm25 = Bm25InvertedIndex::build_from_entries(index.entries()).unwrap();
        let dict = bm25.dictionary(Some("RUN"));
        assert_eq!(dict.len(), 1, "{dict:?}");
        assert!(dict.iter().all(|(t, _)| t.starts_with("run")), "{dict:?}");
    }
}
