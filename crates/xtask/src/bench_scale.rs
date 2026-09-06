//! T-6 (iter-224): scale regression gate.
//!
//! Generates a deterministic ~14k-file synthetic vault and times `hyalo
//! find` / `hyalo links fix` against it, failing if either exceeds a
//! generous wall-time budget. This is an **on-demand** gate
//! (`cargo run -p xtask -- bench-scale`), not a per-PR CI check — see
//! `decision-log.md` DEC-098 for why, and for the budget numbers below.
//!
//! What this covers: gross wall-clock regressions (an accidental O(n²) path,
//! a dropped index fast-path) on a vault large enough that per-file overhead
//! actually shows up. What it does **not** cover: the fuzzy-candidate
//! matching perf debt tracked separately
//! ([[iterations/iteration-206-links-perf-profiling]]), sub-command timing
//! breakdowns, or memory usage — `bench-e2e.sh` (hyperfine-based, needs an
//! external vault) remains the tool for detailed A/B comparisons.

use anyhow::{Context, Result, bail};
use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

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

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let bin = locate_release_binary(&root)?;

    println!("Generating {FILE_COUNT}-file synthetic vault...");
    let vault = tempfile::tempdir().context("creating scratch dir for synthetic vault")?;
    generate_vault(vault.path(), FILE_COUNT)?;

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

/// Find `target/release/hyalo`, building it if it doesn't exist yet — mirrors
/// `bench-e2e.sh`'s own "build if missing" convenience.
fn locate_release_binary(root: &Path) -> Result<std::path::PathBuf> {
    let bin = root.join("target/release/hyalo");
    if bin.is_file() {
        return Ok(bin);
    }
    println!("target/release/hyalo not found; building it (cargo build --release)...");
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "hyalo-cli"])
        .current_dir(root)
        .status()
        .context("spawning cargo build --release")?;
    if !status.success() {
        bail!("cargo build --release failed; cannot run the scale gate");
    }
    if !bin.is_file() {
        bail!(
            "cargo build --release succeeded but {} is missing",
            bin.display()
        );
    }
    Ok(bin)
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

/// Build a deterministic synthetic vault of `count` files under `dir`.
///
/// Deterministic (no external RNG/seed state needed): every property is a
/// pure function of the file's index, so re-running this always produces
/// byte-identical content, which keeps the gate's timing reproducible run to
/// run modulo actual code changes.
fn generate_vault(dir: &Path, count: usize) -> Result<()> {
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
    }
    Ok(())
}
