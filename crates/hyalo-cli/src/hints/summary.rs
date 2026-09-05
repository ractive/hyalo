//! Hints for the vault-overview commands: `summary`, `properties`, `tags`.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{
    Hint, HintContext, LARGE_VAULT_FILE_COUNT, MAX_HINTS, SITE_URL_BROKEN_PERCENT,
    SITE_URL_MIN_BROKEN, build_command_no_glob, build_command_with_glob, status_priority,
};

pub(super) fn hints_for_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    // Compute the large-vault create-index hint upfront so it can be
    // prepended at the end. This guarantees the hint is visible even when all
    // MAX_HINTS slots would otherwise be consumed by orphan / broken-link /
    // links-fix hints on large vaults (NEW-1).
    let create_index_hint: Option<Hint> = if !ctx.has_index && !ctx.quiet {
        let files_total = data
            .get("files")
            .and_then(|f| f.get("total"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if files_total > LARGE_VAULT_FILE_COUNT {
            Some(Hint::new(
                format!("Vault has {files_total} files — create an index for faster queries"),
                build_command_no_glob(ctx, &["create-index"]),
            ))
        } else {
            None
        }
    } else {
        None
    };

    let mut hints = Vec::new();

    hints.push(Hint::new(
        "Browse property names and types",
        build_command_with_glob(ctx, &["properties"]),
    ));
    hints.push(Hint::new(
        "Browse tags and their counts",
        build_command_with_glob(ctx, &["tags"]),
    ));

    // Suggest lint early when there are schema violations — high priority so it
    // is not pushed out by orphans/dead-ends/broken-links hints.
    if let Some(schema_obj) = data.get("schema") {
        let errors = schema_obj
            .get("errors")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let warnings = schema_obj
            .get("warnings")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if (errors > 0 || warnings > 0) && hints.len() < MAX_HINTS {
            let errors_label = if errors == 1 { "error" } else { "errors" };
            let warns_label = if warnings == 1 { "warning" } else { "warnings" };
            // The summary counter measures SCHEMA (frontmatter) violations only
            // — it does not include MD body rules that plain `hyalo lint` runs.
            // Label it "Schema" and point at `hyalo lint --rule SCHEMA` so the
            // hinted command reproduces these exact counts. Bare `hyalo lint`
            // reported wildly different totals (BUG-9: 5 errors / 12 warnings in
            // the summary vs 0 / 660 from `hyalo lint`); the counter now also
            // applies `[lint] ignore` globs, matching what `--rule SCHEMA` sees.
            hints.push(Hint::new(
                format!("Schema: {errors} {errors_label}, {warnings} {warns_label}"),
                build_command_with_glob(ctx, &["lint", "--rule", "SCHEMA"]),
            ));
        }
    }

    // Suggest find --task todo if there are open tasks.
    let tasks_total = data
        .get("tasks")
        .and_then(|t| t.get("total"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let tasks_done = data
        .get("tasks")
        .and_then(|t| t.get("done"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if tasks_total > tasks_done {
        hints.push(Hint::new(
            "Find files with open tasks",
            build_command_with_glob(ctx, &["find", "--task", "todo"]),
        ));
    }

    // Suggest find --orphan if there are orphan files.
    let orphan_count = data
        .get("orphans")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if orphan_count > 0 && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            format!("{orphan_count} orphan files"),
            build_command_with_glob(ctx, &["find", "--orphan"]),
        ));
    }

    // Suggest find --dead-end if there are dead-end files.
    let dead_end_count = data
        .get("dead_ends")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if dead_end_count > 0 && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            format!("{dead_end_count} dead-end files"),
            build_command_with_glob(ctx, &["find", "--dead-end"]),
        ));
    }

    // Suggest find --broken-links if there are broken links.
    let broken_links = data
        .get("links")
        .and_then(|l| l.get("broken"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let total_links = data
        .get("links")
        .and_then(|l| l.get("total"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    // Site-URL heuristic: when nearly every link is "broken" on a link-heavy
    // vault, the links are almost certainly absolute site URLs the vault has no
    // `--site-prefix` configured to resolve (MDN: 49,933/49,935 "broken"), not
    // fixable file typos. Offering `links fix` on 50k links is actively
    // misleading, so emit a diagnostic and skip the fix suggestion.
    // Integer form of `broken/total >= SITE_URL_BROKEN_PERCENT/100` — avoids a
    // lossy u64→f64 cast. `saturating_mul` guards the (implausible) overflow at
    // >1.8e17 links.
    let looks_like_site_urls = broken_links >= SITE_URL_MIN_BROKEN
        && total_links > 0
        && broken_links.saturating_mul(100) >= total_links.saturating_mul(SITE_URL_BROKEN_PERCENT);
    if broken_links > 0 && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            format!("{broken_links} broken links"),
            build_command_with_glob(ctx, &["find", "--broken-links"]),
        ));
        if looks_like_site_urls {
            if hints.len() < MAX_HINTS {
                hints.push(Hint::without_cmd(format!(
                    "{broken_links} of {total_links} links are unresolvable — they look like \
                     absolute site URLs; set `--site-prefix` (or `site_prefix` in .hyalo.toml) \
                     to resolve them rather than running `links fix`"
                )));
            }
        } else if hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                "Auto-fix broken links (dry run)",
                build_command_with_glob(ctx, &["links", "fix"]),
            ));
        }
    }

    // When schema is defined but no violations, or when there's still room,
    // add the general lint / types hints.
    if let Some(schema_obj) = data.get("schema") {
        let errors = schema_obj
            .get("errors")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let warnings = schema_obj
            .get("warnings")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if errors == 0 && warnings == 0 && hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                "Validate frontmatter against schema",
                build_command_with_glob(ctx, &["lint"]),
            ));
        }
        if hints.len() < MAX_HINTS {
            hints.push(Hint::new(
                "Manage type schemas",
                build_command_no_glob(ctx, &["types", "list"]),
            ));
        }
    }

    // Pick 1-2 most interesting status values.
    if let Some(status_arr) = data.get("status").and_then(|s| s.as_array()) {
        let mut groups: Vec<(&str, u8)> = status_arr
            .iter()
            .filter_map(|g| {
                let value = g.get("value").and_then(|v| v.as_str())?;
                Some((value, status_priority(value)))
            })
            .collect();
        groups.sort_by_key(|&(_, p)| p);

        let remaining = MAX_HINTS.saturating_sub(hints.len());
        for (value, _) in groups.into_iter().take(remaining.min(2)) {
            let filter = format!("status={value}");
            hints.push(Hint::new(
                format!("Filter by status: {value}"),
                build_command_no_glob(ctx, &["find", "--property", &filter]),
            ));
        }
    }

    // Prepend the create-index hint (computed at the top of this function) so
    // it is visible even when all MAX_HINTS slots are consumed by health hints
    // (orphans / broken-links / links-fix) on large vaults (NEW-1). Inserting
    // at position 0 displaces the last (lowest-priority) hint when the cap is
    // already reached — the index hint has the highest user-visible payoff.
    if let Some(ci_hint) = create_index_hint {
        hints.insert(0, ci_hint);
        if hints.len() > MAX_HINTS {
            hints.pop();
        }
    }

    hints
}

