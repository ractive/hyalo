#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;

/// Maximum number of symlink hops followed when resolving a write destination.
///
/// A chain longer than this is treated as a loop (or as pathological) and the
/// write is refused rather than spun on.
const MAX_SYMLINK_HOPS: usize = 32;

/// Resolve a write destination through any symlink chain, returning the real
/// file that should be replaced.
///
/// Deliberately *not* `fs::canonicalize`: the destination of a write may not
/// exist yet (creating a brand-new note), and `canonicalize` fails outright in
/// that case. This walks `read_link` manually instead, stopping at the first
/// component that is not a symlink — existing or not.
///
/// Relative link targets are resolved against the directory holding the link,
/// matching kernel semantics. Returns `path` unchanged when it is not a
/// symlink, which is the common case and costs one `symlink_metadata` call.
fn resolve_write_target(path: &Path) -> Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        let is_symlink =
            std::fs::symlink_metadata(&current).is_ok_and(|m| m.file_type().is_symlink());
        if !is_symlink {
            return Ok(current);
        }
        let target = std::fs::read_link(&current)
            .with_context(|| format!("failed to read symlink {}", current.display()))?;
        current = if target.is_absolute() {
            target
        } else {
            current.parent().unwrap_or(Path::new(".")).join(target)
        };
    }
    bail!(
        "refusing to write through a symlink chain deeper than {MAX_SYMLINK_HOPS} hops: {}",
        path.display()
    )
}

/// Walk up from `path` to the nearest ancestor that exists on disk.
///
/// Used to find a canonicalizable anchor when the destination of a prospective
/// write (and possibly several of its parent directories) does not exist yet.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path;
    loop {
        if current.exists() {
            return current.to_path_buf();
        }
        match current.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => current = parent,
            _ => return current.to_path_buf(),
        }
    }
}

/// The one wording every vault-boundary refusal in hyalo uses (iter-202 L-16).
///
/// `subject` names what was refused — `"file"`, `"target path"`, `"output
/// path"`, … — and `resolved` is the canonical destination the path escaped
/// to. Pass `Some(target)` whenever an actual resolution happened (symlink or
/// `..` following) so the user sees both halves of the story: the path they
/// typed and where it really lands. Pass `None` for a purely lexical rejection,
/// where no resolved target exists.
#[must_use]
pub fn outside_vault_message(subject: &str, resolved: Option<&Path>) -> String {
    match resolved {
        Some(target) => format!(
            "{subject} resolves outside vault boundary: {}",
            target.display()
        ),
        None => format!("{subject} resolves outside vault boundary"),
    }
}

/// Like [`outside_vault_message`] but names the effective vault directory so
/// the caller can act on the refusal without a second probe (iter-235,
/// agent-ergonomics review finding 3). An agent that passed `/tmp/foo.md`
/// learns *which* directory is the vault root and how to fix the invocation
/// in the same message, rather than guessing another absolute path.
///
/// The vault dir is appended as `(vault: <dir>)` after the existing subject
/// and resolved-target clause, keeping the long-standing ``resolves outside
/// vault boundary`` substring intact so existing assertions keep passing.
/// `resolved` is the canonical destination the path escaped to, when known.
#[must_use]
pub fn outside_vault_message_with_dir(
    subject: &str,
    resolved: Option<&Path>,
    vault_dir: &Path,
) -> String {
    let base = outside_vault_message(subject, resolved);
    format!("{base} (vault: {})", vault_dir.display())
}

/// The shared self-healing hint text for a vault-boundary refusal (iter-235).
///
/// Names the vault dir the caller should anchor against — pass the same
/// `vault_dir` given to [`outside_vault_message_with_dir`] — and the two ways
/// to fix the invocation: a relative path, or a `cd` to a parent of the vault.
/// Quote the dir back to the caller because the most common failure mode is
/// an agent guessing absolute paths with no idea which directory hyalo is
/// actually rooted at.
#[must_use]
pub fn outside_vault_hint(vault_dir: &Path) -> String {
    format!(
        "pass a path relative to {}, or cd to a parent of it",
        vault_dir.display()
    )
}

