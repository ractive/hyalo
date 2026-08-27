//! Unified index-maintenance journal for every mutating command (ARCH-3, iter-226).
//!
//! Before iter-226, snapshot-index maintenance after a mutation was scattered
//! across three mechanisms, and every mutating command had to opt into the
//! right one by hand:
//!
//! 1. `mutation.rs::save_index_if_dirty` — 8 call sites across `mv`/`set`/
//!    `remove`/`new`/`append`/`lint --fix`, each pairing an `index_dirty`
//!    bool with ad-hoc `update/add/rename_index_entry` helpers.
//! 2. a *local* `patch_index` in `commands/tasks.rs` that saved the index
//!    immediately after every single task toggle (one write per toggle, not
//!    one per invocation).
//! 3. `dispatch::patch_index_for_modified_files` — the graph-aware rescan
//!    used by `links fix/auto --apply` and `lint --fix`.
//!
//! Nothing forced a new mutating command to refresh the persisted index (or
//! its link graph), and the mtime fallback only caught stale *entries*, never
//! stale *link graphs* (`index.rs:439`'s doc comment records a real bug of
//! exactly this class).
//!
//! [`MutationJournal`] replaces all three: it borrows the loaded snapshot
//! index for the whole duration of a mutating command, tracks dirtiness
//! itself, always refreshes entry *and* link graph, and is flushed exactly
//! once (`[`MutationJournal::flush`]`) at the end of the command. A mutating
//! command that wants to touch the index at all must go through the journal —
//! its constructor is the only sanctioned way to obtain `&mut` access to the
//! snapshot inside a command body, so "mutated the file but forgot to refresh
//! the index" stops being expressible in the normal command shape.
//!
//! The journal picks the graph-aware refresh path on its own: mutation
//! methods that can change links (`update_entry`, `rename_entry`,
//! `rescan_modified`, `update_task`) always refresh the persisted
//! [`hyalo_core::link_graph::LinkGraph`] alongside the entry.

use anyhow::Result;
use indexmap::IndexMap;
use serde_json::Value;
use std::path::Path;

use hyalo_core::filter::extract_tags;
use hyalo_core::index::{SnapshotIndex, VaultIndex as _, format_modified};
use hyalo_core::types::TaskInfo;

/// Owns index maintenance for one mutating command invocation.
///
/// Construct it from the command context's snapshot handle + index path,
/// record every mutation through its methods, and call [`Self::flush`] once
/// at the end (commands that abort early simply drop it — a dirty journal
/// that is never flushed just doesn't write, matching the pre-journal
/// behaviour of an early `return`).
///
/// All methods are no-ops when no snapshot index is loaded (no `--index`).
pub struct MutationJournal<'a> {
    index: &'a mut Option<SnapshotIndex>,
    index_path: Option<&'a Path>,
    dirty: bool,
}

impl<'a> MutationJournal<'a> {
    /// Borrow the command's snapshot index (if loaded) and its on-disk path.
    ///
    /// This is the single entry point mutating commands use to obtain write
    /// access to the snapshot index — see the module docs for the guard
    /// rationale.
    pub fn new(index: &'a mut Option<SnapshotIndex>, index_path: Option<&'a Path>) -> Self {
        Self {
            index,
            index_path,
            dirty: false,
        }
    }

    /// Read-only access to the loaded snapshot index (if any).
    ///
    /// Lets a mutating command resolve inputs / read entries through the
    /// same handle it will later record mutations on.
    #[must_use]
    pub fn index(&self) -> Option<&SnapshotIndex> {
        self.index.as_ref()
    }

    /// Whether a snapshot index is loaded (journal methods are live).
    #[must_use]
    pub fn has_index(&self) -> bool {
        self.index.is_some()
    }

