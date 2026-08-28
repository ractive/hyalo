//! Unit tests for the hint generators.
//!
//! Split out of `hints.rs` in iteration 247: the module body is unchanged, it
//! just lives in its own file now.

use super::*;
// Lives in a sibling submodule after the iteration-247 file split and is
// exercised only from here, so the parent has no reason to import it.
use super::find::build_views_set_command;
use serde_json::json;

fn ctx(source: HintSource) -> HintContext {
    HintContext::new(source)
}

// -----------------------------------------------------------------------
// HintBuilder (ARCH-4, iter-225)
// -----------------------------------------------------------------------

#[test]
fn hint_builder_basic_serialization() {
    let b = HintBuilder::cmd("lint")
        .flag_value("--rule", "HYALO006")
        .flag("--detailed");
    assert_eq!(b.build(), "hyalo lint --rule HYALO006 --detailed");
}

#[test]
fn hint_builder_quotes_shell_specials() {
    let b = HintBuilder::cmd("find").flag_value("--property", "status=todo now");
    assert_eq!(b.build(), "hyalo find --property 'status=todo now'");
}

#[test]
fn hint_builder_subcommand_groups() {
    let b = HintBuilder::cmd("task toggle").arg("todo.md").flag("--all");
    assert_eq!(b.build(), "hyalo task toggle todo.md --all");
    assert_eq!(b.argv(), &["hyalo", "task", "toggle", "todo.md", "--all"]);
}

/// The typed half of ARCH-4: a command assembled through `HintBuilder`
/// must be accepted by the *real* clap parser. This is the in-process
/// version of what `tests/e2e/hint_execution.rs` proves by spawning the
/// binary — the `tags --limit 0` drift (a hint that satisfied a substring
/// assertion but failed to run) is now a unit-test failure the moment it
/// is written.
#[test]
fn hint_builder_commands_parse() {
    let cases: Vec<(String, Vec<String>)> = vec![
        ("hyalo summary".to_owned(), vec![]),
        ("hyalo types list".to_owned(), vec![]),
        (
            "hyalo lint".to_owned(),
            vec![
                "--rule".to_owned(),
                "HYALO006".to_owned(),
                "--detailed".to_owned(),
            ],
        ),
        (
            "hyalo find".to_owned(),
            vec![
                "--broken-links".to_owned(),
                "--strict".to_owned(),
                "--limit".to_owned(),
                "0".to_owned(),
            ],
        ),
        (
            "hyalo task toggle".to_owned(),
            vec!["todo.md".to_owned(), "--all".to_owned()],
        ),
        (
            "hyalo tags summary".to_owned(),
            vec!["--limit".to_owned(), "0".to_owned()],
        ),
        (
            "hyalo set".to_owned(),
            vec![
                "--property".to_owned(),
                "status=done".to_owned(),
                "--file".to_owned(),
                "notes/a.md".to_owned(),
            ],
        ),
        (
            "hyalo create-index".to_owned(),
            vec!["--dir".to_owned(), "/my/vault".to_owned()],
        ),
    ];
    for (subcommand, extra) in cases {
        let subcommand = subcommand.strip_prefix("hyalo ").unwrap_or(&subcommand);
        let mut b = HintBuilder::cmd(subcommand);
        for arg in &extra {
            b = b.raw(arg);
        }
        let argv: Vec<String> = b.argv().to_vec();
        <crate::cli::args::Cli as clap::Parser>::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("hint does not parse: {argv:?}: {e}"));
    }
}

/// ARCH-4 drift guard: no NEW hand-written `"hyalo …"` command strings in
/// non-test source. Every hint command must go through [`HintBuilder`] so
/// the argv stays shell-quoted and parseable. A failure here means a new
/// hand-assembled hint was added — build it with `HintBuilder::cmd(...)`
/// instead (the remaining matches below are prose/starts_with checks, not
/// commands, and are allow-listed).
#[test]
fn no_raw_hyalo_command_literals() {
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let allow: &[(&str, &str)] = &[
        // starts_with() checks, not literals of hint commands
        ("cli/help.rs", "if !trimmed.starts_with(\"hyalo \") {"),
        // prose in an error message, not a command
        (
            "commands/mod.rs",
            "\"hyalo does not support dotted path syntax for nested properties — --property \\",
        ),
        // warn.rs messages name the program, they are not commands
        (
            "warn.rs",
            "\"hyalo is configured with dir = \\\"{dir_display}\\\".\\n  \\",
        ),
    ];
    let mut offenders = Vec::new();
    let mut stack = vec![src_dir.clone()];
    while let Some(dir) = stack.pop() {
        let entries =
            std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(std::ffi::OsStr::to_str) != Some("rs") {
                continue;
            }
            let rel = path
                .strip_prefix(&src_dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // Skip whole-file test modules. Before iteration 247 every test
            // module was an inline `#[cfg(test)] mod tests { … }` and the
            // truncation below was enough; splitting `hints.rs` moved this
            // very file out to `hints/tests.rs`, where the marker sits on the
            // `mod tests;` declaration in the parent instead. A file named
            // `tests.rs` is a test module by convention in this crate, and its
            // fixtures legitimately quote commands.
            if path.file_stem().and_then(std::ffi::OsStr::to_str) == Some("tests") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            // Skip inline test modules — their fixtures legitimately quote commands.
            let text = match text.find("#[cfg(test)]") {
                Some(i) => &text[..i],
                None => &text[..],
            };
            for (n, line) in text.lines().enumerate() {
                let t = line.trim_start();
                if t.starts_with("//") || t.starts_with("//!") {
                    continue;
                }
                if line.contains("\"hyalo ") {
                    let key = (rel.as_str(), t);
                    if !allow.contains(&key) {
                        offenders.push(format!("{}:{}: {}", rel, n + 1, t));
                    }
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "raw hyalo command string literals found (build hint commands with \
         HintBuilder::cmd(...) instead — ARCH-4, iter-225):\n{}",
        offenders.join("\n")
    );
}

fn ctx_with_dir(source: HintSource, dir: &str) -> HintContext {
    let mut ctx = HintContext::new(source);
    ctx.dir = Some(dir.to_owned());
    ctx
}

fn ctx_with_glob(source: HintSource, glob: &str) -> HintContext {
    let mut ctx = HintContext::new(source);
    ctx.glob = vec![glob.to_owned()];
    ctx
}

// --- global flag placement (iter-213, dogfood UX-5) ---

/// Every builder must leave `--dir`/`--format` at the tail, so a block of
/// hints under one result set reads as variations of one command instead
/// of scattering the same two flags at different offsets.
#[test]
fn global_flags_are_the_last_thing_every_builder_pushes() {
    let mut ctx = HintContext::new(HintSource::Find);
    ctx.dir = Some("kb".to_owned());
    ctx.format = Some("text".to_owned());
    ctx.glob = vec!["notes/*.md".to_owned()];
    ctx.file_targets = vec!["a.md".to_owned()];

    for cmd in [
        build_command_no_glob(&ctx, &["summary"]),
        build_command_with_file(&ctx, &["read"], "a.md", &[]),
        build_command_with_glob(&ctx, &["lint"]),
        build_command_with_glob_and_files(&ctx, &["lint"]),
        build_find_command_preserving_filters(&ctx, &["--limit", "5"]),
        build_views_set_command(&ctx, "my-view"),
    ] {
        assert!(
            cmd.ends_with("--dir kb --format text"),
            "global flags must trail: {cmd}"
        );
    }
}

// --- shell_quote ---

#[test]
fn shell_quote_plain_string() {
    assert_eq!(shell_quote("status"), "status");
}

#[test]
fn shell_quote_string_with_space() {
    assert_eq!(shell_quote("in progress"), "'in progress'");
}

#[test]
fn shell_quote_string_with_special_chars() {
    assert_eq!(shell_quote("foo$bar"), "'foo$bar'");
}

#[test]
fn shell_quote_string_with_single_quote() {
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
}

#[test]
fn shell_quote_glob_chars() {
    assert_eq!(shell_quote("**/*.md"), "'**/*.md'");
}

#[test]
fn shell_quote_empty_string() {
    assert_eq!(shell_quote(""), "''");
}

// --- build_command ---

#[test]
fn build_command_no_flags() {
    let c = ctx(HintSource::Summary);
    assert_eq!(
        build_command_no_glob(&c, &["properties"]),
        "hyalo properties"
    );
}

#[test]
fn build_command_with_dir() {
    let c = ctx_with_dir(HintSource::Summary, "/my/vault");
    assert_eq!(
        build_command_no_glob(&c, &["tags"]),
        "hyalo tags --dir /my/vault"
    );
}

#[test]
fn build_command_with_glob_propagated() {
    let c = ctx_with_glob(HintSource::PropertiesSummary, "**/*.md");
    assert_eq!(
        build_command_with_glob(&c, &["properties"]),
        "hyalo properties --glob '**/*.md'"
    );
}

// --- status_priority ---

#[test]
fn status_priority_ordering() {
    assert!(status_priority("in-progress") < status_priority("planned"));
    assert!(status_priority("planned") < status_priority("draft"));
    assert!(status_priority("draft") < status_priority("custom"));
    assert!(status_priority("custom") < status_priority("completed"));
}

// --- hints_for_summary ---

#[test]
fn summary_always_includes_properties_and_tags() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 10, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [],
        "tasks": {"total": 0, "done": 0},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(hints.iter().any(|h| {
        h.cmd == "hyalo properties"
            || (h.cmd.starts_with("hyalo properties ") && h.cmd.contains("--dir "))
            || (h.cmd.starts_with("hyalo properties ") && h.cmd.contains("--glob "))
    }));
    assert!(hints.iter().any(|h| {
        h.cmd == "hyalo tags"
            || (h.cmd.starts_with("hyalo tags ") && h.cmd.contains("--dir "))
            || (h.cmd.starts_with("hyalo tags ") && h.cmd.contains("--glob "))
    }));
}

#[test]
fn summary_suggests_tasks_todo_when_open_tasks() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 5, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [],
        "tasks": {"total": 10, "done": 3},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find") && h.cmd.contains("--task") && h.cmd.contains("todo"))
    );
}