/// Report where a prospective write to `path` would really land, when that is
/// outside `vault_root`.
///
/// Resolves `path` through any symlink chain, then anchors on the nearest
/// ancestor that exists on disk (the destination itself, its parent, or higher
/// for a brand-new nested file) and canonicalizes that. Because
/// `fs::create_dir_all` only ever creates plain directories — never symlinks —
/// an in-vault anchor guarantees every component created below it also stays
/// in the vault.
///
/// Returns:
/// - `Ok(None)` — the write stays inside the vault
/// - `Ok(Some(target))` — the write would land at `target`, outside the vault
/// - `Err(_)` — the vault root or the anchor could not be canonicalized
///
/// Call this *before* any `create_dir_all`/`OpenOptions::create_new`/rename so
/// the refusal is a user error (exit 1) rather than a mid-write failure.
pub fn escaping_write_target(vault_root: &Path, path: &Path) -> Result<Option<PathBuf>> {
    let canonical_root = dunce::canonicalize(vault_root).with_context(|| {
        format!(
            "failed to canonicalize vault dir {} while checking a write destination",
            vault_root.display()
        )
    })?;
    let dest = resolve_write_target(path)?;
    let anchor = nearest_existing_ancestor(&dest);
    let canonical_anchor = dunce::canonicalize(&anchor)
        .with_context(|| format!("failed to resolve write destination {}", anchor.display()))?;
    if canonical_anchor.starts_with(&canonical_root) {
        return Ok(None);
    }
    // Re-attach the not-yet-existing components so the reported target names
    // the real file rather than the deepest directory that happens to exist.
    // The boundary decision itself is made on the anchor alone, so a `..` in
    // the suffix cannot widen what is accepted.
    let suffix = dest.strip_prefix(&anchor).unwrap_or(Path::new(""));
    if suffix.as_os_str().is_empty() {
        // `join("")` would append a trailing separator; the anchor *is* the
        // destination here.
        return Ok(Some(canonical_anchor));
    }
    Ok(Some(canonical_anchor.join(suffix)))
}

/// Verify that a symlink-resolved destination is still inside `vault_root`.
///
/// Called only when resolution actually changed the path, so ordinary
/// (non-symlink) writes pay no extra syscalls.
fn ensure_resolved_within(vault_root: &Path, original: &Path, resolved: &Path) -> Result<()> {
    let canonical_root = dunce::canonicalize(vault_root).with_context(|| {
        format!(
            "failed to canonicalize vault dir {} while checking symlink target",
            vault_root.display()
        )
    })?;
    // The resolved file itself may not exist yet (dangling symlink), so
    // canonicalize its parent and re-attach the file name.
    let parent = resolved
        .parent()
        .context("cannot determine parent directory of symlink target")?;
    let canonical_parent = dunce::canonicalize(parent).with_context(|| {
        format!(
            "failed to resolve symlink target directory {}",
            parent.display()
        )
    })?;
    let canonical_target = match resolved.file_name() {
        Some(name) => canonical_parent.join(name),
        None => canonical_parent,
    };
    if !canonical_target.starts_with(&canonical_root) {
        bail!(
            "refusing to write through symlink: {}",
            outside_vault_message(&original.display().to_string(), Some(&canonical_target))
        );
    }
    Ok(())
}

