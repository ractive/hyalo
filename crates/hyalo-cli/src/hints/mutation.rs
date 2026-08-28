//! Hints for the write commands and their read counterparts: `set`/`remove`/`append`, `read`, `backlinks`, `mv`, `task`.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{
    Hint, HintContext, MAX_HINTS, build_command_no_glob, build_command_with_file,
    first_modified_file,
};

pub(super) fn hints_for_mutation(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let first_modified = first_modified_file(data);

    if let Some(file) = first_modified {
        hints.push(Hint::new(
            "Verify the updated file",
            build_command_no_glob(
                ctx,
                &["find", "--file", file, "--fields", "properties,tags"],
            ),
        ));
        hints.push(Hint::new(
            "Read the modified file",
            build_command_no_glob(ctx, &["read", file]),
        ));
    }

    hints
}

pub(super) fn hints_for_read(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let file = data
        .get("file")
        .and_then(|f| f.as_str())
        .or_else(|| ctx.file_targets.first().map(String::as_str));

    if let Some(file) = file {
        hints.push(Hint::new(
            "See metadata for this file",
            build_command_no_glob(ctx, &["find", "--file", file, "--fields", "all"]),
        ));
        hints.push(Hint::new(
            "See what links to this file",
            build_command_with_file(ctx, &["backlinks"], file, &[]),
        ));
        // UX-4 (dogfood pre3): `read --format json` without `--frontmatter`
        // silently omits the `frontmatter` key entirely — `--jq
        // '.results.frontmatter.x'` reads that as `null` indistinguishably
        // from "the property doesn't exist," costing a round trip to
        // discover the flag was the actual gap. Only worth saying when the
        // key really is missing (a caller who already asked for it needs no
        // reminder).
        if data.get("frontmatter").is_none() && hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                "Include frontmatter in the output",
                build_command_with_file(ctx, &["read"], file, &["--frontmatter"]),
            ));
        }
    }

    hints
}

pub(super) fn hints_for_backlinks(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    let mut hints = Vec::new();

    // When output was truncated by the default limit (not an explicit --limit), suggest
    // showing all results.
    if !ctx.has_limit {
        let shown = data
            .get("backlinks")
            .and_then(|b| b.as_array())
            .map_or(0, |a| a.len() as u64);
        if let Some(t) = total
            && shown < t
        {
            let file = data.get("file").and_then(|f| f.as_str()).unwrap_or("");
            hints.push(Hint::new(
                format!("Show all {t} backlinks (no limit)"),
                build_command_with_file(ctx, &["backlinks", "--limit", "0"], file, &[]),
            ));
        }
    }

    let file = data.get("file").and_then(|f| f.as_str());

    if let Some(file) = file {
        hints.push(Hint::new(
            "Read this file's content",
            build_command_with_file(ctx, &["read"], file, &[]),
        ));
        hints.push(Hint::new(
            "See this file's outgoing links",
            build_command_no_glob(ctx, &["find", "--file", file, "--fields", "links"]),
        ));
    }

    // Suggest reading the first backlink source.
    if let Some(backlinks) = data.get("backlinks").and_then(|b| b.as_array())
        && let Some(first_source) = backlinks
            .first()
            .and_then(|b| b.get("source"))
            .and_then(|s| s.as_str())
        && hints.len() < MAX_HINTS
    {
        hints.push(Hint::new(
            format!("Read linking file: {first_source}"),
            build_command_with_file(ctx, &["read"], first_source, &[]),
        ));
    }

    hints
}

pub(super) fn hints_for_mv(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let to_path = data.get("to").and_then(|t| t.as_str());
    let is_dry_run = data
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if let Some(to_path) = to_path {
        if is_dry_run {
            if let Some(from_path) = data.get("from").and_then(|f| f.as_str()) {
                hints.push(Hint::new(
                    "Apply this move",
                    build_command_with_file(ctx, &["mv"], from_path, &["--to", to_path]),
                ));
            }
        } else {
            hints.push(Hint::new(
                "Read the moved file",
                build_command_with_file(ctx, &["read"], to_path, &[]),
            ));
            hints.push(Hint::new(
                "Verify backlinks updated",
                build_command_with_file(ctx, &["backlinks"], to_path, &[]),
            ));
        }
    }

    hints
}

