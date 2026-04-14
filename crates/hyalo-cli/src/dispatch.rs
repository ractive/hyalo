use std::path::Path;

use anyhow::Result;

use crate::cli::args::{
    Commands, FindFilters, LinksAction, PropertiesAction, TagsAction, TaskAction, TypesAction,
    ViewsAction, resolve_single_file,
};
use crate::commands::{
    IndexResolution, ResolvedIndex, append as append_commands, backlinks as backlinks_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, links as links_commands, lint as lint_commands, mv as mv_commands,
    properties, read as read_commands, remove as remove_commands, resolve_index,
    set as set_commands, summary as summary_commands, tags as tag_commands, tasks as task_commands,
};
use crate::output::{CommandOutcome, Format};
use hyalo_core::bm25::parse_language;
use hyalo_core::filter;
use hyalo_core::index::{ScanOptions, SnapshotIndex, VaultIndex as _};
use hyalo_core::schema::SchemaConfig;

/// Default output limit for list commands when no `--limit` is passed and no
/// `default_limit` is set in `.hyalo.toml`.
pub(crate) const DEFAULT_OUTPUT_LIMIT: usize = 50;

/// Shared context for command dispatch.
pub(crate) struct CommandContext<'a> {
    pub dir: &'a Path,
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
    /// Parsed schema configuration from `[schema.*]` sections in `.hyalo.toml`.
    pub schema: &'a SchemaConfig,
    /// Optional exit code override set by commands that need a non-0/2 exit code
    /// (e.g. `lint` returns 1 when errors are found). The output pipeline uses this
    /// to override its own exit code calculation.
    pub exit_code_override: Option<i32>,
    /// Default output limit from `.hyalo.toml` (`default_limit`).
    /// `None` = use `DEFAULT_OUTPUT_LIMIT`.
    /// `Some(0)` = unlimited.
    /// `Some(n)` = limit to n.
    pub config_default_limit: Option<usize>,
}

