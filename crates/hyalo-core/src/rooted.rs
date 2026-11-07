//! Checked filesystem operations. Checks close static escapes; they do not
//! promise protection against directory swaps between checks and system calls.
//! Entry identity (a name, possibly a symlink) is distinct from file identity.
//! Atomic publication and fallible durability finalization are separate effects.
#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeMap, BTreeSet, HashMap, hash_map::DefaultHasher};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use anyhow::{Context, Result, bail};
use tempfile::{NamedTempFile, TempPath};

mod barrier;

/// A captured source no longer denotes the bytes and identity used to plan.
/// Consumers classify this marker without parsing human-readable prose.
#[derive(Debug)]
pub struct SourceConflict(pub &'static str);
impl std::fmt::Display for SourceConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "conflict: {}", self.0)
    }
}
impl std::error::Error for SourceConflict {}

/// A lexical relative name, never an authorization token.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RelativeName(PathBuf);

impl RelativeName {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!(
                "expected a nonempty relative path without parent traversal: {}",
                path.display()
            );
        }
        // C-10 / L-19: Windows resolves `CON.md` to a device, `a:b.md` to an
        // alternate data stream and `a.md.` to `a.md`; no such name can be a
        // note there. Elsewhere they are ordinary file names.
        if cfg!(windows)
            && let Some(bad) = path.components().find_map(|c| match c {
                Component::Normal(name) => name.to_str().filter(|n| !is_portable_windows_name(n)),
                _ => None,
            })
        {
            bail!("not a valid file name on Windows: {bad}");
        }
        let clean: PathBuf = path
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect();
        if clean.as_os_str().is_empty() {
            bail!("expected a file name");
        }
        Ok(Self(clean))
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