/// Write data to a file atomically.
///
/// Creates a temporary file in the same directory as the destination, writes
/// all data, flushes it to stable storage, then renames it into place. A crash
/// mid-write therefore never leaves a truncated or corrupted file — the
/// original is either fully replaced or left untouched.
///
/// **Durability:** the temp file is `sync_all`ed *before* the rename, and on
/// Unix the destination's parent directory is `sync_all`ed *after* it, so the
/// rename itself survives a power loss. The directory fsync is best-effort —
/// some filesystems reject `fsync` on a directory handle, and failing the whole
/// write there would be worse than the missing guarantee (see DEC-063).
///
/// **Symlinks:** when the destination is a symlink, the link is followed (up to
/// [`MAX_SYMLINK_HOPS`] hops) and the *target* is replaced; the symlink stays a
/// symlink (DEC-062). Following is unconditional here — this entry point has no
/// vault context — so callers must have already validated the path. Every CLI
/// mutation path does, via `discovery::resolve_file`, which canonicalizes and
/// enforces the vault boundary. Callers that hold the vault root should prefer
/// [`atomic_write_within`], which re-checks the boundary against the *resolved*
/// destination.
///
/// When the destination already exists, its permissions are preserved.
/// `NamedTempFile` defaults to mode `0600`, so without this step rewrites
/// would silently tighten file permissions on every mutation.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    write_impl(None, path, data)
}

/// Like [`atomic_write`], but refuses to follow a symlink whose target escapes
/// `vault_root`.
///
/// Use this from every call site that knows the vault directory. The extra
/// canonicalization runs only when the destination really was a symlink, so
/// the ordinary write path is unaffected.
///
/// **Residual TOCTOU:** the boundary check (in [`ensure_resolved_within`])
/// canonicalizes and validates the resolved destination, but the temp-file
/// creation and rename that follow use the *non-canonical* path again. If a
/// directory component on that path is swapped for a symlink pointing outside
/// the vault in the narrow window between the check and the write — e.g. by a
/// concurrent process — the write can still land outside the vault. This is
/// accepted as a low residual risk for a single-user local CLI tool; it is
/// not a defense against a concurrent adversary with write access to the
/// vault directory.
pub fn atomic_write_within(vault_root: &Path, path: &Path, data: &[u8]) -> Result<()> {
    write_impl(Some(vault_root), path, data)
}

fn write_impl(vault_root: Option<&Path>, path: &Path, data: &[u8]) -> Result<()> {
    // Inside a bulk [`WritePhase`] the durability fsyncs are the phase's to
    // pay, once per directory at the end (DEC-317); outside one, every write
    // carries the full guarantee on its own.
    let Some(phase) = active_phase() else {
        let parent = write_no_dir_sync(vault_root, path, data, true)?;
        sync_parent_dir(&parent);
        return Ok(());
    };
    let parent = write_no_dir_sync(vault_root, path, data, phase.durable_per_file)?;
    if let Ok(mut dirs) = phase.dirs.lock() {
        dirs.insert(parent);
    }
    phase.tick();
    Ok(())
}

/// The body of [`write_impl`] up to — but not including — the parent-directory
/// fsync, returning the directory that still needs syncing.
///
/// Split out for [`BatchWriter`], which collects those directories and syncs
/// each one *once* at the end of a write phase instead of once per file.
fn write_no_dir_sync(
    vault_root: Option<&Path>,
    path: &Path,
    data: &[u8],
    fsync_data: bool,
) -> Result<PathBuf> {
    let dest = resolve_write_target(path)?;
    // The boundary re-check only matters when resolution actually moved the
    // destination — an ordinary write pays no extra syscalls for it.
    if dest != path
        && let Some(root) = vault_root
    {
        ensure_resolved_within(root, path, &dest)?;
    }
    let dest = dest.as_path();

    let parent = dest
        .parent()
        .context("cannot determine parent directory for atomic write")?;

    // Capture existing permissions (if any) before the rename so we can restore
    // them on the new file — otherwise `NamedTempFile`'s default `0600` wins.
    let existing_perms = std::fs::metadata(dest).ok().map(|m| m.permissions());

    let mut tmp = NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;

    tmp.write_all(data)
        .with_context(|| format!("failed to write temp file for {}", dest.display()))?;

    if let Some(perms) = existing_perms.clone() {
        std::fs::set_permissions(tmp.path(), perms).with_context(|| {
            format!(
                "failed to restore permissions on temp file for {}",
                dest.display()
            )
        })?;
    }

    // Flush the data before the rename: without this, a crash right after the
    // rename can leave the new directory entry pointing at unwritten blocks.
    // A bulk phase (DEC-317, iter-277) opts out of this barrier — see
    // [`BatchWriter`] for what it trades and why.
    if fsync_data {
        tmp.as_file()
            .sync_all()
            .with_context(|| format!("failed to flush temp file for {}", dest.display()))?;
    }

    tmp.persist(dest)
        .with_context(|| format!("failed to persist temp file to {}", dest.display()))?;

    // On some platforms `persist` can reset the mode; re-apply for safety.
    if let Some(perms) = existing_perms {
        std::fs::set_permissions(dest, perms)
            .with_context(|| format!("failed to restore permissions on {}", dest.display()))?;
    }

    Ok(parent.to_path_buf())
}

