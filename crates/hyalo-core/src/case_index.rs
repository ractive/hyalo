use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

/// Mode for case-insensitive link resolution fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseInsensitiveMode {
    /// Enable only if the filesystem is probed as case-insensitive.
    #[default]
    Auto,
    /// Always disabled.
    Off,
    /// Always enabled.
    On,
}

impl CaseInsensitiveMode {
    /// Parse a string into a `CaseInsensitiveMode`.
    ///
    /// Accepted values (case-insensitive): `"auto"`, `"true"`, `"false"`.
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "true" => Ok(Self::On),
            "false" => Ok(Self::Off),
            other => bail!(
                "invalid case_insensitive value {other:?}: expected \"auto\", \"true\", or \"false\""
            ),
        }
    }

    /// Serialize back to a canonical string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "true",
            Self::Off => "false",
        }
    }
}

/// Lowercased-relative-path → list of real relative paths (forward-slash form).
///
/// Hosts two independent lookups that callers can mix and match:
///
/// 1. **Path lookup** ([`lookup_unique`], [`lookup_all`]) — case-insensitive
///    full-path matching. Gated on [`enable_case_insensitive_paths`] so a vault
///    configured with `case_insensitive = "false"` won't accidentally resolve
///    `[[Foo]]` to `foo.md`. Defaults to *disabled* — opt in via
///    [`set_case_insensitive_paths`].
/// 2. **Stem lookup** ([`lookup_stem`], [`lookup_stem_all`]) — bare-basename
///    matching for Obsidian-style short-form wikilinks (`[[note]]` →
///    `sub/note.md` when that stem is unique). Always active regardless of
///    case-insensitive-path mode — short-form is an Obsidian convention, not
///    a case-sensitivity feature.
#[derive(Debug, Default, Clone)]
pub struct CaseInsensitiveIndex {
    /// Map from lowercased path → list of real (original-casing) paths.
    map: HashMap<String, Vec<String>>,
    /// Map from lowercased filename stem → list of real (original-casing) paths.
    /// Used for Obsidian-style bare wikilink resolution.
    stem_map: HashMap<String, Vec<String>>,
    /// When `false`, [`lookup_unique`] and [`lookup_all`] return empty results.
    /// Stem lookups are unaffected. Set by [`set_case_insensitive_paths`] from
    /// the resolved `[links] case_insensitive` mode.
    case_insensitive_paths: bool,
}

