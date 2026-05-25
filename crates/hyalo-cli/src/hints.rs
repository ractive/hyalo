//! Generates drill-down command hints for CLI output.
//!
//! When `--hints` is enabled, each command's output includes suggested next
//! commands. All hints are concrete, executable strings — no templates or
//! placeholders.

/// Maximum number of hints to return from any generator.
const MAX_HINTS: usize = 5;

/// Wall-clock threshold for the slow-query hint (milliseconds).
///
/// Rationale: shorter than the human "this is slow" threshold (~1 s) with
/// margin; longer than typical disk scans on small vaults (~100 ms).
pub(crate) const SLOW_QUERY_THRESHOLD_MS: u64 = 500;

/// File count threshold for the large-vault summary hint.
///
/// Rationale: vaults above this size see measurable benefit from a snapshot
/// index; below it the disk scan is fast enough not to warrant the hint.
pub(crate) const LARGE_VAULT_FILE_COUNT: u64 = 500;

/// Prefix used by lint for frontmatter parse errors. Shared between
/// `commands::lint` and the hint generator to avoid brittle string coupling.
pub(crate) const PARSE_ERROR_PREFIX: &str = "could not parse frontmatter";

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

    /// Advice-only hint with no follow-up command. JSON consumers see
    /// `cmd: ""`; text renderers special-case the empty-cmd shape so the
    /// `  -> <cmd>  # <desc>` layout collapses to `  -> <desc>`.
    fn without_cmd(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            cmd: String::new(),
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
    LinksAuto,
    CreateIndex,
    DropIndex,
    Lint,
    Types { subcommand: Option<String> },
    New { file: String },
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
    /// Wall-clock elapsed time for the command body (set after dispatch).
    /// Used by the slow-query hint; `None` means not yet measured.
    pub elapsed_ms: Option<u64>,
    /// Whether `--quiet` / `-q` was passed.  Suppresses the slow-query hint.
    pub quiet: bool,
    /// Whether an `--index` / `--index-file` snapshot was active for this run.
    /// Suppresses index-suggestion hints when already using an index.
    pub has_index: bool,
    // Find context
    pub fields: Vec<String>,
    pub sort: Option<String>,
    pub has_limit: bool,
    pub has_body_search: bool,
    /// The actual body-search pattern string, when a body search was issued.
    pub body_pattern: Option<String>,
    pub has_regex_search: bool,
    pub property_filters: Vec<String>,
    pub tag_filters: Vec<String>,
    pub task_filter: Option<String>,
    pub file_targets: Vec<String>,
    pub section_filters: Vec<String>,
    /// Set when the query was produced by `--view <name>`; suppresses the
    /// "save as view" hint to avoid suggesting the user save a view they
    /// already have.
    pub view_name: Option<String>,
    /// Task selector used: "all", "section:<name>", or "lines" (for multi-line).
    /// `None` means single-line or no task context.
    pub task_selector: Option<String>,
    // Mutation context
    pub dry_run: bool,
    // Index context
    pub index_path: Option<String>,
    // Links-auto context (for replaying the exact preview scope in hints)
    pub auto_link_file: Option<String>,
    pub auto_link_min_length: Option<usize>,
    pub auto_link_exclude_titles: Vec<String>,
    // Lint-specific context (for smarter hint generation)
    /// Whether `--fix` was passed (not just `--fix --dry-run`).
    pub lint_is_fix: bool,
    /// Single rule filter (`--rule`).
    pub lint_rule: Option<String>,
    /// Rule prefix filter (`--rule-prefix`).
    pub lint_rule_prefix: Option<String>,
    /// Rules to fix (`--fix-rule`, repeatable).
    pub lint_fix_rules: Vec<String>,
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
            elapsed_ms: None,
            quiet: false,
            has_index: false,
            fields: vec![],
            sort: None,
            has_limit: false,
            has_body_search: false,
            body_pattern: None,
            has_regex_search: false,
            property_filters: vec![],
            tag_filters: vec![],
            task_filter: None,
            file_targets: vec![],
            section_filters: vec![],
            view_name: None,
            task_selector: None,
            dry_run: false,
            index_path: None,
            auto_link_file: None,
            auto_link_min_length: None,
            auto_link_exclude_titles: vec![],
            lint_is_fix: false,
            lint_rule: None,
            lint_rule_prefix: None,
            lint_fix_rules: vec![],
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
/// `total` is the real count of items (may exceed the number of items in `data`
/// when output was truncated by a limit). `None` means the command doesn't
/// produce a list with a total.
///
/// Returns at most [`MAX_HINTS`] [`Hint`]s, each with a human-readable description
/// and an executable `hyalo` command (`cmd`).
/// Counts of paths the `--files-from` resolver dropped during input
/// processing. Mirrors the fields injected into the JSON envelope by
/// `output_pipeline::inject_files_from_counters`. Passed into
/// [`generate_hints`] alongside `data` so counter-aware hints can fire even
/// though the counters haven't been merged into the data value yet.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilesFromCounterSummary {
    pub files_missing: u64,
    pub files_skipped_outside_vault: u64,
}