/// Whether `name` (one path component) is a name Windows will not rewrite or
/// redirect: no device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`,
/// `LPT1`-`LPT9`, with or without an extension), no `:` (alternate data
/// stream) and no trailing dot or space.
#[must_use]
pub fn is_portable_windows_name(name: &str) -> bool {
    if name.contains(':') || name.ends_with(['.', ' ']) {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let device = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0');
    !device
}

#[derive(Debug, Clone)]
struct Root(PathBuf);

impl Root {
    fn new(path: &Path) -> Result<Self> {
        let path = dunce::canonicalize(path).context("resolving filesystem root")?;
        if !path.is_dir() {
            bail!("filesystem root is not a directory");
        }
        Ok(Self(path))
    }

    fn check(&self, name: &RelativeName, follow: bool) -> Result<PathBuf> {
        if dunce::canonicalize(&self.0)? != self.0 {
            bail!("filesystem root changed");
        }
        let entry = self.0.join(name.as_path());
        let mut parent = entry.parent().context("target has no parent")?;
        while !parent.try_exists()? {
            parent = parent.parent().context("target has no existing parent")?;
        }
        // The root was freshly canonicalized above. Root-level entries do not
        // need a second canonicalization of that same parent in this check.
        if parent != self.0 && !dunce::canonicalize(parent)?.starts_with(&self.0) {
            bail!("target parent resolves outside root: {}", entry.display());
        }
        match std::fs::symlink_metadata(&entry) {
            Ok(_) => {
                // Validate the followed target even when acting on its entry.
                let resolved = dunce::canonicalize(&entry)?;
                if !resolved.starts_with(&self.0) {
                    bail!("target resolves outside root: {}", entry.display());
                }
                Ok(if follow { resolved } else { entry })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(entry),
            Err(e) => Err(e.into()),
        }
    }

    fn open(&self, name: &RelativeName) -> Result<OpenedTarget> {
        let referent = self.check(name, true)?;
        // Reject static special files before opening: opening a FIFO can block
        // before the handle metadata check below can run. Follow in-root aliases.
        if !std::fs::metadata(&referent)
            .with_context(|| format!("checking {}", name.as_path().display()))?
            .is_file()
        {
            bail!("target is not a regular file");
        }
        let file = File::open(&referent)
            .with_context(|| format!("opening {}", name.as_path().display()))?;
        if !file.metadata()?.is_file() {
            bail!("target is not a regular file");
        }
        Ok(OpenedTarget {
            file,
            entry: name.clone(),
            referent,
        })
    }
}

macro_rules! root_type {
    ($name:ident) => {
        #[doc = "A checked application root with fresh confinement checks at every operation."]
        #[derive(Debug, Clone)]
        pub struct $name(Root);
        impl $name {
            pub fn new(path: impl AsRef<Path>) -> Result<Self> {
                Ok(Self(Root::new(path.as_ref())?))
            }
            #[must_use]
            pub fn path(&self) -> &Path {
                &self.0.0
            }
            pub fn open(&self, name: &RelativeName) -> Result<OpenedTarget> {
                self.0.open(name)
            }
            pub fn capture(&self, name: &RelativeName) -> Result<CapturedInput> {
                CapturedInput::capture(self.0.clone(), self.open(name)?)
            }
            /// [`Self::capture`] with an explicit size cap instead of the
            /// note limit — for artifacts such as snapshot indexes, which
            /// legitimately exceed a note's size on large vaults.
            pub fn capture_with_limit(
                &self,
                name: &RelativeName,
                limit: u64,
            ) -> Result<CapturedInput> {
                CapturedInput::capture_with_limit(self.0.clone(), self.open(name)?, limit)
            }
            pub fn destination(&self, name: RelativeName) -> Result<NewEntry> {
                let path = self.0.check(&name, false)?;
                if std::fs::symlink_metadata(path).is_ok() {
                    bail!("destination already exists");
                }
                Ok(NewEntry {
                    root: self.0.clone(),
                    name,
                })
            }
            pub fn remove_artifact(
                &self,
                name: &RelativeName,
                session: &mut WriteSession,
            ) -> Result<CommitEffect> {
                let path = self.0.check(name, false)?;
                std::fs::remove_file(&path)?;
                Ok(session.record(&path, Operation::Removed))
            }
            pub fn create_directory(
                &self,
                name: &RelativeName,
                session: &mut WriteSession,
            ) -> Result<CommitEffect> {
                let path = self.0.check(name, false)?;
                std::fs::create_dir(&path)?;
                Ok(session.record(&path, Operation::DirectoryCreated))
            }
        }
    };
}
root_type!(VaultRoot);
root_type!(InstallationRoot);
root_type!(ConfigRoot);

/// A confined handle; consumers scan the opened file rather than reopen its path.
pub struct OpenedTarget {
    file: File,
    entry: RelativeName,
    referent: PathBuf,
}

impl OpenedTarget {
    pub fn read_bounded(&mut self, limit: u64) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        Read::by_ref(&mut self.file)
            .take(limit.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            bail!("file exceeds {limit} byte input budget");
        }
        Ok(bytes)
    }
}

/// Captured source bytes are staged on disk and compared byte-for-byte at commit.
/// Neither restored timestamps nor equal-size edits can bypass this check.
pub struct CapturedInput {
    root: Root,
    entry: RelativeName,
    referent: PathBuf,
    exact_identity: ExactIdentity,
    identity: u64,
    /// The bytes callers read through [`Self::reader`] and [`Self::bytes`].
    original: Arc<TempPath>,
    /// Set on a capture of a replacement that is staged but not yet renamed
    /// (DEC-408): the bytes the file on disk must still have. `original`
    /// then holds the staged bytes, which the file on disk does not.
    base: Option<Arc<TempPath>>,
    permissions: std::fs::Permissions,
    size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ExactIdentity {
    volume: u64,
    file: u64,
}

#[cfg(unix)]
fn exact_identity(file: &File) -> Result<ExactIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(ExactIdentity {
        volume: metadata.dev(),
        file: metadata.ino(),
    })
}

#[cfg(windows)]
fn exact_identity(file: &File) -> Result<ExactIdentity> {
    let information = winapi_util::file::information(file)?;
    Ok(ExactIdentity {
        volume: information.volume_serial_number(),
        file: information.file_index(),
    })
}

fn file_identity(file: File) -> Result<(File, ExactIdentity, u64)> {
    let exact = exact_identity(&file)?;
    let mut hasher = DefaultHasher::new();
    exact.hash(&mut hasher);
    // The hash is only a conservative duplicate/conflict guard. A collision
    // refuses a batch; it never grants permission or establishes byte equality.
    Ok((file, exact, hasher.finish()))
}

impl CapturedInput {
    fn capture(root: Root, opened: OpenedTarget) -> Result<Self> {
        Self::capture_with_limit(root, opened, crate::scanner::MAX_FILE_SIZE)
    }

    fn capture_with_limit(root: Root, opened: OpenedTarget, limit: u64) -> Result<Self> {
        let (mut file, exact_identity, identity) = file_identity(opened.file)?;
        let metadata = file.metadata()?;
        let oversized = || {
            crate::user_error(format!(
                "file too large: {} exceeds {} MiB limit",
                opened.entry.as_path().display(),
                limit / (1024 * 1024)
            ))
        };
        if metadata.len() > limit {
            return Err(oversized());
        }
        let mut original = NamedTempFile::new()?;
        let size = std::io::copy(&mut Read::by_ref(&mut file).take(limit + 1), &mut original)?;
        if size > limit {
            return Err(oversized());
        }
        Ok(Self {
            root,
            entry: opened.entry,
            referent: opened.referent,
            exact_identity,
            identity,
            original: Arc::new(original.into_temp_path()),
            base: None,
            permissions: metadata.permissions(),
            size,
        })
    }

    #[must_use]
    pub fn name(&self) -> &RelativeName {
        &self.entry
    }
    #[must_use]
    pub fn physical_identity(&self) -> u64 {
        self.identity
    }
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }
    pub fn reader(&self) -> Result<File> {
        Ok(File::open(&**self.original)?)
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        Ok(std::fs::read(&**self.original)?)
    }

    /// The bytes the file on disk must have for this capture to hold.
    fn verification_base(&self) -> &Arc<TempPath> {
        self.base.as_ref().unwrap_or(&self.original)
    }

    pub fn verify(&self) -> Result<()> {
        let opened = self.root.open(&self.entry).map_err(|error| {
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
            {
                anyhow::Error::new(SourceConflict("source was removed"))
            } else {
                error
            }
        })?;
        if opened.referent != self.referent {
            bail!(SourceConflict("target referent changed"));
        }
        let (mut current, exact_identity, identity) = file_identity(opened.file)?;
        if exact_identity != self.exact_identity || identity != self.identity {
            bail!(SourceConflict("target file identity changed"));
        }
        let mut original = File::open(&***self.verification_base())?;
        let buffer_size = usize::try_from(self.size.clamp(1, 64 * 1024))?;
        let mut left = vec![0u8; buffer_size].into_boxed_slice();
        let mut right = vec![0u8; buffer_size].into_boxed_slice();
        loop {
            let n = original.read(&mut left)?;
            if n == 0 {
                if current.read(&mut right[..1])? != 0 {
                    bail!(SourceConflict("source bytes changed"));
                }
                break;
            }
            if let Err(error) = current.read_exact(&mut right[..n]) {
                if error.kind() == std::io::ErrorKind::UnexpectedEof {
                    bail!(SourceConflict("source bytes changed"));
                }
                return Err(error.into());
            }
            if left[..n] != right[..n] {
                bail!(SourceConflict("source bytes changed"));
            }
        }
        Ok(())
    }

    pub fn prepare(self, bytes: &[u8], session: &WriteSession) -> Result<PreparedReplacement> {
        session.fault(FaultPoint::Prepare)?;
        let mut data = NamedTempFile::new()?;
        data.write_all(bytes)?;
        Ok(PreparedReplacement {
            source: self,
            data: data.into_temp_path(),
        })
    }

    /// Prepare and publish already-validated final bytes, without staging a
    /// second copy for a receipt the caller does not need. Only the active
    /// worker owns the sibling temporary file; no bytes or handles are
    /// retained beyond this call. Under `BulkRewrite` "publish" queues the
    /// replacement: it reaches its name when the session finishes (DEC-408).
    pub(crate) fn replace_immediate(
        self,
        bytes: &[u8],
        session: &mut WriteSession,
    ) -> Result<CommitEffect> {
        session.fault(FaultPoint::Prepare).inspect_err(|error| {
            session.poison_unless_local(error);
        })?;
        let (path, temp) = self.stage_replacement(session, |temp| {
            temp.write_all(bytes)?;
            Ok(())
        })?;
        self.publish_replacement(&path, temp, session)
    }

    fn stage_replacement(
        &self,
        session: &WriteSession,
        write: impl FnOnce(&mut NamedTempFile) -> Result<()>,
    ) -> Result<(PathBuf, NamedTempFile)> {
        self.verify()?;
        let path = self.root.check(&self.entry, true)?;
        let parent = path.parent().context("target has no parent")?;
        let staged = (|| -> Result<NamedTempFile> {
            let mut temp = NamedTempFile::new_in(parent)?;
            write(&mut temp)?;
            temp.as_file().set_permissions(self.permissions.clone())?;
            session.flush(temp.as_file())?;
            session.probe(|| format!("stage {}", entry_name(&self.entry)));
            Ok(temp)
        })();
        match staged {
            Ok(temp) => Ok((path, temp)),
            Err(error) => {
                session.poison_unless_local(&error);
                Err(error)
            }
        }
    }

    fn publish_replacement(
        self,
        path: &Path,
        temp: NamedTempFile,
        session: &mut WriteSession,
    ) -> Result<CommitEffect> {
        if session.durability == Durability::BulkRewrite {
            // The content is flushed; the name changes at `finish`, after one
            // barrier for every staged note, and `finish` verifies the source
            // again right before it renames (DEC-408).
            let device = match barrier::device_of(temp.as_file()) {
                Ok(device) => device,
                Err(error) => {
                    let error = anyhow::Error::from(error);
                    session.poison_unless_local(&error);
                    return Err(error);
                }
            };
            session.stage(
                path.to_path_buf(),
                Staged {
                    entry: self.entry.clone(),
                    temp: temp.into_temp_path(),
                    source: self,
                    device,
                },
            );
            return Ok(WriteSession::queued(path));
        }
        session.fault(FaultPoint::Persist)?;
        self.verify()?;
        temp.persist(path).map_err(|e| e.error)?;
        Ok(session.record(path, Operation::Replaced))
    }

    /// Remove the exact captured entry after rechecking identity and bytes.
    /// A replacement or edit after capture is a source conflict, not removal
    /// authority.
    pub fn remove(self, session: &mut WriteSession) -> Result<CommitEffect> {
        self.verify()?;
        let path = self.root.check(&self.entry, false)?;
        session.fault(FaultPoint::Remove)?;
        std::fs::remove_file(&path)?;
        Ok(session.record(&path, Operation::Removed))
    }
}