impl CaseInsensitiveIndex {
    /// Create an empty index with case-insensitive path lookups disabled.
    /// Stem lookups are always active.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an empty index sized for `capacity` paths.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            map: HashMap::with_capacity(capacity),
            stem_map: HashMap::with_capacity(capacity),
            case_insensitive_paths: false,
        }
    }

    /// Enable or disable case-insensitive path lookups.
    /// Has no effect on stem lookups, which are always active.
    pub fn set_case_insensitive_paths(&mut self, enabled: bool) {
        self.case_insensitive_paths = enabled;
    }

    /// Whether case-insensitive path lookups are enabled on this index.
    #[must_use]
    pub fn case_insensitive_paths_enabled(&self) -> bool {
        self.case_insensitive_paths
    }

    /// Insert a real relative path (forward-slash form). Stores a lowercase key.
    /// Deduplicates: inserting the same path twice has no effect.
    ///
    /// Dedupe is decided against `map` alone, never against `stem_map`
    /// (iter-256, FIND-8). The two are written together, so membership in one
    /// implies membership in the other — but their bucket sizes are wildly
    /// different. A `map` bucket holds only the case-variants of one path
    /// (effectively one entry); a `stem_map` bucket holds every file sharing a
    /// basename, and a docs tree that names every page `index.md` puts the
    /// whole vault in a single bucket. Scanning that bucket per insert made
    /// building the index quadratic: 14 399 MDN files cost 62 ms of pure
    /// string comparison, which was the entire measured cost of
    /// `find --fields links` on an indexed vault.
    pub fn insert(&mut self, rel_path: &str) {
        let key = rel_path.to_ascii_lowercase();
        let candidates = self.map.entry(key).or_default();
        if candidates.iter().any(|c| c == rel_path) {
            return;
        }
        candidates.push(rel_path.to_owned());

        // Also index by filename stem for Obsidian-style bare wikilink resolution.
        let fname = rel_path.rsplit('/').next().unwrap_or(rel_path);
        let stem = if Path::new(fname)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            &fname[..fname.len() - 3]
        } else {
            fname
        };
        let stem_key = stem.to_ascii_lowercase();
        self.stem_map
            .entry(stem_key)
            .or_default()
            .push(rel_path.to_owned());
    }

    /// Look up a relative path (any casing). Returns the canonical real path
    /// only when exactly one candidate exists (unambiguous match).
    ///
    /// Returns `None` when case-insensitive path lookups are disabled on this
    /// index (see [`set_case_insensitive_paths`]).
    pub fn lookup_unique(&self, rel_path: &str) -> Option<&str> {
        if !self.case_insensitive_paths {
            return None;
        }
        let key = rel_path.to_ascii_lowercase();
        let candidates = self.map.get(&key)?;
        if candidates.len() == 1 {
            Some(&candidates[0])
        } else {
            None
        }
    }

    /// Whether the vault contains this exact, case-sensitive relative path.
    ///
    /// Unlike [`lookup_unique`](Self::lookup_unique) this answers plain
    /// membership and is **not** gated on the `case_insensitive_paths`
    /// toggle — callers that need "does this file exist?" without a
    /// filesystem hit (iter-203's directory-index backlink keys) use it.
    #[must_use]
    pub fn contains_path(&self, rel_path: &str) -> bool {
        self.map
            .get(&rel_path.to_ascii_lowercase())
            .is_some_and(|candidates| candidates.iter().any(|c| c == rel_path))
    }

    /// Look up a bare filename stem (no directory, no `.md` extension).
    /// Returns the canonical real path only when exactly one file has that
    /// stem (unambiguous Obsidian-style resolution).
    ///
    /// Example: `lookup_stem("note")` matches `sub/note.md` if it's the only
    /// file named `note.md` in the vault.
    pub fn lookup_stem(&self, stem: &str) -> Option<&str> {
        let key = stem.to_ascii_lowercase();
        let candidates = self.stem_map.get(&key)?;
        if candidates.len() == 1 {
            Some(&candidates[0])
        } else {
            None
        }
    }

    /// Return all candidates for a given path (any casing). Useful for diagnostics.
    ///
    /// Returns an empty slice when case-insensitive path lookups are disabled.
    pub fn lookup_all(&self, rel_path: &str) -> &[String] {
        if !self.case_insensitive_paths {
            return &[];
        }
        let key = rel_path.to_ascii_lowercase();
        self.map.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Return all candidate paths for a bare filename stem (case-insensitive).
    ///
    /// Unlike [`lookup_stem`] (which returns `None` for ambiguous matches),
    /// this method always returns all candidates — useful for detecting when a
    /// short-form link is ambiguous rather than simply unresolvable.
    pub fn lookup_stem_all(&self, stem: &str) -> &[String] {
        let key = stem.to_ascii_lowercase();
        self.stem_map.get(&key).map_or(&[], Vec::as_slice)
    }

    /// Returns `true` if the index contains no entries.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Number of distinct lowercased keys in the index.
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

/// Filename prefix used by the write-based fallback probe.
///
/// Public so callers can sweep orphaned probe files (see
/// [`sweep_stale_case_probes`]) without duplicating the literal.
pub const CASE_PROBE_PREFIX: &str = ".hyalo-case-probe-";

/// Number of times a filesystem probe actually ran in this process.
///
/// Incremented by [`probe_case_insensitive_cached`] only on a cache miss, so a
/// test can assert that a whole command invocation resolves the mode at most
/// once. Not part of the public API surface on purpose.
static PROBE_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Per-process memo of resolved case sensitivity, keyed by canonical vault dir.
static PROBE_CACHE: OnceLock<Mutex<HashMap<PathBuf, bool>>> = OnceLock::new();

/// How many filesystem probes this process has actually performed.
///
/// Exposed for tests and diagnostics; the count only grows on cache misses.
#[must_use]
#[allow(dead_code)] // pub before the ARCH-5 façade (iter-225); kept for diagnostics
pub(crate) fn probe_count() -> usize {
    PROBE_COUNT.load(Ordering::Relaxed)
}

/// Flip the ASCII case of every ASCII letter in `name`.
///
/// Returns `None` when `name` has no ASCII letters — such a name cannot
/// distinguish a case-sensitive filesystem from a case-insensitive one.
/// Deliberately ASCII-only: non-ASCII case folding differs between
/// filesystems (and between Unicode versions), so a flipped non-ASCII name is
/// not a reliable probe.
fn flip_ascii_case(name: &str) -> Option<String> {
    if !name.bytes().any(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    Some(
        name.chars()
            .map(|c| {
                if c.is_ascii_lowercase() {
                    c.to_ascii_uppercase()
                } else if c.is_ascii_uppercase() {
                    c.to_ascii_lowercase()
                } else {
                    c
                }
            })
            .collect(),
    )
}

/// Whether two stat results describe the same filesystem object.
#[cfg(unix)]
fn same_object(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Whether two stat results describe the same filesystem object.
///
/// Windows has no cheap stat-level inode, so this compares the observable
/// attributes instead. A false positive would require two same-case-folded
/// names in one directory (only possible with per-directory case sensitivity
/// enabled) that also agree on type, size and both timestamps.
#[cfg(not(unix))]
fn same_object(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    a.is_dir() == b.is_dir()
        && a.is_file() == b.is_file()
        && a.is_symlink() == b.is_symlink()
        && a.len() == b.len()
        && a.modified().ok() == b.modified().ok()
        && a.created().ok() == b.created().ok()
}

/// Stat-only probe against one existing path.
///
/// Returns `Some(true)` when the case-flipped sibling name resolves to the
/// very same object (case-insensitive filesystem), `Some(false)` when the
/// flipped name is absent or resolves to a *different* object
/// (case-sensitive filesystem), and `None` when `path` is unusable as a probe
/// candidate (no ASCII letters, non-UTF-8 name, no parent, vanished).
fn probe_via_existing_path(path: &Path) -> Option<bool> {
    let name = path.file_name()?.to_str()?;
    let flipped = flip_ascii_case(name)?;
    let flipped_path = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(&flipped),
        _ => PathBuf::from(&flipped),
    };

    let Ok(flipped_meta) = std::fs::symlink_metadata(&flipped_path) else {
        // Flipped name does not exist → the filesystem distinguishes case.
        return Some(false);
    };
    let real_meta = std::fs::symlink_metadata(path).ok()?;
    Some(same_object(&real_meta, &flipped_meta))
}

/// Probe the filesystem under `dir` for case-insensitive behavior **without
/// writing anything**.
///
/// Tries, in order:
///
/// 1. The first directory entry of `dir` whose name contains an ASCII letter —
///    this measures `dir`'s own semantics, which matters on Windows where
///    case sensitivity can be set per directory.
/// 2. `dir` itself, looked up under a case-flipped final component.
///
/// Returns `None` when neither candidate exists (an empty vault whose own
/// path has no ASCII letters, or an unreadable directory), leaving the caller
/// to fall back to [`probe_case_insensitive`].
pub fn probe_case_insensitive_stat(dir: &Path) -> Option<bool> {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            // Never probe with our own leftovers: an orphaned probe file would
            // make the answer depend on a previous run's crash.
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(CASE_PROBE_PREFIX))
            {
                continue;
            }
            if let Some(result) = probe_via_existing_path(&path) {
                return Some(result);
            }
        }
    }

    if let Some(result) = probe_via_existing_path(dir) {
        return Some(result);
    }
    // A relative path such as `.` has no usable final component; retry against
    // its absolute form before giving up.
    let canonical = std::fs::canonicalize(dir).ok()?;
    probe_via_existing_path(&canonical)
}

