//! Hints for `find`, including the save-this-as-a-view suggestion.
//!
//! Split out of the single 5,059-line `hints.rs` in iteration 247 (deep-review
//! hotspot). This is a file split only: the items keep the visibility they had
//! inside the one module, so `hints::...` paths and behaviour are unchanged.

use super::{
    Hint, HintBuilder, HintContext, MAX_HINTS, build_command_no_glob, build_command_with_file,
    build_command_with_glob, build_find_command_composing, build_find_command_preserving_filters,
    build_find_command_with_pattern, status_priority,
};

/// Largest untruncated result set for which `find` still offers
/// `--fields all` (iter-252). Above a handful of files, the full shape is the
/// payload the compact default exists to avoid, so the hint stays quiet.
const MAX_EXPANDABLE_RESULTS: usize = 5;

/// Slugify a string to the charset valid for view names: `[a-z0-9_-]`.
/// Replaces invalid chars with `-`, collapses runs of `-`, and trims leading/trailing `-`.
pub(super) fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
        } else {
            // Replace any non-allowed char with a hyphen (collapsed below).
            if !out.ends_with('-') {
                out.push('-');
            }
        }
    }
    out.trim_matches('-').to_owned()
}

/// Format a confidence floor (0.0-1.0) as a `--min-confidence` CLI argument.
///
/// Rounds to 3 decimal places and trims trailing zeros/the decimal point, so
/// `0.8` stays `"0.8"` rather than growing float-repr noise like
/// `"0.8000000000000001"`, and `0.5` doesn't print as `"0.500"`.
pub(super) fn format_confidence(v: f64) -> String {
    let mut s = format!("{v:.3}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

/// Derive a short, human-readable name from the active filters.
pub(super) fn auto_view_name(ctx: &HintContext) -> String {
    let mut parts: Vec<String> = Vec::new();

    for pf in &ctx.property_filters {
        if let Some(pos) = pf.find("~=") {
            // Regex filter (K~=pattern): use the key, not the pattern.
            let key = &pf[..pos];
            parts.push(key.to_lowercase());
        } else if let Some(pos) = pf.find('=') {
            let val = &pf[pos + 1..];
            if !val.is_empty() {
                parts.push(val.to_lowercase());
            }
        } else if let Some(stripped) = pf.strip_prefix('!') {
            parts.push(format!("no-{stripped}"));
        }
    }

    for tf in &ctx.tag_filters {
        parts.push(tf.to_lowercase());
    }

    if let Some(task) = &ctx.task_filter {
        parts.push(task.to_lowercase());
    }

    let slug = slugify(&parts.join("-"));
    let truncated: String = slug.chars().take(40).collect();
    // Trim any trailing `-` left by truncation mid-word.
    let trimmed = truncated.trim_end_matches('-');
    if trimmed.is_empty() {
        "my-view".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Build the `hyalo views set <name> <filters…>` command string.
pub(super) fn build_views_set_command(ctx: &HintContext, view_name: &str) -> String {
    let mut b = HintBuilder::cmd("views set");
    b.push_quoted(view_name);
    for pf in &ctx.property_filters {
        b.push_raw("--property");
        b.push_quoted(pf);
    }
    for tf in &ctx.tag_filters {
        b.push_raw("--tag");
        b.push_quoted(tf);
    }
    if let Some(task) = &ctx.task_filter {
        b.push_raw("--task");
        b.push_quoted(task);
    }
    b.finish(ctx)
}

/// Suggest saving the current query as a view when at least two
/// view-serializable filter dimensions are active and the query did not
/// itself come from a view. Excludes body/regex search since the actual
/// pattern value is not available in `HintContext`.
pub(super) fn suggest_save_as_view(ctx: &HintContext) -> Option<Hint> {
    if ctx.view_name.is_some() {
        return None;
    }

    // Only count filters that can be round-tripped into a `views set` command.
    // Body/regex search is excluded because `views set` does not support them,
    // not the actual pattern string.
    let filter_count =
        ctx.property_filters.len() + ctx.tag_filters.len() + usize::from(ctx.task_filter.is_some());

    if filter_count < 2 {
        return None;
    }

    let name = auto_view_name(ctx);
    let cmd = build_views_set_command(ctx, &name);
    Some(Hint::new("Save this query as a view", cmd))
}

pub(super) fn hints_for_find(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    // find returns a bare array as the raw command output (the envelope is built later).
    let Some(results) = data.as_array() else {
        return vec![];
    };

    if results.is_empty() {
        // iter-251: an empty result set is the moment an agent most needs a
        // next step, and it used to be the moment hyalo said the least
        // (`No results`, `hints: []`). Lead with the BM25 OR rewrite when a
        // multi-word body search is what came up empty — it is the single most
        // likely fix — then fall through to the generic zero-result hints
        // (did-you-mean, observed values, drop the most selective filter).
        let mut hints: Vec<Hint> = Vec::new();
        // iter-267 (UX-3, reverse direction): the PATTERN was an existing
        // `.md` path, so this was a body search for that literal text. Leading
        // with it here matters more than on the non-empty path — searching for
        // a filename usually finds nothing, and `file not found` is not what
        // came back to explain it.
        if let Some(path) = &ctx.pattern_names_a_file {
            hints.push(Hint::new(
                format!(
                    "'{path}' is a file in this vault — target it instead of searching for its \
                     name"
                ),
                build_command_no_glob(ctx, &["find", "--file", path]),
            ));
        }
        // Skip if the query already contains quotes (phrase search) — splitting on
        // whitespace would produce malformed tokens like `"exact` and `phrase"`.
        if let Some(pat) = &ctx.body_pattern {
            let has_quotes = pat.contains('"');
            let words: Vec<&str> = pat
                .split_whitespace()
                .filter(|w| {
                    !w.starts_with('-')
                        && !w.eq_ignore_ascii_case("or")
                        && !w.eq_ignore_ascii_case("and")
                })
                .collect();
            if !has_quotes && words.len() >= 2 {
                let or_query = words.join(" OR ");
                hints.push(Hint::new(
                    "Try OR instead of AND (match any word)",
                    build_find_command_with_pattern(ctx, &or_query),
                ));
            }
        }
        hints.extend(super::zero_result::zero_result_hints(ctx));
        hints.truncate(super::zero_result::MAX_ZERO_RESULT_HINTS);
        return hints;
    }

    let mut hints = Vec::new();
    let result_count = results.len();
    let is_single = result_count == 1;

    // iter-267 (UX-3, reverse direction): the PATTERN was itself an existing
    // `.md` path, so this ran as a body search for that literal text. The
    // results are legitimate, so this leads rather than replacing them.
    if let Some(path) = &ctx.pattern_names_a_file {
        hints.push(Hint::new(
            format!(
                "'{path}' is a file in this vault — target it instead of searching for its name"
            ),
            build_command_no_glob(ctx, &["find", "--file", path]),
        ));
    }

    // --- Single-result hints ---
    if let Some(first_file) = results[0].get("file").and_then(|f| f.as_str()) {
        hints.push(Hint::new(
            "Read this file's content",
            build_command_with_file(ctx, &["read"], first_file, &[]),
        ));
        if is_single {
            hints.push(Hint::new(
                "See all metadata for this file",
                build_command_no_glob(ctx, &["find", "--file", first_file, "--fields", "all"]),
            ));
        }
        hints.push(Hint::new(
            "See what links to this file",
            build_command_with_file(ctx, &["backlinks"], first_file, &[]),
        ));
    }

    // iter-252: the default result shape carries file, modified, size, lines,
    // title, properties and tags; sections, links, tasks, backlinks and
    // properties-typed are opt-in, so a listing has to say how to get them.
    //
    // Deliberately *not* on every result set. `--fields all` is roughly a 10x
    // payload — the very cost this iteration removed — so suggesting it under
    // a 50-file listing would hand an agent the bill it was just spared. It
    // fires only where expanding is affordable: a handful of results, none of
    // them truncated away. Skipped when the caller already chose `--fields`,
    // and for a single result, where the richer "See all metadata for this
    // file" hint above already spells out `--fields all` for that one file.
    let untruncated = total.is_none_or(|t| result_count as u64 >= t);
    if ctx.fields.is_empty() && !is_single && untruncated && result_count <= MAX_EXPANDABLE_RESULTS
    {
        hints.push(Hint::new(
            "Include the omitted fields (sections, links, tasks, backlinks)",
            build_find_command_preserving_filters(ctx, &["--fields", "all"]),
        ));
    }

    // --- Task bulk operation hints ---
    // When find results target a single file and include task data, suggest bulk task ops.
    if ctx.file_targets.len() == 1 {
        let file = &ctx.file_targets[0];
        let has_open_tasks = results.iter().any(|item| {
            item.get("tasks")
                .and_then(|t| t.as_array())
                .is_some_and(|tasks| {
                    tasks
                        .iter()
                        .any(|t| t.get("done") == Some(&serde_json::Value::Bool(false)))
                })
        });
        if has_open_tasks {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                if let Some(section) = ctx.section_filters.first() {
                    hints.push(Hint::new(
                        format!("Toggle all tasks in section \"{section}\""),
                        build_command_with_file(
                            ctx,
                            &["task", "toggle"],
                            file,
                            &["--section", section],
                        ),
                    ));
                } else {
                    hints.push(Hint::new(
                        "Toggle all tasks in this file",
                        build_command_with_file(ctx, &["task", "toggle"], file, &["--all"]),
                    ));
                }
            }
        }
    }

    // --- Broad query → suggest summary ---
    let has_no_filters = ctx.property_filters.is_empty()
        && ctx.tag_filters.is_empty()
        && ctx.task_filter.is_none()
        && !ctx.has_body_search
        && !ctx.has_regex_search
        && ctx.file_targets.is_empty();

    if has_no_filters && result_count > 10 {
        hints.push(Hint::new(
            if ctx.glob.is_empty() {
                "Get a high-level vault overview"
            } else {
                "Get stats for this file set"
            },
            build_command_with_glob(ctx, &["summary"]),
        ));
    }

    // --- Show-all hint when default limit truncated output ---
    if !ctx.has_limit
        && let Some(t) = total
        && (result_count as u64) < t
    {
        let remaining = MAX_HINTS.saturating_sub(hints.len());
        if remaining > 0 {
            hints.push(Hint::new(
                format!("Show all {t} results (no limit)"),
                build_find_command_preserving_filters(ctx, &["--limit", "0"]),
            ));
        }
    }

    // --- Narrowing for many results (>5) ---
    if result_count > 5 {
        // Tag narrowing (skip tags already filtered on).
        let mut tag_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for item in results {
            if let Some(tags) = item.get("tags").and_then(|t| t.as_array()) {
                for tag in tags {
                    if let Some(name) = tag.as_str()
                        && !ctx.tag_filters.iter().any(|t| t == name)
                    {
                        *tag_counts.entry(name).or_insert(0) += 1;
                    }
                }
            }
        }

        // Collect status property frequencies — skip statuses already filtered on.
        // Handles both scalar and array-valued status properties.
        let mut status_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for item in results {
            let Some(status_val) = item.get("properties").and_then(|p| p.get("status")) else {
                continue;
            };
            // Yield individual &str values from scalar or array status.
            let iter: Box<dyn Iterator<Item = &str>> = match status_val {
                serde_json::Value::String(s) => Box::new(std::iter::once(s.as_str())),
                serde_json::Value::Array(arr) => Box::new(arr.iter().filter_map(|v| v.as_str())),
                _ => Box::new(std::iter::empty()),
            };
            for status in iter {
                let already_filtered = ctx
                    .property_filters
                    .iter()
                    .any(|f| f == &format!("status={status}"));
                if !already_filtered {
                    *status_counts.entry(status).or_insert(0) += 1;
                }
            }
        }

        // Whether the shown results are only a page of a larger set. When they
        // are, the per-tag / per-status frequencies below were computed on the
        // truncated page, so they undercount the true filtered totals — we must
        // not present them as the count the composed command would return
        // (BUG-8: hint said 27, the command returned 37). In that case the
        // parenthetical count is dropped and the hint carries the composing
        // command only.
        let is_truncated = total.is_some_and(|t| (result_count as u64) < t);

        // Pick the most common tag (if any results have tags).
        // Break ties alphabetically for deterministic output.
        if let Some((top_tag, count)) = tag_counts
            .iter()
            .max_by(|(a_tag, a_cnt), (b_tag, b_cnt)| a_cnt.cmp(b_cnt).then(b_tag.cmp(a_tag)))
        {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                // Compose with the active filters (BUG-8): a "narrow by tag"
                // hint on a `--orphan` query must keep `--orphan`, else pasting
                // it widens the set.
                let description = if is_truncated {
                    format!("Narrow by tag: {top_tag}")
                } else {
                    format!("Narrow by tag: {top_tag} ({count} files)")
                };
                hints.push(Hint::new(
                    description,
                    build_find_command_composing(ctx, &["--tag", top_tag]),
                ));
            }
        }

        // Pick the most interesting status value (prefer active/planned over completed).
        let mut status_vec: Vec<(&str, usize, u8)> = status_counts
            .iter()
            .map(|(v, c)| (*v, *c, status_priority(v)))
            .collect();
        // Sort by priority (ascending), then count (descending), then name (ascending).
        status_vec.sort_by(|a, b| a.2.cmp(&b.2).then(b.1.cmp(&a.1)).then(a.0.cmp(b.0)));

        if let Some((top_status, count, _)) = status_vec.first() {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                let status_filter = format!("status={top_status}");
                let description = if is_truncated {
                    format!("Filter by status: {top_status}")
                } else {
                    format!("Filter by status: {top_status} ({count} files)")
                };
                hints.push(Hint::new(
                    description,
                    build_find_command_composing(ctx, &["--property", &status_filter]),
                ));
            }
        }

        // Sort suggestion (only if not already sorting).
        if ctx.sort.is_none() {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                hints.push(Hint::new(
                    "Sort by most recently modified",
                    build_find_command_preserving_filters(
                        ctx,
                        &["--sort", "modified", "--reverse"],
                    ),
                ));
            }
        }

        // Limit suggestion: suggest --limit 10 when not truncated and no explicit limit.
        if !ctx.has_limit && total.is_none_or(|t| (result_count as u64) >= t) {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                hints.push(Hint::new(
                    "Limit to 10 results",
                    build_find_command_preserving_filters(ctx, &["--limit", "10"]),
                ));
            }
        }
    }

    // Suggest saving as a view for non-trivial queries (independent of result count).
    if let Some(view_hint) = suggest_save_as_view(ctx) {
        let remaining = MAX_HINTS.saturating_sub(hints.len());
        if remaining > 0 {
            hints.push(view_hint);
        }
    }

    // Body search → regex suggestion is intentionally omitted.
    // We cannot produce a concrete regex without knowing the user's intent,
    // and a placeholder like `'pattern'` would violate our no-templates contract.

    // Suggest phrase search when body search has multiple words and many results.
    if let Some(pat) = &ctx.body_pattern {
        let has_quotes = pat.contains('"');
        let words: Vec<&str> = pat
            .split_whitespace()
            .filter(|w| {
                !w.starts_with('-')
                    && !w.eq_ignore_ascii_case("or")
                    && !w.eq_ignore_ascii_case("and")
            })
            .collect();
        if !has_quotes && words.len() >= 2 && result_count > 10 {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                let phrase = format!("\"{}\"", words.join(" "));
                hints.push(Hint::new(
                    "Try as exact phrase for more precise results",
                    build_find_command_with_pattern(ctx, &phrase),
                ));
            }
        }
    }

    // Suggest `links fix` when results contain broken links (e.g. from --broken-links).
    // Broken links are serialised with `"path": null` (never omitted) by find's output.
    //
    // iter-274 (UX-3): a broken link whose target is *site-absolute* (`/en-US/
    // docs/...`) is almost never a path typo `links fix` can repair — it is an
    // unconfigured or wrong `site_prefix`, and running the fixer on it costs
    // half a minute to propose nothing. When every broken link in the result
    // set is site-absolute, say that instead of pointing at the fixer.
    let mut broken_total = 0usize;
    let mut broken_site_absolute = 0usize;
    for item in results {
        let Some(links) = item.get("links").and_then(|l| l.as_array()) else {
            continue;
        };
        for link in links {
            if !link.get("path").is_some_and(serde_json::Value::is_null) {
                continue;
            }
            broken_total += 1;
            if link
                .get("target")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|t| t.replace('\\', "/").starts_with('/'))
            {
                broken_site_absolute += 1;
            }
        }
    }
    if broken_total > 0 && MAX_HINTS.saturating_sub(hints.len()) > 0 {
        if broken_site_absolute == broken_total {
            hints.push(Hint::without_cmd(format!(
                "all {broken_total} broken link(s) are site-absolute — set `site_prefix` in \
                 .hyalo.toml (or `--site-prefix`) rather than running `links fix`"
            )));
        } else {
            hints.push(Hint::new(
                "Auto-fix broken links (dry run)",
                build_command_with_glob(ctx, &["links", "fix"]),
            ));
        }
    }

    hints
}
