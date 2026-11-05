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
//!    `patch_index_for_modified_files` helpers;
//! 2. a command module classified as mutating stops naming `MutationJournal`
//!    in its production code (a comment or a test does not count), i.e.
//!    someone adds/rewrites a write path that bypasses the journal; or
//! 3. a module under `commands/` is in no class at all (L-10): every module
//!    must say whether it writes notes, so a new writer cannot start life
//!    outside the guard, and a class entry naming a vanished module is stale.
//!
//! The sanctioned set for direct persistence is: the journal itself,
//! `create_index` / `drop_index` (whole-index build/drop commands), and
//! `run.rs` (initial snapshot load/save plumbing outside command dispatch).

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use syn::visit::{self, Visit};

use crate::workspace::workspace_root;

/// Command modules that write notes and therefore must go through the
/// journal.
const MUTATING_MODULES: &[&str] = &[
    "append",
    "apply",
    "changelog",
    "links",
    "lint",
    "madr",
    "managed_region",
    "mv",
    "new",
    "okf",
    "properties",
    "remove",
    "set",
    "tags",
    "tasks",
    "types",
];

/// Modules that never write a note: queries, output-shaping helpers
/// (`mutation` names the journal only in a doc link), and commands that only
/// touch `.hyalo.toml` or scaffold files outside the index (no entry to
/// maintain).
const NON_MUTATING_MODULES: &[&str] = &[
    "backlinks",
    "config",
    "config_write",
    "files_from",
    "find",
    "init",
    "inputs",
    "lint_rules",
    "mutation",
    "profiles",
    "read",
    "summary",
    "terms",
    "views",
];

/// Files allowed to persist the snapshot index directly (or wrap the
/// journal's flush for legacy call shapes), relative to `crates/hyalo-cli/src`.
const SANCTIONED: &[&str] = &[
    "commands/journal.rs",
    "commands/create_index.rs",
    "commands/drop_index.rs",
    "run.rs",
];

/// Modules that persist the index themselves and are checked by the token scan
/// rather than the journal reference.
const PERSISTENCE_MODULES: &[&str] = &["create_index", "drop_index", "journal"];

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

    // 2. Every module under `commands/` is classified, and a mutating one
    //    names the journal in production code.
    //
    // A module is its file *plus* every file of a same-named submodule
    // directory: iteration 247 split `commands/lint.rs` into `commands/lint.rs`
    // + `commands/lint/`, which moved the journal reference out of the file this
    // gate was reading. Reading the whole module keeps the guard about the
    // module's behaviour rather than about which file inside it holds the call.
    let commands = cli_src.join("commands");
    let present = module_names(&commands)?;
    check_classification(&present, &mut failures);
    for module in MUTATING_MODULES {
        if !present.iter().any(|name| name == module) {
            continue;
        }
        if !module_names_journal(&commands, module)? {
            failures.push(format!(
                "commands/{module}: mutating command does not reference MutationJournal \
                 in production code — every file/link write must record its index \
                 maintenance through the journal"
            ));
        }
    }

    if failures.is_empty() {
        println!(
            "mutation-journal guard: OK ({} mutating modules checked, {} modules classified)",
            MUTATING_MODULES.len(),
            present.len()
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

/// Module names under `commands/`: each `*.rs` file stem and each directory,
/// once (`lint.rs` and `lint/` are the one module `lint`).
fn module_names(commands: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for entry in
        std::fs::read_dir(commands).with_context(|| format!("reading {}", commands.display()))?
    {
        let path = entry?.path();
        let name = if path.is_dir() {
            path.file_name()
        } else if path.extension().is_some_and(|e| e == "rs") {
            path.file_stem()
        } else {
            None
        };
        if let Some(name) = name.and_then(|n| n.to_str())
            && name != "mod"
            && !names.iter().any(|n| n == name)
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Failures for modules in no class, in two classes, or classified but gone.
fn check_classification(present: &[String], failures: &mut Vec<String>) {
    let classes = [MUTATING_MODULES, NON_MUTATING_MODULES, PERSISTENCE_MODULES];
    for name in present {
        let count = classes
            .iter()
            .filter(|c| c.contains(&name.as_str()))
            .count();
        if count == 0 {
            failures.push(format!(
                "commands/{name}: not classified — add it to MUTATING_MODULES (writes notes) \
                 or NON_MUTATING_MODULES in crates/xtask/src/mutation_journal.rs"
            ));
        } else if count > 1 {
            failures.push(format!("commands/{name}: listed in more than one class"));
        }
    }
    for class in classes {
        for name in class {
            if !present.iter().any(|p| p == name) {
                failures.push(format!(
                    "commands/{name}: classified in mutation_journal.rs but no such module"
                ));
            }
        }
    }
}

/// Whether the production code of module `name` (its file and, recursively,
/// its same-named directory) names `MutationJournal`.
fn module_names_journal(commands: &Path, name: &str) -> Result<bool> {
    let mut files: Vec<PathBuf> = Vec::new();
    let file = commands.join(format!("{name}.rs"));
    if file.is_file() {
        files.push(file);
    }
    let dir = commands.join(name);
    if dir.is_dir() {
        let mut stack = vec![dir];
        while let Some(dir) = stack.pop() {
            for entry in
                std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))?
            {
                let path = entry?.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path);
                }
            }
        }
    }
    for path in files {
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        if source_names_journal(&source).with_context(|| format!("parsing {}", path.display()))? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `true` when non-test code names the identifier `MutationJournal`.
fn source_names_journal(source: &str) -> Result<bool> {
    struct Finder(bool);
    impl<'ast> Visit<'ast> for Finder {
        fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
            if !crate::typed_output::test_only(&item.attrs) {
                visit::visit_item_mod(self, item);
            }
        }
        fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
            if !crate::typed_output::test_only(&item.attrs) {
                visit::visit_item_fn(self, item);
            }
        }
        fn visit_ident(&mut self, ident: &'ast proc_macro2::Ident) {
            if ident == "MutationJournal" {
                self.0 = true;
            }
        }
    }
    let file = syn::parse_file(source)?;
    let mut finder = Finder(false);
    finder.visit_file(&file);
    Ok(finder.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_or_a_test_does_not_count_as_a_journal_reference() {
        assert!(!source_names_journal("// MutationJournal\nfn f() {}").unwrap());
        assert!(!source_names_journal("/// MutationJournal\nfn f() {}").unwrap());
        assert!(!source_names_journal("#[cfg(test)]\nmod t { use x::MutationJournal; }").unwrap());
        assert!(source_names_journal("use x::MutationJournal;\nfn f() {}").unwrap());
        assert!(source_names_journal("fn f(j: &mut MutationJournal) {}").unwrap());
    }

    #[test]
    fn unclassified_doubly_classified_and_vanished_modules_fail() {
        let mut failures = Vec::new();
        let present: Vec<String> = MUTATING_MODULES
            .iter()
            .chain(NON_MUTATING_MODULES)
            .chain(PERSISTENCE_MODULES)
            .map(|s| (*s).to_owned())
            .collect();
        check_classification(&present, &mut failures);
        assert!(failures.is_empty(), "{failures:?}");

        let mut with_new = present.clone();
        with_new.push("brand_new".to_owned());
        check_classification(&with_new, &mut failures);
        assert_eq!(failures.len(), 1);

        failures.clear();
        let without_set: Vec<String> = present.into_iter().filter(|n| n != "set").collect();
        check_classification(&without_set, &mut failures);
        assert_eq!(failures.len(), 1);
    }
}