#[must_use]
pub fn generate_hints(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
) -> Vec<Hint> {
    generate_hints_with_counters(ctx, data, total, None)
}

/// Same as [`generate_hints`] but also factors in `--files-from` counters
/// known to the caller (the output pipeline) but not yet injected into
/// `data`. Used by the dispatch layer; tests and other callers can use the
/// no-counters [`generate_hints`].
#[must_use]
pub fn generate_hints_with_counters(
    ctx: &HintContext,
    data: &serde_json::Value,
    total: Option<u64>,
    counters: Option<FilesFromCounterSummary>,
) -> Vec<Hint> {
    let mut hints = match &ctx.source {
        HintSource::Summary => hints_for_summary(ctx, data),
        HintSource::PropertiesSummary => hints_for_properties_summary(ctx, data, total),
        HintSource::TagsSummary => hints_for_tags_summary(ctx, data, total),
        HintSource::Find => hints_for_find(ctx, data, total),
        HintSource::Set | HintSource::Remove | HintSource::Append => hints_for_mutation(ctx, data),
        HintSource::Read => hints_for_read(ctx, data),
        HintSource::Backlinks => hints_for_backlinks(ctx, data, total),
        HintSource::Mv => hints_for_mv(ctx, data),
        HintSource::TaskRead => hints_for_task_read(ctx, data),
        HintSource::TaskToggle | HintSource::TaskSetStatus => hints_for_task_mutation(ctx, data),
        HintSource::LinksFix => hints_for_links_fix(ctx, data),
        HintSource::LinksAuto => hints_for_links_auto(ctx, data),
        HintSource::CreateIndex => hints_for_create_index(ctx, data),
        HintSource::DropIndex => hints_for_drop_index(ctx, data),
        HintSource::Lint => hints_for_lint(ctx, data, total),
        HintSource::Types { .. } => hints_for_types(ctx, data),
        HintSource::New { file } => hints_for_new(ctx, file),
    };
    // iter-144: slow-query index-suggestion hint. Appended after per-command
    // hints so domain-specific hints are not displaced; counts toward MAX_HINTS.
    // Dedupe against the large-vault hint emitted by `hints_for_summary`:
    // when a `summary` run is *both* slow and large, only one create-index
    // hint should occupy a slot.
    if hints.len() < MAX_HINTS
        && let Some(hint) = slow_query_hint(ctx)
        && !hints.iter().any(|h| h.cmd == hint.cmd)
    {
        hints.push(hint);
    }

    // iter-143: `--files-from`-aware hints. Counters are passed in from the
    // output pipeline (the envelope merge happens *after* hint generation,
    // so `data` doesn't carry them yet). Prepended so the `MAX_HINTS` cap
    // doesn't crowd them out — a skipped-input warning is more urgent than
    // a follow-up suggestion.
    let mut ff_hints = files_from_hints(counters);
    ff_hints.append(&mut hints);
    ff_hints.into_iter().take(MAX_HINTS).collect()
}

