//! One normalization boundary for named application inputs. A normalized name
//! is never passed back through the CLI prefix/absolute-path interpreter.
use anyhow::Result;
use hyalo_core::index::{SnapshotIndex, VaultIndex as _};
use hyalo_core::rooted::{RelativeName, VaultRoot};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct NormalizedTarget {
    name: RelativeName,
    relative: String,
}
impl NormalizedTarget {
    fn resolved(relative: &str) -> Result<Self> {
        let name = RelativeName::new(relative)
            .map_err(|error| crate::error::user_error(error.to_string()))?;
        let relative = name.as_path().to_string_lossy().replace('\\', "/");
        Ok(Self { name, relative })
    }
    pub(crate) fn relative(&self) -> &str {
        &self.relative
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedSelection {
    targets: Vec<NormalizedTarget>,
}
impl PreparedSelection {
    /// Interpret explicit CLI paths exactly once, independent of membership.
    pub(crate) fn explicit(dir: &Path, raw: &[String]) -> Result<Self> {
        let mut names = Vec::with_capacity(raw.len());
        for raw in raw {
            let relative = if Path::new(raw).is_absolute() {
                let name = hyalo_core::discovery::strip_absolute_vault_prefix(dir, raw)
                    .ok_or_else(|| {
                        anyhow::Error::new(crate::output::user_diagnostic(
                            crate::output::Format::Json,
                            &hyalo_core::outside_vault_message_with_dir("file", None, dir),
                            Some(raw),
                            Some(&hyalo_core::outside_vault_hint(dir)),
                            None,
                        ))
                    })?;
                crate::warn::warn_llm_misuse(dir);
                name
            } else {
                let name = hyalo_core::discovery::normalize_path(raw);
                hyalo_core::discovery::strip_dir_prefix(dir, &name).unwrap_or(name)
            };
            names.push(relative);
        }
        Self::resolved(&names)
    }
    /// Files-from and single-target resolvers already chose identity using their
    /// own membership/cardinality policy. Validate it without prefix stripping.
    pub(crate) fn resolved(names: &[String]) -> Result<Self> {
        Ok(Self {
            targets: names
                .iter()
                .map(|name| NormalizedTarget::resolved(name))
                .collect::<Result<_>>()?,
        })
    }
    pub(crate) fn names(&self) -> Vec<String> {
        self.targets
            .iter()
            .map(|target| target.relative.clone())
            .collect()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
    /// Authorize the entire selected set before any scanner callback. Missing
    /// files are distinct from refused/unreadable paths and never authorize I/O.
    pub(crate) fn precheck(&self, root: &VaultRoot) -> Result<Vec<CheckedTarget>> {
        let mut checked = Vec::new();
        for target in &self.targets {
            match root.open(&target.name) {
                Ok(_) => {
                    // Apply existing extension/exclusion/path policy to the exact
                    // identity. This resolver never strips a prefix or reads bytes.
                    if let Err(error) = hyalo_core::discovery::resolve_normalized_file_ci(
                        root.path(),
                        target.relative(),
                        false,
                    ) {
                        let crate::output::CommandOutcome::UserError(diagnostic) =
                            crate::commands::resolve_error_to_outcome(
                                error,
                                crate::output::Format::Json,
                                root.path(),
                            )
                        else {
                            unreachable!("resolver failures are diagnostics")
                        };
                        return Err(anyhow::Error::new(diagnostic));
                    }
                    checked.push(CheckedTarget {
                        full: root.path().join(target.name.as_path()),
                        target: target.clone(),
                        exists: true,
                    });
                }
                Err(error)
                    if error.chain().any(|cause| {
                        cause
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    }) =>
                {
                    checked.push(CheckedTarget {
                        full: root.path().join(target.name.as_path()),
                        target: target.clone(),
                        exists: false,
                    });
                }
                Err(error) => {
                    // Preserve the existing directory/extension hints without
                    // interpreting a normalized identity as raw CLI input again.
                    if let Err(
                        reason @ (hyalo_core::discovery::FileResolveError::IsDirectory { .. }
                        | hyalo_core::discovery::FileResolveError::MissingExtension {
                            ..
                        }),
                    ) = hyalo_core::discovery::resolve_normalized_file_ci(
                        root.path(),
                        target.relative(),
                        false,
                    ) {
                        let crate::output::CommandOutcome::UserError(diagnostic) =
                            crate::commands::resolve_error_to_outcome(
                                reason,
                                crate::output::Format::Json,
                                root.path(),
                            )
                        else {
                            unreachable!("resolver failures are diagnostics")
                        };
                        return Err(anyhow::Error::new(diagnostic));
                    }
                    return Err(crate::error::user_error(error.to_string()));
                }
            }
        }
        Ok(checked)
    }
}

pub(crate) struct CheckedTarget {
    full: PathBuf,
    target: NormalizedTarget,
    exists: bool,
}
#[derive(Default, Debug)]
pub(crate) struct RefreshSummary {
    pub(crate) refreshed: usize,
    pub(crate) missing: usize,
    pub(crate) failed: usize,
}
impl RefreshSummary {
    pub(crate) fn all_current(&self, selection: &PreparedSelection) -> bool {
        !selection.is_empty() && self.failed == 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RefreshOperation {
    Existing,
    Insert,
}

/// One bridge for existing-entry refresh AND named absent-entry insertion.
/// The exact checked path and relative key are used together. The legacy index
/// scanner still opens paths, so concurrent directory swaps remain out of scope.
pub(crate) fn refresh_named_selection(
    root: &VaultRoot,
    selection: &PreparedSelection,
    index: &mut SnapshotIndex,
    insert_missing: bool,
) -> Result<RefreshSummary> {
    // Whole-set preflight remains ahead of the first scan or state mutation.
    selection.precheck(root)?;
    index.begin_changes();
    let result = refresh_named_with(root, selection, index, insert_missing, scan_checked_target);
    index.finish_changes();
    result
}
fn refresh_named_with(
    root: &VaultRoot,
    selection: &PreparedSelection,
    index: &mut SnapshotIndex,
    insert_missing: bool,
    mut scan: impl FnMut(&mut SnapshotIndex, &CheckedTarget, RefreshOperation) -> bool,
) -> Result<RefreshSummary> {
    let targets = selection.precheck(root)?;
    let mut summary = RefreshSummary::default();
    for target in targets {
        if !target.exists {
            summary.missing += 1;
            continue;
        }
        let operation = if index.get(target.target.relative()).is_some() {
            RefreshOperation::Existing
        } else if insert_missing {
            RefreshOperation::Insert
        } else {
            summary.failed += 1;
            continue;
        };
        // Recheck at the actual bridge as well as whole-set preflight.
        root.open(&target.target.name)
            .map_err(|error| crate::error::user_error(error.to_string()))?;
        if scan(index, &target, operation) {
            summary.refreshed += 1;
        } else {
            summary.failed += 1;
        }
    }
    Ok(summary)
}
fn scan_checked_target(
    index: &mut SnapshotIndex,
    target: &CheckedTarget,
    operation: RefreshOperation,
) -> bool {
    let rel = target.target.relative();
    match operation {
        RefreshOperation::Existing => {
            hyalo_core::index::refresh_if_changed_at(index, &target.full, rel)
        }
        RefreshOperation::Insert => {
            if let Err(error) = index.insert_or_replace_entry_with_links(&target.full, rel) {
                crate::warn::warn(format!(
                    "{rel}: absent from the snapshot index and could not be read from disk: {error}"
                ));
                return false;
            }
            crate::warn::note(format!(
                "{rel}: absent from the snapshot index — read from disk for this run (run `hyalo create-index` to fold it in)"
            ));
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(dir: &Path) -> SnapshotIndex {
        let pairs: Vec<_> = ["first.md", "second.md"]
            .iter()
            .map(|name| {
                let path = dir.join(name);
                std::fs::write(&path, "---\nstatus: old\n---\n").unwrap();
                (path, (*name).to_owned())
            })
            .collect();
        let build = hyalo_core::index::ScannedIndex::build(
            &pairs,
            None,
            &hyalo_core::index::ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let path = dir.join(".hyalo-index");
        SnapshotIndex::save(&build.index, &path, dir.to_str().unwrap(), None, None).unwrap();
        SnapshotIndex::load(&path).unwrap().unwrap()
    }
    #[test]
    fn actual_refresh_and_insert_share_identity_and_do_not_short_circuit() {
        let dir = tempfile::tempdir().unwrap();
        let mut index = snapshot(dir.path());
        for name in ["first.md", "second.md", "third.md"] {
            std::fs::write(dir.path().join(name), "---\nstatus: changed-longer\n---\n").unwrap();
        }
        let selection = PreparedSelection::resolved(&[
            "first.md".into(),
            "second.md".into(),
            "third.md".into(),
        ])
        .unwrap();
        let mut calls = Vec::new();
        let summary = refresh_named_with(
            &VaultRoot::new(dir.path()).unwrap(),
            &selection,
            &mut index,
            true,
            |index, target, operation| {
                calls.push((target.target.relative().to_owned(), operation));
                let result = scan_checked_target(index, target, operation);
                // A false result must not prevent later refresh OR insertion.
                result && target.target.relative() != "first.md"
            },
        )
        .unwrap();
        assert_eq!(
            calls,
            [
                ("first.md".into(), RefreshOperation::Existing),
                ("second.md".into(), RefreshOperation::Existing),
                ("third.md".into(), RefreshOperation::Insert)
            ]
        );
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.refreshed, 2);
        for name in selection.names() {
            assert_eq!(
                index.get(&name).unwrap().properties["status"],
                "changed-longer"
            );
        }
    }
    #[cfg(unix)]
    #[test]
    fn existing_and_absent_escapes_have_zero_actual_refresh_or_insert_calls() {
        for indexed in [false, true] {
            for parent in [false, true] {
                let project = tempfile::tempdir().unwrap();
                let dir = project.path().join("kb");
                std::fs::create_dir(&dir).unwrap();
                let outside = tempfile::tempdir().unwrap();
                let mut index = snapshot(&dir);
                std::fs::write(dir.join("first.md"), "changed safe contents").unwrap();
                let rel = if indexed { "second.md" } else { "escape.md" };
                let raw = if parent {
                    std::fs::create_dir(dir.join("parent")).unwrap();
                    std::fs::write(dir.join("parent").join(rel), "old").unwrap();
                    if indexed {
                        index
                            .insert_or_replace_entry_with_links(
                                &dir.join("parent").join(rel),
                                &format!("parent/{rel}"),
                            )
                            .unwrap();
                    }
                    std::fs::remove_file(dir.join("parent").join(rel)).unwrap();
                    std::fs::remove_dir(dir.join("parent")).unwrap();
                    std::fs::write(outside.path().join(rel), "external marker").unwrap();
                    std::os::unix::fs::symlink(outside.path(), dir.join("parent")).unwrap();
                    format!("kb/parent/{rel}")
                } else {
                    if indexed {
                        std::fs::remove_file(dir.join(rel)).unwrap();
                    }
                    std::fs::write(outside.path().join(rel), "external marker").unwrap();
                    std::os::unix::fs::symlink(outside.path().join(rel), dir.join(rel)).unwrap();
                    format!("kb/{rel}")
                };
                let safe = if indexed { "safe-new.md" } else { "first.md" };
                if indexed {
                    std::fs::write(dir.join(safe), "new safe contents").unwrap();
                }
                let selection = PreparedSelection::explicit(&dir, &[safe.into(), raw]).unwrap();
                let before = index.get("first.md").unwrap().properties.clone();
                let persisted = std::fs::read(dir.join(".hyalo-index")).unwrap();
                let mut calls = 0;
                let result = refresh_named_with(
                    &VaultRoot::new(&dir).unwrap(),
                    &selection,
                    &mut index,
                    true,
                    |index, target, operation| {
                        calls += 1;
                        scan_checked_target(index, target, operation)
                    },
                );
                assert!(result.is_err());
                assert_eq!(calls, 0, "indexed={indexed}, parent={parent}");
                assert!(index.get("safe-new.md").is_none());
                assert_eq!(index.get("first.md").unwrap().properties, before);
                assert_eq!(std::fs::read(dir.join(".hyalo-index")).unwrap(), persisted);
            }
        }
    }
}
