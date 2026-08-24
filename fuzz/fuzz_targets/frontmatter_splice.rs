//! Fuzz target: frontmatter parse + the minimal-diff splice path
//! (`crates/hyalo-core/src/frontmatter/{parse,splice}.rs`).
//!
//! Every `hyalo set`/`append`/`remove` does a read-modify-write through
//! `write_frontmatter`, which parses whatever YAML is already on disk, then
//! calls `splice_frontmatter` to re-emit unchanged keys byte-for-byte and
//! serialize only what changed (iter-214's minimal-diff write). Both halves
//! run on attacker-controlled bytes (a vault file can come from anywhere —
//! a cloned repo, a shared Obsidian vault, etc.), so both need to survive
//! arbitrary input without panicking.
//!
//! `data` is used directly as the starting file content (so real `.md`
//! fixtures work unmodified as corpus seeds — see `fuzz/corpus/`), with a
//! handful of deterministic mutations derived from its own tail bytes to
//! also drive the splice path on top of whatever frontmatter it parses to.

#![no_main]

use indexmap::IndexMap;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use std::sync::OnceLock;

/// One reusable scratch file for the process's lifetime instead of a fresh
/// `tempdir()` per iteration — `write_frontmatter` needs a real path (it
/// streams the frontmatter/body split from disk), and libFuzzer calls this
/// closure orders of magnitude more often than a per-call tempdir affords.
fn scratch_path() -> &'static std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fuzz.md");
        // Leaked deliberately: the directory must outlive every future call
        // into this process, and cargo-fuzz owns cleanup of its own tmp tree.
        std::mem::forget(dir);
        path
    })
}

fuzz_target!(|data: &[u8]| {
    // Caps allocation-driven runaway inputs so the fuzzer spends its budget
    // on splice logic rather than re-discovering the size limit repeatedly.
    if data.len() > 256 * 1024 {
        return;
    }

    let path = scratch_path();
    if std::fs::write(path, data).is_err() {
        return;
    }

    // Read side: `Document::parse` (via `read_frontmatter`) must never panic
    // on arbitrary bytes, regardless of what the write side below then does.
    let existing = hyalo_core::frontmatter::read_frontmatter(path);
    let mut props: IndexMap<String, Value> = existing.unwrap_or_default();

    // A few deterministic mutations derived from the input's own tail, so a
    // single corpus entry exercises both "parse this" and "splice a change
    // into this" without needing a second, harder-to-seed input format.
    let tail_start = data.len().saturating_sub(64);
    for (i, chunk) in data[tail_start..].chunks(8).enumerate().take(4) {
        props.insert(
            format!("fuzz_prop_{i}"),
            Value::String(String::from_utf8_lossy(chunk).into_owned()),
        );
    }

    // Write side: this is what actually drives `splice_frontmatter` when the
    // file already had a parseable frontmatter block.
    let _ = hyalo_core::frontmatter::write_frontmatter(path, &props);

    // Round-trip: whatever was just written must itself be re-parseable —
    // a splice bug that emits invalid YAML would show up here even if
    // `write_frontmatter` itself didn't panic.
    let _ = hyalo_core::frontmatter::read_frontmatter(path);
});