/// A new name is rechecked during exclusive publication.
pub struct NewEntry {
    root: Root,
    name: RelativeName,
}

impl NewEntry {
    pub fn create(self, bytes: &[u8], session: &mut WriteSession) -> Result<CommitEffect> {
        session.fault(FaultPoint::Prepare)?;
        let path = self.root.check(&self.name, false)?;
        let parent = path.parent().context("target has no parent")?;
        let mut temp = crate::fs_util::temp_for_new_file(parent)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        session.fault(FaultPoint::Persist)?;
        self.root.check(&self.name, false)?;
        temp.persist_noclobber(&path).map_err(|e| e.error)?;
        Ok(session.record(&path, Operation::Created))
    }
}

pub struct PreparedReplacement {
    source: CapturedInput,
    data: TempPath,
}

/// Exact ownership receipt for bytes published by this operation.
///
/// The live entry must retain both the original filesystem object identity and
/// the exact last-published bytes before it can authorize compensation.
pub struct OwnedPublication {
    root: Root,
    entry: RelativeName,
    handle: same_file::Handle,
    expected: Arc<TempPath>,
    /// Set while the replacement is staged and the file on disk is still the
    /// one it replaces (DEC-408).
    staged_over: Option<StagedOver>,
}

/// What the entry on disk must still be while a replacement waits for its rename.
struct StagedOver {
    identity: ExactIdentity,
    base: Arc<TempPath>,
}