#[test]
fn summary_omits_tasks_todo_when_all_done() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 5, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [],
        "tasks": {"total": 10, "done": 10},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(!hints.iter().any(|h| h.cmd.contains("--todo")));
}

#[test]
fn summary_picks_interesting_status_values() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 5, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [
            {"value": "completed", "files": ["a.md"]},
            {"value": "in-progress", "files": ["b.md"]},
            {"value": "planned", "files": ["c.md"]}
        ],
        "tasks": {"total": 0, "done": 0},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    // in-progress should appear before completed
    let in_progress_pos = hints.iter().position(|h| h.cmd.contains("in-progress"));
    let completed_pos = hints.iter().position(|h| h.cmd.contains("completed"));
    assert!(in_progress_pos.is_some(), "should suggest in-progress");
    // completed may appear (only if limit not reached) or not — but in-progress must come first
    if let Some(cp) = completed_pos {
        assert!(in_progress_pos.unwrap() < cp);
    }
}

#[test]
fn summary_max_hints_not_exceeded() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 5, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [
            {"value": "in-progress", "files": ["a.md"]},
            {"value": "planned", "files": ["b.md"]},
            {"value": "draft", "files": ["c.md"]},
            {"value": "idea", "files": ["d.md"]}
        ],
        "tasks": {"total": 5, "done": 1},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(hints.len() <= MAX_HINTS);
}

// --- hints_for_properties_summary ---

#[test]
fn properties_summary_top3_by_count() {
    let c = ctx(HintSource::PropertiesSummary);
    let data = json!([
        {"name": "title", "type": "text", "count": 100},
        {"name": "status", "type": "text", "count": 50},
        {"name": "tags", "type": "list", "count": 30},
        {"name": "author", "type": "text", "count": 5}
    ]);
    let hints = generate_hints(&c, &data, None);
    assert_eq!(hints.len(), 3);
    assert!(hints[0].cmd.contains("title"));
    assert!(hints[1].cmd.contains("status"));
    assert!(hints[2].cmd.contains("tags"));
    // author should not appear (rank 4)
    assert!(!hints.iter().any(|h| h.cmd.contains("author")));
}

#[test]
fn properties_summary_empty_data() {
    let c = ctx(HintSource::PropertiesSummary);
    let hints = generate_hints(&c, &json!([]), None);
    assert!(hints.is_empty());
}

#[test]
fn properties_summary_propagates_glob() {
    let c = ctx_with_glob(HintSource::PropertiesSummary, "notes/*.md");
    let data = json!([{"name": "status", "type": "text", "count": 5}]);
    let hints = generate_hints(&c, &data, None);
    assert!(hints[0].cmd.contains("--glob"));
    assert!(hints[0].cmd.contains("notes/*.md"));
}

// --- hints_for_find ---

fn make_find_item(file: &str, status: Option<&str>, tags: &[&str]) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    if let Some(s) = status {
        props.insert("status".to_owned(), serde_json::Value::String(s.to_owned()));
    }
    json!({
        "file": file,
        "properties": props,
        "tags": tags,
        "sections": [],
        "tasks": [],
        "links": [],
        "modified": "2026-01-01T00:00:00Z"
    })
}

#[test]
fn find_empty_results_no_hints() {
    let c = ctx(HintSource::Find);
    let hints = generate_hints(&c, &json!([]), None);
    assert!(hints.is_empty());
}

#[test]
fn find_single_result_suggests_read_and_backlinks() {
    let c = ctx(HintSource::Find);
    let items = vec![make_find_item("notes/alpha.md", None, &[])];
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("read") && h.cmd.contains("alpha.md")),
        "should suggest read: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("backlinks") && h.cmd.contains("alpha.md")),
        "should suggest backlinks: {hints:?}"
    );
}

#[test]
fn find_many_results_suggests_top_tag() {
    let c = ctx(HintSource::Find);
    // 6 results; rust appears 4 times, cli 2 times — rust should be suggested.
    let items = vec![
        make_find_item("a.md", Some("planned"), &["rust", "cli"]),
        make_find_item("b.md", Some("planned"), &["rust"]),
        make_find_item("c.md", Some("in-progress"), &["rust"]),
        make_find_item("d.md", Some("completed"), &["rust"]),
        make_find_item("e.md", Some("completed"), &["cli"]),
        make_find_item("f.md", Some("completed"), &[]),
    ];
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("--tag") && h.cmd.contains("rust")),
        "should suggest --tag rust (most common): {hints:?}"
    );
}

#[test]
fn find_many_results_suggests_interesting_status() {
    let c = ctx(HintSource::Find);
    // 6 results; in-progress is more interesting than completed.
    let items = vec![
        make_find_item("a.md", Some("in-progress"), &[]),
        make_find_item("b.md", Some("completed"), &[]),
        make_find_item("c.md", Some("completed"), &[]),
        make_find_item("d.md", Some("completed"), &[]),
        make_find_item("e.md", Some("completed"), &[]),
        make_find_item("f.md", Some("completed"), &[]),
    ];
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("--property") && h.cmd.contains("status=in-progress")),
        "should prefer in-progress status: {hints:?}"
    );
}

#[test]
fn find_many_results_no_tags_falls_back_to_status() {
    let c = ctx(HintSource::Find);
    // 6 results, none with tags; should still suggest status narrowing.
    let items = vec![
        make_find_item("a.md", Some("planned"), &[]),
        make_find_item("b.md", Some("planned"), &[]),
        make_find_item("c.md", Some("planned"), &[]),
        make_find_item("d.md", Some("planned"), &[]),
        make_find_item("e.md", Some("planned"), &[]),
        make_find_item("f.md", Some("planned"), &[]),
    ];
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("--property") && h.cmd.contains("status=planned")),
        "should suggest status filter: {hints:?}"
    );
    // No --tag hints when no tags exist.
    assert!(
        !hints.iter().any(|h| h.cmd.contains("--tag")),
        "should not suggest --tag when no tags: {hints:?}"
    );
}

