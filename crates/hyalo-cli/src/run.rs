use std::process;

use clap::{CommandFactory, FromArgMatches};

use crate::cli::args::{Cli, Commands, LinksAction, PropertiesAction, TagsAction, TaskAction};
use crate::cli::help::{filter_examples, filter_long_help};
use crate::commands::{
    ScannedIndexOutcome, append as append_commands, backlinks as backlinks_commands,
    build_scanned_index, create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, init as init_commands, links as links_commands, mv as mv_commands,
    properties, read as read_commands, remove as remove_commands, set as set_commands,
    summary as summary_commands, tags as tag_commands, tasks as task_commands,
};
use crate::hints::{HintContext, HintSource, generate_hints};
use crate::output::{
    CommandOutcome, Format, apply_jq_filter_result, format_success, format_with_hints,
};
use hyalo_core::filter;
use hyalo_core::index::{ScanOptions, SnapshotIndex, VaultIndex};

/// Parse `--where-property` filters and validate `--where-tag` names.
/// Exits with code 1 on invalid input.
fn parse_where_filters(
    where_properties: &[String],
    where_tags: &[String],
) -> Vec<filter::PropertyFilter> {
    let filters = match where_properties
        .iter()
        .map(|s| filter::parse_property_filter(s))
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error: {e}");
            die(1);
        }
    };
    for tag in where_tags {
        if let Err(msg) = crate::commands::tags::validate_tag(tag) {
            eprintln!("Error: {msg}");
            die(1);
        }
    }
    filters
}

/// Exit the process, flushing any pending warning summary first.
///
/// Use this in place of `process::exit` after `warn::init` has been called so
/// that duplicate-warning counts are always reported before the process ends.
fn die(code: i32) -> ! {
    crate::warn::flush_summary();
    process::exit(code)
}