    /// Whether any recorded mutation has dirtied the in-memory index.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Record a frontmatter mutation for `rel_path` (the `set`/`remove`/
    /// `append` write path).
    ///
    /// Patches the entry's `properties`/`tags`/`modified` from the already
    /// updated in-memory `props`, then re-scans the file so the entry's
    /// `links` field *and* the persisted link graph's outbound edges for
    /// this file are current — frontmatter link properties (`related`,
    /// `depends-on`, …) feed the graph, so a property mutation can change
    /// outbound edges.
    ///
    /// Upserts: if `rel_path` is not yet present (a file created outside
    /// `hyalo new`, or on disk before the index was built), it is inserted
    /// from a fresh disk scan rather than silently dropped — a mutation
    /// under `--index` must never leave the index missing the file it just
    /// wrote. No-op only when no index is loaded at all.
    pub fn update_entry(
        &mut self,
        rel_path: &str,
        props: IndexMap<String, Value>,
        full_path: &Path,
    ) -> Result<()> {
        let Some(idx) = self.index.as_mut() else {
            return Ok(());
        };
        if let Some(entry) = idx.get_mut(rel_path) {
            let new_tags = extract_tags(&props);
            entry.properties = props;
            entry.tags = new_tags;
            entry.modified = format_modified(full_path)?;
            idx.refresh_links(full_path, rel_path)?;
        } else {
            idx.insert_or_replace_entry_with_links(full_path, rel_path)?;
        }
        self.dirty = true;
        Ok(())
    }

    /// Record a freshly created file (the `new` write path).
    ///
    /// Scans `full_path` from disk and inserts a complete entry under
    /// `rel_path` (replacing any stale leftover). No-op when no index is
    /// loaded.
    pub fn add_entry(&mut self, rel_path: &str, full_path: &Path) -> Result<()> {
        if let Some(idx) = self.index.as_mut() {
            idx.insert_or_replace_entry(full_path, rel_path)?;
            self.dirty = true;
        }
        Ok(())
    }

    /// Like [`Self::add_entry`], but also registers the file's outbound
    /// links in the persisted [`LinkGraph`] — entry and graph both current.
    ///
    /// BUG-1 (iter-243): used by the `links` heal pass to upsert files the
    /// snapshot never knew, so the discovery pass reads exactly the links a
    /// disk scan would find.
    pub fn add_entry_with_links(&mut self, full_path: &Path, rel_path: &str) -> Result<()> {
        if let Some(idx) = self.index.as_mut() {
            idx.insert_or_replace_entry_with_links(full_path, rel_path)?;
            self.dirty = true;
        }
        Ok(())
    }

    /// Record a file move/rename (the `mv` write path).
    ///
    /// Moves the entry (re-scanning the moved file at its new path),
    /// re-scans every file whose links were rewritten by the move, and
    /// renames the link graph's path keys/sources — so backlink and link
    /// queries stay accurate. No-op when no index is loaded. When `old_rel`
    /// was never indexed, the moved file is upserted at `new_rel` instead
    /// (BUG-1, iter-243): the move must not make an index-unknown file
    /// invisible.
    pub fn rename_entry(
        &mut self,
        dir: &Path,
        old_rel: &str,
        new_rel: &str,
        rewritten_files: &[&str],
    ) -> Result<()> {
        let Some(idx) = self.index.as_mut() else {
            return Ok(());
        };

        // 1. Move the entry: remove old key, re-scan the moved file, insert
        //    under new key (single path-index rebuild via rename_entry).
        //    BUG-1 (iter-243): a file the index never knew (created by an
        //    editor before any create-index saw it) must not silently vanish
        //    from the index by this move — upsert it at the new path, entry
        //    and link graph, like every other mutating write path.
        if !idx.rename_entry(dir, old_rel, new_rel)? {
            idx.insert_or_replace_entry_with_links(&dir.join(new_rel), new_rel)?;
        }

        // 2. Re-scan each file that had links rewritten. The moved file
        //    itself may appear in `rewritten_files` — skip it, step 1
        //    already re-scanned it at the new path. Best-effort.
        for &rel in rewritten_files {
            if rel == new_rel {
                continue;
            }
            let _ = idx.refresh_entry_and_links(dir, rel);
        }

        // 3. Link graph: targets don't change in a move, only source paths.
        idx.graph_mut().rename_path(old_rel, new_rel);

        self.dirty = true;
        Ok(())
    }

