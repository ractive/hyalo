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
/// 14k files — see DEC-098). CI runners vary a lot in single-core speed, and
/// `links fix` in particular does real cross-file resolution work that scales
/// with corpus size, so these budgets sit at roughly 4-8x the observed
/// baseline: enough headroom to absorb a slow shared runner without masking
/// a genuine order-of-magnitude regression, which is the only class of bug
/// this gate exists to catch.
const FIND_BUDGET: Duration = Duration::from_secs(3);
const LINKS_BUDGET: Duration = Duration::from_secs(15);

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

    let mut ok = true;
    if find_time > FIND_BUDGET {
        println!("FAIL: `hyalo find` took {find_time:.2?}, budget is {FIND_BUDGET:.2?}");
        ok = false;
    }
    if links_time > LINKS_BUDGET {
        println!("FAIL: `hyalo links fix` took {links_time:.2?}, budget is {LINKS_BUDGET:.2?}");
        ok = false;
    }
    if ok {
        println!("PASS: both commands stayed within budget on a {FILE_COUNT}-file vault.");
    }
    Ok(ok)
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