/// Largest write phase that still pays the per-file durability fsync.
///
/// A `mv` touching a handful of backlinks, or a `set` over three files, costs
/// well under a tenth of a second at the full guarantee, so it keeps it. Past
/// this the phase is a bulk rewrite and DEC-317 applies.
const DURABLE_BATCH_MAX: usize = 8;

/// How many files a write phase must reach before it reports progress.
///
/// Below this a bulk write finishes faster than a reader notices, and a
/// progress line would be noise on every two-file `set`.
const PROGRESS_THRESHOLD: usize = 200;

/// How often, in files, the progress line is refreshed past the threshold.
const PROGRESS_INTERVAL: usize = 200;

/// Shared state of the write phase currently in progress, if any.
struct PhaseState {
    dirs: Mutex<BTreeSet<PathBuf>>,
    done: AtomicUsize,
    reported: AtomicUsize,
    total: usize,
    label: &'static str,
    quiet: bool,
    durable_per_file: bool,
}

/// The active phase. `None` outside a bulk command, which is the state every
/// single-file mutation and every test runs in.
static PHASE: Mutex<Option<Arc<PhaseState>>> = Mutex::new(None);

/// Read the active phase, if one is installed.
fn active_phase() -> Option<Arc<PhaseState>> {
    PHASE.lock().ok().and_then(|p| p.clone())
}

/// A command's bulk write phase (iter-277, BUG-14).
///
/// Every file written while this guard is alive still gets a temp file in the
/// destination directory and an atomic `rename` into place, so a reader — and
/// a crashed process — never sees a half-written note. That is the guarantee
/// [`atomic_write`] documents as *atomicity*, and it holds here unchanged.
///
/// **DEC-317 — what a bulk phase trades.** [`atomic_write`] additionally
/// `sync_all`s each temp file before the rename, which buys *durability*
/// against power loss and kernel panic. On macOS `sync_all` is
/// `fcntl(F_FULLFSYNC)`, a whole-device barrier costing ~5.5 ms per file
/// regardless of size, and it does **not** amortize across threads (measured
/// on APFS: 5.25 ms/file serial, 4.08 ms/file on four threads). That barrier
/// was the entirety of the 49 s the Obsidian Hub's `lint --fix` and the 25 s a
/// 2190-backlink `mv` took. A phase declaring more than [`DURABLE_BATCH_MAX`]
/// files therefore skips the per-file barrier and fsyncs each touched
/// *directory* once when the guard drops (~0.13 ms each), making the renames
/// durable and leaving the last-written data blocks to the filesystem's own
/// flush. A power cut mid-phase can then lose the content of files already
/// renamed — recovered by re-running a command that is reproducible by
/// construction, over version-tracked markdown. Small phases, and every
/// single-file mutation outside a phase, keep the full guarantee.
///
/// The guard is *ambient*: it installs itself process-wide so that
/// [`atomic_write_within`] picks it up wherever the write happens, including
/// from rayon workers. It is the same shape as
/// [`crate::warn::set_verbose_skips`] and `discovery::set_scan_exclude`, and
/// it exists for the same reason — the write sites are five commands deep and
/// threading a parameter through every one of them buys nothing.
///
/// Nesting is a no-op: a phase begun while another is active returns an inert
/// guard, so an inner command cannot end the outer command's phase early.
///
/// **Windows:** directory handles cannot be `fsync`ed there, so
/// [`sync_parent_dir`] is a no-op and a bulk phase's durability rests on the
/// filesystem's own ordering. Atomicity is unchanged on both platforms.
#[must_use = "the phase ends when the guard is dropped"]
pub struct WritePhase {
    /// `false` for a nested guard, which must not tear down the outer phase.
    owns: bool,
}

