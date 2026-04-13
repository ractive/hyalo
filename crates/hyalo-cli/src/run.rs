use std::process;

use clap::{CommandFactory, FromArgMatches};

use crate::cli::args::{Cli, Commands, FindFilters};
use crate::cli::help::{filter_examples, filter_long_help};
use crate::commands::init as init_commands;
use crate::dispatch::{CommandContext, dispatch};
use crate::error::AppError;
use crate::hints::{CommonHintFlags, HintContext, HintSource};
use crate::output::{CommandOutcome, Format};
use crate::output_pipeline::{COUNT_UNSUPPORTED_ERROR, OutputPipeline};
use hyalo_core::index::SnapshotIndex;

/// Derive the task selector string for hint context.
fn task_selector(line: &[usize], section: Option<&String>, all: bool) -> Option<String> {
    if all {
        Some("all".to_owned())
    } else if let Some(s) = section {
        Some(format!("section:{s}"))
    } else if line.len() > 1 {
        Some("lines".to_owned())
    } else {
        None
    }
}

#[allow(clippy::too_many_lines)]
pub fn run() {
    match run_inner() {
        Ok(()) => {
            crate::warn::flush_summary();
        }
        Err(e) => {
            crate::warn::flush_summary();
            let code = match e {
                AppError::User(msg) => {
                    if !msg.is_empty() {
                        eprintln!("{msg}");
                    }
                    1
                }
                AppError::Internal(err) => {
                    let s = err.to_string();
                    if !s.is_empty() {
                        eprintln!("Error: {err}");
                    }
                    2
                }
                AppError::Clap(err) => {
                    let code = err.exit_code();
                    let _ = err.print();
                    code
                }
                AppError::Exit(code) => code,
            };
            process::exit(code);
        }
    }
}

