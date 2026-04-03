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
/// Value: number of times the message was suppressed (i.e. seen *after* the first print).
///        Inserted as 0 on the first occurrence; incremented on each subsequent one.
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
        if let Some(count) = map.get_mut(msg) {
            // Already printed once — suppress and increment counter.
            *count += 1;
            return;
        }
        // First occurrence: insert with suppression count 0, fall through to print.
        map.insert(msg.to_owned(), 0);
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
        .unwrap_or(0)
}

/// Return whether the given message was tracked at least once after `init()`.
///
/// Note: warnings emitted before `init()` are not tracked.
/// **For use in tests only.**
#[cfg(test)]
pub fn was_emitted(msg: &str) -> bool {
    SUPPRESSED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| m.contains_key(msg)))
        .unwrap_or(false)
}

/// Return the total number of suppressed (duplicate) warning occurrences.
///
/// **For use in tests only.**
#[cfg(test)]
pub fn total_suppressed() -> usize {
    SUPPRESSED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| m.values().sum::<usize>()))
        .unwrap_or(0)
}

/// Emit a "did you mean…" warning when globs matched zero files and the glob
/// pattern appears to redundantly include the configured `--dir` path.
///
/// Example: if `dir` is `files/en-us` and a glob is `files/en-us/web/css/**`,
/// the user probably meant `web/css/**` (since globs are relative to `--dir`).
///
/// Only warns when `matched_count` is 0, at least one glob is provided, and
/// at least one glob starts with a path component matching the dir or its last
/// segment.
pub fn warn_glob_dir_overlap(dir: &std::path::Path, globs: &[String], matched_count: usize) {
    if matched_count > 0 || globs.is_empty() {
        return;
    }

    // Normalise dir to forward slashes, strip leading "./" and trailing "/"
    // so comparisons work on Windows and with `--dir ./docs/` style inputs.
    let dir_str = dir.to_string_lossy().replace('\\', "/");
    let dir_str = dir_str
        .strip_prefix("./")
        .unwrap_or(&dir_str)
        .trim_end_matches('/');

    // Only consider non-trivial dir values (not ".")
    if dir_str == "." || dir_str.is_empty() {
        return;
    }

    for glob in globs {
        // Skip negation patterns
        if glob.starts_with('!') {
            continue;
        }

        // Normalise glob the same way for consistent comparison
        let glob_norm = glob.replace('\\', "/");

        // Check if the glob starts with the full dir path followed by a '/'
        // (e.g. "files/en-us/web/css/**" when dir is "files/en-us").
        // Require a '/' boundary to avoid matching partial prefixes like
        // "files/en-us-old/**".
        let full_prefix = format!("{dir_str}/");
        if let Some(rest) = glob_norm.strip_prefix(full_prefix.as_str())
            && !rest.is_empty()
        {
            warn(format!(
                "glob '{glob}' matched 0 files. Globs are relative to --dir '{dir_str}'. \
                 Did you mean '{rest}'?"
            ));
            return;
        }

        // Also check if the glob starts with the last path component of dir
        // followed by a '/' (e.g. "en-us/web/**" when dir is "files/en-us").
        // Again require the '/' boundary to avoid "notes" matching "notes-archive/**".
        if let Some(last_component) = dir.file_name().and_then(|n| n.to_str()) {
            let component_prefix = format!("{last_component}/");
            if let Some(rest) = glob_norm.strip_prefix(component_prefix.as_str())
                && !rest.is_empty()
            {
                warn(format!(
                    "glob '{glob}' matched 0 files. Globs are relative to --dir '{dir_str}'. \
                     Did you mean '{rest}'?"
                ));
                return;
            }
        }
    }
}

