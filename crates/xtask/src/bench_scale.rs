//! T-6 (iter-224): scale regression gate.
//!
//! Generates a deterministic ~14k-file synthetic vault and times `hyalo
//! find` / `hyalo links fix` against it, failing if either exceeds a
//! generous wall-time budget. CI invokes it with an explicit native target and
//! isolated `CARGO_TARGET_DIR` on Linux, macOS and Windows. It remains available
//! on demand as `cargo run -p xtask -- bench-scale`; see `decision-log.md`
//! DEC-098 for the budget rationale below.
//!
//! What this covers: gross wall-clock regressions (an accidental O(n²) path,
//! a dropped index fast-path) on a vault large enough that per-file overhead
//! actually shows up. What it does **not** cover: the fuzzy-candidate
//! matching perf debt tracked separately
//! ([[iterations/iteration-206-links-perf-profiling]]), sub-command timing
//! breakdowns, or memory usage — `bench-e2e.sh` (hyperfine-based, needs an
//! external vault) remains the tool for detailed A/B comparisons.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use crate::artifact::{ArtifactArgs, build_hyalo};
use crate::workspace::workspace_root;

/// File count large enough that per-file overhead (directory walk, frontmatter
/// parse, link resolution) dominates rather than fixed startup cost.
const FILE_COUNT: usize = 14_000;

/// Each file links to a few others by index, cycling through a fixed set of
/// tags/statuses — enough to give `links` real resolution work (some broken,
/// some valid, some ambiguous-by-title) without needing external randomness.
const TAGS: &[&str] = &["alpha", "beta", "gamma", "delta", "epsilon"];
const STATUSES: &[&str] = &["draft", "active", "done", "archived"];

/// Wall-time budgets, each with generous headroom over the local baseline
/// measured on Apple Silicon (`find`: ~0.39s, `links fix` dry-run: ~3.4s for
/// 14k files — see DEC-098; iter-278 took `links fix` to ~1.1s by answering
/// the literal link probe from the index, and the budget stays where it is). CI runners vary a lot in single-core speed, and
/// `links fix` in particular does real cross-file resolution work that scales
/// with corpus size, so these budgets sit at roughly 4-8x the observed
/// baseline: enough headroom to absorb a slow shared runner without masking
/// a genuine order-of-magnitude regression, which is the only class of bug
/// this gate exists to catch.
const FIND_BUDGET: Duration = Duration::from_secs(3);
const LINKS_BUDGET: Duration = Duration::from_secs(15);

/// Backlink fan-out for the `mv` write-phase case (iter-278, ALLOC-4).
///
/// DEC-317 (iter-277) made a bulk write phase fsync each touched *directory*
/// once instead of every file before its rename, and claimed the win for
/// `mv` — but the corpus that verified it (a copy of the Obsidian Hub) has no
/// note with a large enough fan-out to reach the parallel path: its
/// most-linked note rewrote 8 files, which is exactly the threshold at or
/// below which hyalo deliberately keeps the full per-file guarantee. So the
/// claim was never measured where it applies. This case renames one note that
/// [`FANOUT_BACKLINKS`] others link to, which puts the whole rewrite in one
/// parallel phase.
const FANOUT_BACKLINKS: usize = 2_000;

/// Fan-out small enough to stay on the durable per-file path (`<= 8` files),
/// timed beside the big one so the two costs are read off the same machine in
/// the same run rather than compared against a remembered number.
const FANOUT_SMALL: usize = 8;

/// Budget for renaming a note with [`FANOUT_BACKLINKS`] backlinks.
///
/// Measured at 0.52 s end to end on Apple Silicon (iter-278) — scan of 2 001
/// files, link graph, and the 2 000-file rewrite — i.e. 0.26 ms per rewritten
/// file, against 5.8 ms per file for the 8-file run that stays on the durable
/// per-file path. The same 2 000 files on that path would be ~2 000 × 5.5 ms
/// ≈ 11 s, so a 5 s budget fails loudly if the bulk phase is ever lost without
/// tripping on a slow shared runner.
const MV_FANOUT_BUDGET: Duration = Duration::from_secs(5);