/// Remove orphaned `.hyalo-case-probe-*` files left in `dir` by a fallback
/// probe that was killed between creating and deleting its probe file.
///
/// Only files older than 60 seconds are removed, so a probe running
/// concurrently in another process is never yanked out from under it. Errors
/// are ignored — this is a best-effort sweep. Returns the number of files
/// removed.
pub fn sweep_stale_case_probes(dir: &Path) -> usize {
    sweep_case_probes_older_than(dir, std::time::Duration::from_mins(1))
}

/// Implementation of [`sweep_stale_case_probes`] with the age threshold
/// injected, so tests can exercise the sweep without waiting a minute.
fn sweep_case_probes_older_than(dir: &Path, min_age: std::time::Duration) -> usize {
    use std::time::SystemTime;

    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let now = SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        // A case-insensitive filesystem may surface the probe under either
        // casing, so match without regard to case.
        if !name.to_ascii_lowercase().starts_with(CASE_PROBE_PREFIX) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let old_enough = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Whether `a` and `b` live on the same filesystem/volume.
///
/// Used to decide whether [`probe_case_insensitive`] can safely write its
/// probe file to `std::env::temp_dir()` instead of the vault: case
/// sensitivity is a per-filesystem property, so a probe on the wrong device
/// (e.g. a `tmpfs` `$TMPDIR` when the vault lives on a case-insensitive
/// network share) would give the wrong answer. Returns `false` — "assume
/// different, don't risk it" — whenever either path can't be stat'd or the
/// platform offers no cheap way to compare (ADVISORY-c,
/// adversarial-review-2026-08-23.md).
#[cfg(unix)]
fn same_device(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    ma.dev() == mb.dev()
}

/// Windows has no cheap stat-level device id exposed by `std`, so this
/// compares the canonicalized paths' drive/UNC prefix component instead — a
/// coarser check (misses the rare case of two distinct volumes mounted under
/// the same drive letter, e.g. `subst`), but a false "different device"
/// answer only costs an in-vault probe, never an incorrect result, so
/// erring conservative here is safe.
#[cfg(windows)]
fn same_device(a: &Path, b: &Path) -> bool {
    let prefix = |p: &Path| {
        dunce::canonicalize(p)
            .ok()?
            .components()
            .next()
            .map(|c| c.as_os_str().to_os_string())
    };
    match (prefix(a), prefix(b)) {
        (Some(pa), Some(pb)) => pa == pb,
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_device(_a: &Path, _b: &Path) -> bool {
    false
}

/// Where [`probe_case_insensitive`] should create its probe file.
///
/// Prefers `std::env::temp_dir()` when it is verified to be on the same
/// filesystem as `dir` — keeping the transient create/delete entirely
/// outside the user's vault, so it neither pings file watchers scanning the
/// repo tree nor shows up as a flickering untracked file in `git status`.
/// Falls back to `dir` itself (the original behavior) whenever that can't be
/// verified, since correctness of the case-sensitivity answer always wins
/// over avoiding the noise.
fn probe_write_dir(dir: &Path) -> PathBuf {
    let tmp = std::env::temp_dir();
    if same_device(dir, &tmp) {
        tmp
    } else {
        dir.to_path_buf()
    }
}

/// Probe the filesystem under `dir` for case-insensitive behavior.
///
/// **Write-based fallback probe.** Prefer [`probe_case_insensitive_stat`],
/// which answers the same question with stat calls only; this variant is used
/// when the vault holds no usable probe candidate.
///
/// Creates a temporary file with a lowercase-only name, then stat's its
/// uppercase variant. Returns `Ok(true)` if the filesystem is
/// case-insensitive (uppercase lookup succeeds), `Ok(false)` otherwise.
///
/// On probe errors (permissions, read-only fs), returns `Ok(false)` — we
/// prefer strict semantics as the safe default.
///
/// The probe file itself is written to [`probe_write_dir`]`(dir)` — usually
/// the system temp dir, verified same-device, rather than `dir` — so the
/// vault directory does not see a transient create/delete (ADVISORY-c).
#[allow(clippy::unnecessary_wraps)] // pub before the ARCH-5 façade (iter-225)
pub(crate) fn probe_case_insensitive(dir: &Path) -> Result<bool> {
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};

    let write_dir = probe_write_dir(dir);

    // Try a handful of unique probe names. Include seconds, nanoseconds, PID,
    // and attempt counter to minimize collisions across concurrent calls and
    // processes. On each attempt, ensure neither the lowercase nor uppercase
    // variant preexists — a stray preexisting uppercase file on a
    // case-sensitive filesystem would otherwise cause a false positive.
    for attempt in 0..16u32 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let suffix = format!(
            "{:x}-{:08x}-{:x}-{:x}",
            now.as_secs(),
            now.subsec_nanos(),
            std::process::id(),
            attempt
        );

        let lower_name = format!("{CASE_PROBE_PREFIX}{suffix}");
        let upper_name = lower_name.to_ascii_uppercase();

        let lower_path = write_dir.join(&lower_name);
        let upper_path = write_dir.join(&upper_name);

        if lower_path.exists() || upper_path.exists() {
            continue;
        }

        // `create_new` fails if the file already exists, protecting against
        // races with other processes that happen to pick the same suffix.
        let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lower_path)
        else {
            continue;
        };

        let _ = file.write_all(b"x");
        drop(file);

        let result = std::fs::metadata(&upper_path).is_ok();

        // Clean up — ignore errors; the file is tiny and harmless.
        let _ = std::fs::remove_file(&lower_path);

        return Ok(result);
    }

    // Gave up after max attempts; prefer strict semantics.
    Ok(false)
}

