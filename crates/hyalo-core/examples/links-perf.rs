//! Iteration-206 profiling harness for the `links fix` pipeline.
//!
//! Usage: `cargo run -p hyalo-core --release --example links-perf <vault-dir>`
//!
//! Times each phase of `hyalo links fix` (dry-run) separately — index scan,
//! broken-link detection, `LinkMatcher` construction, and `plan_fixes` — and
//! prints link/broken counts so a wall-clock number can be attributed to a
//! phase instead of guessed at. Read-only: never writes to the vault.
//!
//! Created for the iteration-206 links profiling (see
//! `hyalo-knowledgebase/iterations/iteration-206-links-perf-profiling.md`);
//! kept so future perf regressions can be re-attributed without rebuilding
//! the tooling.

use std::path::{Path, PathBuf};
use std::time::Instant;

use hyalo_core::discovery::{canonicalize_vault_dir, discover_files};
use hyalo_core::index::{ScanOptions, ScannedIndex};
use hyalo_core::link_fix::{LinkMatcher, detect_broken_links_from_index, plan_fixes};

/// Default fuzzy candidacy threshold used by `hyalo links fix`.
const DEFAULT_THRESHOLD: f64 = 0.85;

fn main() -> anyhow::Result<()> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".to_string());
    let canonical = canonicalize_vault_dir(Path::new(&dir))?;
    let prefix = canonical
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);

    let t = Instant::now();
    let file_paths = discover_files(&canonical)?;
    println!(
        "discover_files:       {:>9.3?}  ({} files)",
        t.elapsed(),
        file_paths.len()
    );
    let files: Vec<(PathBuf, String)> = file_paths
        .iter()
        .map(|p| {
            let rel = p
                .strip_prefix(&canonical)
                .unwrap_or(p.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            (p.clone(), rel)
        })
        .collect();

    let t = Instant::now();
    let build = ScannedIndex::build(
        &files,
        prefix.as_deref(),
        &ScanOptions {
            scan_body: true,
            bm25_tokenize: false,
            default_language: None,
            frontmatter_link_props: None,
        },
    )?;
    let index = build.index;
    println!("ScannedIndex::build:  {:>9.3?}", t.elapsed());

    let t = Instant::now();
    let report = detect_broken_links_from_index(&canonical, &index, None, None, false);
    println!(
        "detect_broken_links:  {:>9.3?}  ({} total links, {} broken, {} case, {} reloc, {} amb, {} oov)",
        t.elapsed(),
        report.total_links,
        report.broken.len(),
        report.case_mismatches.len(),
        report.relocations.len(),
        report.ambiguous.len(),
        report.out_of_vault.len(),
    );

    let t = Instant::now();
    let matcher = LinkMatcher::from_index(&index, DEFAULT_THRESHOLD, None);
    println!("LinkMatcher::build:   {:>9.3?}", t.elapsed());

    let t = Instant::now();
    let fix_report = plan_fixes(&report.broken, &matcher);
    println!(
        "plan_fixes:           {:>9.3?}  ({} fixes, {} unfixable)",
        t.elapsed(),
        fix_report.fixes.len(),
        fix_report.unfixable.len(),
    );
    Ok(())
}
