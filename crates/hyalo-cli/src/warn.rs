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

/// Emit an informational note to stderr (prefixed with `note:`).
///
/// Same dedup/quiet semantics as [`warn`], but uses the `note:` prefix so the
/// message reads as advisory rather than a warning. Callers should pass the
/// bare message text — do not include a leading `note:` (the function adds it).
pub fn note(msg: impl AsRef<str>) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }

    let msg = msg.as_ref();

    if let Ok(mut guard) = SUPPRESSED.lock()
        && let Some(ref mut map) = *guard
    {
        if let Some(count) = map.get_mut(msg) {
            *count += 1;
            return;
        }
        map.insert(msg.to_owned(), 0);
    }

    eprintln!("note: {msg}");
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

/// Return whether any tracked warning key starts with the given prefix.
///
/// Useful when the exact message contains a path that's known only at runtime.
/// **For use in tests only.**
#[cfg(test)]
pub fn any_tracked_starts_with(prefix: &str) -> bool {
    SUPPRESSED
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|m| m.keys().any(|k| k.starts_with(prefix))))
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

/// Emit the LLM-misuse warning for a given configured vault `dir`.
///
/// LLM-driven shells (Claude Code etc.) frequently `cd` into the configured
/// `dir` or pass absolute `--file` paths. Both work badly — the configured
/// `dir` (whether from `.hyalo.toml` or a `--dir` flag) already pins the
/// vault root. This warning teaches them not to repeat the mistake, while
/// the underlying command still proceeds.
///
/// Goes through `warn()`, so it dedupes by message (one print per process)
/// and respects `--quiet`.
pub fn warn_llm_misuse(dir: &std::path::Path) {
    let dir_display = dir.display();
    warn(format!(
        "hyalo is configured with dir = \"{dir_display}\".\n  \
         Do not cd into \"{dir_display}\" or pass absolute paths to --file.\n  \
         Run hyalo from the project root and pass paths relative to \"{dir_display}\", e.g.\n    \
         hyalo set iterations/iteration-17.md --property status=in-progress"
    ));
}

/// If the current working directory lies inside the configured vault dir,
/// emit the LLM-misuse warning once.
///
/// Walks ancestors of CWD looking for `.hyalo.toml`. When the file is found
/// in a strict ancestor (not CWD itself) and its `dir` value resolves to a
/// directory that contains CWD, the user has clearly `cd`-ed into the vault
/// — warn so the LLM stops doing it.
///
/// Anything ambiguous (no `.hyalo.toml` in the chain, malformed TOML, vault
/// that fails to canonicalize) is silently skipped.
pub fn warn_if_cwd_in_vault() {
    let Ok(cwd) = std::env::current_dir() else {
        return;
    };
    warn_if_cwd_in_vault_with_cwd(&cwd);
}

/// Same as [`warn_if_cwd_in_vault`], but with an explicit `cwd` so it can be
/// exercised in tests without mutating the process working directory.
pub(crate) fn warn_if_cwd_in_vault_with_cwd(cwd: &std::path::Path) {
    let Ok(cwd_canonical) = dunce::canonicalize(cwd) else {
        return;
    };
    // Walk strict ancestors looking for `.hyalo.toml`. Skip CWD itself —
    // when the config lives in CWD, the project root *is* CWD and there's
    // no "inside the vault" ambiguity to warn about.
    let mut current: Option<&std::path::Path> = cwd_canonical.parent();
    while let Some(ancestor) = current {
        let toml_path = ancestor.join(".hyalo.toml");
        if toml_path.is_file() {
            check_cwd_against_config(&cwd_canonical, ancestor, &toml_path);
            // The closest ancestor `.hyalo.toml` is the project root for this
            // misuse check; whether or not it triggered the warning, stop
            // walking. (Note: `config::load_config()` itself only reads CWD's
            // `.hyalo.toml`, not ancestors — this walk exists specifically to
            // detect the misuse case where the user `cd`-ed past the config.)
            return;
        }
        current = ancestor.parent();
    }
}

