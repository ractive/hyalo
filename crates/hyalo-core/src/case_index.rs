use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::Path;

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
/// Used for case-insensitive link resolution: insert all known paths at
/// index build time, then look up by lowercased target at resolution time.
///
/// Also indexes by lowercased filename stem (without `.md` extension and
/// directory path) for Obsidian-style bare wikilink resolution — e.g.
/// `[[note]]` resolving to `sub/note.md` when that stem is unique.
#[derive(Debug, Default, Clone)]
pub struct CaseInsensitiveIndex {
    /// Map from lowercased path → list of real (original-casing) paths.
    map: HashMap<String, Vec<String>>,
    /// Map from lowercased filename stem → list of real (original-casing) paths.
    /// Used for Obsidian-style bare wikilink resolution.
    stem_map: HashMap<String, Vec<String>>,
}

impl CaseInsensitiveIndex {
    /// Create an empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a real relative path (forward-slash form). Stores a lowercase key.
    /// Deduplicates: inserting the same path twice has no effect.
    pub fn insert(&mut self, rel_path: &str) {
        let key = rel_path.to_ascii_lowercase();
        let candidates = self.map.entry(key).or_default();
        if !candidates.iter().any(|c| c == rel_path) {
            candidates.push(rel_path.to_owned());
        }

        // Also index by filename stem for Obsidian-style bare wikilink resolution.
        let fname = rel_path.rsplit('/').next().unwrap_or(rel_path);
        let stem = fname
            .strip_suffix(".md")
            .or_else(|| fname.strip_suffix(".MD"))
            .unwrap_or(fname);
        let stem_key = stem.to_ascii_lowercase();
        let stem_candidates = self.stem_map.entry(stem_key).or_default();
        if !stem_candidates.iter().any(|c| c == rel_path) {
            stem_candidates.push(rel_path.to_owned());
        }
    }

    /// Look up a relative path (any casing). Returns the canonical real path
    /// only when exactly one candidate exists (unambiguous match).
    pub fn lookup_unique(&self, rel_path: &str) -> Option<&str> {
        let key = rel_path.to_ascii_lowercase();
        let candidates = self.map.get(&key)?;
        if candidates.len() == 1 {
            Some(&candidates[0])
        } else {
            None
        }
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
    pub fn lookup_all(&self, rel_path: &str) -> &[String] {
        let key = rel_path.to_ascii_lowercase();
        self.map.get(&key).map_or(&[], Vec::as_slice)
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

/// Probe the filesystem under `dir` for case-insensitive behavior.
///
/// Creates a temporary file with a lowercase-only name, then stat's its
/// uppercase variant. Returns `Ok(true)` if the filesystem is
/// case-insensitive (uppercase lookup succeeds), `Ok(false)` otherwise.
///
/// On probe errors (permissions, read-only fs), returns `Ok(false)` — we
/// prefer strict semantics as the safe default.
pub fn probe_case_insensitive(dir: &Path) -> Result<bool> {
    use std::io::Write as _;
    use std::time::{SystemTime, UNIX_EPOCH};

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

        let lower_name = format!(".hyalo-case-probe-{suffix}");
        let upper_name = lower_name.to_ascii_uppercase();

        let lower_path = dir.join(&lower_name);
        let upper_path = dir.join(&upper_name);

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

/// Resolve a `CaseInsensitiveMode` to a concrete `bool` given a directory.
///
/// - `Off` → always `false`.
/// - `On` → always `true`.
/// - `Auto` → runs [`probe_case_insensitive`]; falls back to `false` on error.
pub fn mode_enabled(mode: CaseInsensitiveMode, dir: &Path) -> bool {
    match mode {
        CaseInsensitiveMode::Off => false,
        CaseInsensitiveMode::On => true,
        CaseInsensitiveMode::Auto => probe_case_insensitive(dir).unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- CaseInsensitiveIndex ----

    #[test]
    fn insert_and_lookup_unique() {
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("Foo/Bar.md");
        idx.insert("foo/baz.md");

        // Lowercase lookup for "foo/bar.md" → unambiguous → "Foo/Bar.md"
        assert_eq!(idx.lookup_unique("foo/bar.md"), Some("Foo/Bar.md"));
        // Different key → unambiguous → "foo/baz.md"
        assert_eq!(idx.lookup_unique("FOO/BAZ.MD"), Some("foo/baz.md"));
    }

    #[test]
    fn ambiguous_returns_none() {
        let mut idx = CaseInsensitiveIndex::new();
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
}
