use std::path::Path;

use anyhow::Result;

use crate::cli::args::{
    Commands, FindFilters, LinksAction, PropertiesAction, TagsAction, TaskAction, ViewsAction,
};
use crate::commands::{
    IndexResolution, ResolvedIndex, append as append_commands, backlinks as backlinks_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, links as links_commands, mv as mv_commands, properties,
    read as read_commands, remove as remove_commands, resolve_index, set as set_commands,
    summary as summary_commands, tags as tag_commands, tasks as task_commands,
};
use crate::output::{CommandOutcome, Format};
use hyalo_core::filter;
use hyalo_core::index::{ScanOptions, SnapshotIndex, VaultIndex as _};

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
            view: _, // resolved before dispatch
            filters:
                FindFilters {
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
                    title,
                },
        } => {
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
            let needs_body =
                find_commands::needs_body(&parsed_fields, has_task_filter, has_section_filter)
                    || sort_needs_links
                    || sort_needs_title
                    || broken_links
                    || has_title_filter;
            let needs_full_vault = parsed_fields.backlinks || sort_needs_backlinks;
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
                ScanOptions { scan_body },
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
                    limit,
                    broken_links,
                    title.as_deref(),
                    effective_format,
                ),
                IndexResolution::Outcome(outcome) => Ok(outcome),
            }
        }
        Commands::Read {
            file,
            section,
            lines,
            frontmatter,
        } => read_commands::run(
            dir,
            &file,
            section.as_deref(),
            lines.as_deref(),
            frontmatter,
            effective_format,
            ctx.user_format,
        ),
        Commands::Properties { action } => {
            let action = action.unwrap_or(PropertiesAction::Summary { glob: vec![] });
            match action {
                PropertiesAction::Summary { ref glob } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    ScanOptions { scan_body: false },
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
                                properties::properties_summary(idx, file_filter, effective_format)
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        properties::properties_summary(&build.index, None, effective_format)
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
            let action = action.unwrap_or(TagsAction::Summary { glob: vec![] });
            match action {
                TagsAction::Summary { ref glob } => match resolve_index(
                    snapshot_index.as_ref(),
                    dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    false,
                    ScanOptions { scan_body: false },
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
                                tag_commands::tags_summary(idx, file_filter, effective_format)
                            }
                        }
                    }
                    IndexResolution::Resolved(ResolvedIndex::Scanned(build)) => {
                        tag_commands::tags_summary(&build.index, None, effective_format)
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
            TaskAction::Read { file, line } => {
                task_commands::task_read(dir, &file, line, effective_format)
            }
            TaskAction::Toggle { file, line } => task_commands::task_toggle(
                dir,
                &file,
                line,
                effective_format,
                snapshot_index,
                index_path,
            ),
            TaskAction::SetStatus { file, line, status } => {
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
                    line,
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
            ScanOptions { scan_body: true },
        )? {
            IndexResolution::Resolved(resolved) => summary_commands::summary(
                dir,
                resolved.as_index(),
                &glob,
                recent,
                depth,
                site_prefix,
                effective_format,
            ),
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        Commands::Set {
            properties,
            tag,
            file,
            glob,
            where_properties,
            where_tags,
            dry_run,
        } => {
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
            properties,
            tag,
            file,
            glob,
            where_properties,
            where_tags,
            dry_run,
        } => {
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
            properties,
            file,
            glob,
            where_properties,
            where_tags,
            dry_run,
        } => {
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
        Commands::Backlinks { file } => match resolve_index(
            snapshot_index.as_ref(),
            dir,
            &[],
            &[],
            effective_format,
            site_prefix,
            true,
            ScanOptions { scan_body: true },
        )? {
            IndexResolution::Resolved(resolved) => {
                backlinks_commands::backlinks(resolved.as_index(), &file, dir, effective_format)
            }
            IndexResolution::Outcome(outcome) => Ok(outcome),
        },
        Commands::Mv { file, to, dry_run } => mv_commands::mv(
            dir,
            &file,
            &to,
            dry_run,
            effective_format,
            site_prefix,
            snapshot_index,
            index_path,
        ),
        Commands::CreateIndex {
            output,
            allow_outside_vault,
        } => create_index_commands::create_index(
            dir,
            site_prefix,
            output.as_deref(),
            effective_format,
            allow_outside_vault,
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
                ScanOptions { scan_body: true },
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
        // `Init` and `Deinit` are handled as early returns before dispatch is called.
        Commands::Init { .. } => unreachable!("Init is dispatched before this match reached"),
        Commands::Deinit => unreachable!("Deinit is dispatched before this match reached"),
        Commands::Views { action } => {
            let action = action.unwrap_or(ViewsAction::List);
            match action {
                ViewsAction::List => crate::commands::views::list_views(),
                ViewsAction::Set { name, filters } => {
                    crate::commands::views::set_view(&name, &filters)
                }
                ViewsAction::Remove { name } => crate::commands::views::remove_view(&name),
            }
        }
    }
}