impl OwnedPublication {
    #[must_use]
    pub fn name(&self) -> &RelativeName {
        &self.entry
    }

    /// Capture the entry as this publication left it.
    ///
    /// While the replacement is only staged (a bulk phase renames at its end,
    /// DEC-408) the file on disk is still the old one: the capture then holds
    /// the staged bytes and verifies against the old file, so a second write
    /// to the same note in the phase replaces the first instead of reading
    /// bytes that are not there yet.
    pub fn capture_verified(&self) -> Result<CapturedInput> {
        let path = self.root.check(&self.entry, false)?;
        if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!(SourceConflict(
                "published entry kind changed before compensation"
            ));
        }
        let mut captured = CapturedInput::capture(self.root.clone(), self.root.open(&self.entry)?)?;
        let expected = std::fs::read(&**self.expected)?;
        if same_file::Handle::from_path(&path)? == self.handle {
            if captured.bytes()? == expected {
                return Ok(captured);
            }
        } else if let Some(staged) = &self.staged_over
            && captured.exact_identity == staged.identity
            && captured.bytes()? == std::fs::read(&**staged.base)?
        {
            let mut staged_bytes = NamedTempFile::new()?;
            staged_bytes.write_all(&expected)?;
            captured.size = expected.len() as u64;
            captured.base = Some(Arc::clone(&staged.base));
            captured.original = Arc::new(staged_bytes.into_temp_path());
            return Ok(captured);
        }
        bail!(SourceConflict(
            "published entry identity or bytes changed before compensation"
        ));
    }
}

impl PreparedReplacement {
    #[must_use]
    pub fn name(&self) -> &RelativeName {
        self.source.name()
    }
    pub fn commit(self, session: &mut WriteSession) -> Result<CommitEffect> {
        self.commit_with_receipt(session).map(|(effect, _)| effect)
    }

    pub fn commit_with_receipt(
        self,
        session: &mut WriteSession,
    ) -> Result<(CommitEffect, OwnedPublication)> {
        let (path, temp) = self.source.stage_replacement(session, |temp| {
            let mut staged = File::open(&self.data)?;
            std::io::copy(&mut staged, temp)?;
            Ok(())
        })?;
        let handle = same_file::Handle::from_file(temp.as_file().try_clone()?)?;
        let staged_over = (session.durability == Durability::BulkRewrite).then(|| StagedOver {
            identity: self.source.exact_identity,
            base: Arc::clone(self.source.verification_base()),
        });
        let receipt = OwnedPublication {
            root: self.source.root.clone(),
            entry: self.source.entry.clone(),
            handle,
            expected: Arc::new(self.data),
            staged_over,
        };
        let effect = self.source.publish_replacement(&path, temp, session)?;
        Ok((effect, receipt))
    }
}

/// No-clobber regular-file move, implemented by exclusive hard link + unlink.
/// Unsupported filesystems and symlinks fail before changing either name.
/// A failed unlink reports the created destination as a committed effect.
#[allow(clippy::needless_pass_by_value)] // consume both prepared identities, even on partial failure
pub fn move_no_replace(
    source: CapturedInput,
    destination: NewEntry,
    session: &mut WriteSession,
) -> Result<CommitEffect> {
    move_no_replace_with_receipt(source, destination, session).map(|(effect, _)| effect)
}

