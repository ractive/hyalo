//! Generates drill-down command hints for CLI output.
//!
//! When `--hints` is enabled, each command's output includes suggested next
//! commands. All hints are concrete, executable strings — no templates or
//! placeholders.

/// Maximum number of hints to return from any generator.
const MAX_HINTS: usize = 5;

/// A single drill-down hint: a concrete command plus a short human-readable description.
#[derive(Debug, Clone)]
pub struct Hint {
    pub(crate) description: String,
    pub(crate) cmd: String,
}

impl Hint {
    fn new(description: impl Into<String>, cmd: String) -> Self {
        Self {
            description: description.into(),
            cmd,
        }
    }
}

/// Identifies which command produced the output.
pub enum HintSource {
    Summary,
    PropertiesSummary,
    TagsSummary,
    Find,
    Set,
    Remove,
    Append,
    Read,
    Backlinks,
    Mv,
    TaskRead,
    TaskToggle,
    TaskSetStatus,
    LinksFix,
    CreateIndex,
    DropIndex,
}

/// Global flags to propagate into generated hint commands.
///
/// Each `Option` field is `Some` only when the user passed the flag explicitly
/// on the CLI. Values that came from `.hyalo.toml` config are omitted so that
/// the copy-pasted hint command inherits the same config automatically.
pub struct HintContext {
    pub source: HintSource,
    /// `None` means "." (default) or came from config — omit from hints.
    pub dir: Option<String>,
    pub glob: Vec<String>,
    /// Explicit `--format` from CLI (not from config).
    pub format: Option<String>,
    /// Explicit `--hints` from CLI (not from config).
    pub hints: bool,
    // Find context
    pub fields: Vec<String>,
    pub sort: Option<String>,
    pub has_limit: bool,
    pub has_body_search: bool,
    pub has_regex_search: bool,
    pub property_filters: Vec<String>,
    pub tag_filters: Vec<String>,
    pub task_filter: Option<String>,
    pub file_targets: Vec<String>,
    /// Set when the query was produced by `--view <name>`; suppresses the
    /// "save as view" hint to avoid suggesting the user save a view they
    /// already have.
    pub view_name: Option<String>,
    // Mutation context
    pub dry_run: bool,
    // Index context
    pub index_path: Option<String>,
}

/// Common global flags captured once per command dispatch and threaded into
/// every `HintContext`. Avoids repeating the same three field assignments in
/// every `match` arm of `run.rs`.
pub struct CommonHintFlags {
    /// `--dir` value when explicitly passed on the CLI; `None` when inherited
    /// from `.hyalo.toml` (the hint can omit it and rely on config).
    pub dir: Option<String>,
    /// `--format` value when explicitly passed on the CLI.
    pub format: Option<String>,
    /// Whether `--hints` was explicitly passed on the CLI.
    pub hints: bool,
}

impl HintContext {
    pub fn new(source: HintSource) -> Self {
        Self {
            source,
            dir: None,
            glob: vec![],
            format: None,
            hints: false,
            fields: vec![],
            sort: None,
            has_limit: false,
            has_body_search: false,
            has_regex_search: false,
            property_filters: vec![],
            tag_filters: vec![],
            task_filter: None,
            file_targets: vec![],
            view_name: None,
            dry_run: false,
            index_path: None,
        }
    }

    /// Construct a `HintContext` with the common global flags pre-populated.
    ///
    /// Equivalent to calling `new(source)` followed by assigning `dir`,
    /// `format`, and `hints` — extracted here so every `match` arm in
    /// `run.rs` does not repeat those three lines.
    pub fn from_common(source: HintSource, common: &CommonHintFlags) -> Self {
        let mut ctx = Self::new(source);
        ctx.dir.clone_from(&common.dir);
        ctx.format.clone_from(&common.format);
        ctx.hints = common.hints;
        ctx
    }
}