/// Check if task output data (single or array) contains any open (not done) tasks.
pub(super) fn task_result_has_open(data: &serde_json::Value) -> bool {
    // Array case (bulk result)
    if let Some(arr) = data.as_array() {
        return arr
            .iter()
            .any(|t| t.get("done") == Some(&serde_json::Value::Bool(false)));
    }
    // Single task case
    data.get("done") == Some(&serde_json::Value::Bool(false))
}

/// Hints for `task read` — suggest toggling or viewing remaining tasks.
pub(super) fn hints_for_task_read(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    // For bulk reads (--all / --section), suggest toggling the same scope.
    if let Some(selector) = &ctx.task_selector {
        if let Some(file) = ctx.file_targets.first() {
            let has_open = task_result_has_open(data);
            if has_open {
                if selector == "all" {
                    hints.push(Hint::new(
                        "Toggle all tasks in this file",
                        build_command_with_file(ctx, &["task", "toggle"], file, &["--all"]),
                    ));
                } else if let Some(section) = selector.strip_prefix("section:") {
                    hints.push(Hint::new(
                        format!("Toggle all tasks in section \"{section}\""),
                        build_command_with_file(
                            ctx,
                            &["task", "toggle"],
                            file,
                            &["--section", section],
                        ),
                    ));
                }
            } else {
                // NEW-18 (dogfood pre3): a bulk read (`--all` / `--section`)
                // whose tasks are all already done had nothing to toggle and
                // fell straight through to an empty hint list — a listing
                // command that answered "nothing here" was still a
                // navigation dead end instead of pointing anywhere else.
                hints.push(Hint::new(
                    "Find files with open tasks",
                    build_command_no_glob(ctx, &["find", "--task", "todo"]),
                ));
            }
        }
        // For "all" and "section:" selectors, return early — the bulk hints are sufficient.
        // For "lines" selector, fall through to the single-task hint path which handles
        // individual line-based suggestions.
        if selector != "lines" {
            return hints;
        }
    }

    // Single-task read path (backward compatible).
    let file = data.get("file").and_then(|f| f.as_str());
    let line = data.get("line").and_then(serde_json::Value::as_u64);
    let done = data
        .get("done")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if let (Some(file), Some(line)) = (file, line) {
        let line_str = line.to_string();
        if !done {
            hints.push(Hint::new(
                "Toggle this task to done",
                build_command_with_file(ctx, &["task", "toggle"], file, &["--line", &line_str]),
            ));
        }
        hints.push(Hint::new(
            "See all open tasks in this file",
            build_command_no_glob(
                ctx,
                &[
                    "find", "--file", file, "--task", "todo", "--fields", "tasks",
                ],
            ),
        ));
    }

    hints
}

pub(super) fn hints_for_task_mutation(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let file = ctx
        .file_targets
        .first()
        .map(String::as_str)
        .or_else(|| data.get("file").and_then(|f| f.as_str()));

    if let Some(file) = file {
        // Suggest reading the scope that was just mutated.
        if let Some(selector) = &ctx.task_selector {
            if selector == "all" {
                hints.push(Hint::new(
                    "Read all tasks in this file",
                    build_command_with_file(ctx, &["task", "read"], file, &["--all"]),
                ));
            } else if let Some(section) = selector.strip_prefix("section:") {
                hints.push(Hint::new(
                    format!("Read tasks in section \"{section}\""),
                    build_command_with_file(ctx, &["task", "read"], file, &["--section", section]),
                ));
            }
        }

        hints.push(Hint::new(
            "See remaining open tasks",
            build_command_no_glob(
                ctx,
                &[
                    "find", "--file", file, "--task", "todo", "--fields", "tasks",
                ],
            ),
        ));
        hints.push(Hint::new(
            "Read the file",
            build_command_with_file(ctx, &["read"], file, &[]),
        ));
    }

    hints
}