impl WritePhase {
    /// Begin a write phase over `total` files.
    ///
    /// `label` names the phase in the progress line ("rewriting links",
    /// "applying fixes"). `total` decides both whether the phase is small
    /// enough to keep the per-file durability fsync (see
    /// [`DURABLE_BATCH_MAX`]) and whether it is long enough to be worth
    /// reporting progress for; `-q` silences the report either way, via
    /// [`crate::warn::quiet_progress`].
    pub fn begin(total: usize, label: &'static str) -> Self {
        let Ok(mut slot) = PHASE.lock() else {
            return Self { owns: false };
        };
        if slot.is_some() {
            return Self { owns: false };
        }
        *slot = Some(Arc::new(PhaseState {
            dirs: Mutex::new(BTreeSet::new()),
            done: AtomicUsize::new(0),
            reported: AtomicUsize::new(0),
            total,
            label,
            quiet: crate::warn::quiet_progress(),
            durable_per_file: total <= DURABLE_BATCH_MAX,
        }));
        Self { owns: true }
    }
}

impl Drop for WritePhase {
    fn drop(&mut self) {
        if !self.owns {
            return;
        }
        let Some(state) = PHASE.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        let dirs = state
            .dirs
            .lock()
            .map(|d| d.clone())
            .unwrap_or_else(|p| p.into_inner().clone());
        for dir in &dirs {
            sync_parent_dir(dir);
        }
        let done = state.done.load(Ordering::Relaxed);
        if !state.quiet && state.total >= PROGRESS_THRESHOLD && done > 0 {
            eprintln!("{}: {done}/{} files, done", state.label, state.total);
        }
    }
}

impl PhaseState {
    /// Count one completed write and print a progress line when the phase is
    /// long enough to be mistaken for a hang (PERF-3: 49 s of silence reads as
    /// a crash).
    fn tick(&self) {
        let done = self.done.fetch_add(1, Ordering::Relaxed) + 1;
        if self.quiet || self.total < PROGRESS_THRESHOLD {
            return;
        }
        // One reporter at a time: `reported` advances only when this thread
        // wins the compare-exchange, so concurrent workers never interleave
        // two lines for the same milestone.
        let last = self.reported.load(Ordering::Relaxed);
        if done < last + PROGRESS_INTERVAL {
            return;
        }
        if self
            .reported
            .compare_exchange(last, done, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            eprintln!("{}: {done}/{} files", self.label, self.total);
        }
    }
}