#[allow(clippy::needless_pass_by_value)]
pub fn move_no_replace_with_receipt(
    source: CapturedInput,
    destination: NewEntry,
    session: &mut WriteSession,
) -> Result<(CommitEffect, OwnedPublication)> {
    source.verify()?;
    let from = source.root.check(&source.entry, false)?;
    if std::fs::symlink_metadata(&from)?.file_type().is_symlink() {
        bail!("unsupported: no-replace move of a symlink entry");
    }
    let to = destination.root.check(&destination.name, false)?;
    let opened = source.root.open(&source.entry)?;
    if opened.referent != source.referent || exact_identity(&opened.file)? != source.exact_identity
    {
        bail!(SourceConflict("target file identity changed"));
    }
    let handle = same_file::Handle::from_file(opened.file)?;
    let receipt = OwnedPublication {
        root: destination.root.clone(),
        entry: destination.name.clone(),
        handle,
        expected: source.original,
        staged_over: None,
    };
    session.fault(FaultPoint::Persist)?;
    std::fs::hard_link(&from, &to)
        .context("exclusive no-replace move is unavailable or destination exists")?;
    let mut effect = session.record(&to, Operation::Created);
    if let Err(e) = session
        .fault(FaultPoint::Remove)
        .and_then(|()| std::fs::remove_file(&from).map_err(Into::into))
    {
        effect.finalization_error =
            Some(format!("destination created; source removal failed: {e}"));
    } else {
        let removed = session.record(&from, Operation::Removed);
        effect.operation = Operation::Moved;
        effect.entries.extend(removed.entries);
        if effect.finalization_error.is_none() {
            effect.finalization_error = removed.finalization_error;
        }
    }
    Ok((effect, receipt))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Replaced,
    Created,
    Removed,
    Moved,
    DirectoryCreated,
}

#[derive(Debug)]
pub struct CommitEffect {
    operation: Operation,
    entries: Vec<EntryEffect>,
    finalization_error: Option<String>,
}

#[derive(Debug)]
pub struct EntryEffect {
    path: PathBuf,
    operation: Operation,
}
impl EntryEffect {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
    #[must_use]
    pub fn operation(&self) -> Operation {
        self.operation
    }
}