#[test]
fn find_hints_never_exceed_max() {
    let c = ctx(HintSource::Find);
    // 10 results with varied tags and statuses.
    let items: Vec<serde_json::Value> = (0..10)
        .map(|i| make_find_item(&format!("{i}.md"), Some("planned"), &["rust", "cli"]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(hints.len() <= MAX_HINTS);
}

#[test]
fn find_sort_hint_preserves_existing_filters() {
    let mut c = ctx(HintSource::Find);
    c.property_filters = vec!["status=draft".to_owned()];
    c.tag_filters = vec!["research".to_owned()];
    // 6 results to trigger sort/limit hints.
    let items: Vec<serde_json::Value> = (0..6)
        .map(|i| make_find_item(&format!("{i}.md"), Some("draft"), &["research"]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    let sort_hint = hints.iter().find(|h| h.cmd.contains("--sort"));
    assert!(sort_hint.is_some(), "should include a sort hint: {hints:?}");
    let cmd = &sort_hint.unwrap().cmd;
    assert!(
        cmd.contains("--property status=draft"),
        "sort hint should preserve --property filter: {cmd}"
    );
    assert!(
        cmd.contains("--tag research"),
        "sort hint should preserve --tag filter: {cmd}"
    );
}

#[test]
fn find_limit_hint_preserves_existing_filters() {
    let mut c = ctx(HintSource::Find);
    c.tag_filters = vec!["iteration".to_owned()];
    let items: Vec<serde_json::Value> = (0..6)
        .map(|i| make_find_item(&format!("{i}.md"), Some("planned"), &["iteration"]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    let limit_hint = hints.iter().find(|h| h.cmd.contains("--limit"));
    assert!(
        limit_hint.is_some(),
        "should include a limit hint: {hints:?}"
    );
    let cmd = &limit_hint.unwrap().cmd;
    assert!(
        cmd.contains("--tag iteration"),
        "limit hint should preserve --tag filter: {cmd}"
    );
}

// --- flag propagation ---

#[test]
fn dir_flag_propagated_to_all_hints() {
    let c = ctx_with_dir(HintSource::TagsSummary, "/vault");
    // tags summary returns a bare array [{name, count}, ...]
    let data = json!([{"name": "rust", "count": 5}]);
    let hints = generate_hints(&c, &data, None);
    assert!(hints[0].cmd.contains("--dir"));
    assert!(hints[0].cmd.contains("/vault"));
}

// --- new generator tests ---

#[test]
fn mutation_hints_suggest_verify_and_read() {
    let c = ctx(HintSource::Set);
    let data = json!({
        "property": "status",
        "value": "completed",
        "modified": ["notes/alpha.md"],
        "skipped": [],
        "total": 1
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find") && h.cmd.contains("alpha.md")),
        "should suggest verify: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("read") && h.cmd.contains("alpha.md")),
        "should suggest read: {hints:?}"
    );
}

#[test]
fn read_hints_suggest_metadata_and_backlinks() {
    let c = ctx(HintSource::Read);
    let data = json!({"file": "notes/alpha.md", "content": "Some content"});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find") && h.cmd.contains("alpha.md")),
        "should suggest find: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("backlinks") && h.cmd.contains("alpha.md")),
        "should suggest backlinks: {hints:?}"
    );
}

#[test]
fn backlinks_hints_suggest_read_and_outgoing() {
    let c = ctx(HintSource::Backlinks);
    let data = json!({
        "file": "target.md",
        "backlinks": [{"source": "a.md", "line": 5, "target": "target"}],
        "total": 1
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("read") && h.cmd.contains("target.md")),
        "should suggest read target: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("read") && h.cmd.contains("a.md")),
        "should suggest read first backlink source: {hints:?}"
    );
}

#[test]
fn create_index_hints_suggest_find_and_drop() {
    let c = ctx(HintSource::CreateIndex);
    let data = json!({"path": ".hyalo-index", "files_indexed": 42, "warnings": 0});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find") && h.cmd.contains("--index")),
        "should suggest find with index: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("drop-index")),
        "should suggest drop-index: {hints:?}"
    );
}

#[test]
fn drop_index_hints_suggest_create() {
    let c = ctx(HintSource::DropIndex);
    let data = json!({"deleted": ".hyalo-index"});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("create-index")),
        "should suggest create-index: {hints:?}"
    );
}

#[test]
fn mv_dry_run_hints_suggest_apply() {
    let c = ctx(HintSource::Mv);
    let data = json!({
        "from": "old.md",
        "to": "new.md",
        "dry_run": true,
        "updated_files": [],
        "total_files_updated": 0,
        "total_links_updated": 0
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("mv")
            && h.cmd.contains("new.md")
            && !h.cmd.contains("dry-run")),
        "should suggest applying the move: {hints:?}"
    );
}

#[test]
fn mv_applied_hints_suggest_read_and_backlinks() {
    let c = ctx(HintSource::Mv);
    let data = json!({
        "from": "old.md",
        "to": "new.md",
        "dry_run": false,
        "updated_files": [],
        "total_files_updated": 0,
        "total_links_updated": 0
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("read") && h.cmd.contains("new.md")),
        "should suggest reading moved file: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("backlinks") && h.cmd.contains("new.md")),
        "should suggest checking backlinks: {hints:?}"
    );
}

#[test]
fn task_read_undone_suggests_toggle() {
    let c = ctx(HintSource::TaskRead);
    let data =
        json!({"file": "todo.md", "line": 5, "status": " ", "text": "Fix bug", "done": false});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("task toggle")),
        "should suggest toggling undone task: {hints:?}"
    );
}

#[test]
fn task_read_done_omits_toggle() {
    let c = ctx(HintSource::TaskRead);
    let data =
        json!({"file": "todo.md", "line": 5, "status": "x", "text": "Fix bug", "done": true});
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("task toggle")),
        "should not suggest toggling already-done task: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("--task todo")),
        "should suggest viewing open tasks: {hints:?}"
    );
}

#[test]
fn task_mutation_hints_suggest_remaining_tasks() {
    let c = ctx(HintSource::TaskToggle);
    let data =
        json!({"file": "todo.md", "line": 5, "status": "x", "text": "Fix bug", "done": true});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find") && h.cmd.contains("--task") && h.cmd.contains("todo")),
        "should suggest finding remaining tasks: {hints:?}"
    );
}

#[test]
fn links_fix_dry_run_hints_suggest_apply() {
    let c = ctx(HintSource::LinksFix);
    let data = json!({
        "broken": 5,
        "fixable": 3,
        "unfixable": 2,
        "applied": false,
        "fixes": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("links fix --apply")),
        "should suggest applying fixes: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("--broken-links")),
        "should suggest finding broken links: {hints:?}"
    );
}

#[test]
fn links_fix_apply_failures_suggest_checking_failed_fixes() {
    let c = ctx(HintSource::LinksFix);
    let data = json!({
        "broken": 3,
        "fixable": 3,
        "unfixable": 0,
        "applied": true,
        "fixes": [],
        "failed": 2,
        "failed_fixes": [],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.is_empty() && h.description.contains("failed_fixes")),
        "should surface an advice-only hint pointing at failed_fixes: {hints:?}"
    );
}

#[test]
fn links_fix_no_failures_omits_failed_fixes_hint() {
    let c = ctx(HintSource::LinksFix);
    let data = json!({
        "broken": 3,
        "fixable": 3,
        "unfixable": 0,
        "applied": true,
        "fixes": [],
        "failed": 0,
        "failed_fixes": [],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.description.contains("failed_fixes")),
        "should not mention failed_fixes when nothing failed: {hints:?}"
    );
}