/// Best-effort fsync of the directory holding a just-renamed file, so the
/// rename itself is durable. No-op on non-Unix targets, where directory
/// handles cannot be opened this way.
#[cfg(unix)]
fn sync_parent_dir(parent: &Path) {
    if let Ok(dir) = std::fs::File::open(parent) {
        drop(dir.sync_all());
    }
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("output.txt");
        atomic_write(&target, b"hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "hello world");
    }

    #[test]
    fn atomic_write_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("output.txt");
        std::fs::write(&target, "old content").unwrap();
        atomic_write(&target, b"new content").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new content");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_0644() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("mode-0644.txt");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o644)).unwrap();
        atomic_write(&target, b"new content").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "mode should be preserved across rewrite");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_existing_mode_0600() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("mode-0600.txt");
        std::fs::write(&target, "old").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        atomic_write(&target, b"new content").unwrap();
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "tight mode should be preserved across rewrite");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_new_file_uses_platform_default() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("brand-new.txt");
        atomic_write(&target, b"data").unwrap();
        // For a brand-new file we don't enforce a specific mode — just make sure the
        // file was created. Platform umask governs the exact bits.
        let mode = std::fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert!(mode != 0, "mode should be non-zero: {mode:o}");
    }

    /// Durability is not observable from a unit test — a real power-loss
    /// window cannot be reproduced in-process. Assert it structurally instead:
    /// the `sync_all` call must appear before `persist` in the source, and the
    /// parent-directory fsync after it (iter-191).
    #[test]
    fn atomic_write_syncs_before_persist() {
        let src = include_str!("fs_util.rs");
        let sync = src
            .find("tmp.as_file()")
            .expect("atomic_write must sync the temp file");
        let persist = src
            .find("tmp.persist(dest)")
            .expect("atomic_write must persist the temp file");
        let dir_sync = src
            .find("sync_parent_dir(parent);")
            .expect("atomic_write must fsync the parent directory");
        assert!(
            sync < persist,
            "sync_all must happen before persist, otherwise the rename can \
             expose unwritten blocks"
        );
        assert!(
            persist < dir_sync,
            "the parent directory fsync must follow the rename it is making durable"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_follows_symlink_and_keeps_it_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        atomic_write(&link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the symlink must survive the write"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_follows_relative_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("notes")).unwrap();
        let target = tmp.path().join("notes").join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink("notes/real.md", &link).unwrap();

        atomic_write(&link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_within_refuses_symlink_escaping_root() {
        let outside = tempfile::tempdir().unwrap();
        let escapee = outside.path().join("secret.md");
        std::fs::write(&escapee, "untouched").unwrap();

        let vault = tempfile::tempdir().unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&escapee, &link).unwrap();

        let err = atomic_write_within(vault.path(), &link, b"pwned").unwrap_err();
        assert!(
            err.to_string().contains("outside vault boundary"),
            "got: {err}"
        );
        assert_eq!(
            std::fs::read_to_string(&escapee).unwrap(),
            "untouched",
            "the out-of-vault file must not be modified"
        );
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_within_allows_intra_vault_symlink() {
        let vault = tempfile::tempdir().unwrap();
        let target = vault.path().join("real.md");
        std::fs::write(&target, "old").unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        atomic_write_within(vault.path(), &link, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_refuses_symlink_loop() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a.md");
        let b = tmp.path().join("b.md");
        std::os::unix::fs::symlink(&b, &a).unwrap();
        std::os::unix::fs::symlink(&a, &b).unwrap();

        let err = atomic_write(&a, b"data").unwrap_err();
        assert!(err.to_string().contains("symlink chain"), "got: {err}");
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_creates_target_of_dangling_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("not-yet.md");
        let link = tmp.path().join("alias.md");
        std::os::unix::fs::symlink(&missing, &link).unwrap();

        atomic_write(&link, b"created").unwrap();

        assert_eq!(std::fs::read_to_string(&missing).unwrap(), "created");
    }

    #[test]
    fn outside_vault_message_two_path_form() {
        let msg = outside_vault_message("file", Some(Path::new("/elsewhere/secret.md")));
        assert_eq!(
            msg,
            "file resolves outside vault boundary: /elsewhere/secret.md"
        );
    }

    #[test]
    fn outside_vault_message_without_resolved_target() {
        assert_eq!(
            outside_vault_message("path", None),
            "path resolves outside vault boundary"
        );
    }

    #[test]
    fn outside_vault_message_with_dir_appends_vault() {
        let msg = outside_vault_message_with_dir(
            "file",
            Some(Path::new("/elsewhere/secret.md")),
            Path::new("/home/me/kb"),
        );
        assert!(
            msg.contains("file resolves outside vault boundary: /elsewhere/secret.md"),
            "keeps the long-standing substring: {msg}"
        );
        assert!(
            msg.contains("(vault: /home/me/kb)"),
            "names the effective vault dir: {msg}"
        );
    }

    #[test]
    fn outside_vault_message_with_dir_no_resolved_target() {
        let msg = outside_vault_message_with_dir("path", None, Path::new("/repo/docs"));
        assert!(
            msg.contains("path resolves outside vault boundary"),
            "base message intact: {msg}"
        );
        assert!(
            msg.contains("(vault: /repo/docs)"),
            "vault dir still named: {msg}"
        );
    }

    #[test]
    fn outside_vault_hint_names_dir_and_two_fixes() {
        let hint = outside_vault_hint(Path::new("/home/me/kb"));
        assert!(
            hint.contains("relative to /home/me/kb"),
            "names the vault dir: {hint}"
        );
        assert!(
            hint.contains("cd to a parent"),
            "offers the cd alternative: {hint}"
        );
    }

    #[test]
    fn escaping_write_target_accepts_plain_in_vault_path() {
        let vault = tempfile::tempdir().unwrap();
        let dest = vault.path().join("note.md");
        std::fs::write(&dest, "x").unwrap();
        assert!(
            escaping_write_target(vault.path(), &dest)
                .unwrap()
                .is_none(),
            "an ordinary in-vault file must not be refused"
        );
    }

    #[test]
    fn escaping_write_target_accepts_not_yet_existing_nested_path() {
        let vault = tempfile::tempdir().unwrap();
        let dest = vault.path().join("a").join("b").join("new.md");
        assert!(
            escaping_write_target(vault.path(), &dest)
                .unwrap()
                .is_none(),
            "a brand-new nested destination anchors on the vault root"
        );
    }

    #[cfg(unix)]
    #[test]
    fn escaping_write_target_reports_symlink_escape() {
        let outside = tempfile::tempdir().unwrap();
        let escapee = outside.path().join("secret.md");
        std::fs::write(&escapee, "untouched").unwrap();
        let vault = tempfile::tempdir().unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&escapee, &link).unwrap();

        let target = escaping_write_target(vault.path(), &link)
            .unwrap()
            .expect("the escape must be reported");
        assert_eq!(
            target,
            dunce::canonicalize(&escapee).unwrap(),
            "the reported target must be the canonical destination"
        );
    }

    #[cfg(unix)]
    #[test]
    fn escaping_write_target_reports_escape_through_symlinked_directory() {
        let outside = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), vault.path().join("outdir")).unwrap();

        // Neither the file nor its intermediate directories exist yet: the
        // check must anchor on the symlinked directory, which already does.
        let dest = vault.path().join("outdir").join("a").join("planted.md");
        let target = escaping_write_target(vault.path(), &dest)
            .unwrap()
            .expect("a write below a symlinked-out directory must be refused");
        assert!(
            target.ends_with("a/planted.md"),
            "the reported target keeps the not-yet-created components: {}",
            target.display()
        );
        assert!(
            !outside.path().join("a").exists(),
            "the check must not create anything"
        );
    }

    #[cfg(unix)]
    #[test]
    fn escaping_write_target_accepts_intra_vault_symlink() {
        let vault = tempfile::tempdir().unwrap();
        let real = vault.path().join("real.md");
        std::fs::write(&real, "x").unwrap();
        let link = vault.path().join("alias.md");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        assert!(
            escaping_write_target(vault.path(), &link)
                .unwrap()
                .is_none(),
            "a symlink that stays inside the vault is fine"
        );
    }

    #[test]
    fn atomic_write_fails_if_parent_missing() {
        let tmp = tempfile::tempdir().unwrap();
        // The "missing" subdirectory does not exist, so the temp file cannot be created.
        let target = tmp.path().join("missing").join("file.txt");
        let err = atomic_write(&target, b"data").unwrap_err();
        assert!(
            err.to_string().contains("failed to create temp file"),
            "got: {err}"
        );
    }
}