/// Return an index-suggestion hint when the command was slow and no index is active.
///
/// Eligible sources: `find`, `lint`, `backlinks`, `properties summary`,
/// `tags summary`, `summary`, and `read` — commands that scan the vault and
/// benefit from a snapshot index.
///
/// Suppressed when:
/// - `ctx.quiet` is true (`--quiet` flag).
/// - `ctx.has_index` is true (snapshot already active).
/// - `elapsed_ms` is below [`SLOW_QUERY_THRESHOLD_MS`].
fn slow_query_hint(ctx: &HintContext) -> Option<Hint> {
    // Only eligible commands produce vault scans that an index can speed up.
    let eligible = matches!(
        ctx.source,
        HintSource::Find
            | HintSource::Lint
            | HintSource::Backlinks
            | HintSource::PropertiesSummary
            | HintSource::TagsSummary
            | HintSource::Summary
            | HintSource::Read
    );
    if !eligible {
        return None;
    }
    if ctx.quiet || ctx.has_index {
        return None;
    }
    let elapsed = ctx.elapsed_ms?;
    if elapsed <= SLOW_QUERY_THRESHOLD_MS {
        return None;
    }
    Some(Hint::new(
        format!("Command took {elapsed} ms. Create an index for faster queries:"),
        "hyalo create-index".to_owned(),
    ))
}

/// Return `--files-from`-counter hints when the resolver reported non-zero
/// `files_missing` or `files_skipped_outside_vault`. `files_skipped_non_md`
/// is intentionally not hinted — it's common when piping from `git diff` and
/// not actionable (the caller's diff included `.toml` / `.md.lock` / etc).
fn files_from_hints(counters: Option<FilesFromCounterSummary>) -> Vec<Hint> {
    let mut out = Vec::new();
    let Some(c) = counters else {
        return out;
    };

    if c.files_missing > 0 {
        out.push(Hint::without_cmd(format!(
            "{} input path(s) did not exist on disk (likely deletions); \
             use `git diff --name-only --diff-filter=AMR` upstream to filter them out",
            c.files_missing
        )));
    }
    if c.files_skipped_outside_vault > 0 {
        out.push(Hint::without_cmd(format!(
            "{} input path(s) were outside the vault; \
             check your --dir or the upstream filter",
            c.files_skipped_outside_vault
        )));
    }
    out
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

/// Build a command where `file_arg` is a positional file path following `subcommand_args`.
///
/// If `file_arg` starts with `-`, emits `--file <path>` instead of the bare positional
/// to prevent clap from interpreting the filename as a flag.
fn build_command_with_file(
    ctx: &HintContext,
    subcommand_args: &[&str],
    file_arg: &str,
    trailing_args: &[&str],
) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    for arg in subcommand_args {
        parts.push(shell_quote(arg));
    }
    push_file_positional(&mut parts, file_arg);
    for arg in trailing_args {
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

/// Like `build_command_with_glob` but also preserves `--file` / positional file
/// targets so that lint hints don't widen scope from a single file to the whole
/// vault.
fn build_command_with_glob_and_files(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    for arg in args {
        parts.push(shell_quote(arg));
    }
    push_global_flags(&mut parts, ctx);
    for glob in &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }
    for ft in &ctx.file_targets {
        parts.push(shell_quote(ft));
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

/// Build a `find` command that replaces the body search pattern with `new_pattern`
/// while preserving all other existing filters (property, tag, task, file targets,
/// glob). The pattern is inserted as a positional argument immediately after `find`.
fn build_find_command_with_pattern(ctx: &HintContext, new_pattern: &str) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned(), "find".to_owned()];
    parts.push(shell_quote(new_pattern));
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
    push_global_flags(&mut parts, ctx);
    for glob in &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }
    parts.join(" ")
}