/// Generate concrete drill-down hints from a command's JSON output.
///
/// Returns at most [`MAX_HINTS`] [`Hint`]s, each with a human-readable description
/// and an executable `hyalo` command (`cmd`).
#[must_use]
pub fn generate_hints(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let hints = match &ctx.source {
        HintSource::Summary => hints_for_summary(ctx, data),
        HintSource::PropertiesSummary => hints_for_properties_summary(ctx, data),
        HintSource::TagsSummary => hints_for_tags_summary(ctx, data),
        HintSource::Find => hints_for_find(ctx, data),
        HintSource::Set | HintSource::Remove | HintSource::Append => hints_for_mutation(ctx, data),
        HintSource::Read => hints_for_read(ctx, data),
        HintSource::Backlinks => hints_for_backlinks(ctx, data),
        HintSource::Mv => hints_for_mv(ctx, data),
        HintSource::TaskRead => hints_for_task_read(ctx, data),
        HintSource::TaskToggle | HintSource::TaskSetStatus => hints_for_task_mutation(ctx, data),
        HintSource::LinksFix => hints_for_links_fix(ctx, data),
        HintSource::CreateIndex => hints_for_create_index(ctx, data),
        HintSource::DropIndex => hints_for_drop_index(ctx, data),
    };
    hints.into_iter().take(MAX_HINTS).collect()
}

// ---------------------------------------------------------------------------
// Command builder
// ---------------------------------------------------------------------------

/// Push the global flags that were explicitly passed on the CLI.
fn push_global_flags(parts: &mut Vec<String>, ctx: &HintContext) {
    if let Some(dir) = &ctx.dir {
        parts.push("--dir".to_owned());
        parts.push(shell_quote(dir));
    }
    if let Some(fmt) = &ctx.format {
        parts.push("--format".to_owned());
        parts.push(shell_quote(fmt));
    }
    if ctx.hints {
        parts.push("--hints".to_owned());
    }
}

/// Build a command that intentionally omits `--glob` (for file-specific hints).
fn build_command_no_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    for arg in args {
        parts.push(shell_quote(arg));
    }
    push_global_flags(&mut parts, ctx);
    parts.join(" ")
}

/// Build a command that propagates `--glob` when present.
fn build_command_with_glob(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    for arg in args {
        parts.push(shell_quote(arg));
    }
    push_global_flags(&mut parts, ctx);
    for glob in &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }
    parts.join(" ")
}

/// Build a `find` command that preserves the caller's existing filters (property,
/// tag, task, file targets) plus `--glob`, then appends `extra_args`.  Use this for
/// hints like sort and limit that refine the current query without changing its scope.
fn build_find_command_preserving_filters(ctx: &HintContext, extra_args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned(), "find".to_owned()];
    for pf in &ctx.property_filters {
        parts.push("--property".to_owned());
        parts.push(shell_quote(pf));
    }
    for tf in &ctx.tag_filters {
        parts.push("--tag".to_owned());
        parts.push(shell_quote(tf));
    }
    if let Some(task) = &ctx.task_filter {
        parts.push("--task".to_owned());
        parts.push(shell_quote(task));
    }
    for ft in &ctx.file_targets {
        parts.push("--file".to_owned());
        parts.push(shell_quote(ft));
    }
    for arg in extra_args {
        parts.push(shell_quote(arg));
    }
    push_global_flags(&mut parts, ctx);
    for glob in &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }
    parts.join(" ")
}