#[test]
fn links_auto_apply_failures_suggest_checking_apply_outcomes() {
    let c = ctx(HintSource::LinksAuto);
    let data = json!({
        "matched": 5,
        "dry_run": false,
        "applied": true,
        "files_applied": 3,
        "files_skipped": 0,
        "files_failed": 2,
        "apply_outcomes": [],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.is_empty() && h.description.contains("apply_outcomes")),
        "should surface an advice-only hint pointing at apply_outcomes: {hints:?}"
    );
}

#[test]
fn links_auto_no_failures_omits_apply_outcomes_hint() {
    let c = ctx(HintSource::LinksAuto);
    let data = json!({
        "matched": 5,
        "dry_run": false,
        "applied": true,
        "files_applied": 5,
        "files_skipped": 0,
        "files_failed": 0,
        "apply_outcomes": [],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints
            .iter()
            .any(|h| h.description.contains("apply_outcomes")),
        "should not mention apply_outcomes when nothing failed: {hints:?}"
    );
}

#[test]
fn find_broad_query_suggests_summary() {
    let c = ctx(HintSource::Find);
    // 15 results, no filters
    let items: Vec<serde_json::Value> = (0..15)
        .map(|i| make_find_item(&format!("{i}.md"), Some("completed"), &[]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("summary")),
        "broad query should suggest summary: {hints:?}"
    );
}

#[test]
fn find_with_filters_does_not_suggest_summary() {
    let mut c = ctx(HintSource::Find);
    c.tag_filters = vec!["rust".to_owned()];
    let items: Vec<serde_json::Value> = (0..15)
        .map(|i| make_find_item(&format!("{i}.md"), Some("completed"), &["rust"]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("summary")),
        "filtered query should not suggest summary: {hints:?}"
    );
}

#[test]
fn find_suppresses_already_filtered_tag() {
    let mut c = ctx(HintSource::Find);
    c.tag_filters = vec!["rust".to_owned()];
    let items: Vec<serde_json::Value> = (0..10)
        .map(|i| make_find_item(&format!("{i}.md"), Some("planned"), &["rust", "cli"]))
        .collect();
    let data = json!(items);
    let hints = generate_hints(&c, &data, None);
    // Should NOT suggest narrowing *by* the already-filtered `rust` tag.
    // Narrow hints now compose with the active filter (BUG-8), so the
    // preserved `--tag rust` legitimately appears in the command; the
    // *narrow target* is what must differ. Assert the narrow hint's
    // description names `cli`, not `rust`.
    assert!(
        !hints.iter().any(|h| h.description == "Narrow by tag: rust"
            || h.description.starts_with("Narrow by tag: rust ")),
        "should not suggest narrowing by already-filtered tag: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.description.starts_with("Narrow by tag: cli")),
        "should suggest narrowing by non-filtered tag: {hints:?}"
    );
}

#[test]
fn summary_broken_links_suggests_links_fix() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": 10,
        "links": {"total": 20, "broken": 3},
        "properties": [],
        "tags": [],
        "status": [],
        "tasks": {"total": 0, "done": 0},
        "orphans": 0
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("links fix")),
        "summary with broken links should suggest links fix: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("--broken-links")),
        "summary with broken links should also suggest find --broken-links: {hints:?}"
    );
}

#[test]
fn summary_no_broken_links_omits_links_fix() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": 10,
        "links": {"total": 20, "broken": 0},
        "properties": [],
        "tags": [],
        "status": [],
        "tasks": {"total": 0, "done": 0},
        "orphans": 0
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("links fix")),
        "summary without broken links should not suggest links fix: {hints:?}"
    );
}

#[test]
fn find_with_broken_links_suggests_links_fix() {
    let c = ctx(HintSource::Find);
    let item = json!({
        "file": "doc.md",
        "properties": {},
        "tags": [],
        "sections": [],
        "tasks": [],
        "links": [
            {"target": "existing.md", "path": "existing.md", "kind": "wiki"},
            {"target": "gone.md", "path": null, "kind": "wiki"}
        ],
        "modified": "2026-01-01T00:00:00Z"
    });
    let data = json!([item]);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("links fix")),
        "find results with broken links should suggest links fix: {hints:?}"
    );
}

#[test]
fn find_without_broken_links_omits_links_fix() {
    let c = ctx(HintSource::Find);
    let item = json!({
        "file": "doc.md",
        "properties": {},
        "tags": [],
        "sections": [],
        "tasks": [],
        "links": [
            {"target": "existing.md", "path": "existing.md", "kind": "wiki"}
        ],
        "modified": "2026-01-01T00:00:00Z"
    });
    let data = json!([item]);
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("links fix")),
        "find results without broken links should not suggest links fix: {hints:?}"
    );
}

// --- hints_for_lint ---

#[test]
fn lint_hints_name_hidden_errors_when_truncated() {
    // UX-4 (dogfood v0.20.0): the truncated `files[]` slice shows only
    // warnings, but the authoritative count is `errors: 4` — the
    // show-all hint must say the errors are hidden, or the summary line
    // reads like a bug.
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{
            "file": "warn-only.md",
            "rule_groups": [{
                "rule": "MD040",
                "severity": "warn",
                "count": 1,
                "shown": 1,
                "violations": [{"severity": "warn", "message": "fenced code block should have a language"}]
            }]
        }],
        "files_truncated": true,
        "files_with_violations": 60,
        "errors": 4,
        "warnings": 7716
    });
    let hints = generate_hints(&c, &data, None);
    let show_all = hints
        .iter()
        .find(|h| h.cmd.contains("--limit 0"))
        .expect("show-all hint should be present");
    assert!(
        show_all.description.contains("4 errors hidden"),
        "hint should name the hidden errors: {show_all:?}"
    );
}

#[test]
fn lint_hints_no_hidden_errors_claim_in_fix_mode_dry_run() {
    // Review M-1 (PR #277): fix-mode shapes groups as
    // `fixed_groups`/`remaining_groups`, so the read-only
    // `rule_groups` computation must not claim hidden errors there —
    // even though the truncation hint itself is kept in dry-run mode.
    let mut c = ctx(HintSource::Lint);
    c.dry_run = true;
    c.lint_is_fix = true;
    let data = json!({
        "files": [{
            "file": "err.md",
            "remaining_groups": [{
                "rule": "MD011",
                "severity": "error",
                "count": 1,
                "violations": [{"severity": "error", "message": "Reversed link syntax: (a)[b]"}]
            }]
        }],
        "files_truncated": true,
        "files_with_violations": 60,
        "errors": 4,
        "warnings": 7716,
        "dry_run": true,
        "total_fixed": 0,
        "total_remaining": 4
    });
    let hints = generate_hints(&c, &data, None);
    let show_all = hints
        .iter()
        .find(|h| h.cmd.contains("--limit 0"))
        .expect("show-all hint should still be present in dry-run mode");
    assert!(
        !show_all.description.contains("hidden"),
        "fix-mode dry-run must not claim hidden errors: {show_all:?}"
    );
}

#[test]
fn lint_hints_suggest_fix_when_violations() {
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{"file": "test.md", "violations": [{"severity": "error", "message": "missing required property"}]}],
        "violations": 1,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(!hints.is_empty());
    assert!(
        hints.iter().any(|h| h.cmd.contains("lint --fix")),
        "should suggest lint --fix: {hints:?}"
    );
}

#[test]
fn lint_hints_suggest_apply_when_dry_run() {
    let mut c = ctx(HintSource::Lint);
    c.dry_run = true;
    c.lint_is_fix = true; // --dry-run requires --fix per CLI spec
    let data = json!({
        "files": [],
        "violations": 0,
        "total_fixed": 3,
        "total_remaining": 0,
        "fixes": [{"file": "test.md", "actions": [{"kind": "insert-default", "property": "status", "new": "draft"}]}],
        "dry_run": true,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("lint --fix") && !h.cmd.contains("--dry-run")),
        "dry-run mode should suggest lint --fix without --dry-run: {hints:?}"
    );
}