#[allow(clippy::too_many_lines)]
pub fn run() {
    // Pre-scan for --quiet / -q so config-loading warnings are also suppressed.
    let early_quiet = std::env::args().any(|a| a == "--quiet" || a == "-q");
    crate::warn::init(early_quiet);

    // Load per-project config from .hyalo.toml in CWD before parsing args.
    // This lets us hide flags that already have config-provided defaults,
    // keeping `--help` output focused on what the user actually needs to set.
    let config = crate::config::load_config();

    // Build the clap Command and hide global flags that are already covered by
    // the project config.  `mut_arg` is scoped to the root command, but because
    // both `--dir` and `--format` are declared `global = true`, hiding them on
    // the root is sufficient for --help at every level.
    let hide_dir = config
        .dir
        .components()
        .ne(std::path::Path::new(".").components());
    let hide_format = config.format != "json";

    let mut cmd = Cli::command();
    if hide_dir {
        cmd = cmd.mut_arg("dir", |a| a.hide(true));
    }
    if hide_format {
        cmd = cmd.mut_arg("format", |a| a.hide(true));
    }

    // Apply runtime-filtered help text so that examples and cookbook entries
    // that reference config-defaulted flags are stripped from help output.
    // `after_help` is shown by `-h`; `after_long_help` is shown by `--help`.
    cmd = cmd
        .after_help(filter_examples(hide_dir, hide_format))
        .after_long_help(filter_long_help(hide_dir, hide_format));

    // Global args (--format, --jq, etc.) are only defined on the root Command
    // in clap derive — they aren't propagated to subcommands until parse time.
    // We can't use mut_subcommand to hide them from `init --help` because
    // they don't exist on the subcommand Command node yet.  This is a known
    // clap limitation with `global = true` derive args.
    let raw_args: Vec<String> = std::env::args().collect();
    let matches = match cmd.try_get_matches_from(raw_args.iter().map(String::as_str)) {
        Ok(m) => m,
        Err(e) => {
            // Intercept `--filter` before falling through to clap's built-in
            // suggestion, which picks `--file` (closest by Levenshtein distance).
            // Users almost always mean `--property` here.
            if e.kind() == clap::error::ErrorKind::UnknownArgument
                && crate::suggest::unknown_arg_is(&e, "--filter")
            {
                eprintln!(
                    "error: unexpected argument '--filter' found\n\n\
                     tip: did you mean '--property'?\n\n\
                     Example: hyalo find --property status=planned\n"
                );
                die(2);
            }

            // Only attempt subcommand suggestions when clap couldn't recognise a
            // flag or subcommand — this avoids misleading tips for other error kinds.
            if matches!(
                e.kind(),
                clap::error::ErrorKind::InvalidSubcommand | clap::error::ErrorKind::UnknownArgument
            ) && let Some(suggestion) =
                crate::suggest::suggest_subcommand_correction(&raw_args, &Cli::command())
            {
                eprintln!("{e}\n  tip: did you mean:\n\n    {suggestion}\n");
                die(2);
            }

            // Suggest --version / --help when the user types a close misspelling
            // as a bare subcommand (e.g. `hyalo versio`, `hyalo hep`).
            if e.kind() == clap::error::ErrorKind::InvalidSubcommand {
                use clap::error::{ContextKind, ContextValue};
                if let Some(invalid) = e.context().find_map(|(k, v)| {
                    if k == ContextKind::InvalidSubcommand {
                        if let ContextValue::String(s) = v {
                            Some(s.as_str())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }) {
                    for (target, suggestion) in [("version", "--version"), ("help", "--help")] {
                        if strsim::damerau_levenshtein(invalid, target) <= 2 {
                            eprintln!("{e}\n  tip: did you mean `hyalo {suggestion}`?\n");
                            die(2);
                        }
                    }
                }
            }

            let code = e.exit_code();
            let _ = e.print();
            die(code);
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => {
            let code = e.exit_code();
            let _ = e.print();
            die(code);
        }
    };

    // Re-apply quiet flag from the fully-parsed CLI (the early pre-scan
    // covers the common case but this ensures correctness after full parsing).
    crate::warn::init(cli.quiet);

    // `init` operates on CWD directly and needs no config or format resolution.
    // Dispatch it before the rest of the setup.
    // The global --dir flag is used as the dir value for .hyalo.toml.
    if let Commands::Init { claude } = cli.command {
        let init_dir = cli.dir.as_deref().and_then(|p| p.to_str());
        let code = match init_commands::run_init(init_dir, claude) {
            Ok(CommandOutcome::Success(output)) => {
                println!("{output}");
                0
            }
            Ok(CommandOutcome::UserError(output)) => {
                eprintln!("{output}");
                1
            }
            Err(e) => {
                eprintln!("Error: {e}");
                2
            }
        };
        die(code);
    }

    // Merge: CLI args override config, config overrides hardcoded defaults.
    // Track whether --dir was explicitly passed (not from config) so hints
    // can omit it when the user relies on .hyalo.toml.
    let dir_from_cli = cli.dir.is_some();
    let format_from_cli = cli.format.is_some();
    let hints_from_cli = cli.hints;
    let dir = cli.dir.unwrap_or(config.dir);

    // Validate that --dir is not a file path
    if dir.is_file() {
        eprintln!(
            "Error: --dir path '{}' is a file, not a directory. Use --file to target a single file.",
            dir.display()
        );
        die(1);
    }

    // Derive site_prefix with tri-state precedence:
    //
    //   1. CLI --site-prefix flag  (present → use it; empty string = explicit disable)
    //   2. `site_prefix` in .hyalo.toml  (same: empty string = explicit disable)
    //   3. Auto-derive from canonicalized dir's last path component
    //      (only runs when neither 1 nor 2 is present)
    //
    // Empty strings in (1) and (2) short-circuit the chain and result in
    // site_prefix = None, suppressing all absolute-link resolution.
    let site_prefix_owned: Option<String> = if cli.site_prefix.is_some() {
        // Explicit CLI flag wins — empty string intentionally disables prefix.
        cli.site_prefix.filter(|s| !s.is_empty())
    } else if config.site_prefix.is_some() {
        // Config file override — empty string intentionally disables prefix.
        config.site_prefix.filter(|s| !s.is_empty())
    } else {
        // Auto-derive from the last component of the resolved dir.
        match std::fs::canonicalize(&dir) {
            Ok(canonical) => canonical
                .file_name()
                .and_then(|n| n.to_str())
                .map(std::borrow::ToOwned::to_owned),
            Err(_) => {
                // Fallback for non-existent paths: use file_name() on the raw path.
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| *s != ".")
                    .map(std::borrow::ToOwned::to_owned)
            }
        }
    };
    let site_prefix = site_prefix_owned.as_deref();
    // CLI --format is already validated by Clap; fall back to config (String) with runtime parse.
    let format = if let Some(f) = cli.format {
        f
    } else if let Some(fmt) = Format::from_str_opt(&config.format) {
        fmt
    } else {
        eprintln!(
            "Invalid output format '{}' in .hyalo.toml; supported formats are: json, text",
            config.format
        );
        die(2);
    };
    let hints_flag = if cli.hints {
        true
    } else if cli.no_hints {
        false
    } else {
        config.hints
    };

    // --jq operates on JSON, so it conflicts with an explicit --format text.
    let jq_filter = cli.jq.as_deref();

    // `read` defaults to text output (unlike other commands which default to json).
    // Skip the override when --jq is active (jq needs JSON).
    let format = if !format_from_cli
        && jq_filter.is_none()
        && matches!(cli.command, Commands::Read { .. })
    {
        Format::Text
    } else {
        format
    };
    if jq_filter.is_some() && format != Format::Json {
        eprintln!("Error: --jq cannot be combined with --format {format}");
        eprintln!("  --jq always operates on JSON output; drop --format or use --format json");
        die(2);
    }
    // When --jq or --hints is active, force JSON internally so we can re-parse the output.
    // The user-requested format is applied afterwards.
    let hints_active = hints_flag && jq_filter.is_none();
    let effective_format = if jq_filter.is_some() || hints_active {
        Format::Json
    } else {
        format
    };

    // Build hint context before the command dispatch.
    // Only include CLI-explicit flags in hints — config values are inherited
    // automatically when the user runs the hint command from the same CWD.
    let hint_ctx = if hints_flag && jq_filter.is_none() {
        let dir_hint = if dir_from_cli {
            dir.to_str()
                .map(std::borrow::ToOwned::to_owned)
                .filter(|s| s != ".")
        } else {
            None
        };
        let format_hint = if format_from_cli {
            Some(format.to_string())
        } else {
            None
        };

        match &cli.command {
            Commands::Summary { glob, .. } => Some(HintContext {
                source: HintSource::Summary,
                dir: dir_hint,
                glob: glob.clone(),
                format: format_hint,
                hints: hints_from_cli,
            }),
            Commands::Properties {
                action: Some(PropertiesAction::Summary { glob }),
            } => Some(HintContext {
                source: HintSource::PropertiesSummary,
                dir: dir_hint,
                glob: glob.clone(),
                format: format_hint,
                hints: hints_from_cli,
            }),
            Commands::Tags {
                action: Some(TagsAction::Summary { glob }),
            } => Some(HintContext {
                source: HintSource::TagsSummary,
                dir: dir_hint,
                glob: glob.clone(),
                format: format_hint,
                hints: hints_from_cli,
            }),
            Commands::Find { glob, .. } => Some(HintContext {
                source: HintSource::Find,
                dir: dir_hint,
                glob: glob.clone(),
                format: format_hint,
                hints: hints_from_cli,
            }),
            _ => None,
        }
    } else {
        None
    };

    // Warn when --hints is passed to mutation commands, which do not generate hints.
    if hints_from_cli
        && matches!(
            &cli.command,
            Commands::Set { .. } | Commands::Remove { .. } | Commands::Append { .. }
        )
    {
        crate::warn::warn("--hints has no effect on mutation commands");
    }

    // Load snapshot index if --index is provided.
    // Read-only commands use it to skip disk scans. Mutation commands use it to
    // keep the index up-to-date after each file write (they still read/write
    // individual files on disk, but patch the index entry in-place).
    let uses_index = !matches!(
        &cli.command,
        Commands::Init { .. }
            | Commands::CreateIndex { .. }
            | Commands::DropIndex { .. }
            | Commands::Read { .. }
    );
    let mut snapshot_index: Option<SnapshotIndex> = if uses_index {
        if let Some(ref index_path) = cli.index {
            match SnapshotIndex::load(index_path) {
                Ok(Some(idx)) => {
                    // Warn when the snapshot was built for a different vault or
                    // site-prefix — the index data may not match the current run.
                    let canonical_dir = std::fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
                    let vault_dir_str = canonical_dir.to_string_lossy();
                    if idx.validate(&vault_dir_str, site_prefix) {
                        Some(idx)
                    } else {
                        let (hdr_vault, hdr_prefix, _, _) = idx.header_info();
                        crate::warn::warn(format!(
                            "index was built for vault '{hdr_vault}' (prefix {hdr_prefix:?}) but current \
                             vault is '{vault_dir_str}' (prefix {site_prefix:?}); falling back to disk scan",
                        ));
                        None
                    }
                }
                Ok(None) => None, // incompatible schema — already warned; fall back to disk scan
                Err(e) => {
                    crate::warn::warn(format!(
                        "failed to load index: {e}; falling back to disk scan"
                    ));
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let result = match cli.command {
        Commands::Find {
            ref pattern,
            ref regexp,
            ref properties,
            ref tag,
            ref task,
            ref sections,
            ref file,
            ref glob,
            ref fields,
            ref sort,
            reverse,
            limit,
            broken_links,
            ref title,
        } => {
            // Parse property filters
            let prop_filters: Vec<filter::PropertyFilter> = match properties
                .iter()
                .map(|s| filter::parse_property_filter(s))
                .collect::<Result<Vec<_>, _>>()
            {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Error: {e}");
                    die(1);
                }
            };
            // Parse task filter
            let task_filter = match task.as_deref().map(filter::parse_task_filter) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    eprintln!("Error: {e}");
                    die(1);
                }
                None => None,
            };
            // Parse fields
            let parsed_fields = match filter::Fields::parse(fields) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Error: {e}");
                    die(1);
                }
            };
            // Parse sort
            let sort_field = match sort.as_deref().map(filter::parse_sort) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    eprintln!("Error: {e}");
                    die(1);
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
                    eprintln!("Error: {e}");
                    die(1);
                }
            };

            for t in tag {
                if let Err(msg) = crate::commands::tags::validate_tag(t) {
                    eprintln!("Error: {msg}");
                    die(1);
                }
            }

            if let Some(ref idx) = snapshot_index {
                find_commands::find(
                    idx,
                    &dir,
                    site_prefix,
                    pattern.as_deref(),
                    regexp.as_deref(),
                    &prop_filters,
                    tag,
                    task_filter.as_ref(),
                    &section_filters,
                    file,
                    glob,
                    &parsed_fields,
                    sort_field.as_ref(),
                    reverse,
                    limit,
                    broken_links,
                    title.as_deref(),
                    effective_format,
                )
            } else {
                let sort_needs_backlinks =
                    matches!(sort_field.as_ref(), Some(filter::SortField::BacklinksCount));
                let sort_needs_links =
                    matches!(sort_field.as_ref(), Some(filter::SortField::LinksCount));
                let sort_needs_title =
                    matches!(sort_field.as_ref(), Some(filter::SortField::Title));
                let has_content_search = pattern.is_some() || regexp.is_some();
                let has_task_filter = task_filter.is_some();
                let has_section_filter = !section_filters.is_empty();
                let has_title_filter = title.is_some();
                let needs_body = find_commands::needs_body(
                    &parsed_fields,
                    has_content_search,
                    has_task_filter,
                    has_section_filter,
                ) || sort_needs_links
                    || sort_needs_title
                    || broken_links
                    || has_title_filter;
                let needs_full_vault = parsed_fields.backlinks || sort_needs_backlinks;
                // The link graph is only built when scan_body is true, so
                // backlinks / backlink-sort always require body scanning.
                let scan_body = needs_body || needs_full_vault;
                let build = match build_scanned_index(
                    &dir,
                    file,
                    glob,
                    effective_format,
                    site_prefix,
                    needs_full_vault,
                    &ScanOptions { scan_body },
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        die(2);
                    }
                };
                match build {
                    ScannedIndexOutcome::Index(build) => find_commands::find(
                        &build.index,
                        &dir,
                        site_prefix,
                        pattern.as_deref(),
                        regexp.as_deref(),
                        &prop_filters,
                        tag,
                        task_filter.as_ref(),
                        &section_filters,
                        file,
                        glob,
                        &parsed_fields,
                        sort_field.as_ref(),
                        reverse,
                        limit,
                        broken_links,
                        title.as_deref(),
                        effective_format,
                    ),
                    ScannedIndexOutcome::Outcome(o) => Ok(o),
                }
            }
        }
        Commands::Read {
            ref file,
            ref section,
            ref lines,
            frontmatter,
        } => read_commands::run(
            &dir,
            file,
            section.as_deref(),
            lines.as_deref(),
            frontmatter,
            effective_format,
        ),
        Commands::Properties { action } => {
            let action = action.unwrap_or(PropertiesAction::Summary { glob: vec![] });
            match action {
                PropertiesAction::Summary { ref glob } => {
                    if let Some(ref idx) = snapshot_index {
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
                    } else {
                        let build = match build_scanned_index(
                            &dir,
                            &[],
                            glob,
                            effective_format,
                            site_prefix,
                            false,
                            &ScanOptions { scan_body: false },
                        ) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("Error: {e}");
                                die(2);
                            }
                        };
                        match build {
                            ScannedIndexOutcome::Index(build) => {
                                properties::properties_summary(&build.index, None, effective_format)
                            }
                            ScannedIndexOutcome::Outcome(o) => Ok(o),
                        }
                    }
                }
                PropertiesAction::Rename {
                    ref from,
                    ref to,
                    ref glob,
                } => properties::properties_rename(
                    &dir,
                    from,
                    to,
                    glob,
                    effective_format,
                    &mut snapshot_index,
                    cli.index.as_deref(),
                ),
            }
        }
        Commands::Tags { action } => {
            let action = action.unwrap_or(TagsAction::Summary { glob: vec![] });
            match action {
                TagsAction::Summary { ref glob } => {
                    if let Some(ref idx) = snapshot_index {
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
                    } else {
                        let build = match build_scanned_index(
                            &dir,
                            &[],
                            glob,
                            effective_format,
                            site_prefix,
                            false,
                            &ScanOptions { scan_body: false },
                        ) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("Error: {e}");
                                die(2);
                            }
                        };
                        match build {
                            ScannedIndexOutcome::Index(build) => {
                                tag_commands::tags_summary(&build.index, None, effective_format)
                            }
                            ScannedIndexOutcome::Outcome(o) => Ok(o),
                        }
                    }
                }
                TagsAction::Rename {
                    ref from,
                    ref to,
                    ref glob,
                } => tag_commands::tags_rename(
                    &dir,
                    from,
                    to,
                    glob,
                    effective_format,
                    &mut snapshot_index,
                    cli.index.as_deref(),
                ),
            }
        }
        Commands::Task { action } => match action {
            TaskAction::Read { ref file, line } => {
                task_commands::task_read(&dir, file, line, effective_format)
            }
            TaskAction::Toggle { ref file, line } => task_commands::task_toggle(
                &dir,
                file,
                line,
                effective_format,
                &mut snapshot_index,
                cli.index.as_deref(),
            ),
            TaskAction::SetStatus {
                ref file,
                line,
                ref status,
            } => {
                if status.chars().count() != 1 {
                    let out = crate::output::format_error(
                        effective_format,
                        "--status must be a single character",
                        None,
                        Some("example: --status '?' or --status '-'"),
                        None,
                    );
                    eprintln!("{out}");
                    die(1);
                }
                task_commands::task_set_status(
                    &dir,
                    file,
                    line,
                    status.chars().next().unwrap(),
                    effective_format,
                    &mut snapshot_index,
                    cli.index.as_deref(),
                )
            }
        },
        Commands::Summary {
            ref glob,
            recent,
            depth,
        } => {
            if let Some(ref idx) = snapshot_index {
                summary_commands::summary(
                    &dir,
                    idx,
                    glob,
                    recent,
                    depth,
                    site_prefix,
                    effective_format,
                )
            } else {
                let build = match build_scanned_index(
                    &dir,
                    &[],
                    glob,
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions { scan_body: true },
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        die(2);
                    }
                };
                match build {
                    ScannedIndexOutcome::Index(build) => summary_commands::summary(
                        &dir,
                        &build.index,
                        glob,
                        recent,
                        depth,
                        site_prefix,
                        effective_format,
                    ),
                    ScannedIndexOutcome::Outcome(o) => Ok(o),
                }
            }
        }
        Commands::Set {
            ref properties,
            ref tag,
            ref file,
            ref glob,
            ref where_properties,
            ref where_tags,
        } => {
            let where_prop_filters = parse_where_filters(where_properties, where_tags);
            set_commands::set(
                &dir,
                properties,
                tag,
                file,
                glob,
                &where_prop_filters,
                where_tags,
                effective_format,
                &mut snapshot_index,
                cli.index.as_deref(),
            )
        }
        Commands::Remove {
            ref properties,
            ref tag,
            ref file,
            ref glob,
            ref where_properties,
            ref where_tags,
        } => {
            let where_prop_filters = parse_where_filters(where_properties, where_tags);
            remove_commands::remove(
                &dir,
                properties,
                tag,
                file,
                glob,
                &where_prop_filters,
                where_tags,
                effective_format,
                &mut snapshot_index,
                cli.index.as_deref(),
            )
        }
        Commands::Append {
            ref properties,
            ref file,
            ref glob,
            ref where_properties,
            ref where_tags,
        } => {
            let where_prop_filters = parse_where_filters(where_properties, where_tags);
            append_commands::append(
                &dir,
                properties,
                file,
                glob,
                &where_prop_filters,
                where_tags,
                effective_format,
                &mut snapshot_index,
                cli.index.as_deref(),
            )
        }
        Commands::Backlinks { ref file } => {
            if let Some(ref idx) = snapshot_index {
                backlinks_commands::backlinks(idx, file, &dir, effective_format)
            } else {
                let build = match build_scanned_index(
                    &dir,
                    &[],
                    &[],
                    effective_format,
                    site_prefix,
                    true,
                    &ScanOptions { scan_body: true },
                ) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("Error: {e}");
                        die(2);
                    }
                };
                match build {
                    ScannedIndexOutcome::Index(build) => {
                        backlinks_commands::backlinks(&build.index, file, &dir, effective_format)
                    }
                    ScannedIndexOutcome::Outcome(o) => Ok(o),
                }
            }
        }
        Commands::Mv {
            ref file,
            ref to,
            dry_run,
        } => mv_commands::mv(
            &dir,
            file,
            to,
            dry_run,
            effective_format,
            site_prefix,
            &mut snapshot_index,
            cli.index.as_deref(),
        ),
        Commands::CreateIndex {
            ref output,
            allow_outside_vault,
        } => create_index_commands::create_index(
            &dir,
            site_prefix,
            output.as_deref(),
            effective_format,
            allow_outside_vault,
        ),
        Commands::DropIndex {
            ref path,
            allow_outside_vault,
        } => drop_index_commands::drop_index(
            &dir,
            path.as_deref(),
            effective_format,
            allow_outside_vault,
        ),
        Commands::Links { action } => match action {
            LinksAction::Fix {
                dry_run: _,
                apply,
                threshold,
                ref glob,
                ref ignore_target,
            } => {
                if let Some(ref idx) = snapshot_index {
                    links_commands::links_fix(
                        idx,
                        &dir,
                        site_prefix,
                        glob,
                        !apply,
                        threshold,
                        ignore_target,
                        effective_format,
                    )
                } else {
                    let build = match build_scanned_index(
                        &dir,
                        &[],
                        &[],
                        effective_format,
                        site_prefix,
                        true,
                        &ScanOptions { scan_body: true },
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("Error: {e}");
                            die(2);
                        }
                    };
                    match build {
                        ScannedIndexOutcome::Index(build) => links_commands::links_fix(
                            &build.index,
                            &dir,
                            site_prefix,
                            glob,
                            !apply,
                            threshold,
                            ignore_target,
                            effective_format,
                        ),
                        ScannedIndexOutcome::Outcome(o) => Ok(o),
                    }
                }
            }
        },
        // `Init` is handled as an early return before this match is reached.
        Commands::Init { .. } => unreachable!("Init is dispatched before this match reached"),
    };

    match result {
        Ok(CommandOutcome::Success(output)) => {
            if let Some(filter) = jq_filter {
                // Parse the JSON output we forced above, then apply the user filter.
                let value: serde_json::Value = match serde_json::from_str(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = crate::output::format_error(
                            format,
                            "internal error: failed to parse command JSON output",
                            None,
                            None,
                            Some(&e.to_string()),
                        );
                        eprintln!("{msg}");
                        die(2);
                    }
                };
                match apply_jq_filter_result(filter, &value) {
                    Ok(filtered) => println!("{filtered}"),
                    Err(e) => {
                        let msg = crate::output::format_error(
                            format,
                            "jq filter failed",
                            None,
                            None,
                            Some(&e),
                        );
                        eprintln!("{msg}");
                        die(1);
                    }
                }
            } else if let Some(ctx) = &hint_ctx {
                // Re-parse the output to generate hints, then format with them.
                let value: serde_json::Value = if let Ok(v) = serde_json::from_str(&output) {
                    v
                } else {
                    // Should not happen since effective_format is forced to JSON,
                    // but fall through to plain output if it does.
                    println!("{output}");
                    die(0);
                };
                let hints = generate_hints(ctx, &value);
                let formatted = format_with_hints(format, &value, &hints);
                println!("{formatted}");
            } else if hints_active {
                // --hints forced JSON internally but this command has no hint
                // generator.  Convert back to the user-requested format.
                match serde_json::from_str::<serde_json::Value>(&output) {
                    Ok(value) => {
                        println!("{}", format_success(format, &value));
                    }
                    Err(_) => println!("{output}"),
                }
            } else {
                println!("{output}");
            }
        }
        Ok(CommandOutcome::UserError(output)) => {
            eprintln!("{output}");
            die(1);
        }
        Err(e) => {
            let msg = crate::output::format_error(
                format,
                &e.to_string(),
                None,
                None,
                e.chain()
                    .nth(1)
                    .map(std::string::ToString::to_string)
                    .as_deref(),
            );
            eprintln!("{msg}");
            die(2);
        }
    }

    // Flush any dedup summary on the success path (die() handles error paths).
    crate::warn::flush_summary();
}
