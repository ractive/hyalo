use std::path::PathBuf;
use std::time::Duration;

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hyalo::commands::find::find;
use hyalo::commands::properties::properties_summary;
use hyalo::commands::summary::summary;
use hyalo::commands::tags::tags_summary;
use hyalo::content_search::ContentSearchVisitor;
use hyalo::discovery::discover_files;
use hyalo::filter::Fields;
use hyalo::frontmatter::read_frontmatter;
use hyalo::output::Format;
use hyalo::scanner::{FileVisitor, scan_file_multi};
use hyalo::tasks::TaskCounter;

fn vault_path() -> Option<PathBuf> {
    let path = std::env::var("HYALO_BENCH_VAULT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("../obsidian-hub"));
    if path.is_dir() {
        Some(path)
    } else {
        eprintln!(
            "WARN: vault not found at {}. Set HYALO_BENCH_VAULT. Skipping vault benchmarks.",
            path.display()
        );
        None
    }
}

fn bench_discover_files(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    c.bench_function("discover_files", |b| {
        b.iter(|| discover_files(black_box(&vault)).unwrap())
    });
}

fn bench_read_frontmatter(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = discover_files(&vault).unwrap();

    // Pick a representative sample: first, middle, last
    let indices = [0, files.len() / 2, files.len().saturating_sub(1)];
    let mut group = c.benchmark_group("read_frontmatter");
    for (i, &idx) in indices.iter().enumerate() {
        if let Some(path) = files.get(idx) {
            group.bench_function(format!("file_{i}"), |b| {
                b.iter(|| read_frontmatter(black_box(path)).unwrap())
            });
        }
    }
    group.finish();
}

fn bench_read_all_frontmatter(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = discover_files(&vault).unwrap();

    let mut group = c.benchmark_group("read_all_frontmatter");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("all_files", |b| {
        b.iter(|| {
            for file in &files {
                // Some vault files may have malformed YAML — skip parse errors,
                // but still measure the I/O and parsing attempt.
                let _ = read_frontmatter(black_box(file));
            }
        })
    });
    group.finish();
}

fn bench_scan_all_files(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let files = discover_files(&vault).unwrap();

    let mut group = c.benchmark_group("scan_all_files");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("multi_visitor", |b| {
        b.iter(|| {
            for file in &files {
                let mut counter = TaskCounter::new();
                let mut search = ContentSearchVisitor::new("obsidian");
                let visitors: &mut [&mut dyn FileVisitor] = &mut [&mut counter, &mut search];
                scan_file_multi(black_box(file), visitors)
                    .expect("scan_file_multi failed during benchmark");
            }
        })
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Command-level benchmarks (exercise parallel processing)
// ---------------------------------------------------------------------------

fn bench_cmd_find(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let fields = Fields::default();

    let mut group = c.benchmark_group("cmd_find");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("all_files", |b| {
        b.iter(|| {
            find(
                black_box(&vault),
                None,
                &[],
                &[],
                None,
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap()
        })
    });
    group.finish();
}

fn bench_cmd_find_content_search(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };
    let fields = Fields::default();

    let mut group = c.benchmark_group("cmd_find");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("content_search", |b| {
        b.iter(|| {
            find(
                black_box(&vault),
                Some("obsidian"),
                &[],
                &[],
                None,
                None,
                None,
                &fields,
                None,
                None,
                Format::Json,
            )
            .unwrap()
        })
    });
    group.finish();
}

fn bench_cmd_properties(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };

    let mut group = c.benchmark_group("cmd_properties");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("all_files", |b| {
        b.iter(|| properties_summary(black_box(&vault), None, None, Format::Json).unwrap())
    });
    group.finish();
}

fn bench_cmd_tags(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };

    let mut group = c.benchmark_group("cmd_tags");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("all_files", |b| {
        b.iter(|| tags_summary(black_box(&vault), None, None, Format::Json).unwrap())
    });
    group.finish();
}

fn bench_cmd_summary(c: &mut Criterion) {
    let Some(vault) = vault_path() else { return };

    let mut group = c.benchmark_group("cmd_summary");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("all_files", |b| {
        b.iter(|| summary(black_box(&vault), None, 10, Format::Json).unwrap())
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_discover_files,
    bench_read_frontmatter,
    bench_read_all_frontmatter,
    bench_scan_all_files,
    bench_cmd_find,
    bench_cmd_find_content_search,
    bench_cmd_properties,
    bench_cmd_tags,
    bench_cmd_summary,
);
criterion_main!(benches);
