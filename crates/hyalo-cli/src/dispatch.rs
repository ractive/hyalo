use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::args::{
    Commands, FindFilters, IndexFlags, LinksAction, LintRulesAction, PropertiesAction, TagsAction,
    TaskAction, TypesAction, ViewsAction, resolve_single_file,
};
use crate::commands::inputs::{ResolutionPolicy, ResolvedInputsOrOutcome, resolve_inputs};
use crate::commands::{
    IndexResolution, ResolvedIndex, append as append_commands, backlinks as backlinks_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, links as links_commands, lint as lint_commands,
    lint_rules as lint_rules_commands, mv as mv_commands, properties, read as read_commands,
    remove as remove_commands, resolve_index, set as set_commands, summary as summary_commands,
    tags as tag_commands, tasks as task_commands,
};
use crate::output::{CommandOutcome, Format};
use hyalo_core::bm25::parse_language;
use hyalo_core::case_index::{CaseInsensitiveIndex, CaseInsensitiveMode, mode_enabled};
use hyalo_core::filter;
use hyalo_core::index::{ScanOptions, SnapshotIndex, VaultIndex as _};
use hyalo_core::schema::SchemaConfig;

/// Default output limit for list commands when no `--limit` is passed and no
/// `default_limit` is set in `.hyalo.toml`.
pub(crate) const DEFAULT_OUTPUT_LIMIT: usize = 50;

/// Build a [`CaseInsensitiveIndex`] from a full vault directory scan.
///
/// The scan is always vault-wide — not scoped to any `--file` or `--glob`
/// argument — because case-insensitive link resolution must find *any* file
/// in the vault, even files not included in the current query scope. A scoped
/// `VaultIndex` (built by `collect_files` when `--file` is used) would omit
/// the very link targets we need to resolve, so we re-walk from disk rather
/// than reusing the command's `VaultIndex`.
///
/// Errors during discovery are silently ignored (the index will just be less
/// complete, which degrades gracefully to no case-insensitive fallback).
pub(crate) fn build_case_index_from_dir(dir: &std::path::Path) -> CaseInsensitiveIndex {
    use hyalo_core::discovery;
    let mut idx = CaseInsensitiveIndex::new();
    if let Ok(files) = discovery::discover_files(dir) {
        for file in &files {
            let rel = discovery::relative_path(dir, file);
            idx.insert(&rel);
        }
    }
    idx
}

/// Build a [`CaseInsensitiveIndex`] from a full vault directory scan and set
/// case-insensitive path lookups according to `mode`. Always returns
/// `Some(index)` — the stem map (used for Obsidian short-form wikilink
/// resolution) is needed regardless of case-insensitive path mode, and the
/// index is cheap to build. The `Option` return type is kept for API
/// compatibility with the many call sites that historically expected `None`.
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn maybe_case_index(
    mode: CaseInsensitiveMode,
    dir: &std::path::Path,
) -> Option<CaseInsensitiveIndex> {
    let mut idx = build_case_index_from_dir(dir);
    idx.set_case_insensitive_paths(mode_enabled(mode, dir));
    Some(idx)
}

/// Shared context for command dispatch.
pub(crate) struct CommandContext<'a> {
    pub dir: &'a Path,
    /// The directory where `.hyalo.toml` was loaded from.  This is the
    /// project root when `dir` comes from `dir = "subdir"` in the config,
    /// or the `--dir` target when the user passes `--dir` explicitly.
    /// Views and types are stored in `config_dir/.hyalo.toml`.
    pub config_dir: &'a Path,
    /// The vault dir as configured (`config.dir.to_string_lossy()`).
    /// Used for `--files-from` prefix stripping in the unified resolver.
    pub configured_dir_str: &'a str,
    pub site_prefix: Option<&'a str>,
    /// Internal format — always Json; commands build JSON, pipeline handles conversion.
    pub effective_format: Format,
    /// The user-requested format (Text or Json). Used by `read` to decide between
    /// `RawOutput` (text mode) and `Success` (JSON mode).
    pub user_format: Format,
    pub snapshot_index: &'a mut Option<SnapshotIndex>,
    pub index_path: Option<&'a Path>,
    /// Default stemming language from `[search] language` in `.hyalo.toml`.
    pub config_language: Option<&'a str>,
    /// Frontmatter property names to scan for `[[wikilink]]` values in the link graph.
    /// Comes from `[links] frontmatter_properties` in `.hyalo.toml`. `None` = use defaults.
    pub frontmatter_link_props: Option<&'a [String]>,
    /// Parsed schema configuration from `[schema.*]` sections in `.hyalo.toml`.
    pub schema: &'a SchemaConfig,
    /// When `true`, schema validation runs on every `set`/`append` operation even
    /// without `--validate`. Comes from `validate_on_write = true` in `.hyalo.toml`.
    pub validate_on_write: bool,
    /// Vault-relative paths excluded from `hyalo lint`. From `[lint] ignore` in `.hyalo.toml`.
    pub lint_ignore: &'a [String],
    /// Markdown lint configuration from `[lint]` in `.hyalo.toml`.
    pub md_lint: &'a hyalo_mdlint::LintConfig,
    /// Case-insensitive link resolution mode from `[links] case_insensitive`.
    pub case_insensitive_mode: CaseInsensitiveMode,
    /// Optional exit code override set by commands that need a non-0/2 exit code
    /// (e.g. `lint` returns 1 when errors are found). The output pipeline uses this
    /// to override its own exit code calculation.
    pub exit_code_override: Option<i32>,
    /// Default output limit from `.hyalo.toml` (`default_limit`).
    /// `None` = use `DEFAULT_OUTPUT_LIMIT`.
    /// `Some(0)` = unlimited.
    /// `Some(n)` = limit to n.
    pub config_default_limit: Option<usize>,
    /// When true, the output is consumed programmatically (`--jq` or `--count`),
    /// so the default limit should not apply — only an explicit `--limit` is honoured.
    pub programmatic_output: bool,
    /// Strict schema validation mode from `[lint] strict = true` in `.hyalo.toml`.
    /// When `true`, "no 'type' property" and "undeclared property" warnings are
    /// promoted to errors, and lint exits non-zero on them.
    pub lint_strict: bool,
    /// `--files-from` counters captured during dispatch for commands that resolve
    /// `--files-from` inside `resolve_inputs` (read/backlinks/task). Surfaced by
    /// the output pipeline as `files_from_counters` in the envelope.
    pub files_from_counters: Option<crate::commands::files_from::FilesFromCounters>,
}

