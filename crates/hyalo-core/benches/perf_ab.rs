//! A/B benchmarks comparing the optimized (memchr + rayon + parallel walk)
//! implementation against sequential baselines.
//!
//! Run with:
//!   HYALO_BENCH_VAULT=../mdn/files/en-us cargo bench --bench perf_ab
//!
//! Each benchmark group contains two functions: `sequential` and `parallel`
//! (or `naive` and `memchr`), so criterion reports the relative speedup.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hyalo_core::content_search::ContentSearchVisitor;
use hyalo_core::discovery::discover_files;
use hyalo_core::index::{ScanOptions, ScannedIndex};
use hyalo_core::scanner::{FileVisitor, scan_file_multi, scan_slice_multi};
use hyalo_core::tasks::TaskCounter;

fn vault_path() -> Option<PathBuf> {
    let path = std::env::var("HYALO_BENCH_VAULT")
        .map_or_else(|_| PathBuf::from("../mdn/files/en-us"), PathBuf::from);
    if path.is_dir() {
        Some(path)
    } else {
        eprintln!(
            "WARN: vault not found at {}. Set HYALO_BENCH_VAULT. Skipping benchmarks.",
            path.display()
        );
        None
    }
}

/// Prepare (files, rel_paths) tuples for index building.
fn prepare_files(vault: &Path) -> Vec<(PathBuf, String)> {
    let files = discover_files(vault).unwrap();
    files
        .into_iter()
        .map(|full| {
            let rel = full
                .strip_prefix(vault)
                .unwrap_or(&full)
                .to_string_lossy()
                .into_owned();
            (full, rel)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Benchmark: scan_file (BufReader vs memchr slice)
// ---------------------------------------------------------------------------

/// Baseline: read file line-by-line with BufReader (simulates old I/O path overhead).
fn scan_bufreader(path: &Path) {
    let file = std::fs::File::open(path).unwrap();
    let mut reader = BufReader::new(file);
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        black_box(&buf);
    }
}

fn bench_scan_file(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = discover_files(&vault).unwrap();

    let mut group = c.benchmark_group("scan_all_files");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // Current: memchr-based scan_file_multi (which uses scan_buffer_multi internally)
    group.bench_function("memchr_slice", |b| {
        b.iter(|| {
            for file in &files {
                let mut counter = TaskCounter::new();
                let mut search = ContentSearchVisitor::new("XMLHttpRequest");
                let visitors: &mut [&mut dyn FileVisitor] = &mut [&mut counter, &mut search];
                scan_file_multi(black_box(file), visitors).unwrap();
            }
        });
    });

    // Baseline: read file + iterate lines with BufReader (simulates old I/O path)
    group.bench_function("bufreader_lines", |b| {
        b.iter(|| {
            for file in &files {
                scan_bufreader(black_box(file));
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: index build (sequential vs parallel rayon)
// ---------------------------------------------------------------------------

fn bench_index_build(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = prepare_files(&vault);
    let options = ScanOptions { scan_body: true };

    let mut group = c.benchmark_group("index_build");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));

    // Current: parallel (rayon par_iter)
    group.bench_function("parallel_rayon", |b| {
        b.iter(|| {
            ScannedIndex::build(black_box(&files), None, &options).unwrap();
        });
    });

    // Baseline: sequential — build with rayon thread pool limited to 1 thread
    group.bench_function("sequential", |b| {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        b.iter(|| {
            pool.install(|| {
                ScannedIndex::build(black_box(&files), None, &options).unwrap();
            });
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: content search with/without fast-reject
// ---------------------------------------------------------------------------

fn bench_content_search(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = discover_files(&vault).unwrap();

    let mut group = c.benchmark_group("content_search");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // With fast-reject: read file, check if pattern exists, only scan if so
    group.bench_function("with_fast_reject", |b| {
        b.iter(|| {
            let mut total = 0usize;
            let mut scratch = Vec::new();
            for file in &files {
                let data = std::fs::read(black_box(file)).unwrap();
                let lowered_pattern = b"xmlhttprequest";
                if hyalo_core::content_search::fast_reject(&data, lowered_pattern, &mut scratch) {
                    continue;
                }
                let mut visitor = ContentSearchVisitor::new("XMLHttpRequest");
                scan_slice_multi(&data, &mut [&mut visitor]).unwrap();
                total += visitor.into_matches().len();
            }
            total
        });
    });

    // Without fast-reject: always run full scan
    group.bench_function("without_fast_reject", |b| {
        b.iter(|| {
            let mut total = 0usize;
            for file in &files {
                let mut visitor = ContentSearchVisitor::new("XMLHttpRequest");
                scan_file_multi(black_box(file), &mut [&mut visitor]).unwrap();
                total += visitor.into_matches().len();
            }
            total
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmark: discover_files (parallel walk)
// ---------------------------------------------------------------------------

fn bench_discover(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };

    let mut group = c.benchmark_group("discover_files");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(20));

    // Current: parallel walk (build_parallel)
    group.bench_function("parallel_walk", |b| {
        b.iter(|| discover_files(black_box(&vault)).unwrap());
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_discover,
    bench_scan_file,
    bench_index_build,
    bench_content_search,
);
criterion_main!(benches);
