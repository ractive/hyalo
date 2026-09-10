//! Complete preparation precedes serial publication. No rollback is promised.
//! The report survives I/O, durability, index and later output failures.
use anyhow::{Context, Result, bail};
use hyalo_core::rooted::{
    CapturedInput, Durability, PreparedReplacement, RelativeName, VaultRoot, WriteSession,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EffectState {
    Unchanged,
    Committed,
    NotAttempted,
    FailedBeforeCommit,
    CommittedWithFinalizationError,
    Restored,
    RestoreFailed,
    Kept,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum EffectFailure {
    SourceConflict,
    Io,
    Finalization,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[cfg_attr(test, ts(optional_fields))]
pub struct PathEffect {
    pub file: String,
    pub state: EffectState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<EffectFailure>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[serde(rename_all = "snake_case")]
pub enum IndexDisposition {
    NotUsed,
    Updated,
    Invalidated,
    UpdateFailed,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export))]
#[cfg_attr(test, ts(optional_fields))]
pub struct ApplyReport {
    pub paths: Vec<PathEffect>,
    pub index: IndexDisposition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_error: Option<String>,
}

impl ApplyReport {
    pub(crate) fn failed(&self) -> bool {
        self.paths.iter().any(|p| {
            matches!(
                p.state,
                EffectState::FailedBeforeCommit
                    | EffectState::CommittedWithFinalizationError
                    | EffectState::RestoreFailed
            )
        }) || self.index_error.is_some()
    }
    pub(crate) fn committed_paths(&self) -> Vec<String> {
        self.paths
            .iter()
            .filter(|p| {
                matches!(
                    p.state,
                    EffectState::Committed
                        | EffectState::CommittedWithFinalizationError
                        | EffectState::Restored
                        | EffectState::Kept
                )
            })
            .map(|p| p.file.clone())
            .collect()
    }
}

/// CLI presentation only: this reporter owns no filesystem policy or session.
struct ProgressReporter<'a> {
    label: &'a str,
    total: usize,
    done: usize,
    quiet: bool,
}
impl ProgressReporter<'_> {
    fn record(&mut self, effect: &PathEffect) -> Option<String> {
        if !matches!(
            effect.state,
            EffectState::Committed | EffectState::CommittedWithFinalizationError
        ) {
            return None;
        }
        self.done += 1;
        if !self.quiet && self.total >= 200 && self.done > 0 && self.done.is_multiple_of(200) {
            Some(format!(
                "{}: {}/{} files",
                self.label, self.done, self.total
            ))
        } else {
            None
        }
    }
    fn finish(&self) -> Option<String> {
        (!self.quiet && self.total >= 200 && self.done > 0)
            .then(|| format!("{}: {}/{} files, done", self.label, self.done, self.total))
    }
}
fn emit_progress(message: Option<String>) {
    if let Some(message) = message {
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "{message}");
    }
}

/// Staging closes files between entries and limits cumulative source+output
/// bytes to 8 GiB. Hitting this budget refuses the whole batch before writes.
pub(crate) struct PreparedChangeSet {
    root: VaultRoot,
    changes: Vec<(String, PreparedChange)>,
    identities: HashSet<u64>,
    staged_bytes: u64,
    output_bytes: usize,
    session: WriteSession,
}

enum PreparedChange {
    Replace(PreparedReplacement),
    Unchanged(CapturedInput),
}

