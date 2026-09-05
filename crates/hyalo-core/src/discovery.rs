#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex, OnceLock};

use crate::case_index::CaseInsensitiveIndex;
use crate::link_graph::strip_site_prefix;
use crate::util::levenshtein;

/// Process-global set of `[scan] include` globs.
///
/// The vault walker skips hidden (dot-prefixed) paths by default. When a vault
/// opts a hidden subtree back in via `[scan] include = ["glob", …]`, the CLI
/// installs the compiled glob set here once at startup (see
/// [`set_scan_include`]). [`discover_files`] then descends into any otherwise
/// hidden directory whose vault-relative path is a prefix of, or matches, one
/// of these globs — so the include list is honored by *every* command that
/// discovers vault files without threading the config through each call site.
///
/// `.git/**` is always hard-excluded regardless of the include list.
static SCAN_INCLUDE: OnceLock<Option<ScanInclude>> = OnceLock::new();

/// Process-global set of `[scan] exclude` globs (iter-265, DEC-277).
///
/// Obsidian's "Excluded files" has no analogue in hyalo: the only exclusion
/// knobs were per-feature (`[lint] ignore`, `[okf] ignore`, `[schema] exempt`),
/// so a vault of Templater templates had to be excluded once per command that
/// grew a knob. `[scan] exclude = ["Templates/**"]` is applied here, at file
/// discovery, which makes matching files invisible to *every* command — `find`,
/// `summary`, `tags`, `properties`, `lint`, `links *`, `mv`'s link graph,
/// `backlinks`, `create-index`, `views`, `types`, `okf`, `madr`.
///
/// Precedence: exclusion is the widest knob and wins over the narrower
/// per-feature lists — a file excluded here is never seen, so `[lint] ignore`
/// and friends only ever narrow further within what the walk returned. An
/// explicitly named target (`--file`, `--files-from`) is *refused* rather than
/// silently dropped, so a script never mistakes "excluded" for "clean".
static SCAN_EXCLUDE: OnceLock<Option<ScanExclude>> = OnceLock::new();

/// Compiled `[scan] exclude` configuration.
#[derive(Clone)]
pub struct ScanExclude {
    /// Glob set matched against vault-relative, forward-slash paths.
    set: GlobSet,
    /// The original patterns, in order, so a refusal can name the glob that
    /// matched rather than just saying "excluded".
    patterns: Vec<String>,
}

impl ScanExclude {
    /// Whether the vault-relative path `rel` is excluded, and by which pattern.
    #[must_use]
    pub fn matching_glob(&self, rel: &str) -> Option<&str> {
        let hit = self.set.matches(rel);
        hit.first()
            .and_then(|&i| self.patterns.get(i))
            .map(String::as_str)
    }

    /// Whether the vault-relative path `rel` is excluded.
    #[must_use]
    pub fn is_excluded(&self, rel: &str) -> bool {
        self.set.is_match(rel)
    }
}

/// Install the `[scan] exclude` glob set for this process.
///
/// Same contract as [`set_scan_include`]: idempotent-once, and invalid globs
/// are returned as `(pattern, message)` pairs rather than disabling the rest.
///
/// # Errors
/// Never returns `Err`.
pub fn set_scan_exclude(patterns: &[String]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    let compiled = if patterns.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        let mut kept = Vec::new();
        for pat in patterns {
            // `literal_separator(true)`: `*` and `?` do not cross a `/`,
            // matching how `[lint] ignore` and `--glob` already behave, so
            // `*.md` excludes only top-level files and `Templates/**` is
            // needed (and sufficient — `**` still crosses separators) to drop
            // a whole subtree, the way Obsidian's excluded folders work.
            match GlobBuilder::new(pat).literal_separator(true).build() {
                Ok(g) => {
                    builder.add(g);
                    kept.push(pat.clone());
                }
                Err(e) => errors.push((pat.clone(), e.to_string())),
            }
        }
        builder.build().ok().map(|set| ScanExclude {
            set,
            patterns: kept,
        })
    };
    let _ = SCAN_EXCLUDE.set(compiled);
    errors
}

/// The installed `[scan] exclude` set, if this process has one.
#[must_use]
pub fn scan_exclude() -> Option<&'static ScanExclude> {
    match SCAN_EXCLUDE.get() {
        Some(Some(exc)) => Some(exc),
        _ => None,
    }
}

/// Whether the vault-relative path `rel` is dropped by `[scan] exclude`.
#[must_use]
pub fn is_scan_excluded(rel: &str) -> bool {
    scan_exclude().is_some_and(|exc| exc.is_excluded(rel))
}

/// The `[scan] exclude` glob that drops `rel`, if any.
#[must_use]
pub fn scan_exclude_glob(rel: &str) -> Option<&'static str> {
    scan_exclude().and_then(|exc| exc.matching_glob(rel))
}

/// How many files the last completed vault walk dropped because of
/// `[scan] exclude`, for `summary`'s `results.files.excluded`.
static EXCLUDED_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Files dropped by `[scan] exclude` during this process's walks.
#[must_use]
pub fn scan_excluded_count() -> usize {
    EXCLUDED_COUNT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Record that a walk (or an index load) dropped `dropped` files.
///
/// `fetch_max` rather than `fetch_add`: one CLI run walks the vault several
/// times (count, collect, hint) and loads the index once, and the figure that
/// `summary` reports is "how many files this vault lost", not "how many drops
/// happened".
pub fn note_scan_excluded(dropped: usize) {
    EXCLUDED_COUNT.fetch_max(dropped, std::sync::atomic::Ordering::Relaxed);
}

/// Reset the excluded counter. **Tests only.**
pub fn reset_scan_excluded_count() {
    EXCLUDED_COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// Compiled `[scan] include` configuration.
#[derive(Clone)]
struct ScanInclude {
    /// Full glob set, matched against files to decide inclusion.
    set: GlobSet,
    /// Literal directory prefixes of each glob (the path portion before the
    /// first glob metacharacter). Used to decide whether to *descend* into a
    /// hidden directory: the walker enters a dir when it lies on the path to,
    /// or beneath, one of these prefixes.
    dir_prefixes: Vec<String>,
}

impl ScanInclude {
    /// Cheap clone for moving into the parallel-walk `filter_entry` closure
    /// (`GlobSet` is `Send + Sync` and cheap to clone — it shares compiled
    /// automata internally).
    fn clone_shared(&self) -> Self {
        self.clone()
    }

    /// Whether the walker should descend into the hidden vault-relative
    /// directory `rel` (forward-slash, no trailing slash) because a glob reaches
    /// into or beneath it.
    fn allows_dir(&self, rel: &str) -> bool {
        // `.git` is never re-included, even if a glob would match it.
        if rel == ".git" || rel.starts_with(".git/") {
            return false;
        }
        self.dir_prefixes.iter().any(|p| {
            p == rel
                || p.starts_with(&format!("{rel}/")) // rel is an ancestor of the prefix
                || rel.starts_with(&format!("{p}/")) // rel is inside the prefix
        })
    }

    /// Whether the file at vault-relative path `rel` is explicitly re-included.
    fn allows_file(&self, rel: &str) -> bool {
        if rel.starts_with(".git/") {
            return false;
        }
        self.set.is_match(rel)
    }
}

/// Literal directory prefix of a forward-slash glob: everything up to (but not
/// including) the segment that first contains a glob metacharacter. E.g.
/// `.claude/skills/**` → `.claude/skills`, `.config/*.md` → `.config`,
/// `.obsidian/**/x.md` → `.obsidian`. A glob with metacharacters in its first
/// segment yields `""`.
fn glob_dir_prefix(glob: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for seg in glob.split('/') {
        if seg.contains(['*', '?', '[', '{']) {
            break;
        }
        kept.push(seg);
    }
    // Drop a trailing filename-looking segment only if the whole glob was
    // literal (no metachars at all): then the "directory" is its parent.
    if kept.len() == glob.split('/').count() {
        kept.pop();
    }
    kept.join("/")
}

/// Process-wide switch for frontmatter-`aliases:` link resolution
/// (iter-272 Part B, DEC-288). Default **on**, like DEC-267 case folding.
static LINK_ALIASES: OnceLock<bool> = OnceLock::new();

/// Install the effective `[links] aliases` setting for this process.
///
/// Idempotent-once, exactly like [`set_scan_include`]: the CLI calls it once
/// after config resolution, before any command runs. Following that pattern
/// keeps the setting off the signature of every resolver, index builder and
/// graph pass that would otherwise have to forward it.
pub fn set_link_aliases(enabled: bool) {
    let _ = LINK_ALIASES.set(enabled);
}

/// Whether frontmatter `aliases:` resolve links in this process. Defaults to
/// `true` when nothing configured it (library callers, tests).
#[must_use]
pub fn link_aliases_enabled() -> bool {
    LINK_ALIASES.get().copied().unwrap_or(true)
}

/// Install the `[scan] include` glob set for this process.
///
/// Idempotent-once: only the first call takes effect (the walker reads it
/// through a `OnceLock`). The CLI calls this exactly once, right after config
/// resolution, before any command runs. `patterns` are vault-relative,
/// forward-slash globs (e.g. `.claude/skills/**`). Invalid globs are skipped
/// with the returned error list so a single bad pattern doesn't disable the
/// rest.
///
/// # Errors
/// Never returns `Err`; instead returns the list of `(pattern, message)` pairs
/// for any globs that failed to compile, so the caller can surface a warning.
pub fn set_scan_include(patterns: &[String]) -> Vec<(String, String)> {
    let mut errors = Vec::new();
    let compiled = if patterns.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        let mut dir_prefixes = Vec::new();
        for pat in patterns {
            match GlobBuilder::new(pat).literal_separator(true).build() {
                Ok(g) => {
                    builder.add(g);
                    let prefix = glob_dir_prefix(pat);
                    if !prefix.is_empty() {
                        dir_prefixes.push(prefix);
                    }
                }
                Err(e) => errors.push((pat.clone(), e.to_string())),
            }
        }
        builder
            .build()
            .ok()
            .map(|set| ScanInclude { set, dir_prefixes })
    };
    let _ = SCAN_INCLUDE.set(compiled);
    errors
}

/// Paths already reported as skipped by [`discover_files`], so a command that
/// walks the vault more than once per run does not repeat itself.
///
/// A single `hyalo` invocation calls `discover_files` several times (counting,
/// then collecting, then hinting), which made every out-of-vault symlink skip
/// print two or three identical warnings (iter-202 M-5). The set is
/// process-global and never cleared: one process is one CLI run.
static WARNED_SKIPS: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Emit `message` for `path` unless the same path was already reported.
///
/// A poisoned lock is treated as "not yet warned" — a duplicate warning is a
/// far better outcome than aborting a walk over it.
fn warn_skip_once(path: &Path, message: &str) {
    let first = match WARNED_SKIPS.lock() {
        Ok(mut seen) => seen.insert(path.to_path_buf()),
        Err(_) => true,
    };
    if first {
        eprintln!("warning: skipping {}: {message}", path.display());
    }
}

/// True when any component of `rel` is a hidden (dot-prefixed) segment.
fn has_hidden_component(rel: &str) -> bool {
    rel.split('/')
        .any(|c| c.starts_with('.') && c != "." && c != "..")
}

/// Collect all `.md` files under the given directory, respecting `.gitignore` and skipping hidden dirs.
///
/// Hidden (dot-prefixed) paths are skipped unless a `[scan] include` glob
/// (installed via [`set_scan_include`]) re-includes them; `.git/**` is always
/// excluded.
pub fn discover_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let include = match SCAN_INCLUDE.get() {
        Some(Some(inc)) => Some(inc),
        _ => None,
    };
    discover_files_with_include(dir, include)
}

/// Which files a vault walk should collect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    /// `*.md` — the notes that make up the vault (the historical behaviour).
    Markdown,
    /// Every other file that carries an extension — the vault's attachments
    /// (iter-261). Extension-less files are skipped; see [`discover_attachments`].
    NonMarkdown,
}

impl FileKind {
    fn accepts(self, path: &Path) -> bool {
        match path.extension() {
            // `Markdown` keeps the historical case-sensitive `== "md"` test;
            // `NonMarkdown` folds case so a stray `.MD` is not misfiled as an
            // attachment. A file matching neither is simply not collected.
            Some(ext) => match self {
                Self::Markdown => ext == "md",
                Self::NonMarkdown => !ext.eq_ignore_ascii_case("md"),
            },
            None => false,
        }
    }
}

/// Test-friendly variant of [`discover_files`] with an explicit include
/// override, bypassing the process-global `OnceLock` (which can only be set
/// once per process). Production code uses [`discover_files`].
fn discover_files_with_include(dir: &Path, include: Option<&ScanInclude>) -> Result<Vec<PathBuf>> {
    discover_files_with_include_ext(dir, include, FileKind::Markdown)
}

/// The shared vault walk, parameterized by which files to keep.
fn discover_files_with_include_ext(
    dir: &Path,
    include: Option<&ScanInclude>,
    kind: FileKind,
) -> Result<Vec<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    let (err_tx, err_rx) = mpsc::channel::<String>();
    let walk_root = dir.to_path_buf();
    let mut builder = WalkBuilder::new(dir);
    builder.git_ignore(true);
    if let Some(inc) = include {
        // Take over hidden-skipping so `[scan] include` can re-admit specific
        // dot-subtrees. `filter_entry` prunes hidden dirs/files not covered by
        // the include list while still descending into included ones.
        builder.hidden(false);
        let root = walk_root.clone();
        let inc = inc.clone_shared();
        builder.filter_entry(move |entry| {
            let rel = relative_path(&root, entry.path());
            if rel.is_empty() {
                return true;
            }
            if !has_hidden_component(&rel) {
                return true;
            }
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            // Hidden path: admit only if the include list reaches it.
            if is_dir {
                inc.allows_dir(&rel)
            } else {
                inc.allows_file(&rel)
            }
        });
    } else {
        builder.hidden(true); // skip hidden files/dirs
    }
    builder.build_parallel().run(|| {
        let tx = tx.clone();
        let err_tx = err_tx.clone();
        Box::new(move |entry| {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    let _ = err_tx.send(format!("{e}"));
                    return ignore::WalkState::Continue;
                }
            };
            let path = entry.path();
            if kind.accepts(path) && path.is_file() {
                let _ = tx.send(path.to_path_buf());
            }
            ignore::WalkState::Continue
        })
    });
    drop(tx); // close sender so rx iterator terminates
    drop(err_tx); // close error sender so err_rx iterator terminates

    for e in err_rx {
        eprintln!("warning: directory walk error: {e}");
    }

    let mut files: Vec<PathBuf> = rx.into_iter().collect();

    // Sort before filtering so the dedup below is deterministic: when a file is
    // reachable under two spellings, the lexicographically first one wins on
    // every platform and every run.
    files.sort();

    // Two jobs in one pass:
    //
    // 1. Drop symlinks whose target resolves outside the vault boundary.
    // 2. Drop duplicate spellings of the same file (iter-202 M-5). An in-vault
    //    symlink and its target are two directory entries but one file; left
    //    unmerged they make `links fix --apply` rewrite the same note twice
    //    (the second write trips the concurrent-modification guard) and
    //    double-count `find --count`, `summary` and glob-write totals.
    //
    // Only paths that are actually symlinks are canonicalized — the common
    // (non-symlink) case pays one `symlink_metadata` call and no more. A
    // non-symlink entry's canonical form is just the canonical vault root
    // joined with its vault-relative path, because the walker never descends
    // through directory symlinks.
    // Which spelling represents the file matters (iter-207, BUG-7): keeping
    // whichever the sort saw first meant an alphabetically-earlier symlink
    // (`alias-target.md -> target.md`) shadowed the real file, so `links fix`
    // lost `target.md` from the fuzzy candidate set (a `[fuzzy 0.966]` offer
    // became `Unfixable: 1`) and reported fixes against the alias name.
    // The real file always wins; a symlink only represents the group when
    // every spelling of it is a symlink.
    let canonical_dir = canonicalize_vault_dir(dir)?;
    let mut seen: HashMap<PathBuf, usize> = HashMap::with_capacity(files.len());
    let mut kept: Vec<(PathBuf, bool)> = Vec::with_capacity(files.len());
    for path in files {
        let is_symlink = path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink());
        let canonical = if is_symlink {
            match dunce::canonicalize(&path) {
                Ok(canonical) if canonical.starts_with(&canonical_dir) => canonical,
                Ok(_) => {
                    warn_skip_once(&path, "symlink target resolves outside vault");
                    continue;
                }
                Err(e) => {
                    warn_skip_once(&path, &format!("failed to resolve path: {e}"));
                    continue;
                }
            }
        } else {
            match path.strip_prefix(dir) {
                Ok(rel) => canonical_dir.join(rel),
                Err(_) => path.clone(),
            }
        };
        match seen.get(&canonical) {
            None => {
                seen.insert(canonical, kept.len());
                kept.push((path, is_symlink));
            }
            // A real file replaces the symlink that got there first.
            Some(&idx) if kept[idx].1 && !is_symlink => kept[idx] = (path, is_symlink),
            Some(_) => {}
        }
    }

    // Replacing a representative can disturb the sorted order established
    // above; restore it so callers keep their deterministic enumeration.
    let mut kept: Vec<PathBuf> = kept.into_iter().map(|(p, _)| p).collect();
    kept.sort();

    // `[scan] exclude` is applied last, on the deduplicated result, so a file
    // reachable under two spellings is excluded once and the counter below
    // matches the number of *files* the vault lost, not directory entries.
    if let Some(exc) = scan_exclude() {
        let before = kept.len();
        kept.retain(|p| !exc.is_excluded(&relative_path(dir, p)));
        let dropped = before - kept.len();
        if dropped > 0 {
            note_scan_excluded(dropped);
        }
    }

    Ok(kept)
}

