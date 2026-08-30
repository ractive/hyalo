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
///
/// iteration 254 (HELP-2): the three flags carry the same short/long help
/// constants `find` uses, so the input trio reads identically on `read`,
/// `task read/toggle/set` and `backlinks` — and each short line fits one
/// rendered line instead of the three-to-five it used to wrap to.
#[derive(Debug, Default, Clone, clap::Args)]
pub(crate) struct InputSelection {
    /// Target file (relative to --dir) — positional form (single file)
    #[arg(value_name = "FILE", conflicts_with_all = ["file", "glob", "files_from"])]
    pub file_positional: Option<String>,

    #[arg(
        long,
        short = 'f',
        value_name = "FILE",
        conflicts_with_all = ["glob", "files_from", "file_positional"],
        help = crate::cli::args::FILE_FLAG_SHORT_DOC,
        long_help = crate::cli::args::FILE_FLAG_DOC,
    )]
    pub file: Vec<String>,

    #[arg(
        long,
        short = 'g',
        value_name = "GLOB",
        conflicts_with_all = ["file", "files_from"],
        help = crate::cli::args::GLOB_FLAG_SHORT_DOC,
        long_help = crate::cli::args::GLOB_FLAG_DOC,
    )]
    pub glob: Vec<String>,

    #[arg(
        long,
        value_name = "PATH",
        conflicts_with_all = ["file", "glob", "file_positional"],
        help = crate::cli::args::FILES_FROM_FLAG_SHORT_DOC,
        long_help = crate::cli::args::FILES_FROM_FLAG_DOC,
    )]
    pub files_from: Option<String>,
}