impl CommitEffect {
    #[must_use]
    pub fn operation(&self) -> Operation {
        self.operation
    }
    #[must_use]
    pub fn entries(&self) -> &[EntryEffect] {
        &self.entries
    }
    #[must_use]
    pub fn finalization_error(&self) -> Option<&str> {
        self.finalization_error.as_deref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Durability {
    PerFile,
    PerDirectory,
    /// A bulk rewrite phase (DEC-408): every replacement is written to a
    /// sibling temporary file and flushed as it is staged, the session
    /// renames none of them until [`WriteSession::finish`], and `finish`
    /// runs one barrier per device, then the renames, then the directory
    /// fences. After power loss every note is whole, the old bytes or the new,
    /// never empty; a phase may end as a mix. New entries still flush.
    BulkRewrite,
}

impl Durability {
    /// The policy for a phase that may rewrite `count` existing notes (the
    /// candidates, not the notes that turn out to change). One note keeps the
    /// per-file path; more than one is a `BulkRewrite`, which costs one
    /// barrier for the phase instead of one per note, so no smaller threshold
    /// is needed. `PerDirectory` is not that policy: it flushes every file
    /// with a full barrier, which made `lint --fix` over the Obsidian Hub take
    /// 39 s instead of 5 s (iteration 329).
    #[must_use]
    pub const fn for_rewrites(count: usize) -> Self {
        if count > 1 {
            Self::BulkRewrite
        } else {
            Self::PerFile
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    Prepare,
    SyncFile,
    /// Fires where a note's name changes: at the rename of `finish` under
    /// `BulkRewrite`, otherwise when the replacement is committed.
    Persist,
    /// The device barrier of a `BulkRewrite` phase.
    Barrier,
    Remove,
    Finalize,
}

/// A replacement written and flushed beside its note, waiting for the rename.
/// It holds a path and no open handle, so a phase can stage more notes than
/// the process may keep open.
struct Staged {
    entry: RelativeName,
    temp: TempPath,
    /// Verified again right before the rename (DEC-408).
    source: CapturedInput,
    device: Option<u64>,
}

/// Told each step of a phase before it happens (tests only).
#[cfg(test)]
type Probe = Arc<dyn Fn(&str) + Send + Sync>;

/// State every [`worker`](WriteSession::worker) of one phase shares.
#[derive(Default)]
struct Shared {
    /// The first I/O failure of a staging step. Once set, `finish` renames
    /// nothing, so a phase that cannot stage everything changes nothing.
    poison: Mutex<Option<String>>,
    fault_skip: usize,
    fault_hits: AtomicUsize,
    /// Test seam: told each step of a phase as it is about to happen, so a
    /// test can check the order against the file system itself.
    #[cfg(test)]
    probe: Option<Probe>,
}

/// A note a bulk phase staged and did not rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unpublished {
    entry: String,
    reason: String,
    conflict: bool,
}

impl Unpublished {
    /// The vault-relative name, with `/` separators.
    #[must_use]
    pub fn entry(&self) -> &str {
        &self.entry
    }
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
    /// The note changed on disk after it was read ([`SourceConflict`]).
    #[must_use]
    pub fn is_source_conflict(&self) -> bool {
        self.conflict
    }
}

/// What [`WriteSession::finish_staged`] did. The notes it names are untouched
/// and their temporary files are gone; every other queued note was renamed.
#[derive(Debug, Default)]
pub struct FinishReport {
    unpublished: Vec<Unpublished>,
    finalization: Option<String>,
}

impl FinishReport {
    #[must_use]
    pub fn unpublished(&self) -> &[Unpublished] {
        &self.unpublished
    }
    /// The directory fences failed after the renames: the renamed notes
    /// exist, but are not yet known to survive power loss.
    #[must_use]
    pub fn finalization_error(&self) -> Option<&str> {
        self.finalization.as_deref()
    }
    /// The unpublished notes by [`entry_key`], for matching a caller's own
    /// per-note effects in one pass.
    #[must_use]
    pub fn unpublished_by_entry(&self) -> HashMap<&str, &Unpublished> {
        self.unpublished
            .iter()
            .map(|note| (note.entry(), note))
            .collect()
    }
    pub fn into_result(self) -> Result<()> {
        if let Some(first) = self.unpublished.first() {
            bail!(
                "{} of the queued notes were not written, first {}: {}",
                self.unpublished.len(),
                first.entry,
                first.reason
            );
        }
        match self.finalization {
            Some(error) => bail!("{error}"),
            None => Ok(()),
        }
    }
}

/// Explicit invocation-owned durability. Drop does not report success; a
/// `BulkRewrite` session dropped before [`finish`](Self::finish) renames
/// nothing and removes its temporary files.
pub struct WriteSession {
    durability: Durability,
    directories: BTreeSet<PathBuf>,
    fault: Option<FaultPoint>,
    shared: Arc<Shared>,
    /// Replacements this worker staged, by the path they will replace.
    staged: BTreeMap<PathBuf, Staged>,
}

impl std::fmt::Debug for WriteSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WriteSession")
            .field("durability", &self.durability)
            .field("directories", &self.directories)
            .field("fault", &self.fault)
            .field("staged", &self.staged.len())
            .finish_non_exhaustive()
    }
}

impl WriteSession {
    #[must_use]
    pub fn new(durability: Durability) -> Self {
        Self {
            durability,
            directories: BTreeSet::new(),
            fault: None,
            shared: Arc::default(),
            staged: BTreeMap::new(),
        }
    }
    /// Deterministic fault seam; scoped to this session, never environment/global state.
    #[must_use]
    pub fn with_fault(durability: Durability, fault: FaultPoint) -> Self {
        Self {
            fault: Some(fault),
            ..Self::new(durability)
        }
    }
    /// [`with_fault`](Self::with_fault) that lets the first `skip` hits of the
    /// fault point through, so a fault can land on the nth note of a phase.
    #[cfg(test)]
    pub(crate) fn with_fault_after(durability: Durability, fault: FaultPoint, skip: usize) -> Self {
        Self {
            shared: Arc::new(Shared {
                fault_skip: skip,
                ..Shared::default()
            }),
            ..Self::with_fault(durability, fault)
        }
    }
    /// A session that tells `probe` each step of the phase before it happens.
    #[cfg(test)]
    fn with_probe(durability: Durability, probe: Probe) -> Self {
        Self {
            shared: Arc::new(Shared {
                probe: Some(probe),
                ..Shared::default()
            }),
            ..Self::new(durability)
        }
    }
    #[cfg(test)]
    fn probe(&self, event: impl FnOnce() -> String) {
        if let Some(probe) = &self.shared.probe {
            probe(&event());
        }
    }
    #[cfg(not(test))]
    #[inline]
    #[allow(clippy::unused_self)]
    fn probe(&self, _event: impl FnOnce() -> String) {}
    /// The durability policy this session was created with.
    #[must_use]
    pub fn durability(&self) -> Durability {
        self.durability
    }
    /// A parallel worker inherits policy, but owns its pending directories
    /// and staged replacements.
    ///
    /// `pub` (not `pub(crate)`) so a parallel bulk mutator outside this crate
    /// — e.g. `hyalo-cli`'s `lint --fix`, which lints files on the rayon pool
    /// — can give each worker its own session for the actual write and
    /// [`absorb`](Self::absorb) the results back into one top-level session
    /// afterward, rather than serializing every worker's write through a
    /// shared lock just to batch the deferred directory fence (DEC-317,
    /// codebase review 2026-10-03 item 3).
    #[must_use]
    pub fn worker(&self) -> Self {
        Self {
            durability: self.durability,
            directories: BTreeSet::new(),
            fault: self.fault,
            shared: Arc::clone(&self.shared),
            staged: BTreeMap::new(),
        }
    }
    /// Transfer a worker's pending finalization and staged replacements to the
    /// invocation owner. See [`worker`](Self::worker).
    pub fn absorb(&mut self, mut worker: Self) {
        self.directories.append(&mut worker.directories);
        self.staged.append(&mut worker.staged);
    }
    /// How many distinct directories are queued for a deferred fsync under
    /// `Durability::PerDirectory`/`BulkRewrite`, counting the directories of
    /// replacements still waiting for their rename (always 0 under `PerFile`,
    /// which fsyncs inline in `record()` and never queues).
    ///
    /// `pub` for the same cross-crate reason as [`worker`](Self::worker):
    /// `hyalo-cli`'s batched `lint --fix` proves it absorbed every parallel
    /// worker into one directory-deduplicated top-level session — not one
    /// session per file — by checking this count before calling
    /// [`finish`](Self::finish) (PR #369 review item 5).
    #[must_use]
    pub fn pending_directories(&self) -> usize {
        let mut all: BTreeSet<&Path> = self.directories.iter().map(PathBuf::as_path).collect();
        all.extend(self.staged.keys().filter_map(|path| path.parent()));
        all.len()
    }
    /// How many replacements wait for their rename.
    #[must_use]
    pub fn staged_notes(&self) -> usize {
        self.staged.len()
    }
    fn fault(&self, point: FaultPoint) -> Result<()> {
        if self.fault == Some(point)
            && self.shared.fault_hits.fetch_add(1, Ordering::Relaxed) >= self.shared.fault_skip
        {
            bail!("injected {point:?} failure");
        }
        Ok(())
    }
    /// Push a staged temporary file's content to the device. Per-file
    /// durability pays the full flush here; a bulk phase pays the cheap one
    /// and one barrier at `finish`.
    fn flush(&self, file: &File) -> Result<()> {
        self.fault(FaultPoint::SyncFile)?;
        if self.durability == Durability::BulkRewrite {
            barrier::flush_content(file)?;
        } else {
            file.sync_all()?;
        }
        Ok(())
    }
    /// Remember the first failure of a staging step so `finish` renames
    /// nothing. What concerns one note only is not one: a [`SourceConflict`],
    /// or a directory that cannot be written to (`PermissionDenied`,
    /// `NotFound`, a read-only volume). Anything else (a full disk, a failed
    /// write or flush) says the staged notes cannot be trusted as a set.
    fn poison_unless_local(&self, error: &anyhow::Error) {
        let local = error.downcast_ref::<SourceConflict>().is_some()
            || error.downcast_ref::<std::io::Error>().is_some_and(|io| {
                matches!(
                    io.kind(),
                    std::io::ErrorKind::PermissionDenied
                        | std::io::ErrorKind::NotFound
                        | std::io::ErrorKind::ReadOnlyFilesystem
                )
            });
        if self.durability != Durability::BulkRewrite || local {
            return;
        }
        self.shared
            .poison
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_or_insert_with(|| format!("{error:#}"));
    }
    fn stage(&mut self, path: PathBuf, staged: Staged) {
        // A second write to the same note replaces the first; dropping the
        // earlier entry removes its temporary file.
        self.staged.insert(path, staged);
    }
    fn queued(path: &Path) -> CommitEffect {
        CommitEffect {
            operation: Operation::Replaced,
            entries: vec![EntryEffect {
                path: path.to_path_buf(),
                operation: Operation::Replaced,
            }],
            finalization_error: None,
        }
    }
    fn record(&mut self, path: &Path, operation: Operation) -> CommitEffect {
        let result = (|| -> Result<()> {
            // Bulk workers defer both the fence and its fault seam to finish.
            if self.durability != Durability::BulkRewrite {
                self.fault(FaultPoint::Finalize)?;
            }
            let parent = path.parent().context("target has no parent")?;
            match self.durability {
                Durability::PerFile => sync_directory(parent),
                Durability::PerDirectory | Durability::BulkRewrite => {
                    self.directories.insert(parent.to_path_buf());
                    Ok(())
                }
            }
        })();
        CommitEffect {
            operation,
            entries: vec![EntryEffect {
                path: path.to_path_buf(),
                operation,
            }],
            finalization_error: result.err().map(|e| e.to_string()),
        }
    }
    /// Run the deferred end of the phase and fail if any queued note was not
    /// written or a fence failed. A caller that must say which notes changed
    /// uses [`finish_staged`](Self::finish_staged).
    pub fn finish(self) -> Result<()> {
        self.finish_staged().into_result()
    }
    /// Barrier, then renames, then directory fences (DEC-408), and report
    /// which queued notes did not make it.
    ///
    /// Nothing is renamed if a staging step failed, the barrier failed or the
    /// session is dropped first; every note is then untouched. Otherwise a
    /// note whose source changed since it was read, or whose rename failed,
    /// stays as it was and is listed, and the others are renamed: the phase
    /// can end as a mix of old and new notes, never a damaged one.
    #[must_use]
    pub fn finish_staged(mut self) -> FinishReport {
        let mut report = FinishReport::default();
        let staged = std::mem::take(&mut self.staged);
        if !staged.is_empty() {
            self.publish_staged(staged, &mut report);
        }
        report.finalization = (|| -> Result<()> {
            self.fault(FaultPoint::Finalize)?;
            for parent in &self.directories {
                self.probe(|| format!("fence {}", parent.display()));
                sync_directory(parent)?;
            }
            Ok(())
        })()
        .err()
        .map(|error| format!("{error:#}"));
        report
    }
    fn publish_staged(&mut self, staged: BTreeMap<PathBuf, Staged>, report: &mut FinishReport) {
        let refuse_all =
            |report: &mut FinishReport, staged: BTreeMap<PathBuf, Staged>, reason: &str| {
                report
                    .unpublished
                    .extend(staged.into_values().map(|note| Unpublished {
                        entry: entry_name(&note.entry),
                        reason: reason.to_owned(),
                        conflict: false,
                    }));
            };
        let poison = self
            .shared
            .poison
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(reason) = poison {
            refuse_all(report, staged, &format!("staging failed: {reason}"));
            return;
        }
        // One barrier per device, after every note's content was flushed and
        // before the first rename. `st_dev` tells nested mounts apart.
        let mut anchors: BTreeMap<u64, &Path> = BTreeMap::new();
        for (path, note) in &staged {
            if let (Some(device), Some(parent)) = (note.device, path.parent()) {
                anchors.entry(device).or_insert(parent);
            }
        }
        let barrier_result = self.fault(FaultPoint::Barrier).and_then(|()| {
            self.probe(|| "barrier".to_owned());
            anchors.iter().try_for_each(|(device, anchor)| {
                self.probe(|| format!("barrier-device {device}"));
                barrier::barrier(anchor).map_err(anyhow::Error::from)
            })
        });
        if let Err(error) = barrier_result {
            refuse_all(
                report,
                staged,
                &format!("flushing staged notes failed: {error:#}"),
            );
            return;
        }
        // Each note is verified and renamed independently, so the renames run
        // on the pool. The injected fault is decided here, in path order, so a
        // test lands it on a chosen note.
        let queue: Vec<_> = staged
            .into_iter()
            .map(|(path, note)| {
                let injected = self.fault(FaultPoint::Persist);
                (path, note, injected)
            })
            .collect();
        let this = &*self;
        let rename = |(path, note, injected): (PathBuf, Staged, Result<()>)| {
            let outcome = injected
                .and_then(|()| {
                    this.probe(|| format!("rename {}", entry_name(&note.entry)));
                    note.source.verify()
                })
                .and_then(|()| {
                    note.temp
                        .persist(&path)
                        .map_err(|error| anyhow::Error::from(error.error))
                });
            (path, note.entry, outcome)
        };
        #[cfg(not(miri))]
        let outcomes: Vec<_> = {
            use rayon::prelude::*;
            queue.into_par_iter().map(rename).collect()
        };
        #[cfg(miri)]
        let outcomes: Vec<_> = queue.into_iter().map(rename).collect();
        for (path, entry, outcome) in outcomes {
            match outcome {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        self.directories.insert(parent.to_path_buf());
                    }
                }
                Err(error) => report.unpublished.push(Unpublished {
                    entry: entry_name(&entry),
                    conflict: error.downcast_ref::<SourceConflict>().is_some(),
                    reason: format!("{error:#}"),
                }),
            }
        }
    }
}

/// The vault-relative spelling effects and reports use: `/` on every platform.
fn entry_name(entry: &RelativeName) -> String {
    entry.as_path().to_string_lossy().replace('\\', "/")
}

/// The key a vault-relative name has in [`FinishReport::unpublished_by_entry`]:
/// the one derivation, whatever separators or `./` the caller spelled it with.
#[must_use]
pub fn entry_key(rel: &str) -> String {
    RelativeName::new(rel).map_or_else(|_| rel.replace('\\', "/"), |name| entry_name(&name))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?
        .sync_all()
        .context("syncing parent directory")
}
/// Windows has no supported per-directory fsync equivalent here. Replacement
/// files are flushed before publication except under BulkRewrite policy, but
/// directory-entry crash durability is explicitly unavailable; atomic/exclusive
/// publication remains supported.
#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<()> {
    Ok(())
}

/// Whether this backend provides an explicit directory-entry durability fence.
#[must_use]
pub const fn directory_sync_supported() -> bool {
    cfg!(unix)
}
#[cfg(not(any(unix, windows)))]
fn sync_directory(_path: &Path) -> Result<()> {
    bail!("unsupported: directory durability on this platform")
}

#[cfg(test)]
mod tests;
