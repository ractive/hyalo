//! Fuzz target: the MessagePack snapshot-index loader
//! (`SnapshotIndex::load`, `crates/hyalo-core/src/index.rs`).
//!
//! A `.hyalo-index` snapshot is untrusted input in the same sense a vault
//! file is — it can come from a synced/shared directory, not just a run of
//! `hyalo` itself — and `load_inner` already carries defense-in-depth caps
//! (SEC-2 entry-count limit, SEC-3 graph/BM25-postings limits, SEC-1 rel-path
//! validation) precisely because a crafted MessagePack header can otherwise
//! drive large allocations before those checks ever run. This target is the
//! systematic complement to the one-off PoCs that motivated those checks.

#![no_main]

use hyalo_core::index::SnapshotIndex;
use libfuzzer_sys::fuzz_target;
use std::sync::OnceLock;

/// `SnapshotIndex::load` only takes a path (it needs to `stat` the file
/// before reading, to enforce the size cap) — one reusable scratch file
/// avoids a `tempdir()` per iteration across libFuzzer's millions of calls.
fn scratch_path() -> &'static std::path::PathBuf {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("snapshot.msgpack");
        std::mem::forget(dir);
        path
    })
}

fuzz_target!(|data: &[u8]| {
    // Above this, every input just re-exercises the same size-cap rejection
    // path (MAX_INDEX_FILE_SIZE is 512 MiB) rather than the parser itself.
    if data.len() > 4 * 1024 * 1024 {
        return;
    }
    let path = scratch_path();
    if std::fs::write(path, data).is_err() {
        return;
    }
    let _ = SnapshotIndex::load(path);
});