    /// Record a task-checkbox mutation (`task toggle` / `task set`).
    ///
    /// Patches the task's status/done flags in the entry and rebuilds
    /// section task counts + `modified`. Task lines cannot change links, so
    /// (like the pre-journal `tasks::patch_index` this replaces) no link
    /// rescan is needed. Batched: nothing is written to disk until
    /// [`Self::flush`], so a multi-file toggle saves once.
    ///
    /// Upserts: if `rel_path` is not yet present, the write this method
    /// records has already landed on disk (callers invoke this after
    /// `toggle_tasks`/`set_tasks_status`), so a fresh disk scan already
    /// reflects the post-toggle state — insert it and register its links
    /// rather than dropping the mutation. No-op only when no index is
    /// loaded at all.
    pub fn update_task(&mut self, full_path: &Path, rel_path: &str, info: &TaskInfo) -> Result<()> {
        let Some(idx) = self.index.as_mut() else {
            return Ok(());
        };
        if idx.get(rel_path).is_none() {
            idx.insert_or_replace_entry_with_links(full_path, rel_path)?;
            self.dirty = true;
            return Ok(());
        }
        if let Some(entry) = idx.get_mut(rel_path) {
            if let Some(task) = entry.tasks.iter_mut().find(|t| t.line == info.line) {
                task.status = info.status;
                task.done = info.done;
            }
            // Rebuild section task counts from the updated task list.
            // Each section owns the range [section.line, next_section.line).
            let section_starts: Vec<usize> = entry.sections.iter().map(|s| s.line).collect();
            for (si, section) in entry.sections.iter_mut().enumerate() {
                let start = section_starts[si];
                let end = section_starts.get(si + 1).copied().unwrap_or(usize::MAX);
                let total = entry
                    .tasks
                    .iter()
                    .filter(|t| t.line >= start && t.line < end)
                    .count();
                if total > 0 {
                    let done = entry
                        .tasks
                        .iter()
                        .filter(|t| t.line >= start && t.line < end && t.done)
                        .count();
                    section.tasks = Some(hyalo_core::types::TaskCount { total, done });
                } else {
                    section.tasks = None;
                }
            }
            entry.modified = format_modified(full_path)?;
            self.dirty = true;
        }
        Ok(())
    }

    /// Record body rewrites performed directly on disk (the `links fix/auto
    /// --apply` and `lint --fix` write paths).
    ///
    /// Re-scans each vault-relative `modified_files` entry once via
    /// `refresh_entry_and_links`, refreshing the full entry (properties,
    /// tags, links, sections, tasks, modified timestamp) *and* the persisted
    /// link graph's outbound edges. Files that fail to rescan produce a
    /// warning and are skipped (best-effort, matching the pre-journal
    /// behaviour). No-op when no index is loaded.
    ///
    /// Upserts: an entry not yet present is inserted from a fresh disk scan
    /// (with link-graph registration) instead of being silently skipped.
    pub fn rescan_modified(&mut self, dir: &Path, modified_files: &[String]) -> Result<()> {
        if modified_files.is_empty() {
            return Ok(());
        }
        let Some(idx) = self.index.as_mut() else {
            return Ok(());
        };
        for rel in modified_files {
            let result = if idx.get(rel).is_some() {
                idx.refresh_entry_and_links(dir, rel).map(|_| ())
            } else {
                idx.insert_or_replace_entry_with_links(&dir.join(rel), rel)
            };
            match result {
                Ok(()) => self.dirty = true,
                Err(e) => {
                    eprintln!("warning: could not refresh index entry for {rel}: {e:#}");
                }
            }
        }
        Ok(())
    }

