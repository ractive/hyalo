//! Checked filesystem operations. Checks close static escapes; they do not
//! promise protection against directory swaps between checks and system calls.
//! Entry identity (a name, possibly a symlink) is distinct from file identity.
//! Atomic publication and fallible durability finalization are separate effects.
#![allow(clippy::missing_errors_doc)]

use std::collections::{BTreeSet, hash_map::DefaultHasher};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use tempfile::{NamedTempFile, TempPath};

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
        let resolved_parent = dunce::canonicalize(parent)?;
        if !resolved_parent.starts_with(&self.0) {
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
    original: TempPath,
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
        let (mut file, exact_identity, identity) = file_identity(opened.file)?;
        let metadata = file.metadata()?;
        let oversized = || {
            crate::user_error(format!(
                "file too large: {} exceeds {} MiB limit",
                opened.entry.as_path().display(),
                crate::scanner::MAX_FILE_SIZE / (1024 * 1024)
            ))
        };
        if metadata.len() > crate::scanner::MAX_FILE_SIZE {
            return Err(oversized());
        }
        let mut original = NamedTempFile::new()?;
        let size = std::io::copy(
            &mut Read::by_ref(&mut file).take(crate::scanner::MAX_FILE_SIZE + 1),
            &mut original,
        )?;
        if size > crate::scanner::MAX_FILE_SIZE {
            return Err(oversized());
        }
        Ok(Self {
            root,
            entry: opened.entry,
            referent: opened.referent,
            exact_identity,
            identity,
            original: original.into_temp_path(),
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
        Ok(File::open(&self.original)?)
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        Ok(std::fs::read(&self.original)?)
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
        let mut original = self.reader()?;
        let mut left = vec![0u8; 64 * 1024].into_boxed_slice();
        let mut right = vec![0u8; 64 * 1024].into_boxed_slice();
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
        let mut temp = NamedTempFile::new_in(parent)?;
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
    expected: TempPath,
}

impl OwnedPublication {
    #[must_use]
    pub fn name(&self) -> &RelativeName {
        &self.entry
    }

    pub fn capture_verified(&self) -> Result<CapturedInput> {
        let path = self.root.check(&self.entry, false)?;
        if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!(SourceConflict(
                "published entry kind changed before compensation"
            ));
        }
        let captured = CapturedInput::capture(self.root.clone(), self.root.open(&self.entry)?)?;
        let current_handle = same_file::Handle::from_path(&path)?;
        if current_handle != self.handle || captured.bytes()? != std::fs::read(&self.expected)? {
            bail!(SourceConflict(
                "published entry identity or bytes changed before compensation"
            ));
        }
        Ok(captured)
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
        self.source.verify()?;
        let path = self.source.root.check(&self.source.entry, true)?;
        let parent = path.parent().context("target has no parent")?;
        let mut temp = NamedTempFile::new_in(parent)?;
        let mut staged = File::open(&self.data)?;
        std::io::copy(&mut staged, &mut temp)?;
        temp.as_file()
            .set_permissions(self.source.permissions.clone())?;
        if session.durability != Durability::BulkRewrite {
            session.fault(FaultPoint::SyncFile)?;
            temp.as_file().sync_all()?;
        }
        let handle = same_file::Handle::from_file(temp.as_file().try_clone()?)?;
        let receipt = OwnedPublication {
            root: self.source.root.clone(),
            entry: self.source.entry.clone(),
            handle,
            expected: self.data,
        };
        session.fault(FaultPoint::Persist)?;
        self.source.verify()?;
        temp.persist(&path).map_err(|e| e.error)?;
        Ok((session.record(&path, Operation::Replaced), receipt))
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
    /// Atomic replacements without a per-file content flush; directories are
    /// finalized once by the invocation owner. Bulk rewrites can lose published
    /// content on power loss or kernel panic (DEC-317). New entries still flush.
    BulkRewrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultPoint {
    Prepare,
    SyncFile,
    Persist,
    Remove,
    Finalize,
}

/// Explicit invocation-owned durability. Drop does not report success.
pub struct WriteSession {
    durability: Durability,
    directories: BTreeSet<PathBuf>,
    fault: Option<FaultPoint>,
}

impl WriteSession {
    #[must_use]
    pub fn new(durability: Durability) -> Self {
        Self {
            durability,
            directories: BTreeSet::new(),
            fault: None,
        }
    }
    /// Deterministic fault seam; scoped to this session, never environment/global state.
    #[must_use]
    pub fn with_fault(durability: Durability, fault: FaultPoint) -> Self {
        Self {
            durability,
            directories: BTreeSet::new(),
            fault: Some(fault),
        }
    }
    /// A parallel worker inherits policy, but owns its pending directories.
    pub(crate) fn worker(&self) -> Self {
        Self {
            durability: self.durability,
            directories: BTreeSet::new(),
            fault: self.fault,
        }
    }
    /// Transfer a worker's pending finalization to the invocation owner.
    pub(crate) fn absorb(&mut self, mut worker: Self) {
        self.directories.append(&mut worker.directories);
    }
    fn fault(&self, point: FaultPoint) -> Result<()> {
        if self.fault == Some(point) {
            bail!("injected {point:?} failure");
        }
        Ok(())
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
    pub fn finish(self) -> Result<()> {
        self.fault(FaultPoint::Finalize)?;
        for parent in &self.directories {
            sync_directory(parent)?;
        }
        Ok(())
    }
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