/// Print a summary of suppressed duplicate warnings, if any.
///
/// Should be called just before the process exits. Prints to stderr.
/// If no warnings were suppressed (or `init` was never called) this is a no-op.
pub fn flush_summary() {
    let total_suppressed: usize = match SUPPRESSED.lock() {
        Ok(guard) => guard.as_ref().map_or(0, |map| map.values().sum()),
        Err(_) => return,
    };

    if !QUIET.load(Ordering::Relaxed) && total_suppressed > 0 {
        eprintln!("warning: {total_suppressed} additional identical warning(s) suppressed");
    }
}

/// Test-level mutex to serialise tests that touch the global warn state.
///
/// The global `SUPPRESSED` and `QUIET` statics are shared across all tests in
/// the same process, so parallel execution would cause interference. Any test
/// module that calls `reset_for_test()` / `init()` / `was_emitted()` must
/// acquire this lock first.
#[cfg(test)]
pub(crate) static WARN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_first_occurrence_not_suppressed() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        warn("msg-dedup-first");
        assert_eq!(suppressed_count_for("msg-dedup-first"), 0);
    }

    #[test]
    fn dedup_second_occurrence_counted() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        warn("msg-dedup-second");
        warn("msg-dedup-second");
        assert_eq!(suppressed_count_for("msg-dedup-second"), 1);
    }

    #[test]
    fn dedup_many_occurrences_all_counted() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
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
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(true);
        warn("msg-quiet-a");
        warn("msg-quiet-a");
        // In quiet mode nothing is tracked or printed.
        assert_eq!(suppressed_count_for("msg-quiet-a"), 0);
    }

    #[test]
    fn different_messages_not_deduped() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
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
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
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

    // -----------------------------------------------------------------------
    // warn_glob_dir_overlap tests
    // -----------------------------------------------------------------------

    #[test]
    fn glob_overlap_no_warning_when_results_found() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("files/en-us");
        warn_glob_dir_overlap(dir, &["files/en-us/web/**".to_owned()], 5);
        assert!(!was_emitted(
            "glob 'files/en-us/web/**' matched 0 files. Globs are relative to --dir 'files/en-us'. Did you mean 'web/**'?"
        ));
    }

    #[test]
    fn glob_overlap_no_warning_when_dir_is_dot() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new(".");
        warn_glob_dir_overlap(dir, &["web/**".to_owned()], 0);
        // No warning should be tracked at all
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn glob_overlap_warns_on_full_dir_prefix() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("files/en-us");
        warn_glob_dir_overlap(dir, &["files/en-us/web/css/**".to_owned()], 0);
        assert!(was_emitted(
            "glob 'files/en-us/web/css/**' matched 0 files. Globs are relative to --dir 'files/en-us'. Did you mean 'web/css/**'?"
        ));
    }

    #[test]
    fn glob_overlap_warns_on_last_component() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("files/en-us");
        warn_glob_dir_overlap(dir, &["en-us/web/**".to_owned()], 0);
        assert!(was_emitted(
            "glob 'en-us/web/**' matched 0 files. Globs are relative to --dir 'files/en-us'. Did you mean 'web/**'?"
        ));
    }

    #[test]
    fn glob_overlap_no_false_positive_on_partial_prefix() {
        // "notes" should NOT match "notes-archive/**"
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("vault/notes");
        warn_glob_dir_overlap(dir, &["notes-archive/**".to_owned()], 0);
        // Should not emit any glob-overlap warning
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn glob_overlap_no_false_positive_on_partial_dir_prefix() {
        // "files/en-us" should NOT match "files/en-us-old/**"
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("files/en-us");
        warn_glob_dir_overlap(dir, &["files/en-us-old/**".to_owned()], 0);
        // Should not emit any glob-overlap warning
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn glob_overlap_skips_negation_patterns() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("docs");
        warn_glob_dir_overlap(dir, &["!docs/drafts/**".to_owned()], 0);
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn glob_overlap_no_warning_when_globs_empty() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("docs");
        warn_glob_dir_overlap(dir, &[], 0);
        assert_eq!(total_suppressed(), 0);
    }
}
