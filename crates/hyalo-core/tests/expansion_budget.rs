use hyalo_core::bm25::Bm25InvertedIndex;
use hyalo_core::index::SnapshotIndex;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct MeasuredAllocator;
static MEASURE: AtomicBool = AtomicBool::new(false);
static BYTES: AtomicUsize = AtomicUsize::new(0);
// Safety: all operations delegate the unchanged layout/pointer to System.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if MEASURE.load(Ordering::Relaxed) {
            BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(ptr, layout);
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if MEASURE.load(Ordering::Relaxed) {
            BYTES.fetch_add(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn compact_snapshot_refuses_expansion_before_effect_refresh_with_measured_allocations() {
    let tmp = tempfile::tempdir().unwrap();
    let note = tmp.path().join("a.md");
    std::fs::write(&note, "unchanged authored note\n").unwrap();
    let term = "x".repeat(8192);
    let bm25 = serde_json::json!({"postings": {term: [{"doc_id":0,"term_freq":32768,"positions":(0..32768).collect::<Vec<_>>()}]}, "doc_lengths":[32768], "doc_paths":["a.md"],"avgdl":32768.0,"tokenizer_version":1});
    let index: Bm25InvertedIndex = serde_json::from_value(bm25.clone()).unwrap();
    let envelope = serde_json::json!({"header":{"vault_dir":tmp.path(),"site_prefix":null,"created_at":0,"pid":0,"format_version":2},"entries":[],"graph":{"index":{}},"bm25_index":bm25});
    let path = tmp.path().join(".snapshot");
    let bytes = rmp_serde::to_vec_named(&envelope).unwrap();
    assert!(bytes.len() < 200_000);
    std::fs::write(&path, &bytes).unwrap();
    BYTES.store(0, Ordering::Relaxed);
    MEASURE.store(true, Ordering::Relaxed);
    assert!(index.reconstruct_all_tokens().is_empty());
    let mut snapshot = SnapshotIndex::load(&path).unwrap().unwrap();
    assert!(snapshot.bm25_index().is_none());
    assert!(snapshot.validate_before_changes().is_err());
    assert!(
        snapshot
            .apply_changes(tmp.path(), &["a.md".into()])
            .is_err()
    );
    MEASURE.store(false, Ordering::Relaxed);
    let allocated = BYTES.load(Ordering::Relaxed);
    assert!(allocated < 4 * 1024 * 1024, "allocated {allocated} bytes");
    assert_eq!(
        std::fs::read_to_string(note).unwrap(),
        "unchanged authored note\n"
    );
    eprintln!(
        "snapshot_bytes={}, attempted_expansion>256MiB, measured_allocation_bytes={allocated}, budget=4MiB",
        bytes.len()
    );
}