/// Read `dir` out of `toml_path` and warn if `cwd_canonical` is inside the
/// resolved vault. Best-effort: anything malformed is silently skipped.
fn check_cwd_against_config(
    cwd_canonical: &std::path::Path,
    config_dir: &std::path::Path,
    toml_path: &std::path::Path,
) {
    let Ok(text) = std::fs::read_to_string(toml_path) else {
        return;
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
        return;
    };
    let dir_value = parsed.get("dir").and_then(|v| v.as_str()).unwrap_or(".");
    let dir_path = std::path::Path::new(dir_value);
    if dir_path
        .components()
        .eq(std::path::Path::new(".").components())
    {
        // dir = "." — the configured vault *is* the project root, no nested
        // subdir to be "inside".
        return;
    }
    let vault_path = config_dir.join(dir_path);
    let Ok(vault_canonical) = dunce::canonicalize(&vault_path) else {
        return;
    };
    if cwd_canonical.starts_with(&vault_canonical) {
        warn_llm_misuse(dir_path);
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

    // -----------------------------------------------------------------------
    // Iteration 128 — LLM-misuse warning
    // -----------------------------------------------------------------------

    #[test]
    fn llm_misuse_warning_text_references_dir_and_example() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("hyalo-knowledgebase");
        warn_llm_misuse(dir);
        assert!(any_tracked_starts_with(
            "hyalo is configured with dir = \"hyalo-knowledgebase\""
        ));
    }

    #[test]
    fn llm_misuse_warning_dedupes_across_calls() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let dir = std::path::Path::new("kb");
        warn_llm_misuse(dir);
        warn_llm_misuse(dir);
        warn_llm_misuse(dir);
        // First print is recorded with suppression count 0; the next two are
        // suppressed.
        assert_eq!(total_suppressed(), 2);
    }

    fn make_project_with_config(dir_value: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join(".hyalo.toml"),
            format!("dir = \"{dir_value}\"\n"),
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join(dir_value)).unwrap();
        tmp
    }

    #[test]
    fn cwd_in_vault_warns_when_inside_configured_dir() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let project = make_project_with_config("kb");
        let vault = project.path().join("kb");
        warn_if_cwd_in_vault_with_cwd(&vault);
        assert!(any_tracked_starts_with(
            "hyalo is configured with dir = \"kb\""
        ));
    }

    #[test]
    fn cwd_in_vault_warns_when_inside_a_subdir_of_vault() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let project = make_project_with_config("kb");
        let nested = project.path().join("kb/iterations");
        std::fs::create_dir_all(&nested).unwrap();
        warn_if_cwd_in_vault_with_cwd(&nested);
        assert!(any_tracked_starts_with(
            "hyalo is configured with dir = \"kb\""
        ));
    }

    #[test]
    fn cwd_in_vault_no_warn_when_cwd_equals_config_dir() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let project = make_project_with_config("kb");
        // CWD is the project root (where .hyalo.toml lives) — that's the
        // intended invocation site, no warning.
        warn_if_cwd_in_vault_with_cwd(project.path());
        assert_eq!(total_suppressed(), 0);
        assert!(!any_tracked_starts_with("hyalo is configured with dir"));
    }

    #[test]
    fn cwd_in_vault_no_warn_when_no_config_in_ancestors() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let tmp = tempfile::tempdir().unwrap();
        // No .hyalo.toml anywhere up the chain — silently skip.
        warn_if_cwd_in_vault_with_cwd(tmp.path());
        assert_eq!(total_suppressed(), 0);
        assert!(!any_tracked_starts_with("hyalo is configured with dir"));
    }

    #[test]
    fn cwd_in_vault_no_warn_when_dir_is_dot() {
        // dir = "." means the project root *is* the vault — no nested vault to
        // be inside, so we never warn.
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
        // Make a nested dir to invoke from.
        let nested = tmp.path().join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        warn_if_cwd_in_vault_with_cwd(&nested);
        assert_eq!(total_suppressed(), 0);
    }

    #[test]
    fn cwd_in_vault_no_warn_when_cwd_is_sibling_of_vault() {
        let _guard = super::WARN_TEST_LOCK.lock().unwrap();
        reset_for_test();
        init(false);
        let project = make_project_with_config("kb");
        // CWD is a sibling of the vault, not inside it.
        let sibling = project.path().join("other");
        std::fs::create_dir_all(&sibling).unwrap();
        warn_if_cwd_in_vault_with_cwd(&sibling);
        assert_eq!(total_suppressed(), 0);
    }
}