/// Push a file argument that is safe as a positional arg.
///
/// If the filename starts with `-`, clap would interpret it as a flag.
/// In that case, emit `--file <path>` (flag form) instead of the bare positional.
fn push_file_positional(parts: &mut Vec<String>, file: &str) {
    if file.starts_with('-') {
        parts.push("--file".to_owned());
        parts.push(shell_quote(file));
    } else {
        parts.push(shell_quote(file));
    }
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
            hints.push(Hint::new(
                format!("Lint: {errors} errors, {warnings} warnings"),
                build_command_with_glob(ctx, &["lint"]),
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
    if broken_links > 0 && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            format!("{broken_links} broken links"),
            build_command_with_glob(ctx, &["find", "--broken-links"]),
        ));
        if hints.len() < MAX_HINTS {
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

    // Large-vault index-suggestion hint. Fires when the vault exceeds the
    // LARGE_VAULT_FILE_COUNT threshold and no snapshot index is active.
    // Suppressed by `--quiet` to match the slow-query hint's behavior.
    if hints.len() < MAX_HINTS && !ctx.has_index && !ctx.quiet {
        let files_total = data
            .get("files")
            .and_then(|f| f.get("total"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if files_total > LARGE_VAULT_FILE_COUNT {
            hints.push(Hint::new(
                format!("Vault has {files_total} files — create an index for faster queries:"),
                "hyalo create-index".to_owned(),
            ));
        }
    }

    hints
}

fn hints_for_properties_summary(
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
                build_command_with_glob(ctx, &["properties", "--limit", "0"]),
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

fn hints_for_find(ctx: &HintContext, data: &serde_json::Value, total: Option<u64>) -> Vec<Hint> {
    // find returns a bare array as the raw command output (the envelope is built later).
    let Some(results) = data.as_array() else {
        return vec![];
    };

    if results.is_empty() {
        // When a multi-word BM25 search returns nothing, suggest trying OR instead.
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
                return vec![Hint::new(
                    "Try OR instead of AND (match any word)",
                    build_find_command_with_pattern(ctx, &or_query),
                )];
            }
        }
        return vec![];
    }

    let mut hints = Vec::new();
    let result_count = results.len();
    let is_single = result_count == 1;

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

fn hints_for_tags_summary(
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
                build_command_with_glob(ctx, &["tags", "--limit", "0"]),
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
            build_command_no_glob(ctx, &["read", file]),
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
            build_command_with_file(ctx, &["backlinks"], file, &[]),
        ));
    }

    hints
}

