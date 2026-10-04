//! Single-pass classification of vault entries against the VCS ignore sources
//! a normal vault walk honours (iteration 309, DEC-349).
//!
//! `summary` reports how many `.md` files `.gitignore` (and `.ignore`,
//! `.git/info/exclude`, git's global excludes) hide from the vault. The
//! `ignore` crate's walker drops those entries before any callback sees them,
//! so iteration 306 counted them with a second, rules-disabled walk — 0.15 s
//! of a 1.55 s MDN summary, paid even by a vault with no ignore file at all.
//!
//! This module lets one rules-disabled walk decide, per entry, what the
//! rules-enabled walk would have done. It is a port of the `ignore` crate's
//! own precedence (`ignore::dir::Ignore::matched_ignore`, private to that
//! crate) restricted to the sources that are known **before** the walk
//! starts:
//!
//! - the vault root's own `.ignore`, `.gitignore` and `.git/info/exclude`;
//! - every ancestor directory's `.ignore`, `.gitignore` and
//!   `.git/info/exclude` (the crate's "parents"), matched against the
//!   canonical absolute path exactly as the crate rebases it;
//! - git's global excludes file, rooted at the current directory.
//!
//! An ignore source *inside* the vault below its root (a nested `.gitignore`,
//! `.ignore`, `.git` or `.jj`) only becomes known while the walk is already
//! classifying its siblings, so the walk reports it and the caller falls back
//! to the crate's own rules-enabled walk for the file set. A `.jj` directory
//! or a `.git` *file* (a worktree) at the root or above is refused up front
//! for the same reason: the crate resolves those through code this port does
//! not reproduce. The file set a fast-path walk returns is therefore always
//! the one the crate would return; `discovery`'s differential tests pin that.
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// What the ignore sources say about one entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// No rule matched.
    None,
    /// An ignore rule matched: the rules-enabled walk drops the entry (and,
    /// for a directory, everything below it).
    Ignore,
    /// A `!negation` matched last: the entry is kept even when hidden.
    Whitelist,
}

impl<T> From<Match<T>> for Verdict {
    fn from(m: Match<T>) -> Self {
        match m {
            Match::None => Self::None,
            Match::Ignore(_) => Self::Ignore,
            Match::Whitelist(_) => Self::Whitelist,
        }
    }
}

/// The three per-directory ignore sources the crate reads.
struct Level {
    /// `.ignore` in this directory.
    ignore: Gitignore,
    /// `.gitignore` in this directory.
    git_ignore: Gitignore,
    /// `.git/info/exclude`, when this directory holds a `.git` directory.
    git_exclude: Gitignore,
    /// Whether this directory holds a `.git` (the crate's `has_git`).
    has_git: bool,
}

impl Level {
    fn has_rules(&self) -> bool {
        !self.ignore.is_empty() || !self.git_ignore.is_empty() || !self.git_exclude.is_empty()
    }
}

/// The VCS ignore sources that apply to a vault walk rooted at `root_dir`,
/// as far as they are knowable before the walk starts.
pub(crate) struct PreknownIgnores {
    /// The walk root exactly as given to the walker (the crate's
    /// `IgnoreInner::dir` for the root level).
    root_dir: PathBuf,
    root: Level,
    /// `root_dir` canonicalized, the base the crate rebases paths onto before
    /// matching an ancestor's rules. `None` when canonicalization failed, in
    /// which case the crate applies no ancestor rules either.
    abs_root: Option<PathBuf>,
    /// Ancestor directories of `abs_root`, nearest first.
    ancestors: Vec<Level>,
    global: Gitignore,
    /// The crate's `any_git`: git rules apply only inside a repository.
    any_git: bool,
    /// Whether any source can match anything at all.
    has_rules: bool,
}

/// Why a fast-path classification is not possible for this vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// A `.jj` directory or a `.git` file (a worktree) at the root or above.
    VcsLayout,
}

impl PreknownIgnores {
    /// Read the root's and the ancestors' ignore sources plus the global
    /// excludes file, mirroring how the crate builds them.
    pub(crate) fn load(root_dir: &Path) -> Result<Self, Unsupported> {
        let root = load_level(root_dir)?;
        let (abs_root, ancestors) = match std::fs::canonicalize(root_dir) {
            Ok(abs) => {
                let mut levels = Vec::new();
                let mut cursor = abs.as_path();
                while let Some(parent) = cursor.parent() {
                    levels.push(load_level(parent)?);
                    cursor = parent;
                }
                (Some(abs), levels)
            }
            // The crate drops the parents silently in this case too.
            Err(_) => (None, Vec::new()),
        };
        // `WalkBuilder` roots the global excludes at the process CWD.
        let global = match std::env::current_dir() {
            Ok(cwd) => GitignoreBuilder::new(cwd).build_global().0,
            Err(_) => Gitignore::empty(),
        };
        let any_git = root.has_git || ancestors.iter().any(|l| l.has_git);
        let has_rules = !root.ignore.is_empty()
            || ancestors.iter().any(|l| !l.ignore.is_empty())
            || (any_git
                && (root.has_rules()
                    || ancestors.iter().any(Level::has_rules)
                    || !global.is_empty()));
        Ok(Self {
            root_dir: root_dir.to_path_buf(),
            root,
            abs_root,
            ancestors,
            global,
            any_git,
            has_rules,
        })
    }