#[test]
fn lint_hints_always_suggest_types_list() {
    let c = ctx(HintSource::Lint);
    let data = json!({"files": [], "violations": 0});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("types list")),
        "should always suggest types list: {hints:?}"
    );
}

#[test]
fn lint_hints_never_exceed_max() {
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{"file": "test.md", "violations": [{"severity": "error", "message": "x", "type": "iteration"}]}],
        "violations": 5,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(hints.len() <= MAX_HINTS);
}

// --- hints_for_types ---

#[test]
fn types_list_hints_suggest_show() {
    let c = ctx(HintSource::Types {
        subcommand: Some("list".to_owned()),
    });
    let data = json!([
        {"type": "iteration", "required": ["title"], "has_filename_template": true, "property_count": 3},
        {"type": "note", "required": [], "has_filename_template": false, "property_count": 1},
    ]);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("types show")),
        "should suggest types show: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("lint")),
        "should suggest lint: {hints:?}"
    );
}

#[test]
fn types_show_hints_suggest_lint_and_find() {
    let c = ctx(HintSource::Types {
        subcommand: Some("show".to_owned()),
    });
    let data = json!({"type": "iteration", "required": ["title"], "properties": {}});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("lint")),
        "should suggest lint: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("find --property")),
        "should suggest find --property: {hints:?}"
    );
}

#[test]
fn types_show_hints_suggest_scaffold_when_required_nonempty() {
    // iter-143: when the type declares any `required` properties, `types
    // show` surfaces a hint to scaffold a new file via `hyalo new`.
    let c = ctx(HintSource::Types {
        subcommand: Some("show".to_owned()),
    });
    let data = json!({
        "type": "iteration",
        "required": ["title", "status"],
        "properties": {},
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("new --type iteration")),
        "should suggest scaffolding a new file: {hints:?}"
    );
}

#[test]
fn types_show_hints_no_scaffold_when_required_empty() {
    // iter-143: when `required` is empty, the scaffold hint is dropped
    // (it would only emit a `type:` stub — low value).
    let c = ctx(HintSource::Types {
        subcommand: Some("show".to_owned()),
    });
    let data = json!({
        "type": "note",
        "required": [],
        "properties": {},
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("new --type")),
        "should NOT suggest scaffolding when required is empty: {hints:?}"
    );
}

#[test]
fn lint_hints_schema_violation_suggests_types_show() {
    // iter-143: when SCHEMA violations land on a typed file, surface
    // `hyalo types show <T>`.
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{
            "file": "foo.md",
            "type": "iteration",
            "rule_groups": [{
                "rule": "SCHEMA", "count": 2, "shown": 2,
                "truncated": false, "severity": "error", "autofixable": false,
                "violations": [],
            }]
        }],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("types show iteration")),
        "should suggest types show for the failing type: {hints:?}"
    );
}

#[test]
fn lint_hints_schema_violation_suggests_types_show_in_fix_mode() {
    // iter-143 follow-up (Copilot review on PR #169): the SCHEMA→`types
    // show` hint must also fire in `--fix` / `--fix --dry-run` output,
    // where violations live under `remaining_groups` instead of
    // `rule_groups`.
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{
            "file": "foo.md",
            "type": "iteration",
            "fixed_groups": [],
            "remaining_groups": [{
                "rule": "SCHEMA", "count": 1, "shown": 1,
                "truncated": false, "severity": "error", "autofixable": false,
                "violations": [],
            }],
            "conflicts": [],
        }],
        "dry_run": true,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("types show iteration")),
        "should suggest types show in fix-mode too: {hints:?}"
    );
}

#[test]
fn lint_hints_schema_violation_skipped_when_already_focused() {
    // iter-143: when the user is already filtering on SCHEMA via --rule
    // SCHEMA (or --rule-prefix HYALO), the `types show` hint would be
    // redundant — suppress it.
    let mut c = ctx(HintSource::Lint);
    c.lint_rule = Some("SCHEMA".to_owned());
    let data = json!({
        "files": [{
            "file": "foo.md",
            "type": "iteration",
            "rule_groups": [{
                "rule": "SCHEMA", "count": 1, "shown": 1,
                "truncated": false, "severity": "error", "autofixable": false,
                "violations": [],
            }]
        }],
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("types show")),
        "should NOT suggest types show when --rule SCHEMA: {hints:?}"
    );
}

#[test]
fn files_from_hints_fire_on_missing_and_outside_vault() {
    // iter-143: the FilesFromCounterSummary path produces advice hints.
    let c = ctx(HintSource::Find);
    let data = json!({"results": [], "total": 0});
    let counters = FilesFromCounterSummary {
        files_missing: 3,
        files_skipped_outside_vault: 1,
    };
    let hints = generate_hints_with_counters(&c, &data, None, Some(counters));
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.is_empty() && h.description.contains("3 input path")),
        "should warn about missing inputs: {hints:?}"
    );
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.is_empty() && h.description.contains("outside the vault")),
        "should warn about outside-vault inputs: {hints:?}"
    );
}

#[test]
fn files_from_hints_silent_when_zero_counters() {
    let c = ctx(HintSource::Find);
    let data = json!({"results": []});
    let counters = FilesFromCounterSummary::default();
    let hints = generate_hints_with_counters(&c, &data, None, Some(counters));
    assert!(
        !hints.iter().any(|h| h.cmd.is_empty()),
        "no advice hints expected when counters are zero: {hints:?}"
    );
}

#[test]
fn types_set_hints_suggest_show_and_lint() {
    let c = ctx(HintSource::Types {
        subcommand: Some("set".to_owned()),
    });
    let data = json!({"type": "iteration", "action": "updated"});
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("types show iteration")),
        "should suggest types show for updated type: {hints:?}"
    );
    assert!(
        hints.iter().any(|h| h.cmd.contains("lint")),
        "should suggest lint: {hints:?}"
    );
}

// --- UX-1: per-rule hints for HYALO001 / HYALO002 ---

#[test]
fn lint_hints_hyalo001_suggests_fix_rule() {
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{
            "file": "test.md",
            "rule_groups": [{"rule": "HYALO001", "count": 3, "shown": 3, "truncated": false,
                             "severity": "error", "autofixable": true,
                             "violations": [{"line": 4, "column": 1, "message": "bare []"}]}]
        }],
        "total": 3,
        "rules_fired": 1,
        "files_with_violations": 1,
        "files_checked": 1,
        "files_truncated": false,
        "errors": 3,
        "warnings": 0,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("HYALO001") && h.cmd.contains("--fix")),
        "should suggest lint --rule HYALO001 --fix for HYALO001 violations: {hints:?}"
    );
}

#[test]
fn lint_hints_hyalo002_suggests_find_todo() {
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{
            "file": "iterations/iter-1.md",
            "rule_groups": [{"rule": "HYALO002", "count": 5, "shown": 3, "truncated": true,
                             "severity": "error", "autofixable": false,
                             "violations": [{"line": 21, "column": 1, "message": "completed but tasks remain"}]}]
        }],
        "total": 5,
        "rules_fired": 1,
        "files_with_violations": 1,
        "files_checked": 1,
        "files_truncated": false,
        "errors": 5,
        "warnings": 0,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("find --task todo") && h.cmd.contains("iter-1")),
        "should suggest find --task todo with worst-offender file: {hints:?}"
    );
}

// --- UX-2: rule dominance hint ---