fn hints_for_backlinks(
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
fn task_result_has_open(data: &serde_json::Value) -> bool {
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
fn hints_for_task_read(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
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

fn hints_for_task_mutation(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
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

fn hints_for_links_auto(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    let is_dry_run = !data
        .get("applied")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let total = data
        .get("total")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);

    if is_dry_run && total > 0 {
        // Rebuild the exact command from the preview, preserving all
        // scope-narrowing flags so the apply doesn't widen the mutation set.
        let mut args: Vec<&str> = vec!["links", "auto", "--apply"];
        let min_str;
        if let Some(ml) = ctx.auto_link_min_length
            && ml != 3
        {
            args.push("--min-length");
            min_str = ml.to_string();
            args.push(&min_str);
        }
        let cmd = build_command_with_glob(ctx, &args);
        // Append --file and --exclude-title after the builder (they are not
        // glob-related and aren't handled by build_command_with_glob).
        let mut parts = vec![cmd];
        if let Some(ref f) = ctx.auto_link_file {
            parts.push(format!("--file {}", shell_quote(f)));
        }
        for et in &ctx.auto_link_exclude_titles {
            parts.push(format!("--exclude-title {}", shell_quote(et)));
        }
        hints.push(Hint::new(
            format!("Apply {total} auto-links"),
            parts.join(" "),
        ));
    }

    hints
}

fn hints_for_create_index(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let mut hints = Vec::new();

    // Use bare `--index` (defaults to .hyalo-index in vault dir) for the default path.
    // Only include the explicit path when the index was created at a non-default location.
    let index_path = data
        .get("path")
        .and_then(|p| p.as_str())
        .or(ctx.index_path.as_deref());

    // Only treat as default when no path was reported or it's the bare default name.
    // Custom paths like `sub/.hyalo-index` must emit the explicit path in the hint.
    let is_default = index_path.is_none_or(|p| p == ".hyalo-index");

    let hint_cmd = if is_default {
        build_command_no_glob(ctx, &["find", "--index"])
    } else {
        build_command_no_glob(
            ctx,
            &["find", "--index-file", index_path.unwrap_or(".hyalo-index")],
        )
    };

    hints.push(Hint::new("Query using the index", hint_cmd));
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

/// Ratio threshold for rule dominance (UX-2).
const RULE_DOMINANCE_RATIO: f64 = 0.5;
/// Absolute minimum violations for rule dominance (UX-2).
const RULE_DOMINANCE_MIN: usize = 50;

/// Return a per-rule hint entry for HYALO001 or HYALO002, or `None` for other rules.
fn per_rule_hint(ctx: &HintContext, rule_id: &str, worst_file: Option<&str>) -> Option<Hint> {
    match rule_id {
        "HYALO001" => Some(Hint::new(
            "Auto-fix HYALO001 violations",
            build_lint_with_filter_flags(ctx, &["lint", "--rule", "HYALO001", "--fix"]),
        )),
        "HYALO002" => worst_file.map(|file| {
            // Use `--file <path>` rather than a positional, since `find`'s
            // positional argument is the search pattern.
            Hint::new(
                format!("See open tasks in {file}"),
                build_command_no_glob(ctx, &["find", "--task", "todo", "--file", file]),
            )
        }),
        _ => None,
    }
}

/// Build a lint command that preserves `--rule`, `--rule-prefix`, `--fix-rule`, glob, and
/// file targets from the current context, then appends `args`.
fn build_lint_with_filter_flags(ctx: &HintContext, args: &[&str]) -> String {
    let mut parts: Vec<String> = vec!["hyalo".to_owned()];
    for arg in args {
        parts.push(shell_quote(arg));
    }
    // Preserve rule/prefix/fix-rule filters from the original invocation.
    if let Some(rule) = &ctx.lint_rule
        && !args.contains(&"--rule")
    {
        parts.push("--rule".to_owned());
        parts.push(shell_quote(rule));
    }
    if let Some(prefix) = &ctx.lint_rule_prefix {
        parts.push("--rule-prefix".to_owned());
        parts.push(shell_quote(prefix));
    }
    for fr in &ctx.lint_fix_rules {
        parts.push("--fix-rule".to_owned());
        parts.push(shell_quote(fr));
    }
    push_global_flags(&mut parts, ctx);
    for glob in &ctx.glob {
        parts.push("--glob".to_owned());
        parts.push(shell_quote(glob));
    }
    for ft in &ctx.file_targets {
        parts.push(shell_quote(ft));
    }
    parts.join(" ")
}

/// Accumulate rule violation counts from a named array field in a file JSON object.
fn accumulate_rule_groups(
    file: &serde_json::Value,
    key: &str,
    totals: &mut std::collections::HashMap<String, usize>,
) {
    if let Some(groups) = file.get(key).and_then(|rg| rg.as_array()) {
        for group in groups {
            if let (Some(rule), Some(count)) = (
                group.get("rule").and_then(serde_json::Value::as_str),
                group.get("count").and_then(serde_json::Value::as_u64),
            ) {
                *totals.entry(rule.to_owned()).or_default() +=
                    usize::try_from(count).unwrap_or(usize::MAX);
            }
        }
    }
}

/// Collect per-rule violation counts across all files, scanning both `rule_groups` (read-only)
/// and `fixed_groups` + `remaining_groups` (fix-mode).
fn collect_rule_totals(data: &serde_json::Value) -> std::collections::HashMap<String, usize> {
    let mut totals: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let Some(files) = data.get("files").and_then(|f| f.as_array()) else {
        return totals;
    };
    for file in files {
        // Read-only shape.
        accumulate_rule_groups(file, "rule_groups", &mut totals);
        // Fix-mode shapes (count all including fixed for dominance analysis).
        accumulate_rule_groups(file, "remaining_groups", &mut totals);
        accumulate_rule_groups(file, "fixed_groups", &mut totals);
    }
    totals
}

fn hints_for_lint(ctx: &HintContext, data: &serde_json::Value, _total: Option<u64>) -> Vec<Hint> {
    let mut hints: Vec<Hint> = Vec::new();

    let is_fix_mode = ctx.lint_is_fix;
    let is_dry_run = ctx.dry_run
        || data
            .get("dry_run")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

    // -----------------------------------------------------------------------
    // Show-all hint when output is truncated.
    // -----------------------------------------------------------------------
    let is_limited = data
        .get("files_truncated")
        .or_else(|| data.get("limited"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if !ctx.has_limit && is_limited {
        let total_violations = data
            .get("files_with_violations")
            .or_else(|| data.get("files_with_issues"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        hints.push(Hint::new(
            format!("Show all {total_violations} files with issues (no limit)"),
            build_command_with_glob_and_files(ctx, &["lint", "--limit", "0"]),
        ));
    }

    // -----------------------------------------------------------------------
    // UX-7: Smart fix/dry-run hints.
    // -----------------------------------------------------------------------
    if !is_fix_mode {
        // Not in fix mode: suggest preview (don't suggest apply directly).
        let has_violations = data
            .get("files")
            .and_then(|f| f.as_array())
            .is_some_and(|files| {
                files.iter().any(|file| {
                    file.get("rule_groups")
                        .and_then(|rg| rg.as_array())
                        .is_some_and(|groups| {
                            groups.iter().any(|g| {
                                g.get("autofixable")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(false)
                            })
                        })
                        || file
                            .get("violations")
                            .and_then(|v| v.as_array())
                            .is_some_and(|v| !v.is_empty())
                })
            });
        if has_violations && hints.len() < MAX_HINTS {
            // Preserve --rule / --rule-prefix / --fix-rule from the current
            // invocation so the suggested preview doesn't widen scope.
            hints.push(Hint::new(
                "Preview auto-fixes",
                build_lint_with_filter_flags(ctx, &["lint", "--fix", "--dry-run"]),
            ));
        }
    } else if is_dry_run {
        // Dry-run: if there are fixes that would be applied, suggest actually applying.
        let total_fixed = data
            .get("total_fixed")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        // Also check old-shape `fixes` for backward compat.
        let has_fixes = total_fixed > 0
            || data
                .get("fixes")
                .and_then(|f| f.as_array())
                .is_some_and(|a| !a.is_empty());
        if has_fixes && hints.len() < MAX_HINTS {
            // Apply hint mirrors the previewed scope — preserve --rule,
            // --rule-prefix, and --fix-rule via the lint-aware builder.
            hints.push(Hint::new(
                "Apply auto-fixes",
                build_lint_with_filter_flags(ctx, &["lint", "--fix"]),
            ));
        }
    }
    // If is_fix_mode && !is_dry_run: don't suggest fix hints (already applied).

    // -----------------------------------------------------------------------
    // UX-1: per-rule hints for HYALO001 and HYALO002.
    // -----------------------------------------------------------------------
    // Find the "worst-offender" file (first in files array — already sorted by total desc).
    let worst_file = data
        .get("files")
        .and_then(|f| f.as_array())
        .and_then(|arr| arr.first())
        .and_then(|f| f.get("file"))
        .and_then(serde_json::Value::as_str);

    let rule_totals = collect_rule_totals(data);
    let mut per_rule_hint_rules: Vec<String> = Vec::new();
    for rule_id in &["HYALO001", "HYALO002"] {
        if rule_totals.get(*rule_id).is_some_and(|&c| c > 0) {
            per_rule_hint_rules.push(rule_id.to_string());
        }
    }
    for rule_id in &per_rule_hint_rules {
        if hints.len() >= MAX_HINTS {
            break;
        }
        // De-dupe: don't add if we already have a hint with this rule.
        let already = hints.iter().any(|h| h.cmd.contains(rule_id.as_str()));
        if already {
            continue;
        }
        if let Some(hint) = per_rule_hint(ctx, rule_id, worst_file) {
            hints.push(hint);
        }
    }

    // -----------------------------------------------------------------------
    // UX-2: rule dominance hint.
    // -----------------------------------------------------------------------
    let grand_total: usize = rule_totals.values().sum();
    if grand_total > 0
        && hints.len() < MAX_HINTS
        && let Some((dominant_rule, dominant_count_ref)) =
            rule_totals.iter().max_by_key(|(_, c)| *c)
    {
        let dominant_count = *dominant_count_ref;
        #[allow(clippy::cast_precision_loss)]
        let ratio = dominant_count as f64 / grand_total as f64;
        if ratio >= RULE_DOMINANCE_RATIO && dominant_count >= RULE_DOMINANCE_MIN {
            let already = hints.iter().any(|h| {
                h.cmd.contains("lint-rules show") && h.cmd.contains(dominant_rule.as_str())
            });
            if !already {
                hints.push(Hint::new(
                    format!("Tune {dominant_rule} if too noisy on this KB"),
                    build_command_no_glob(ctx, &["lint-rules", "show", dominant_rule]),
                ));
            }
        }
    }

    // -----------------------------------------------------------------------
    // Parse-error hint.
    // -----------------------------------------------------------------------
    let has_parse_errors = data
        .get("files")
        .and_then(|f| f.as_array())
        .is_some_and(|files| {
            files.iter().any(|file| {
                file.get("rule_groups")
                    .and_then(|rg| rg.as_array())
                    .is_some_and(|groups| {
                        groups.iter().any(|g| {
                            g.get("violations")
                                .and_then(|v| v.as_array())
                                .is_some_and(|vs| {
                                    vs.iter().any(|v| {
                                        v.get("message")
                                            .and_then(|m| m.as_str())
                                            .is_some_and(|m| m.starts_with(PARSE_ERROR_PREFIX))
                                    })
                                })
                        })
                    })
                    || file
                        .get("violations")
                        .and_then(|v| v.as_array())
                        .is_some_and(|v| {
                            v.iter().any(|violation| {
                                violation
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .is_some_and(|m| m.starts_with(PARSE_ERROR_PREFIX))
                            })
                        })
            })
        });
    if has_parse_errors && hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            "Show all files with unfixable frontmatter errors",
            build_command_with_glob_and_files(ctx, &["lint", "--limit", "0"]),
        ));
    }

    // -----------------------------------------------------------------------
    // SCHEMA → `types show <T>` (iter-143). Surface a per-type hint when
    // SCHEMA violations land on files that declared a `type:`. Skip when
    // the user is already focused on schema rules (--rule SCHEMA or
    // --rule-prefix HYALO).
    // -----------------------------------------------------------------------
    let already_schema_focused = ctx.lint_rule.as_deref() == Some("SCHEMA")
        || ctx
            .lint_rule_prefix
            .as_deref()
            .is_some_and(|p| p.starts_with("HYALO"));
    if !already_schema_focused && let Some(files) = data.get("files").and_then(|f| f.as_array()) {
        // Collect distinct types that have at least one SCHEMA violation.
        // Preserve first-seen order; cap at 2 distinct types to avoid noise.
        //
        // Inspect both the read-only mode shape (`rule_groups`) and the
        // fix-mode shape (`remaining_groups`) so the hint fires regardless
        // of which lint mode produced the output.
        let mut schema_types: Vec<String> = Vec::new();
        for file in files {
            let has_schema_in = |key: &str| {
                file.get(key)
                    .and_then(|rg| rg.as_array())
                    .is_some_and(|groups| {
                        groups.iter().any(|g| {
                            g.get("rule").and_then(serde_json::Value::as_str) == Some("SCHEMA")
                        })
                    })
            };
            if !has_schema_in("rule_groups") && !has_schema_in("remaining_groups") {
                continue;
            }
            let Some(t) = file.get("type").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !schema_types.iter().any(|x| x == t) {
                schema_types.push(t.to_owned());
            }
            if schema_types.len() >= 2 {
                break;
            }
        }
        for t in &schema_types {
            if hints.len() >= MAX_HINTS {
                break;
            }
            hints.push(Hint::new(
                format!("Show schema for type: {t}"),
                build_command_no_glob(ctx, &["types", "show", t]),
            ));
        }
    }

    // -----------------------------------------------------------------------
    // Always suggest listing defined types.
    // -----------------------------------------------------------------------
    if hints.len() < MAX_HINTS {
        hints.push(Hint::new(
            "See defined type schemas",
            build_command_no_glob(ctx, &["types", "list"]),
        ));
    }

    hints
}

fn hints_for_types(ctx: &HintContext, data: &serde_json::Value) -> Vec<Hint> {
    let subcommand = match &ctx.source {
        HintSource::Types { subcommand } => subcommand.as_deref().unwrap_or("list"),
        _ => "list",
    };

    let mut hints = Vec::new();

    match subcommand {
        "list" => {
            // Suggest showing the first listed type.
            if let Some(first_type) = data
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|entry| entry.get("type"))
                .and_then(serde_json::Value::as_str)
            {
                hints.push(Hint::new(
                    format!("Show schema for type: {first_type}"),
                    build_command_no_glob(ctx, &["types", "show", first_type]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate all files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
        }
        "show" => {
            let type_name = data.get("type").and_then(serde_json::Value::as_str);
            // Suggest scaffolding a new file of this type when the type
            // declares any `required` properties. Without required fields,
            // `hyalo new` would only emit a `type:` stub — low value.
            let has_required = data
                .get("required")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|arr| !arr.is_empty());
            if let Some(name) = type_name
                && has_required
                && hints.len() < MAX_HINTS
            {
                let placeholder = format!("path/to/new-{name}.md");
                hints.push(Hint::new(
                    format!("Scaffold a new file of type: {name}"),
                    build_command_no_glob(ctx, &["new", "--type", name, "--file", &placeholder]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "List all type schemas",
                    build_command_no_glob(ctx, &["types", "list"]),
                ));
            }
            if let Some(name) = type_name
                && hints.len() < MAX_HINTS
            {
                let filter = format!("type={name}");
                hints.push(Hint::new(
                    format!("Find files of type: {name}"),
                    build_command_no_glob(ctx, &["find", "--property", &filter]),
                ));
            }
        }
        "set" => {
            let type_name = data.get("type").and_then(serde_json::Value::as_str);
            if let Some(name) = type_name
                && hints.len() < MAX_HINTS
            {
                hints.push(Hint::new(
                    format!("Review updated schema: {name}"),
                    build_command_no_glob(ctx, &["types", "show", name]),
                ));
            }
            if hints.len() < MAX_HINTS {
                hints.push(Hint::new(
                    "Validate files against schema",
                    build_command_no_glob(ctx, &["lint"]),
                ));
            }
        }
        _ => {}
    }

    hints
}

fn hints_for_new(ctx: &HintContext, file: &str) -> Vec<Hint> {
    vec![Hint::new(
        "Validate the new file and see placeholder violations",
        build_command_no_glob(ctx, &["lint", "--file", file]),
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
    fn lint_hints_suggest_fix_when_violations() {
        let c = ctx(HintSource::Lint);
        let data = json!({
            "files": [{"file": "test.md", "violations": [{"severity": "error", "message": "missing required property"}]}],
            "total": 1,
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
            "total": 0,
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
        let data = json!({"files": [], "total": 0});
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
            "total": 5,
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
            "errors": 0,
            "warnings": 0,
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
}