impl PreparedChangeSet {
    pub(crate) fn new(dir: &Path, count: usize) -> Result<Self> {
        Ok(Self {
            root: VaultRoot::new(dir)?,
            changes: Vec::new(),
            identities: HashSet::new(),
            staged_bytes: 0,
            output_bytes: 0,
            session: WriteSession::new(if count > 8 {
                Durability::PerDirectory
            } else {
                Durability::PerFile
            }),
        })
    }
    pub(crate) fn capture(&mut self, rel: &str) -> Result<CapturedInput> {
        let captured = self.root.capture(&RelativeName::new(rel)?)?;
        if !self.identities.insert(captured.physical_identity()) {
            bail!(hyalo_core::UserFacingError { message: format!("duplicate physical mutation target: {rel}"),
                hint: Some("select each file once; aliases and hard links cannot be combined in one mutation".into()), cause: None });
        }
        self.staged_bytes = self
            .staged_bytes
            .checked_add(captured.size())
            .context("staging size overflow")?;
        self.check_budget()?;
        Ok(captured)
    }
    fn check_budget(&self) -> Result<()> {
        if self.staged_bytes > 8 * 1024 * 1024 * 1024 {
            bail!("mutation preparation exceeds 8 GiB staging budget");
        }
        Ok(())
    }
    pub(crate) fn push(&mut self, captured: CapturedInput, bytes: Option<&[u8]>) -> Result<()> {
        let name = captured
            .name()
            .as_path()
            .to_string_lossy()
            .replace('\\', "/");
        let change = if let Some(bytes) = bytes {
            self.staged_bytes = self
                .staged_bytes
                .checked_add(bytes.len() as u64)
                .context("staging size overflow")?;
            self.check_budget()?;
            PreparedChange::Replace(captured.prepare(bytes, &self.session)?)
        } else {
            PreparedChange::Unchanged(captured)
        };
        self.changes.push((name, change));
        Ok(())
    }
    pub(crate) fn charge_output(&mut self, value: &serde_json::Value) -> Result<()> {
        self.output_bytes = Self::measure_output(value, self.output_bytes.saturating_add(1))?;
        Ok(())
    }
    #[allow(clippy::unused_self)] // validate output within the live preparation phase
    pub(crate) fn check_output(&self, value: &serde_json::Value) -> Result<()> {
        Self::measure_output(value, 0).map(|_| ())
    }
    fn measure_output(value: &serde_json::Value, initial: usize) -> Result<usize> {
        struct BudgetWriter(usize);
        impl std::io::Write for BudgetWriter {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self.0.saturating_add(bytes.len());
                if self.0 > 64 * 1024 * 1024 {
                    return Err(std::io::Error::other(
                        "mutation output exceeds 64 MiB budget",
                    ));
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = BudgetWriter(initial);
        serde_json::to_writer(&mut writer, value)
            .map_err(|error| crate::error::user_error(error.to_string()))?;
        Ok(writer.0)
    }
    pub(crate) fn preview(self) -> ApplyReport {
        ApplyReport {
            paths: self
                .changes
                .into_iter()
                .map(|(file, _)| PathEffect {
                    file,
                    state: EffectState::Unchanged,
                    error: None,
                    category: None,
                })
                .collect(),
            index: IndexDisposition::NotUsed,
            index_error: None,
        }
    }
    pub(crate) fn apply(
        self,
        journal: &mut super::journal::MutationJournal<'_>,
        label: &str,
    ) -> ApplyReport {
        let mut progress = ProgressReporter {
            label,
            total: self.changes.len(),
            done: 0,
            quiet: hyalo_core::warn::quiet_progress(),
        };
        let report = self.apply_with(
            journal,
            |_, _| Ok(()),
            |effect| emit_progress(progress.record(effect)),
        );
        emit_progress(progress.finish());
        report
    }
    pub(crate) fn apply_with(
        mut self,
        journal: &mut super::journal::MutationJournal<'_>,
        mut before: impl FnMut(usize, &str) -> Result<()>,
        mut progress: impl FnMut(&PathEffect),
    ) -> ApplyReport {
        let mut paths = Vec::with_capacity(self.changes.len());
        let mut stopped = false;
        for (index, (file, change)) in self.changes.into_iter().enumerate() {
            if stopped {
                paths.push(PathEffect {
                    file,
                    state: EffectState::NotAttempted,
                    error: None,
                    category: None,
                });
                continue;
            }
            let result = before(index, &file).and_then(|()| match change {
                PreparedChange::Replace(change) => change.commit(&mut self.session).map(Some),
                PreparedChange::Unchanged(source) => source.verify().map(|()| None),
            });
            let (state, error, category) = match result {
                Ok(None) => (EffectState::Unchanged, None, None),
                Ok(Some(effect)) => match effect.finalization_error() {
                    Some(error) => (
                        EffectState::CommittedWithFinalizationError,
                        Some(error.to_owned()),
                        Some(EffectFailure::Finalization),
                    ),
                    None => (EffectState::Committed, None, None),
                },
                Err(error) => {
                    let category = if error
                        .downcast_ref::<hyalo_core::rooted::SourceConflict>()
                        .is_some()
                    {
                        EffectFailure::SourceConflict
                    } else {
                        EffectFailure::Io
                    };
                    (
                        EffectState::FailedBeforeCommit,
                        Some(error.to_string()),
                        Some(category),
                    )
                }
            };
            stopped = error.is_some();
            let effect = PathEffect {
                file,
                state,
                error,
                category,
            };
            progress(&effect);
            paths.push(effect);
        }
        if let Err(error) = self.session.finish() {
            for path in &mut paths {
                if path.state == EffectState::Committed {
                    path.state = EffectState::CommittedWithFinalizationError;
                    path.error = Some(error.to_string());
                    path.category = Some(EffectFailure::Finalization);
                }
            }
        }
        let mut report = ApplyReport {
            paths,
            index: IndexDisposition::NotUsed,
            index_error: None,
        };
        // No `?` after publication: every path reaches mandatory reconciliation.
        let committed = report.committed_paths();
        let observed: Vec<_> = report
            .paths
            .iter()
            .filter(|p| p.state == EffectState::Unchanged)
            .map(|p| p.file.clone())
            .chain(committed)
            .collect();
        let unsafe_paths: Vec<_> = report
            .paths
            .iter()
            .filter(|path| path.state == EffectState::FailedBeforeCommit)
            .map(|path| path.file.clone())
            .collect();
        let (index, error) = journal.finalize_observed(self.root.path(), &observed, &unsafe_paths);
        report.index = index;
        report.index_error = error;
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexed_late_same_size_restored_mtime_conflict_invalidates_snapshot() {
        indexed_failed_source_invalidates(false);
    }

    #[cfg(unix)]
    #[test]
    fn indexed_late_external_symlink_failure_invalidates_without_rescan() {
        indexed_failed_source_invalidates(true);
    }

    fn indexed_failed_source_invalidates(escape: bool) {
        use hyalo_core::index::{ScanOptions, ScannedIndex, SnapshotIndex};
        let dir = tempfile::tempdir().unwrap();
        let pairs: Vec<_> = ["a.md", "b.md"]
            .into_iter()
            .map(|name| {
                let path = dir.path().join(name);
                std::fs::write(&path, "---\nstatus: old\n---\n").unwrap();
                (path, name.to_owned())
            })
            .collect();
        let built = ScannedIndex::build(
            &pairs,
            None,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: None,
            },
        )
        .unwrap();
        let index_path = dir.path().join(".hyalo-index");
        SnapshotIndex::save(
            &built.index,
            &index_path,
            dir.path().to_str().unwrap(),
            None,
            None,
        )
        .unwrap();
        let mut index = SnapshotIndex::load(&index_path).unwrap();
        let mut set = PreparedChangeSet::new(dir.path(), 2).unwrap();
        for (_, name) in &pairs {
            let captured = set.capture(name).unwrap();
            set.push(captured, Some(b"---\nstatus: new\n---\n"))
                .unwrap();
        }
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("external.md");
        std::fs::write(&external, "external marker").unwrap();
        let source = dir.path().join("b.md");
        let original_time = std::fs::metadata(&source).unwrap().modified().unwrap();
        let mut journal =
            super::super::journal::MutationJournal::new(&mut index, Some(&index_path));
        let report = set.apply_with(
            &mut journal,
            |position, _| {
                if position == 1 {
                    if escape {
                        #[cfg(unix)]
                        {
                            std::fs::remove_file(&source)?;
                            std::os::unix::fs::symlink(&external, &source)?;
                        }
                        return Ok(());
                    }
                    std::fs::write(&source, "---\nstatus: edt\n---\n")?;
                    std::fs::File::options()
                        .write(true)
                        .open(&source)?
                        .set_modified(original_time)?;
                }
                Ok(())
            },
            |_| {},
        );
        assert_eq!(report.paths[0].state, EffectState::Committed);
        assert_eq!(
            report.paths[1].category,
            Some(if escape {
                EffectFailure::Io
            } else {
                EffectFailure::SourceConflict
            })
        );
        assert!(matches!(report.index, IndexDisposition::Invalidated));
        assert!(!index_path.exists());
        assert!(index.is_none());
        assert_eq!(
            std::fs::read_to_string(source).unwrap(),
            if escape {
                "external marker"
            } else {
                "---\nstatus: edt\n---\n"
            }
        );
        assert!(
            std::fs::read_to_string(dir.path().join("a.md"))
                .unwrap()
                .contains("status: new")
        );
    }

    #[test]
    fn no_op_version_conflict_does_not_silently_claim_the_requested_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        std::fs::write(&path, b"same").unwrap();
        let mut set = PreparedChangeSet::new(dir.path(), 1).unwrap();
        let captured = set.capture("a.md").unwrap();
        set.push(captured, None).unwrap();
        std::fs::write(&path, b"edit").unwrap();
        let mut index = None;
        let mut journal = super::super::journal::MutationJournal::new(&mut index, None);
        let report = set.apply(&mut journal, "test");
        assert_eq!(report.paths[0].state, EffectState::FailedBeforeCommit);
        assert_eq!(
            report.paths[0].category,
            Some(EffectFailure::SourceConflict)
        );
        assert_eq!(std::fs::read(path).unwrap(), b"edit");
    }

    #[test]
    fn progress_is_invocation_owned_and_quiet_is_captured() {
        let effect = PathEffect {
            file: "a".into(),
            state: EffectState::Committed,
            error: None,
            category: None,
        };
        let mut visible = ProgressReporter {
            label: "setting properties",
            total: 200,
            done: 0,
            quiet: false,
        };
        let mut quiet = ProgressReporter {
            label: "other",
            total: 200,
            done: 0,
            quiet: true,
        };
        for _ in 0..199 {
            assert!(visible.record(&effect).is_none());
        }
        assert_eq!(
            visible.record(&effect).as_deref(),
            Some("setting properties: 200/200 files")
        );
        assert_eq!(
            visible.finish().as_deref(),
            Some("setting properties: 200/200 files, done")
        );
        assert!(quiet.record(&effect).is_none());
        assert!(quiet.finish().is_none());
        assert_eq!(quiet.done, 1);
    }

    #[test]
    fn source_conflict_is_distinct_from_io_and_stops_later_commits() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = PreparedChangeSet::new(dir.path(), 2).unwrap();
        for name in ["a.md", "b.md"] {
            std::fs::write(dir.path().join(name), b"old").unwrap();
            let captured = set.capture(name).unwrap();
            set.push(captured, Some(b"new")).unwrap();
        }
        std::fs::write(dir.path().join("a.md"), b"edit").unwrap();
        let mut index = None;
        let mut journal = super::super::journal::MutationJournal::new(&mut index, None);
        let report = set.apply(&mut journal, "test");
        assert_eq!(
            report.paths[0].category,
            Some(EffectFailure::SourceConflict)
        );
        assert_eq!(report.paths[1].state, EffectState::NotAttempted);
        assert_eq!(std::fs::read(dir.path().join("b.md")).unwrap(), b"old");
    }

    #[test]
    fn second_file_failure_retains_first_and_marks_rest_unattempted() {
        let dir = tempfile::tempdir().unwrap();
        let mut set = PreparedChangeSet::new(dir.path(), 3).unwrap();
        for name in ["a.md", "b.md", "c.md"] {
            std::fs::write(dir.path().join(name), b"old").unwrap();
            let captured = set.capture(name).unwrap();
            set.push(captured, Some(b"new")).unwrap();
        }
        let mut index = None;
        let mut journal = super::super::journal::MutationJournal::new(&mut index, None);
        let report = set.apply_with(
            &mut journal,
            |index, _| {
                if index == 1 {
                    bail!("second-file I/O failure");
                }
                Ok(())
            },
            |_| {},
        );
        assert_eq!(
            report.paths.iter().map(|p| &p.state).collect::<Vec<_>>(),
            [
                &EffectState::Committed,
                &EffectState::FailedBeforeCommit,
                &EffectState::NotAttempted
            ]
        );
        assert_eq!(report.paths[1].category, Some(EffectFailure::Io));
        assert_eq!(std::fs::read(dir.path().join("a.md")).unwrap(), b"new");
        assert_eq!(std::fs::read(dir.path().join("b.md")).unwrap(), b"old");
    }
}