/// Whether a vault-relative path or link target ends in `.md` (case-insensitive).
#[must_use]
pub fn has_md_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// Whether a link target carries an **explicit, non-markdown** file extension —
/// `img.png`, `Books.base`, `report.pdf` (iter-261 / BUG-5, BUG-6).
///
/// Such a target is an attachment reference: Obsidian resolves it against every
/// file in the vault (not just the notes) and never treats it as a note. A bare
/// `[[Foo]]` or a `[[Foo.md]]` is *not* one, and neither is a target whose only
/// dot sits in a directory component (`v1.2/notes`).
#[must_use]
pub fn has_non_md_extension(target: &str) -> bool {
    let name = target.rsplit(['/', '\\']).next().unwrap_or(target);
    // A leading dot is a hidden-file marker, not an extension (`.gitignore`).
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    if dot == 0 || dot + 1 == name.len() {
        return false;
    }
    !name[dot + 1..].eq_ignore_ascii_case("md")
}

/// Collect every non-`.md` file under `dir` that carries a file extension,
/// as vault-relative forward-slash paths (iter-261 / BUG-5, BUG-6).
///
/// These are the vault's **attachments** — images, PDFs, Obsidian `.base`
/// files — which Obsidian resolves links against exactly like notes. They are
/// indexed by basename and by path so `![[img.png]]` and `[[Books.base]]`
/// resolve the way Obsidian's "shortest path when possible" setting does.
///
/// Extension-less files are deliberately skipped: their basename key would
/// collide with a note's stem (`LICENSE` vs `LICENSE.md`) and turn a link that
/// resolves today into an ambiguous one.
pub fn discover_attachments(dir: &Path) -> Result<Vec<String>> {
    let include = match SCAN_INCLUDE.get() {
        Some(Some(inc)) => Some(inc),
        _ => None,
    };
    let files = discover_files_with_include_ext(dir, include, FileKind::NonMarkdown)?;
    Ok(files.iter().map(|f| relative_path(dir, f)).collect())
}

/// Resolve a link target that carries an explicit non-`.md` extension, using
/// the vault-wide attachment index (iter-261 / BUG-5, BUG-6).
///
/// Beyond what [`resolve_target`] already does for any target (literal path,
/// case-insensitive path, unique basename), this adds the *source-relative*
/// attempt Obsidian makes for a partially-qualified wikilink: `![[sub/x.png]]`
/// written in `notes/a.md` also finds `notes/sub/x.png`. Markdown destinations
/// already normalize against the source directory, so this only fills the
/// wikilink gap.
///
/// Returns `None` for any target without a non-`.md` extension, so callers can
/// use it as a pure fallback after their normal resolution attempt.
#[must_use]
pub fn resolve_attachment_from_source(
    canonical_dir: &Path,
    source_rel: &str,
    kind: crate::links::LinkKind,
    target: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Option<String> {
    if !has_non_md_extension(target) {
        return None;
    }
    if kind != crate::links::LinkKind::Wikilink {
        return None;
    }
    let normalized = target.replace('\\', "/");
    if !normalized.contains('/') || normalized.starts_with('/') {
        return None;
    }
    let src_rel = crate::link_graph::normalize_target(Path::new(source_rel), &normalized);
    resolve_target(canonical_dir, &src_rel, site_prefix, case_index)
}

/// Resolve a path argument relative to `--dir`. Verifies it exists and is `.md`.
/// Returns the full path under `dir` and the normalized relative path (for display).
/// Rejects absolute paths and `..` segments to prevent escaping the base directory.
///
/// This is the case-sensitive form — equivalent to
/// [`resolve_file_ci`]`(dir, path_arg, false)`. Use `resolve_file_ci` to honor
/// `[links] case_insensitive` for CLI `--file` arguments.
pub fn resolve_file(dir: &Path, path_arg: &str) -> Result<(PathBuf, String), FileResolveError> {
    resolve_file_ci(dir, path_arg, false)
}

/// Like [`resolve_file`], but when `case_insensitive` is `true` and the literal
/// (exact-casing) lookup misses, fall back to a case-insensitive directory scan
/// of the target's parent, resolving to the real on-disk casing when exactly
/// one case-variant exists.
///
/// This closes the gap (iter-184 CI failure) where `backlinks --file foo.md`
/// failed with "file not found" on a case-sensitive filesystem (Linux) even
/// with `[links] case_insensitive = "true"`, because the literal
/// `Path::is_file()` check never consulted the setting. On a case-insensitive
/// filesystem (macOS/Windows default) the literal check already succeeds, so
/// the fallback is a no-op there.
///
/// The fallback runs through the same vault-boundary / traversal / symlink
/// checks as the literal path — it only substitutes the on-disk casing of the
/// final path components, never a different directory.
pub fn resolve_file_ci(
    dir: &Path,
    path_arg: &str,
    case_insensitive: bool,
) -> Result<(PathBuf, String), FileResolveError> {
    // Reject null bytes before any further processing.  A null byte in the
    // path could bypass the `.md` extension check on some platforms because
    // the OS treats the string as ending at the first `\0`.
    if path_arg.contains('\0') {
        return Err(FileResolveError::InvalidPath {
            path: path_arg.to_owned(),
            reason: "contains null byte",
        });
    }

    let mut normalized = normalize_path(path_arg);

    // Strip the dir prefix if present.  Users often pass CWD-relative paths
    // like `hyalo-knowledgebase/foo.md` when dir is `hyalo-knowledgebase`.
    // This is equivalent to `foo.md` — strip it before any further checks.
    if let Some(stripped) = strip_dir_prefix(dir, &normalized) {
        normalized = stripped;
    }

    // Reject path traversal attempts — use `OutsideVault` so the user
    // understands the path was rejected because it escapes the vault, not
    // because the file doesn't exist. `has_unsafe_windows_colon` catches the
    // Windows drive-relative (`C:foo`) and NTFS-ADS (`a.md:stream`) cases
    // that `is_absolute()` and `has_parent_traversal()` alone do not (M-2).
    if normalized.starts_with('/')
        || Path::new(&normalized).is_absolute()
        || has_unsafe_windows_colon(&normalized)
    {
        return Err(FileResolveError::OutsideVault {
            path: normalized,
            resolved: None,
        });
    }

    // A `..` component is rejected lexically, before resolution — even when
    // the path would land back inside the vault (e.g. `../sub/note.md` from
    // `sub/`). This is a *policy* rejection (no `..` allowed, ever), not a
    // claim that the path escapes the vault, so it gets its own error variant
    // rather than reusing `OutsideVault` (F3-4 / DEC-094): the two have
    // different, non-interchangeable honest messages — see
    // `FileResolveError`'s `Display` impl.
    if has_parent_traversal(&normalized) {
        return Err(FileResolveError::ParentTraversal { path: normalized });
    }

    // A trailing slash is directory syntax. It must be trimmed before any hint
    // is built, or the glob hint comes out as `sub//*` — a pattern that matches
    // nothing, so copy-pasting it produces a clean-looking exit 0 on a
    // directory that was never linted (iter-210 / BUG-13).
    let bare = normalized.trim_end_matches('/');
    if !std::path::Path::new(bare)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        // If the path refers to a directory inside the vault, suggest --glob
        // instead of the misleading "{name}.md" hint.
        if !bare.is_empty() && dir.join(bare).is_dir() {
            let glob_hint = format!("--glob '{bare}/*'");
            return Err(FileResolveError::IsDirectory {
                path: normalized,
                hint: glob_hint,
            });
        }

        // Only suggest `{path}.md` when that file actually exists (iter-210 /
        // BUG-13). Suggesting a candidate blindly turned `nosuchdir/` into
        // "did you mean nosuchdir/.md?" — a path that can never exist. When
        // there is no such file, fall through to the ordinary not-found path
        // below, which still gets a chance to offer a fuzzy sibling.
        let candidate = format!("{bare}.md");
        if !bare.is_empty()
            && (dir.join(&candidate).is_file()
                || (case_insensitive && resolve_case_insensitive(dir, &candidate).is_some()))
        {
            return Err(FileResolveError::MissingExtension {
                path: normalized,
                hint: candidate,
            });
        }

        // Nothing plausible to suggest. A non-`.md` path is never resolvable as
        // a note, so report not-found here rather than falling through to the
        // generic lookup below (which would happily accept an on-disk
        // `notes.txt`). A trailing slash skips the fuzzy sibling search too:
        // `sub/` is directory syntax, and matching it against notes in the
        // parent directory produces nonsense suggestions.
        if !normalized.ends_with('/')
            && let Some(suggestion) = fuzzy_suggestion_for(dir, bare)
        {
            return Err(FileResolveError::NotFoundSuggestion {
                path: normalized,
                suggestion,
            });
        }
        return Err(FileResolveError::NotFound { path: normalized });
    }

    let mut full = dir.join(&normalized);
    if !full.is_file() {
        // Case-insensitive fallback: when the literal-casing lookup misses and
        // the caller opted into case-insensitive resolution, walk each path
        // component and match it against on-disk entries ignoring ASCII case.
        // Only accept a unique match at every level — an ambiguous level (two
        // entries differing only in case) is left to the literal `NotFound`.
        if case_insensitive
            && let Some((ci_full, ci_rel)) = resolve_case_insensitive(dir, &normalized)
        {
            full = ci_full;
            normalized = ci_rel;
        }
    }
    if !full.is_file() {
        // Try fuzzy-matching against .md siblings in the same parent directory.
        if let Some(suggestion) = fuzzy_suggestion_for(dir, &normalized) {
            return Err(FileResolveError::NotFoundSuggestion {
                path: normalized,
                suggestion,
            });
        }
        return Err(FileResolveError::NotFound { path: normalized });
    }

    // After confirming the file exists, canonicalize to resolve symlinks and
    // verify the real path stays within the vault directory.
    let canonical_dir = canonicalize_vault_dir(dir).map_err(|_| FileResolveError::NotFound {
        path: normalized.clone(),
    })?;
    match ensure_within_vault(&canonical_dir, &full) {
        Ok(true) => {}
        Ok(false) => {
            // Report where the path really lands, not just what the user typed
            // (iter-202 L-16): a symlink escape is far easier to understand
            // with both halves shown.
            return Err(FileResolveError::OutsideVault {
                path: normalized.clone(),
                resolved: dunce::canonicalize(&full)
                    .ok()
                    .map(|p| p.display().to_string()),
            });
        }
        Err(_) => {
            // Canonicalization of the target failed (permission error, symlink loop, etc.).
            // Do not claim "outside vault" — the path simply could not be resolved.
            return Err(FileResolveError::NotFound { path: normalized });
        }
    }

    // The file exists and is inside the vault, but `[scan] exclude` says no
    // command should see it (iter-265, DEC-277). Refuse loudly: reporting the
    // glob is the only way the caller learns why a file they can see on disk
    // is invisible to hyalo.
    if let Some(glob) = scan_exclude_glob(&normalized) {
        return Err(FileResolveError::ScanExcluded {
            path: normalized,
            glob: glob.to_owned(),
        });
    }

    Ok((full, normalized))
}

/// Resolve a vault-relative path case-insensitively by walking components.
///
/// For each component of `rel` (forward-slash form), scan the corresponding
/// on-disk directory and pick the entry whose name equals the component under
/// ASCII case-folding. Requires exactly one such entry at every level — if a
/// level is ambiguous (e.g. both `Foo` and `foo` exist) or missing, returns
/// `None` and the caller keeps the literal `NotFound`.
///
/// Returns the real-casing full path and its vault-relative (forward-slash)
/// form. Does no vault-boundary check itself — the caller re-runs the same
/// `ensure_within_vault` guard on the result.
fn resolve_case_insensitive(dir: &Path, rel: &str) -> Option<(PathBuf, String)> {
    // Empty or already-literal-matching inputs are handled by the caller.
    if rel.is_empty() {
        return None;
    }

    let mut current = dir.to_path_buf();
    let mut real_components: Vec<String> = Vec::new();

    for component in rel.split('/') {
        if component.is_empty() {
            return None;
        }

        // Case-insensitive scan of `current` for a unique match. We always scan
        // (rather than short-circuiting on an exact-casing `is_file`) so that on
        // a case-insensitive host FS we still recover the *true* on-disk casing
        // instead of echoing the caller's argument casing. Prefer an exact-case
        // hit when one is present alongside case-variants.
        let entries = std::fs::read_dir(&current).ok()?;
        let mut exact: Option<String> = None;
        let mut ci_matches: Vec<String> = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name_str = name.to_str()?;
            if name_str == component {
                exact = Some(name_str.to_owned());
            } else if name_str.eq_ignore_ascii_case(component) {
                ci_matches.push(name_str.to_owned());
            }
        }
        let on_disk_name = match (exact, ci_matches.len()) {
            // Exact-casing entry present — always prefer it.
            (Some(e), _) => e,
            // Exactly one case-variant — use it.
            (None, 1) => ci_matches.into_iter().next()?,
            // Zero or ambiguous (>1) — cannot resolve.
            (None, _) => return None,
        };
        current = current.join(&on_disk_name);
        real_components.push(on_disk_name);
    }

    Some((current, real_components.join("/")))
}

/// Vault-relative "did you mean" suggestion for a path that did not resolve.
///
/// Looks for the closest `.md` sibling of `rel` (a vault-relative,
/// forward-slash path) and rebuilds the answer as a vault-relative path, so a
/// miss inside a subdirectory suggests `sub/readme.md` rather than a bare
/// `readme.md` the caller cannot paste back.
///
/// Shared by the missing-extension and the plain not-found paths so both offer
/// the same suggestion quality (iter-210 / BUG-13).
fn fuzzy_suggestion_for(dir: &Path, rel: &str) -> Option<String> {
    let sibling_name = fuzzy_match_sibling(&dir.join(rel))?;
    Some(match Path::new(rel).parent() {
        Some(parent) if parent != Path::new("") => {
            format!("{}/{sibling_name}", parent.display())
        }
        _ => sibling_name,
    })
}

/// Find the closest `.md` sibling by Levenshtein distance.
///
/// Returns the file-name of the best match when the distance is at most 3,
/// or `None` when nothing is close enough.
fn fuzzy_match_sibling(full: &Path) -> Option<String> {
    let parent = full.parent()?;
    let target_name = full.file_name()?.to_str()?;

    let entries = std::fs::read_dir(parent).ok()?;
    let mut best: Option<(usize, String)> = None;

    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        let is_md = std::path::Path::new(name_str)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if !is_md || name_str == target_name {
            continue;
        }
        let dist = levenshtein(target_name, name_str);
        if dist <= 3 && best.as_ref().is_none_or(|(d, _)| dist < *d) {
            best = Some((dist, name_str.to_owned()));
        }
    }

    best.map(|(_, name)| name)
}