#[allow(clippy::too_many_lines)]
fn run_inner() -> Result<(), AppError> {
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
                return Err(AppError::Exit(2));
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
                return Err(AppError::Exit(2));
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
                            return Err(AppError::Exit(2));
                        }
                    }
                }
            }

            return Err(AppError::Clap(e));
        }
    };
    let mut cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => return Err(AppError::Clap(e)),
    };

    // Re-apply quiet flag from the fully-parsed CLI (the early pre-scan
    // covers the common case but this ensures correctness after full parsing).
    crate::warn::init(cli.quiet);

    // `init` operates on CWD directly and needs no config or format resolution.
    // Dispatch it before the rest of the setup.
    // The global --dir flag is used as the dir value for .hyalo.toml.
    // Reject --count early — init is not a list command.
    if cli.count
        && matches!(
            cli.command,
            Commands::Init { .. } | Commands::Deinit | Commands::Completion { .. }
        )
    {
        eprintln!("{COUNT_UNSUPPORTED_ERROR}");
        return Err(AppError::Exit(2));
    }
    if let Commands::Init { claude } = cli.command {
        let init_dir = cli.dir.as_deref().and_then(|p| p.to_str());
        match init_commands::run_init(init_dir, claude) {
            Ok(CommandOutcome::Success { output, .. } | CommandOutcome::RawOutput(output)) => {
                println!("{output}");
                return Ok(());
            }
            Ok(CommandOutcome::UserError(output)) => return Err(AppError::User(output)),
            Err(e) => return Err(AppError::Internal(e)),
        }
    }
    if let Commands::Deinit = cli.command {
        match init_commands::run_deinit() {
            Ok(CommandOutcome::Success { output, .. } | CommandOutcome::RawOutput(output)) => {
                println!("{output}");
                return Ok(());
            }
            Ok(CommandOutcome::UserError(output)) => return Err(AppError::User(output)),
            Err(e) => return Err(AppError::Internal(e)),
        }
    }
    if let Commands::Completion { shell } = cli.command {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "hyalo", &mut std::io::stdout());
        return Ok(());
    }
    // Merge: CLI args override config, config overrides hardcoded defaults.
    // Track whether --dir was explicitly passed (not from config) so hints
    // can omit it when the user relies on .hyalo.toml.
    let dir_from_cli = cli.dir.is_some();
    let format_from_cli = cli.format.is_some();
    let hints_from_cli = cli.hints;
    let dir = cli.dir.unwrap_or(config.dir);

    // Validate that --dir exists and is a directory (symlinks to directories are fine).
    if !dir.exists() {
        return Err(AppError::User(format!(
            "Error: --dir path '{}' does not exist.",
            dir.display()
        )));
    }
    if dir.is_file() {
        return Err(AppError::User(format!(
            "Error: --dir path '{}' is a file, not a directory. Use --file to target a single file.",
            dir.display()
        )));
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
                // canonicalize can still fail on valid directories (e.g. broken
                // symlink chains on some platforms). Fall back to the raw path
                // component rather than losing the prefix entirely.
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
        return Err(AppError::Exit(2));
    };
    let hints_flag = if cli.hints {
        true
    } else if cli.no_hints {
        false
    } else {
        config.hints
    };

    // Resolve --view: load the named view from .hyalo.toml and merge CLI overrides.
    if let Commands::Find {
        view: Some(ref view_name),
        ref mut filters,
        ..
    } = cli.command
    {
        let views = crate::commands::views::load_views();
        match views.get(view_name) {
            Some(base) => {
                let overlay = std::mem::take(filters);
                *filters = base.clone();
                filters.merge_from(&overlay);
            }
            None => {
                return Err(AppError::User(format!(
                    "Error: unknown view '{view_name}'\n\n  tip: run 'hyalo views list' to see available views"
                )));
            }
        }
    }

    // If the CLI didn't supply a pattern but the view did, propagate it.
    // Skip when --regexp is active — BM25 pattern and regex are mutually exclusive
    // (clap enforces this for CLI args, but a view's pattern bypasses clap).
    if let Commands::Find {
        ref mut pattern,
        ref filters,
        ..
    } = cli.command
        && pattern.is_none()
        && filters.regexp.is_none()
        && let Some(ref view_pattern) = filters.pattern
    {
        *pattern = Some(view_pattern.clone());
    }

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
    // --count replaces the entire output pipeline, so check its conflicts first.
    if cli.count && jq_filter.is_some() {
        eprintln!("Error: --count cannot be combined with --jq");
        eprintln!(
            "  --count prints the bare total; --jq applies a custom filter — use one or the other"
        );
        return Err(AppError::Exit(2));
    }
    if jq_filter.is_some() && format != Format::Json {
        eprintln!("Error: --jq cannot be combined with --format {format}");
        eprintln!("  --jq always operates on JSON output; drop --format or use --format json");
        return Err(AppError::Exit(2));
    }
    // Always force JSON internally so the output pipeline can wrap results in the
    // envelope.  The user-requested format is applied by the pipeline afterwards.
    let effective_format = Format::Json;

    // Build hint context before the command dispatch.
    // Only include CLI-explicit flags in hints — config values are inherited
    // automatically when the user runs the hint command from the same CWD.
    let hint_ctx = if hints_flag && jq_filter.is_none() {
        // Capture the three global flags that every HintContext arm needs.
        // Computed once here so each arm can call HintContext::from_common
        // instead of repeating the same three field assignments.
        let common = CommonHintFlags {
            dir: if dir_from_cli {
                dir.to_str()
                    .map(std::borrow::ToOwned::to_owned)
                    .filter(|s| s != ".")
            } else {
                None
            },
            format: if format_from_cli {
                Some(format.to_string())
            } else {
                None
            },
            hints: hints_from_cli,
        };

        match &cli.command {
            Commands::Summary { glob, .. } => {
                let mut ctx = HintContext::from_common(HintSource::Summary, &common);
                ctx.glob.clone_from(glob);
                Some(ctx)
            }
            Commands::Properties {
                action: Some(crate::cli::args::PropertiesAction::Summary { glob }),
            } => {
                let mut ctx = HintContext::from_common(HintSource::PropertiesSummary, &common);
                ctx.glob.clone_from(glob);
                Some(ctx)
            }
            Commands::Tags {
                action: Some(crate::cli::args::TagsAction::Summary { glob }),
            } => {
                let mut ctx = HintContext::from_common(HintSource::TagsSummary, &common);
                ctx.glob.clone_from(glob);
                Some(ctx)
            }
            Commands::Tags { action: None } => {
                // Bare `hyalo tags` defaults to summary with no glob.
                Some(HintContext::from_common(HintSource::TagsSummary, &common))
            }
            Commands::Find {
                pattern,
                file_positional,
                view,
                filters:
                    FindFilters {
                        glob,
                        regexp,
                        properties,
                        tag,
                        task,
                        file,
                        fields,
                        sort,
                        limit,
                        sections,
                        ..
                    },
            } => {
                // Merge positional files for hint context (view merging happens later)
                let file = if file_positional.is_empty() {
                    file
                } else {
                    file_positional
                };
                let mut ctx = HintContext::from_common(HintSource::Find, &common);
                ctx.glob.clone_from(glob);
                ctx.fields.clone_from(fields);
                ctx.sort.clone_from(sort);
                ctx.has_limit = limit.is_some();
                ctx.has_body_search = pattern.is_some();
                ctx.body_pattern.clone_from(pattern);
                ctx.has_regex_search = regexp.is_some();
                ctx.property_filters.clone_from(properties);
                ctx.tag_filters.clone_from(tag);
                ctx.task_filter.clone_from(task);
                ctx.file_targets.clone_from(file);
                ctx.section_filters.clone_from(sections);
                ctx.view_name.clone_from(view);
                Some(ctx)
            }
            Commands::Set {
                file_positional,
                file,
                glob,
                dry_run,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Set, &common);
                ctx.glob.clone_from(glob);
                let src = if file_positional.is_empty() {
                    file
                } else {
                    file_positional
                };
                ctx.file_targets.clone_from(src);
                ctx.dry_run = *dry_run;
                Some(ctx)
            }
            Commands::Remove {
                file_positional,
                file,
                glob,
                dry_run,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Remove, &common);
                ctx.glob.clone_from(glob);
                let src = if file_positional.is_empty() {
                    file
                } else {
                    file_positional
                };
                ctx.file_targets.clone_from(src);
                ctx.dry_run = *dry_run;
                Some(ctx)
            }
            Commands::Append {
                file_positional,
                file,
                glob,
                dry_run,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Append, &common);
                ctx.glob.clone_from(glob);
                let src = if file_positional.is_empty() {
                    file
                } else {
                    file_positional
                };
                ctx.file_targets.clone_from(src);
                ctx.dry_run = *dry_run;
                Some(ctx)
            }
            Commands::Read {
                file_positional,
                file,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Read, &common);
                if let Some(f) = file_positional.as_ref().or(file.as_ref()) {
                    ctx.file_targets = vec![f.clone()];
                }
                Some(ctx)
            }
            Commands::Backlinks {
                file_positional,
                file,
            } => {
                let mut ctx = HintContext::from_common(HintSource::Backlinks, &common);
                if let Some(f) = file_positional.as_ref().or(file.as_ref()) {
                    ctx.file_targets = vec![f.clone()];
                }
                Some(ctx)
            }
            Commands::Mv {
                file_positional,
                file,
                dry_run,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Mv, &common);
                if let Some(f) = file_positional.as_ref().or(file.as_ref()) {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.dry_run = *dry_run;
                Some(ctx)
            }
            Commands::Task { action } => {
                let (source, file_pos, file_flag, selector) = match action {
                    crate::cli::args::TaskAction::Toggle {
                        file_positional,
                        file,
                        line,
                        section,
                        all,
                    } => (
                        HintSource::TaskToggle,
                        file_positional,
                        file,
                        task_selector(line, section.as_ref(), *all),
                    ),
                    crate::cli::args::TaskAction::Set {
                        file_positional,
                        file,
                        line,
                        section,
                        all,
                        ..
                    } => (
                        HintSource::TaskSetStatus,
                        file_positional,
                        file,
                        task_selector(line, section.as_ref(), *all),
                    ),
                    crate::cli::args::TaskAction::Read {
                        file_positional,
                        file,
                        line,
                        section,
                        all,
                    } => (
                        HintSource::TaskRead,
                        file_positional,
                        file,
                        task_selector(line, section.as_ref(), *all),
                    ),
                };
                let mut ctx = HintContext::from_common(source, &common);
                if let Some(f) = file_pos.as_ref().or(file_flag.as_ref()) {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.task_selector = selector;
                Some(ctx)
            }
            Commands::Links { action } => match action {
                crate::cli::args::LinksAction::Fix { apply, glob, .. } => {
                    let mut ctx = HintContext::from_common(HintSource::LinksFix, &common);
                    ctx.glob.clone_from(glob);
                    ctx.dry_run = !apply;
                    Some(ctx)
                }
            },
            Commands::CreateIndex { output, .. } => {
                let mut ctx = HintContext::from_common(HintSource::CreateIndex, &common);
                ctx.index_path = output.as_ref().map(|p| p.to_string_lossy().into_owned());
                Some(ctx)
            }
            Commands::DropIndex { .. } => {
                Some(HintContext::from_common(HintSource::DropIndex, &common))
            }
            Commands::Properties { .. }
            | Commands::Tags { .. }
            | Commands::Init { .. }
            | Commands::Deinit
            | Commands::Completion { .. }
            | Commands::Views { .. }
            | Commands::Types { .. }
            | Commands::Lint { .. } => None,
        }
    } else {
        None
    };

    // Load snapshot index if --index is provided.
    // Read-only commands use it to skip disk scans. Mutation commands use it to
    // keep the index up-to-date after each file write (they still read/write
    // individual files on disk, but patch the index entry in-place).
    //
    // Resolve relative --index paths against the vault directory so that
    // `--index .hyalo-index` (the default) works regardless of CWD.
    if let Some(ref p) = cli.index
        && p.is_relative()
    {
        cli.index = Some(dir.join(p));
    }
    let uses_index = !matches!(
        &cli.command,
        Commands::Init { .. }
            | Commands::CreateIndex { .. }
            | Commands::DropIndex { .. }
            | Commands::Read { .. }
            | Commands::Views { .. }
            | Commands::Lint { .. }
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

    let config_language_owned = config.search_language.clone();
    let schema = config.schema;
    let mut ctx = CommandContext {
        dir: &dir,
        site_prefix,
        effective_format,
        user_format: format,
        snapshot_index: &mut snapshot_index,
        index_path: cli.index.as_deref(),
        config_language: config_language_owned.as_deref(),
        schema: &schema,
        exit_code_override: None,
    };
    let result = dispatch(cli.command, &mut ctx);
    let exit_code_override = ctx.exit_code_override;

    let pipeline = OutputPipeline {
        user_format: format,
        jq_filter,
        hint_ctx: hint_ctx.as_ref(),
        count: cli.count,
    };
    let code = pipeline.finalize(result);
    // Commands like `lint` may override the exit code even on success output.
    let final_code = exit_code_override.unwrap_or(code);
    if final_code == 0 {
        Ok(())
    } else {
        Err(AppError::Exit(final_code))
    }
}
