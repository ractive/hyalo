use std::io::IsTerminal as _;
use std::path::Path;
use std::process;
use std::time::Instant;

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};

use crate::cli::args::{Cli, Commands, FindFilters, IndexFlags};
use crate::cli::banner::cwd_help_banner;
use crate::cli::help::{filter_examples, filter_long_help};
use crate::commands::files_from::FilesFromCounters;
use crate::commands::init as init_commands;
use crate::dispatch::{CommandContext, dispatch};
use crate::error::AppError;
use crate::hints::{CommonHintFlags, HintContext, HintSource};
use crate::output::{CommandOutcome, Format};
use crate::output_pipeline::{COUNT_UNSUPPORTED_ERROR, OutputPipeline};
use hyalo_core::index::SnapshotIndex;

/// The explicit `--profile <name>` from a command, if any. Only `hyalo lint`
/// accepts an ephemeral `--profile` overlay today; the scan-include installer
/// consults this so a `--profile skills` run reaches `.claude/skills/` even on
/// a vault not yet initialized with the profile.
fn active_profile_name(command: &Commands) -> Option<&str> {
    match command {
        Commands::Lint {
            profile: Some(name),
            ..
        } => Some(name.as_str()),
        _ => None,
    }
}

/// Resolve the default output format based on whether stdout is a TTY.
///
/// - TTY (interactive terminal): `Format::Text` — human-readable by default.
/// - Piped / redirected: `Format::Json` — machine-readable by default.
///
/// Takes an `is_tty: bool` parameter so callers can inject a test value.
/// Production call site: `resolve_format_by_tty(std::io::stdout().is_terminal())`.
pub(crate) fn resolve_format_by_tty(is_tty: bool) -> Format {
    if is_tty { Format::Text } else { Format::Json }
}

/// Best-effort output format for errors raised while resolving `--dir`,
/// before the full format-resolution block (which needs the final `dir`/
/// `config` and therefore runs later) has executed.
///
/// Mirrors that later precedence — explicit `--format` > `--jq` forcing
/// JSON > config `format` > TTY detection — using whichever config is
/// already in scope at the call site. For an invalid `--dir` (missing or a
/// file) there is no target config to reload anyway, so the ambient config
/// is the only one that could plausibly apply; this is not an approximation
/// in that case, just an early evaluation of the same rule.
fn early_format(
    cli_format: Option<Format>,
    jq_present: bool,
    config_format: Option<&str>,
) -> Format {
    cli_format
        .or(if jq_present { Some(Format::Json) } else { None })
        .or_else(|| config_format.and_then(Format::from_str_opt))
        .unwrap_or_else(|| resolve_format_by_tty(std::io::stdout().is_terminal()))
}

/// Express the resolved vault `dir` as a path relative to `cwd`, using
/// forward slashes, for `--format github` annotation prefixing.
///
/// GitHub resolves annotation `file=` paths against the workspace (repo) root,
/// which is assumed to be the CWD. Strategy:
///
///   1. If `dir` is already relative (the common case — `.hyalo.toml` sets
///      `dir = "hyalo-knowledgebase"`), it *is* the CWD-relative prefix; a bare
///      `.` collapses to the empty prefix.
///   2. Otherwise, canonicalize both `dir` and `cwd` and strip the CWD prefix.
///   3. If the vault lies outside the CWD (or canonicalization fails), fall back
///      to the empty prefix so paths stay vault-relative rather than emitting a
///      confusing absolute or `../`-laden path.
fn vault_dir_relative_to_cwd(dir: &std::path::Path, cwd: &std::path::Path) -> String {
    let to_fwd = |p: &std::path::Path| p.to_string_lossy().replace('\\', "/");
    let clean = |s: String| -> String {
        let s = s.strip_prefix("./").unwrap_or(&s).to_owned();
        let trimmed = s.trim_end_matches('/');
        if trimmed == "." {
            String::new()
        } else {
            trimmed.to_owned()
        }
    };

    if dir.is_relative() {
        return clean(to_fwd(dir));
    }

    if let (Ok(dir_abs), Ok(cwd_abs)) = (dunce::canonicalize(dir), dunce::canonicalize(cwd))
        && let Ok(rel) = dir_abs.strip_prefix(&cwd_abs)
    {
        return clean(to_fwd(rel));
    }

    String::new()
}

/// Extract the effective index path from whichever subcommand is active.
///
/// Walks the command tree and retrieves `IndexFlags` from the matching arm,
/// then delegates to `IndexFlags::effective_index_path`.
/// Relative `--index-file` paths are resolved against the current working directory.
/// Returns `None` for commands that do not carry `IndexFlags`.
///
/// `global_index_file` is the value of the top-level `--index-file` flag; it
/// is used as a fallback when the subcommand does not specify its own path.
/// The subcommand value always takes precedence.
fn effective_index_path_for(
    cmd: &Commands,
    vault_dir: &std::path::Path,
    global_index_file: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    use crate::cli::args::{LinksAction, PropertiesAction, TagsAction, TaskAction};

    let flags: Option<&IndexFlags> = match cmd {
        Commands::Find { index_flags, .. }
        | Commands::Summary { index_flags, .. }
        | Commands::Backlinks { index_flags, .. }
        | Commands::Set { index_flags, .. }
        | Commands::Remove { index_flags, .. }
        | Commands::Append { index_flags, .. }
        | Commands::Mv { index_flags, .. }
        | Commands::Read { index_flags, .. }
        | Commands::Lint { index_flags, .. }
        | Commands::New { index_flags, .. } => Some(index_flags),
        Commands::Tags { action } => match action {
            Some(
                TagsAction::Summary { index_flags, .. } | TagsAction::Rename { index_flags, .. },
            ) => Some(index_flags),
            None => None,
        },
        Commands::Properties { action } => match action {
            Some(
                PropertiesAction::Summary { index_flags, .. }
                | PropertiesAction::Rename { index_flags, .. },
            ) => Some(index_flags),
            None => None,
        },
        Commands::Links { action } => match action {
            Some(LinksAction::Fix { index_flags, .. } | LinksAction::Auto { index_flags, .. }) => {
                Some(index_flags)
            }
            None => None,
        },
        Commands::Task { action } => match action {
            TaskAction::Read { index_flags, .. }
            | TaskAction::Toggle { index_flags, .. }
            | TaskAction::Set { index_flags, .. } => Some(index_flags),
        },
        // CreateIndex never *reads* an index — the global --index-file is an
        // output-path synonym there (merged into --output in run_inner). Return
        // early so we don't attempt to load a non-existent target as an input.
        Commands::CreateIndex { .. } => return None,
        Commands::DropIndex { .. }
        | Commands::Init { .. }
        | Commands::Deinit
        | Commands::Completion { .. }
        | Commands::Config
        | Commands::Types { .. }
        | Commands::Okf { .. }
        | Commands::Madr { .. }
        | Commands::Changelog { .. }
        | Commands::LintRules { .. } => None,
        Commands::Views { action } => match action {
            Some(crate::cli::args::ViewsAction::Run { index_flags, .. }) => Some(index_flags),
            _ => None,
        },
    };

    // Subcommand flags take precedence; fall back to global --index-file.
    let (raw, came_from_index_file) = if let Some(flags) = flags {
        if let Some(path) = flags.effective_index_path(vault_dir) {
            let came_from_file = flags.index_file.is_some();
            (path, came_from_file)
        } else {
            let global = global_index_file?;
            (global.to_path_buf(), true)
        }
    } else {
        let global = global_index_file?;
        (global.to_path_buf(), true)
    };

    // Relative --index-file paths are resolved against CWD.
    // Bare --index already returns an absolute-or-relative-to-vault path from
    // effective_index_path(), so only resolve when the path is still relative
    // and it came from --index-file (not bare --index).
    let resolved = if raw.is_relative() && came_from_index_file {
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        cwd.join(&raw)
    } else {
        raw
    };
    Some(resolved)
}

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