/// Return true if the path contains any `..` (parent directory) component.
/// This is the correct way to detect path traversal — checking for the `..`
/// component directly rather than a substring match, which incorrectly rejects
/// legitimate filenames like `etc..md`.
pub fn has_parent_traversal(path: &str) -> bool {
    Path::new(path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Return true if `path` contains a colon in a way that is unsafe on
/// Windows: either a drive-relative prefix (`C:foo` — note *no* separator
/// after the colon, unlike `C:\foo`) or an NTFS Alternate Data Stream marker
/// (`a.md:stream`).
///
/// `C:foo` is drive-*relative*: `Path::is_absolute()` returns `false` for it
/// (there is no root component, only a `Prefix`), so it slips past an
/// `is_absolute()` + `has_parent_traversal()` boundary check and later
/// resolves against the process's current directory on that drive —
/// potentially outside the vault. `a.md:stream` is lexically an ordinary
/// in-vault filename (Rust's generic path parser does not split on `:`
/// inside a component), but the OS resolves it to an alternate data stream
/// on `a.md` rather than the file itself — a silent wrong-target write, not
/// an escape, but still not what the caller meant (M-2,
/// adversarial-review-2026-08-23.md).
///
/// A colon is an ordinary, harmless filename character on non-Windows
/// platforms (and in fact a legal one — some Unix filesystems allow it), so
/// this check only applies on Windows; it is a compile-time no-op
/// everywhere else.
#[must_use]
pub fn has_unsafe_windows_colon(path: &str) -> bool {
    #[cfg(windows)]
    {
        path.contains(':')
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Canonicalize the vault directory once.
///
/// Callers that invoke `ensure_within_vault` in a loop should call this once
/// upfront and pass the result to every `ensure_within_vault` call, avoiding
/// repeated canonicalization of the same directory.
pub fn canonicalize_vault_dir(dir: &Path) -> Result<PathBuf> {
    dunce::canonicalize(dir)
        .with_context(|| format!("failed to canonicalize vault dir: {}", dir.display()))
}

/// Verify that `full` resolves to a path within `canonical_dir` after following symlinks.
///
/// Accepts an already-canonicalized vault directory to avoid re-canonicalizing
/// on every call (important when called in a per-link loop).
///
/// Returns:
/// - `Ok(true)`  — `full` is within the vault
/// - `Ok(false)` — `full` resolves outside the vault boundary
/// - `Err(_)`    — `full` could not be canonicalized (permission error, symlink loop, etc.)
pub(crate) fn ensure_within_vault(canonical_dir: &Path, full: &Path) -> Result<bool> {
    let canonical_full = dunce::canonicalize(full)
        .with_context(|| format!("failed to canonicalize path: {}", full.display()))?;
    Ok(canonical_full.starts_with(canonical_dir))
}

/// If `path_arg` is an absolute path that lies inside the canonical vault `dir`,
/// return the equivalent vault-relative path. Otherwise return `None`.
///
/// Lets the CLI rewrite an absolute `--file` path (which LLM-driven shells
/// often pass) into the relative form `resolve_file` expects, while keeping
/// genuinely-out-of-vault absolute paths unchanged so they still hit
/// `OutsideVault`.
///
/// Returns `None` if:
/// - the input is not absolute (caller can pass it straight through),
/// - the vault dir cannot be canonicalized,
/// - the (possibly canonicalized) input does not start with the canonical vault,
/// - or the path equals the vault dir itself (no remainder).
#[must_use]
pub fn strip_absolute_vault_prefix(dir: &Path, path_arg: &str) -> Option<String> {
    let p = Path::new(path_arg);
    if !p.is_absolute() {
        return None;
    }
    let canonical_dir = canonicalize_vault_dir(dir).ok()?;
    // Prefer canonicalized input (resolves symlinks, `..`, etc.); fall back to
    // the literal path so non-existent files inside the vault still rewrite.
    let candidate = dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let stripped = candidate.strip_prefix(&canonical_dir).ok()?;
    // Reject leftover parent-traversal segments. When canonicalize falls back
    // to the literal path, a string like `/vault/../vault/x.md` can survive
    // strip_prefix with a `..` in the remainder; we must not hand that to
    // resolve_file as if it were a clean vault-relative path.
    if stripped
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    let s = stripped.to_string_lossy().replace('\\', "/");
    if s.is_empty() { None } else { Some(s) }
}

/// Strip the vault `dir` prefix from a path if present.
///
/// Compares path components so `dir = "docs"` matches the leading `docs/` in
/// `docs/notes/foo.md` but does NOT match `docs-old/foo.md`.  Returns the
/// remaining path after the prefix, or `None` if the path doesn't start with
/// the `dir` components.
///
/// When `dir` is an absolute path (e.g. `/home/user/docs`), only the last
/// component (`docs`) is used for matching — the user's `--file` argument is
/// always relative, so only the directory name matters.
pub fn strip_dir_prefix(dir: &Path, normalized: &str) -> Option<String> {
    let norm_path = Path::new(normalized);

    // For relative dirs (the common case from .hyalo.toml), try a direct
    // component-wise strip.  For absolute dirs, fall back to the last
    // component so that e.g. dir="/tmp/kb" matches "kb/note.md".
    let stripped = norm_path.strip_prefix(dir).ok().or_else(|| {
        let last = dir.file_name()?;
        norm_path.strip_prefix(last).ok()
    })?;

    let s = stripped.to_string_lossy().replace('\\', "/");
    if s.is_empty() { None } else { Some(s) }
}

/// Normalize a path argument: strip leading `./`, normalize separators to forward slashes.
fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}

/// Check if a path argument is a glob or negation pattern.
///
/// Returns `true` for paths containing `*`, `?`, `[`, or a leading `!`
/// (negation glob).
#[must_use]
#[allow(dead_code)] // Used in tests only
pub(crate) fn is_glob(path: &str) -> bool {
    path.starts_with('!')
        || path.starts_with("\\!")
        || path.contains('*')
        || path.contains('?')
        || path.contains('[')
}

/// Match discovered files against a glob pattern.
///
/// If `pattern` starts with `!`, it is treated as a negation: all discovered
/// files are returned **except** those matching the remainder of the pattern.
///
/// Positive patterns (no `!` prefix) work as before — only files matching the
/// pattern are returned.
///
/// The glob is matched against paths relative to `dir`.
#[allow(dead_code)] // Used in tests only
pub(crate) fn match_glob(
    dir: &Path,
    files: &[PathBuf],
    pattern: &str,
) -> Result<Vec<(PathBuf, String)>> {
    // Normalize `\!` → `!` so that shell-escaped negation globs work.
    // Some shells (and Claude Code's Bash tool) escape `!` to `\!` even
    // inside single quotes.
    let normalized;
    let pattern = if let Some(rest) = pattern.strip_prefix("\\!") {
        normalized = format!("!{rest}");
        normalized.as_str()
    } else {
        pattern
    };

    if let Some(neg_pattern) = pattern.strip_prefix('!') {
        anyhow::ensure!(
            !neg_pattern.is_empty(),
            "negation glob pattern must not be empty (got '!')"
        );
        // Negation glob: return all files that do NOT match the pattern.
        let glob = GlobBuilder::new(neg_pattern)
            .literal_separator(true)
            .build()
            .context("invalid glob negation pattern")?
            .compile_matcher();

        let mut matched = Vec::new();
        for file in files {
            let rel = relative_path(dir, file);
            if !glob.is_match(&rel) {
                matched.push((file.clone(), rel));
            }
        }
        return Ok(matched);
    }

    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .context("invalid glob pattern")?
        .compile_matcher();

    let mut matched = Vec::new();
    for file in files {
        let rel = relative_path(dir, file);
        if glob.is_match(&rel) {
            matched.push((file.clone(), rel));
        }
    }
    Ok(matched)
}

/// Match discovered files against multiple glob patterns.
///
/// Patterns prefixed with `!` (or `\!`) are treated as negations.
/// - If there are any positive patterns, a file must match at least one.
/// - If there are no positive patterns, all files start as candidates.
/// - A file is excluded if it matches any negative pattern.
///
/// The glob is matched against paths relative to `dir`.
pub fn match_globs(
    dir: &Path,
    files: &[PathBuf],
    patterns: &[String],
) -> Result<Vec<(PathBuf, String)>> {
    // Normalize `\!` → `!` for each pattern
    let normalized: Vec<String> = patterns
        .iter()
        .map(|p| {
            if let Some(rest) = p.strip_prefix("\\!") {
                format!("!{rest}")
            } else {
                p.clone()
            }
        })
        .collect();

    // Separate into positive and negative patterns
    let mut positive: Vec<&str> = Vec::new();
    let mut negative: Vec<&str> = Vec::new();
    for p in &normalized {
        if let Some(neg) = p.strip_prefix('!') {
            anyhow::ensure!(
                !neg.is_empty(),
                "negation glob pattern must not be empty (got '!')"
            );
            negative.push(neg);
        } else {
            positive.push(p.as_str());
        }
    }

    // Build the positive GlobSet (empty means "match all")
    let positive_set = if positive.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &positive {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build positive globset")?,
        )
    };

    // Build the negative GlobSet
    let negative_set = if negative.is_empty() {
        None
    } else {
        let mut builder = GlobSetBuilder::new();
        for pat in &negative {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .context("invalid glob negation pattern")?,
            );
        }
        Some(
            builder
                .build()
                .context("failed to build negative globset")?,
        )
    };

    let mut matched = Vec::new();
    for file in files {
        let rel = relative_path(dir, file);
        let passes_positive = positive_set.as_ref().is_none_or(|gs| gs.is_match(&rel));
        let passes_negative = negative_set.as_ref().is_none_or(|gs| !gs.is_match(&rel));
        if passes_positive && passes_negative {
            matched.push((file.clone(), rel));
        }
    }
    Ok(matched)
}

/// Get the relative path of a file from a directory, using forward slashes on all platforms.
#[must_use]
pub fn relative_path(dir: &Path, file: &Path) -> String {
    let raw = file.strip_prefix(dir).map_or_else(
        |_| file.to_string_lossy().to_string(),
        |p| p.to_string_lossy().to_string(),
    );
    // Normalize to forward slashes for consistent output and glob matching on Windows.
    raw.replace('\\', "/")
}

/// Errors specific to file resolution.
#[derive(Debug)]
pub enum FileResolveError {
    NotFound {
        path: String,
    },
    NotFoundSuggestion {
        path: String,
        suggestion: String,
    },
    MissingExtension {
        path: String,
        hint: String,
    },
    IsDirectory {
        path: String,
        hint: String,
    },
    OutsideVault {
        path: String,
        /// Canonical destination the path escaped to, when resolution (symlink
        /// following) produced one. `None` for a purely lexical rejection
        /// (absolute path, Windows drive-relative, or NTFS ADS marker).
        resolved: Option<String>,
    },
    /// The path contains a `..` component and was rejected lexically, before
    /// any resolution was attempted. Distinct from [`Self::OutsideVault`]
    /// (F3-4): a `..`-bearing path may well resolve *inside* the vault (e.g.
    /// `../sub/note.md` from `sub/`), so claiming it "resolves outside vault
    /// boundary" would be false. The real policy is narrower — no `..`
    /// component is ever accepted, regardless of where it would land — and
    /// this variant's `Display` says exactly that instead.
    ParentTraversal {
        path: String,
    },
    /// The path resolves to a real vault file that `[scan] exclude` drops
    /// (iter-265, DEC-277). An explicitly named target is *refused* rather
    /// than silently skipped: a script that asked for one specific file and
    /// got a clean exit 0 would read "excluded" as "nothing wrong here".
    ScanExcluded {
        path: String,
        /// The `[scan] exclude` glob that matched, so the message says which
        /// line of `.hyalo.toml` to change.
        glob: String,
    },
    InvalidPath {
        path: String,
        reason: &'static str,
    },
}

impl std::fmt::Display for FileResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound { path } => write!(f, "file not found: {path}"),
            Self::NotFoundSuggestion { path, suggestion } => {
                write!(f, "file not found: {path} (did you mean {suggestion}?)")
            }
            Self::MissingExtension { path, hint } => {
                write!(f, "file not found: {path} (did you mean {hint}?)")
            }
            Self::IsDirectory { path, hint } => {
                write!(f, "path is a directory, not a file: {path} (try {hint})")
            }
            Self::OutsideVault {
                path,
                resolved: Some(target),
            } => {
                write!(
                    f,
                    "file resolves outside vault boundary: {path} -> {target}"
                )
            }
            Self::OutsideVault {
                path,
                resolved: None,
            } => {
                write!(f, "file resolves outside vault boundary: {path}")
            }
            Self::ParentTraversal { path } => {
                write!(
                    f,
                    "path contains '..' and is rejected: {path} (paths must be \
                     vault-relative without '..' components, even when the target \
                     is inside the vault — use the vault-relative form instead, \
                     e.g. \"broken.md\" or \"sub/broken.md\", not \"../broken.md\")"
                )
            }
            Self::ScanExcluded { path, glob } => {
                write!(
                    f,
                    "file is excluded by [scan] exclude = [\"{glob}\"]: {path} \
                     (remove the glob from .hyalo.toml, or narrow it, to operate on this file)"
                )
            }
            Self::InvalidPath { path, reason } => {
                write!(f, "invalid path ({reason}): {path}")
            }
        }
    }
}

impl std::error::Error for FileResolveError {}

/// Normalize a link's *target string* to the vault-relative form used for
/// resolution, applying the kind-dependent branching shared by the read-side
/// entry points (`resolve_link_from_source` / `classify_link_from_source`).
///
/// This is the single owner of the wikilink/markdown/site-absolute/path-
/// qualified/bare-basename branching so the Exists and Classify modes can
/// never drift apart again (iter-189 task 2).
///
/// - Wikilinks are vault-relative by definition, returned as-written.
/// - Markdown site-absolute (`/site/...`) targets are returned as-written.
/// - Markdown path-qualified targets are normalized against the source dir.
/// - Markdown bare basenames try source-relative first, falling back to the
///   raw target. The two modes differ only in *how* they decide whether the
///   source-relative candidate "resolves", so that decision is abstracted
///   behind `src_rel_resolves` — the sole intentional behavioral seam between
///   Exists and Classify (locked by tests in iter-189 task 1):
///   - Exists mode passes `|c| resolve_target(...).is_some()`.
///   - Classify mode passes `|c| !matches!(classify_link(...), Broken)`.
fn normalize_link_target<'a>(
    kind: crate::links::LinkKind,
    source_rel: &str,
    target: &'a str,
    src_rel_resolves: impl FnOnce(&str) -> bool,
) -> std::borrow::Cow<'a, str> {
    use crate::link_graph::normalize_target;
    use crate::links::LinkKind;

    match kind {
        LinkKind::Wikilink => std::borrow::Cow::Borrowed(target),
        LinkKind::Markdown => {
            if target.starts_with('/') {
                std::borrow::Cow::Borrowed(target)
            } else if target.contains('/') || target.contains('\\') {
                let mut norm = normalize_target(Path::new(source_rel), target);
                // iter-211 / BUG-10: `normalize_path_components` drops a
                // trailing slash, erasing the one signal that makes `foo/` an
                // *explicit* directory reference. Without this, the relative
                // spelling `[a](foo/)` resolved to `foo.md` while the
                // site-absolute `[b](/foo/)` — which never goes through
                // normalization — resolved to `foo/index.md`, so one file was
                // a backlink of the other's target. Re-attach it and let
                // `resolve_target` apply the documented precedence once.
                if (target.ends_with('/') || target.ends_with('\\'))
                    && !norm.is_empty()
                    && !norm.ends_with('/')
                {
                    norm.push('/');
                }
                std::borrow::Cow::Owned(norm)
            } else {
                // Bare basename: try source-relative first so same-folder links
                // resolve correctly, then fall back to the raw target.
                let src_rel = normalize_target(Path::new(source_rel), target);
                if src_rel_resolves(&src_rel) {
                    std::borrow::Cow::Owned(src_rel)
                } else {
                    std::borrow::Cow::Borrowed(target)
                }
            }
        }
    }
}

/// Whether an already-normalized link target points outside the scanned vault.
///
/// After [`normalize_link_target`] has resolved `.`/`..` components, a target
/// that still starts with `..` could only be reached by walking above the
/// vault root — it is out of scope rather than broken (iter-193).
///
/// Note the deliberate narrowness: a site-absolute target (`/src/foo.md`) is
/// **not** classified here, because a vault that *is* the site root makes such
/// a link a genuine miss, and silently hiding those would be worse than the
/// noise it saves.
#[must_use]
pub fn normalized_target_escapes_vault(normalized: &str) -> bool {
    normalized == ".." || normalized.starts_with("../")
}

/// Whether a link written in `source_rel` points outside the scanned vault.
///
/// Applies the same kind-dependent normalization as the read-side resolvers,
/// then asks [`normalized_target_escapes_vault`]. Touches no filesystem.
#[must_use]
pub fn link_target_escapes_vault(
    source_rel: &str,
    kind: crate::links::LinkKind,
    target: &str,
) -> bool {
    // `|_| false` for the bare-basename seam: an unresolvable bare basename
    // falls back to the raw target, which has no path separators and so can
    // never escape the vault.
    let normalized = normalize_link_target(kind, source_rel, target, |_| false);
    normalized_target_escapes_vault(normalized.as_ref())
}

/// Resolve a single parsed link (from `source_rel`) to a vault-relative path,
/// or `None` when it does not resolve to a known vault file.
///
/// This is the shared **Exists**-mode entry point (iter-188 task 0,
/// `ResolveMode::Exists`): "does this link resolve to a vault file?" It
/// centralizes the kind-dependent normalization — via the shared
/// [`normalize_link_target`] helper — that `find --broken-links` and the
/// HYALO006 lint rule both need, so neither has to reimplement the
/// wikilink/markdown/site-absolute branching or call `resolve_target` directly.
///
/// Its Classify-mode sibling is [`classify_link_from_source`], which returns
/// the full fix-policy verdict (case/short-form buckets) instead of a plain
/// resolve/not-resolve answer; both route through the same normalization helper.
///
/// - Wikilinks are vault-relative by definition, resolved as-written.
/// - Markdown site-absolute (`/site/...`) targets are resolved as-written.
/// - Markdown path-qualified targets are normalized against the source dir.
/// - Markdown bare basenames try source-relative first, then fall back to the
///   raw target (matching the pre-existing `find` behavior).
#[must_use]
pub fn resolve_link_from_source(
    canonical_dir: &Path,
    source_rel: &str,
    kind: crate::links::LinkKind,
    target: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Option<String> {
    let resolved = normalize_link_target(kind, source_rel, target, |src_rel| {
        resolve_target(canonical_dir, src_rel, site_prefix, case_index).is_some()
    });
    resolve_target(canonical_dir, resolved.as_ref(), site_prefix, case_index).or_else(|| {
        // iter-261 / BUG-6: a partially-qualified attachment wikilink
        // (`![[sub/img.png]]`) is also resolved relative to the source folder,
        // the way Obsidian does. Pure fallback — `None` for every target
        // without an explicit non-`.md` extension.
        resolve_attachment_from_source(
            canonical_dir,
            source_rel,
            kind,
            target,
            site_prefix,
            case_index,
        )
    })
}

// ---------------------------------------------------------------------------
// Classify mode — full fix-policy verdict (iter-189 task 2)
// ---------------------------------------------------------------------------
//
// The Classify-mode entry point [`classify_link_from_source`] is the sibling of
// the Exists-mode [`resolve_link_from_source`]. Both route through the shared
// [`normalize_link_target`] helper so the kind-dependent normalization can never
// drift between the two modes. Classify additionally distinguishes the buckets
// the `links fix` command needs (case-mismatch, short-form valid/mismatch/
// ambiguous, broken); Exists only answers resolve/not-resolve.
//
// Both are *read-side* resolvers. The *rewrite-side* resolver that plans how to
// rewrite links when files move / titles are auto-linked lives separately in
// `crate::link_resolve::LinkResolver`; it is not a duplicate of these two.

/// The Classify-mode verdict for a single link's resolution against the
/// filesystem and an optional case-insensitive index.
///
/// Returns:
/// - `Resolved(None)` — link resolves exactly and its on-disk casing matches
///   the canonical form (or no index was supplied).
/// - `Resolved(Some(canonical))` — link resolves exactly but the on-disk
///   casing differs from the canonical form (case-insensitive filesystem
///   papered over a mismatch); caller should record as a case-mismatch.
/// - `CaseMismatch(canonical)` — exact resolution failed but the case index
///   found a unique canonical path that differs from the written target only
///   in ASCII case; caller should record as a case-mismatch.
/// - `StemRelocation(canonical)` — exact resolution failed and the rescue came
///   from the bare-stem fallback, so the canonical path is in a *different
///   place*, not merely cased differently (iter-211 / BUG-12). Reporting these
///   as `CaseMismatch` printed `[link-case-mismatch]` next to an old and new
///   target that differ by a whole directory.
/// - `ShortFormValid` — a short-form wikilink whose stem resolves to exactly
///   one file in the vault with matching casing; nothing to fix.
/// - `ShortFormStemMismatch(correct_stem)` — a short-form wikilink whose stem
///   resolves to exactly one file, but the written casing of the stem differs
///   from the on-disk filename stem; `new_target` is the corrected stem
///   (never a path — never expanded).
/// - `ShortFormAmbiguous` — a short-form wikilink whose stem matches ≥2 files.
/// - `Broken` — nothing resolves.
#[derive(PartialEq)]
pub(crate) enum LinkResolution {
    Resolved(Option<String>),
    CaseMismatch(String),
    StemRelocation(String),
    ShortFormValid,
    ShortFormStemMismatch(String),
    ShortFormAmbiguous,
    Broken,
}

/// Precomputed case-insensitive stem → candidate paths map used to resolve
/// short-form wikilinks when no [`CaseInsensitiveIndex`] is available.
/// Built once per `detect_broken_links*` call so each lookup is O(1).
pub(crate) struct StemIndex {
    map: std::collections::HashMap<String, Vec<String>>,
}

impl StemIndex {
    pub(crate) fn build(vault_files: &[String]) -> Self {
        let mut map: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for path in vault_files {
            let fname = path.rsplit('/').next().unwrap_or(path.as_str());
            let stem = fname.strip_suffix(".md").unwrap_or(fname);
            map.entry(stem.to_ascii_lowercase())
                .or_default()
                .push(path.clone());
        }
        Self { map }
    }

    fn lookup(&self, stem: &str) -> Vec<&str> {
        self.map
            .get(&stem.to_ascii_lowercase())
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }
}

/// Classify a short-form wikilink target (no `/`) against the vault's stem
/// index.  Returns a `LinkResolution` that covers valid, stem-case-mismatch,
/// ambiguous, and broken cases without ever producing a full path.
///
/// When `expand_short_form` is `true`, the caller has opted into path
/// expansion — skip the short-form special handling and let the caller fall
/// through to regular path-based classification.
fn classify_short_form_wikilink(
    target: &str,
    stem_index: &StemIndex,
    case_index: Option<&CaseInsensitiveIndex>,
    expand_short_form: bool,
) -> Option<LinkResolution> {
    if expand_short_form {
        return None; // caller should use regular path-based classification
    }

    // Only apply to bare stems (no directory separator). Wikilinks with an
    // explicit `.md` extension (e.g. `[[Note.md]]`) are path-like targets;
    // let the caller handle them via regular path-based classification rather
    // than mismatching them as stem lookups against `"Note.md"`.
    if target.contains('/') || target.contains('\\') {
        return None;
    }
    // Skip wikilinks with an explicit `.md` extension (case-insensitive),
    // which are path-like targets and should go through path-based handling.
    if Path::new(target)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return None;
    }

    // Look up the stem case-insensitively. Prefer the case_index when
    // available (O(1) hash lookup); otherwise use the precomputed
    // per-invocation stem index built from `vault_files`.
    let matches: Vec<&str> = if let Some(idx) = case_index {
        idx.lookup_stem_all(target)
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        stem_index.lookup(target)
    };

    // iter-272 Part B (DEC-288): no file carries that stem — but a note may
    // declare it as a frontmatter alias, which is how Obsidian resolves
    // `[[Leah]]` to `Leah Ferguson.md`. An alias-resolved link is *valid as
    // written*: it needs no rewrite, so it is `ShortFormValid` rather than a
    // relocation, and it never reaches the fuzzy matcher — which used to offer
    // `Leah → Lewuathe.md` at confidence 0.87 on the Obsidian Hub. Two notes
    // claiming one alias is ambiguous, exactly like two files sharing a stem.
    if matches.is_empty()
        && let Some(idx) = case_index
    {
        match idx.lookup_alias_all(target).len() {
            0 => {}
            1 => return Some(LinkResolution::ShortFormValid),
            _ => return Some(LinkResolution::ShortFormAmbiguous),
        }
    }

    match matches.len() {
        0 => Some(LinkResolution::Broken),
        1 => {
            // Exactly one match — the link is valid. Check if the stem casing differs.
            let canonical_path = matches[0];
            let canonical_fname = canonical_path.rsplit('/').next().unwrap_or(canonical_path);
            let canonical_stem = canonical_fname
                .strip_suffix(".md")
                .unwrap_or(canonical_fname);

            if target == canonical_stem {
                Some(LinkResolution::ShortFormValid)
            } else {
                // Stem casing differs — propose the canonical stem (not a full path).
                Some(LinkResolution::ShortFormStemMismatch(
                    canonical_stem.to_string(),
                ))
            }
        }
        _ => Some(LinkResolution::ShortFormAmbiguous),
    }
}