/// Resolve the effective limit for a list command.
///
/// Precedence (highest first):
/// 1. `cli_limit = Some(n)` — user passed `--limit n` (0 = unlimited → returns `None`)
/// 2. If `programmatic` is true (`--jq` or `--count`), skip the default limit — the
///    output is consumed by a pipeline that needs complete results.
/// 3. `config_default` = `Some(n)` from `.hyalo.toml` (0 = unlimited → returns `None`)
/// 4. `DEFAULT_OUTPUT_LIMIT` — hard-coded fallback
///
/// Returns `None` for unlimited, `Some(n)` for an effective cap.
fn resolve_limit(
    cli_limit: Option<usize>,
    config_default: Option<usize>,
    programmatic: bool,
) -> Option<usize> {
    match cli_limit {
        Some(0) => None, // explicit --limit 0 = unlimited
        Some(n) => Some(n),
        None => {
            if programmatic {
                return None;
            }
            match config_default {
                Some(0) => None, // config default_limit = 0 = unlimited
                Some(n) => Some(n),
                None => Some(DEFAULT_OUTPUT_LIMIT),
            }
        }
    }
}

/// Public wrapper for [`patch_index_for_modified_files`] used by the body-lint pass.
pub(crate) fn patch_index_for_modified_files_pub(
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dir: &Path,
    modified_files: &[String],
) -> Result<()> {
    patch_index_for_modified_files(snapshot_index, index_path, dir, modified_files)
}

/// Convert a legacy [`lint_commands::FileLintResult`] (frontmatter/view violations, old shape)
/// into an [`lint_commands::ExtFileLintResult`] (new rule_groups shape).
///
/// View violations are grouped under the synthetic rule id `SCHEMA`.
fn adapt_view_result_to_ext(
    result: &lint_commands::FileLintResult,
) -> lint_commands::ExtFileLintResult {
    let violations: Vec<lint_commands::BodyViolation> = result
        .violations
        .iter()
        .map(|v| lint_commands::BodyViolation {
            line: 0,
            column: 0,
            message: v.message.clone(),
            fix: None,
        })
        .collect();

    let total = violations.len();
    let rule_groups = if total == 0 {
        vec![]
    } else {
        vec![lint_commands::RuleGroup {
            rule: "SCHEMA".to_string(),
            count: total,
            shown: total,
            truncated: false,
            severity: "warn".to_string(),
            autofixable: false,
            violations,
        }]
    };

    lint_commands::ExtFileLintResult {
        file: result.file.clone(),
        doc_type: None,
        rule_groups,
    }
}

/// Inject an [`lint_commands::ExtFileLintResult`] into the serialized
/// [`lint_commands::ExtLintOutput`] stored inside a `CommandOutcome`.
///
/// Deserializes the JSON, prepends the new file result, updates `files_with_violations`
/// and `total`, then re-serializes.
fn inject_ext_file_result(
    outcome: CommandOutcome,
    extra: &lint_commands::ExtFileLintResult,
) -> Result<CommandOutcome> {
    let (payload, total_count) = match outcome {
        CommandOutcome::Success { output, total } => (output, total),
        other => return Ok(other),
    };

    let mut value: serde_json::Value =
        serde_json::from_str(&payload).context("failed to re-parse extended lint output JSON")?;

    if let Some(obj) = value.as_object_mut() {
        let extra_violations: usize = extra.rule_groups.iter().map(|g| g.count).sum();
        let is_fix_mode = obj.contains_key("total_remaining");

        // In fix-mode, the per-file shape is `ExtFileLintFixResult` (with
        // `fixed_groups`/`remaining_groups`/`conflicts`), not the read-only
        // `ExtFileLintResult` shape. Adapt before injecting so the renderer
        // and JSON consumers see consistent structure.
        let extra_value = if is_fix_mode {
            let remaining_groups = serde_json::to_value(&extra.rule_groups)
                .context("failed to serialize view lint groups")?;
            serde_json::json!({
                "file": extra.file,
                "fixed_groups": serde_json::Value::Array(Vec::new()),
                "remaining_groups": remaining_groups,
                "conflicts": serde_json::Value::Array(Vec::new()),
            })
        } else {
            serde_json::to_value(extra).context("failed to serialize view lint result")?
        };

        if let Some(files) = obj.get_mut("files").and_then(|f| f.as_array_mut()) {
            files.insert(0, extra_value);
        }
        // Read-only shape has `total`.
        if let Some(n) = obj.get_mut("total").and_then(|v| v.as_u64()) {
            obj.insert(
                "total".to_string(),
                serde_json::Value::from(n + extra_violations as u64),
            );
        }
        // Fix-mode shape has `total_remaining`.
        if let Some(n) = obj.get_mut("total_remaining").and_then(|v| v.as_u64()) {
            obj.insert(
                "total_remaining".to_string(),
                serde_json::Value::from(n + extra_violations as u64),
            );
        }
        if extra_violations > 0
            && let Some(n) = obj
                .get_mut("files_with_violations")
                .and_then(|v| v.as_u64())
        {
            obj.insert(
                "files_with_violations".to_string(),
                serde_json::Value::from(n + 1),
            );
        }
        // Bump severity totals so the summary stays consistent with the
        // injected groups. View violations are categorised by `severity`
        // ("error" or "warn") on each rule group.
        let mut extra_errors: u64 = 0;
        let mut extra_warnings: u64 = 0;
        for g in &extra.rule_groups {
            let n = g.count as u64;
            match g.severity.as_str() {
                "error" => extra_errors += n,
                _ => extra_warnings += n,
            }
        }
        if extra_errors > 0
            && let Some(n) = obj.get_mut("errors").and_then(|v| v.as_u64())
        {
            obj.insert(
                "errors".to_string(),
                serde_json::Value::from(n + extra_errors),
            );
        }
        if extra_warnings > 0
            && let Some(n) = obj.get_mut("warnings").and_then(|v| v.as_u64())
        {
            obj.insert(
                "warnings".to_string(),
                serde_json::Value::from(n + extra_warnings),
            );
        }
    }

    let extra_violations: usize = extra.rule_groups.iter().map(|g| g.count).sum();
    let bump_total = extra_violations > 0;
    let new_payload = crate::output::format_success(crate::output::Format::Json, &value);
    Ok(match total_count {
        Some(t) => {
            CommandOutcome::success_with_total(new_payload, if bump_total { t + 1 } else { t })
        }
        None => CommandOutcome::success(new_payload),
    })
}

/// Patch the snapshot index for a list of vault-relative paths that were
/// modified on disk.  Uses `refresh_entry` to fully re-scan each file
/// (properties, tags, links, sections, tasks), then flushes to disk once.
fn patch_index_for_modified_files(
    snapshot_index: &mut Option<SnapshotIndex>,
    index_path: Option<&Path>,
    dir: &Path,
    modified_files: &[String],
) -> Result<()> {
    if modified_files.is_empty() {
        return Ok(());
    }
    let Some(idx) = snapshot_index.as_mut() else {
        return Ok(());
    };
    let mut dirty = false;
    for rel in modified_files {
        match idx.refresh_entry(dir, rel) {
            Ok(true) => dirty = true,
            Ok(false) => {} // not in index, nothing to update
            Err(e) => {
                eprintln!("warning: could not refresh index entry for {rel}: {e:#}");
            }
        }
    }
    crate::commands::mutation::save_index_if_dirty(snapshot_index, index_path, dirty)
}