/// Check whether the command's injected `file` list is empty after `--files-from` resolution.
///
/// Called after [`resolve_files_from_for_command`] to detect "all entries filtered
/// out" — used by the caller to short-circuit dispatch with an empty result rather
/// than letting the command fall through to a full-vault scan.
#[allow(clippy::match_same_arms)]
fn files_from_command_file_list_is_empty(cmd: &Commands) -> bool {
    match cmd {
        Commands::Find {
            filters: FindFilters { file, .. },
            ..
        } => file.is_empty(),
        Commands::Lint { file, .. } => file.is_empty(),
        Commands::Set { file, .. } => file.is_empty(),
        Commands::Remove { file, .. } => file.is_empty(),
        Commands::Append { file, .. } => file.is_empty(),
        Commands::Mv { glob, .. } => glob.is_empty(),
        _ => false,
    }
}

/// Produce an empty successful `CommandOutcome` for a command when `--files-from`
/// resolved to zero files.  The payload is the command's natural "zero results" shape.
fn empty_result_for_command(cmd: &Commands) -> CommandOutcome {
    // For find: empty array with total=0.
    // For lint: empty lint output.
    // For mutation commands (set/remove/append/mv): empty array.
    match cmd {
        Commands::Find { .. } => {
            CommandOutcome::success_with_total(serde_json::json!([]).to_string(), 0)
        }
        Commands::Lint { .. } => {
            let payload = serde_json::json!({
                "files": [],
                "total": 0,
                "rules_fired": 0,
                "files_with_violations": 0,
                "files_truncated": false,
                "errors": 0,
                "warnings": 0
            });
            CommandOutcome::success_with_total(payload.to_string(), 0)
        }
        // Mutation commands: empty array
        _ => CommandOutcome::success_with_total(serde_json::json!([]).to_string(), 0),
    }
}

/// Pre-dispatch `--files-from` resolution for commands that accept it.
///
/// Delegates to [`crate::commands::inputs::resolve_files_from_to_rel_paths`]
/// which is the single file-resolution entry point for the entire application.
///
/// When a command carries a `files_from` source this function:
/// 1. Resolves path lines from the source (file or stdin `-`) via the unified resolver.
/// 2. Injects the resolved vault-relative paths into the command's `file` Vec
///    (or `glob` Vec for `mv` batch mode), clearing competing selectors.
/// 3. Returns `Some(FilesFromCounters)` for the output pipeline to merge into the envelope.
///
/// Returns `Ok(None)` when the command does not carry `--files-from`.
/// Returns `Err(...)` only for I/O failures reading the source.
///
/// When the resolved file list is empty, the caller is expected to use
/// [`files_from_command_file_list_is_empty`] to short-circuit dispatch.
#[allow(clippy::match_same_arms)]
fn resolve_files_from_for_command(
    cmd: &mut Commands,
    dir: &Path,
    configured_dir: &str,
    snapshot_index: Option<&hyalo_core::index::SnapshotIndex>,
) -> Result<Option<FilesFromCounters>> {
    use crate::commands::inputs::resolve_files_from_to_rel_paths;

    match cmd {
        Commands::Find {
            filters:
                FindFilters {
                    files_from,
                    file,
                    glob,
                    ..
                },
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            *file = paths;
            glob.clear();
            Ok(Some(counters))
        }
        Commands::Lint {
            files_from,
            file,
            file_positional,
            glob,
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            *file = paths;
            file_positional.clear();
            glob.clear();
            Ok(Some(counters))
        }
        Commands::Set {
            files_from,
            file,
            file_positional,
            glob,
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            *file = paths;
            file_positional.clear();
            glob.clear();
            Ok(Some(counters))
        }
        Commands::Remove {
            files_from,
            file,
            file_positional,
            glob,
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            *file = paths;
            file_positional.clear();
            glob.clear();
            Ok(Some(counters))
        }
        Commands::Append {
            files_from,
            file,
            file_positional,
            glob,
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            *file = paths;
            file_positional.clear();
            glob.clear();
            Ok(Some(counters))
        }
        Commands::Mv {
            files_from,
            glob,
            file,
            file_positional,
            ..
        } => {
            let Some(source) = files_from.take() else {
                return Ok(None);
            };
            let (paths, counters) =
                resolve_files_from_to_rel_paths(&source, dir, configured_dir, snapshot_index)?;
            // Mv batch mode is driven by --glob/--property/--tag/--type selectors,
            // so we feed the resolved vault-relative paths into `glob`. Each path
            // is a literal (no wildcards), and globset treats a literal pattern
            // as an exact-match — so this selects exactly the listed files.
            *glob = paths;
            *file = None;
            *file_positional = None;
            Ok(Some(counters))
        }
        _ => Ok(None),
    }
}

