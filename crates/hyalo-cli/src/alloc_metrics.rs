//! Opt-in process counters for repository scale measurements.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

struct CountingAllocator;

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static ENABLED: AtomicBool = AtomicBool::new(false);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record_allocation(size: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every operation delegates to `System` with the original pointer and
// layout. The atomics only observe requested sizes and do not affect ownership.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: delegated with the caller-provided valid layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ENABLED.load(Ordering::Relaxed) {
            let size = layout.size() as u64;
            let _ = LIVE_BYTES.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
                Some(live.saturating_sub(size))
            });
        }
        // SAFETY: delegated with the same pointer/layout contract as the caller.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: delegated with the caller-provided allocation and new size.
        let replacement = unsafe { System.realloc(pointer, old, new_size) };
        if !replacement.is_null() && ENABLED.load(Ordering::Relaxed) {
            let size = old.size() as u64;
            let _ = LIVE_BYTES.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
                Some(live.saturating_sub(size))
            });
            record_allocation(new_size);
        }
        replacement
    }
}

pub fn enable_if_requested() {
    let allocation_report = std::env::var_os("HYALO_INTERNAL_ALLOC_REPORT").is_some();
    let metrics_report = std::env::var_os("HYALO_INTERNAL_METRICS_REPORT").is_some();
    if allocation_report {
        ENABLED.store(true, Ordering::Relaxed);
    }
    if allocation_report || metrics_report {
        hyalo_core::internal_metrics::enable();
    }
}

pub fn write_report() {
    let metrics_path = std::env::var("HYALO_INTERNAL_METRICS_REPORT").ok();
    let allocation_path = std::env::var("HYALO_INTERNAL_ALLOC_REPORT").ok();
    let Some(path) = metrics_path.or(allocation_path) else {
        return;
    };
    let reads = hyalo_core::internal_metrics::snapshot();
    let allocation_enabled = ENABLED.load(Ordering::Relaxed);
    let report = serde_json::json!({
        "allocations": allocation_enabled.then(|| ALLOCATIONS.load(Ordering::Relaxed)),
        "allocated_bytes": allocation_enabled.then(|| ALLOCATED_BYTES.load(Ordering::Relaxed)),
        "peak_live_bytes": allocation_enabled.then(|| PEAK_LIVE_BYTES.load(Ordering::Relaxed)),
        "logical_source_reads": reads.logical_source_reads,
        "logical_body_reads": reads.logical_body_reads,
        "index_entries_refreshed": reads.index_entries_refreshed,
        "allocation_scope": "successful CLI parent process after main starts; allocator requests, not RSS or child-worker allocations",
        "read_scope": "successful shared-scanner file reads; body means a scan requested body events; excludes stat/open calls, byte counts and kernel syscalls",
        "refresh_scope": "successful in-memory index entry replacements or insertions after a source scan; excludes initial index construction and persistence syscalls",
    });
    if let Err(error) = std::fs::write(&path, format!("{report}\n")) {
        eprintln!("warning: could not write internal allocation report {path}: {error}");
    }
}
