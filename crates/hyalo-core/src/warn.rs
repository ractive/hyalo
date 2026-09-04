//! Lightweight warning helper for `hyalo-core`.
//!
//! Core-level code should call [`warn`] instead of `eprintln!` so that
//! the message is formatted consistently. The CLI layer (`hyalo-cli`)
//! provides its own richer warning system with quiet-mode suppression and
//! dedup tracking; this module is intentionally minimal — it just writes to
//! stderr with a standard `warning:` prefix.
//!
//! It also owns the process-wide **skipped-file collector** (iter-265,
//! DEC-278). A vault full of Templater templates used to make every scanning
//! command print one multi-line `serde_yaml` excerpt per unparsable file — 251
//! stderr lines for a 28-template vault, on `summary`, `find`, `tags`,
//! `properties`, `lint` and `mv` alike. Those diagnostics are now *collected*
//! here and collapsed by the CLI into one summary line at the end of the run;
//! the full excerpts are still available under `RUST_LOG=hyalo=debug` or
//! `[scan] verbose_skips = true`.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Emit a warning message to stderr.
///
/// Formats the message with a `warning: ` prefix, matching the convention used
/// by the CLI layer.
pub fn warn(msg: impl AsRef<str>) {
    eprintln!("warning: {}", msg.as_ref());
}

/// A vault file a scan could not use, with the reason it was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedFile {
    /// Vault-relative path of the file that was skipped.
    pub path: String,
    /// Human-readable reason — typically the multi-line YAML parse diagnostic.
    pub reason: String,
    /// What kind of problem this was, deciding which summary line counts it.
    pub kind: SkipKind,
}

/// Why a file was dropped from a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipKind {
    /// The YAML frontmatter would not parse (or was structurally invalid).
    /// Reported by `hyalo lint --rule HYALO005`.
    Frontmatter,
    /// Anything else: unreadable bytes, invalid UTF-8, an oversized file.
    Other,
}

/// Files dropped during this process's scans, in the order they were seen.
///
/// Process-global and never cleared outside tests: one `hyalo` process is one
/// CLI run. A poisoned lock degrades to "not collected" rather than aborting a
/// walk.
static SKIPPED: Mutex<Vec<SkippedFile>> = Mutex::new(Vec::new());

/// Whether per-file skip diagnostics are streamed to stderr as they happen.
static VERBOSE_SKIPS: AtomicBool = AtomicBool::new(false);

/// Set of paths already recorded, so a command that walks the vault more than
/// once in a run does not count the same file twice.
static RECORDED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Turn per-file skip diagnostics back on (`[scan] verbose_skips = true`, or
/// `RUST_LOG` asking for hyalo debug output).
///
/// The CLI calls this once at startup, before any command runs.
pub fn set_verbose_skips(verbose: bool) {
    VERBOSE_SKIPS.store(verbose, Ordering::Relaxed);
}

/// Whether per-file skip diagnostics are being streamed.
#[must_use]
pub fn verbose_skips() -> bool {
    VERBOSE_SKIPS.load(Ordering::Relaxed)
}

/// Whether `RUST_LOG` asks for hyalo debug/trace output.
///
/// Accepts the shapes an operator actually types: `hyalo=debug`, `debug`,
/// `trace`, `hyalo_core=trace`, and any comma-separated list containing one of
/// them. Anything else (`info`, `warn`, unset) leaves skips collapsed.
#[must_use]
pub fn rust_log_wants_debug() -> bool {
    let Ok(value) = std::env::var("RUST_LOG") else {
        return false;
    };
    value.split(',').any(|part| {
        let level = part.rsplit('=').next().unwrap_or("").trim();
        level.eq_ignore_ascii_case("debug") || level.eq_ignore_ascii_case("trace")
    })
}

/// Record a file that a scan could not use.
///
/// In verbose mode the full diagnostic is printed immediately, exactly as it
/// was before iter-265. Otherwise it is collected for the end-of-run summary.
/// Repeat records for the same path are ignored, so a command that walks the
/// vault twice still reports one skip per file.
pub fn record_skip(path: impl Into<String>, reason: impl Into<String>, kind: SkipKind) {
    let path = path.into();
    let reason = reason.into();
    if let Ok(mut seen) = RECORDED.lock() {
        if seen.iter().any(|p| p == &path) {
            return;
        }
        seen.push(path.clone());
    }
    if verbose_skips() {
        eprintln!("warning: skipping {path}: {reason}");
    }
    if let Ok(mut skipped) = SKIPPED.lock() {
        skipped.push(SkippedFile { path, reason, kind });
    }
}

/// Every file recorded by [`record_skip`] so far, in first-seen order.
#[must_use]
pub fn skipped_files() -> Vec<SkippedFile> {
    SKIPPED.lock().map(|s| s.clone()).unwrap_or_default()
}

/// How many files were skipped for unparsable frontmatter.
#[must_use]
pub fn skipped_frontmatter_count() -> usize {
    SKIPPED.lock().map_or(0, |s| {
        s.iter().filter(|f| f.kind == SkipKind::Frontmatter).count()
    })
}

/// How many files were skipped for any reason.
#[must_use]
pub fn skipped_count() -> usize {
    SKIPPED.lock().map_or(0, |s| s.len())
}

/// Clear the collector. **Tests only** — the statics outlive a single test.
pub fn reset_skips_for_test() {
    if let Ok(mut s) = SKIPPED.lock() {
        s.clear();
    }
    if let Ok(mut s) = RECORDED.lock() {
        s.clear();
    }
    VERBOSE_SKIPS.store(false, Ordering::Relaxed);
}

/// Serialises tests that touch the process-global skip collector.
pub static SKIP_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_counts_by_kind() {
        let _guard = SKIP_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_skips_for_test();
        record_skip("a.md", "bad yaml", SkipKind::Frontmatter);
        record_skip("b.md", "invalid utf-8", SkipKind::Other);
        assert_eq!(skipped_count(), 2);
        assert_eq!(skipped_frontmatter_count(), 1);
        reset_skips_for_test();
    }

    #[test]
    fn repeat_path_recorded_once() {
        let _guard = SKIP_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        reset_skips_for_test();
        record_skip("dup.md", "bad yaml", SkipKind::Frontmatter);
        record_skip("dup.md", "bad yaml", SkipKind::Frontmatter);
        assert_eq!(skipped_count(), 1);
        reset_skips_for_test();
    }

    #[test]
    fn rust_log_debug_shapes() {
        // Not using the env var itself (tests share a process); exercise the
        // parsing through a local re-implementation of the same predicate.
        let wants = |value: &str| {
            value.split(',').any(|part| {
                let level = part.rsplit('=').next().unwrap_or("").trim();
                level.eq_ignore_ascii_case("debug") || level.eq_ignore_ascii_case("trace")
            })
        };
        assert!(wants("hyalo=debug"));
        assert!(wants("debug"));
        assert!(wants("info,hyalo_core=trace"));
        assert!(!wants("info"));
        assert!(!wants(""));
    }
}