/// Parse `--where-property` filters and validate `--where-tag` names.
/// Returns an error string on invalid input.
fn parse_where_filters(
    where_properties: &[String],
    where_tags: &[String],
) -> Result<Vec<filter::PropertyFilter>, String> {
    let filters = where_properties
        .iter()
        .map(|s| filter::parse_property_filter(s))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    for tag in where_tags {
        crate::commands::tags::validate_tag(tag)?;
    }
    Ok(filters)
}

pub(crate) fn dispatch(command: Commands, ctx: &mut CommandContext<'_>) -> Result<CommandOutcome> {
    let dir = ctx.dir;
    let site_prefix = ctx.site_prefix;
    let effective_format = ctx.effective_format;
    let snapshot_index = &mut *ctx.snapshot_index;
    let index_path = ctx.index_path;

    match command {
        Commands::Find {
            pattern,
            file_positional,
            view: _, // resolved before dispatch
            filters: mut filters_raw,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // Merge positional files into filters (clap prevents positional+--file
            // and positional+--glob at parse time; a view may have set glob though).
            if !file_positional.is_empty() {
                if !filters_raw.glob.is_empty() {
                    crate::warn::warn(
                        "positional file arguments override the view's --glob; \
                         glob filter has been ignored",
                    );
                }
                filters_raw.file = file_positional;
                filters_raw.glob.clear(); // file overrides view's glob
            }
            let FindFilters {
                pattern: _, // pattern is handled in run.rs before dispatch
                regexp,
                properties,
                tag,
                task,
                sections,
                file,
                glob,
                fields,
                sort,
                reverse,
                limit,
                broken_links,
                orphan,
                dead_end,
                title,
                language,
                files_from: _, // resolved in run.rs before dispatch
            } = filters_raw;
            if orphan && dead_end {
                crate::warn::warn(
                    "--orphan and --dead-end are mutually exclusive (no file can be both); results will always be empty",
                );
            }
            // Parse property filters
            let prop_filters: Vec<filter::PropertyFilter> = match properties
                .iter()
                .map(|s| filter::parse_property_filter(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };
            // Parse task filter
            let task_filter = match task.as_deref().map(filter::parse_task_filter) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
                None => None,
            };
            // Parse fields
            let parsed_fields = match filter::Fields::parse(&fields) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };
            // Parse sort
            let sort_field = match sort.as_deref().map(filter::parse_sort) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
                None => None,
            };
            // Parse section filters
            let section_filters: Vec<hyalo_core::heading::SectionFilter> = match sections
                .iter()
                .map(|s| hyalo_core::heading::SectionFilter::parse(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };

            for t in &tag {
                if let Err(msg) = crate::commands::tags::validate_tag(t) {
                    return Ok(CommandOutcome::UserError(format!("Error: {msg}")));
                }
            }

            // Validate --language flag and config language against supported languages.
            if let Some(ref lang) = language
                && let Err(e) = parse_language(lang)
            {
                return Ok(CommandOutcome::UserError(format!(
                    "invalid --language value {lang:?}: {e}"
                )));
            }
            if let Some(cfg_lang) = ctx.config_language
                && let Err(e) = parse_language(cfg_lang)
            {
                return Ok(CommandOutcome::UserError(format!(
                    "invalid [search].language config value {cfg_lang:?}: {e}"
                )));
            }

            // Strip the dir prefix from --file args so that
            // filter_index_entries matches vault-relative paths.
            let file: Vec<String> = file
                .into_iter()
                .map(|f| hyalo_core::discovery::strip_dir_prefix(dir, &f).unwrap_or(f))
                .collect();

            let sort_needs_backlinks =
                matches!(sort_field.as_ref(), Some(filter::SortField::BacklinksCount));
            let sort_needs_links =
                matches!(sort_field.as_ref(), Some(filter::SortField::LinksCount));
            let sort_needs_title = matches!(sort_field.as_ref(), Some(filter::SortField::Title));
            let has_task_filter = task_filter.is_some();
            let has_section_filter = !section_filters.is_empty();
            let has_title_filter = title.is_some();
            // BM25 pattern search requires reading file bodies for each candidate.
            let has_bm25_search = pattern.is_some();
            let needs_body =
                find_commands::needs_body(&parsed_fields, has_task_filter, has_section_filter)
                    || sort_needs_links
                    || sort_needs_title
                    || broken_links
                    || orphan
                    || dead_end
                    || has_title_filter
                    || has_bm25_search;
            let needs_full_vault =
                parsed_fields.backlinks || sort_needs_backlinks || orphan || dead_end;
            // The link graph is only built when scan_body is true, so
            // backlinks / backlink-sort always require body scanning.
            let scan_body = needs_body || needs_full_vault;
            match resolve_index(
                snapshot_index.as_ref(),
                dir,
                &file,
                &glob,
                effective_format,
                site_prefix,
                needs_full_vault,
                &ScanOptions {
                    scan_body,
                    bm25_tokenize: false,
                    default_language: None,
                    frontmatter_link_props: ctx.frontmatter_link_props,
                },
            )? {
                IndexResolution::Resolved(resolved) => {
                    let ci = maybe_case_index(ctx.case_insensitive_mode, dir);
                    find_commands::find(
                        resolved.as_index(),
                        dir,
                        site_prefix,
                        pattern.as_deref(),
                        regexp.as_deref(),
                        &prop_filters,
                        &tag,
                        task_filter.as_ref(),
                        &section_filters,
                        &file,
                        &glob,
                        &parsed_fields,
                        sort_field.as_ref(),
                        reverse,
                        resolve_limit(limit, ctx.config_default_limit, ctx.programmatic_output),
                        broken_links,
                        orphan,
                        dead_end,
                        title.as_deref(),
                        effective_format,
                        language.as_deref(),
                        ctx.config_language,
                        ci.as_ref(),
                    )
                }
                IndexResolution::Outcome(outcome) => Ok(outcome),
            }
        }
        Commands::Read {
            selection,
            section,
            lines,
            frontmatter,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            match resolve_inputs(
                &selection,
                dir,
                ctx.configured_dir_str,
                snapshot_index.as_ref(),
                &ResolutionPolicy::Single { allow_glob: false },
                effective_format,
            )? {
                ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                ResolvedInputsOrOutcome::Resolved(r) => {
                    ctx.files_from_counters = r.counters;
                    let (_full, file) = r
                        .files
                        .into_iter()
                        .next()
                        .context("Single resolution returned no files")?;
                    read_commands::run(
                        dir,
                        &file,
                        section.as_deref(),
                        lines.as_deref(),
                        frontmatter,
                        effective_format,
                        ctx.user_format,
                    )
                }
            }
        }
        Commands::Properties { action } => {
            let action = action.unwrap_or(PropertiesAction::Summary {
                glob: vec![],
                limit: None,
                index_flags: IndexFlags::default(),
            });
            match action {
                PropertiesAction::Summary {
                    ref glob,
                    limit: cli_limit,
                    index_flags: _, // consumed in run.rs before dispatch
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(ResolvedIndex::Snapshot(idx)) => {
                        let filtered =
                            find_commands::filter_index_entries(idx.entries(), &[], glob);
                        match filtered {
                            Err(e) => Err(e),
                            Ok(filtered) => {
                                let paths: Vec<String> =
                                    filtered.iter().map(|e| e.rel_path.clone()).collect();
                                let file_filter = if glob.is_empty() {
                                    None
                                } else {
                                    Some(paths.as_slice())
                                };
                                properties::properties_summary(
                                    idx,
                                    file_filter,
                                    effective_format,
                                    resolve_limit(
                                        cli_limit,
                                        ctx.config_default_limit,
                                        ctx.programmatic_output,
                                    ),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        properties::properties_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                PropertiesAction::Rename {
                    from,
                    to,
                    glob,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => properties::properties_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
                    dry_run,
                    effective_format,
                    snapshot_index,
                    index_path,
                ),
            }
        }
        Commands::Tags { action } => {
            let action = action.unwrap_or(TagsAction::Summary {
                glob: vec![],
                limit: None,
                index_flags: IndexFlags::default(),
            });
            match action {
                TagsAction::Summary {
                    ref glob,
                    limit: cli_limit,
                    index_flags: _, // consumed in run.rs before dispatch
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(ResolvedIndex::Snapshot(idx)) => {
                        let filtered =
                            find_commands::filter_index_entries(idx.entries(), &[], glob);
                        match filtered {
                            Err(e) => Err(e),
                            Ok(filtered) => {
                                let paths: Vec<String> =
                                    filtered.iter().map(|e| e.rel_path.clone()).collect();
                                let file_filter = if glob.is_empty() {
                                    None
                                } else {
                                    Some(paths.as_slice())
                                };
                                tag_commands::tags_summary(
                                    idx,
                                    file_filter,
                                    effective_format,
                                    resolve_limit(
                                        cli_limit,
                                        ctx.config_default_limit,
                                        ctx.programmatic_output,
                                    ),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        tag_commands::tags_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                TagsAction::Rename {
                    from,
                    to,
                    glob,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => tag_commands::tags_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
                    dry_run,
                    effective_format,
                    snapshot_index,
                    index_path,
                ),
            }
        }
        Commands::Task { action } => {
            match action {
                TaskAction::Read {
                    selection,
                    line,
                    section,
                    all,
                    index_flags: _, // consumed in run.rs before dispatch
                } => {
                    let configured_dir = ctx.configured_dir_str;
                    match resolve_inputs(
                        &selection,
                        dir,
                        configured_dir,
                        snapshot_index.as_ref(),
                        &ResolutionPolicy::Single { allow_glob: false },
                        effective_format,
                    )? {
                        ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                        ResolvedInputsOrOutcome::Resolved(r) => {
                            ctx.files_from_counters = r.counters;
                            let (_full, file) = r
                                .files
                                .into_iter()
                                .next()
                                .context("Single resolution returned no files")?;
                            task_commands::task_read(
                                dir,
                                &file,
                                &line,
                                section.as_deref(),
                                all,
                                effective_format,
                            )
                        }
                    }
                }
                TaskAction::Toggle {
                    selection,
                    line,
                    section,
                    all,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => {
                    if selection.files_from.is_some() && !all && section.is_none() {
                        let out = crate::output::format_error(
                            effective_format,
                            "--files-from requires --all or --section",
                            None,
                            Some(
                                "try: --files-from <list> --all   or   --files-from <list> --section <heading>",
                            ),
                            Some("--line is per-file and cannot compose with --files-from"),
                        );
                        return Ok(CommandOutcome::UserError(out));
                    }
                    let configured_dir = ctx.configured_dir_str;
                    match resolve_inputs(
                        &selection,
                        dir,
                        configured_dir,
                        snapshot_index.as_ref(),
                        &ResolutionPolicy::SingleOrMany,
                        effective_format,
                    )? {
                        ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                        ResolvedInputsOrOutcome::Resolved(r) => {
                            ctx.files_from_counters.clone_from(&r.counters);
                            if r.files.len() == 1 {
                                // Single file: delegate directly — no wrapping.
                                let (_full_path, rel) = &r.files[0];
                                task_commands::task_toggle(
                                    dir,
                                    rel,
                                    &line,
                                    section.as_deref(),
                                    all,
                                    effective_format,
                                    snapshot_index,
                                    index_path,
                                    dry_run,
                                )
                            } else {
                                // Multi-file: collect each file's raw results into a
                                // flat array and let the pipeline wrap it in the
                                // standard `{"results": [...], "total": N}` envelope.
                                // `total` matches the flattened item count (consistent
                                // with other list-shaped outputs and `--count`).
                                let mut flat: Vec<serde_json::Value> = Vec::new();
                                for (_full_path, rel) in &r.files {
                                    let outcome = task_commands::task_toggle(
                                        dir,
                                        rel,
                                        &line,
                                        section.as_deref(),
                                        all,
                                        effective_format,
                                        snapshot_index,
                                        index_path,
                                        dry_run,
                                    )?;
                                    match outcome {
                                        CommandOutcome::Success { output, .. } => {
                                            let val: serde_json::Value =
                                                serde_json::from_str(&output)
                                                    .unwrap_or(serde_json::Value::Null);
                                            match val {
                                                serde_json::Value::Array(items) => {
                                                    flat.extend(items);
                                                }
                                                other => flat.push(other),
                                            }
                                        }
                                        other => return Ok(other),
                                    }
                                }
                                let total = flat.len() as u64;
                                let output = serde_json::to_string(&flat)
                                    .context("failed to serialize multi-file task toggle output")?;
                                Ok(CommandOutcome::success_with_total(output, total))
                            }
                        }
                    }
                }
                TaskAction::Set {
                    selection,
                    line,
                    section,
                    all,
                    status,
                    dry_run,
                    index_flags: _, // consumed in run.rs before dispatch
                } => {
                    if selection.files_from.is_some() && !all && section.is_none() {
                        let out = crate::output::format_error(
                            effective_format,
                            "--files-from requires --all or --section",
                            None,
                            Some(
                                "try: --files-from <list> --all --status <c>   or   --files-from <list> --section <heading> --status <c>",
                            ),
                            Some("--line is per-file and cannot compose with --files-from"),
                        );
                        return Ok(CommandOutcome::UserError(out));
                    }
                    if status.chars().count() != 1 {
                        let out = crate::output::format_error(
                            effective_format,
                            "--status must be a single character",
                            None,
                            Some("example: --status '?' or --status '-'"),
                            None,
                        );
                        return Ok(CommandOutcome::UserError(out));
                    }
                    // chars().count() == 1 guarantees next() returns Some.
                    let ch = status
                        .chars()
                        .next()
                        .ok_or_else(|| anyhow::anyhow!("--status must be a single character"))?;

                    let configured_dir = ctx.configured_dir_str;
                    match resolve_inputs(
                        &selection,
                        dir,
                        configured_dir,
                        snapshot_index.as_ref(),
                        &ResolutionPolicy::SingleOrMany,
                        effective_format,
                    )? {
                        ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                        ResolvedInputsOrOutcome::Resolved(r) => {
                            ctx.files_from_counters.clone_from(&r.counters);
                            if r.files.len() == 1 {
                                // Single file: delegate directly — no wrapping.
                                let (_full_path, rel) = &r.files[0];
                                task_commands::task_set_status(
                                    dir,
                                    rel,
                                    &line,
                                    section.as_deref(),
                                    all,
                                    ch,
                                    effective_format,
                                    snapshot_index,
                                    index_path,
                                    dry_run,
                                )
                            } else {
                                // Multi-file: collect each file's raw results into a
                                // flat array and let the pipeline wrap it in the
                                // standard `{"results": [...], "total": N}` envelope.
                                // `total` matches the flattened item count.
                                let mut flat: Vec<serde_json::Value> = Vec::new();
                                for (_full_path, rel) in &r.files {
                                    let outcome = task_commands::task_set_status(
                                        dir,
                                        rel,
                                        &line,
                                        section.as_deref(),
                                        all,
                                        ch,
                                        effective_format,
                                        snapshot_index,
                                        index_path,
                                        dry_run,
                                    )?;
                                    match outcome {
                                        CommandOutcome::Success { output, .. } => {
                                            let val: serde_json::Value =
                                                serde_json::from_str(&output)
                                                    .unwrap_or(serde_json::Value::Null);
                                            match val {
                                                serde_json::Value::Array(items) => {
                                                    flat.extend(items);
                                                }
                                                other => flat.push(other),
                                            }
                                        }
                                        other => return Ok(other),
                                    }
                                }
                                let total = flat.len() as u64;
                                let output = serde_json::to_string(&flat)
                                    .context("failed to serialize multi-file task set output")?;
                                Ok(CommandOutcome::success_with_total(output, total))
                            }
                        }
                    }
                }
            }
        }
        Commands::Summary {
            glob,
            recent,
            depth,
            index_flags: _, // consumed in run.rs before dispatch
        } => match resolve_index(
            snapshot_index.as_ref(),
            dir,
            &[],
            &glob,
            effective_format,
            site_prefix,
            true,
            &ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
                frontmatter_link_props: ctx.frontmatter_link_props,
            },
        )? {
            IndexResolution::Resolved(resolved) => {
                let ci = maybe_case_index(ctx.case_insensitive_mode, dir);
                summary_commands::summary(
                    dir,
                    resolved.as_index(),
                    &glob,
                    recent,
                    depth,
                    site_prefix,
                    effective_format,
                    ctx.schema,
                    ci.as_ref(),
                )
            }
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        Commands::Set {
            file_positional,
            properties,
            tag,
            mut file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            validate,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            if !file_positional.is_empty() {
                file = file_positional;
            }
            let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };
            let do_validate = validate || ctx.validate_on_write;
            set_commands::set(
                dir,
                &properties,
                &tag,
                &file,
                &glob,
                &where_prop_filters,
                &where_tags,
                effective_format,
                snapshot_index,
                index_path,
                dry_run,
                do_validate,
                if do_validate { Some(ctx.schema) } else { None },
            )
        }
        Commands::Remove {
            file_positional,
            properties,
            tag,
            mut file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            if !file_positional.is_empty() {
                file = file_positional;
            }
            let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };
            remove_commands::remove(
                dir,
                &properties,
                &tag,
                &file,
                &glob,
                &where_prop_filters,
                &where_tags,
                effective_format,
                snapshot_index,
                index_path,
                dry_run,
            )
        }
        Commands::Append {
            file_positional,
            properties,
            mut file,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            where_properties,
            where_tags,
            dry_run,
            validate,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            if !file_positional.is_empty() {
                file = file_positional;
            }
            let where_prop_filters = match parse_where_filters(&where_properties, &where_tags) {
                Ok(f) => f,
                Err(e) => {
                    return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                }
            };
            let do_validate = validate || ctx.validate_on_write;
            append_commands::append(
                dir,
                &properties,
                &file,
                &glob,
                &where_prop_filters,
                &where_tags,
                effective_format,
                snapshot_index,
                index_path,
                dry_run,
                do_validate,
                if do_validate { Some(ctx.schema) } else { None },
            )
        }
        Commands::Backlinks {
            selection,
            limit: cli_limit,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            match resolve_inputs(
                &selection,
                dir,
                ctx.configured_dir_str,
                snapshot_index.as_ref(),
                &ResolutionPolicy::Single { allow_glob: false },
                effective_format,
            )? {
                ResolvedInputsOrOutcome::Outcome(o) => Ok(o),
                ResolvedInputsOrOutcome::Resolved(r) => {
                    ctx.files_from_counters = r.counters;
                    let (_full, file) = r
                        .files
                        .into_iter()
                        .next()
                        .context("Single resolution returned no files")?;
                    match resolve_index(
                        snapshot_index.as_ref(),
                        dir,
                        &[],
                        &[],
                        effective_format,
                        site_prefix,
                        true,
                        &ScanOptions {
                            scan_body: true,
                            bm25_tokenize: false,
                            default_language: None,
                            frontmatter_link_props: ctx.frontmatter_link_props,
                        },
                    )? {
                        IndexResolution::Resolved(resolved) => backlinks_commands::backlinks(
                            resolved.as_index(),
                            &file,
                            dir,
                            effective_format,
                            resolve_limit(
                                cli_limit,
                                ctx.config_default_limit,
                                ctx.programmatic_output,
                            ),
                        ),
                        IndexResolution::Outcome(outcome) => Ok(outcome),
                    }
                }
            }
        }
        Commands::Mv {
            file_positional,
            file,
            to,
            glob,
            files_from: _, // resolved in run.rs before dispatch
            properties,
            tag,
            r#type,
            dry_run,
            apply,
            on_conflict,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // Parse property filters for batch mode
            let prop_filters: Vec<hyalo_core::filter::PropertyFilter> = match properties
                .iter()
                .map(|s| hyalo_core::filter::parse_property_filter(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(f) => f,
                Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
            };
            // Build type filters as additional property filters (type=<value>)
            let type_filters: Vec<hyalo_core::filter::PropertyFilter> = {
                let mut tf = Vec::new();
                for t in &r#type {
                    match hyalo_core::filter::parse_property_filter(&format!("type={t}")) {
                        Ok(f) => tf.push(f),
                        Err(e) => return Ok(CommandOutcome::UserError(format!("Error: {e}"))),
                    }
                }
                tf
            };
            let all_prop_filters: Vec<hyalo_core::filter::PropertyFilter> =
                prop_filters.into_iter().chain(type_filters).collect();

            let has_selectors = !glob.is_empty() || !all_prop_filters.is_empty() || !tag.is_empty();
            let has_file = file_positional.is_some() || file.is_some();

            if !has_selectors && !has_file {
                return Ok(CommandOutcome::UserError(
                    "Error: no source selection provided: pass a FILE (single-file mode) or at least one of --glob/--property/--tag/--type (batch mode)".to_string(),
                ));
            }

            let is_batch = has_selectors;

            if is_batch {
                // Validate tag filters.
                for t in &tag {
                    if let Err(msg) = crate::commands::tags::validate_tag(t) {
                        return Ok(CommandOutcome::UserError(format!("Error: {msg}")));
                    }
                }

                // --dry-run and --apply are mutually exclusive (also enforced by clap).
                let effective_apply = apply && !dry_run;

                mv_commands::mv_batch(
                    dir,
                    file_positional.as_deref(),
                    file.as_deref(),
                    &glob,
                    &all_prop_filters,
                    &tag,
                    &to,
                    effective_apply,
                    &on_conflict,
                    effective_format,
                    site_prefix,
                    snapshot_index,
                    index_path,
                )
            } else {
                let file = match resolve_single_file(file_positional, file) {
                    Ok(f) => f,
                    Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
                };
                let effective_dry_run = dry_run;
                mv_commands::mv(
                    dir,
                    &file,
                    &to,
                    effective_dry_run,
                    effective_format,
                    site_prefix,
                    snapshot_index,
                    index_path,
                )
            }
        }
        Commands::CreateIndex {
            output,
            allow_outside_vault,
        } => create_index_commands::create_index(
            dir,
            site_prefix,
            output.as_deref(),
            effective_format,
            allow_outside_vault,
            ctx.config_language,
        ),
        Commands::DropIndex {
            path,
            allow_outside_vault,
        } => drop_index_commands::drop_index(
            dir,
            path.as_deref(),
            effective_format,
            allow_outside_vault,
        ),
        Commands::Links { action } => match action.unwrap_or(LinksAction::Fix {
            dry_run: true,
            apply: false,
            threshold: 0.8,
            glob: vec![],
            ignore_target: vec![],
            expand_short_form: false,
            index_flags: IndexFlags::default(),
        }) {
            LinksAction::Fix {
                dry_run: _,
                apply,
                threshold,
                glob,
                ignore_target,
                expand_short_form,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                // Scope the immutable borrow of snapshot_index (via resolve_index)
                // so we can borrow it mutably for index updates afterwards.
                let (outcome, modified_files) = match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    &[],
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions {
                        scan_body: true,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(resolved) => {
                        let ci = maybe_case_index(ctx.case_insensitive_mode, dir);
                        links_commands::links_fix(
                            resolved.as_index(),
                            dir,
                            site_prefix,
                            &glob,
                            !apply,
                            threshold,
                            &ignore_target,
                            effective_format,
                            ci.as_ref(),
                            expand_short_form,
                        )?
                    }
                    IndexResolution::Outcome(outcome) => (outcome, Vec::new()),
                };
                // resolved is dropped — safe to borrow snapshot_index mutably.
                patch_index_for_modified_files(snapshot_index, index_path, dir, &modified_files)?;
                Ok(outcome)
            }
            LinksAction::Auto {
                dry_run: _,
                apply,
                min_length,
                exclude_title,
                first_only,
                exclude_target_glob,
                file,
                glob,
                index_flags: _, // consumed in run.rs before dispatch
            } => {
                let (outcome, modified_files) = match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    &[],
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
                        frontmatter_link_props: ctx.frontmatter_link_props,
                    },
                )? {
                    IndexResolution::Resolved(resolved) => links_commands::links_auto(
                        resolved.as_index(),
                        dir,
                        apply,
                        min_length,
                        &exclude_title,
                        first_only,
                        &exclude_target_glob,
                        file.as_deref(),
                        &glob,
                        effective_format,
                    )?,
                    IndexResolution::Outcome(outcome) => (outcome, Vec::new()),
                };
                patch_index_for_modified_files(snapshot_index, index_path, dir, &modified_files)?;
                Ok(outcome)
            }
        },
        Commands::Lint {
            file_positional,
            file,
            glob,
            r#type: lint_type,
            files_from: _, // resolved in run.rs before dispatch
            fix,
            dry_run,
            limit: cli_limit,
            detailed,
            rule,
            rule_prefix,
            max_per_rule,
            fix_rule,
            strict: lint_strict_flag,
            index_flags: _, // consumed in run.rs before dispatch
        } => {
            // --strict flag wins over config value; config value is the fallback.
            let effective_strict = lint_strict_flag || ctx.lint_strict;
            // Resolve --type to a glob pattern from its filename_template.
            let type_glob: Option<String> = if let Some(type_name) = lint_type {
                use hyalo_core::filename_template::FilenameTemplate;
                match ctx.schema.types.get(&type_name) {
                    Some(ts) => match &ts.filename_template {
                        Some(template_str) => match FilenameTemplate::parse(template_str) {
                            Ok(tpl) => Some(tpl.to_glob()),
                            Err(e) => {
                                return Ok(crate::output::CommandOutcome::UserError(
                                    crate::output::format_error(
                                        ctx.user_format,
                                        &format!(
                                            "invalid filename_template for type '{type_name}': {e}"
                                        ),
                                        None,
                                        None,
                                        None,
                                    ),
                                ));
                            }
                        },
                        None => {
                            return Ok(crate::output::CommandOutcome::UserError(
                                crate::output::format_error(
                                    ctx.user_format,
                                    &format!("type '{type_name}' has no filename_template defined"),
                                    None,
                                    Some(
                                        "set one with: hyalo types set <name> --filename-template <pattern>",
                                    ),
                                    None,
                                ),
                            ));
                        }
                    },
                    None => {
                        return Ok(crate::output::CommandOutcome::UserError(
                            crate::output::format_error(
                                ctx.user_format,
                                &format!("unknown type '{type_name}'"),
                                None,
                                Some("run `hyalo types list` to see available types"),
                                None,
                            ),
                        ));
                    }
                }
            } else {
                None
            };

            // Build the file list. Positional arg is treated as a single --file.
            let mut files_arg: Vec<String> = file;
            if let Some(pos) = file_positional {
                files_arg.insert(0, pos);
            }
            // --type expands to a glob that overrides file/glob args.
            let effective_glob: Vec<String> = if let Some(g) = type_glob {
                vec![g]
            } else {
                glob
            };

            let file_pairs = match crate::commands::collect_files(
                dir,
                &files_arg,
                &effective_glob,
                ctx.user_format,
            )? {
                crate::commands::FilesOrOutcome::Files(f) => f,
                crate::commands::FilesOrOutcome::Outcome(o) => return Ok(o),
            };

            let fix_mode = if fix {
                if dry_run {
                    lint_commands::FixMode::DryRun
                } else {
                    lint_commands::FixMode::Apply
                }
            } else {
                lint_commands::FixMode::Off
            };

            // Filter out files matching `[lint] ignore` entries.
            //
            // Each entry is matched against the vault-relative path (with `/`
            // separators) as a glob: `vendor/**/*.md`, `legacy/known-bad.md`,
            // `templates/*.md`. An entry without glob meta-characters is matched
            // literally (exact path equality on the normalized path).
            let filtered_pairs: Vec<_> = if ctx.lint_ignore.is_empty() {
                file_pairs
            } else {
                use globset::{GlobBuilder, GlobSetBuilder};
                let mut builder = GlobSetBuilder::new();
                let mut build_failed = false;
                for pat in ctx.lint_ignore {
                    match GlobBuilder::new(pat)
                        .literal_separator(true)
                        .backslash_escape(true)
                        .build()
                    {
                        Ok(g) => {
                            builder.add(g);
                        }
                        Err(e) => {
                            crate::warn::warn(format!(
                                "invalid [lint] ignore pattern {pat:?}: {e}"
                            ));
                            build_failed = true;
                        }
                    }
                }
                let set = if build_failed {
                    None
                } else {
                    builder.build().ok()
                };
                match set {
                    Some(set) => file_pairs
                        .into_iter()
                        .filter(|(_, rel)| {
                            let norm = rel.replace('\\', "/");
                            !set.is_match(&norm)
                        })
                        .collect(),
                    // If building the set failed (warning already emitted above),
                    // fall back to no filtering rather than silently ignoring
                    // potentially relevant files.
                    None => file_pairs,
                }
            };

            // Decide which lint path to use: extended (body+frontmatter) or legacy.
            // The extended path is used whenever the new flags are active OR when the
            // engine is available.  We always use the extended path now.
            let md_engine = hyalo_mdlint::HyaloLintEngine::create()
                .map_err(|e| anyhow::anyhow!("failed to create lint engine: {e}"))?;

            let max_per_rule_eff =
                max_per_rule.unwrap_or_else(|| ctx.md_lint.max_violations_per_rule());
            // CLI --limit overrides the config max_files when provided.
            let max_files_eff = cli_limit.unwrap_or_else(|| ctx.md_lint.max_files());

            let mut ext_opts = lint_commands::ExtLintOptions {
                fix: fix_mode,
                detailed,
                rule_filter: rule.as_deref(),
                rule_prefix: rule_prefix.as_deref(),
                max_per_rule: max_per_rule_eff,
                max_files: max_files_eff,
                fix_rules: &fix_rule,
                snapshot_index,
                index_path,
                vault_dir: dir,
                strict: effective_strict,
            };

            let (outcome, mut counts) = lint_commands::lint_files_extended(
                &filtered_pairs,
                ctx.schema,
                &md_engine,
                ctx.md_lint,
                &mut ext_opts,
            )?;

            // Additional config-level lint: check view definitions.
            // NOTE: in the extended path, view violations are reported separately.
            // For now we keep the prepend behavior to maintain compatibility.
            let config_violations = lint_commands::validate_views(ctx.config_dir);
            let outcome = if let Some(view_result) = config_violations {
                for v in &view_result.violations {
                    match v.severity {
                        lint_commands::Severity::Error => counts.errors += 1,
                        lint_commands::Severity::Warn => counts.warnings += 1,
                    }
                }
                counts.files_with_issues += 1;
                // Adapt view result into the new shape — inject as a file with SCHEMA group.
                let adapted = adapt_view_result_to_ext(&view_result);
                inject_ext_file_result(outcome, &adapted)?
            } else {
                outcome
            };

            // Signal exit code 1 when errors remain after fixes (set before returning).
            if counts.errors > 0 {
                ctx.exit_code_override = Some(1);
            }

            Ok(outcome)
        }
        Commands::LintRules { action } => {
            let action = action.unwrap_or(LintRulesAction::List {
                enabled_only: false,
                disabled_only: false,
                rule_prefix: None,
            });
            let md_engine = hyalo_mdlint::HyaloLintEngine::create()
                .map_err(|e| anyhow::anyhow!("failed to create lint engine: {e}"))?;
            match action {
                LintRulesAction::List {
                    enabled_only,
                    disabled_only,
                    rule_prefix,
                } => Ok(lint_rules_commands::list_rules(
                    ctx.config_dir,
                    &md_engine,
                    ctx.md_lint,
                    ctx.schema,
                    enabled_only,
                    disabled_only,
                    rule_prefix.as_deref(),
                    effective_format,
                )),
                LintRulesAction::Show { rule_id } => Ok(lint_rules_commands::show_rule(
                    &rule_id,
                    &md_engine,
                    ctx.md_lint,
                    ctx.schema,
                    ctx.user_format,
                )),
                LintRulesAction::Set {
                    rule_id,
                    enabled,
                    severity,
                    dry_run,
                } => lint_rules_commands::set_rule(
                    ctx.config_dir,
                    &rule_id,
                    enabled,
                    severity.as_deref(),
                    dry_run,
                    &md_engine,
                    ctx.md_lint,
                    ctx.user_format,
                ),
                LintRulesAction::Remove { rule_id, dry_run } => lint_rules_commands::remove_rule(
                    ctx.config_dir,
                    &rule_id,
                    dry_run,
                    &md_engine,
                    ctx.md_lint,
                    ctx.user_format,
                ),
            }
        }
        // `Init`, `Deinit`, and `Completion` are handled as early returns before dispatch is called.
        Commands::Init { .. } => unreachable!("Init is dispatched before this match reached"),
        Commands::Deinit => unreachable!("Deinit is dispatched before this match reached"),
        Commands::Completion { .. } => {
            unreachable!("Completion is dispatched before this match reached")
        }
        Commands::Views { action } => {
            let action = action.unwrap_or(ViewsAction::List);
            match action {
                ViewsAction::List => {
                    crate::commands::views::list_views(ctx.config_dir, effective_format)
                }
                ViewsAction::Set {
                    name,
                    pattern,
                    mut filters,
                } => {
                    if pattern.is_some() && filters.regexp.is_some() {
                        return Ok(CommandOutcome::UserError(
                            "Error: PATTERN and --regexp are mutually exclusive".to_owned(),
                        ));
                    }
                    filters.pattern = pattern;
                    crate::commands::views::set_view(
                        ctx.config_dir,
                        &name,
                        &filters,
                        effective_format,
                    )
                }
                ViewsAction::Remove { name } => {
                    crate::commands::views::remove_view(ctx.config_dir, &name, effective_format)
                }
                ViewsAction::Run {
                    name,
                    mut filters,
                    index_flags: _, // consumed in run.rs before dispatch
                } => {
                    // Load the named view and merge the CLI overlay on top.
                    let views = crate::commands::views::load_views(ctx.config_dir);
                    match views.get(&name) {
                        Some(base) => {
                            let overlay = std::mem::take(&mut filters);
                            filters = base.clone();
                            filters.merge_from(&overlay);
                        }
                        None => {
                            return Ok(CommandOutcome::UserError(format!(
                                "Error: unknown view '{name}'\n\n  tip: run 'hyalo views list' to see available views"
                            )));
                        }
                    }
                    // Propagate the view's saved pattern to the BM25 search.
                    let pattern = filters.pattern.clone();
                    let FindFilters {
                        regexp,
                        properties,
                        tag,
                        task,
                        sections,
                        file,
                        glob,
                        fields,
                        sort,
                        reverse,
                        limit,
                        broken_links,
                        orphan,
                        dead_end,
                        title,
                        language,
                        ..
                    } = filters;
                    if orphan && dead_end {
                        crate::warn::warn(
                            "--orphan and --dead-end are mutually exclusive (no file can be both); results will always be empty",
                        );
                    }
                    for t in &tag {
                        if let Err(msg) = crate::commands::tags::validate_tag(t) {
                            return Ok(CommandOutcome::UserError(format!("Error: {msg}")));
                        }
                    }
                    if let Some(ref lang) = language
                        && let Err(e) = parse_language(lang)
                    {
                        return Ok(CommandOutcome::UserError(format!(
                            "invalid --language value {lang:?}: {e}"
                        )));
                    }
                    if let Some(cfg_lang) = ctx.config_language
                        && let Err(e) = parse_language(cfg_lang)
                    {
                        return Ok(CommandOutcome::UserError(format!(
                            "invalid [search].language config value {cfg_lang:?}: {e}"
                        )));
                    }
                    let prop_filters: Vec<filter::PropertyFilter> = match properties
                        .iter()
                        .map(|s| filter::parse_property_filter(s))
                        .collect::<Result<Vec<_>, _>>()
                    {
                        Ok(f) => f,
                        Err(e) => {
                            return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                        }
                    };
                    let task_filter = match task.as_deref().map(filter::parse_task_filter) {
                        Some(Ok(f)) => Some(f),
                        Some(Err(e)) => {
                            return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                        }
                        None => None,
                    };
                    let parsed_fields = match filter::Fields::parse(&fields) {
                        Ok(f) => f,
                        Err(e) => {
                            return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                        }
                    };
                    let sort_field = match sort.as_deref().map(filter::parse_sort) {
                        Some(Ok(f)) => Some(f),
                        Some(Err(e)) => {
                            return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                        }
                        None => None,
                    };
                    let section_filters: Vec<hyalo_core::heading::SectionFilter> = match sections
                        .iter()
                        .map(|s| hyalo_core::heading::SectionFilter::parse(s))
                        .collect::<Result<Vec<_>, _>>()
                    {
                        Ok(f) => f,
                        Err(e) => {
                            return Ok(CommandOutcome::UserError(format!("Error: {e}")));
                        }
                    };
                    let file: Vec<String> = file
                        .into_iter()
                        .map(|f| hyalo_core::discovery::strip_dir_prefix(dir, &f).unwrap_or(f))
                        .collect();
                    let sort_needs_backlinks =
                        matches!(sort_field.as_ref(), Some(filter::SortField::BacklinksCount));
                    let sort_needs_links =
                        matches!(sort_field.as_ref(), Some(filter::SortField::LinksCount));
                    let sort_needs_title =
                        matches!(sort_field.as_ref(), Some(filter::SortField::Title));
                    let has_task_filter = task_filter.is_some();
                    let has_section_filter = !section_filters.is_empty();
                    let has_bm25_search = pattern.is_some();
                    let has_title_filter = title.is_some();
                    let needs_body = find_commands::needs_body(
                        &parsed_fields,
                        has_task_filter,
                        has_section_filter,
                    ) || sort_needs_links
                        || sort_needs_title
                        || broken_links
                        || orphan
                        || dead_end
                        || has_title_filter
                        || has_bm25_search;
                    let needs_full_vault =
                        parsed_fields.backlinks || sort_needs_backlinks || orphan || dead_end;
                    let scan_body = needs_body || needs_full_vault;
                    match resolve_index(
                        snapshot_index.as_ref(),
                        dir,
                        &file,
                        &glob,
                        effective_format,
                        site_prefix,
                        needs_full_vault,
                        &ScanOptions {
                            scan_body,
                            bm25_tokenize: false,
                            default_language: None,
                            frontmatter_link_props: ctx.frontmatter_link_props,
                        },
                    )? {
                        IndexResolution::Resolved(resolved) => {
                            let ci = maybe_case_index(ctx.case_insensitive_mode, dir);
                            find_commands::find(
                                resolved.as_index(),
                                dir,
                                site_prefix,
                                pattern.as_deref(),
                                regexp.as_deref(),
                                &prop_filters,
                                &tag,
                                task_filter.as_ref(),
                                &section_filters,
                                &file,
                                &glob,
                                &parsed_fields,
                                sort_field.as_ref(),
                                reverse,
                                resolve_limit(
                                    limit,
                                    ctx.config_default_limit,
                                    ctx.programmatic_output,
                                ),
                                broken_links,
                                orphan,
                                dead_end,
                                title.as_deref(),
                                effective_format,
                                language.as_deref(),
                                ctx.config_language,
                                ci.as_ref(),
                            )
                        }
                        IndexResolution::Outcome(outcome) => Ok(outcome),
                    }
                }
            }
        }
        Commands::Types { action } => {
            let action = action.unwrap_or(TypesAction::List);
            match action {
                TypesAction::List => Ok(crate::commands::types::list_types(ctx.schema)),
                TypesAction::Show { type_name } => Ok(crate::commands::types::show_type(
                    &type_name,
                    ctx.schema,
                    effective_format,
                )),
                TypesAction::Remove { type_name } => crate::commands::types::remove_type(
                    ctx.config_dir,
                    &type_name,
                    effective_format,
                ),
                TypesAction::Set {
                    type_name,
                    required,
                    default,
                    property_type,
                    property_values,
                    filename_template,
                    dry_run,
                } => crate::commands::types::set_type(
                    ctx.config_dir,
                    &type_name,
                    &required,
                    &default,
                    &property_type,
                    &property_values,
                    filename_template.as_deref(),
                    dry_run,
                    effective_format,
                ),
            }
        }
        Commands::New { r#type, file } => {
            crate::commands::new::create_new(ctx.dir, &r#type, &file, ctx.schema, effective_format)
        }
        // Config is dispatched as an early-return in run.rs before dispatch() is called.
        Commands::Config => unreachable!("Config command is handled before dispatch"),
    }
}