#[test]
fn lint_hints_dominant_rule_suggests_tune() {
    let c = ctx(HintSource::Lint);
    // MD013 has 80 of 100 total violations → 80% share, ≥50 absolute.
    let mut groups: Vec<serde_json::Value> = Vec::new();
    for _ in 0..80 {
        groups.push(json!({"line": 1, "column": 1, "message": "line too long"}));
    }
    let data = json!({
        "files": [
            {"file": "a.md", "rule_groups": [
                {"rule": "MD013", "count": 80, "shown": 3, "truncated": true,
                 "severity": "warn", "autofixable": false,
                 "violations": [{"line": 1, "column": 1, "message": "line too long"}]}
            ]},
            {"file": "b.md", "rule_groups": [
                {"rule": "HYALO001", "count": 20, "shown": 3, "truncated": true,
                 "severity": "error", "autofixable": true,
                 "violations": [{"line": 2, "column": 1, "message": "bare []"}]}
            ]}
        ],
        "total": 100,
        "rules_fired": 2,
        "files_with_violations": 2,
        "files_checked": 5,
        "files_truncated": false,
        "errors": 20,
        "warnings": 80,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.contains("lint-rules show MD013")),
        "should suggest lint-rules show for dominant rule (80%): {hints:?}"
    );
}

#[test]
fn lint_hints_no_dominance_when_below_threshold() {
    let c = ctx(HintSource::Lint);
    // MD013 has 30 of 60 total (50% but only 30 absolute < 50 min).
    let data = json!({
        "files": [
            {"file": "a.md", "rule_groups": [
                {"rule": "MD013", "count": 30, "shown": 3, "truncated": true,
                 "severity": "warn", "autofixable": false,
                 "violations": [{"line": 1, "column": 1, "message": "line too long"}]}
            ]},
            {"file": "b.md", "rule_groups": [
                {"rule": "HYALO001", "count": 30, "shown": 3, "truncated": true,
                 "severity": "error", "autofixable": true,
                 "violations": [{"line": 2, "column": 1, "message": "bare []"}]}
            ]}
        ],
        "total": 60,
        "rules_fired": 2,
        "files_with_violations": 2,
        "files_checked": 5,
        "files_truncated": false,
        "errors": 30,
        "warnings": 30,
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.cmd.contains("lint-rules show")),
        "should not suggest lint-rules show when below dominance threshold: {hints:?}"
    );
}

// --- UX-7: smart fix/dry-run hints ---

#[test]
fn lint_hints_not_fix_mode_suggests_preview_not_apply() {
    // When not in fix mode, only preview should be suggested, not apply.
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files": [{"file": "test.md", "rule_groups": [
            {"rule": "MD009", "count": 2, "shown": 2, "truncated": false,
             "severity": "warn", "autofixable": true,
             "violations": [{"line": 3, "column": 10, "message": "trailing spaces"}]}
        ]}],
        "total": 2,
        "rules_fired": 1,
        "files_with_violations": 1,
        "files_checked": 1,
        "files_truncated": false,
        "errors": 0,
        "warnings": 2,
    });
    let hints = generate_hints(&c, &data, None);
    // Should have preview hint.
    assert!(
        hints.iter().any(|h| h.cmd.contains("--fix --dry-run")),
        "non-fix mode should suggest preview: {hints:?}"
    );
    // Should NOT suggest direct apply (user should preview first).
    assert!(
        !hints
            .iter()
            .any(|h| h.cmd.contains("lint --fix") && !h.cmd.contains("--dry-run")),
        "non-fix mode should NOT suggest apply directly: {hints:?}"
    );
}

#[test]
fn lint_hints_fix_mode_applied_no_fix_hints() {
    // When fix was applied (not dry-run), no fix hints.
    let mut c = ctx(HintSource::Lint);
    c.lint_is_fix = true;
    // dry_run defaults to false
    let data = json!({
        "files": [],
        "total_fixed": 3,
        "total_remaining": 0,
        "total_conflicts": 0,
        "rules_fired": 1,
        "files_with_violations": 0,
        "files_checked": 3,
        "files_truncated": false,
        "remaining_errors": 0,
        "remaining_warnings": 0,
        "dry_run": false,
    });
    let hints = generate_hints(&c, &data, None);
    // Should NOT suggest any lint --fix hints since we already applied.
    assert!(
        !hints.iter().any(|h| h.cmd.contains("lint --fix")),
        "after applying fixes, should not suggest fix again: {hints:?}"
    );
}

// --- slow_query_hint ---

fn data_empty_array() -> serde_json::Value {
    json!([])
}

/// Slow find with no index should emit the slow-query hint.
#[test]
fn slow_query_hint_fires_for_slow_find() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 1);
    let h = slow_query_hint(&c);
    assert!(h.is_some(), "expected slow-query hint");
    let h = h.unwrap();
    assert!(h.cmd == "hyalo create-index", "cmd: {}", h.cmd);
    assert!(h.description.contains("ms"), "desc: {}", h.description);
}

/// Exactly at the threshold (not strictly greater) should NOT fire.
#[test]
fn slow_query_hint_does_not_fire_at_threshold() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS);
    assert!(slow_query_hint(&c).is_none());
}

/// Fast query should not emit the hint.
#[test]
fn slow_query_hint_does_not_fire_when_fast() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(50);
    assert!(slow_query_hint(&c).is_none());
}

/// `--quiet` suppresses the slow-query hint even when slow.
#[test]
fn slow_query_hint_suppressed_by_quiet() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 100);
    c.quiet = true;
    assert!(slow_query_hint(&c).is_none());
}

/// Active index suppresses the slow-query hint even when slow.
#[test]
fn slow_query_hint_suppressed_when_has_index() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 100);
    c.has_index = true;
    assert!(slow_query_hint(&c).is_none());
}

/// Missing elapsed (`None`) means not yet measured — no hint.
#[test]
fn slow_query_hint_not_emitted_when_elapsed_none() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = None;
    assert!(slow_query_hint(&c).is_none());
}

/// Ineligible source (e.g. Set) never emits slow-query hint.
#[test]
fn slow_query_hint_not_emitted_for_ineligible_source() {
    let mut c = ctx(HintSource::Set);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 100);
    assert!(slow_query_hint(&c).is_none());
}

/// All eligible sources should emit the hint when slow and no index.
#[test]
fn slow_query_hint_fires_for_all_eligible_sources() {
    for source in [
        HintSource::Find,
        HintSource::Lint,
        HintSource::Backlinks,
        HintSource::PropertiesSummary,
        HintSource::TagsSummary,
        HintSource::Summary,
        HintSource::Read,
    ] {
        let mut c = ctx(source);
        c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 1);
        assert!(
            slow_query_hint(&c).is_some(),
            "expected slow-query hint for source"
        );
    }
}

/// Slow-query hint appears in generate_hints output (via generate_hints_with_counters).
#[test]
fn slow_query_hint_surfaces_through_generate_hints() {
    let mut c = ctx(HintSource::Find);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 1);
    let hints = generate_hints(&c, &data_empty_array(), Some(0));
    assert!(
        hints.iter().any(|h| h.cmd == "hyalo create-index"),
        "expected create-index hint: {hints:?}"
    );
}

// --- large-vault summary hint ---

fn summary_data(files_total: u64) -> serde_json::Value {
    json!({
        "files": {"total": files_total, "by_directory": []},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [],
        "tasks": {"total": 0, "done": 0},
        "recent_files": []
    })
}

/// Large vault (above threshold) with no index should emit the large-vault hint.
#[test]
fn large_vault_summary_hint_fires_when_over_threshold() {
    let c = ctx(HintSource::Summary);
    let data = summary_data(LARGE_VAULT_FILE_COUNT + 1);
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd == "hyalo create-index" && h.description.contains("files")),
        "expected large-vault hint: {hints:?}"
    );
}

