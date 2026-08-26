//! ARCH-3 guard (iter-226): a mutating command cannot silently skip index
//! refresh.
//!
//! Since iter-226 every mutating command records its index maintenance
//! through `MutationJournal` (`crates/hyalo-cli/src/commands/journal.rs`).
//! This gate source-scans the hyalo-cli crate and fails CI when either:
//!
//! 1. one of the pre-journal mechanisms reappears outside the sanctioned
//!    files — a direct `SnapshotIndex::save_to` call, or a reference to the
//!    deleted `mutation::{update_index_entry, add_index_entry,
//!    rename_index_entry, save_index_if_dirty}` /
//!    `patch_index_for_modified_files` helpers; or
//! 2. a mutating command module (the fixed list below) stops referencing
//!    `MutationJournal`, i.e. someone adds/rewrites a write path that
//!    bypasses the journal.
//!
//! The sanctioned set for direct persistence is: the journal itself,
//! `create_index` / `drop_index` (whole-index build/drop commands), and
//! `run.rs` (initial snapshot load/save plumbing outside command dispatch).

use anyhow::{Context, Result};
use std::path::Path;

use crate::workspace::workspace_root;

/// Command modules that mutate files and therefore must go through the
/// journal. New mutating commands: add the module here — the gate then
/// enforces the journal reference.
const MUTATING_MODULES: &[&str] = &[
    "commands/set.rs",
    "commands/remove.rs",
    "commands/append.rs",
    "commands/new.rs",
    "commands/mv.rs",
    "commands/tasks.rs",
    "commands/properties.rs",
    "commands/tags.rs",
    "commands/links.rs",
    "commands/lint.rs",
];

/// Files allowed to persist the snapshot index directly (or wrap the
/// journal's flush for legacy call shapes).
const SANCTIONED: &[&str] = &[
    "commands/journal.rs",
    "commands/create_index.rs",
    "commands/drop_index.rs",
    "src/run.rs",
];

/// Pre-journal helper names whose reappearance outside `mutation.rs`
/// (post-deletion: anywhere) signals a bypass of the journal.
const FORBIDDEN_TOKENS: &[&str] = &[
    "save_index_if_dirty",
    "update_index_entry",
    "add_index_entry",
    "rename_index_entry",
    "patch_index_for_modified_files",
    ".save_to(",
];

/// `MutationJournal::flush` is the only sanctioned flush; direct `save_to`
/// outside the sanctioned set is caught above.
fn rel(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    let cli_src = root.join("crates/hyalo-cli/src");

    let mut failures: Vec<String> = Vec::new();

    // 1. Forbidden persistence tokens outside sanctioned files.
    let mut stack = vec![cli_src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in
            std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let rel_path = rel(&path);
            let short = rel_path
                .strip_prefix(rel(&cli_src).as_str())
                .unwrap_or(&rel_path)
                .trim_start_matches('/')
                .to_string();
            let sanctioned = SANCTIONED.iter().any(|s| short == *s);
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            for token in FORBIDDEN_TOKENS {
                if content.contains(token) && !sanctioned {
                    failures.push(format!(
                        "{short}: forbidden index-persistence token `{token}` \
                         (mutations must go through MutationJournal)"
                    ));
                }
            }
        }
    }

    // 2. Mutating modules must reference the journal.
    for module in MUTATING_MODULES {
        let path = cli_src.join(module);
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        if !content.contains("MutationJournal") {
            failures.push(format!(
                "{module}: mutating command does not reference MutationJournal \
                 — every file/link write must record its index maintenance \
                 through the journal"
            ));
        }
    }

    if failures.is_empty() {
        println!(
            "mutation-journal guard: OK ({} mutating modules checked)",
            MUTATING_MODULES.len()
        );
        Ok(true)
    } else {
        for f in &failures {
            eprintln!("mutation-journal guard FAILURE: {f}");
        }
        eprintln!(
            "\n{} failure(s). See crates/hyalo-cli/src/commands/journal.rs \
             module docs (ARCH-3, iter-226).",
            failures.len()
        );
        Ok(false)
    }
}