/// Classify an already-normalized (vault-relative) target against the
/// filesystem and an optional case-insensitive index.
fn classify_link(
    canonical_dir: &Path,
    resolved_target: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> LinkResolution {
    let exact = resolve_target(canonical_dir, resolved_target, site_prefix, None);

    if let Some(exact_str) = exact {
        // Link resolves exactly. If we have a case index, also check whether the
        // resolved path has incorrect casing compared to the canonical on-disk
        // path. On case-insensitive filesystems, `exact` may contain the
        // user-written casing rather than the canonical casing.
        if let Some(idx) = case_index
            && let Some(canonical_path) =
                resolve_target(canonical_dir, resolved_target, site_prefix, Some(idx))
        {
            let canonical_fwd = canonical_path.replace('\\', "/");
            let exact_fwd = exact_str.replace('\\', "/");
            if exact_fwd != canonical_fwd {
                return LinkResolution::Resolved(Some(canonical_fwd));
            }
        }
        return LinkResolution::Resolved(None);
    }

    // Exact resolution failed. If we have a case index, try the
    // case-insensitive fallback. `resolve_target` already handles the `.md`
    // extension and directory-index fallbacks internally, and — for bare,
    // non-site-absolute targets — an Obsidian stem lookup anywhere in the
    // vault. The first three are casing/spelling differences on the *same*
    // path; the last one is a relocation, and iter-211 / BUG-12 keeps them
    // apart so the caller can label and gate them honestly.
    if let Some(idx) = case_index
        && let Some(canonical_path) =
            resolve_target(canonical_dir, resolved_target, site_prefix, Some(idx))
    {
        let canonical = canonical_path.replace('\\', "/");
        if is_case_only_variant(resolved_target, &canonical) {
            return LinkResolution::CaseMismatch(canonical);
        }
        return LinkResolution::StemRelocation(canonical);
    }

    LinkResolution::Broken
}

/// Whether `canonical` is the same path as `written` up to ASCII case and the
/// spelling fallbacks `resolve_target` applies on the *same* path (`.md`
/// suffix, `/index.md` directory index, trailing slash).
///
/// Used to tell a genuine case-mismatch apart from a bare-stem relocation
/// (iter-211 / BUG-12): `Foo.MD` → `foo.md` is a casing fix, `foo.md` →
/// `sub/foo.md` is a move.
fn is_case_only_variant(written: &str, canonical: &str) -> bool {
    let w = written.trim_end_matches('/');
    if w.is_empty() {
        return false;
    }
    canonical.eq_ignore_ascii_case(w)
        || canonical.eq_ignore_ascii_case(&format!("{w}.md"))
        || canonical.eq_ignore_ascii_case(&format!("{w}/{DIRECTORY_INDEX_FILE}"))
}

/// Resolve a link's target to a vault-relative path and classify it — the
/// **Classify**-mode entry point (iter-189 task 2), sibling of
/// [`resolve_link_from_source`].
///
/// Where Exists mode answers only "does this link resolve to a vault file?",
/// Classify mode returns the full fix-policy verdict: the case-mismatch and
/// short-form (valid / stem-mismatch / ambiguous) buckets that the `links fix`
/// command needs. Both modes route the kind-dependent normalization through the
/// shared [`normalize_link_target`] helper so they can never drift apart.
///
/// `stem_index` is the flat stem lookup used for short-form wikilink resolution
/// when `case_index` is `None`.
///
/// `expand_short_form` — when `true`, skip Obsidian short-form handling and
/// fall through to regular path-based classification (opt-in via
/// `--expand-short-form`).
pub(crate) fn classify_link_from_source(
    canonical_dir: &Path,
    source_rel: &str,
    link: &crate::links::Link,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
    stem_index: &StemIndex,
    expand_short_form: bool,
) -> (String, LinkResolution) {
    use crate::links::LinkKind;

    // iter-261 / BUG-5, BUG-6: a target with an explicit non-`.md` extension is
    // an attachment reference. If it resolves to a real vault file it is simply
    // fine — never a case-mismatch to "correct", never a relocation, and never
    // (see `link_score`) a fuzzy `.base → .md` candidate.
    if has_non_md_extension(&link.target)
        && let Some(path) = resolve_link_from_source(
            canonical_dir,
            source_rel,
            link.kind,
            &link.target,
            site_prefix,
            case_index,
        )
    {
        return (path, LinkResolution::Resolved(None));
    }

    match link.kind {
        LinkKind::Wikilink => {
            // For short-form wikilinks (no `/`), apply Obsidian stem resolution first.
            // This prevents `resolve_target`'s internal stem lookup (inside classify_link)
            // from misidentifying a valid short-form link as a CaseMismatch.
            //
            // Strategy (when !expand_short_form):
            // 1. Try strict path-only check (no case_index) to catch vault-root exact files.
            // 2. If path-only resolves → check for case mismatch via the full classify_link.
            // 3. If path-only fails → use stem classification to determine the correct verdict.
            //
            // When expand_short_form=true: bypass stem classification entirely and use the
            // regular classify_link path, which may expand short-form via stem resolution.
            if !link.target.contains('/') && !link.target.contains('\\') {
                if expand_short_form {
                    // `--expand-short-form` opted into old path-expansion behavior.
                    // Check path-only (no index) so that the internal stem lookup in
                    // `resolve_target` cannot silently turn `[[Corina]]` into
                    // `CaseMismatch("sub/Corina.md")` — we want it to be `Broken`
                    // when `Corina.md` doesn't exist at the vault root, so that
                    // `plan_fixes` can then suggest the full path `[[sub/Corina]]`.
                    let res = classify_link(canonical_dir, &link.target, site_prefix, None);
                    return (link.target.clone(), res);
                }
                // Strategy (when !expand_short_form):
                // 1. Try strict path-only check (no case_index) to catch vault-root exact files.
                // 2. If path-only resolves → check for case mismatch via the full classify_link.
                // 3. If path-only fails → use stem classification to determine the correct verdict.
                let path_only = classify_link(canonical_dir, &link.target, site_prefix, None);
                if let LinkResolution::Resolved(_) = path_only {
                    // File exists at the vault root (exact path). Re-run with full
                    // case_index to detect root-file casing mismatches (e.g. [[corina]]
                    // for vault-root Corina.md) and keep the short form.
                    let full_res =
                        classify_link(canonical_dir, &link.target, site_prefix, case_index);
                    return (link.target.clone(), full_res);
                }
                // Path-only failed → use stem classification.
                if let Some(stem_res) = classify_short_form_wikilink(
                    &link.target,
                    stem_index,
                    case_index,
                    false, // expand_short_form already checked above
                ) {
                    return (link.target.clone(), stem_res);
                }
            }
            // Path-form link or classify_short_form_wikilink returned None (shouldn't
            // happen; it always returns Some when called with expand_short_form=false).
            // Fall through to the regular path-based classification.
            let res = classify_link(canonical_dir, &link.target, site_prefix, case_index);
            (link.target.clone(), res)
        }
        LinkKind::Markdown => {
            // Markdown normalization shares the Exists-mode branching via
            // `normalize_link_target`. The only Classify-specific seam is the
            // bare-basename fallback predicate: a source-relative candidate that
            // merely case-mismatches still counts as "resolved" and is preferred
            // over the raw target. To avoid classifying the source-relative
            // candidate twice (once in the predicate, once for the verdict), the
            // predicate caches its verdict in `probe_res`; when the helper picks
            // that candidate we reuse the cached verdict instead of re-running.
            let mut probe: Option<(String, LinkResolution)> = None;
            let resolved = normalize_link_target(link.kind, source_rel, &link.target, |src_rel| {
                let res = classify_link(canonical_dir, src_rel, site_prefix, case_index);
                let resolved = res != LinkResolution::Broken;
                probe = Some((src_rel.to_string(), res));
                resolved
            });
            // Reuse the cached verdict when the helper selected the exact
            // source-relative candidate the predicate classified.
            if let Some((probed_rel, probed_res)) = probe
                && resolved.as_ref() == probed_rel
            {
                return (resolved.into_owned(), probed_res);
            }
            let res = classify_link(canonical_dir, resolved.as_ref(), site_prefix, case_index);
            (resolved.into_owned(), res)
        }
    }
}

/// Percent-decode a URL path component (`%20` → space, `%2F` → `/`, …).
///
/// Returns `Some(decoded)` only when the input actually contained a valid,
/// UTF-8-clean escape sequence; returns `None` when there was nothing to decode
/// so callers can avoid an allocation on the common no-escape path.
///
/// If any escape is malformed (`%` not followed by two hex digits) or the
/// decoded bytes are not valid UTF-8, the **literal** input is preserved: we
/// return `None` (treat as "nothing safely decodable") rather than corrupt the
/// path. This keeps `[x](100%25done.md)` — a literal filename with a stray `%`
/// — resolving as written.
#[must_use]
pub(crate) fn percent_decode_path(input: &str) -> Option<String> {
    if !input.contains('%') {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let mut decoded_any = false;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Need two more hex digits.
            let hi = bytes.get(i + 1).copied().and_then(hex_val);
            let lo = bytes.get(i + 2).copied().and_then(hex_val);
            match (hi, lo) {
                (Some(h), Some(l)) => {
                    out.push((h << 4) | l);
                    i += 3;
                    decoded_any = true;
                    continue;
                }
                // Malformed escape: bail out, preserve the literal input.
                _ => return None,
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    if !decoded_any {
        return None;
    }
    // Decoded bytes must be valid UTF-8; otherwise keep the literal text.
    String::from_utf8(out).ok()
}

/// Map an ASCII hex digit byte to its 0–15 value.
#[must_use]
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Resolve one concrete candidate vault-relative path to an on-disk file.
///
/// Returns the canonical vault-relative path when `candidate` names a file
/// inside the vault, `None` otherwise. When `case_index` is supplied the
/// canonical on-disk casing is preferred over the caller's spelling, and an
/// unambiguous case-insensitive match is accepted when the literal path is
/// absent. Shared by the `<target>.md` and `<target>/index.md` attempts in
/// [`resolve_target`] so the two can never drift apart (iter-203).
fn resolve_candidate_path(
    canonical_dir: &Path,
    candidate: &str,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Option<String> {
    let full = canonical_dir.join(candidate);
    if full.is_file() {
        if ensure_within_vault(canonical_dir, &full).unwrap_or(false) {
            if let Some(idx) = case_index
                && let Some(canonical_path) = idx.lookup_unique(candidate)
            {
                return Some(canonical_path.to_owned());
            }
            return Some(candidate.to_owned());
        }
        return None;
    }
    if let Some(idx) = case_index
        && let Some(canonical_path) = idx.lookup_unique(candidate)
    {
        let full_resolved = canonical_dir.join(canonical_path);
        if ensure_within_vault(canonical_dir, &full_resolved).unwrap_or(false) {
            return Some(canonical_path.to_owned());
        }
    }
    None
}

/// The file name a directory link target resolves to (iter-203).
///
/// `foo`, `/foo` and `foo/` all resolve to `foo/index.md` — the directory-index
/// convention used by MDN, GitHub Docs and most static-site generators.
pub const DIRECTORY_INDEX_FILE: &str = "index.md";

/// Given a vault-relative path, return the directory a link may spell instead
/// of the file itself: `foo/index.md` → `Some("foo")` (iter-203).
///
/// Returns `None` when the path is not a directory index, or when it is the
/// vault-root `index.md` — that file has no directory name to stand in for it.
/// The `.md` suffix is matched case-insensitively, like everywhere else in
/// resolution.
#[must_use]
pub fn directory_for_index_file(rel: &str) -> Option<&str> {
    const SUFFIX_LEN: usize = "/index.md".len();
    if rel.len() <= SUFFIX_LEN {
        return None;
    }
    let (dir, suffix) = rel.split_at(rel.len() - SUFFIX_LEN);
    suffix.eq_ignore_ascii_case("/index.md").then_some(dir)
}

/// Resolve a link target to a file path relative to the vault root.
///
/// # Resolution order
///
/// 1. The target as written (exact path, then case-insensitive index).
/// 2. The target with `.md` appended (`foo` → `foo.md`).
/// 3. `<target>/index.md` — a target that names a directory resolves to that
///    directory's index file (iter-203).
/// 4. Obsidian-style bare-stem lookup (bare, non-site-absolute targets only).
///
/// Steps 2 and 3 swap places when the target was written with a **trailing
/// slash** (`foo/`), an explicit directory reference. So `foo` prefers
/// `foo.md` over `foo/index.md`, while `foo/` prefers `foo/index.md` — and
/// each still falls back to the other.
///
/// Returns the relative path if the file exists within the vault, or None.
///
/// `canonical_dir` must be a pre-canonicalized vault path (see `canonicalize_vault_dir`).
/// Callers iterating over many links should canonicalize once and reuse the result.
///
/// When `case_index` is provided, a case-insensitive fallback lookup is performed:
/// - If the literal path resolves, the canonical on-disk path from the index is returned
///   (correcting casing differences introduced on case-insensitive filesystems).
/// - If the literal path does NOT resolve, the index is consulted for an unambiguous
///   case-insensitive match and returned if found.
#[must_use]
pub fn resolve_target(
    canonical_dir: &Path,
    target: &str,
    site_prefix: Option<&str>,
    case_index: Option<&CaseInsensitiveIndex>,
) -> Option<String> {
    if target.is_empty() {
        return None;
    }

    // Normalize backslashes to forward slashes
    let mut target = target.replace('\\', "/");

    // Strip fragment (#...) and query string (?...) before resolution.
    // These are URL components that don't correspond to filesystem paths.
    if let Some(pos) = target.find('#') {
        target.truncate(pos);
    }
    if let Some(pos) = target.find('?') {
        target.truncate(pos);
    }
    // L-23: percent-decode the path portion so `[x](my%20dest.md)` resolves to
    // `my dest.md`. Decoding is applied uniformly (resolve_target is
    // kind-agnostic); wikilinks never contain percent-escapes in practice, so
    // this only affects markdown-style destinations. Invalid or non-UTF-8
    // escape sequences keep the literal text (see `percent_decode_path`).
    if let Some(decoded) = percent_decode_path(&target) {
        target = decoded;
    }
    // Remember whether the author wrote a trailing slash before it is stripped:
    // `/foo/` is unambiguously a *directory* reference, so the `.md`-append
    // attempt below must not apply to it (iter-203).
    let trailing_slash = target.len() > 1 && target.ends_with('/');
    // Strip trailing slash (e.g. "docs/page/" → "docs/page")
    while target.ends_with('/') && target.len() > 1 {
        target.pop();
    }
    if target.is_empty() {
        return None;
    }

    // Normalize absolute paths using site_prefix (same logic as LinkGraph).
    // `/docs/page.md` with site_prefix "docs" becomes `page.md`.
    //
    // Remember whether the link was written site-absolute: such a target names
    // a path from the site root, so the Obsidian bare-stem fallback at the end
    // of this function must not apply to it (iter-200 / dogfood M-1 — `/actions`
    // was "resolving" to `graphql/reference/actions.md` at confidence 1.0 and
    // getting rewritten by a plain `links fix --apply`).
    let site_absolute = target.starts_with('/');
    let target = if target.starts_with('/') {
        let stripped = strip_site_prefix(&target, site_prefix);
        // Reject traversal even after prefix stripping (e.g. `/docs/../../etc/passwd`)
        if has_parent_traversal(&stripped) {
            return None;
        }
        stripped
    } else {
        if has_parent_traversal(&target) || Path::new(&target).is_absolute() {
            return None;
        }
        target
    };

    let full = canonical_dir.join(&target);
    if full.is_file() {
        // Ok(true) = within vault; Ok(false) or Err = reject
        if ensure_within_vault(canonical_dir, &full).unwrap_or(false) {
            // If an index is provided, prefer the canonical on-disk casing from
            // the index over the literal input casing. This matters on
            // case-insensitive filesystems where `is_file()` succeeds even when
            // the literal casing differs from what is stored on disk.
            if let Some(idx) = case_index
                && let Some(canonical_path) = idx.lookup_unique(&target)
            {
                return Some(canonical_path.to_owned());
            }
            return Some(target.clone());
        }
        return None;
    }

    // Exact literal path does not exist. Try case-insensitive index lookup.
    if let Some(idx) = case_index
        && let Some(canonical_path) = idx.lookup_unique(&target)
    {
        // Verify the resolved path is within vault bounds.
        let full_resolved = canonical_dir.join(canonical_path);
        if ensure_within_vault(canonical_dir, &full_resolved).unwrap_or(false) {
            return Some(canonical_path.to_owned());
        }
    }

    let target_has_md_ext = std::path::Path::new(&target)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));

    if !target_has_md_ext {
        // Two suffix attempts remain, and their *relative order* is what
        // encodes the precedence rule (iter-203):
        //
        // - `<target>.md`        — the classic stem form (`foo` → `foo.md`).
        // - `<target>/index.md`  — the directory-index convention every
        //   static-site corpus (MDN, GitHub Docs, Docusaurus, Hugo) links
        //   against, where `/foo` names the page at `foo/index.md`.
        //
        // Normally the file form wins: `foo` resolves to `foo.md` even when
        // `foo/index.md` also exists. A target written with a *trailing slash*
        // (`foo/`) is an explicit directory reference, so the order flips and
        // the index file wins. The `.md` attempt is still kept as a
        // last-chance fallback for trailing-slash targets so a sloppily
        // written `page/` keeps resolving to `page.md` when the directory
        // does not exist at all.
        //
        // Both attempts run before the Obsidian bare-stem fallback below: a
        // concrete path beats a fuzzy basename search. Unlike that fallback,
        // the directory-index attempt also applies to site-absolute targets —
        // resolving `/foo` to `foo/index.md` is a path lookup, not a guess.
        let with_ext = format!("{target}.md");
        let dir_index = format!("{target}/{DIRECTORY_INDEX_FILE}");
        let ordered = if trailing_slash {
            [dir_index.as_str(), with_ext.as_str()]
        } else {
            [with_ext.as_str(), dir_index.as_str()]
        };
        for candidate in ordered {
            if let Some(hit) = resolve_candidate_path(canonical_dir, candidate, case_index) {
                return Some(hit);
            }
        }
    }

    // Obsidian-style bare stem resolution: if the target has no path separator,
    // look it up by filename stem. Resolves `[[note]]` to `sub/note.md` when
    // exactly one file in the vault has that stem.
    //
    // Only `'/'` is tested here, unlike the sibling separator guards in this file
    // (`stem_classification`, `classify_link`, the markdown-destination branch),
    // which see raw link targets and therefore must test `'\\'` too. This guard
    // runs *after* the unconditional `replace('\\', "/")` at the top of this
    // function, so `target` cannot contain a backslash at this point: a Windows
    // -flavoured target like `note.md\` has already become `note.md`, and cannot
    // be truncated into a mangled stem such as `note.`. Adding `'\\'` here would
    // be an unreachable condition. See `resolve_target_backslash_targets_are_
    // normalized_before_stem_resolution` for the pinned invariant.
    if !target.contains('/')
        && !site_absolute
        && let Some(idx) = case_index
    {
        // Try the target as-is (could already be a stem or have .md).
        let stem = if Path::new(target.as_str())
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        {
            &target[..target.len() - 3]
        } else {
            &target
        };
        if let Some(canonical_path) = idx.lookup_stem(stem) {
            let full_resolved = canonical_dir.join(canonical_path);
            if ensure_within_vault(canonical_dir, &full_resolved).unwrap_or(false) {
                return Some(canonical_path.to_owned());
            }
        }
        // iter-272 Part B (DEC-288): last resort — a frontmatter `aliases:`
        // entry. Obsidian resolves `[[Leah]]` to the note declaring
        // `aliases: [Leah]`, and 7 of the Obsidian Hub's 47 genuinely-broken
        // targets were exactly that. Deliberately *after* every path and stem
        // attempt, so a real filename always wins over someone else's alias;
        // `lookup_alias` answers `None` when two notes claim the same alias,
        // which keeps the link ambiguous rather than resolving it to whichever
        // note happened to be scanned first.
        if let Some(canonical_path) = idx.lookup_alias(stem) {
            let full_resolved = canonical_dir.join(canonical_path);
            if ensure_within_vault(canonical_dir, &full_resolved).unwrap_or(false) {
                return Some(canonical_path.to_owned());
            }
        }
    }

    None
}

/// Visitor that reads a file's frontmatter `aliases:` and stops before the
/// body — the cheapest scan the scanner offers (iter-272 Part B).
struct AliasVisitor {
    aliases: Vec<String>,
}

impl crate::scanner::FileVisitor for AliasVisitor {
    fn on_frontmatter(
        &mut self,
        props: indexmap::IndexMap<String, serde_json::Value>,
    ) -> crate::scanner::ScanAction {
        self.aliases = crate::filter::extract_aliases(&props);
        // Nothing below the frontmatter matters, so the body is never read.
        crate::scanner::ScanAction::Stop
    }
}

/// Read one file's declared frontmatter `aliases:` without reading its body.
///
/// Returns an empty vec for a file with no frontmatter, no `aliases:` key, or
/// unparseable YAML — an alias map is an optimisation of resolution, never a
/// reason to fail a command.
#[must_use]
pub fn read_aliases(path: &Path) -> Vec<String> {
    let mut visitor = AliasVisitor {
        aliases: Vec::new(),
    };
    match crate::scanner::scan_file_multi(path, &mut [&mut visitor]) {
        Ok(()) => visitor.aliases,
        Err(_) => Vec::new(),
    }
}

/// Populate `idx` with every note's declared frontmatter `aliases:` by
/// scanning the vault's frontmatter (iter-272 Part B, DEC-288).
///
/// Only the frontmatter of each file is read — the visitor stops the scan the
/// moment the properties are parsed — so the pass costs one `open` + one short
/// `read` per note and touches no body bytes.
pub fn populate_aliases_from_dir(dir: &Path, idx: &mut CaseInsensitiveIndex) {
    if !link_aliases_enabled() {
        return;
    }
    let Ok(files) = discover_files(dir) else {
        return;
    };
    for file in &files {
        let aliases = read_aliases(file);
        if aliases.is_empty() {
            continue;
        }
        idx.insert_aliases(&relative_path(dir, file), aliases);
    }
}

/// Whether `target`, written in `source_rel`, resolves only because some note
/// declares it as a frontmatter alias (iter-272 Part B).
///
/// Used to label a resolved link with `via: "alias"` and to stop `links fix`
/// from fuzzy-rewriting a target that is a perfectly good alias. Answers
/// `false` for every target that a path or stem lookup would have resolved.
#[must_use]
pub fn resolves_via_alias(target: &str, case_index: Option<&CaseInsensitiveIndex>) -> bool {
    let Some(idx) = case_index else {
        return false;
    };
    // Only a bare, non-site-absolute target can name an alias — the same guard
    // `resolve_target` applies before its stem and alias lookups.
    if target.is_empty() || target.contains('/') || target.contains('\\') {
        return false;
    }
    let stem = target
        .strip_suffix(".md")
        .or_else(|| target.strip_suffix(".MD"))
        .unwrap_or(target);
    let stem = stem.split('#').next().unwrap_or(stem);
    if stem.is_empty() || !idx.has_alias(stem) {
        return false;
    }
    // A filename always beats an alias, so a target the stem map already
    // answers did not resolve "via alias" even when an alias also matches.
    idx.lookup_stem(stem).is_none() && idx.lookup_alias(stem).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- iter-272 Part B (DEC-288): frontmatter `aliases:` resolution ---

    /// Build a vault whose notes declare aliases, plus the matching index.
    fn alias_vault(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf, CaseInsensitiveIndex) {
        let tmp = tempfile::tempdir().unwrap();
        for (rel, body) in files {
            let p = tmp.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(p, body).unwrap();
        }
        let canon = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        for f in discover_files(tmp.path()).unwrap() {
            idx.insert(&relative_path(tmp.path(), &f));
        }
        populate_aliases_from_dir(tmp.path(), &mut idx);
        (tmp, canon, idx)
    }

    #[test]
    fn a_unique_alias_resolves_and_reports_via_alias() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("Leah Ferguson.md", "---\ntitle: Leah Ferguson\naliases:\n- Leah\n---\n"),
            ("src.md", "see [[Leah]]\n"),
        ]);
        assert_eq!(
            resolve_target(&canon, "Leah", None, Some(&idx)).as_deref(),
            Some("Leah Ferguson.md")
        );
        assert!(resolves_via_alias("Leah", Some(&idx)));
        // Case folds like every other lookup (DEC-267).
        assert_eq!(
            resolve_target(&canon, "leah", None, Some(&idx)).as_deref(),
            Some("Leah Ferguson.md")
        );
    }

    #[test]
    fn a_filename_always_beats_someone_elses_alias() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("Leah.md", "---\ntitle: The real Leah\n---\n"),
            (
                "Leah Ferguson.md",
                "---\ntitle: Leah Ferguson\naliases:\n- Leah\n---\n",
            ),
        ]);
        assert_eq!(
            resolve_target(&canon, "Leah", None, Some(&idx)).as_deref(),
            Some("Leah.md")
        );
        assert!(
            !resolves_via_alias("Leah", Some(&idx)),
            "a filename match is not a `via: alias` resolution"
        );
    }

    #[test]
    fn an_alias_claimed_by_two_notes_is_ambiguous_not_resolved() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("a.md", "---\ntitle: A\naliases:\n- Shared\n---\n"),
            ("b.md", "---\ntitle: B\naliases: Shared\n---\n"),
        ]);
        assert_eq!(idx.lookup_alias_all("shared").len(), 2);
        assert_eq!(resolve_target(&canon, "Shared", None, Some(&idx)), None);
        assert!(!resolves_via_alias("Shared", Some(&idx)));
    }

    #[test]
    fn the_string_form_of_aliases_is_accepted() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("Leah Ferguson.md", "---\ntitle: L\naliases: Leah\n---\n"),
        ]);
        assert_eq!(
            resolve_target(&canon, "Leah", None, Some(&idx)).as_deref(),
            Some("Leah Ferguson.md")
        );
    }

    #[test]
    fn an_alias_with_a_fragment_or_label_still_resolves() {
        let (_tmp, canon, idx) = alias_vault(&[
            (
                "Leah Ferguson.md",
                "---\ntitle: L\naliases:\n- Leah\n---\n\n## Work\n",
            ),
        ]);
        // `resolve_target` strips the fragment; the alias half is what is left.
        assert_eq!(
            resolve_target(&canon, "Leah#Work", None, Some(&idx)).as_deref(),
            Some("Leah Ferguson.md")
        );
        // A `[[alias|label]]` never reaches the resolver with its label — the
        // extractor splits it off — so the target is the bare alias.
        assert_eq!(
            resolve_target(&canon, "Leah", None, Some(&idx)).as_deref(),
            Some("Leah Ferguson.md")
        );
    }

    #[test]
    fn a_note_aliasing_its_own_stem_changes_nothing() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("note.md", "---\ntitle: N\naliases:\n- note\n---\n"),
        ]);
        assert_eq!(
            resolve_target(&canon, "note", None, Some(&idx)).as_deref(),
            Some("note.md")
        );
        assert!(!resolves_via_alias("note", Some(&idx)));
    }

    #[test]
    fn a_path_qualified_target_never_consults_aliases() {
        let (_tmp, canon, idx) = alias_vault(&[
            ("sub/Leah Ferguson.md", "---\ntitle: L\naliases:\n- Leah\n---\n"),
        ]);
        assert_eq!(resolve_target(&canon, "sub/Leah", None, Some(&idx)), None);
        assert!(!resolves_via_alias("sub/Leah", Some(&idx)));
    }

    // --- iter-265: `[scan] exclude` glob matching ---

    /// Compile a `[scan] exclude` set without touching the process-global
    /// `OnceLock` (which only one test per process could ever set).
    fn compiled_exclude(patterns: &[&str]) -> ScanExclude {
        let mut builder = GlobSetBuilder::new();
        let mut kept = Vec::new();
        for pat in patterns {
            builder.add(
                GlobBuilder::new(pat)
                    .literal_separator(true)
                    .build()
                    .unwrap(),
            );
            kept.push((*pat).to_owned());
        }
        ScanExclude {
            set: builder.build().unwrap(),
            patterns: kept,
        }
    }

    #[test]
    fn scan_exclude_matches_a_subtree_but_not_its_siblings() {
        let exc = compiled_exclude(&["Templates/**"]);
        assert!(exc.is_excluded("Templates/album.md"));
        assert!(exc.is_excluded("Templates/nested/book.md"));
        assert!(!exc.is_excluded("Templates.md"));
        assert!(!exc.is_excluded("Notes/Templates.md"));
        assert!(!exc.is_excluded("a.md"));
    }

    #[test]
    fn scan_exclude_names_the_glob_that_matched() {
        let exc = compiled_exclude(&["Templates/**", "archive/*.md"]);
        assert_eq!(exc.matching_glob("archive/old.md"), Some("archive/*.md"));
        assert_eq!(exc.matching_glob("Templates/x.md"), Some("Templates/**"));
        assert_eq!(exc.matching_glob("keep.md"), None);
    }

    #[test]
    fn scan_exclude_respects_the_path_separator() {
        // `*.md` must not reach into a subdirectory — the separator is literal,
        // matching how `[lint] ignore` and `--glob` already behave.
        let exc = compiled_exclude(&["*.md"]);
        assert!(exc.is_excluded("top.md"));
        assert!(!exc.is_excluded("sub/nested.md"));
    }

    #[test]
    fn set_scan_exclude_reports_invalid_globs_without_dropping_the_rest() {
        // An unclosed character class is the classic typo. `set_scan_exclude`
        // itself writes a `OnceLock`, so exercise the compile step the way the
        // function does rather than calling it (a second call would be a no-op
        // in a process where another test already set it).
        let mut errors = Vec::new();
        let mut ok = 0;
        for pat in ["Templates/**", "[unclosed"] {
            match GlobBuilder::new(pat).literal_separator(true).build() {
                Ok(_) => ok += 1,
                Err(e) => errors.push((pat, e.to_string())),
            }
        }
        assert_eq!(ok, 1);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, "[unclosed");
    }

    // --- L-23: percent-decoding ---

    #[test]
    fn percent_decode_space() {
        assert_eq!(
            percent_decode_path("my%20dest.md").as_deref(),
            Some("my dest.md")
        );
    }

    #[test]
    fn percent_decode_lowercase_and_uppercase_hex() {
        assert_eq!(percent_decode_path("a%2fb.md").as_deref(), Some("a/b.md"));
        assert_eq!(percent_decode_path("a%2Fb.md").as_deref(), Some("a/b.md"));
    }

    #[test]
    fn percent_decode_no_escape_returns_none() {
        assert_eq!(percent_decode_path("plain.md"), None);
    }

    #[test]
    fn percent_decode_malformed_keeps_literal() {
        // `%2` is truncated, `%zz` is non-hex: both preserve the literal input.
        assert_eq!(percent_decode_path("bad%2.md"), None);
        assert_eq!(percent_decode_path("bad%zz.md"), None);
        // A stray `%` with no hex at all (e.g. `100%done`).
        assert_eq!(percent_decode_path("100%done.md"), None);
    }

    #[test]
    fn percent_decode_non_utf8_keeps_literal() {
        // `%FF` alone is not valid UTF-8 → keep literal (return None).
        assert_eq!(percent_decode_path("bad%FF.md"), None);
    }

    #[test]
    fn resolve_target_percent_encoded_space() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("my dest.md"), "# Dest").unwrap();
        // Use dunce::canonicalize (via canonicalize_vault_dir), not the raw
        // std canonicalize: on Windows std's version returns a `\\?\`
        // extended-length-path prefix, which `ensure_within_vault`'s own
        // dunce-canonicalized comparison would then fail to prefix-match,
        // making this test spuriously fail on windows-latest CI.
        let canon = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canon, "my%20dest.md", None, None).as_deref(),
            Some("my dest.md")
        );
    }

    #[test]
    fn discover_finds_md_files() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "# Note").unwrap();
        fs::write(tmp.path().join("readme.txt"), "text").unwrap();
        fs::create_dir_all(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/deep.md"), "# Deep").unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "md"));
    }

    /// M-5 (iter-202): a symlink and its target are two directory entries but
    /// one file. Enumerating both made whole-vault writers rewrite the same
    /// note twice — the second write tripping the concurrent-modification
    /// guard — and inflated every count the CLI reports.
    #[cfg(unix)]
    #[test]
    fn discover_dedups_intra_vault_symlink_and_target() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("real.md"), "# Real").unwrap();
        std::os::unix::fs::symlink("real.md", tmp.path().join("alias.md")).unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "one file, two spellings: {files:?}");
        assert!(
            files[0].ends_with("real.md"),
            "the real file represents the group, not the alias: {files:?}"
        );
    }

    /// BUG-7 (iter-207): the surviving spelling must be the *real* file even
    /// when the symlink sorts first. Keeping the alias dropped the target from
    /// the fuzzy candidate set (`[fuzzy 0.966]` → `Unfixable: 1`) and made
    /// `links fix` report rewrites against a name that is not the file.
    #[cfg(unix)]
    #[test]
    fn discover_dedup_prefers_real_file_over_alphabetically_earlier_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("target.md"), "# Target").unwrap();
        std::os::unix::fs::symlink("target.md", tmp.path().join("alias-target.md")).unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "one file, two spellings: {files:?}");
        assert!(
            files[0].ends_with("target.md") && !files[0].ends_with("alias-target.md"),
            "the non-symlink spelling wins even though it sorts later: {files:?}"
        );
    }

    /// When every spelling is a symlink there is no real file to prefer, so
    /// the deterministic first-in-sort-order fallback still applies.
    #[cfg(unix)]
    #[test]
    fn discover_dedup_falls_back_to_first_when_all_spellings_are_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        // The target lives in a hidden directory, so the walker never
        // enumerates it directly: the group consists of two symlinks only.
        fs::create_dir_all(tmp.path().join(".store")).unwrap();
        fs::write(tmp.path().join(".store/real.md"), "# Real").unwrap();
        std::os::unix::fs::symlink(".store/real.md", tmp.path().join("a-alias.md")).unwrap();
        std::os::unix::fs::symlink(".store/real.md", tmp.path().join("z-alias.md")).unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1, "one file, two aliases: {files:?}");
        assert!(
            files[0].ends_with("a-alias.md"),
            "with no real spelling available the first in sort order wins: {files:?}"
        );
    }

    /// The surviving spelling must not depend on walk order, which is
    /// non-deterministic (the walker is parallel).
    #[cfg(unix)]
    #[test]
    fn discover_dedup_is_stable_across_runs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("zzz")).unwrap();
        fs::write(tmp.path().join("zzz/real.md"), "# Real").unwrap();
        std::os::unix::fs::symlink("zzz/real.md", tmp.path().join("aaa.md")).unwrap();
        fs::write(tmp.path().join("other.md"), "# Other").unwrap();

        let first = discover_files(tmp.path()).unwrap();
        for _ in 0..5 {
            assert_eq!(
                discover_files(tmp.path()).unwrap(),
                first,
                "dedup must pick the same spelling every run"
            );
        }
        assert_eq!(first.len(), 2, "other.md + zzz/real.md: {first:?}");
        assert!(
            first.iter().any(|f| f.ends_with("zzz/real.md")),
            "the real file represents the aliased group: {first:?}"
        );
        assert!(
            !first.iter().any(|f| f.ends_with("aaa.md")),
            "the alias must not survive alongside its target: {first:?}"
        );
    }

    /// Two distinct notes that merely *look* similar must both survive — the
    /// dedup key is the canonical path, not the file name or content.
    #[cfg(unix)]
    #[test]
    fn discover_keeps_distinct_files_with_symlinks_present() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "# A").unwrap();
        fs::write(tmp.path().join("b.md"), "# B").unwrap();
        std::os::unix::fs::symlink("a.md", tmp.path().join("c.md")).unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 2, "a.md (as a.md) and b.md: {files:?}");
        assert!(files.iter().any(|f| f.ends_with("b.md")), "{files:?}");
    }

    #[test]
    fn discover_skips_hidden_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("visible.md"), "# Visible").unwrap();
        fs::create_dir_all(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join(".hidden/secret.md"), "# Secret").unwrap();

        let files = discover_files(tmp.path()).unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("visible.md"));
    }

    /// Build a `ScanInclude` directly for testing the include-override path
    /// without touching the process-global `OnceLock`.
    fn test_include(patterns: &[&str]) -> ScanInclude {
        let mut builder = GlobSetBuilder::new();
        let mut dir_prefixes = Vec::new();
        for p in patterns {
            builder.add(GlobBuilder::new(p).literal_separator(true).build().unwrap());
            let prefix = glob_dir_prefix(p);
            if !prefix.is_empty() {
                dir_prefixes.push(prefix);
            }
        }
        ScanInclude {
            set: builder.build().unwrap(),
            dir_prefixes,
        }
    }

    #[test]
    fn glob_dir_prefix_cases() {
        assert_eq!(glob_dir_prefix(".claude/skills/**"), ".claude/skills");
        assert_eq!(glob_dir_prefix(".config/*.md"), ".config");
        assert_eq!(glob_dir_prefix(".obsidian/**/x.md"), ".obsidian");
        assert_eq!(glob_dir_prefix("**/*.md"), "");
        // A fully-literal glob's "directory" is the parent of the file.
        assert_eq!(glob_dir_prefix(".claude/skills/a.md"), ".claude/skills");
    }

    #[test]
    fn scan_include_reaches_dot_dir() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("visible.md"), "# Visible").unwrap();
        fs::create_dir_all(tmp.path().join(".claude/skills/foo")).unwrap();
        fs::write(tmp.path().join(".claude/skills/foo/SKILL.md"), "# Skill").unwrap();
        // A different hidden dir must stay excluded.
        fs::create_dir_all(tmp.path().join(".secret")).unwrap();
        fs::write(tmp.path().join(".secret/leak.md"), "# Leak").unwrap();

        let inc = test_include(&[".claude/skills/**"]);
        let files = discover_files_with_include(tmp.path(), Some(&inc)).unwrap();
        let rels: Vec<String> = files.iter().map(|f| relative_path(tmp.path(), f)).collect();
        assert!(rels.contains(&"visible.md".to_owned()));
        assert!(
            rels.contains(&".claude/skills/foo/SKILL.md".to_owned()),
            "included dot-subtree reachable: {rels:?}"
        );
        assert!(
            !rels.iter().any(|r| r.contains(".secret")),
            "unrelated hidden dir stays excluded: {rels:?}"
        );
    }

    #[test]
    fn scan_include_never_reaches_git() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".git/config.md"), "# never").unwrap();
        // Even a permissive glob must not pull `.git` files in.
        let inc = test_include(&[".git/**", "**/*.md"]);
        let files = discover_files_with_include(tmp.path(), Some(&inc)).unwrap();
        let rels: Vec<String> = files.iter().map(|f| relative_path(tmp.path(), f)).collect();
        assert!(
            !rels.iter().any(|r| r.starts_with(".git/")),
            ".git is hard-excluded: {rels:?}"
        );
    }

    #[test]
    fn glob_matching() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "").unwrap();
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(tmp.path().join("notes/b.md"), "").unwrap();
        fs::write(tmp.path().join("notes/c.md"), "").unwrap();

        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "notes/*.md").unwrap();
        assert_eq!(matched.len(), 2);

        let matched_all = match_glob(tmp.path(), &files, "**/*.md").unwrap();
        assert_eq!(matched_all.len(), 3);
    }

    #[test]
    fn glob_star_does_not_cross_slash() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "b.md", "sub/c.md", "sub/deep/d.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let star = match_glob(tmp.path(), &files, "*.md").unwrap();
        // *.md should NOT match sub/c.md or sub/deep/d.md
        assert_eq!(star.len(), 2);

        let double_star = match_glob(tmp.path(), &files, "**/*.md").unwrap();
        assert_eq!(double_star.len(), 4);
    }

    #[test]
    fn resolve_file_success() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (path, rel) = resolve_file(tmp.path(), "note.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "note.md");
    }

    // --- M-2: Windows drive-relative / NTFS-ADS rejection ---
    // (adversarial-review-2026-08-23.md)

    /// Lexical check runs on every platform, but only actually rejects on
    /// Windows — a colon is a legal filename character on Unix/macOS, so
    /// `has_unsafe_windows_colon` is intentionally a no-op there. This test
    /// pins that platform split so a future change can't accidentally start
    /// rejecting harmless colon-containing filenames off Windows.
    #[test]
    fn has_unsafe_windows_colon_is_platform_gated() {
        let result = has_unsafe_windows_colon("a.md:stream");
        assert_eq!(result, cfg!(windows));
    }

    #[test]
    fn has_unsafe_windows_colon_accepts_plain_paths() {
        assert!(!has_unsafe_windows_colon("notes/a.md"));
        assert!(!has_unsafe_windows_colon("a.md"));
    }

    /// T-4 (iter-224): a wider shape table than the single `a.md:stream`
    /// case above, run on every platform via the same `cfg!(windows)`
    /// pinning — pure string-level checks, so unlike a `Path`-based test
    /// (whose `Prefix`/`RootDir` component parsing is itself platform-
    /// specific) these exercise the same lexical decision on every host.
    #[test]
    fn has_unsafe_windows_colon_shape_table() {
        let unsafe_shapes = [
            "C:foo.md",         // drive-relative, no separator after colon
            "a.md:stream",      // NTFS Alternate Data Stream marker
            "sub/a.md:stream",  // ADS marker on a nested path
            "C:foo/bar.md",     // drive-relative with a nested remainder
            "notes:archive.md", // colon inside an ordinary-looking component
        ];
        for shape in unsafe_shapes {
            assert_eq!(
                has_unsafe_windows_colon(shape),
                cfg!(windows),
                "shape {shape:?} should be flagged exactly on Windows"
            );
        }

        let safe_shapes = ["notes/a.md", "a.md", "sub/dir/note.md", "a-b_c.md"];
        for shape in safe_shapes {
            assert!(
                !has_unsafe_windows_colon(shape),
                "shape {shape:?} contains no colon and must never be flagged"
            );
        }
    }

    /// `C:foo` (no `\` after the colon) is drive-*relative*, not absolute:
    /// `Path::is_absolute()` returns `false` for it on Windows (unlike the
    /// already-rejected `C:\foo`), so it must be caught by the dedicated
    /// colon check rather than the `is_absolute()` branch. Gated to Windows
    /// because `"C:foo"` is just an ordinary filename on Unix — rejecting it
    /// there would be a real, unwanted behavior change. CI runs
    /// windows-latest, so this executes for real.
    #[test]
    #[cfg(windows)]
    fn resolve_file_rejects_windows_drive_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_file(tmp.path(), "C:foo.md").unwrap_err();
        assert!(matches!(err, FileResolveError::OutsideVault { .. }));
    }

    #[test]
    #[cfg(windows)]
    fn resolve_file_rejects_ntfs_alternate_data_stream_path() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("a.md"), "").unwrap();
        let err = resolve_file(tmp.path(), "a.md:stream").unwrap_err();
        assert!(matches!(err, FileResolveError::OutsideVault { .. }));
    }

    // --- Task 4: case-insensitive CLI file-argument resolution ---

    #[test]
    fn resolve_file_ci_off_does_not_fallback() {
        // With case_insensitive = false, a lowercase arg must NOT resolve to a
        // capitalized on-disk file (mirrors case-sensitive-filesystem behavior
        // regardless of the host FS).
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Foo.md"), "").unwrap();

        let err = resolve_file_ci(tmp.path(), "foo.md", false);
        // On a case-insensitive host FS the literal is_file() would succeed, so
        // only assert the negative on a genuinely case-sensitive scan by
        // checking the *rel casing* when it does resolve.
        if let Ok((_, rel)) = err {
            // Host FS is case-insensitive: literal lookup wins, rel keeps arg casing.
            assert_eq!(rel, "foo.md");
        }
    }

    #[test]
    fn resolve_file_ci_on_resolves_wrong_case() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Foo.md"), "").unwrap();

        let (path, rel) = resolve_file_ci(tmp.path(), "foo.md", true).unwrap();
        assert!(path.is_file());
        // On a case-sensitive FS the fallback substitutes the real casing
        // (`Foo.md`); on a case-insensitive host the literal `is_file()` already
        // succeeds and keeps the arg casing (`foo.md`). Both are valid — the
        // point is that resolution succeeds either way.
        let host_ci = crate::case_index::probe_case_insensitive(tmp.path()).unwrap_or(false);
        if host_ci {
            assert_eq!(rel, "foo.md");
        } else {
            assert_eq!(rel, "Foo.md");
        }
    }

    #[test]
    fn resolve_file_ci_on_resolves_nested_wrong_case() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Sub/Deep")).unwrap();
        fs::write(tmp.path().join("Sub/Deep/Note.md"), "").unwrap();

        let (path, rel) = resolve_file_ci(tmp.path(), "sub/deep/note.md", true).unwrap();
        assert!(path.is_file());
        let host_ci = crate::case_index::probe_case_insensitive(tmp.path()).unwrap_or(false);
        if host_ci {
            assert_eq!(rel, "sub/deep/note.md");
        } else {
            assert_eq!(rel, "Sub/Deep/Note.md");
        }
    }

    #[test]
    fn resolve_case_insensitive_direct_nested_substitutes_casing() {
        // Unit-level: the component-walk helper itself always substitutes the
        // real casing regardless of host FS (it lists dirs and matches names),
        // so this asserts the substitution logic directly without depending on
        // the outer literal `is_file()` short-circuit.
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("Sub/Deep")).unwrap();
        fs::write(tmp.path().join("Sub/Deep/Note.md"), "").unwrap();

        let (_full, rel) =
            resolve_case_insensitive(tmp.path(), "SUB/deep/NOTE.md").expect("should resolve");
        assert_eq!(rel, "Sub/Deep/Note.md");
    }

    #[test]
    fn resolve_file_ci_exact_case_unaffected() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Foo.md"), "").unwrap();

        // Exact casing still resolves and keeps its casing.
        let (_, rel) = resolve_file_ci(tmp.path(), "Foo.md", true).unwrap();
        assert_eq!(rel, "Foo.md");
    }

    #[test]
    fn resolve_file_ci_missing_still_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Foo.md"), "").unwrap();

        // A file that doesn't exist in any casing still errors.
        assert!(matches!(
            resolve_file_ci(tmp.path(), "bar.md", true),
            Err(FileResolveError::NotFound { .. } | FileResolveError::NotFoundSuggestion { .. })
        ));
    }

    #[test]
    fn resolve_case_insensitive_ambiguous_returns_none() {
        // When two entries differ only in case at the same level, the scan is
        // ambiguous and must not guess. (Skipped on case-insensitive host FS
        // where the two files can't coexist.)
        let tmp = tempfile::tempdir().unwrap();
        let a = fs::write(tmp.path().join("Foo.md"), "");
        let b = fs::write(tmp.path().join("foo.md"), "");
        if a.is_ok() && b.is_ok() && tmp.path().join("Foo.md").exists() {
            // Verify both really coexist (case-sensitive FS).
            let count = fs::read_dir(tmp.path())
                .unwrap()
                .filter(|e| {
                    e.as_ref().is_ok_and(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .eq_ignore_ascii_case("foo.md")
                    })
                })
                .count();
            if count == 2 {
                assert!(resolve_case_insensitive(tmp.path(), "FOO.md").is_none());
            }
        }
    }

    #[test]
    fn resolve_file_strips_leading_dot_slash() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (_, rel) = resolve_file(tmp.path(), "./note.md").unwrap();
        assert_eq!(rel, "note.md");
    }

    #[test]
    fn resolve_file_strips_leading_dot_backslash() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let (_, rel) = resolve_file(tmp.path(), r".\note.md").unwrap();
        assert_eq!(rel, "note.md");
    }

    #[test]
    fn resolve_file_missing_extension_hint() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        let err = resolve_file(tmp.path(), "note").unwrap_err();
        match err {
            FileResolveError::MissingExtension { path, hint } => {
                assert_eq!(path, "note");
                assert_eq!(hint, "note.md");
            }
            other => {
                panic!("expected MissingExtension, got {other:?}")
            }
        }
    }

    #[test]
    fn resolve_file_rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("note.md"), "").unwrap();

        // Absolute path
        assert!(matches!(
            resolve_file(tmp.path(), "/etc/passwd.md"),
            Err(FileResolveError::OutsideVault { .. })
        ));

        // Parent directory traversal (F3-4: its own variant, not OutsideVault —
        // see resolve_file_parent_traversal_message_is_honest_not_outside_vault).
        assert!(matches!(
            resolve_file(tmp.path(), "../secret.md"),
            Err(FileResolveError::ParentTraversal { .. })
        ));

        // Embedded traversal
        assert!(matches!(
            resolve_file(tmp.path(), "sub/../../../etc/passwd.md"),
            Err(FileResolveError::ParentTraversal { .. })
        ));
    }

    #[test]
    fn is_glob_detects_patterns() {
        assert!(is_glob("*.md"));
        assert!(is_glob("notes/**/*.md"));
        assert!(is_glob("note[123].md"));
        assert!(!is_glob("notes/file.md"));
    }

    #[test]
    fn is_glob_detects_negation_prefix() {
        assert!(is_glob("!notes/draft.md"));
        assert!(is_glob("!**/index.md"));
    }

    #[test]
    fn is_glob_detects_escaped_negation_prefix() {
        assert!(is_glob("\\!notes/draft.md"));
        assert!(is_glob("\\!**/index.md"));
    }

    #[test]
    fn glob_negation_escaped_backslash_bang() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "b.md", "notes/draft.md", "notes/final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        // `\!` should be treated identically to `!` (shell escaping workaround)
        let matched = match_glob(tmp.path(), &files, "\\!notes/draft.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(
            !rels.contains(&"notes/draft.md"),
            "draft.md should be excluded via escaped negation"
        );
        assert!(rels.contains(&"notes/final.md"));
        assert!(rels.contains(&"a.md"));
        assert_eq!(matched.len(), 3);
    }

    #[test]
    fn glob_negation_excludes_matching_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "b.md", "notes/draft.md", "notes/final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        // Exclude a specific file
        let matched = match_glob(tmp.path(), &files, "!notes/draft.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(
            !rels.contains(&"notes/draft.md"),
            "draft.md should be excluded"
        );
        assert!(rels.contains(&"notes/final.md"));
        assert!(rels.contains(&"a.md"));
        assert_eq!(matched.len(), 3);
    }

    #[test]
    fn glob_negation_with_wildcard() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "draft-b.md", "draft-c.md", "final.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "!draft-*").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(!rels.iter().any(|r| r.starts_with("draft-")));
        assert!(rels.contains(&"a.md"));
        assert!(rels.contains(&"final.md"));
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn glob_negation_double_star_excludes_recursively() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["index.md", "notes/index.md", "notes/real.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let matched = match_glob(tmp.path(), &files, "!**/index.md").unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert!(!rels.iter().any(|r| r.ends_with("index.md")));
        assert!(rels.contains(&"notes/real.md"));
        assert_eq!(matched.len(), 1);
    }

    #[test]
    fn match_globs_multiple_positive_patterns_union() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["root.md", "sub1/a.md", "sub1/b.md", "sub2/c.md"],
        );
        let files = discover_files(tmp.path()).unwrap();

        let patterns: Vec<String> = vec!["sub1/**".to_owned(), "sub2/**".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 3);
        assert!(rels.contains(&"sub1/a.md"));
        assert!(rels.contains(&"sub1/b.md"));
        assert!(rels.contains(&"sub2/c.md"));
        assert!(!rels.contains(&"root.md"));
    }

    #[test]
    fn match_globs_positive_and_negative() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/keep.md", "sub/draft.md", "root.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let patterns: Vec<String> = vec!["sub/**".to_owned(), "!sub/draft.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 1);
        assert!(rels.contains(&"sub/keep.md"));
        assert!(!rels.contains(&"sub/draft.md"));
    }

    #[test]
    fn match_globs_no_positive_means_all_files() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "b.md", "draft.md"]);
        let files = discover_files(tmp.path()).unwrap();

        // Only a negation pattern — should return all files except matching ones
        let patterns: Vec<String> = vec!["!draft.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &patterns).unwrap();
        let rels: Vec<_> = matched.iter().map(|(_, r)| r.as_str()).collect();
        assert_eq!(matched.len(), 2);
        assert!(rels.contains(&"a.md"));
        assert!(rels.contains(&"b.md"));
        assert!(!rels.contains(&"draft.md"));
    }

    #[test]
    fn match_globs_single_pattern_same_as_match_glob() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md", "notes/b.md", "notes/c.md"]);
        let files = discover_files(tmp.path()).unwrap();

        let single: Vec<String> = vec!["notes/*.md".to_owned()];
        let matched = match_globs(tmp.path(), &files, &single).unwrap();
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn match_globs_empty_negation_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a.md"]);
        let files = discover_files(tmp.path()).unwrap();
        let patterns: Vec<String> = vec!["!".to_owned()];
        assert!(match_globs(tmp.path(), &files, &patterns).is_err());
    }

    // ── strip_dir_prefix unit tests ──────────────────────────────────

    #[test]
    fn strip_dir_prefix_matches_single_component() {
        let dir = Path::new("docs");
        assert_eq!(
            strip_dir_prefix(dir, "docs/notes/foo.md"),
            Some("notes/foo.md".to_owned())
        );
    }

    #[test]
    fn strip_dir_prefix_matches_multi_component() {
        let dir = Path::new("my/docs");
        assert_eq!(
            strip_dir_prefix(dir, "my/docs/foo.md"),
            Some("foo.md".to_owned())
        );
    }

    #[test]
    fn strip_dir_prefix_no_match() {
        let dir = Path::new("docs");
        assert_eq!(strip_dir_prefix(dir, "notes/foo.md"), None);
    }

    #[test]
    fn strip_dir_prefix_partial_component_no_match() {
        // "docs-old/foo.md" should NOT match dir = "docs"
        let dir = Path::new("docs");
        assert_eq!(strip_dir_prefix(dir, "docs-old/foo.md"), None);
    }

    #[test]
    fn strip_dir_prefix_exact_match_returns_none() {
        // The path IS the dir — nothing remains after stripping
        let dir = Path::new("docs");
        assert_eq!(strip_dir_prefix(dir, "docs"), None);
    }

    // ── resolve_file CWD-relative fallback tests ──────────────────────

    #[test]
    fn resolve_file_cwd_relative_fallback() {
        // Simulate: dir = "kb", user passes "kb/note.md"
        let tmp = tempfile::tempdir().unwrap();
        let kb = tmp.path().join("kb");
        fs::create_dir_all(&kb).unwrap();
        fs::write(kb.join("note.md"), "# Note").unwrap();

        let (path, rel) = resolve_file(&kb, "kb/note.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "note.md");
    }

    #[test]
    fn resolve_file_cwd_relative_nested() {
        // dir = "kb", user passes "kb/sub/deep.md"
        let tmp = tempfile::tempdir().unwrap();
        let kb = tmp.path().join("kb");
        fs::create_dir_all(kb.join("sub")).unwrap();
        fs::write(kb.join("sub/deep.md"), "").unwrap();

        let (path, rel) = resolve_file(&kb, "kb/sub/deep.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "sub/deep.md");
    }

    #[test]
    fn resolve_file_cwd_relative_always_strips_prefix() {
        // dir = "kb", KB contains both "note.md" and "kb/note.md".
        // Passing "kb/note.md" always strips to "note.md" because the
        // prefix is removed during normalization (before existence check).
        let tmp = tempfile::tempdir().unwrap();
        let kb = tmp.path().join("kb");
        fs::create_dir_all(kb.join("kb")).unwrap();
        fs::write(kb.join("note.md"), "top").unwrap();
        fs::write(kb.join("kb/note.md"), "nested").unwrap();

        let (path, rel) = resolve_file(&kb, "kb/note.md").unwrap();
        assert!(path.is_file());
        // Prefix is stripped unconditionally: resolves to "note.md"
        assert_eq!(rel, "note.md");
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "top",
            "should resolve to the stripped (vault-relative) file"
        );
    }

    #[test]
    fn resolve_file_cwd_relative_not_found_still_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let kb = tmp.path().join("kb");
        fs::create_dir_all(&kb).unwrap();
        // No file at all
        let err = resolve_file(&kb, "kb/nonexistent.md").unwrap_err();
        assert!(
            matches!(err, FileResolveError::NotFound { .. }),
            "expected NotFound, got {err:?}"
        );
    }

    fn make_files(dir: &Path, paths: &[&str]) {
        for path in paths {
            let full = dir.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(full, "").unwrap();
        }
    }

    // --- iter-203: directory targets resolve to <target>/index.md ---

    #[test]
    fn resolve_target_directory_resolves_to_index_md() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["foo/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        for target in ["foo", "/foo", "foo/", "/foo/"] {
            assert_eq!(
                resolve_target(&canonical, target, None, None),
                Some("foo/index.md".to_owned()),
                "target {target} must resolve to the directory index"
            );
        }
    }

    #[test]
    fn resolve_target_nested_directory_resolves_to_index_md() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["web/api/document/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/web/api/document", None, None),
            Some("web/api/document/index.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "web/api/document/", None, None),
            Some("web/api/document/index.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_directory_without_index_stays_broken() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["foo/page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        for target in ["foo", "/foo", "foo/", "/foo/"] {
            assert_eq!(
                resolve_target(&canonical, target, None, None),
                None,
                "target {target} has no index.md and must not resolve"
            );
        }
    }

    #[test]
    fn resolve_target_file_beats_directory_index() {
        // Precedence: `foo.md` wins over `foo/index.md` when both exist.
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["foo.md", "foo/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "foo", None, None),
            Some("foo.md".to_owned())
        );
        // …but a trailing slash is an explicit directory reference, so it must
        // reach the index file even though `foo.md` exists.
        assert_eq!(
            resolve_target(&canonical, "foo/", None, None),
            Some("foo/index.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_explicit_index_forms_keep_working() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["foo/index.md", "bar/page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "foo/index", None, None),
            Some("foo/index.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "foo/index.md", None, None),
            Some("foo/index.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "/bar/page", None, None),
            Some("bar/page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_directory_index_case_variants_via_index() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["Foo/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("Foo/index.md");
        assert_eq!(
            resolve_target(&canonical, "/foo", None, Some(&idx)),
            Some("Foo/index.md".to_owned()),
            "case-differing directory target must resolve through the index"
        );
    }

    #[test]
    fn resolve_target_directory_index_with_site_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["guide/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/docs/guide", Some("docs"), None),
            Some("guide/index.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_directory_index_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["foo/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "../foo", None, None), None);
        assert_eq!(
            resolve_target(&canonical, "sub/../../foo", None, None),
            None
        );
    }

    #[test]
    fn directory_for_index_file_cases() {
        assert_eq!(directory_for_index_file("foo/index.md"), Some("foo"));
        assert_eq!(directory_for_index_file("a/b/Index.MD"), Some("a/b"));
        assert_eq!(directory_for_index_file("index.md"), None);
        assert_eq!(directory_for_index_file("foo/notindex.md"), None);
        assert_eq!(directory_for_index_file("/index.md"), None);
    }

    #[test]
    fn resolve_target_stem_appends_md() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "note", None, None),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_explicit_md_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "note.md", None, None),
            Some("note.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_stem() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "sub/other", None, None),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_subpath_with_extension() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "sub/other.md", None, None),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_bare_stem_without_index_returns_none() {
        // Without a case/stem index, bare stems can't resolve to subdirectories
        // (no filesystem scan is performed for stem matching).
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "other", None, None), None);
    }

    #[test]
    fn resolve_target_bare_stem_with_index_resolves_unique() {
        // Obsidian-style: [[other]] resolves to sub/other.md when the stem is unique.
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("sub/other.md");
        assert_eq!(
            resolve_target(&canonical, "other", None, Some(&idx)),
            Some("sub/other.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_bare_stem_works_when_case_insensitive_paths_disabled() {
        // Regression for iter-137: the bare-basename stem fallback must work
        // even when `case_insensitive_paths` is disabled. Stem lookup is an
        // Obsidian short-form convention, independent of case-sensitivity
        // mode. Previously broke on Linux when no `.hyalo.toml` was present
        // because `maybe_case_index` returned None when mode was off, and the
        // stem-fallback inside `resolve_target` was gated on `case_index`
        // being Some.
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["archive/b.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        // Note: case-insensitive paths intentionally NOT enabled — emulates
        // Linux with `case_insensitive = "auto"` and a case-sensitive FS.
        idx.insert("archive/b.md");
        assert_eq!(
            resolve_target(&canonical, "b", None, Some(&idx)),
            Some("archive/b.md".to_owned()),
            "bare-stem fallback must work without case-insensitive paths"
        );
    }

    #[test]
    fn resolve_target_backslash_targets_are_normalized_before_stem_resolution() {
        // iter-195: pins why the bare-stem guard in `resolve_target` tests only
        // `'/'` while its three siblings in this file test `'/'` and `'\\'`.
        // Backslashes are normalized to `/` at the top of `resolve_target`, so a
        // Windows-flavoured target can never reach the stem branch carrying a
        // separator that the guard fails to see, and can never be truncated into
        // a mangled stem like `note.` (which would wrongly match `note..md`).
        // Platform-independent: the normalization is a string replace, not a
        // `Path` operation.
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note..md", "sub/note.md", "sub/other.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("note..md");
        idx.insert("sub/note.md");
        idx.insert("sub/other.md");

        // Sanity: the mangled stem `note.` IS resolvable when asked for
        // directly, so the assertions below are about normalization and not
        // about an index that would miss anyway.
        assert_eq!(
            resolve_target(&canonical, "note.", None, Some(&idx)),
            Some("note..md".to_owned()),
            "index sanity: the stem `note.` resolves when asked for directly"
        );

        // `note.md\` normalizes to `note.md`, so it takes the ordinary bare-name
        // path and resolves by stem `note` — never by the mangled stem `note.`.
        assert_eq!(
            resolve_target(&canonical, "note.md\\", None, Some(&idx)),
            Some("sub/note.md".to_owned()),
            "a trailing backslash must normalize away, not mangle the stem"
        );

        // A backslash-separated target is treated as the equivalent
        // forward-slash path and never falls through to stem lookup.
        assert_eq!(
            resolve_target(&canonical, "sub\\other.md", None, Some(&idx)),
            Some("sub/other.md".to_owned()),
            "backslash-separated targets resolve as paths"
        );
        assert_eq!(
            resolve_target(&canonical, "missing\\other", None, Some(&idx)),
            None,
            "a path-like backslash target must not be rescued by stem lookup"
        );
    }

    #[test]
    fn resolve_target_bare_stem_ambiguous_returns_none() {
        // Two files with the same stem → ambiguous → None.
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a/note.md", "b/note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("a/note.md");
        idx.insert("b/note.md");
        assert_eq!(resolve_target(&canonical, "note", None, Some(&idx)), None);
    }

    #[test]
    fn resolve_target_nonexistent_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "nonexistent", None, None), None);
    }

    #[test]
    fn resolve_target_empty_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "", None, None), None);
    }

    #[test]
    fn resolve_target_rejects_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(resolve_target(&canonical, "../note", None, None), None);
        assert_eq!(
            resolve_target(&canonical, "sub/../../note", None, None),
            None
        );
        // /etc/passwd normalizes to "etc/passwd" which doesn't exist in the vault
        assert_eq!(resolve_target(&canonical, "/etc/passwd", None, None), None);
    }

    #[test]
    fn resolve_target_non_md_file_exact_match() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["image.png"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "image.png", None, None),
            Some("image.png".to_owned())
        );
    }

    // --- path traversal: dotdot in filename should not be rejected ---

    #[test]
    fn resolve_file_accepts_dotdot_in_filename() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(tmp.path().join("notes/etc..md"), "# dotdot").unwrap();

        let (path, rel) = resolve_file(tmp.path(), "notes/etc..md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "notes/etc..md");
    }

    #[test]
    fn resolve_file_rejects_parent_traversal_segments() {
        let tmp = tempfile::tempdir().unwrap();

        assert!(matches!(
            resolve_file(tmp.path(), "../secret.md"),
            Err(FileResolveError::ParentTraversal { .. })
        ));

        assert!(matches!(
            resolve_file(tmp.path(), "sub/../../etc/passwd.md"),
            Err(FileResolveError::ParentTraversal { .. })
        ));
    }

    #[test]
    fn resolve_target_accepts_dotdot_in_filename() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["etc..md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        assert_eq!(
            resolve_target(&canonical, "etc..md", None, None),
            Some("etc..md".to_owned())
        );
    }

    #[test]
    fn resolve_target_rejects_parent_traversal_segment() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        assert_eq!(resolve_target(&canonical, "../secret.md", None, None), None);
    }

    // --- symlink escape tests ---

    #[cfg(unix)]
    #[test]
    fn resolve_file_rejects_symlink_escape() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "# Secret").unwrap();

        // Create a symlink inside vault that points outside
        std::os::unix::fs::symlink(outside.path(), vault.path().join("linked")).unwrap();

        let err = resolve_file(vault.path(), "linked/secret.md").unwrap_err();
        assert!(
            matches!(err, FileResolveError::OutsideVault { .. }),
            "expected OutsideVault, got {err:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_target_rejects_symlink_escape() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.md"), "# Secret").unwrap();

        std::os::unix::fs::symlink(outside.path(), vault.path().join("linked")).unwrap();

        let canonical = canonicalize_vault_dir(vault.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "linked/secret", None, None),
            None
        );
        assert_eq!(
            resolve_target(&canonical, "linked/secret.md", None, None),
            None
        );
    }

    // --- site_prefix resolution ---

    #[test]
    fn resolve_target_absolute_with_site_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/docs/page.md", Some("docs"), None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_no_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/page.md", None, None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_nonmatching_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["other/b.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        // site_prefix "docs" doesn't match "/other/b.md", so strip just the "/"
        assert_eq!(
            resolve_target(&canonical, "/other/b.md", Some("docs"), None),
            Some("other/b.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_absolute_stem_with_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "/docs/page", Some("docs"), None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_trailing_slash() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page.md/", None, None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page/", None, None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_query_string() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page?foo=bar", None, None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page.md?dv=winzip", None, None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_fragment() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page#section", None, None),
            Some("page.md".to_owned())
        );
        assert_eq!(
            resolve_target(&canonical, "page.md#heading", None, None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_strips_query_and_fragment_combined() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        assert_eq!(
            resolve_target(&canonical, "page?foo=bar#section", None, None),
            Some("page.md".to_owned())
        );
        // Trailing slash + query + fragment
        assert_eq!(
            resolve_target(&canonical, "page/?q=1#top", None, None),
            Some("page.md".to_owned())
        );
    }

    #[test]
    fn resolve_target_fragment_only_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["page.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        // "#section" → empty target after stripping → None
        assert_eq!(resolve_target(&canonical, "#section", None, None), None);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_file_allows_symlink_within_vault() {
        let vault = tempfile::tempdir().unwrap();
        let subdir = vault.path().join("notes");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("real.md"), "# Real").unwrap();

        // Symlink within the vault is fine
        std::os::unix::fs::symlink(&subdir, vault.path().join("alias")).unwrap();

        let (path, rel) = resolve_file(vault.path(), "alias/real.md").unwrap();
        assert!(path.is_file());
        assert_eq!(rel, "alias/real.md");
    }

    #[test]
    fn resolve_file_rejects_null_byte_in_path() {
        let vault = tempfile::tempdir().unwrap();
        // A null byte must be rejected before the `.md` check so that it
        // cannot be used to bypass the extension validation on any platform.
        let err = resolve_file(vault.path(), "notes/file\0.md").unwrap_err();
        assert!(matches!(
            err,
            FileResolveError::InvalidPath {
                reason: "contains null byte",
                ..
            }
        ));
        assert!(err.to_string().contains("contains null byte"));
    }

    #[test]
    fn resolve_file_rejects_null_byte_only_path() {
        let vault = tempfile::tempdir().unwrap();
        let err = resolve_file(vault.path(), "\0").unwrap_err();
        assert!(matches!(
            err,
            FileResolveError::InvalidPath {
                reason: "contains null byte",
                ..
            }
        ));
        assert!(err.to_string().contains("contains null byte"));
    }

    // -----------------------------------------------------------------------
    // Iteration 76 — directory detection hint
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_file_directory_suggests_glob() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("notes")).unwrap();

        let err = resolve_file(tmp.path(), "notes").unwrap_err();
        match err {
            FileResolveError::IsDirectory { ref path, ref hint } => {
                assert_eq!(path, "notes");
                assert!(hint.contains("--glob"));
                assert!(hint.contains("notes/*"));
            }
            other => panic!("expected IsDirectory, got {other:?}"),
        }
        assert!(err.to_string().contains("directory"));
    }

    /// iter-210 / BUG-13: the `did you mean X.md?` hint is only offered when
    /// `X.md` actually exists. With nothing on disk there is no candidate, so
    /// the error is a plain not-found rather than a suggestion the user cannot
    /// act on.
    #[test]
    fn resolve_file_without_ext_and_no_candidate_is_plain_not_found() {
        let tmp = tempfile::tempdir().unwrap();
        // "notes" doesn't exist as a dir, as a file, or as "notes.md".
        let err = resolve_file(tmp.path(), "notes").unwrap_err();
        assert!(
            matches!(err, FileResolveError::NotFound { .. }),
            "expected NotFound, got {err:?}"
        );
        assert!(!err.to_string().contains("did you mean"));
    }

    /// iter-210 / BUG-13: a trailing slash is directory syntax. The glob hint
    /// must not double the separator — `--glob 'sub//*'` matches nothing, so
    /// pasting it reports a clean vault for a directory that was never scanned.
    #[test]
    fn resolve_file_trailing_slash_directory_hint_has_single_separator() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();

        let err = resolve_file(tmp.path(), "sub/").unwrap_err();
        match err {
            FileResolveError::IsDirectory { ref hint, .. } => {
                assert_eq!(hint, "--glob 'sub/*'", "got {hint}");
            }
            other => panic!("expected IsDirectory, got {other:?}"),
        }
    }

    /// iter-210 / BUG-13: `nosuchdir/` used to be answered with
    /// "did you mean nosuchdir/.md?" — a path that can never exist.
    #[test]
    fn resolve_file_missing_directory_does_not_suggest_dot_md() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("readme.md"), "").unwrap();

        let err = resolve_file(tmp.path(), "nosuchdir/").unwrap_err();
        assert!(
            matches!(err, FileResolveError::NotFound { .. }),
            "expected NotFound, got {err:?}"
        );
        let msg = err.to_string();
        assert!(
            !msg.contains(".md"),
            "no bogus candidate should appear: {msg}"
        );
    }

    /// A non-`.md` path that exists on disk is still not a note: resolution
    /// must not start accepting `notes.txt` just because the missing-extension
    /// hint became conditional.
    #[test]
    fn resolve_file_non_md_existing_file_is_not_resolved() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("notes.txt"), "").unwrap();

        let err = resolve_file(tmp.path(), "notes.txt").unwrap_err();
        assert!(
            matches!(
                err,
                FileResolveError::NotFound { .. } | FileResolveError::NotFoundSuggestion { .. }
            ),
            "expected a not-found variant, got {err:?}"
        );
    }

    // -----------------------------------------------------------------------
    // Iteration 76 — fuzzy file name suggestion
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_file_fuzzy_suggests_close_match() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("readme.md"), "").unwrap();

        // "readem.md" is 1 edit away from "readme.md"
        let err = resolve_file(tmp.path(), "readem.md").unwrap_err();
        match err {
            FileResolveError::NotFoundSuggestion { ref suggestion, .. } => {
                assert_eq!(suggestion, "readme.md");
            }
            other => panic!("expected NotFoundSuggestion, got {other:?}"),
        }
        assert!(err.to_string().contains("did you mean"));
    }

    #[test]
    fn resolve_file_fuzzy_suggests_with_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/readme.md"), "").unwrap();

        // "sub/readem.md" should suggest "sub/readme.md", not just "readme.md"
        let err = resolve_file(tmp.path(), "sub/readem.md").unwrap_err();
        match err {
            FileResolveError::NotFoundSuggestion { ref suggestion, .. } => {
                assert_eq!(suggestion, "sub/readme.md");
            }
            other => panic!("expected NotFoundSuggestion, got {other:?}"),
        }
    }

    #[test]
    fn resolve_file_no_fuzzy_for_distant_names() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("readme.md"), "").unwrap();

        // "zzzzz.md" is far from "readme.md" — should get plain NotFound
        let err = resolve_file(tmp.path(), "zzzzz.md").unwrap_err();
        assert!(matches!(err, FileResolveError::NotFound { .. }));
    }

    // -----------------------------------------------------------------------
    // Iteration 78 — path traversal returns OutsideVault
    // -----------------------------------------------------------------------

    #[test]
    fn resolve_file_parent_traversal_message_is_honest_not_outside_vault() {
        // F3-4: `../Cargo.toml` from the vault root would actually resolve
        // *outside* the vault here, so this specific input can't prove the
        // false-positive — see
        // `resolve_file_parent_traversal_from_subdir_does_not_claim_outside_vault`
        // for the in-vault case. This test only pins the message wording: it
        // must name the real no-`..` policy, never the (potentially false)
        // "outside vault boundary" claim.
        let tmp = tempfile::tempdir().unwrap();

        let err = resolve_file(tmp.path(), "../Cargo.toml").unwrap_err();
        assert!(matches!(err, FileResolveError::ParentTraversal { .. }));
        let msg = err.to_string();
        assert!(
            msg.contains("..") && msg.to_lowercase().contains("vault-relative"),
            "message should name the no-'..' policy, got: {msg}",
        );
        assert!(
            !msg.contains("resolves outside vault boundary"),
            "message must not claim the path resolves outside the vault — that's \
             not what was checked (purely lexical rejection): {msg}",
        );
    }

    #[test]
    fn resolve_file_parent_traversal_from_subdir_does_not_claim_outside_vault() {
        // F3-4's actual false-positive repro: from a vault subdirectory,
        // `../file.md` names a file squarely inside the vault, yet the
        // lexical no-`..` gate still refuses it (by policy) — the message
        // must not claim it "resolves outside vault boundary", since it does
        // not.
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("in-vault.md"), "").unwrap();

        let err = resolve_file(tmp.path(), "sub/../in-vault.md").unwrap_err();
        assert!(matches!(err, FileResolveError::ParentTraversal { .. }));
        let msg = err.to_string();
        assert!(
            !msg.contains("resolves outside vault boundary"),
            "in-vault path must not be told it escapes the vault: {msg}"
        );
    }

    #[test]
    fn resolve_file_absolute_path_says_outside_vault() {
        let tmp = tempfile::tempdir().unwrap();

        let err = resolve_file(tmp.path(), "/etc/passwd.md").unwrap_err();
        assert!(matches!(err, FileResolveError::OutsideVault { .. }));
    }

    // -----------------------------------------------------------------------
    // Iteration 128 — strip_absolute_vault_prefix
    // -----------------------------------------------------------------------

    #[test]
    fn strip_abs_returns_none_for_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            strip_absolute_vault_prefix(tmp.path(), "notes/foo.md"),
            None
        );
        assert_eq!(strip_absolute_vault_prefix(tmp.path(), "./foo.md"), None);
    }

    #[test]
    fn strip_abs_strips_prefix_for_path_inside_vault() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("notes")).unwrap();
        fs::write(tmp.path().join("notes/foo.md"), "").unwrap();

        let canonical = dunce::canonicalize(tmp.path()).unwrap();
        let abs = canonical.join("notes/foo.md");
        let abs_str = abs.to_string_lossy();

        assert_eq!(
            strip_absolute_vault_prefix(tmp.path(), &abs_str),
            Some("notes/foo.md".to_owned())
        );
    }

    #[test]
    fn strip_abs_returns_none_for_path_outside_vault() {
        let tmp = tempfile::tempdir().unwrap();
        // /etc/passwd.md cannot lie inside a tempdir
        assert_eq!(
            strip_absolute_vault_prefix(tmp.path(), "/etc/passwd.md"),
            None
        );
    }

    #[test]
    fn strip_abs_returns_none_for_path_equal_to_vault() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = dunce::canonicalize(tmp.path()).unwrap();
        let abs_str = canonical.to_string_lossy();
        assert_eq!(strip_absolute_vault_prefix(tmp.path(), &abs_str), None);
    }

    #[test]
    fn strip_abs_handles_nonexistent_file_inside_vault() {
        // A user can pass an absolute path to a file that doesn't exist (yet).
        // We still want to rewrite it so `resolve_file` can produce the proper
        // NotFound diagnostic instead of a misleading OutsideVault.
        let tmp = tempfile::tempdir().unwrap();
        let canonical = dunce::canonicalize(tmp.path()).unwrap();
        let abs = canonical.join("missing.md");
        let abs_str = abs.to_string_lossy();
        assert_eq!(
            strip_absolute_vault_prefix(tmp.path(), &abs_str),
            Some("missing.md".to_owned())
        );
    }

    #[test]
    fn strip_abs_rejects_parent_traversal_in_nonexistent_path() {
        // When canonicalize falls back to the literal path (because the file
        // doesn't exist), a `..` segment can survive strip_prefix. We must
        // refuse to rewrite such paths — handing `../foo.md` to resolve_file
        // would either error or silently escape the vault.
        let tmp = tempfile::tempdir().unwrap();
        let canonical = dunce::canonicalize(tmp.path()).unwrap();
        // Build something like `<canonical>/sub/../escape.md` where neither
        // `sub` nor `escape.md` exists, so canonicalize fails and we keep the
        // literal form with `..` intact.
        let abs = canonical.join("sub/../escape.md");
        let abs_str = abs.to_string_lossy();
        assert_eq!(strip_absolute_vault_prefix(tmp.path(), &abs_str), None);
    }

    // -----------------------------------------------------------------------
    // Iteration 117 — case-insensitive index integration
    // -----------------------------------------------------------------------

    /// On a case-sensitive filesystem (Linux), a literal wrong-casing path
    /// should not resolve without an index, but should resolve with an index.
    /// On a case-insensitive filesystem (macOS), the literal path already
    /// resolves, so we test that the index returns the canonical casing.
    #[cfg(target_os = "linux")]
    #[test]
    fn resolve_target_uses_case_index_when_literal_misses() {
        use crate::case_index::CaseInsensitiveIndex;
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["web/foo/index.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        // Without index, wrong casing must not resolve on case-sensitive fs.
        assert_eq!(
            resolve_target(&canonical, "Web/Foo/index.md", None, None),
            None,
            "expected None without index on case-sensitive fs"
        );

        // Build an index with the real path. Enable case-insensitive paths
        // so `lookup_unique` will fire (default-off as of iter-137).
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        idx.insert("web/foo/index.md");

        // With index, wrong-cased input resolves to the canonical path.
        assert_eq!(
            resolve_target(&canonical, "Web/Foo/index.md", None, Some(&idx)),
            Some("web/foo/index.md".to_owned()),
            "expected canonical path from index"
        );
    }

    /// On any platform: even when the literal path resolves (because the
    /// filesystem is case-insensitive or the casing already matches), the
    /// index should return the canonical on-disk casing.
    #[test]
    fn resolve_target_index_canonical_casing_on_match() {
        use crate::case_index::CaseInsensitiveIndex;
        let tmp = tempfile::tempdir().unwrap();
        // Create the file with lowercase path.
        make_files(tmp.path(), &["docs/guide.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();

        let mut idx = CaseInsensitiveIndex::new();
        idx.insert("docs/guide.md");

        // Exact match — index should still confirm the canonical path.
        assert_eq!(
            resolve_target(&canonical, "docs/guide.md", None, Some(&idx)),
            Some("docs/guide.md".to_owned())
        );
    }

    // --- out-of-vault classification (iter-193) ---

    #[test]
    fn normalized_target_escapes_vault_only_on_parent_walks() {
        assert!(normalized_target_escapes_vault(".."));
        assert!(normalized_target_escapes_vault("../outside/x.md"));
        assert!(!normalized_target_escapes_vault("sub/x.md"));
        assert!(!normalized_target_escapes_vault("..x.md"));
        assert!(!normalized_target_escapes_vault(""));
        // Site-absolute targets stay classified as ordinary (possibly broken)
        // vault links — see the doc comment for why.
        assert!(!normalized_target_escapes_vault("/src/x.md"));
    }

    #[test]
    fn link_target_escapes_vault_normalizes_against_source() {
        use crate::links::LinkKind;

        // `sub/a.md` + `../../outside.md` climbs above the vault root.
        assert!(link_target_escapes_vault(
            "sub/a.md",
            LinkKind::Markdown,
            "../../outside.md"
        ));
        // One `..` from `sub/` only reaches the vault root.
        assert!(!link_target_escapes_vault(
            "sub/a.md",
            LinkKind::Markdown,
            "../sibling.md"
        ));
        // Wikilinks are vault-relative by definition.
        assert!(!link_target_escapes_vault(
            "sub/a.md",
            LinkKind::Wikilink,
            "other"
        ));
        // A bare markdown basename can never escape.
        assert!(!link_target_escapes_vault(
            "sub/a.md",
            LinkKind::Markdown,
            "other.md"
        ));
    }

    // -----------------------------------------------------------------
    // iter-261 / BUG-5, BUG-6 — attachments
    // -----------------------------------------------------------------

    #[test]
    fn non_md_extension_detection() {
        assert!(has_non_md_extension("img.png"));
        assert!(has_non_md_extension("Templates/Bases/Books.base"));
        assert!(has_non_md_extension("a/b.PDF"));
        assert!(!has_non_md_extension("note"));
        assert!(!has_non_md_extension("note.md"));
        assert!(!has_non_md_extension("note.MD"));
        // The dot lives in a directory component, not the filename.
        assert!(!has_non_md_extension("v1.2/notes"));
        // A dotfile has no extension.
        assert!(!has_non_md_extension(".gitignore"));
        assert!(!has_non_md_extension("trailing."));
    }

    /// A vault with one note, one attachment in a sibling folder, and a second
    /// attachment nested under the note's own folder.
    fn attachment_vault() -> (tempfile::TempDir, PathBuf, CaseInsensitiveIndex) {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &[
                "notes/a.md",
                "02 Attachments/task-plugins-sorted.png",
                "notes/sub/img2.png",
                "Templates/Bases/Books.base",
            ],
        );
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        for p in [
            "notes/a.md",
            "02 Attachments/task-plugins-sorted.png",
            "notes/sub/img2.png",
            "Templates/Bases/Books.base",
        ] {
            idx.insert(p);
        }
        (tmp, canonical, idx)
    }

    #[test]
    fn attachment_resolves_by_unique_basename() {
        let (_tmp, canonical, idx) = attachment_vault();
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "notes/a.md",
                crate::links::LinkKind::Wikilink,
                "task-plugins-sorted.png",
                None,
                Some(&idx),
            ),
            Some("02 Attachments/task-plugins-sorted.png".to_owned())
        );
    }

    #[test]
    fn attachment_resolves_by_full_vault_path_and_case_folded_path() {
        let (_tmp, canonical, idx) = attachment_vault();
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "notes/a.md",
                crate::links::LinkKind::Wikilink,
                "Templates/Bases/Books.base",
                None,
                Some(&idx),
            ),
            Some("Templates/Bases/Books.base".to_owned())
        );
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "notes/a.md",
                crate::links::LinkKind::Wikilink,
                "templates/bases/books.BASE",
                None,
                Some(&idx),
            ),
            Some("Templates/Bases/Books.base".to_owned())
        );
    }

    #[test]
    fn attachment_resolves_relative_to_the_source_folder() {
        // BUG-6: `![[sub/img2.png]]` written in `notes/a.md` names
        // `notes/sub/img2.png`, exactly as Obsidian resolves it.
        let (_tmp, canonical, idx) = attachment_vault();
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "notes/a.md",
                crate::links::LinkKind::Wikilink,
                "sub/img2.png",
                None,
                Some(&idx),
            ),
            Some("notes/sub/img2.png".to_owned())
        );
    }

    #[test]
    fn ambiguous_attachment_basename_does_not_resolve() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(tmp.path(), &["a/x.png", "b/x.png", "note.md"]);
        let canonical = canonicalize_vault_dir(tmp.path()).unwrap();
        let mut idx = CaseInsensitiveIndex::new();
        idx.set_case_insensitive_paths(true);
        for p in ["a/x.png", "b/x.png", "note.md"] {
            idx.insert(p);
        }
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "note.md",
                crate::links::LinkKind::Wikilink,
                "x.png",
                None,
                Some(&idx),
            ),
            None,
            "two attachments share the basename — Obsidian resolves neither"
        );
    }

    #[test]
    fn a_bare_wikilink_never_resolves_to_an_attachment() {
        // `[[Books]]` is a note reference; only `[[Books.base]]` names the file.
        let (_tmp, canonical, idx) = attachment_vault();
        assert_eq!(
            resolve_link_from_source(
                &canonical,
                "notes/a.md",
                crate::links::LinkKind::Wikilink,
                "Books",
                None,
                Some(&idx),
            ),
            None
        );
    }

    #[test]
    fn discover_attachments_lists_non_md_files_only() {
        let tmp = tempfile::tempdir().unwrap();
        make_files(
            tmp.path(),
            &["a.md", "img/x.png", "Templates/B.base", "plain"],
        );
        let mut found = discover_attachments(tmp.path()).unwrap();
        found.sort();
        assert_eq!(found, vec!["Templates/B.base", "img/x.png"]);
    }
}
