/// Unified file-input selection flags, flattened into every command that
/// operates on one or more files.
///
/// Replaces the per-command combination of:
/// - `file_positional: Option<String>` / `Vec<String>`
/// - `file: Option<String>` / `Vec<String>`
/// - `glob: Vec<String>`
/// - `files_from: Option<String>`
///
/// Clap enforces that `--file`, `--glob`, and `--files-from` are mutually
/// exclusive with each other (and with `file_positional`).
#[derive(Debug, Default, Clone, clap::Args)]
pub(crate) struct InputSelection {
    /// Target file (relative to --dir) — positional form (single file)
    #[arg(value_name = "FILE", conflicts_with_all = ["file", "glob", "files_from"])]
    pub file_positional: Option<String>,

    /// Target file(s) (relative to --dir) — flag form, repeatable.
    /// Mutually exclusive with --glob and --files-from.
    #[arg(long, short = 'f', value_name = "PATH", conflicts_with_all = ["glob", "files_from", "file_positional"])]
    pub file: Vec<String>,

    /// Glob pattern(s) to match files, relative to --dir (repeatable).
    /// Prefix '!' to negate (e.g. '!**/draft-*').
    /// Mutually exclusive with --file and --files-from.
    #[arg(long, short = 'g', value_name = "PATTERN", conflicts_with_all = ["file", "files_from"])]
    pub glob: Vec<String>,

    /// Read file paths from PATH (one per line); use '-' to read from stdin.
    /// Non-.md paths and paths outside the vault are silently skipped.
    /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
    /// Input is deduplicated; results follow first-seen order.
    /// Mutually exclusive with --file and --glob.
    #[arg(long, value_name = "PATH|-", conflicts_with_all = ["file", "glob", "file_positional"])]
    pub files_from: Option<String>,
}