/// Wrap a string in single-quotes if it contains any shell-special characters.
///
/// Uses an allowlist of safe characters — anything not in the list triggers quoting.
/// Single-quoting avoids variable expansion and is safer than double-quoting.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty()
        || s.chars().any(|c| {
            !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '/' | ':' | '@' | '=' | ',' | '+')
        })
    {
        // In single-quoted strings, the only character that needs escaping is '
        // which is done by ending the quote, adding an escaped quote, and reopening.
        format!("'{}'", s.replace('\'', "'\\''"))
    } else {
        s.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Status priority helpers
// ---------------------------------------------------------------------------

/// Priority rank for a status value: lower = more interesting.
fn status_priority(value: &str) -> u8 {
    if value.eq_ignore_ascii_case("in-progress")
        || value.eq_ignore_ascii_case("in progress")
        || value.eq_ignore_ascii_case("active")
    {
        0
    } else if value.eq_ignore_ascii_case("planned") || value.eq_ignore_ascii_case("todo") {
        1
    } else if value.eq_ignore_ascii_case("draft") || value.eq_ignore_ascii_case("idea") {
        2
    } else if value.eq_ignore_ascii_case("completed")
        || value.eq_ignore_ascii_case("done")
        || value.eq_ignore_ascii_case("archived")
    {
        4
    } else {
        3
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Extract the first modified file path from mutation output (single object or array).
fn first_modified_file(data: &serde_json::Value) -> Option<&str> {
    fn extract(obj: &serde_json::Value) -> Option<&str> {
        obj.get("modified")
            .and_then(|m| m.as_array())
            .and_then(|a| a.first())
            .and_then(|f| f.as_str())
    }
    if let Some(arr) = data.as_array() {
        arr.iter().find_map(extract)
    } else {
        extract(data)
    }
}

// ---------------------------------------------------------------------------
// Per-source hint generators
// ---------------------------------------------------------------------------

fn hints_for_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    hints.push(Hint::new(
        "Browse property names and types",
        build_command_with_glob(ctx, &["properties"]),
    ));
    hints.push(Hint::new(
        "Browse tags and their counts",
        build_command_with_glob(ctx, &["tags"]),
    ));

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

    // Suggest find --broken-links if there are broken links.
    let broken_links = data
        .get("links")
        .and_then(|l| l.get("broken"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if broken_links > 0 {
        let remaining = MAX_HINTS.saturating_sub(hints.len());
        if remaining > 0 {
            hints.push(Hint::new(
                "List files with broken links",
                build_command_with_glob(ctx, &["find", "--broken-links"]),
            ));
        }
        let remaining = MAX_HINTS.saturating_sub(hints.len());
        if remaining > 0 {
            hints.push(Hint::new(
                "Auto-fix broken links (dry run)",
                build_command_with_glob(ctx, &["links", "fix"]),
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

    hints
}

fn hints_for_properties_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let Some(arr) = data.as_array() else {
        return vec![];
    };

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
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries
        .into_iter()
        .take(3)
        .map(|(name, count)| {
            Hint::new(
                format!("Find {count} files with property: {name}"),
                build_command_with_glob(ctx, &["find", "--property", name]),
            )
        })
        .collect()
}

/// Slugify a string to the charset valid for view names: `[a-z0-9_-]`.
/// Replaces invalid chars with `-`, collapses runs of `-`, and trims leading/trailing `-`.
fn slugify(s: &str) -> String {
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

/// Derive a short, human-readable name from the active filters.
fn auto_view_name(ctx: &HintContext) -> String {
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
fn build_views_set_command(ctx: &HintContext, view_name: &str) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    push_global_flags(&mut parts, ctx);
    parts.push("views".to_owned());
    parts.push("set".to_owned());
    parts.push(shell_quote(view_name));
    for pf in &ctx.property_filters {
        parts.push("--property".to_owned());
        parts.push(shell_quote(pf));
    }
    for tf in &ctx.tag_filters {
        parts.push("--tag".to_owned());
        parts.push(shell_quote(tf));
    }
    if let Some(task) = &ctx.task_filter {
        parts.push("--task".to_owned());
        parts.push(shell_quote(task));
    }
    parts.join(" ")
}

/// Suggest saving the current query as a view when at least two
/// view-serializable filter dimensions are active and the query did not
/// itself come from a view. Excludes body/regex search since the actual
/// pattern value is not available in `HintContext`.
fn suggest_save_as_view(ctx: &HintContext) -> Option<Hint> {
    if ctx.view_name.is_some() {
        return None;
    }

    // Only count filters that can be round-tripped into a `views set` command.
    // Body/regex search is excluded because HintContext only stores a bool,
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

fn hints_for_find(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    // find returns a bare array as the raw command output (the envelope is built later).
    let Some(results) = data.as_array() else {
        return vec![];
    };

    if results.is_empty() {
        return vec![];
    }

    let mut hints = Vec::new();
    let result_count = results.len();
    let is_single = result_count == 1;

    // --- Single-result hints ---
    if let Some(first_file) = results[0].get("file").and_then(|f| f.as_str()) {
        hints.push(Hint::new(
            "Read this file's content",
            build_command_no_glob(ctx, &["read", "--file", first_file]),
        ));
        if is_single {
            hints.push(Hint::new(
                "See all metadata for this file",
                build_command_no_glob(ctx, &["find", "--file", first_file, "--fields", "all"]),
            ));
        }
        hints.push(Hint::new(
            "See what links to this file",
            build_command_no_glob(ctx, &["backlinks", "--file", first_file]),
        ));
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

        // Pick the most common tag (if any results have tags).
        // Break ties alphabetically for deterministic output.
        if let Some((top_tag, count)) = tag_counts
            .iter()
            .max_by(|(a_tag, a_cnt), (b_tag, b_cnt)| a_cnt.cmp(b_cnt).then(b_tag.cmp(a_tag)))
        {
            let remaining = MAX_HINTS.saturating_sub(hints.len());
            if remaining > 0 {
                hints.push(Hint::new(
                    format!("Narrow by tag: {top_tag} ({count} files)"),
                    build_command_with_glob(ctx, &["find", "--tag", top_tag]),
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
                hints.push(Hint::new(
                    format!("Filter by status: {top_status} ({count} files)"),
                    build_command_with_glob(
                        ctx,
                        &["find", "--property", &format!("status={top_status}")],
                    ),
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

        // Limit suggestion (only if not already limited).
        if !ctx.has_limit {
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

    // Suggest `links fix` when results contain broken links (e.g. from --broken-links).
    // Broken links are serialised with `"path": null` (never omitted) by find's output.
    let has_broken_links = results.iter().any(|item| {
        item.get("links")
            .and_then(|l| l.as_array())
            .is_some_and(|links| {
                links
                    .iter()
                    .any(|link| link.get("path").is_some_and(serde_json::Value::is_null))
            })
    });
    if has_broken_links {
        let remaining = MAX_HINTS.saturating_sub(hints.len());
        if remaining > 0 {
            hints.push(Hint::new(
                "Auto-fix broken links (dry run)",
                build_command_with_glob(ctx, &["links", "fix"]),
            ));
        }
    }

    hints
}

fn hints_for_tags_summary(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    // tags summary returns a bare array [{name, count}, ...].
    let Some(tags_arr) = data.as_array() else {
        return vec![];
    };

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
    entries.sort_by(|a, b| b.1.cmp(&a.1));

    entries
        .into_iter()
        .take(3)
        .map(|(name, count)| {
            Hint::new(
                format!("Find {count} files tagged: {name}"),
                build_command_with_glob(ctx, &["find", "--tag", name]),
            )
        })
        .collect()
}

fn hints_for_mutation(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
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
            build_command_no_glob(ctx, &["read", "--file", file]),
        ));
    }

    hints
}

fn hints_for_read(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
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
            build_command_no_glob(ctx, &["backlinks", "--file", file]),
        ));
    }

    hints
}

fn hints_for_backlinks(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let file = data.get("file").and_then(|f| f.as_str());

    if let Some(file) = file {
        hints.push(Hint::new(
            "Read this file's content",
            build_command_no_glob(ctx, &["read", "--file", file]),
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
    {
        hints.push(Hint::new(
            format!("Read linking file: {first_source}"),
            build_command_no_glob(ctx, &["read", "--file", first_source]),
        ));
    }

    hints
}

fn hints_for_mv(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
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
                    build_command_no_glob(ctx, &["mv", "--file", from_path, "--to", to_path]),
                ));
            }
        } else {
            hints.push(Hint::new(
                "Read the moved file",
                build_command_no_glob(ctx, &["read", "--file", to_path]),
            ));
            hints.push(Hint::new(
                "Verify backlinks updated",
                build_command_no_glob(ctx, &["backlinks", "--file", to_path]),
            ));
        }
    }

    hints
}

/// Hints for `task read` — suggest toggling or viewing remaining tasks.
fn hints_for_task_read(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

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
                build_command_no_glob(
                    ctx,
                    &["task", "toggle", "--file", file, "--line", &line_str],
                ),
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

fn hints_for_task_mutation(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let file = data.get("file").and_then(|f| f.as_str());

    if let Some(file) = file {
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
            build_command_no_glob(ctx, &["read", "--file", file]),
        ));
    }

    hints
}

fn hints_for_links_fix(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let is_dry_run = !data
        .get("applied")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let fixable = data
        .get("fixable")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let unfixable = data
        .get("unfixable")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    if is_dry_run && fixable > 0 {
        hints.push(Hint::new(
            format!("Apply {fixable} fixes"),
            build_command_with_glob(ctx, &["links", "fix", "--apply"]),
        ));
    }

    if unfixable > 0 {
        hints.push(Hint::new(
            "List files with remaining broken links",
            build_command_with_glob(ctx, &["find", "--broken-links"]),
        ));
    }

    hints
}

fn hints_for_create_index(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let index_path = data
        .get("path")
        .and_then(|p| p.as_str())
        .or(ctx.index_path.as_deref())
        .unwrap_or(".hyalo-index");

    hints.push(Hint::new(
        "Query using the index",
        build_command_no_glob(ctx, &["find", "--index", index_path]),
    ));
    hints.push(Hint::new(
        "Delete the index when done",
        build_command_no_glob(ctx, &["drop-index"]),
    ));

    hints
}

fn hints_for_drop_index(ctx: &HintContext, _data: &serde_json::Value) -> Vec<Hint> {
    vec![Hint::new(
        "Rebuild the index",
        build_command_no_glob(ctx, &["create-index"]),
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(source: HintSource) -> HintContext {
        HintContext::new(source)
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
        assert!(
            hints.iter().any(|h| h.cmd.contains("find")
                && h.cmd.contains("--task")
                && h.cmd.contains("todo"))
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &json!([]));
        assert!(hints.is_empty());
    }

    #[test]
    fn properties_summary_propagates_glob() {
        let c = ctx_with_glob(HintSource::PropertiesSummary, "notes/*.md");
        let data = json!([{"name": "status", "type": "text", "count": 5}]);
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &json!([]));
        assert!(hints.is_empty());
    }

    #[test]
    fn find_single_result_suggests_read_and_backlinks() {
        let c = ctx(HintSource::Find);
        let items = vec![make_find_item("notes/alpha.md", None, &[])];
        let data = json!(items);
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
        assert!(
            hints.iter().any(|h| h.cmd.contains("find")
                && h.cmd.contains("--task")
                && h.cmd.contains("todo")),
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
        let hints = generate_hints(&c, &data);
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
    fn find_broad_query_suggests_summary() {
        let c = ctx(HintSource::Find);
        // 15 results, no filters
        let items: Vec<serde_json::Value> = (0..15)
            .map(|i| make_find_item(&format!("{i}.md"), Some("completed"), &[]))
            .collect();
        let data = json!(items);
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
        // Should NOT suggest narrowing by --tag rust (already filtered).
        // Sort/limit hints may legitimately include --tag rust as a preserved filter,
        // so only check narrowing hints (those whose description starts with "Narrow").
        assert!(
            !hints
                .iter()
                .any(|h| h.description.starts_with("Narrow") && h.cmd.contains("--tag rust")),
            "should not suggest narrowing by already-filtered tag: {hints:?}"
        );
        assert!(
            hints.iter().any(|h| h.cmd.contains("--tag cli")),
            "should suggest non-filtered tag: {hints:?}"
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
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
        let hints = generate_hints(&c, &data);
        assert!(
            !hints.iter().any(|h| h.cmd.contains("links fix")),
            "find results without broken links should not suggest links fix: {hints:?}"
        );
    }
}