/// Exactly at the threshold (not strictly greater) should NOT fire.
#[test]
fn large_vault_summary_hint_does_not_fire_at_threshold() {
    let c = ctx(HintSource::Summary);
    let data = summary_data(LARGE_VAULT_FILE_COUNT);
    let hints = generate_hints(&c, &data, None);
    // The hint should not appear.
    assert!(
        !hints
            .iter()
            .any(|h| h.cmd == "hyalo create-index" && h.description.contains("files")),
        "unexpected large-vault hint at threshold: {hints:?}"
    );
}

/// Small vault should not emit the large-vault hint.
#[test]
fn large_vault_summary_hint_not_fired_for_small_vault() {
    let c = ctx(HintSource::Summary);
    let data = summary_data(10);
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints
            .iter()
            .any(|h| h.cmd == "hyalo create-index" && h.description.contains("files")),
        "unexpected large-vault hint for small vault: {hints:?}"
    );
}

/// Active index suppresses the large-vault hint.
#[test]
fn large_vault_summary_hint_suppressed_when_has_index() {
    let mut c = ctx(HintSource::Summary);
    c.has_index = true;
    let data = summary_data(LARGE_VAULT_FILE_COUNT + 100);
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints
            .iter()
            .any(|h| h.cmd == "hyalo create-index" && h.description.contains("files")),
        "large-vault hint should be suppressed with active index: {hints:?}"
    );
}

/// `--quiet` suppresses the large-vault hint (parity with slow-query hint).
#[test]
fn large_vault_summary_hint_suppressed_by_quiet() {
    let mut c = ctx(HintSource::Summary);
    c.quiet = true;
    let data = summary_data(LARGE_VAULT_FILE_COUNT + 100);
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints
            .iter()
            .any(|h| h.cmd == "hyalo create-index" && h.description.contains("files")),
        "large-vault hint should be suppressed by --quiet: {hints:?}"
    );
}

/// When both slow-query and large-vault conditions fire, only one
/// `create-index` hint should appear in the envelope (dedupe by `cmd`).
#[test]
fn create_index_hint_deduped_when_both_conditions_fire() {
    let mut c = ctx(HintSource::Summary);
    c.elapsed_ms = Some(SLOW_QUERY_THRESHOLD_MS + 1);
    let data = summary_data(LARGE_VAULT_FILE_COUNT + 100);
    let hints = generate_hints(&c, &data, None);
    let n = hints
        .iter()
        .filter(|h| h.cmd == "hyalo create-index")
        .count();
    assert_eq!(
        n, 1,
        "expected exactly one create-index hint, got {n}: {hints:?}"
    );
}

/// NEW-1 regression: create-index hint must appear even when orphans +
/// broken-links + links-fix hints would otherwise consume all MAX_HINTS
/// slots. On real large vaults (MDN: 4245 orphans, 49933 broken links)
/// the health hints used to crowd out the index hint entirely.
#[test]
fn create_index_hint_visible_even_when_health_hints_fill_cap() {
    let c = ctx(HintSource::Summary);
    // A large vault with both orphans and broken links to generate many hints.
    let data = json!({
        "files": {"total": LARGE_VAULT_FILE_COUNT + 14000, "by_directory": []},
        "orphans": 4245u64,
        "dead_ends": 100u64,
        "links": {"total": 100_000u64, "broken": 49_933u64},
        "properties": [],
        "tags": {"tags": [], "total": 0},
        "status": [],
        "tasks": {"total": 0, "done": 0},
        "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    // Total hints must not exceed MAX_HINTS.
    assert!(
        hints.len() <= MAX_HINTS,
        "hints exceeded MAX_HINTS={MAX_HINTS}: {hints:?}"
    );
    // The create-index hint must be present despite the health-hint pressure.
    assert!(
        hints.iter().any(|h| h.cmd == "hyalo create-index"),
        "create-index hint missing; hints: {hints:?}"
    );
    // create-index should be the first hint (highest priority).
    assert_eq!(
        hints[0].cmd, "hyalo create-index",
        "create-index should be first hint; got: {hints:?}"
    );
}

// --- okf hints (iter-177) ---

#[test]
fn okf_index_dry_run_drift_suggests_apply_then_validate() {
    let c = ctx(HintSource::OkfIndex);
    let data = json!({ "apply": false, "changed": 2 });
    let hints = generate_hints(&c, &data, None);
    assert_eq!(hints[0].cmd, "hyalo okf index --apply");
    assert_eq!(hints[1].cmd, "hyalo lint --profile okf");
}

#[test]
fn okf_index_clean_dry_run_only_validate() {
    let c = ctx(HintSource::OkfIndex);
    let data = json!({ "apply": false, "changed": 0 });
    let hints = generate_hints(&c, &data, None);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].cmd, "hyalo lint --profile okf");
}

#[test]
fn okf_index_apply_omits_apply_hint() {
    let c = ctx(HintSource::OkfIndex);
    // Even with changes, an --apply run should not re-suggest applying.
    let data = json!({ "apply": true, "changed": 3 });
    let hints = generate_hints(&c, &data, None);
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].cmd, "hyalo lint --profile okf");
}

#[test]
fn okf_validate_hint_drops_redundant_profile_flag_when_active() {
    let mut c = ctx(HintSource::OkfLog);
    c.okf_profile_active = true;
    let hints = generate_hints(&c, &json!({}), None);
    assert_eq!(hints[0].cmd, "hyalo lint");
}

// --- iter-180: hint trust ---

#[test]
fn slow_query_create_index_hint_carries_dir() {
    // BUG-7: the slow-query create-index hint dropped an explicit --dir.
    let mut c = ctx_with_dir(HintSource::Find, "/my/vault");
    c.elapsed_ms = Some(1062);
    let items = vec![make_find_item("a.md", None, &[])];
    let hints = generate_hints(&c, &json!(items), None);
    let ci = hints
        .iter()
        .find(|h| h.cmd.starts_with("hyalo create-index"))
        .expect("slow-query create-index hint expected");
    assert_eq!(ci.cmd, "hyalo create-index --dir /my/vault");
    // BUG-7: no dangling colon on the description.
    assert!(
        !ci.description.ends_with(':'),
        "description should not end with a colon: {:?}",
        ci.description
    );
}

#[test]
fn summary_large_vault_create_index_hint_carries_dir() {
    let c = ctx_with_dir(HintSource::Summary, "/vault");
    let data = json!({
        "files": {"total": 501, "by_directory": []},
        "properties": [], "tags": {"tags": [], "total": 0},
        "status": [], "tasks": {"total": 0, "done": 0}, "recent_files": []
    });
    let hints = generate_hints(&c, &data, None);
    let ci = hints
        .iter()
        .find(|h| h.cmd.starts_with("hyalo create-index"))
        .expect("large-vault create-index hint expected");
    assert_eq!(ci.cmd, "hyalo create-index --dir /vault");
    assert!(!ci.description.ends_with(':'));
}

#[test]
fn find_orphan_show_all_preserves_orphan_filter() {
    // BUG-8: "Show all N results" on a --orphan query must keep --orphan.
    let mut c = ctx(HintSource::Find);
    c.orphan_filter = true;
    let items: Vec<serde_json::Value> = (0..50)
        .map(|i| make_find_item(&format!("{i}.md"), None, &[]))
        .collect();
    // total (79) > shown (50) → truncated, so the show-all hint fires.
    let hints = generate_hints(&c, &json!(items), Some(79));
    let show_all = hints
        .iter()
        .find(|h| h.description.starts_with("Show all"))
        .expect("show-all hint expected");
    assert!(
        show_all.cmd.contains("--orphan"),
        "show-all hint must preserve --orphan: {}",
        show_all.cmd
    );
    assert!(show_all.cmd.contains("--limit 0"));
}

