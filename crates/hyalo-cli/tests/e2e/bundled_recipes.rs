//! Iteration 254 (FIND-2) — every `--jq` recipe in a bundled skill, template or
//! rule file asks for the fields it reads.
//!
//! `hyalo find --property status=planned --jq '… .tasks …'` shipped in
//! `skill-hyalo-tidy.md` for two iterations. Since iteration 252 took `tasks`
//! out of the default field set it returned the empty list for every vault:
//! not an error, not a warning — just a silently wrong answer, in a recipe an
//! agent is told to trust. The next shape change must not be able to leave one
//! behind, so the bundled files are checked mechanically.

use std::path::{Path, PathBuf};

/// Fields a `find` result carries only on request.
const OPT_IN_FIELDS: &[&str] = &[
    "tasks",
    "sections",
    "links",
    "backlinks",
    "properties_typed",
];

/// Flags that make an opt-in field appear: an explicit projection, a saved view
/// that may pin one, or a filter whose own semantics auto-include it.
const SATISFYING_FLAGS: &[&str] = &[
    "--fields",
    "--view",
    "--section",
    "--task",
    "--broken-links",
    "--orphan",
    "--dead-end",
];

/// Every bundled markdown file `init` can write into a user's vault.
fn bundled_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("templates dir is readable") {
            let path = entry.expect("readable dir entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    assert!(
        out.len() >= 5,
        "expected the bundled skill/rule templates, found {out:?}"
    );
    out
}

/// Join backslash-continued shell lines so a command split across lines is
/// checked as the single command line it is.
fn logical_lines(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut pending = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(head) = trimmed.strip_suffix('\\') {
            pending.push_str(head.trim_end());
            pending.push(' ');
        } else {
            pending.push_str(trimmed);
            out.push(std::mem::take(&mut pending));
        }
    }
    if !pending.is_empty() {
        out.push(pending);
    }
    out
}

/// The opt-in fields a `hyalo find … --jq …` line reads but never asks for.
fn unrequested_fields(line: &str) -> Vec<&'static str> {
    if !line.contains("hyalo find") || !line.contains("--jq") {
        return Vec::new();
    }
    if SATISFYING_FLAGS.iter().any(|f| line.contains(f)) {
        return Vec::new();
    }
    OPT_IN_FIELDS
        .iter()
        .copied()
        .filter(|f| line.contains(&format!(".{f}")))
        .collect()
}

#[test]
fn bundled_jq_recipes_request_the_fields_they_read() {
    let mut failures: Vec<String> = Vec::new();
    for path in bundled_files() {
        let content = std::fs::read_to_string(&path).expect("bundled file is readable");
        for line in logical_lines(&content) {
            let missing = unrequested_fields(&line);
            if !missing.is_empty() {
                failures.push(format!(
                    "{}: reads {missing:?} but names no --fields/--view/auto-including filter:\n    {line}",
                    path.display()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "bundled --jq recipes that would silently return nothing:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_recipe_guard_itself_catches_the_shape_iteration_252_broke() {
    // Mutation test: the exact line that shipped wrong, and the fixed one.
    let broken = "hyalo find --property status=planned --index --jq '.results | map(select(.tasks | length > 0))'";
    assert_eq!(unrequested_fields(broken), vec!["tasks"]);

    let fixed = "hyalo find --property status=planned --fields tasks --index --jq '.results | map(select(.tasks | length > 0))'";
    assert!(unrequested_fields(fixed).is_empty());

    // A filter that auto-includes the field is enough on its own.
    let auto = "hyalo find --task todo --jq '.results | map(.tasks)'";
    assert!(unrequested_fields(auto).is_empty());

    // A `summary` recipe reading `.results.tasks.total` is a different payload.
    let not_find = "hyalo summary --jq '.results.tasks.total'";
    assert!(unrequested_fields(not_find).is_empty());

    // A continuation line must be judged as part of its command.
    let split = logical_lines(
        "hyalo find --fields tasks \\\n  --jq '.results | map({file, n: (.tasks | length)})'\n",
    );
    assert_eq!(split.len(), 1, "{split:?}");
    assert!(unrequested_fields(&split[0]).is_empty());
}
