//! Lightweight warning system for hyalo CLI.
//!
//! Provides:
//! - Quiet mode: suppress all warnings when `-q` / `--quiet` is set.
//! - Dedup tracking: identical warning messages are counted but only
//!   printed once; `flush_summary()` reports how many were suppressed.
//!
//! # Initialisation
//!
//! Call `init(quiet)` as early as possible after CLI flags are parsed.
//! Until `init` is called the system defaults to non-quiet mode, so any
//! warnings emitted before initialisation (e.g. from config loading) are
//! still printed.
//!
//! # Usage
//!
//! ```ignore
//! warn::init(cli.quiet);
//! // ...
//! warn::warn("skipping foo.md: invalid frontmatter");
//! // ...
//! warn::flush_summary();
//! ```

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static QUIET: AtomicBool = AtomicBool::new(false);

/// Per-message suppression counters.
/// Key: warning message string.
/// Value: number of times the message was suppressed (i.e. seen after the first).
static SUPPRESSED: Mutex<Option<HashMap<String, usize>>> = Mutex::new(None);

/// Initialise the warning system.
///
/// Must be called once, early in `main`, after CLI flags are parsed.
/// Calling it more than once is safe but redundant.
pub fn init(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
    // Initialise the dedup map (replacing None with an empty map).
    if let Ok(mut guard) = SUPPRESSED.lock()
        && guard.is_none()
    {
        *guard = Some(HashMap::new());
    }
}

/// Emit a warning message to stderr.
///
/// - If quiet mode is active the message is silently discarded.
/// - If the identical message has already been printed once, it is counted
///   as suppressed rather than re-printed.
/// - If `init` has not been called yet the message is always printed and
///   dedup tracking is skipped (the dedup map is not yet initialised).
pub fn warn(msg: impl AsRef<str>) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }

    let msg = msg.as_ref();

    // Try dedup tracking.
    if let Ok(mut guard) = SUPPRESSED.lock()
        && let Some(ref mut map) = *guard
    {
        let count = map.entry(msg.to_owned()).or_insert(0);
        if *count > 0 {
            // Already printed once — suppress and increment counter.
            *count += 1;
            return;
        }
        // First occurrence: mark as seen (count = 1) and fall through to print.
        *count = 1;
        // guard.is_none() means init() hasn't been called yet — fall through to print.
    }

    eprintln!("warning: {msg}");
}

/// Reset the warning system to its initial state.
///
/// **For use in tests only.**  Clears the dedup map and resets the quiet flag
/// so that each test starts from a clean slate.  This is necessary because the
/// static globals persist across tests within the same process.
#[cfg(test)]
pub fn reset_for_test() {
    QUIET.store(false, Ordering::Relaxed);
    if let Ok(mut guard) = SUPPRESSED.lock() {
        *guard = None;
    }
}

/// Return the number of times the given message was suppressed (seen after the
/// first print).
///
/// **For use in tests only.**
#[cfg(test)]
pub fn suppressed_count_for(msg: &str) -> usize {
    SUPPRESSED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|m| m.get(msg).copied()))
        .map(|seen| seen.saturating_sub(1))
        .unwrap_or(0)
}

/// Return the total number of suppressed (duplicate) warning occurrences.
///
/// **For use in tests only.**
#[cfg(test)]
pub fn total_suppressed() -> usize {
    SUPPRESSED
        .lock()
        .ok()
        .and_then(|g| {
            g.as_ref().map(|m| {
                m.values()
                    .map(|&seen| seen.saturating_sub(1))
                    .sum::<usize>()
            })
        })
        .unwrap_or(0)
}

/// Print a summary of suppressed duplicate warnings, if any.
///
/// Should be called just before the process exits. Prints to stderr.
/// If no warnings were suppressed (or `init` was never called) this is a no-op.
pub fn flush_summary() {
    let total_suppressed: usize = match SUPPRESSED.lock() {
        Ok(guard) => guard
            .as_ref()
            .map(|map| {
                map.values()
                    // Each entry counts: first print is not suppressed, so suppress count
                    // is (total_seen - 1). We stored total_seen in the map entry.
                    .map(|&seen| seen.saturating_sub(1))
                    .sum()
            })
            .unwrap_or(0),
        Err(_) => return,
    };

    if total_suppressed > 0 {
        eprintln!("warning: {total_suppressed} additional identical warning(s) suppressed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // Unit tests use a coarse test-level mutex to serialise execution.
    // The global SUPPRESSED and QUIET statics are shared across all tests in the
    // same process, so parallel execution would cause interference.
    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    #[test]
    fn dedup_first_occurrence_not_suppressed() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        warn("msg-dedup-first");
        assert_eq!(suppressed_count_for("msg-dedup-first"), 0);
    }

    #[test]
    fn dedup_second_occurrence_counted() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        warn("msg-dedup-second");
        warn("msg-dedup-second");
        assert_eq!(suppressed_count_for("msg-dedup-second"), 1);
    }

    #[test]
    fn dedup_many_occurrences_all_counted() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        for _ in 0..5 {
            warn("msg-dedup-many");
        }
        // First one is printed; remaining 4 are suppressed.
        assert_eq!(suppressed_count_for("msg-dedup-many"), 4);
        assert_eq!(total_suppressed(), 4);
    }

    #[test]
    fn quiet_mode_suppresses_all() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(true);
        warn("msg-quiet-a");
        warn("msg-quiet-a");
        // In quiet mode nothing is tracked or printed.
        assert_eq!(suppressed_count_for("msg-quiet-a"), 0);
    }

    #[test]
    fn different_messages_not_deduped() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        warn("msg-diff-a");
        warn("msg-diff-b");
        assert_eq!(suppressed_count_for("msg-diff-a"), 0);
        assert_eq!(suppressed_count_for("msg-diff-b"), 0);
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn total_suppressed_across_multiple_messages() {
        let _guard = TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        // "x" fires 3 times → 2 suppressed; "y" fires 2 times → 1 suppressed
        for _ in 0..3 {
            warn("msg-total-x");
        }
        for _ in 0..2 {
            warn("msg-total-y");
        }
        assert_eq!(total_suppressed(), 3);
    }
}
