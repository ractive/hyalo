use criterion::{Criterion, black_box, criterion_group, criterion_main};
use hyalo_core::filter::parse_property_filter;
use hyalo_core::links::{Link, extract_links_from_text};
use hyalo_core::scanner::{strip_inline_code, strip_inline_comments};
use hyalo_core::tasks::detect_task_checkbox;

fn bench_strip_inline_code(c: &mut Criterion) {
    let no_backtick = "This is a regular line with no code spans at all";
    let with_backtick = "Use `HashMap` and `Vec<String>` for the `cache` field";

    let mut group = c.benchmark_group("strip_inline_code");
    group.bench_function("no_backticks", |b| {
        b.iter(|| strip_inline_code(black_box(no_backtick)));
    });
    group.bench_function("with_backticks", |b| {
        b.iter(|| strip_inline_code(black_box(with_backtick)));
    });
    group.finish();
}

fn bench_strip_inline_comments(c: &mut Criterion) {
    let no_comment = "Regular line without any percent signs";
    let with_comment = "Visible text %%hidden comment%% more visible text";

    let mut group = c.benchmark_group("strip_inline_comments");
    group.bench_function("no_comments", |b| {
        b.iter(|| strip_inline_comments(black_box(no_comment)));
    });
    group.bench_function("with_comments", |b| {
        b.iter(|| strip_inline_comments(black_box(with_comment)));
    });
    group.finish();
}

fn bench_detect_task_checkbox(c: &mut Criterion) {
    let mut group = c.benchmark_group("detect_task_checkbox");
    group.bench_function("task_line", |b| {
        b.iter(|| detect_task_checkbox(black_box("- [ ] Write benchmarks")));
    });
    group.bench_function("non_task_line", |b| {
        b.iter(|| detect_task_checkbox(black_box("Regular paragraph text")));
    });
    group.bench_function("indented_task", |b| {
        b.iter(|| detect_task_checkbox(black_box("    - [x] Nested done task")));
    });
    group.finish();
}

fn bench_extract_links(c: &mut Criterion) {
    let sparse = "See [[Note A]] for details on this topic.";
    let dense = "Links: [[A]], [[B|label]], [[C#heading]], [md](d.md), ![[embed]]";

    let mut group = c.benchmark_group("extract_links");
    group.bench_function("sparse_line", |b| {
        b.iter(|| {
            let mut out: Vec<Link> = Vec::new();
            extract_links_from_text(black_box(sparse), &mut out);
            out
        });
    });
    group.bench_function("dense_line", |b| {
        b.iter(|| {
            let mut out: Vec<Link> = Vec::new();
            extract_links_from_text(black_box(dense), &mut out);
            out
        });
    });
    group.finish();
}

fn bench_parse_property_filter(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_property_filter");
    group.bench_function("equality", |b| {
        b.iter(|| parse_property_filter(black_box("status=draft")));
    });
    group.bench_function("comparison", |b| {
        b.iter(|| parse_property_filter(black_box("priority>=3")));
    });
    group.bench_function("exists", |b| {
        b.iter(|| parse_property_filter(black_box("title")));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_strip_inline_code,
    bench_strip_inline_comments,
    bench_detect_task_checkbox,
    bench_extract_links,
    bench_parse_property_filter,
);
criterion_main!(benches);