/// Resolve the effective limit for a list command.
///
/// Precedence (highest first):
/// 1. `cli_limit = Some(n)` — user passed `--limit n` (0 = unlimited → returns `None`)
/// 2. `config_default` = `Some(n)` from `.hyalo.toml` (0 = unlimited → returns `None`)
/// 3. `DEFAULT_OUTPUT_LIMIT` — hard-coded fallback
///
/// Returns `None` for unlimited, `Some(n)` for an effective cap.
fn resolve_limit(cli_limit: Option<usize>, config_default: Option<usize>) -> Option<usize> {
    match cli_limit {
        Some(0) => None, // explicit --limit 0 = unlimited
        Some(n) => Some(n),
        None => match config_default {
            Some(0) => None, // config default_limit = 0 = unlimited
            Some(n) => Some(n),
            None => Some(DEFAULT_OUTPUT_LIMIT),
        },
    }
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
                ScanOptions {
                    scan_body,
                    bm25_tokenize: false,
                    default_language: None,
                },
            )? {
                IndexResolution::Resolved(resolved) => find_commands::find(
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
                    resolve_limit(limit, ctx.config_default_limit),
                    broken_links,
                    orphan,
                    dead_end,
                    title.as_deref(),
                    effective_format,
                    language.as_deref(),
                    ctx.config_language,
                ),
                IndexResolution::Outcome(outcome) => Ok(outcome),
            }
        }
        Commands::Read {
            file_positional,
            file,
            section,
            lines,
            frontmatter,
        } => {
            let file = match resolve_single_file(file_positional, file) {
                Ok(f) => f,
                Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
            };
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
        Commands::Properties { action } => {
            let action = action.unwrap_or(PropertiesAction::Summary {
                glob: vec![],
                limit: None,
            });
            match action {
                PropertiesAction::Summary {
                    ref glob,
                    limit: cli_limit,
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
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
                                    resolve_limit(cli_limit, ctx.config_default_limit),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        properties::properties_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(cli_limit, ctx.config_default_limit),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                PropertiesAction::Rename { from, to, glob } => properties::properties_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
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
            });
            match action {
                TagsAction::Summary {
                    ref glob,
                    limit: cli_limit,
                } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    ScanOptions {
                        scan_body: false,
                        bm25_tokenize: false,
                        default_language: None,
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
                                    resolve_limit(cli_limit, ctx.config_default_limit),
                                )
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        tag_commands::tags_summary(
                            &build.index,
                            None,
                            effective_format,
                            resolve_limit(cli_limit, ctx.config_default_limit),
                        )
                    }
                    IndexResolution::Outcome(outcome) => Ok(outcome),
                },
                TagsAction::Rename { from, to, glob } => tag_commands::tags_rename(
                    dir,
                    &from,
                    &to,
                    &glob,
                    effective_format,
                    snapshot_index,
                    index_path,
                ),
            }
        }
        Commands::Task { action } => match action {
            TaskAction::Read {
                file_positional,
                file,
                line,
                section,
                all,
            } => {
                let file = match resolve_single_file(file_positional, file) {
                    Ok(f) => f,
                    Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
                };
                task_commands::task_read(
                    dir,
                    &file,
                    &line,
                    section.as_deref(),
                    all,
                    effective_format,
                )
            }
            TaskAction::Toggle {
                file_positional,
                file,
                line,
                section,
                all,
            } => {
                let file = match resolve_single_file(file_positional, file) {
                    Ok(f) => f,
                    Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
                };
                task_commands::task_toggle(
                    dir,
                    &file,
                    &line,
                    section.as_deref(),
                    all,
                    effective_format,
                    snapshot_index,
                    index_path,
                )
            }
            TaskAction::Set {
                file_positional,
                file,
                line,
                section,
                all,
                status,
            } => {
                let file = match resolve_single_file(file_positional, file) {
                    Ok(f) => f,
                    Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
                };
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
                task_commands::task_set_status(
                    dir,
                    &file,
                    &line,
                    section.as_deref(),
                    all,
                    ch,
                    effective_format,
                    snapshot_index,
                    index_path,
                )
            }
        },
        Commands::Summary {
            glob,
            recent,
            depth,
        } => match resolve_index(
            snapshot_index.as_ref(),
            dir,
            &[],
            &glob,
            effective_format,
            site_prefix,
            true,
            ScanOptions {
                scan_body: true,
                bm25_tokenize: false,
                default_language: None,
            },
        )? {
            IndexResolution::Resolved(resolved) => summary_commands::summary(
                dir,
                resolved.as_index(),
                &glob,
                recent,
                depth,
                site_prefix,
                effective_format,
                ctx.schema,
            ),
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        Commands::Set {
            file_positional,
            properties,
            tag,
            mut file,
            glob,
            where_properties,
            where_tags,
            dry_run,
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
            )
        }
        Commands::Remove {
            file_positional,
            properties,
            tag,
            mut file,
            glob,
            where_properties,
            where_tags,
            dry_run,
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
            where_properties,
            where_tags,
            dry_run,
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
            )
        }
        Commands::Backlinks {
            file_positional,
            file,
            limit: cli_limit,
        } => {
            let file = match resolve_single_file(file_positional, file) {
                Ok(f) => f,
                Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
            };
            match resolve_index(
                snapshot_index.as_ref(),
                dir,
                &[],
                &[],
                effective_format,
                site_prefix,
                true,
                ScanOptions {
                    scan_body: true,
                    bm25_tokenize: false,
                    default_language: None,
                },
            )? {
                IndexResolution::Resolved(resolved) => backlinks_commands::backlinks(
                    resolved.as_index(),
                    &file,
                    dir,
                    effective_format,
                    resolve_limit(cli_limit, ctx.config_default_limit),
                ),
                IndexResolution::Outcome(outcome) => Ok(outcome),
            }
        }
        Commands::Mv {
            file_positional,
            file,
            to,
            dry_run,
        } => {
            let file = match resolve_single_file(file_positional, file) {
                Ok(f) => f,
                Err(e) => return Ok(CommandOutcome::UserError(format!("{e}"))),
            };
            mv_commands::mv(
                dir,
                &file,
                &to,
                dry_run,
                effective_format,
                site_prefix,
                snapshot_index,
                index_path,
            )
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
        Commands::Links { action } => match action {
            LinksAction::Fix {
                dry_run: _,
                apply,
                threshold,
                glob,
                ignore_target,
            } => match resolve_index(
                snapshot_index.as_ref(),
                dir,
                &[],
                &[],
                effective_format,
                site_prefix,
                true,
                ScanOptions {
                    scan_body: true,
                    bm25_tokenize: false,
                    default_language: None,
                },
            )? {
                IndexResolution::Resolved(resolved) => links_commands::links_fix(
                    resolved.as_index(),
                    dir,
                    site_prefix,
                    &glob,
                    !apply,
                    threshold,
                    &ignore_target,
                    effective_format,
                ),
                IndexResolution::Outcome(outcome) => Ok(outcome),
            },
        },
        Commands::Lint {
            file_positional,
            file,
            glob,
            fix,
            dry_run,
            limit: cli_limit,
        } => {
            // Build the file list. Positional arg is treated as a single --file.
            let mut files_arg: Vec<String> = file;
            if let Some(pos) = file_positional {
                files_arg.insert(0, pos);
            }

            let file_pairs =
                match crate::commands::collect_files(dir, &files_arg, &glob, ctx.user_format)? {
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

            let (outcome, counts) = lint_commands::lint_files_with_options(
                &file_pairs,
                ctx.schema,
                fix_mode,
                resolve_limit(cli_limit, ctx.config_default_limit),
            )?;

            // Signal exit code 1 when errors remain after fixes (set before returning).
            if counts.errors > 0 {
                ctx.exit_code_override = Some(1);
            }

            Ok(outcome)
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
                ViewsAction::List => crate::commands::views::list_views(),
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
                    crate::commands::views::set_view(&name, &filters)
                }
                ViewsAction::Remove { name } => crate::commands::views::remove_view(&name),
            }
        }
        Commands::Types { action } => {
            let action = action.unwrap_or(TypesAction::List);
            match action {
                TypesAction::List => Ok(crate::commands::types::list_types(ctx.schema)),
                TypesAction::Show { type_name } => {
                    Ok(crate::commands::types::show_type(&type_name, ctx.schema))
                }
                TypesAction::Remove { type_name } => {
                    crate::commands::types::remove_type(&type_name)
                }
                TypesAction::Set {
                    type_name,
                    required,
                    default,
                    property_type,
                    property_values,
                    filename_template,
                    dry_run,
                } => crate::commands::types::set_type(
                    dir,
                    &type_name,
                    &required,
                    &default,
                    &property_type,
                    &property_values,
                    filename_template.as_deref(),
                    dry_run,
                ),
            }
        }
    }
}