pub(super) fn hints_for_properties_summary(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };

    let mut hints = Vec::new();

    // When output was truncated by the default limit (not an explicit --limit), suggest
    // showing all results.
    if !ctx.has_limit {
        let shown = arr.len() as u64;
        if let Some(t) = total
            && shown < t
        {
            hints.push(Hint::new(
                format!("Show all {t} properties (no limit)"),
                build_command_with_glob(ctx, &["properties", "summary", "--limit", "0"]),
            ));
        }
    }

    // Sort by count descending, take top 3.
    let mut entries: Vec<(&str, u64)> = arr
        .iter()
        .filter_map(|e| {
            let name = e.get("name").and_then(|n| n.as_str())?;
            let count = e
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            Some((name, count))
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));

    for (name, count) in entries.into_iter().take(3) {
        if hints.len() >= MAX_HINTS {
            break;
        }
        hints.push(Hint::new(
            format!("Find {count} files with property: {name}"),
            build_command_with_glob(ctx, &["find", "--property", name]),
        ));
    }

    hints
}

pub(super) fn hints_for_tags_summary(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    // tags summary returns a bare array [{name, count}, ...].
    let Some(tags_arr) = data.as_array() else {
        return vec![];
    };

    let mut hints = Vec::new();

    // When output was truncated by the default limit (not an explicit --limit), suggest
    // showing all results.
    if !ctx.has_limit {
        let shown = tags_arr.len() as u64;
        if let Some(t) = total
            && shown < t
        {
            hints.push(Hint::new(
                format!("Show all {t} tags (no limit)"),
                build_command_with_glob(ctx, &["tags", "summary", "--limit", "0"]),
            ));
        }
    }

    // Sort by count descending, take top 3.
    let mut entries: Vec<(&str, u64)> = tags_arr
        .iter()
        .filter_map(|entry| {
            let name = entry.get("name").and_then(|n| n.as_str())?;
            let count = entry
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            Some((name, count))
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));

    for (name, count) in entries.into_iter().take(3) {
        if hints.len() >= MAX_HINTS {
            break;
        }
        hints.push(Hint::new(
            format!("Find {count} files tagged: {name}"),
            build_command_with_glob(ctx, &["find", "--tag", name]),
        ));
    }

    hints
}
