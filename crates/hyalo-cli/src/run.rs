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
use crate::output_pipeline::{OutputPipeline, count_unsupported_error};
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

/// Print an `init`/`deinit` report in the requested format (DEC-262).
///
/// These two do **not** flip to JSON just because stdout is a pipe, unlike the
/// pipeline commands: their summary is a human progress report, the same stance
/// `read` takes for note bodies. `--format json` — or `--jq`, which implies it —
/// opts in; everything else prints the text summary.
fn emit_init_report(
    report: &crate::commands::init::Report,
    format: Option<Format>,
    jq: Option<&str>,
) -> Result<(), AppError> {
    let json = match format {
        Some(f) => f == Format::Json,
        None => jq.is_some(),
    };
    if !json {
        // Sanitized because the summary can echo vault-derived strings (a `dir`
        // value, a profile skill directory) that never pass through the JSON
        // pipeline's own sanitization.
        println!(
            "{}",
            crate::output::sanitize_control_chars(&report.to_text())
        );
        return Ok(());
    }
    let envelope = crate::output::build_envelope_value(&report.to_json(), None, &[]);
    if let Some(filter) = jq {
        return match crate::output::apply_jq_filter_result(filter, &envelope) {
            Ok(filtered) => {
                println!("{}", crate::output::sanitize_control_chars(&filtered));
                Ok(())
            }
            Err(e) => Err(AppError::User(crate::output::format_error(
                Format::Json,
                "jq filter failed",
                None,
                None,
                Some(&e),
            ))),
        };
    }
    println!(
        "{}",
        crate::output::sanitize_control_chars(&crate::output::format_prebuilt_envelope(
            Format::Json,
            &envelope,
            None,
            &[],
            &envelope,
        ))
    );
    Ok(())
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
        // iter-266 IDX-1: `--index` is accepted on the bare group as well as on
        // the subcommand, so `hyalo properties --index` and
        // `hyalo properties summary --index` are the same request. Whichever
        // side actually names an index wins; the subcommand takes precedence.
        Commands::Tags {
            action,
            index_flags: bare,
            ..
        } => match action {
            Some(
                TagsAction::Summary { index_flags, .. } | TagsAction::Rename { index_flags, .. },
            ) if index_flags.effective_index_path(vault_dir).is_some() => Some(index_flags),
            _ => Some(bare),
        },
        Commands::Properties {
            action,
            index_flags: bare,
            ..
        } => match action {
            Some(
                PropertiesAction::Summary { index_flags, .. }
                | PropertiesAction::Rename { index_flags, .. },
            ) if index_flags.effective_index_path(vault_dir).is_some() => Some(index_flags),
            _ => Some(bare),
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
        | Commands::Help { .. }
        | Commands::Config { .. }
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
        Commands::Lint { fix, dry_run, .. } if *fix => {
            // Fix-mode shape: `total_fixed`/`total_remaining`/`total_conflicts`
            // plus `remaining_errors`/`remaining_warnings` (iter-218 NEW-6b) —
            // distinct from the read-only shape below so a `--files-from`
            // run that resolves to zero files still reports the shape its
            // non-empty counterpart would, letting `output.rs`'s
            // `format_lint_fix_output_text` detection and any JSON consumer
            // reading `.total_fixed`/`.remaining_errors` see a consistent
            // key set regardless of whether any file matched. Serializing
            // `ExtLintFixOutput::default()` (review finding #5) instead of a
            // hand-written `json!` literal means this can never drift out of
            // sync with the real struct's field set. `dry_run` is always
            // serialized (iter-216 D-4), so the empty shape carries it as a
            // plain `false`/`true`, same as the non-empty path.
            let empty = crate::commands::lint::ExtLintFixOutput {
                dry_run: *dry_run,
                ..Default::default()
            };
            // `ExtLintFixOutput` has no maps or floats, so this cannot
            // actually fail; the fallback keeps the "no unwrap/expect
            // outside tests" rule rather than assuming infallibility.
            let payload = serde_json::to_value(&empty)
                .unwrap_or_else(|_| serde_json::json!({"files": [], "dry_run": *dry_run}));
            CommandOutcome::success_with_total(payload.to_string(), 0)
        }
        Commands::Lint { dry_run, .. } => {
            // Read-only shape: serialize `ExtLintOutput::default()` (with
            // only `dry_run` overridden) rather than a hand-written `json!`
            // literal, so this can never drift out of the real field set —
            // the hand-written literal this replaced still said `"total"`
            // after iter-216 D-2 renamed that key to `violations` and
            // omitted `files_checked`/`files_ignored`/`dry_run` entirely,
            // silently reporting the pre-iter-216 shape whenever
            // `--files-from` resolved to zero files. Mirrors the
            // `ExtLintFixOutput` branch above.
            let empty = crate::commands::lint::ExtLintOutput {
                dry_run: *dry_run,
                ..Default::default()
            };
            // `ExtLintOutput` has no maps or floats, so this cannot actually
            // fail; the fallback keeps the "no unwrap/expect outside tests"
            // rule rather than assuming infallibility.
            let payload = serde_json::to_value(&empty)
                .unwrap_or_else(|_| serde_json::json!({"files": [], "dry_run": *dry_run}));
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
                        eprintln!("error: {err}");
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

/// Does `token` look like a word from an unquoted multi-word `find` query
/// rather than a file target? (iter-267, UX-3)
///
/// Three guards keep a real path out of this branch: a path separator, an
/// explicit `.md` extension, and existence on disk (either as given or with
/// `.md` appended, which is the spelling `read`/`lint` already suggest). What
/// is left — a bare word naming nothing in the vault — is a quoting accident
/// in every realistic invocation, so it earns the quoting message instead of
/// `file not found`.
fn looks_like_unquoted_query_word(token: &str, dir: &std::path::Path) -> bool {
    if token.is_empty()
        || token.contains('/')
        || token.contains('\\')
        // Case-insensitive, matching `discovery`'s own markdown test — a
        // `NOTE.MD` target is a path, not a stray query word.
        || std::path::Path::new(token)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
        || token.starts_with('-')
    {
        return false;
    }
    !dir.join(token).exists() && !dir.join(format!("{token}.md")).exists()
}

/// `-h` layout for the top-level command (iteration 251).
///
/// Clap has one template per `Command`, so the grouped command list lives in
/// `after_help` and this template moves it *above* the global options — the
/// order an agent needs (what can I run? then how do I shape the output?).
/// `{subcommands}` is deliberately absent: the grouped block replaces the
/// alphabetical one. `--help` keeps clap's default layout untouched.
const TOP_SHORT_HELP_TEMPLATE: &str = "\
{before-help}{about-with-newline}
{usage-heading} {usage}{after-help}

GLOBAL OPTIONS (every command):
{options}";

/// Hide every global flag from `-h` (but not `--help`) so a subcommand's short
/// help can stand in one pointer line for the whole block.
///
/// `hide_short_help` is set on the root's own arg objects; clap propagates
/// `global = true` args to subcommands at build time, so the flag travels with
/// them. A subcommand that declares its *own* arg of the same name (`find`'s
/// `--index-file` from [`crate::cli::args::IndexFlags`]) keeps its own copy
/// visible, which is the intent: it is documented per command, not globally.
fn hide_globals_from_short_help(mut cmd: clap::Command) -> clap::Command {
    const GLOBAL_ARG_IDS: [&str; 9] = [
        "dir",
        "format",
        "jq",
        "count",
        "hints",
        "no_hints",
        "site_prefix",
        "quiet",
        "index_file",
    ];
    for id in GLOBAL_ARG_IDS {
        cmd = cmd.mut_arg(id, |a| a.hide_short_help(true));
    }
    cmd
}

/// Append the global-options pointer line to every subcommand's `-h`,
/// recursively (so `hyalo properties rename -h` gets it too).
///
/// Only `after_help` is set, never `after_long_help`: this runs solely on an
/// invocation that already contains `-h`, so `--help` never reaches it.
fn attach_subcommand_pointer(cmd: clap::Command, pointer: &str) -> clap::Command {
    let names: Vec<String> = cmd
        .get_subcommands()
        .map(|s| s.get_name().to_owned())
        .collect();
    let mut cmd = cmd;
    for name in names {
        cmd = cmd.mut_subcommand(name, |sub| {
            // `help` prints other commands' help; a pointer on it is noise.
            if sub.get_name() == "help" {
                return sub;
            }
            let sub = attach_subcommand_pointer(sub, pointer);
            match sub.get_after_help().map(ToString::to_string) {
                Some(existing) if !existing.is_empty() => {
                    sub.after_help(format!("{existing}\n\n{pointer}"))
                }
                _ => sub.after_help(pointer.to_owned()),
            }
        });
    }
    cmd
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
    if let Some(banner) = cwd_help_banner(&config.dir) {
        cmd = cmd.before_help(banner.clone()).before_long_help(banner);
    }

    // Apply runtime-filtered help text so that examples and cookbook entries
    // that reference config-defaulted flags are stripped from help output.
    // `after_help` is shown by `-h`; `after_long_help` is shown by `--help`.
    // iter-251: the 40 one-flag EXAMPLES moved off `-h` (where they taught one
    // flag per line and never that filters compose) onto `--help`, which now
    // carries both them and the COOKBOOK. `-h`'s own examples are installed
    // below, in the short-help branch.
    cmd = cmd.after_long_help(format!(
        "{}\n\n{}",
        filter_long_help(hide_dir, hide_format),
        filter_examples(hide_dir, hide_format)
    ));

    // Global args (--format, --jq, etc.) are only defined on the root Command
    // in clap derive — they aren't propagated to subcommands until parse time.
    // We can't use mut_subcommand to hide them from `init --help` because
    // they don't exist on the subcommand Command node yet.  This is a known
    // clap limitation with `global = true` derive args.
    let raw_args: Vec<String> = std::env::args().collect();
    // iter-256 HELP-5: `hyalo help <cmd>` becomes `hyalo <cmd> -h` here, so
    // the short-help reshaping below applies to it unchanged.
    let raw_args = crate::cli::help::rewrite_help_to_short_page(raw_args, &cmd);

    // iter-251: reshape *short* help only. `-h` is what an agent reads first
    // and 7.7 KB of it (29 KB for `--help`) is what stopped them reading past
    // `find`; `--help` keeps every word. The reshaping is applied here rather
    // than as static clap attributes because clap has no per-page (`-h` vs
    // `--help`) template or subcommand-level `hide_short_help`, and because
    // both variants depend on what `.hyalo.toml` already supplies.
    if raw_args.iter().any(|a| a == "-h") {
        if crate::suggest::top_level_subcommand(&raw_args, &Cli::command()).is_some() {
            // Subcommand `-h`: the ~1.9 KB global block was repeated
            // identically on all 27 subcommands. Collapse it to one pointer
            // line; `--help` still lists every global in full.
            cmd = hide_globals_from_short_help(cmd);
            let pointer = crate::cli::help::global_pointer(hide_dir, hide_format);
            cmd = attach_subcommand_pointer(cmd, &pointer);
            // `find` additionally gets composed examples and its own
            // `--help` pointer, overriding the bare global line.
            let find_tail = crate::cli::help::find_after_short_help(&pointer);
            cmd = cmd.mut_subcommand("find", |sub| sub.after_help(find_tail));
        } else {
            // Top-level `-h`: commands grouped by intent ahead of the global
            // options, with composed examples instead of 40 single-flag ones.
            // `[possible values: …]` duplicates what the one-line help
            // already says and forces a wrap; `--help` still prints it.
            // The global `--index-file` is a pure alias for a flag each
            // index-aware subcommand documents itself, so it earns no line on
            // the page an agent reads first.
            cmd = cmd
                .mut_arg("format", |a| a.hide_possible_values(true))
                .mut_arg("index_file", |a| a.hide_short_help(true))
                .help_template(TOP_SHORT_HELP_TEMPLATE)
                .after_help(crate::cli::help::short_help_body(hide_dir, hide_format));
        }
    }
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

            // Unknown long flag that names a schema-declared property →
            // `--property` hint. Models and users reach for natural-language
            // flags (`hyalo find --status planned`) even though the skill/help
            // teach `--property status=planned`; clap's own tip ("to pass
            // '--status' as a value, use '-- --status'") actively misleads.
            // Only fires when the flag names a real property in the effective
            // schema (checked via suggest::is_schema_property), so unrelated
            // typos keep clap's normal error.
            if e.kind() == clap::error::ErrorKind::UnknownArgument
                && let Some(name) = crate::suggest::unknown_long_flag_name(&e)
                && crate::suggest::is_schema_property(&config.schema, &name)
            {
                eprintln!(
                    "error: unexpected argument '--{name}' found\n\n\
                     tip: '{name}' is a frontmatter property, not a flag — did you mean \
                     '--property {name}=<value>'?\n\n\
                     Example: hyalo find --property {name}=<value>\n"
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

            // Intercept `--property` / `--tag` on the `new` subcommand.
            // `new` scaffolds strictly from the schema type's declared
            // required properties and defaults; UX-5 (dogfood v0.22.0) is
            // that nothing in the failure told the reader where properties
            // *do* get set, and clap's nearest-match tip picks `--type`.
            // Point at the `new … && set …` chain instead of growing `new`'s
            // flag surface (decision-log: no new CLI flags without a payoff
            // the existing commands cannot deliver).
            if e.kind() == clap::error::ErrorKind::UnknownArgument
                && crate::suggest::top_level_subcommand(&raw_args, &Cli::command()) == Some("new")
                && (crate::suggest::unknown_arg_is(&e, "--property")
                    || crate::suggest::unknown_arg_is(&e, "-p")
                    || crate::suggest::unknown_arg_is(&e, "--tag")
                    || crate::suggest::unknown_arg_is(&e, "-t"))
            {
                eprintln!(
                    "error: `hyalo new` scaffolds from the type's schema and accepts no \
                     --property/--tag\n\n\
                     hint: create the file, then set what the schema does not declare:\n\n    \
                     hyalo new --type <TYPE> --file <FILE> && hyalo set <FILE> --property k=v\n"
                );
                return Err(AppError::Exit(2));
            }

            // iter-274 (UX-25): `--index` on `create-index` / `drop-index`.
            // Those two commands WRITE (or delete) a snapshot; `--index` is the
            // read-side flag that says "answer this query from a snapshot", so
            // the combination is meaningless rather than misspelled. clap's
            // generic "unexpected argument '--index' found / tip: a similar
            // argument exists: '--index-file'" pointed at the very flag the
            // caller had already passed. Say what is actually wrong.
            if e.kind() == clap::error::ErrorKind::UnknownArgument
                && crate::suggest::unknown_arg_is(&e, "--index")
                && let Some(sub @ ("create-index" | "drop-index")) =
                    crate::suggest::top_level_subcommand(&raw_args, &Cli::command())
            {
                let (verb, path_flag) = if sub == "create-index" {
                    ("writes", "-o/--output PATH")
                } else {
                    ("deletes", "-p/--path PATH")
                };
                eprintln!(
                    "error: `hyalo {sub} --index` cannot be combined — `--index` reads a \
                     snapshot, and `{sub}` {verb} one\n\n\
                     hint: name the snapshot with {path_flag} (the global --index-file PATH is \
                     accepted as a synonym); use `--index` on a querying command such as \
                     `hyalo find --index`\n"
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
                    // iter-267 (UX-13): `hyalo index` is the name agents and
                    // users reach for first. clap's nearest match by edit
                    // distance is `find`, which has nothing to do with the
                    // snapshot, so the empty state pointed the wrong way.
                    // Name the two commands that actually manage it.
                    if invalid == "index" {
                        eprintln!(
                            "{e}\n  hint: did you mean 'hyalo create-index'? (writes the \
                             `.hyalo-index` snapshot; 'hyalo drop-index' removes it, and read \
                             commands opt in with --index)\n"
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
                | Commands::Config { .. }
        )
    {
        let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
        eprintln!(
            "{}",
            crate::output::format_error(fmt, count_unsupported_error(), None, None, None)
        );
        // User error (unsupported flag for this command) → exit 1, not 2
        // (2 is reserved for internal errors — iter-181 task 2).
        return Err(AppError::Exit(1));
    }
    // `--format github` is lint-only everywhere else (see the rejection further
    // down, which `init`/`deinit` never reach because they dispatch here first).
    // Reject it with the identical message rather than silently printing text.
    if cli.format == Some(Format::Github)
        && matches!(&cli.command, Commands::Init { .. } | Commands::Deinit)
    {
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
        return Err(AppError::Exit(1));
    }
    if let Commands::Init {
        claude,
        pi,
        profile,
    } = &mut cli.command
    {
        let init_dir = cli.dir.as_deref().and_then(|p| p.to_str());
        let report = init_commands::run_init(init_dir, *claude, *pi, profile.as_deref())
            .map_err(AppError::Internal)?;
        return emit_init_report(&report, cli.format, cli.jq.as_deref());
    }
    if let Commands::Deinit = &mut cli.command {
        let deinit_dir = cli.dir.as_deref().and_then(|p| p.to_str());
        let report = init_commands::run_deinit(deinit_dir).map_err(AppError::Internal)?;
        return emit_init_report(&report, cli.format, cli.jq.as_deref());
    }
    if let Commands::Completion { shell } = &mut cli.command {
        let mut cmd = Cli::command();
        clap_complete::generate(*shell, &mut cmd, "hyalo", &mut std::io::stdout());
        return Ok(());
    }
    // `config` inspects CWD directly and does not need normal pipeline setup.
    // Dispatch before config validation (dir-doesn't-exist check) so it always works.
    if let Commands::Config { raw } = &mut cli.command {
        let raw = *raw;
        // Report exactly what the rest of the CLI would use, by going through
        // the shared `--dir` resolution rather than a second, divergent one
        // (iter-201, H-4). `config` is the command users reach for to *check*
        // which `.hyalo.toml` applies, so it must not have its own answer.
        let dir_override = cli.dir.as_deref();
        let effective = crate::config::resolve_effective(config.clone(), dir_override);
        if let Some(note) = crate::config::dir_override_note(&effective) {
            crate::warn::note(note);
        }
        // Determine output format with the SAME precedence every other
        // command uses: --format flag > --jq (forces JSON) > `.hyalo.toml`
        // format pin > TTY detection.
        //
        // iter-267 (UX-18) made this the `format`/`format_source` the report
        // itself carries — the answer to "what will hyalo do here?" — so it
        // must match what a real pipeline command run in this same directory
        // would resolve to. Stopping at `cli.format.unwrap_or_else(TTY)` (the
        // pre-267 behaviour) skipped the config-file pin entirely: a vault
        // pinning `format = "text"` reported `format: "json"` under a piped
        // invocation, while `format_source` said "config" — self-contradictory,
        // and wrong for the one command whose job is this answer.
        let format = if let Some(f) = cli.format {
            f
        } else if cli.jq.is_some() {
            crate::output::Format::Json
        } else if let Some(fmt_str) = effective.config.format.as_deref() {
            crate::output::Format::from_str_opt(fmt_str).unwrap_or_else(|| {
                // Malformed value; `report.malformed`/`parse_error` already
                // surfaces this — fall back to TTY detection here too.
                if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                    crate::output::Format::Text
                } else {
                    crate::output::Format::Json
                }
            })
        } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
            crate::output::Format::Text
        } else {
            crate::output::Format::Json
        };
        // UX-3 (dogfood pre3): every other command relies on this stderr
        // warning as the only signal that its config is unusable, but
        // `hyalo config` itself already leads its own report body with the
        // identical diagnostic (`malformed`/`parse_error` in JSON, the
        // `malformed: true` block in text) — printing it a second time here
        // added nothing but noise for the one command whose entire purpose is
        // to show it.
        let report = crate::commands::config::collect_config_report(
            &cwd,
            effective,
            dir_override.is_some(),
            cli.site_prefix.as_deref(),
            raw,
            format,
            cli.format.is_some(),
        )
        .map_err(AppError::Internal)?;

        // `--jq` operates on the full envelope, exactly as it does for pipeline
        // commands. Before iter-192 the filter was accepted and silently
        // ignored, printing the unfiltered object.
        if let Some(filter) = cli.jq.as_deref() {
            let envelope = crate::commands::config::config_envelope(&report);
            return match crate::output::apply_jq_filter_result(filter, &envelope) {
                Ok(filtered) => {
                    println!("{}", crate::output::sanitize_control_chars(&filtered));
                    Ok(())
                }
                Err(e) => Err(AppError::User(crate::output::format_error(
                    format,
                    "jq filter failed",
                    None,
                    None,
                    Some(&e),
                ))),
            };
        }

        // Hints follow the same precedence as every other command: on by
        // default, `--no-hints` (or `hints = false` in .hyalo.toml) turns them
        // off, an explicit `--hints` turns them back on.
        let show_hints = cli.hints || (!cli.no_hints && config.hints);
        match crate::commands::config::run_config(&report, format, show_hints) {
            CommandOutcome::RawBytes(_) => {
                unreachable!("config does not emit RawBytes")
            }
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
    // Save the CWD config's own `dir` string for the redundant-`--dir` note.
    // We need it before `config` is potentially shadowed by the target config.
    let cwd_config_dir_str = config.dir.display().to_string();
    // Determine the effective vault directory and the config to use. The
    // `--dir` semantics live in `config::resolve_effective` so `hyalo config`
    // and every other command answer "which .hyalo.toml applies?" identically
    // (iter-201, H-4).
    if let Some(cli_dir) = cli.dir.as_deref() {
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
    }
    let effective = crate::config::resolve_effective(config, cli.dir.as_deref());
    // Announce a `--dir` that switched to a different configuration before the
    // command runs, so the change of rules is visible in the same stream as the
    // results it shaped.
    if let Some(note) = crate::config::dir_override_note(&effective) {
        crate::warn::note(note);
    }
    // Only now is it known which `.hyalo.toml` governs the run, so only now can
    // an unusable one be reported without naming a file `--dir` already
    // discarded (iter-213, UX-5).
    crate::config::emit_config_diagnostics(&effective);

    // H-1 (iter-221): a project-local .hyalo.toml whose `dir` resolves
    // outside its own config directory tried to redefine hyalo's own
    // read/write scope — refuse every command, not just writes, since even a
    // read would operate against a boundary the config was never entitled to
    // set for itself. Gated on `!dir_from_cli`: an explicit `--dir` is the
    // user's own choice, and `EffectiveConfig::dir` is always that literal
    // value (never this config's own `dir` field, see `resolve_effective`),
    // so a run with `--dir` given is safe regardless of what a discovered
    // ancestor config wrote.
    if !dir_from_cli && let Some(diagnostic) = effective.config.dir_out_of_bounds.as_deref() {
        let fmt = early_format(
            cli.format,
            cli.jq.is_some(),
            effective.config.format.as_deref(),
        );
        return Err(AppError::User(crate::output::format_error(
            fmt, diagnostic, None, None, None,
        )));
    }

    let crate::config::EffectiveConfig {
        config,
        dir,
        dir_redundant,
        ..
    } = effective;

    // A `.hyalo.toml` that exists but could not be parsed leaves the run on
    // built-in defaults: a different vault root, no schema, no `[lint] ignore`,
    // no views. Reads may proceed on those defaults (the warning is
    // `-q`-proof), but a command that *writes* must not — it would edit files
    // the user did not configure hyalo to touch, using rules they did not
    // write. Refuse with the parse diagnostic and exit 1 (iter-201, M-2).
    //
    // BUG-19 (iter-265, DEC-279) widens the same refusal to commands whose
    // *exit code is a gate* — `lint`, `find --strict`, `views run`. Those are
    // reads, but a caller acts on their verdict, and a verdict computed without
    // `[lint] ignore` or the schemas is not the verdict the vault asked for.
    // Warning and exiting 0 turned a broken config into a green CI build.
    if let Some(diagnostic) = config.malformed.as_deref()
        && (cli.command.writes() || cli.command.gates())
    {
        let kind = if cli.command.writes() {
            "a mutating command"
        } else {
            "a command whose exit code is a gate"
        };
        let fmt = early_format(cli.format, cli.jq.is_some(), config.format.as_deref());
        return Err(AppError::User(crate::output::format_error(
            fmt,
            &format!(
                "refusing to run {kind} with an unusable .hyalo.toml \
                 ({}/.hyalo.toml): {diagnostic}",
                config.config_dir.display()
            ),
            None,
            Some("Fix the config file, or pass --dir to target a vault whose config parses."),
            None,
        )));
    }
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

    // Install `[links] aliases` (iter-272, DEC-296) once, before any command
    // runs, so every resolver, index builder and graph pass sees the same
    // answer without threading it through their signatures.
    hyalo_core::discovery::set_link_aliases(config.alias_links_enabled);

    // Install `[scan] exclude` (iter-265, DEC-277) the same way, so every
    // command's discovery — and every `--index` load — drops the same files.
    for (pat, msg) in hyalo_core::discovery::set_scan_exclude(&config.scan_exclude) {
        crate::warn::warn(format!("invalid [scan] exclude glob {pat:?}: {msg}"));
    }

    // DEC-278: per-file skip diagnostics are collected, not streamed, unless
    // the vault asks for them or `RUST_LOG` is turned up for this one run.
    hyalo_core::warn::set_verbose_skips(
        config.scan_verbose_skips || hyalo_core::warn::rust_log_wants_debug(),
    );

    // Note when --dir is redundant: the user passed the dir .hyalo.toml already
    // resolves to. The config still applies (iter-201) — the flag is simply
    // noise, which is all this note now claims.
    if dir_redundant {
        crate::warn::note(format!(
            "--dir is redundant; .hyalo.toml already sets dir = \"{cwd_config_dir_str}\""
        ));
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

    // iter-213 (UX-1): running from inside the configured vault used to draw a
    // "do not cd into the vault" scolding, because `.hyalo.toml` was read from
    // the working directory and nowhere else, so the run really did lose the
    // config. `config::load_config` now adopts the governing ancestor config
    // instead (announcing itself when the vault is wider than CWD), which makes
    // the invocation correct rather than merely tolerated — so the scolding is
    // gone. The absolute-`--file` half of the misuse warning stays: that one
    // still fires from `commands::resolve_file_user`.
    // Derive site_prefix with tri-state precedence:
    //
    //   1. CLI --site-prefix flag  (present → use it; empty string = explicit disable)
    //   2. `site_prefix` in .hyalo.toml  (same: empty string = explicit disable)
    //   3. Auto-derive from canonicalized dir's last path component
    //      (only runs when neither 1 nor 2 is present)
    //
    // Empty strings in (1) and (2) short-circuit the chain and result in
    // site_prefix = None, suppressing all absolute-link resolution.
    let (site_prefix_owned, _site_prefix_source) = crate::config::resolve_site_prefix(
        cli.site_prefix.as_deref(),
        config.site_prefix.as_deref(),
        &dir,
    );
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

    // L-5: the `read` override below is a decision about *results* only — the
    // ambient format (explicit `--format`, else json when stdout is a pipe)
    // still governs error envelopes, so a scripted `hyalo read` failure parses
    // like every other command's.
    let error_format = format;
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
    // iter-235/238: `--filenames-only` / `--filenames0` are find-local
    // projections that bypass the JSON envelope entirely (raw paths). clap
    // already rejects them alongside `--jq` and `--count` (which likewise
    // replace the whole pipeline); here we cover an explicit `--format json`,
    // which is a contradictory projection (JSON envelope vs. raw paths). The
    // default JSON-when-piped format does NOT conflict — the projection
    // overrides the format so a piped `find --filenames-only | sort` works
    // without a `--format text` chore, which is the whole point of the flags.
    let filename_projection = match &cli.command {
        Commands::Find {
            filters:
                FindFilters {
                    filenames_only,
                    filenames0,
                    ..
                },
            ..
        } if *filenames_only || *filenames0 => {
            if *filenames_only {
                "--filenames-only"
            } else {
                "--filenames0"
            }
        }
        _ => "",
    };
    if format_from_cli && format == Format::Json && !filename_projection.is_empty() {
        eprintln!(
            "{}",
            crate::output::format_error(
                format,
                &format!("{filename_projection} cannot be combined with --format json"),
                None,
                Some(
                    "--filenames-only/--filenames0 print raw paths; drop --format (or use --format text)"
                ),
                None,
            )
        );
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
            // The summary flags live on the explicit subcommand OR, for the bare
            // group form, on the group itself (M-8) — read whichever holds them.
            Commands::Properties {
                glob: bare_glob,
                limit: bare_limit,
                index_flags: _,
                action,
            } if !matches!(
                action,
                Some(crate::cli::args::PropertiesAction::Rename { .. })
            ) =>
            {
                let (glob, limit) = match action {
                    Some(crate::cli::args::PropertiesAction::Summary { glob, limit, .. }) => {
                        (glob, limit)
                    }
                    _ => (bare_glob, bare_limit),
                };
                let mut ctx = HintContext::from_common(HintSource::PropertiesSummary, &common);
                ctx.glob.clone_from(glob);
                ctx.has_limit = limit.is_some();
                Some(ctx)
            }
            Commands::Tags {
                glob: bare_glob,
                limit: bare_limit,
                index_flags: _,
                action,
            } if !matches!(action, Some(crate::cli::args::TagsAction::Rename { .. })) => {
                let (glob, limit) = match action {
                    Some(crate::cli::args::TagsAction::Summary { glob, limit, .. }) => {
                        (glob, limit)
                    }
                    _ => (bare_glob, bare_limit),
                };
                let mut ctx = HintContext::from_common(HintSource::TagsSummary, &common);
                ctx.glob.clone_from(glob);
                ctx.has_limit = limit.is_some();
                Some(ctx)
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
            Commands::Read {
                selection,
                section,
                lines,
                ..
            } => {
                let mut ctx = HintContext::from_common(HintSource::Read, &common);
                if let Some(f) = selection
                    .file_positional
                    .as_ref()
                    .or(selection.file.first())
                {
                    ctx.file_targets = vec![f.clone()];
                }
                ctx.read_narrowed = section.is_some() || lines.is_some();
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
            // iter-210 (dogfood UX-4): `views list` and `lint-rules list` used
            // to emit no hints at all — a listing with nothing to click.
            // `set`/`remove` already end somewhere concrete; `run` and `show`
            // used to fall through to the catch-all `None` below (NEW-18,
            // dogfood pre3) despite `run` being a full `find` query and
            // `show` inspecting one specific, actionable rule.
            Commands::Views { action } => match action {
                None | Some(crate::cli::args::ViewsAction::List) => {
                    Some(HintContext::from_common(HintSource::ViewsList, &common))
                }
                // `views run <name>` is `find --view <name>` under another
                // name — dispatch.rs merges the saved view with this same
                // CLI overlay and calls the identical `find_commands::find`.
                // Reproduce that merge here too (read-only — dispatch.rs's
                // own resolution is untouched, so there is no risk of
                // double-merging list filters): otherwise the hint context
                // would only ever see the overlay, not the view's own saved
                // filters, and every derived hint would silently drop them
                // (NEW-18, dogfood pre3; mirrors the `Commands::Find { view:
                // Some(_), .. }` early-merge above, lines ~976-987).
                Some(crate::cli::args::ViewsAction::Run {
                    name,
                    pattern,
                    filters: overlay,
                    ..
                }) => {
                    let views = crate::commands::views::load_views(&config_dir);
                    let mut merged = views.get(name).cloned().unwrap_or_default();
                    let mut overlay = overlay.clone();
                    overlay.pattern.clone_from(pattern);
                    merged.merge_from(&overlay);
                    let FindFilters {
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
                        pattern: merged_pattern,
                        ..
                    } = merged;
                    let mut ctx = HintContext::from_common(HintSource::Find, &common);
                    ctx.glob = glob;
                    ctx.fields = fields;
                    ctx.sort = sort;
                    ctx.reverse = reverse;
                    ctx.has_limit = limit.is_some();
                    ctx.has_body_search = merged_pattern.is_some();
                    ctx.body_pattern = merged_pattern;
                    ctx.has_regex_search = regexp.is_some();
                    ctx.property_filters = properties;
                    ctx.tag_filters = tag;
                    ctx.task_filter = task;
                    ctx.file_targets = file;
                    ctx.section_filters = sections;
                    ctx.broken_links_filter = broken_links;
                    ctx.orphan_filter = orphan;
                    ctx.dead_end_filter = dead_end;
                    ctx.title_filter = title;
                    ctx.view_name = Some(name.clone());
                    Some(ctx)
                }
                Some(
                    crate::cli::args::ViewsAction::Set { .. }
                    | crate::cli::args::ViewsAction::Remove { .. },
                ) => None,
            },
            Commands::LintRules { action } => match action {
                None | Some(crate::cli::args::LintRulesAction::List { .. }) => {
                    Some(HintContext::from_common(HintSource::LintRulesList, &common))
                }
                // `lint-rules show <ID>` inspects one specific rule; the
                // natural next step is either lint scoped to it or tweaking
                // its configuration (NEW-18, dogfood pre3).
                Some(crate::cli::args::LintRulesAction::Show { rule_id }) => {
                    let mut ctx = HintContext::from_common(HintSource::LintRulesShow, &common);
                    ctx.lint_rule = Some(rule_id.clone());
                    Some(ctx)
                }
                Some(
                    crate::cli::args::LintRulesAction::Set { .. }
                    | crate::cli::args::LintRulesAction::Remove { .. },
                ) => None,
            },
            Commands::Properties { .. }
            | Commands::Tags { .. }
            | Commands::Init { .. }
            | Commands::Deinit
            | Commands::Completion { .. }
            | Commands::Help { .. }
            | Commands::Config { .. }
            | Commands::Madr { .. }
            | Commands::Changelog { .. } => None,
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
        // iter-267 (UX-18): a vault that has already been indexed should be
        // told to *use* the snapshot, not to build another one. Only probed
        // when no index was requested, so the common `--index` path pays
        // nothing for the stat.
        ctx.snapshot_on_disk = index_path_buf.is_none() && dir.join(".hyalo-index").is_file();
        // iter-267 (UX-3, reverse direction): a PATTERN that is itself an
        // existing `.md` path is a body search for that literal text. Record
        // it so `find`'s hints can offer `--file`.
        if let Some(pat) = ctx.body_pattern.as_deref()
            && std::path::Path::new(pat)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            && dir.join(pat).is_file()
        {
            ctx.pattern_names_a_file = Some(pat.to_owned());
        }
        // Preserve the active index into every derived hint so it queries the
        // same snapshot rather than silently rescanning the vault (BUG-7
        // audit; widened from `find` to every command in iter-274 / UX-2 —
        // `summary`'s `find --orphan` / `find --broken-links` hints were the
        // worst offenders, costing a full rescan each). A path equal to the
        // default `<vault>/.hyalo-index` re-emits as bare `--index`; any other
        // path re-emits as `--index-file <path>`. Hints whose command does not
        // accept the flag drop it — see `command_accepts_index`.
        if let Some(ref p) = index_path_buf {
            let default_path = dir.join(".hyalo-index");
            ctx.find_index = if *p == default_path {
                HintContext::default_find_index()
            } else {
                // Shortened to its CWD-relative form when possible — the same
                // path is repeated on every derived hint (iter-210 / UX-5).
                HintContext::file_find_index(crate::hints::shorten_index_path_for_hint(p))
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
                    // M-6: a snapshot is a point-in-time copy — edits made
                    // outside it (by hand, by another tool, or by hyalo itself
                    // without `--index`) are simply invisible, and the run
                    // still exits 0. Probe the vault's directory mtimes
                    // (bounded-depth walk, iter-249 UX-1) and warn when they
                    // postdate the snapshot, so a stale index is at least
                    // noisy instead of silently wrong.
                    // iter-247 (deep-review S-2): warn-but-serve stays the
                    // default — the probe is a heuristic, and turning a
                    // heuristic into a hard refusal would make every indexed
                    // query hostage to filesystem mtime granularity.
                    //
                    // UX-7 (iter-265, DEC-280): when the run *named* its files,
                    // do better than the heuristic. One `stat` per named target
                    // is cheap and exact, so refresh those entries in memory and
                    // stay quiet — `find --index --file just-appended.md` now
                    // reports the file's current size and line count instead of
                    // a snapshot from before the append. The warning survives
                    // for whole-vault queries, where refreshing everything would
                    // re-introduce the cost iteration 260 removed.
                    let mut idx = idx;
                    let targets = cli.command.explicit_file_targets();
                    let refreshed_all_targets = if targets.is_empty() {
                        false
                    } else {
                        targets.iter().all(|rel| {
                            hyalo_core::index::refresh_if_changed_on_disk(&mut idx, &dir, rel)
                        })
                    };
                    if !refreshed_all_targets && !cli.command.write_repairs_named_targets() {
                        let (_, _, created_at, _) = idx.header_info();
                        let dirs_moved =
                            hyalo_core::index::newest_dir_mtime(&dir).is_some_and(|newest| {
                                newest
                                    > created_at
                                        .saturating_add(hyalo_core::index::STALENESS_TOLERANCE_SECS)
                            });
                        if dirs_moved {
                            crate::warn::warn(
                                "index older than vault; results may be stale — re-run create-index",
                            );
                        } else if let Some(rel) =
                            // INDEX-1 (iter-273, BUG-12): the directory probe
                            // above sees notes added and removed, but an
                            // in-place overwrite moves no directory mtime — so
                            // a rewritten note was served from the snapshot
                            // silently. Fall through to the per-entry mtime
                            // comparison only when the cheap probe found
                            // nothing, so the extra `stat`s are paid once, on
                            // the vault that looked clean.
                            hyalo_core::index::first_file_modified_since_snapshot(
                                    &idx, &dir,
                                )
                        {
                            crate::warn::warn(format!(
                                "index older than vault ({rel} changed on disk since the index \
                                 was built); results may be stale — re-run create-index"
                            ));
                        }
                    }
                    Some(idx)
                } else {
                    let (hdr_vault, hdr_prefix, _, _) = idx.header_info();
                    crate::warn::warn(format!(
                        "index does not match this run ({}); falling back to disk scan",
                        crate::config::index_mismatch_summary(
                            hdr_vault,
                            &vault_dir_str,
                            hdr_prefix,
                            site_prefix,
                        ),
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
    // DEC-290: `[schema]` was present but unloadable, so `schema` above is the
    // empty fallback. `set`/`append` refuse when validation was requested.
    let schema_invalid = config.schema_invalid;
    let frontmatter_link_props_owned = config.frontmatter_link_props;
    let mut validate_on_write = config.validate_on_write;
    let lint_ignore = config.lint_ignore;
    let okf_ignore = config.okf_ignore;
    let changelog_path = config.changelog_path;
    let case_insensitive_mode = config.case_insensitive_mode;
    let case_insensitive_resolve = config.case_insensitive_resolve;
    let auto_link_exclude_titles_set = config.auto_link_exclude_titles_set;
    let auto_link_exclude_titles = config.auto_link_exclude_titles;
    let auto_link_exclude_target_globs = config.auto_link_exclude_target_globs;
    let auto_link_first_only = config.auto_link_first_only;
    let auto_link_warn_common_titles = config.auto_link_warn_common_titles;
    let config_fuzzy_min_confidence = config.fuzzy_min_confidence;
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

    // iter-267 (UX-3): `hyalo find dataview plugin` is an unquoted two-word
    // body search, not a query plus a file target — but clap hands the second
    // word to FILE and the run died with a bare `file not found: plugin`,
    // which describes the symptom and hides the cause. Catch the argv shape
    // before dispatch and name the quoted command that does what was meant.
    if let Commands::Find {
        pattern: Some(pattern),
        file_positional,
        ..
    } = &cli.command
        && let Some(bad) = file_positional
            .iter()
            .find(|t| looks_like_unquoted_query_word(t, &dir))
    {
        let quoted: String = std::iter::once(pattern.as_str())
            .chain(file_positional.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!(
            "error: '{bad}' is not a file; did you mean hyalo find '{quoted}'?\n\n\
             tip: the FIRST positional is the body-search PATTERN; every later one is a FILE \
             target, so an unquoted multi-word query is read as a query plus file names. \
             Quote the whole phrase.\n"
        );
        return Err(AppError::Exit(2));
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
                    error_format,
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
        schema_invalid: schema_invalid.as_deref(),
        validate_on_write,
        lint_ignore: &lint_ignore,
        okf_ignore: &okf_ignore,
        changelog_path: changelog_path.as_deref(),
        md_lint: &md_lint,
        case_insensitive_mode,
        case_insensitive_resolve,
        auto_link_exclude_titles: &auto_link_exclude_titles,
        auto_link_exclude_titles_set,
        auto_link_exclude_target_globs: &auto_link_exclude_target_globs,
        auto_link_first_only,
        config_fuzzy_min_confidence,
        auto_link_warn_common_titles,
        exit_code_override: None,
        config_default_limit,
        programmatic_output: jq_filter.is_some() || cli.count,
        lint_strict: lint_strict_from_config,
        lint_profiles: lint_profiles_active,
        files_from_counters: None,
        // iter-273: remember *how* the file list arrived, because
        // `resolve_files_from_for_command` above has already flattened
        // `--files-from` into the command's `file` field.
        file_list_from_files_from: files_from_counters.is_some(),
        zero_result_values: std::collections::BTreeMap::new(),
        zero_result_body_search: None,
    };

    // When --files-from resolved to zero files (all entries filtered/missing),
    // short-circuit with an empty result rather than falling through to "scan all".
    //
    // Capture wall-clock elapsed around the dispatch body so the slow-query
    // hint can fire when the command took longer than SLOW_QUERY_THRESHOLD_MS.
    // We measure here (not inside dispatch) so hint rendering is excluded.
    // iter-247 (deep-review dogfood note): `summary --format text` used to open
    // its report with a `kb dir: <path>` banner — the only command that put
    // resolution context on stdout, and a cwd-dependent line every text-mode
    // script had to strip. The banner moves to the `note:` stderr channel this
    // CLI already uses to announce which vault a run resolved: visible in a
    // terminal, absent from a pipe, suppressed by `-q`. JSON is untouched (the
    // envelope still carries `dir`). Captured before dispatch because
    // `cli.command` is moved into it, and emitted only on success so a failed
    // run does not narrate a vault it never summarised.
    let summary_kb_dir_note =
        matches!(cli.command, Commands::Summary { .. }) && format == Format::Text;

    // iter-264 (BUG-22): `find`'s envelope always carries the three
    // `--files-from` counters, zero when the flag was not used, so a consumer
    // can read `.files_missing` without first checking how the file list was
    // supplied. `views run` is `find` under another spelling and must stay
    // byte-identical to it. Captured before dispatch, which moves `cli.command`.
    let find_always_reports_counters = matches!(
        cli.command,
        Commands::Find { .. }
            | Commands::Views {
                action: Some(crate::cli::args::ViewsAction::Run { .. })
            }
    );

    let dispatch_start = Instant::now();
    let result = if files_from_empty {
        // Produce the appropriate empty payload for the command type.
        Ok(empty_result_for_command(&cli.command))
    } else {
        dispatch(cli.command, &mut ctx)
    };
    // Saturate at u64::MAX on absurdly long runs (> ~585 million years).
    let elapsed_ms = u64::try_from(dispatch_start.elapsed().as_millis()).unwrap_or(u64::MAX);

    if summary_kb_dir_note && result.is_ok() {
        crate::warn::note(format!("kb dir: {}", dir.display()));
    }

    // Inject elapsed into hint context so slow_query_hint can read it.
    if let Some(ref mut hctx) = hint_ctx {
        hctx.elapsed_ms = Some(elapsed_ms);
        // iter-251: a `find` that matched nothing collected the distinct
        // values of every filtered property key during the scan it already
        // paid for. Hand them to the hint layer so the zero-result
        // did-you-mean names real values instead of guessing.
        hctx.observed_property_values = std::mem::take(&mut ctx.zero_result_values);
        // iter-258: same trip, one more answer — when the empty query filtered
        // on a property *regex* that body text does match, the hint layer can
        // say so and hand over the equivalent `find -e`.
        hctx.body_search_suggestion = ctx.zero_result_body_search.take();
    }

    let exit_code_override = ctx.exit_code_override;
    // Prefer counters captured inside dispatch (read/backlinks/task path through
    // `resolve_inputs`); fall back to the pre-dispatch path used by other commands.
    let final_files_from_counters = ctx
        .files_from_counters
        .take()
        .or(files_from_counters)
        .or_else(|| {
            find_always_reports_counters
                .then(crate::commands::files_from::FilesFromCounters::default)
        });

    let pipeline = OutputPipeline {
        user_format: format,
        error_format,
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