    /// Persist the index to disk if any recorded mutation dirtied it.
    ///
    /// Called exactly once, at the end of the mutating command. No-op when
    /// clean or when no index/path is available.
    pub fn flush(&mut self) -> Result<()> {
        if self.dirty
            && let (Some(idx), Some(idx_path)) = (self.index.as_mut(), self.index_path)
        {
            idx.save_to(idx_path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_vault(dir: &Path, name: &str) -> String {
        let rel = format!("{name}.md");
        fs::write(
            dir.join(&rel),
            format!("---\ntitle: {name}\ntags: [a]\n---\n\n# {name}\n\n- [ ] todo\n"),
        )
        .unwrap();
        rel
    }

    fn build_snapshot(dir: &Path) -> SnapshotIndex {
        let files = hyalo_core::discovery::discover_files(dir).unwrap();
        let pairs: Vec<(std::path::PathBuf, String)> = files
            .into_iter()
            .map(|f| {
                let rel = f
                    .strip_prefix(dir)
                    .unwrap_or(&f)
                    .to_string_lossy()
                    .replace('\\', "/");
                (f, rel)
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
        let snap_path = dir.join(".hyalo-index-test");
        SnapshotIndex::save(&build.index, &snap_path, "/vault", None, None).unwrap();
        SnapshotIndex::load(&snap_path).unwrap().unwrap()
    }

    #[test]
    fn update_entry_refreshes_entry_and_link_graph() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = make_vault(tmp.path(), "note");
        let full = tmp.path().join(&rel);
        let idx = build_snapshot(tmp.path());
        // Simulate a `set related` mutation changing the frontmatter and
        // the file on disk together.
        let mut props = idx.get(&rel).unwrap().properties.clone();
        props.insert("related".to_string(), Value::String("[[other]]".into()));
        fs::write(
            &full,
            "---\ntitle: note\ntags: [a]\nrelated: '[[other]]'\n---\n\n# note\n\n- [ ] todo\n",
        )
        .unwrap();

        let mut holder = Some(idx);
        let index_path = tmp.path().join("index.json");
        {
            let mut journal = MutationJournal::new(&mut holder, Some(&index_path));
            journal.update_entry(&rel, props, &full).unwrap();
            assert!(journal.is_dirty());
            journal.flush().unwrap();
        }
        assert!(index_path.is_file());

        let mut persisted = SnapshotIndex::load(&index_path).unwrap().unwrap();
        // Entry updated...
        assert!(
            persisted
                .get(&rel)
                .unwrap()
                .properties
                .contains_key("related")
        );
        // ...AND the persisted link graph is current (the index.rs:439
        // stale-graph regression class): the note now backlinks `other`.
        assert!(
            persisted
                .graph_mut()
                .backlinks("other")
                .iter()
                .any(|b| b.source == rel),
            "persisted link graph must contain the new frontmatter-derived edge"
        );
    }

    #[test]
    fn update_task_marks_dirty_without_intermediate_save() {
        let tmp = tempfile::tempdir().unwrap();
        let rel = make_vault(tmp.path(), "note");
        let idx = build_snapshot(tmp.path());
        let info = TaskInfo {
            line: 8,
            status: 'x',
            text: "todo".to_string(),
            done: true,
        };
        let mut holder = Some(idx);
        let index_path = tmp.path().join("index.json");
        let mut journal = MutationJournal::new(&mut holder, Some(&index_path));
        journal
            .update_task(&tmp.path().join(&rel), &rel, &info)
            .unwrap();
        assert!(journal.is_dirty());
        assert!(!index_path.exists(), "no save before flush()");
        journal.flush().unwrap();
        assert!(index_path.exists());
        let persisted = SnapshotIndex::load(&index_path).unwrap().unwrap();
        let entry = persisted.get(&rel).unwrap();
        assert_eq!(entry.tasks.first().map(|t| t.done), Some(true));
    }

    #[test]
    fn no_index_loaded_is_a_noop() {
        let mut holder: Option<SnapshotIndex> = None;
        let mut journal = MutationJournal::new(&mut holder, None);
        assert!(!journal.has_index());
        journal
            .add_entry("x.md", Path::new("/nonexistent/x.md"))
            .unwrap();
        journal.flush().unwrap();
        assert!(!journal.is_dirty());
    }
}