    /// Classify one walk entry, given the path the walker reports for it.
    ///
    /// Valid only while no ignore source exists between the root and the
    /// entry (the walk enforces that and falls back otherwise), so the only
    /// in-tree level with rules is the root itself.
    pub(crate) fn classify(&self, path: &Path, is_dir: bool) -> Verdict {
        if !self.has_rules {
            return Verdict::None;
        }
        let path = strip_prefix("./", path).unwrap_or(path);
        let mut m_ignore = self.root.ignore.matched(path, is_dir);
        let mut m_gi = Match::None;
        let mut m_exclude = Match::None;
        if self.any_git {
            m_gi = self.root.git_ignore.matched(path, is_dir);
            m_exclude = self.root.git_exclude.matched(path, is_dir);
        }
        let mut saw_git = self.root.has_git;
        if let Some(abs_root) = &self.abs_root {
            let abs_path = abs_root.join(self.relative_to_root(path));
            for level in &self.ancestors {
                if m_ignore.is_none() {
                    m_ignore = level.ignore.matched(&abs_path, is_dir);
                }
                if self.any_git && !saw_git && m_gi.is_none() {
                    m_gi = level.git_ignore.matched(&abs_path, is_dir);
                }
                if self.any_git && !saw_git && m_exclude.is_none() {
                    m_exclude = level.git_exclude.matched(&abs_path, is_dir);
                }
                saw_git = saw_git || level.has_git;
            }
        }
        let m_global = if self.any_git {
            self.global.matched(path, is_dir)
        } else {
            Match::None
        };
        m_ignore.or(m_gi).or(m_exclude).or(m_global).into()
    }

    /// The crate's rebasing of a walk path onto the root before it is joined
    /// to the canonical root (`ignore::dir::Ignore::matched_ignore`).
    fn relative_to_root<'a>(&'a self, path: &'a Path) -> &'a Path {
        if self.root_dir.as_path() == Path::new(".") {
            return path;
        }
        let without_dot_slash = strip_if_is_prefix("./", &self.root_dir);
        let relative_base = strip_if_is_prefix(without_dot_slash, path);
        strip_if_is_prefix("/", relative_base)
    }
}

/// Whether a file name is one the crate reads ignore rules or a repository
/// boundary from. Seeing one below the root means the fast path is invalid.
pub(crate) fn is_ignore_source_name(name: &std::ffi::OsStr) -> bool {
    name == ".gitignore" || name == ".ignore" || name == ".git" || name == ".jj"
}

/// The crate's hidden test (`ignore::pathutil::is_hidden_entry`): a leading
/// dot, or on Windows the hidden file attribute (free — the walker caches the
/// metadata the directory listing returned).
pub(crate) fn is_hidden_entry(entry: &ignore::DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if entry
            .metadata()
            .is_ok_and(|md| md.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0)
        {
            return true;
        }
    }
    entry
        .path()
        .file_name()
        .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."))
}

fn load_level(dir: &Path) -> Result<Level, Unsupported> {
    if dir.join(".jj").exists() {
        return Err(Unsupported::VcsLayout);
    }
    let git_type = dir.join(".git").metadata().ok().map(|md| md.file_type());
    if git_type.is_some_and(|t| !t.is_dir()) {
        return Err(Unsupported::VcsLayout);
    }
    let has_git = git_type.is_some();
    let git_exclude = if has_git {
        build(dir, &dir.join(".git").join("info").join("exclude"))
    } else {
        Gitignore::empty()
    };
    Ok(Level {
        ignore: build(dir, &dir.join(".ignore")),
        git_ignore: build(dir, &dir.join(".gitignore")),
        git_exclude,
        has_git,
    })
}

/// The crate's `create_gitignore`: rules rooted at `root`, read from `file`
/// when it exists; unreadable files and bad globs contribute nothing.
fn build(root: &Path, file: &Path) -> Gitignore {
    if !file.exists() {
        return Gitignore::empty();
    }
    let mut builder = GitignoreBuilder::new(root);
    // A partially invalid file still contributes its valid lines, exactly as
    // the crate's builder keeps them.
    let _ = builder.add(file);
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// The crate's `pathutil::strip_prefix`: byte-wise on Unix, component-wise
/// elsewhere.
fn strip_prefix<'a>(prefix: &'a (impl AsRef<Path> + ?Sized), path: &'a Path) -> Option<&'a Path> {
    #[cfg(unix)]
    {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let prefix = prefix.as_ref().as_os_str().as_bytes();
        let bytes = path.as_os_str().as_bytes();
        if prefix.len() > bytes.len() || prefix != &bytes[..prefix.len()] {
            None
        } else {
            Some(Path::new(OsStr::from_bytes(&bytes[prefix.len()..])))
        }
    }
    #[cfg(not(unix))]
    {
        path.strip_prefix(prefix.as_ref()).ok()
    }
}

fn strip_if_is_prefix<'a>(prefix: &'a (impl AsRef<Path> + ?Sized), path: &'a Path) -> &'a Path {
    strip_prefix(prefix, path).unwrap_or(path)
}