/// Resolve case sensitivity for `dir`, probing at most once per process.
///
/// The result is memoized in a process-global map keyed by the canonical form
/// of `dir` (falling back to the path as given when it cannot be
/// canonicalized), so the several `mode_enabled` call sites in a single
/// command invocation share one answer — and one probe.
///
/// The probe itself is stat-only ([`probe_case_insensitive_stat`]) whenever
/// the vault contains any usable candidate; the write-based
/// [`probe_case_insensitive`] runs only for a vault that offers none.
pub fn probe_case_insensitive_cached(dir: &Path) -> bool {
    let key = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    let cache = PROBE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    if let Ok(map) = cache.lock()
        && let Some(&cached) = map.get(&key)
    {
        return cached;
    }

    PROBE_COUNT.fetch_add(1, Ordering::Relaxed);
    let resolved = match probe_case_insensitive_stat(dir) {
        Some(result) => result,
        None => probe_case_insensitive(dir).unwrap_or(false),
    };

    if let Ok(mut map) = cache.lock() {
        map.insert(key, resolved);
    }
    resolved
}

/// Resolve a `CaseInsensitiveMode` to a concrete `bool` given a directory.
///
/// - `Off` → always `false`.
/// - `On` → always `true`.
/// - `Auto` → runs [`probe_case_insensitive_cached`]; falls back to `false`
///   when the filesystem cannot be probed at all (for example a read-only
///   mount holding an empty vault). That fallback means case-insensitive link
///   resolution silently turns **off**; set `[links] case_insensitive = "true"`
///   to force it on.
pub fn mode_enabled(mode: CaseInsensitiveMode, dir: &Path) -> bool {
    match mode {
        CaseInsensitiveMode::Off => false,
        CaseInsensitiveMode::On => true,
        CaseInsensitiveMode::Auto => probe_case_insensitive_cached(dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CaseInsensitiveIndex ----

    #[test]
    fn insert_and_lookup_unique() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("Foo/Bar.md");
        idx.insert("foo/baz.md");

        // Lowercase lookup for "foo/bar.md" → unambiguous → "Foo/Bar.md"
        assert_eq!(idx.lookup_unique("foo/bar.md"), Some("Foo/Bar.md"));
        // Different key → unambiguous → "foo/baz.md"
        assert_eq!(idx.lookup_unique("FOO/BAZ.MD"), Some("foo/baz.md"));
    }

    #[test]
    fn lookup_unique_disabled_returns_none() {
        // Default-constructed index has case-insensitive path lookups OFF;
        // lookup_unique returns None even when a match exists.
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("Foo/Bar.md");
        assert!(idx.lookup_unique("foo/bar.md").is_none());
        assert!(idx.lookup_all("foo/bar.md").is_empty());

        // Stem lookup, however, is always active.
        assert_eq!(idx.lookup_stem("Bar"), Some("Foo/Bar.md"));
    }

    #[test]
    fn ambiguous_returns_none() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("Foo.md");
        idx.insert("foo.md");

        // Two candidates → ambiguous → None
        assert!(idx.lookup_unique("foo.md").is_none());
        // But lookup_all should return both
        assert_eq!(idx.lookup_all("foo.md").len(), 2);
    }

    #[test]
    fn empty_index_returns_none() {
        let idx = CaseInsensitiveIndex::new();
        assert!(idx.lookup_unique("anything.md").is_none());
        assert!(idx.lookup_all("anything.md").is_empty());
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
    }

    // ---- Stem lookup (Obsidian-style bare wikilink resolution) ----

    #[test]
    fn contains_path_is_case_sensitive_and_ignores_toggle() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("Foo/index.md");
        // Toggle is off by default; contains_path must still answer.
        assert!(idx.contains_path("Foo/index.md"));
        assert!(!idx.contains_path("foo/index.md"));
        assert!(!idx.contains_path("bar/index.md"));
    }

    #[test]
    fn lookup_stem_unique() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("sub/note.md");
        idx.insert("other/readme.md");
        // "note" stem is unique → resolves
        assert_eq!(idx.lookup_stem("note"), Some("sub/note.md"));
        // Case-insensitive stem lookup
        assert_eq!(idx.lookup_stem("NOTE"), Some("sub/note.md"));
    }

    #[test]
    fn lookup_stem_ambiguous() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("a/note.md");
        idx.insert("b/note.md");
        // Two files with same stem → ambiguous → None
        assert!(idx.lookup_stem("note").is_none());
    }

    #[test]
    fn lookup_stem_empty_index() {
        let idx = CaseInsensitiveIndex::new();
        assert!(idx.lookup_stem("anything").is_none());
    }

    #[test]
    fn deduplication() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("Foo/Bar.md");
        idx.insert("Foo/Bar.md"); // duplicate
        // Should still be unique (one candidate)
        assert_eq!(idx.lookup_unique("foo/bar.md"), Some("Foo/Bar.md"));
        assert_eq!(idx.lookup_all("foo/bar.md").len(), 1);
    }

    #[test]
    fn probe_roundtrip() {
        // We don't assert true or false — the filesystem determines that.
        // We just assert the call doesn't panic and returns Ok(_).
        let tmp = tempfile::tempdir().unwrap();
        let result = probe_case_insensitive(tmp.path());
        assert!(result.is_ok(), "probe returned Err: {:?}", result.err());
    }

    #[test]
    fn mode_parse_valid() {
        assert_eq!(
            CaseInsensitiveMode::parse("auto").unwrap(),
            CaseInsensitiveMode::Auto
        );
        assert_eq!(
            CaseInsensitiveMode::parse("AUTO").unwrap(),
            CaseInsensitiveMode::Auto
        );
        assert_eq!(
            CaseInsensitiveMode::parse("true").unwrap(),
            CaseInsensitiveMode::On
        );
        assert_eq!(
            CaseInsensitiveMode::parse("True").unwrap(),
            CaseInsensitiveMode::On
        );
        assert_eq!(
            CaseInsensitiveMode::parse("false").unwrap(),
            CaseInsensitiveMode::Off
        );
        assert_eq!(
            CaseInsensitiveMode::parse("FALSE").unwrap(),
            CaseInsensitiveMode::Off
        );
    }

    #[test]
    fn mode_parse_invalid() {
        assert!(CaseInsensitiveMode::parse("maybe").is_err());
        assert!(CaseInsensitiveMode::parse("yes").is_err());
        assert!(CaseInsensitiveMode::parse("").is_err());
    }

    #[test]
    fn mode_as_str_roundtrip() {
        for &mode in &[
            CaseInsensitiveMode::Auto,
            CaseInsensitiveMode::On,
            CaseInsensitiveMode::Off,
        ] {
            let s = mode.as_str();
            let parsed = CaseInsensitiveMode::parse(s).unwrap();
            assert_eq!(mode, parsed);
        }
    }

    #[test]
    fn mode_enabled_on_off() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert!(!mode_enabled(CaseInsensitiveMode::Off, dir));
        assert!(mode_enabled(CaseInsensitiveMode::On, dir));
    }

    // ---- Stat-only probe ----

    #[test]
    fn flip_ascii_case_flips_letters_only() {
        assert_eq!(flip_ascii_case("Foo.md").as_deref(), Some("fOO.MD"));
        assert_eq!(flip_ascii_case("a1-B").as_deref(), Some("A1-b"));
        // No ASCII letters → unusable as a probe candidate.
        assert!(flip_ascii_case("1234").is_none());
        assert!(flip_ascii_case("").is_none());
        assert!(flip_ascii_case("日本語").is_none());
    }

    /// `probe_roundtrip` above deliberately doesn't assert a direction
    /// because it runs on every platform and the filesystem under the OS
    /// temp dir varies. On real Windows, though, an NTFS volume is
    /// case-insensitive by default (per-directory case sensitivity is an
    /// explicit opt-in feature `tempfile::tempdir()` never sets), so this
    /// pins the actual expected answer on the one platform where it's a
    /// known constant — the gap iter-224 T-4 closes for the case-index
    /// probe (mirrors the M-2 drive-relative/ADS tests added alongside it).
    #[cfg(windows)]
    #[test]
    fn probe_case_insensitive_is_true_on_real_ntfs_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let probed = probe_case_insensitive(tmp.path()).unwrap();
        assert!(
            probed,
            "NTFS is case-insensitive by default; the probe should detect it \
             on a real Windows temp directory"
        );
    }

    /// Same contract as above, exercised through the stat-only probe variant
    /// (used when the vault already has a usable candidate file, rather than
    /// writing a throwaway probe file).
    #[cfg(windows)]
    #[test]
    fn stat_probe_is_true_on_real_ntfs_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Note.md"), "x").unwrap();
        let probed = probe_case_insensitive_stat(tmp.path()).expect("candidate file exists");
        assert!(
            probed,
            "NTFS is case-insensitive by default; the stat probe should \
             detect it on a real Windows temp directory"
        );
    }

    #[test]
    fn stat_probe_agrees_with_write_probe() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("Note.md"), "x").unwrap();

        let stat = probe_case_insensitive_stat(dir).expect("candidate file exists");
        let write = probe_case_insensitive(dir).unwrap();
        assert_eq!(
            stat, write,
            "stat-only probe disagreed with the write-based probe"
        );
    }

    #[test]
    fn stat_probe_writes_nothing_into_the_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("Note.md"), "x").unwrap();

        let before: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        let _ = probe_case_insensitive_stat(dir);
        let after: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(before, after, "stat-only probe created or removed a file");
    }

    #[test]
    fn stat_probe_ignores_orphaned_probe_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        // Only candidate is an orphaned probe file; it must be skipped, so the
        // answer comes from the vault dir itself (still `Some`).
        std::fs::write(dir.join(format!("{CASE_PROBE_PREFIX}deadbeef")), "x").unwrap();
        assert!(probe_case_insensitive_stat(dir).is_some());
    }

    #[test]
    fn stat_probe_handles_empty_vault_via_dir_name() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Vault");
        std::fs::create_dir(&dir).unwrap();
        // Empty vault: no entry candidate, but the dir's own name works.
        assert!(probe_case_insensitive_stat(&dir).is_some());
    }

    #[test]
    fn stat_probe_detects_distinct_case_variants() {
        // When both casings exist as *different* files, the filesystem is
        // necessarily case-sensitive. On a case-insensitive filesystem the
        // second write just overwrites the first, so the setup is skipped.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("a.md"), "one").unwrap();
        std::fs::write(dir.join("A.MD"), "two").unwrap();
        if std::fs::read_to_string(dir.join("a.md")).unwrap() != "one" {
            return; // case-insensitive filesystem — nothing to assert here
        }
        assert_eq!(probe_case_insensitive_stat(dir), Some(false));
    }

    // ---- ADVISORY-c: write-based probe stays out of the vault ----
    // (adversarial-review-2026-08-23.md)

    #[test]
    fn same_device_is_reflexive() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            same_device(tmp.path(), tmp.path()),
            "a path must be considered the same device as itself"
        );
    }

    #[test]
    fn same_device_false_for_nonexistent_path() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(
            !same_device(tmp.path(), &missing),
            "an unstatable path must conservatively answer 'different device'"
        );
    }

    #[test]
    fn write_probe_does_not_touch_the_vault_when_temp_dir_is_same_device() {
        // `tempfile::tempdir()` creates its directory inside
        // `std::env::temp_dir()`, so the vault built here is guaranteed to be
        // on the same device as the temp dir the probe should prefer —
        // making this portable across CI platforms without needing a second
        // real filesystem mounted.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert!(
            same_device(dir, &std::env::temp_dir()),
            "test precondition: a tempdir must be same-device as env::temp_dir()"
        );

        let before: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();

        // Force the write-based fallback path directly (bypassing the
        // stat-only probe) against an otherwise-empty vault.
        let _ = probe_case_insensitive(dir).unwrap();

        let after: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            before, after,
            "write-based probe must not create or leave any file in the vault \
             directory when a same-device temp dir is available"
        );
    }

    #[test]
    fn write_probe_leaves_no_residual_file_in_temp_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let sys_tmp = std::env::temp_dir();
        let before: std::collections::HashSet<_> = std::fs::read_dir(&sys_tmp)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.file_name())
            .filter(|n| {
                n.to_str()
                    .is_some_and(|s| s.to_ascii_lowercase().starts_with(CASE_PROBE_PREFIX))
            })
            .collect();

        let _ = probe_case_insensitive(dir).unwrap();

        // Poll instead of asserting immediately: the system temp dir is shared
        // (sibling tests probe it concurrently), and on NTFS a deleted file
        // stays visible in directory listings while delete-pending (e.g. a CI
        // antivirus briefly holds a handle). Residue must *clear*, not be
        // instantaneously absent.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let after: std::collections::HashSet<_> = std::fs::read_dir(&sys_tmp)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.file_name())
                .filter(|n| {
                    n.to_str()
                        .is_some_and(|s| s.to_ascii_lowercase().starts_with(CASE_PROBE_PREFIX))
                })
                .collect();
            let residue: Vec<_> = after.difference(&before).collect();
            if residue.is_empty() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "probe must clean up its own file in the temp dir, leaving no \
                 residue; still present after 10s: {residue:?}"
            );
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[test]
    fn write_probe_detection_matches_ground_truth_when_redirected_to_temp_dir() {
        // The probe now writes into env::temp_dir() rather than `dir` itself
        // whenever they're same-device — this must not change the *answer*,
        // only where the transient file lives. Cross-check against a direct
        // filesystem oracle: write "abc", read back "ABC", see if it's the
        // same content (case-insensitive filesystem) or a miss
        // (case-sensitive).
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let probed = probe_case_insensitive(dir).unwrap();

        let oracle_lower = dir.join("hyalo-oracle-probe-abc");
        std::fs::write(&oracle_lower, "content").unwrap();
        let oracle_upper = dir.join("HYALO-ORACLE-PROBE-ABC");
        let ground_truth = std::fs::metadata(&oracle_upper).is_ok();
        std::fs::remove_file(&oracle_lower).unwrap();

        assert_eq!(
            probed, ground_truth,
            "redirecting the probe to temp_dir must not change the detected \
             case-sensitivity answer"
        );
    }

    #[test]
    fn cached_probe_runs_at_most_once_per_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join("Note.md"), "x").unwrap();

        let before = probe_count();
        let first = probe_case_insensitive_cached(dir);
        let after_first = probe_count();
        assert_eq!(
            after_first,
            before + 1,
            "first call should probe exactly once"
        );

        // Repeat the way a command's several `mode_enabled` call sites would.
        for _ in 0..7 {
            assert_eq!(mode_enabled(CaseInsensitiveMode::Auto, dir), first);
        }
        assert_eq!(
            probe_count(),
            after_first,
            "cached probe re-probed the same directory"
        );
    }

    // ---- Stale probe sweep ----

    #[test]
    fn sweep_removes_only_old_probe_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let probe = dir.join(format!("{CASE_PROBE_PREFIX}cafe"));
        std::fs::write(&probe, "x").unwrap();
        std::fs::write(dir.join("note.md"), "x").unwrap();

        // Fresh probe file is left alone by the real 60s threshold.
        assert_eq!(sweep_stale_case_probes(dir), 0);
        assert!(probe.exists());

        // With no minimum age it is swept, and unrelated files survive.
        assert_eq!(
            sweep_case_probes_older_than(dir, std::time::Duration::ZERO),
            1
        );
        assert!(!probe.exists());
        assert!(dir.join("note.md").exists());
    }

    #[test]
    fn sweep_on_missing_dir_is_a_noop() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(sweep_stale_case_probes(&tmp.path().join("nope")), 0);
    }

    // ---- iter-256 FIND-8: dedupe is decided against `map`, not `stem_map` ----

    /// The quadratic scan removed in iter-256 was the *stem* dedupe. These are
    /// the two properties that scan was there for; both must still hold now
    /// that the `map` bucket decides duplicates for both maps.
    #[test]
    fn reinserting_the_same_path_is_still_a_no_op_in_both_maps() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        for _ in 0..5 {
            idx.insert("docs/guide/index.md");
        }
        assert_eq!(idx.lookup_all("DOCS/GUIDE/INDEX.MD"), ["docs/guide/index.md"]);
        assert_eq!(idx.lookup_stem_all("index"), ["docs/guide/index.md"]);
        assert_eq!(idx.lookup_stem("index"), Some("docs/guide/index.md"));
    }

    /// A docs tree that names every page `index.md` puts the whole vault in one
    /// stem bucket — the shape (MDN, 14 399 files) that made the old dedupe
    /// quadratic. Every distinct path must still be recorded, and the stem must
    /// still report itself ambiguous.
    #[test]
    fn many_paths_sharing_one_stem_are_all_recorded_and_stay_ambiguous() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        let paths: Vec<String> = (0..500).map(|i| format!("files/p{i}/index.md")).collect();
        for p in &paths {
            idx.insert(p);
            // Re-inserting mid-stream must not add a second copy.
            idx.insert(p);
        }
        assert_eq!(idx.lookup_stem_all("index").len(), paths.len());
        assert_eq!(idx.lookup_stem("index"), None, "500 candidates is ambiguous");
        assert_eq!(idx.lookup_unique("FILES/P42/INDEX.MD"), Some("files/p42/index.md"));
    }

    /// Two paths differing only in case share a `map` bucket but are distinct
    /// entries — the case the `map`-side dedupe must not collapse.
    #[test]
    fn case_variants_of_one_path_stay_separate_candidates() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("Notes/Foo.md");
        idx.insert("Notes/foo.md");
        idx.insert("Notes/Foo.md");
        let mut all = idx.lookup_all("notes/foo.md").to_vec();
        all.sort();
        assert_eq!(all, ["Notes/Foo.md", "Notes/foo.md"]);
        assert_eq!(idx.lookup_unique("notes/foo.md"), None, "ambiguous by case");
        assert_eq!(idx.lookup_stem_all("Foo").len(), 2);
    }
}