/// Number of timed repetitions per command; the median absorbs one-off OS
/// scheduling noise (e.g. a background process stealing a core mid-run)
/// without hiding a real regression the way taking the min would.
const REPEATS: usize = 3;

#[derive(Debug)]
struct VaultStats {
    files: usize,
    valid_edges: usize,
    broken_edges: usize,
    source_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct AllocationStats {
    allocations: u64,
    allocated_bytes: u64,
    peak_live_bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
struct ReadStats {
    logical_source_reads: u64,
    logical_body_reads: u64,
    index_entries_refreshed: u64,
}

#[derive(Debug, Deserialize)]
struct ProcessMetricsReport {
    allocations: Option<u64>,
    allocated_bytes: Option<u64>,
    peak_live_bytes: Option<u64>,
    logical_source_reads: u64,
    logical_body_reads: u64,
    index_entries_refreshed: u64,
}

struct TimedSample {
    elapsed: Duration,
    output_bytes: usize,
    allocations: AllocationStats,
    reads: ReadStats,
}

pub fn run(args: &ArtifactArgs) -> Result<bool> {
    let root = workspace_root()?;
    let artifact = build_hyalo(&root, args, true)?;
    let bin = artifact.executable;
    println!(
        "Cargo artifact={} host={} target={} EXE_SUFFIX={:?}",
        bin.display(),
        artifact.host,
        artifact.requested_target.as_deref().unwrap_or("native"),
        std::env::consts::EXE_SUFFIX
    );

    println!("Generating {FILE_COUNT}-file synthetic vault...");
    let vault = tempfile::tempdir().context("creating scratch dir for synthetic vault")?;
    let stats = generate_vault(vault.path(), FILE_COUNT)?;
    println!(
        "fixture files={} valid_edges={} broken_edges={} source_bytes={}",
        stats.files, stats.valid_edges, stats.broken_edges, stats.source_bytes
    );

    let metadata_args = [
        "find",
        "--property",
        "status=active",
        "--fields",
        "file",
        "--format",
        "json",
    ];
    let metadata = measure_samples(&bin, vault.path(), &metadata_args, REPEATS)?;
    for sample in &metadata {
        validate_metadata_counters("metadata-query/disk", sample.reads, true)?;
    }
    print_samples("metadata-query/disk", &metadata);

    checked_output(&bin, vault.path(), &["create-index", "--format", "json"])?;
    let indexed_args = [
        "find",
        "--index",
        "--property",
        "status=active",
        "--fields",
        "file",
        "--format",
        "json",
    ];
    let indexed = measure_samples(&bin, vault.path(), &indexed_args, REPEATS)?;
    for sample in &indexed {
        validate_metadata_counters("metadata-query/index", sample.reads, false)?;
    }
    print_samples("metadata-query/index", &indexed);

    println!("Timing `hyalo find` ({REPEATS} runs)...");
    let find_time = median_duration(&bin, vault.path(), &["find", "--format", "json"], REPEATS)?;
    println!("  median: {find_time:.2?} (budget: {FIND_BUDGET:.2?})");

    println!("Timing `hyalo links fix` ({REPEATS} runs)...");
    let links_time = median_duration(
        &bin,
        vault.path(),
        &["links", "fix", "--format", "json"],
        REPEATS,
    )?;
    println!("  median: {links_time:.2?} (budget: {LINKS_BUDGET:.2?})");

    println!("Timing `hyalo mv` across {FANOUT_BACKLINKS} backlinks ({REPEATS} runs)...");
    let mv_big = median_mv_fanout(&bin, FANOUT_BACKLINKS, REPEATS)?;
    println!("  median: {mv_big:.2?} (budget: {MV_FANOUT_BUDGET:.2?}) — parallel write phase");
    let mv_small = median_mv_fanout(&bin, FANOUT_SMALL, REPEATS)?;
    println!(
        "  {FANOUT_SMALL} backlinks: {mv_small:.2?} — durable per-file path, \
         {:.1} ms/file against {:.2} ms/file at {FANOUT_BACKLINKS}",
        mv_small.as_secs_f64() * 1000.0 / FANOUT_SMALL as f64,
        mv_big.as_secs_f64() * 1000.0 / FANOUT_BACKLINKS as f64,
    );

    let bulk = tempfile::tempdir().context("creating bulk-property scale vault")?;
    let bulk_stats = generate_vault(bulk.path(), FILE_COUNT)?;
    let selected = (0..FILE_COUNT).filter(|i| i % STATUSES.len() == 0).count();
    let preview = measure_once(
        &bin,
        bulk.path(),
        &[
            "set",
            "--glob",
            "note-*.md",
            "--where-property",
            "status=draft",
            "--property",
            "status=reviewed",
            "--dry-run",
            "--format",
            "json",
        ],
    )?;
    validate_bulk_counters("bulk-property/preview", preview.reads)?;
    println!(
        "bulk-property/preview files={} selected={} source_bytes={} logical_source_reads={} logical_body_reads={} index_entries_refreshed={} elapsed_ms={:.3} allocations={} allocated_bytes={} peak_live_bytes={} output_bytes={}",
        bulk_stats.files,
        selected,
        bulk_stats.source_bytes,
        preview.reads.logical_source_reads,
        preview.reads.logical_body_reads,
        preview.reads.index_entries_refreshed,
        preview.elapsed.as_secs_f64() * 1000.0,
        preview.allocations.allocations,
        preview.allocations.allocated_bytes,
        preview.allocations.peak_live_bytes,
        preview.output_bytes
    );
    let apply = measure_once(
        &bin,
        bulk.path(),
        &[
            "set",
            "--glob",
            "note-*.md",
            "--where-property",
            "status=draft",
            "--property",
            "status=reviewed",
            "--format",
            "json",
        ],
    )?;
    validate_bulk_counters("bulk-property/apply", apply.reads)?;
    println!(
        "bulk-property/apply files={} selected={} logical_source_reads={} logical_body_reads={} index_entries_refreshed={} elapsed_ms={:.3} allocations={} allocated_bytes={} peak_live_bytes={} output_bytes={}",
        FILE_COUNT,
        selected,
        apply.reads.logical_source_reads,
        apply.reads.logical_body_reads,
        apply.reads.index_entries_refreshed,
        apply.elapsed.as_secs_f64() * 1000.0,
        apply.allocations.allocations,
        apply.allocations.allocated_bytes,
        apply.allocations.peak_live_bytes,
        apply.output_bytes
    );

    let refresh = tempfile::tempdir().context("creating graph-refresh scale vault")?;
    let refresh_stats = generate_vault(refresh.path(), 2_000)?;
    checked_output(&bin, refresh.path(), &["create-index", "--format", "json"])?;
    let refresh_selected = 500;
    let refresh_sample = measure_once(
        &bin,
        refresh.path(),
        &[
            "set",
            "--glob",
            "note-*.md",
            "--where-property",
            "status=draft",
            "--property",
            "status=reviewed",
            "--index",
            "--format",
            "json",
        ],
    )?;
    validate_graph_refresh_counters(refresh_sample.reads, refresh_selected)?;
    println!(
        "graph-refresh files={} valid_edges={} broken_edges={} index_entries_refreshed={} logical_source_reads={} logical_body_reads={} elapsed_ms={:.3} allocations={} allocated_bytes={} peak_live_bytes={} index_bytes={}",
        refresh_stats.files,
        refresh_stats.valid_edges,
        refresh_stats.broken_edges,
        refresh_sample.reads.index_entries_refreshed,
        refresh_sample.reads.logical_source_reads,
        refresh_sample.reads.logical_body_reads,
        refresh_sample.elapsed.as_secs_f64() * 1000.0,
        refresh_sample.allocations.allocations,
        refresh_sample.allocations.allocated_bytes,
        refresh_sample.allocations.peak_live_bytes,
        std::fs::metadata(refresh.path().join(".hyalo-index"))?.len()
    );
    run_counter_control(&bin)?;
    println!(
        "noise-controls repeats={REPEATS} statistic=median process=fresh-per-sample fixture=deterministic generation-and-allocation-probe-copy=outside-timing filesystem-cache=first-sample-coldish/subsequent-warm allocator=separate-identical-vault-parent-process-request-counter/no-child-workers read-counters=timed-parent-process-successful-shared-scanner-operations/report-write-included logical_body_reads=body-events-requested/excludes-line-count-tail index_entries_refreshed=successful-in-memory-replacement-or-insertion/excludes-initial-build filesystem-syscalls=not-measured"
    );

    let mut ok = true;
    if mv_big > MV_FANOUT_BUDGET {
        println!(
            "FAIL: `hyalo mv` over {FANOUT_BACKLINKS} backlinks took {mv_big:.2?}, budget is {MV_FANOUT_BUDGET:.2?}"
        );
        ok = false;
    }
    if find_time > FIND_BUDGET {
        println!("FAIL: `hyalo find` took {find_time:.2?}, budget is {FIND_BUDGET:.2?}");
        ok = false;
    }
    if links_time > LINKS_BUDGET {
        println!("FAIL: `hyalo links fix` took {links_time:.2?}, budget is {LINKS_BUDGET:.2?}");
        ok = false;
    }
    if ok {
        println!(
            "PASS: find/links stayed within budget on a {FILE_COUNT}-file vault, \
             and `mv` within budget at {FANOUT_BACKLINKS} backlinks."
        );
    }
    Ok(ok)
}

/// Time `hyalo mv` on a note that `backlinks` other notes link to, `repeats`
/// times over a freshly generated vault each time, and return the median
/// (iter-278, ALLOC-4).
///
/// A fresh vault per repetition is the point: `mv` is a mutation, so a second
/// run over the same tree would rename a note nothing links to any more and
/// measure nothing. Generation happens outside the timed region.
fn median_mv_fanout(bin: &Path, backlinks: usize, repeats: usize) -> Result<Duration> {
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let vault = tempfile::tempdir().context("creating scratch dir for fan-out vault")?;
        generate_fanout_vault(vault.path(), backlinks)?;
        let start = Instant::now();
        let output = Command::new(bin)
            .arg("--dir")
            .arg(vault.path())
            .arg("--no-hints")
            // Single-file `mv` applies by default (`--apply` is a batch-mode
            // flag and is refused here), so this writes for real.
            .args(["mv", "hub.md", "hub-renamed.md", "--format", "json"])
            .output()
            .context("running hyalo mv")?;
        let elapsed = start.elapsed();
        if !output.status.success() {
            bail!(
                "hyalo mv exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        samples.push(elapsed);
    }
    samples.sort();
    Ok(samples[samples.len() / 2])
}

/// Build a vault of `backlinks` notes that all link to a single `hub.md`.
///
/// Every linking note carries exactly one `[[hub]]`, so renaming `hub.md`
/// rewrites every one of them — one write phase of `backlinks` files, which is
/// the fan-out DEC-317's parallel path was built for and the Obsidian Hub copy
/// never reached.
fn generate_fanout_vault(dir: &Path, backlinks: usize) -> Result<()> {
    let hub = dir.join("hub.md");
    std::fs::write(&hub, "---\ntitle: Hub\n---\n\nThe hub note.\n")
        .with_context(|| format!("writing {}", hub.display()))?;
    for i in 0..backlinks {
        let path = dir.join(format!("linker-{i:05}.md"));
        let content = format!(
            "---\ntitle: Linker {i}\n---\n\nNote {i} points at [[hub]] and nothing else.\n"
        );
        std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(())
}

/// Run `hyalo <args>` against `vault` `repeats` times and return the median
/// wall-clock duration. Fails loudly (rather than silently timing a broken
/// invocation) if any run exits non-zero.
fn median_duration(bin: &Path, vault: &Path, args: &[&str], repeats: usize) -> Result<Duration> {
    let mut samples = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        let output = Command::new(bin)
            .arg("--dir")
            .arg(vault)
            .arg("--no-hints")
            .args(args)
            .output()
            .with_context(|| format!("running hyalo {args:?}"))?;
        let elapsed = start.elapsed();
        if !output.status.success() {
            bail!(
                "hyalo {args:?} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        samples.push(elapsed);
    }
    samples.sort();
    Ok(samples[samples.len() / 2])
}

fn measure_samples(
    bin: &Path,
    vault: &Path,
    args: &[&str],
    repeats: usize,
) -> Result<Vec<TimedSample>> {
    (0..repeats)
        .map(|_| measure_once(bin, vault, args))
        .collect()
}

fn measure_once(bin: &Path, vault: &Path, args: &[&str]) -> Result<TimedSample> {
    let probe = tempfile::tempdir().context("creating allocation-probe vault copy")?;
    copy_tree(vault, probe.path())?;
    let reports = tempfile::tempdir().context("creating measured-read-report directory")?;
    let report = reports.path().join("reads.json");
    let start = Instant::now();
    let output = Command::new(bin)
        .arg("--dir")
        .arg(vault)
        .arg("--no-hints")
        .args(args)
        .env("HYALO_INTERNAL_METRICS_REPORT", &report)
        .output()
        .with_context(|| format!("running measured hyalo {args:?}"))?;
    let elapsed = start.elapsed();
    ensure_success(args, &output)?;
    let report = read_process_metrics(&report)?;
    let allocations = allocation_probe(bin, probe.path(), args)?;
    Ok(TimedSample {
        elapsed,
        output_bytes: output.stdout.len() + output.stderr.len(),
        allocations,
        reads: ReadStats {
            logical_source_reads: report.logical_source_reads,
            logical_body_reads: report.logical_body_reads,
            index_entries_refreshed: report.index_entries_refreshed,
        },
    })
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.context("walking scale fixture for allocation probe")?;
        let relative = entry.path().strip_prefix(source)?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn allocation_probe(bin: &Path, vault: &Path, args: &[&str]) -> Result<AllocationStats> {
    let reports = tempfile::tempdir().context("creating allocation-report directory")?;
    let report = reports.path().join("alloc.json");
    let output = Command::new(bin)
        .arg("--dir")
        .arg(vault)
        .arg("--no-hints")
        .args(args)
        .env("HYALO_INTERNAL_ALLOC_REPORT", &report)
        .output()
        .with_context(|| format!("running allocation probe for hyalo {args:?}"))?;
    ensure_success(args, &output)?;
    let report = read_process_metrics(&report)?;
    Ok(AllocationStats {
        allocations: report
            .allocations
            .context("allocation probe report omitted allocations")?,
        allocated_bytes: report
            .allocated_bytes
            .context("allocation probe report omitted allocated_bytes")?,
        peak_live_bytes: report
            .peak_live_bytes
            .context("allocation probe report omitted peak_live_bytes")?,
    })
}

fn read_metrics_probe(bin: &Path, vault: &Path, args: &[&str]) -> Result<ReadStats> {
    let reports = tempfile::tempdir().context("creating read-counter report directory")?;
    let report = reports.path().join("reads.json");
    let output = Command::new(bin)
        .arg("--dir")
        .arg(vault)
        .arg("--no-hints")
        .args(args)
        .env("HYALO_INTERNAL_METRICS_REPORT", &report)
        .output()
        .with_context(|| format!("running read-counter control for hyalo {args:?}"))?;
    ensure_success(args, &output)?;
    let report = read_process_metrics(&report)?;
    Ok(ReadStats {
        logical_source_reads: report.logical_source_reads,
        logical_body_reads: report.logical_body_reads,
        index_entries_refreshed: report.index_entries_refreshed,
    })
}

fn read_process_metrics(path: &Path) -> Result<ProcessMetricsReport> {
    serde_json::from_slice(&std::fs::read(path).with_context(|| {
        format!(
            "reading internal metrics report {} (the measured binary may be stale)",
            path.display()
        )
    })?)
    .context("parsing internal metrics report")
}

fn run_counter_control(bin: &Path) -> Result<()> {
    let vault = tempfile::tempdir().context("creating read-counter control vault")?;
    for name in ["counter-a.md", "counter-b.md"] {
        std::fs::write(
            vault.path().join(name),
            format!("---\nstatus: active\n---\n\ncounter-control-needle in {name}\n"),
        )?;
    }
    let base = read_metrics_probe(
        bin,
        vault.path(),
        &[
            "find",
            "--property",
            "status=active",
            "--fields",
            "file",
            "--file",
            "counter-a.md",
            "--format",
            "json",
        ],
    )?;
    let additional_source = read_metrics_probe(
        bin,
        vault.path(),
        &[
            "find",
            "--property",
            "status=active",
            "--fields",
            "file",
            "--file",
            "counter-a.md",
            "--file",
            "counter-b.md",
            "--format",
            "json",
        ],
    )?;
    let body = read_metrics_probe(
        bin,
        vault.path(),
        &[
            "find",
            "counter-control-needle",
            "--file",
            "counter-a.md",
            "--format",
            "json",
        ],
    )?;
    validate_counter_control(base, additional_source, body)?;
    println!(
        "counter-control PASS base_source_reads={} additional_source_reads={} base_body_reads={} body_search_reads={} body_search_body_reads={} index_entries_refreshed={}",
        base.logical_source_reads,
        additional_source.logical_source_reads,
        base.logical_body_reads,
        body.logical_source_reads,
        body.logical_body_reads,
        body.index_entries_refreshed,
    );
    Ok(())
}

fn validate_counter_control(base: ReadStats, additional: ReadStats, body: ReadStats) -> Result<()> {
    if base.logical_source_reads == 0
        || additional.logical_source_reads != base.logical_source_reads + 1
        || base.logical_body_reads != 0
        || body.logical_source_reads != base.logical_source_reads
        || body.logical_body_reads != body.logical_source_reads
        || base.index_entries_refreshed != 0
        || additional.index_entries_refreshed != 0
        || body.index_entries_refreshed != 0
    {
        bail!(
            "read-counter control did not observe the expected one-source and body-read changes: base={base:?} additional={additional:?} body={body:?}"
        );
    }
    Ok(())
}

fn validate_metadata_counters(label: &str, reads: ReadStats, disk: bool) -> Result<()> {
    let expected_sources = if disk { FILE_COUNT as u64 } else { 0 };
    if reads.logical_source_reads != expected_sources
        || reads.logical_body_reads != 0
        || reads.index_entries_refreshed != 0
    {
        bail!("{label} reported unexpected logical counters: {reads:?}");
    }
    Ok(())
}

fn validate_bulk_counters(label: &str, reads: ReadStats) -> Result<()> {
    if reads.logical_source_reads == 0
        || reads.logical_body_reads == 0
        || reads.logical_body_reads > reads.logical_source_reads
        || reads.index_entries_refreshed != 0
    {
        bail!("{label} did not report observed bulk source/body reads: {reads:?}");
    }
    Ok(())
}

fn validate_graph_refresh_counters(reads: ReadStats, expected_refreshes: usize) -> Result<()> {
    if reads.logical_source_reads == 0
        || reads.logical_body_reads == 0
        || reads.logical_body_reads > reads.logical_source_reads
        || reads.index_entries_refreshed != expected_refreshes as u64
    {
        bail!(
            "graph-refresh did not report observed source/body reads and {expected_refreshes} applied index refreshes: {reads:?}"
        );
    }
    Ok(())
}

fn checked_output(bin: &Path, vault: &Path, args: &[&str]) -> Result<Output> {
    let output = Command::new(bin)
        .arg("--dir")
        .arg(vault)
        .arg("--no-hints")
        .args(args)
        .output()
        .with_context(|| format!("running hyalo {args:?}"))?;
    ensure_success(args, &output)?;
    Ok(output)
}

fn ensure_success(args: &[&str], output: &Output) -> Result<()> {
    if !output.status.success() {
        bail!(
            "hyalo {args:?} exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn print_samples(label: &str, samples: &[TimedSample]) {
    let mut times: Vec<_> = samples.iter().map(|sample| sample.elapsed).collect();
    times.sort();
    let median = times[times.len() / 2];
    for (index, sample) in samples.iter().enumerate() {
        println!(
            "{label} sample={} state={} elapsed_ms={:.3} logical_source_reads={} logical_body_reads={} index_entries_refreshed={} allocations={} allocated_bytes={} peak_live_bytes={} output_bytes={}",
            index + 1,
            if index == 0 {
                "cold-process"
            } else {
                "warm-filesystem-cache"
            },
            sample.elapsed.as_secs_f64() * 1000.0,
            sample.reads.logical_source_reads,
            sample.reads.logical_body_reads,
            sample.reads.index_entries_refreshed,
            sample.allocations.allocations,
            sample.allocations.allocated_bytes,
            sample.allocations.peak_live_bytes,
            sample.output_bytes
        );
    }
    println!("{label} median_ms={:.3}", median.as_secs_f64() * 1000.0);
}

/// Build a deterministic synthetic vault of `count` files under `dir`.
///
/// Deterministic (no external RNG/seed state needed): every property is a
/// pure function of the file's index, so re-running this always produces
/// byte-identical content, which keeps the gate's timing reproducible run to
/// run modulo actual code changes.
fn generate_vault(dir: &Path, count: usize) -> Result<VaultStats> {
    let mut source_bytes = 0u64;
    let mut broken_edges = 0usize;
    for i in 0..count {
        let tag_a = TAGS[i % TAGS.len()];
        let tag_b = TAGS[(i / TAGS.len()) % TAGS.len()];
        let status = STATUSES[i % STATUSES.len()];

        // Link to a handful of neighbours by index: two that exist, one that
        // (for roughly 1 in 20 files) intentionally doesn't, so `links`
        // has real broken-link and resolution work to do, not just parsing.
        let target_a = (i + 1) % count;
        let target_b = (i + 7) % count;
        let broken = if i % 20 == 0 {
            broken_edges += 1;
            "\n- see [[note-does-not-exist]]".to_owned()
        } else {
            String::new()
        };

        let content = format!(
            "---\ntitle: Note {i}\nstatus: {status}\ntags: [{tag_a}, {tag_b}]\n---\n\n\
             Body text for note {i}. Links to [[note-{target_a:05}]] and \
             [[note-{target_b:05}]].{broken}\n"
        );

        let path = dir.join(format!("note-{i:05}.md"));
        let mut f =
            std::fs::File::create(&path).with_context(|| format!("creating {}", path.display()))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("writing {}", path.display()))?;
        source_bytes += content.len() as u64;
    }
    Ok(VaultStats {
        files: count,
        valid_edges: count * 2,
        broken_edges,
        source_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_fixture_derived_read_counts_are_rejected() {
        let fixed = ReadStats {
            logical_source_reads: 1,
            logical_body_reads: 0,
            index_entries_refreshed: 0,
        };
        let error = validate_counter_control(fixed, fixed, fixed).unwrap_err();
        assert!(error.to_string().contains("did not observe"));
    }

    #[test]
    fn missing_bulk_read_observer_is_rejected() {
        let unobserved = ReadStats {
            logical_source_reads: 0,
            logical_body_reads: 0,
            index_entries_refreshed: 0,
        };
        let error = validate_bulk_counters("bulk-property/preview", unobserved).unwrap_err();
        assert!(error.to_string().contains("did not report observed"));
    }
}