#[test]
fn find_orphan_narrow_by_tag_composes_and_drops_stale_count() {
    // BUG-8: the narrow-by-tag hint on a --orphan query must keep --orphan
    // and, when the result set was truncated, drop the (misleading) count.
    let mut c = ctx(HintSource::Find);
    c.orphan_filter = true;
    let items: Vec<serde_json::Value> = (0..50)
        .map(|i| make_find_item(&format!("{i}.md"), None, &["iteration"]))
        .collect();
    let hints = generate_hints(&c, &json!(items), Some(79));
    let narrow = hints
        .iter()
        .find(|h| h.description.starts_with("Narrow by tag"))
        .expect("narrow-by-tag hint expected");
    assert!(
        narrow.cmd.contains("--orphan") && narrow.cmd.contains("--tag iteration"),
        "narrow hint must compose --orphan with the new tag: {}",
        narrow.cmd
    );
    // Truncated set → no parenthetical count in the description.
    assert!(
        !narrow.description.contains('('),
        "count must be dropped when truncated: {:?}",
        narrow.description
    );
}

#[test]
fn find_narrow_by_tag_keeps_count_when_not_truncated() {
    let c = ctx(HintSource::Find);
    let items: Vec<serde_json::Value> = (0..6)
        .map(|i| make_find_item(&format!("{i}.md"), None, &["iteration"]))
        .collect();
    // total == shown → not truncated → count is accurate and shown.
    let hints = generate_hints(&c, &json!(items), Some(6));
    let narrow = hints
        .iter()
        .find(|h| h.description.starts_with("Narrow by tag"))
        .expect("narrow-by-tag hint expected");
    assert!(
        narrow.description.contains("(6 files)"),
        "count should be shown when not truncated: {:?}",
        narrow.description
    );
}

/// iter-210 / UX-5: the snapshot path is the longest token a hint carries
/// and it is repeated on every derived query. Inside the working directory
/// it renders relative — shorter, and still runnable verbatim.
#[test]
fn index_path_renders_relative_to_the_working_directory() {
    let cwd = std::env::current_dir().expect("cwd");
    let inside = cwd.join("sub").join(".hyalo-index");
    assert_eq!(shorten_index_path_for_hint(&inside), "sub/.hyalo-index");
}

/// A path outside the working directory has no shorter runnable spelling,
/// so it stays absolute rather than turning into a `../..` chain.
#[test]
fn index_path_outside_the_working_directory_stays_absolute() {
    let outside = std::path::Path::new(if cfg!(windows) {
        r"C:\definitely\elsewhere\.hyalo-index"
    } else {
        "/definitely/elsewhere/.hyalo-index"
    });
    assert_eq!(
        shorten_index_path_for_hint(outside),
        outside.display().to_string()
    );
}

#[test]
fn find_index_file_preserved_in_derived_hints() {
    // BUG-7 audit: --index-file was a dropped flag.
    let mut c = ctx(HintSource::Find);
    c.find_index = FindIndexHint::File("sub/.hyalo-index".to_owned());
    let items: Vec<serde_json::Value> = (0..50)
        .map(|i| make_find_item(&format!("{i}.md"), None, &[]))
        .collect();
    let hints = generate_hints(&c, &json!(items), Some(200));
    let show_all = hints
        .iter()
        .find(|h| h.description.starts_with("Show all"))
        .expect("show-all hint expected");
    assert!(
        show_all.cmd.contains("--index-file sub/.hyalo-index"),
        "derived hint must preserve --index-file: {}",
        show_all.cmd
    );
}

#[test]
fn summary_schema_hint_relabeled_and_targets_schema_rule() {
    // BUG-9: relabel "Lint" → "Schema" and target `lint --rule SCHEMA`.
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 10, "by_directory": []},
        "properties": [], "tags": {"tags": [], "total": 0},
        "status": [], "tasks": {"total": 0, "done": 0}, "recent_files": [],
        "schema": {"errors": 5, "warnings": 12, "files_with_violations": 8}
    });
    let hints = generate_hints(&c, &data, None);
    let schema_hint = hints
        .iter()
        .find(|h| h.description.starts_with("Schema:"))
        .expect("schema hint expected");
    assert_eq!(schema_hint.description, "Schema: 5 errors, 12 warnings");
    assert_eq!(schema_hint.cmd, "hyalo lint --rule SCHEMA");
    assert!(
        !hints.iter().any(|h| h.description.starts_with("Lint:")),
        "should not use the old \"Lint:\" label: {hints:?}"
    );
}

#[test]
fn lint_fix_apply_suppresses_stale_show_all_hint() {
    let mut c = ctx(HintSource::Lint);
    c.lint_is_fix = true; // apply (not dry-run)
    let data = json!({
        "files_truncated": true,
        "files_with_issues": 42,
        "files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        !hints.iter().any(|h| h.description.contains("Show all")),
        "post-apply output must not carry the stale show-all hint: {hints:?}"
    );
}

#[test]
fn lint_readonly_keeps_show_all_hint() {
    let c = ctx(HintSource::Lint);
    let data = json!({
        "files_truncated": true,
        "files_with_issues": 42,
        "files": []
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.description.contains("Show all")),
        "read-only lint should keep the show-all hint: {hints:?}"
    );
}

#[test]
fn summary_site_url_heuristic_replaces_links_fix() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 14000, "by_directory": []},
        "properties": [], "tags": {"tags": [], "total": 0},
        "status": [], "tasks": {"total": 0, "done": 0}, "recent_files": [],
        "orphans": 0, "dead_ends": 0,
        "links": {"total": 49935, "broken": 49933}
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints
            .iter()
            .any(|h| h.cmd.is_empty() && h.description.contains("--site-prefix")),
        "site-URL diagnostic expected: {hints:?}"
    );
    assert!(
        !hints.iter().any(|h| h.cmd.contains("links fix")),
        "links fix should be suppressed when links look like site URLs: {hints:?}"
    );
}

#[test]
fn summary_normal_broken_links_still_offers_links_fix() {
    let c = ctx(HintSource::Summary);
    let data = json!({
        "files": {"total": 100, "by_directory": []},
        "properties": [], "tags": {"tags": [], "total": 0},
        "status": [], "tasks": {"total": 0, "done": 0}, "recent_files": [],
        "orphans": 0, "dead_ends": 0,
        "links": {"total": 400, "broken": 12}
    });
    let hints = generate_hints(&c, &data, None);
    assert!(
        hints.iter().any(|h| h.cmd.contains("links fix")),
        "a handful of broken links should still offer links fix: {hints:?}"
    );
}

#[test]
fn find_show_all_and_narrow_hints_preserve_active_sort() {
    // Milder variant of BUG-8: a truncated, explicitly sorted query's
    // derived hints (show-all, narrow-by-tag) must keep --sort/--reverse,
    // else "show all" / "narrow by tag" silently reverts to default
    // ordering instead of reproducing the query that produced them.
    let mut c = ctx(HintSource::Find);
    c.sort = Some("modified".to_owned());
    c.reverse = true;
    let items: Vec<serde_json::Value> = (0..50)
        .map(|i| make_find_item(&format!("{i}.md"), None, &["iteration"]))
        .collect();
    let hints = generate_hints(&c, &json!(items), Some(79));

    let show_all = hints
        .iter()
        .find(|h| h.description.starts_with("Show all"))
        .expect("show-all hint expected");
    assert!(
        show_all.cmd.contains("--sort modified") && show_all.cmd.contains("--reverse"),
        "show-all hint must preserve --sort/--reverse: {}",
        show_all.cmd
    );

    let narrow = hints
        .iter()
        .find(|h| h.description.starts_with("Narrow by tag"))
        .expect("narrow-by-tag hint expected");
    assert!(
        narrow.cmd.contains("--sort modified") && narrow.cmd.contains("--reverse"),
        "narrow-by-tag hint must preserve --sort/--reverse: {}",
        narrow.cmd
    );

    // Because ctx.sort is Some, the literal "Sort by most recently
    // modified" suggestion must not also fire (it only applies when no
    // sort is active).
    assert!(
        !hints
            .iter()
            .any(|h| h.description == "Sort by most recently modified"),
        "should not suggest sorting when a sort is already active: {hints:?}"
    );
}