#[allow(clippy::too_many_lines)]
pub fn run() {
    crate::broken_pipe::install();
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
    let hide_format = config.format.as_deref().is_some_and(|f| f != "json");

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let cwd_has_config = cwd.join(".hyalo.toml").is_file();

    let mut cmd = Cli::command();
    if hide_dir {
        cmd = cmd.mut_arg("dir", |a| a.hide(true));
    }
    if hide_format {
        cmd = cmd.mut_arg("format", |a| a.hide(true));
    }

    // Append `(kb dir: <dir>)` to the version string when .hyalo.toml is in CWD.
    // Preserves the git-provenance suffix produced by `build_version_string()`.
    if cwd_has_config {
        let version_with_dir = format!(
            "{} (kb dir: {})",
            crate::cli::args::build_version_string(),
            config.dir.display()
        );
        cmd = cmd.version(version_with_dir);
    }

    // Prepend CWD-aware banner when relevant (info if .hyalo.toml in CWD,
    // warning if running from inside the vault). Shown by both -h and --help.
    if let Some(banner) = cwd_help_banner() {
        cmd = cmd.before_help(banner.clone()).before_long_help(banner);
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

            // Intercept `--tag` / `-t` on the `append` subcommand. Tags are
            // scalar list items, so there is nothing to "append" in the
            // property-level sense — `hyalo set --tag T` is the right tool.
            // Surface that hint instead of clap's generic unknown-arg error.
            //
            // Gate the hint on the *resolved* top-level subcommand rather
            // than a substring scan, so unrelated commands whose args happen
            // to include `append` (e.g. `hyalo find append`) don't get the
            // `hyalo append`-specific message.
            if e.kind() == clap::error::ErrorKind::UnknownArgument
                && crate::suggest::top_level_subcommand(&raw_args, &Cli::command())
                    == Some("append")
                && (crate::suggest::unknown_arg_is(&e, "--tag")
                    || crate::suggest::unknown_arg_is(&e, "-t"))
            {
                eprintln!(
                    "error: `hyalo append` does not accept --tag (tags are scalar list items, not appendable)\n\n\
                     hint: use `hyalo set <file> --tag <tag>` to add a tag\n"
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
            // BUT: scope this to top-level subcommands only — don't fire when the
            // parent context is already a known subcommand like `properties`.
            if e.kind() == clap::error::ErrorKind::InvalidSubcommand {
                use clap::error::{ContextKind, ContextValue};
                let top_sub = crate::suggest::top_level_subcommand(&raw_args, &Cli::command());
                let parent_is_properties = matches!(top_sub, Some("properties" | "property"));
                let parent_is_views = top_sub == Some("views");
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
                    // Special hint for `hyalo views <name>`: suggest `views run <name>`
                    // when `<name>` matches a known view in .hyalo.toml.
                    if parent_is_views {
                        // Load views from the config resolved so far (use CWD as fallback).
                        let config_dir_for_views = config.config_dir.clone();
                        let known_views = crate::commands::views::load_views(&config_dir_for_views);
                        if known_views.contains_key(invalid) {
                            eprintln!("{e}\n  hint: did you mean 'hyalo views run {invalid}'?\n");
                            return Err(AppError::Exit(2));
                        }
                        // If not an exact match, still give a generic hint.
                        eprintln!(
                            "{e}\n  hint: to run a saved view use 'hyalo views run <name>' \
                             (run 'hyalo views list' to see all views)\n"
                        );
                        return Err(AppError::Exit(2));
                    }
                    // Special hint for `hyalo properties <something>` typos.
                    if parent_is_properties {
                        eprintln!(
                            "{e}\n  hint: 'properties' has subcommands; try 'hyalo properties summary' or 'hyalo properties rename'\n"
                        );
                        return Err(AppError::Exit(2));
                    }
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
            &cli.command,
            Commands::Init { .. }
                | Commands::Deinit
                | Commands::Completion { .. }
                | Commands::Config
        )
    {
        let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
        eprintln!(
            "{}",
            crate::output::format_error(fmt, COUNT_UNSUPPORTED_ERROR, None, None, None)
        );
        // User error (unsupported flag for this command) → exit 1, not 2
        // (2 is reserved for internal errors — iter-181 task 2).
        return Err(AppError::Exit(1));
    }
    if let Commands::Init {
        claude,
        pi,
        profile,
    } = &mut cli.command
    {
        let init_dir = cli.dir.as_deref().and_then(|p| p.to_str());
        match init_commands::run_init(init_dir, *claude, *pi, profile.as_deref()) {
            Ok(CommandOutcome::Success { output, .. } | CommandOutcome::RawOutput(output)) => {
                // Sanitized because RawOutput content may echo raw file text (init/deinit
                // summaries can include vault-derived strings) that never passes through
                // the JSON pipeline's own sanitization.
                println!("{}", crate::output::sanitize_control_chars(&output));
                return Ok(());
            }
            Ok(CommandOutcome::UserError(output)) => return Err(AppError::User(output)),
            Err(e) => return Err(AppError::Internal(e)),
        }
    }
    if let Commands::Deinit = &mut cli.command {
        match init_commands::run_deinit() {
            Ok(CommandOutcome::Success { output, .. } | CommandOutcome::RawOutput(output)) => {
                // Sanitized because RawOutput content may echo raw file text (init/deinit
                // summaries can include vault-derived strings) that never passes through
                // the JSON pipeline's own sanitization.
                println!("{}", crate::output::sanitize_control_chars(&output));
                return Ok(());
            }
            Ok(CommandOutcome::UserError(output)) => return Err(AppError::User(output)),
            Err(e) => return Err(AppError::Internal(e)),
        }
    }
    if let Commands::Completion { shell } = &mut cli.command {
        let mut cmd = Cli::command();
        clap_complete::generate(*shell, &mut cmd, "hyalo", &mut std::io::stdout());
        return Ok(());
    }
    // `config` inspects CWD directly and does not need normal pipeline setup.
    // Dispatch before config validation (dir-doesn't-exist check) so it always works.
    if let Commands::Config = &mut cli.command {
        // Determine output format (respect --format if given; otherwise default to Text
        // since this command is read-only introspection, not a pipeline command).
        let format = cli.format.unwrap_or_else(|| {
            if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                crate::output::Format::Text
            } else {
                crate::output::Format::Json
            }
        });
        // A `--dir` override wins over the config's own `dir`: report the
        // effective vault directory the rest of the CLI would use, not the
        // config-file value it shadows (ff-rdp B6). When `--dir` names a
        // directory that has its own `.hyalo.toml`, load from there.
        let dir_override = cli.dir.as_deref();
        let report = crate::commands::config::collect_config_report(&cwd, dir_override)
            .map_err(AppError::Internal)?;
        match crate::commands::config::run_config(&report, format) {
            CommandOutcome::Success { output, .. } | CommandOutcome::RawOutput(output) => {
                // Sanitized because the text-mode RawOutput branch echoes the raw
                // .hyalo.toml contents (`report.raw_contents`), which never passes
                // through the JSON pipeline's own sanitization.
                print!("{}", crate::output::sanitize_control_chars(&output));
                return Ok(());
            }
            CommandOutcome::UserError(output) => return Err(AppError::User(output)),
        }
    }
    // Merge: CLI args override config, config overrides hardcoded defaults.
    // Track whether --dir was explicitly passed (not from config) so hints
    // can omit it when the user relies on .hyalo.toml.
    let dir_from_cli = cli.dir.is_some();
    // Capture the raw CLI --dir string before it's consumed by the match below.
    // Used later to compute `configured_dir_str` for --files-from prefix
    // stripping: when --dir is explicit, the target's .hyalo.toml may report
    // config.dir = "." (no config file found), losing the multi-segment prefix
    // the user passed (e.g. "files/en-us"). Saving the raw CLI string here
    // lets us restore it as the effective configured_dir for the resolver.
    let cli_dir_str: Option<String> = cli.dir.as_deref().map(|p| p.to_string_lossy().into_owned());
    let format_from_cli = cli.format.is_some();
    let hints_from_cli = cli.hints;
    // Save the CWD-derived vault dir for the redundant-dir warning below.
    // We need this before `config` is potentially shadowed by the target config.
    let cwd_config_resolved_dir = config.config_dir.join(&config.dir);
    let cwd_config_dir_str = config.dir.display().to_string();
    // Whether the CWD `.hyalo.toml` was actually parsed cleanly. Used to gate
    // the redundant-`--dir` warning so we don't claim a malformed/ignored config
    // "already sets dir = ...".
    let cwd_config_parsed_ok = config.loaded_from_file;
    // Determine the effective vault directory and the config to use:
    //
    // - When --dir is explicitly provided on the CLI, validate first, then
    //   reload .hyalo.toml from the target directory so its schema, format,
    //   hints, site_prefix, and search config apply — not the caller's CWD
    //   config.
    // - Otherwise, keep the CWD config (already loaded) and use its dir.
    let (dir, config) = if let Some(cli_dir) = cli.dir {
        // Validate before loading config to avoid misleading file-read warnings.
        if !cli_dir.exists() {
            let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
            return Err(AppError::User(crate::output::format_error(
                fmt,
                &format!("--dir path '{}' does not exist.", cli_dir.display()),
                None,
                None,
                None,
            )));
        }
        if cli_dir.is_file() {
            let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
            return Err(AppError::User(crate::output::format_error(
                fmt,
                &format!(
                    "--dir path '{}' is a file, not a directory. Use --file to target a single file.",
                    cli_dir.display()
                ),
                None,
                None,
                None,
            )));
        }
        let target_config = crate::config::load_config_from(&cli_dir);
        (cli_dir, target_config)
    } else {
        let vault_dir = config.dir.clone();
        (vault_dir, config)
    };
    // The directory where .hyalo.toml lives. Views/types are stored there.
    let config_dir = config.config_dir.clone();

    // Install the `[scan] include` globs process-wide so every command's file
    // discovery descends into the opted-in hidden dot-subtrees. A `--profile`
    // overlay (below) can add to this via its fragment; union those in now so
    // an un-initialized vault run with `--profile skills` still reaches
    // `.claude/skills/`.
    {
        let mut include = config.scan_include.clone();
        if let Some(profile_name) = active_profile_name(&cli.command) {
            let extra = crate::config::overlay_scan_include(&config_dir, profile_name);
            for pat in extra {
                if !include.contains(&pat) {
                    include.push(pat);
                }
            }
        }
        for (pat, msg) in hyalo_core::discovery::set_scan_include(&include) {
            crate::warn::warn(format!("invalid [scan] include glob {pat:?}: {msg}"));
        }
    }

    // Warn when --dir is redundant: the user passed a dir that matches what
    // .hyalo.toml would have resolved to anyway. Only fires when .hyalo.toml
    // is present AND parsed successfully — otherwise the loader fell back to
    // defaults and the message would be misleading.
    if dir_from_cli && cwd_has_config && cwd_config_parsed_ok {
        // Compare the CLI-provided dir against what the CWD config resolved to.
        // `cwd_config_resolved_dir` was captured before `config` was shadowed.
        if let (Ok(a), Ok(b)) = (
            dunce::canonicalize(&dir),
            dunce::canonicalize(&cwd_config_resolved_dir),
        ) && a == b
        {
            crate::warn::note(format!(
                "--dir is redundant; .hyalo.toml already sets dir = \"{cwd_config_dir_str}\""
            ));
        }
    }

    // Validate that the resolved dir exists and is a directory (for the
    // non-CLI case where dir comes from .hyalo.toml).
    if !dir.exists() {
        let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
        return Err(AppError::User(crate::output::format_error(
            fmt,
            &format!("--dir path '{}' does not exist.", dir.display()),
            None,
            None,
            None,
        )));
    }
    if dir.is_file() {
        let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
        return Err(AppError::User(crate::output::format_error(
            fmt,
            &format!(
                "--dir path '{}' is a file, not a directory. Use --file to target a single file.",
                dir.display()
            ),
            None,
            None,
            None,
        )));
    }

    // LLM-driven shells (Claude Code etc.) often `cd` into the configured
    // vault dir and pass paths relative to that subdir. The current command
    // works, but the next call from a sibling dir blows up. If CWD is inside
    // the configured vault, warn once. Skipped when --dir was passed
    // explicitly: the user has named the vault directly, so the ancestor
    // walk would just produce false positives from unrelated `.hyalo.toml`
    // files. Init/Deinit/Completion early-return above this point.
    if !dir_from_cli {
        crate::warn::warn_if_cwd_in_vault();
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
    // Resolve the output format.
    //
    // Precedence (highest first):
    //   1. Explicit `--format` CLI flag.
    //   2. `--jq` (forces JSON unless an explicit format is set, since jq
    //      operates on JSON — without this, TTY users running
    //      `hyalo find ... --jq '...'` would hit the format-conflict error).
    //   3. `format = "..."` in `.hyalo.toml`.
    //   4. TTY detection: `text` when stdout is a terminal, `json` when piped.
    let format = if let Some(f) = cli.format {
        f
    } else if cli.jq.is_some() {
        Format::Json
    } else if let Some(ref fmt_str) = config.format {
        if let Some(fmt) = Format::from_str_opt(fmt_str) {
            fmt
        } else {
            eprintln!(
                "Invalid output format '{fmt_str}' in .hyalo.toml; supported formats are: json, text"
            );
            // Misconfiguration is a user error → exit 1 (iter-181 task 2).
            return Err(AppError::Exit(1));
        }
    } else {
        // No explicit flag or config — use TTY detection.
        resolve_format_by_tty(std::io::stdout().is_terminal())
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
        view: Some(view_name),
        filters,
        ..
    } = &mut cli.command
    {
        let views = crate::commands::views::load_views(&config_dir);
        if let Some(base) = views.get(view_name) {
            let overlay = std::mem::take(filters);
            *filters = base.clone();
            filters.merge_from(&overlay);
        } else {
            // Offer a fuzzy suggestion when the view name is a close typo
            // of a known view (reuses the same threshold as --tag/--property).
            const MAX_DIST: usize = 2;
            let known: Vec<&str> = views.keys().map(String::as_str).collect();
            let suggestion = known
                .iter()
                .map(|k| (strsim::damerau_levenshtein(view_name, k), *k))
                .filter(|(d, _)| *d <= MAX_DIST)
                .min_by_key(|(d, _)| *d)
                .map(|(_, k)| k);
            let tip = if let Some(s) = suggestion {
                format!("did you mean: hyalo find --view {s}?")
            } else {
                "run 'hyalo views list' to see available views".to_owned()
            };
            return Err(AppError::User(crate::output::format_error(
                format,
                &format!("unknown view '{view_name}'"),
                None,
                Some(&tip),
                None,
            )));
        }
    }

    // If the CLI didn't supply a pattern but the view did, propagate it.
    // Skip when --regexp is active — BM25 pattern and regex are mutually exclusive
    // (clap enforces this for CLI args, but a view's pattern bypasses clap).
    if let Commands::Find {
        pattern, filters, ..
    } = &mut cli.command
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
    let format =
        if !format_from_cli && jq_filter.is_none() && matches!(&cli.command, Commands::Read { .. })
        {
            Format::Text
        } else {
            format
        };
    // --count replaces the entire output pipeline, so check its conflicts first.
    if cli.count && jq_filter.is_some() {
        eprintln!(
            "{}",
            crate::output::format_error(
                format,
                "--count cannot be combined with --jq",
                None,
                Some(
                    "--count prints the bare total; --jq applies a custom filter — use one or the other"
                ),
                None,
            )
        );
        // Conflicting user flags → exit 1 (iter-181 task 2).
        return Err(AppError::Exit(1));
    }
    if jq_filter.is_some() && format != Format::Json {
        eprintln!(
            "{}",
            crate::output::format_error(
                format,
                &format!("--jq cannot be combined with --format {format}"),
                None,
                Some("--jq always operates on JSON output; drop --format or use --format json"),
                None,
            )
        );
        // --jq + --format text is a user error → exit 1, not 2 (iter-181 task 2).
        return Err(AppError::Exit(1));
    }
    // `--format github` is lint-only: it emits GitHub Actions workflow commands
    // for lint violations. Reject it for every other subcommand with a clear
    // message listing the valid formats, so `hyalo find --format github` fails
    // fast instead of producing meaningless output.
    if format == Format::Github && !matches!(cli.command, Commands::Lint { .. }) {
        eprintln!(
            "{}",
            crate::output::format_error(
                Format::Text,
                "--format github is only supported by `hyalo lint`",
                None,
                Some("valid formats for this command are: json, text"),
                None,
            )
        );
        // Unsupported format for this command is a user error → exit 1 (iter-181 task 2).
        return Err(AppError::Exit(1));
    }
    // `--count` prints a bare integer; it is meaningless alongside the
    // annotation stream `--format github` produces. Reject the combination.
    if format == Format::Github && cli.count {
        eprintln!(
            "{}",
            crate::output::format_error(
                Format::Text,
                "--count cannot be combined with --format github",
                None,
                Some("--format github emits inline annotations; drop --count to see them"),
                None,
            )
        );
        // Conflicting user flags → exit 1 (iter-181 task 2).
        return Err(AppError::Exit(1));
    }

    // Compute the annotation path prefix for `--format github`: the vault dir
    // expressed relative to CWD. GitHub resolves annotation `file=` paths
    // against the workspace (repo) root, but lint emits vault-relative paths, so
    // each is prefixed with this. CI is assumed to run from the repo root.
    let github_path_prefix = if format == Format::Github {
        vault_dir_relative_to_cwd(&dir, &cwd)
    } else {
        String::new()
    };

    // Always force JSON internally so the output pipeline can wrap results in the
    // envelope.  The user-requested format is applied by the pipeline afterwards.
    let effective_format = Format::Json;

    // Build hint context before the command dispatch.
    // Only include CLI-explicit flags in hints — config values are inherited
    // automatically when the user runs the hint command from the same CWD.
    let mut hint_ctx = if hints_flag && jq_filter.is_none() {
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
                action: Some(crate::cli::args::PropertiesAction::Summary { glob, limit, .. }),
            } => {
                let mut ctx = HintContext::from_common(HintSource::PropertiesSummary, &common);
                ctx.glob.clone_from(glob);
                ctx.has_limit = limit.is_some();
                Some(ctx)
            }
            Commands::Tags {
                action: Some(crate::cli::args::TagsAction::Summary { glob, limit, .. }),
            } => {
                let mut ctx = HintContext::from_common(HintSource::TagsSummary, &common);
                ctx.glob.clone_from(glob);
                ctx.has_limit = limit.is_some();
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
                        reverse,
                        limit,
                        sections,
                        broken_links,
                        orphan,
                        dead_end,
                        title,
                        ..
                    },
                ..
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
                ctx.reverse = *reverse;
                ctx.has_limit = limit.is_some();
                ctx.has_body_search = pattern.is_some();
                ctx.body_pattern.clone_from(pattern);
                ctx.has_regex_search = regexp.is_some();
                ctx.property_filters.clone_from(properties);
                ctx.tag_filters.clone_from(tag);
                ctx.task_filter.clone_from(task);
                ctx.file_targets.clone_from(file);
                ctx.section_filters.clone_from(sections);
                // Graph + title filters: preserved into derived hints so a
                // "narrow by tag" / "show all" hint on a `--orphan` /
                // `--broken-links` query keeps that scope (BUG-8).
                ctx.broken_links_filter = *broken_links;
                ctx.orphan_filter = *orphan;
                ctx.dead_end_filter = *dead_end;
                ctx.title_filter.clone_from(title);
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
            Commands::Read { selection, .. } => {
                let mut ctx = HintContext::from_common(HintSource::Read, &common);
                if let Some(f) = selection
                    .file_positional
                    .as_ref()
                    .or(selection.file.first())
                {
                    ctx.file_targets = vec![f.clone()];
                }
                Some(ctx)
            }
            Commands::Backlinks {
                selection, limit, ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Backlinks, &common);
                if let Some(f) = selection
                    .file_positional
                    .as_ref()
                    .or(selection.file.first())
                {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.has_limit = limit.is_some();
                Some(ctx)
            }
            Commands::Mv {
                file_positional,
                file,
                dry_run,
                apply,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Mv, &common);
                if let Some(f) = file_positional.as_ref().or(file.as_ref()) {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.dry_run = *dry_run || !apply;
                Some(ctx)
            }
            Commands::Task { action } => {
                let (source, selection, selector) = match action {
                    crate::cli::args::TaskAction::Toggle {
                        selection,
                        line,
                        section,
                        all,
                        ..
                    } => (
                        HintSource::TaskToggle,
                        selection,
                        task_selector(line, section.as_ref(), *all),
                    ),
                    crate::cli::args::TaskAction::Set {
                        selection,
                        line,
                        section,
                        all,
                        ..
                    } => (
                        HintSource::TaskSetStatus,
                        selection,
                        task_selector(line, section.as_ref(), *all),
                    ),
                    crate::cli::args::TaskAction::Read {
                        selection,
                        line,
                        section,
                        all,
                        ..
                    } => (
                        HintSource::TaskRead,
                        selection,
                        task_selector(line, section.as_ref(), *all),
                    ),
                };
                let mut ctx = HintContext::from_common(source, &common);
                if let Some(f) = selection
                    .file_positional
                    .as_ref()
                    .or(selection.file.first())
                {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.task_selector = selector;
                Some(ctx)
            }
            Commands::Links { action } => match action {
                Some(crate::cli::args::LinksAction::Fix { apply, glob, .. }) => {
                    let mut ctx = HintContext::from_common(HintSource::LinksFix, &common);
                    ctx.glob.clone_from(glob);
                    ctx.dry_run = !apply;
                    Some(ctx)
                }
                Some(crate::cli::args::LinksAction::Auto {
                    apply,
                    glob,
                    file,
                    min_length,
                    exclude_title,
                    ..
                }) => {
                    let mut ctx = HintContext::from_common(HintSource::LinksAuto, &common);
                    ctx.glob.clone_from(glob);
                    ctx.dry_run = !apply;
                    ctx.auto_link_file.clone_from(file);
                    ctx.auto_link_min_length = Some(*min_length);
                    ctx.auto_link_exclude_titles.clone_from(exclude_title);
                    Some(ctx)
                }
                None => {
                    // Default: dry-run fix
                    let mut ctx = HintContext::from_common(HintSource::LinksFix, &common);
                    ctx.dry_run = true;
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
            Commands::Lint {
                file_positional,
                file,
                glob,
                r#type: _,
                fix,
                dry_run,
                limit,
                rule,
                rule_prefix,
                fix_rule,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Lint, &common);
                ctx.glob.clone_from(glob);
                ctx.dry_run = *dry_run;
                ctx.has_limit = limit.is_some();
                ctx.lint_is_fix = *fix;
                ctx.lint_rule.clone_from(rule);
                ctx.lint_rule_prefix.clone_from(rule_prefix);
                ctx.lint_fix_rules.clone_from(fix_rule);
                let mut targets: Vec<String> = file_positional.clone();
                targets.extend(file.clone());
                ctx.file_targets = targets;
                Some(ctx)
            }
            Commands::Types { action } => {
                use crate::cli::args::TypesAction;
                let subcommand = match action {
                    Some(TypesAction::List) | None => Some("list".to_owned()),
                    Some(TypesAction::Show { .. }) => Some("show".to_owned()),
                    Some(TypesAction::Remove { .. }) => Some("remove".to_owned()),
                    Some(TypesAction::Set { .. }) => Some("set".to_owned()),
                };
                Some(HintContext::from_common(
                    HintSource::Types { subcommand },
                    &common,
                ))
            }
            Commands::New { file, .. } => Some(HintContext::from_common(
                HintSource::New { file: file.clone() },
                &common,
            )),
            Commands::Okf { action } => {
                use crate::cli::args::OkfAction;
                let source = match action {
                    OkfAction::Index { .. } => HintSource::OkfIndex,
                    OkfAction::Log { .. } => HintSource::OkfLog,
                };
                let mut ctx = HintContext::from_common(source, &common);
                // The validate hint drops the redundant `--profile okf` flag when
                // the profile is already active via `[lint] profiles`.
                ctx.okf_profile_active = config.lint_profiles.iter().any(|p| p == "okf");
                Some(ctx)
            }
            Commands::Properties { .. }
            | Commands::Tags { .. }
            | Commands::Init { .. }
            | Commands::Deinit
            | Commands::Completion { .. }
            | Commands::Config
            | Commands::Views { .. }
            | Commands::Madr { .. }
            | Commands::Changelog { .. }
            | Commands::LintRules { .. } => None,
        }
    } else {
        None
    };

    // Extract the effective index path from the subcommand's IndexFlags.
    // --index-file PATH wins; bare --index resolves to vault_dir/.hyalo-index.
    // Relative --index-file paths are resolved against CWD (caller convention).
    let index_path_buf: Option<std::path::PathBuf> =
        effective_index_path_for(&cli.command, &dir, cli.index_file.as_deref());

    // Propagate --quiet and has-index into hint context now that we know both.
    // `quiet` suppresses the slow-query hint; `has_index` suppresses all
    // index-suggestion hints when a snapshot is already in use.
    // `has_index` is set from index_path_buf because the snapshot load may fail
    // (fall back to disk scan), but the *intent* to use an index is what matters
    // for hint suppression — we don't want to suggest creating an index that the
    // user already requested.
    if let Some(ref mut ctx) = hint_ctx {
        ctx.quiet = cli.quiet;
        ctx.has_index = index_path_buf.is_some();
        // Preserve the active index into derived `find` hints so they query the
        // same snapshot rather than silently rescanning the vault (BUG-7
        // audit). A path equal to the default `<vault>/.hyalo-index` re-emits
        // as bare `--index`; any other path re-emits as `--index-file <path>`.
        if matches!(ctx.source, HintSource::Find)
            && let Some(ref p) = index_path_buf
        {
            let default_path = dir.join(".hyalo-index");
            ctx.find_index = if *p == default_path {
                HintContext::default_find_index()
            } else {
                HintContext::file_find_index(p.to_string_lossy().into_owned())
            };
        }
    }

    let mut snapshot_index: Option<SnapshotIndex> = if let Some(ref p) = index_path_buf {
        match SnapshotIndex::load(p) {
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
    };

    let config_language_owned = config.search_language.clone();
    let config_default_limit = config.default_limit;
    let mut schema = config.schema;
    let frontmatter_link_props_owned = config.frontmatter_link_props;
    let mut validate_on_write = config.validate_on_write;
    let lint_ignore = config.lint_ignore;
    let okf_ignore = config.okf_ignore;
    let changelog_path = config.changelog_path;
    let case_insensitive_mode = config.case_insensitive_mode;
    let mut md_lint = config.md_lint;
    let mut lint_strict_from_config = config.lint_strict;
    // Active conformance profiles: from `[lint] profiles` in `.hyalo.toml`, or
    // extended by an explicit `--profile` overlay below (which composes rather
    // than replaces, so a `--profile` flag adds to the file-activated set).
    let mut lint_profiles_active = config.lint_profiles;

    // `hyalo lint --profile <name>` overlays an embedded config fragment for this
    // invocation only (no `.hyalo.toml` write). The overlay reuses the same
    // fragment-merge code as `hyalo init --profile <name>`, so on a vault already
    // initialized that way it is idempotent — plain `hyalo lint` and
    // `hyalo lint --profile <name>` yield identical schema/rules. An unknown
    // profile is a hard user error surfaced before dispatch.
    if let Commands::Lint {
        profile: Some(profile_name),
        ..
    } = &cli.command
    {
        match crate::config::overlay_profile(&config_dir, profile_name) {
            Ok(overlay) => {
                schema = overlay.schema;
                md_lint = overlay.md_lint;
                validate_on_write = overlay.validate_on_write;
                // `overlay_profile` re-parses the *merged* (existing + fragment)
                // config, so `overlay.lint_strict` already reflects the correct
                // combined value — ORing in the pre-overlay value here would
                // incorrectly keep strict mode on even if the merged config
                // does not set it. An explicit `--strict` flag still wins later
                // in dispatch.
                lint_strict_from_config = overlay.lint_strict;
                // The explicit --profile activates every profile the merged
                // (existing file + fragment) config declares, so a `--profile`
                // flag *adds* to whatever `[lint] profiles` the vault already
                // activates rather than replacing it.
                lint_profiles_active = overlay.lint_profiles;
            }
            Err(e) => {
                return Err(AppError::User(crate::output::format_error(
                    format,
                    &format!("{e:#}"),
                    None,
                    None,
                    None,
                )));
            }
        }
    }

    // Propagate the configured frontmatter-link property list into the loaded
    // snapshot so that per-file refreshes (`rescan_entry` / `rename_entry`) use
    // the same list as the initial index build.
    if let Some(idx) = snapshot_index.as_mut() {
        idx.set_frontmatter_link_props(frontmatter_link_props_owned.clone());
    }
    // For `create-index`, merge the global `--index-file` flag into the
    // subcommand's `-o / --output` field.  Both are synonyms on this subcommand.
    // If both are provided and differ, return a clear user error.
    if let Commands::CreateIndex {
        output,
        allow_outside_vault: _,
    } = &mut cli.command
        && let Some(global_path) = cli.index_file.as_ref()
    {
        match output.as_ref() {
            // `--output` already set to the same value — no-op.
            Some(local) if local == global_path => {}
            // Both flags given with different values — conflict.
            Some(local) => {
                let out = crate::output::format_error(
                    effective_format,
                    "conflicting output paths for create-index",
                    None,
                    Some("pass either -o/--output or --index-file, not both with different paths"),
                    Some(&format!(
                        "--output = {}, --index-file = {}",
                        local.display(),
                        global_path.display()
                    )),
                );
                let pipeline = OutputPipeline {
                    user_format: format,
                    jq_filter,
                    hint_ctx: hint_ctx.as_ref(),
                    count: cli.count,
                    files_from_counters: None,
                    github_path_prefix: String::new(),
                };
                let code = pipeline.finalize(Ok(CommandOutcome::UserError(out)));
                return if code == 0 {
                    Ok(())
                } else {
                    Err(AppError::Exit(code))
                };
            }
            // Only `--index-file` provided — promote to `--output`.
            None => {
                *output = Some(global_path.clone());
            }
        }
    }

    // Resolve --files-from before dispatch. This converts the files_from source
    // into the command's `file` list and returns skip counters for the envelope.
    // When the snapshot is active, route resolution through the snapshot so paths
    // absent from the index count as missing (iter-143: --index → snapshot is
    // the source of truth, no disk fallback).
    //
    // Done *before* constructing `CommandContext` so the snapshot_index borrow
    // for resolution doesn't conflict with the `&mut` stored on ctx.
    // Compute the effective configured-dir string for --files-from prefix
    // stripping. Two sources in priority order:
    //
    // 1. Explicit `--dir <path>` on the CLI (relative or absolute as typed).
    //    When the target dir has no .hyalo.toml, config.dir falls back to "."
    //    which would suppress all prefix stripping. Using the raw CLI value
    //    instead preserves multi-segment dirs (e.g. "files/en-us") so that
    //    repo-relative git output like "files/en-us/foo.md" is resolved
    //    correctly (NEW-3).
    //
    // 2. `config.dir` from .hyalo.toml (e.g. "files/en-us", "kb", ".").
    let configured_dir_owned: String = match cli_dir_str {
        Some(s) => s,
        None => config.dir.to_string_lossy().into_owned(),
    };
    let configured_dir_str: &str = &configured_dir_owned;
    let (files_from_counters, files_from_empty) = match resolve_files_from_for_command(
        &mut cli.command,
        &dir,
        configured_dir_str,
        snapshot_index.as_ref(),
    ) {
        Ok(Some(c)) => {
            let empty = files_from_command_file_list_is_empty(&cli.command);
            (Some(c), empty)
        }
        Ok(None) => (None, false),
        Err(e) => {
            return Err(AppError::Internal(e));
        }
    };

    let mut ctx = CommandContext {
        dir: &dir,
        config_dir: &config_dir,
        configured_dir_str,
        site_prefix,
        effective_format,
        user_format: format,
        snapshot_index: &mut snapshot_index,
        index_path: index_path_buf.as_deref(),
        config_language: config_language_owned.as_deref(),
        frontmatter_link_props: frontmatter_link_props_owned.as_deref(),
        schema: &schema,
        validate_on_write,
        lint_ignore: &lint_ignore,
        okf_ignore: &okf_ignore,
        changelog_path: changelog_path.as_deref(),
        md_lint: &md_lint,
        case_insensitive_mode,
        exit_code_override: None,
        config_default_limit,
        programmatic_output: jq_filter.is_some() || cli.count,
        lint_strict: lint_strict_from_config,
        lint_profiles: lint_profiles_active,
        files_from_counters: None,
    };

    // When --files-from resolved to zero files (all entries filtered/missing),
    // short-circuit with an empty result rather than falling through to "scan all".
    //
    // Capture wall-clock elapsed around the dispatch body so the slow-query
    // hint can fire when the command took longer than SLOW_QUERY_THRESHOLD_MS.
    // We measure here (not inside dispatch) so hint rendering is excluded.
    let dispatch_start = Instant::now();
    let result = if files_from_empty {
        // Produce the appropriate empty payload for the command type.
        Ok(empty_result_for_command(&cli.command))
    } else {
        dispatch(cli.command, &mut ctx)
    };
    // Saturate at u64::MAX on absurdly long runs (> ~585 million years).
    let elapsed_ms = u64::try_from(dispatch_start.elapsed().as_millis()).unwrap_or(u64::MAX);

    // Inject elapsed into hint context so slow_query_hint can read it.
    if let Some(ref mut hctx) = hint_ctx {
        hctx.elapsed_ms = Some(elapsed_ms);
    }

    let exit_code_override = ctx.exit_code_override;
    // Prefer counters captured inside dispatch (read/backlinks/task path through
    // `resolve_inputs`); fall back to the pre-dispatch path used by other commands.
    let final_files_from_counters = ctx.files_from_counters.take().or(files_from_counters);

    let pipeline = OutputPipeline {
        user_format: format,
        jq_filter,
        hint_ctx: hint_ctx.as_ref(),
        count: cli.count,
        files_from_counters: final_files_from_counters,
        github_path_prefix,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// When stdout is a TTY, the default format should be `Text`.
    #[test]
    fn resolve_format_by_tty_returns_text_for_tty() {
        assert_eq!(resolve_format_by_tty(true), Format::Text);
    }

    /// When stdout is piped (not a TTY), the default format should be `Json`.
    #[test]
    fn resolve_format_by_tty_returns_json_for_pipe() {
        assert_eq!(resolve_format_by_tty(false), Format::Json);
    }

    /// A relative vault dir is used verbatim as the CWD-relative prefix.
    #[test]
    fn vault_prefix_relative_dir_used_verbatim() {
        let cwd = std::path::Path::new("/repo");
        assert_eq!(
            vault_dir_relative_to_cwd(std::path::Path::new("hyalo-knowledgebase"), cwd),
            "hyalo-knowledgebase"
        );
        assert_eq!(
            vault_dir_relative_to_cwd(std::path::Path::new("sub/kb"), cwd),
            "sub/kb"
        );
        assert_eq!(
            vault_dir_relative_to_cwd(std::path::Path::new("./kb/"), cwd),
            "kb"
        );
    }

    /// A `.` vault dir (vault == CWD) collapses to an empty prefix.
    #[test]
    fn vault_prefix_dot_dir_is_empty() {
        let cwd = std::path::Path::new("/repo");
        assert_eq!(
            vault_dir_relative_to_cwd(std::path::Path::new("."), cwd),
            ""
        );
    }

    /// An absolute vault dir under the CWD is stripped to a relative prefix.
    #[test]
    fn vault_prefix_absolute_dir_under_cwd() {
        let tmp = std::env::temp_dir().join(format!("hyalo-prefix-{}", std::process::id()));
        let kb = tmp.join("kb");
        std::fs::create_dir_all(&kb).unwrap();
        let got = vault_dir_relative_to_cwd(&kb, &tmp);
        assert_eq!(got, "kb");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
