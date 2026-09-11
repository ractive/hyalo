//! Opt-in process counters used by repository-owned measurement gates.
//!
//! These counters describe successful logical operations in Hyalo's shared
//! scanner and index owner. They are not filesystem syscall, byte-read, or RSS
//! measurements. Normal library and CLI use leaves them disabled.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LOGICAL_SOURCE_READS: AtomicU64 = AtomicU64::new(0);
static LOGICAL_BODY_READS: AtomicU64 = AtomicU64::new(0);
static INDEX_ENTRIES_REFRESHED: AtomicU64 = AtomicU64::new(0);

/// A process-local snapshot of repository measurement counters.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Snapshot {
    pub logical_source_reads: u64,
    pub logical_body_reads: u64,
    pub index_entries_refreshed: u64,
}

/// Enable counters for this process. Intended for the CLI's internal metrics
/// report bridge; normal commands never call this.
#[doc(hidden)]
pub fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

/// Return the current process-local counters.
#[doc(hidden)]
pub fn snapshot() -> Snapshot {
    Snapshot {
        logical_source_reads: LOGICAL_SOURCE_READS.load(Ordering::Relaxed),
        logical_body_reads: LOGICAL_BODY_READS.load(Ordering::Relaxed),
        index_entries_refreshed: INDEX_ENTRIES_REFRESHED.load(Ordering::Relaxed),
    }
}

/// Record one successfully acquired logical source. `body` is true when the
/// acquisition made the complete source available for body processing.
#[doc(hidden)]
pub fn record_source_read(body: bool) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    LOGICAL_SOURCE_READS.fetch_add(1, Ordering::Relaxed);
    if body {
        LOGICAL_BODY_READS.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn record_index_entries_refreshed(count: usize) {
    if ENABLED.load(Ordering::Relaxed) {
        INDEX_ENTRIES_REFRESHED.fetch_add(count as u64, Ordering::Relaxed);
    }
}
