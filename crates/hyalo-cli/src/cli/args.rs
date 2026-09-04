use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand};

use crate::cli::inputs::InputSelection;
use crate::list_commands::list_commands_phrase;
use crate::output::Format;

/// Token substituted with [`list_commands_phrase`] in help templates.
///
/// Shared with [`crate::cli::help`] so both templates use the same marker.
pub(crate) const LIST_COMMANDS_PLACEHOLDER: &str = "{LIST_COMMANDS}";

/// Token substituted with [`crate::list_commands::limited_commands_phrase`].
///
/// Distinct from [`LIST_COMMANDS_PLACEHOLDER`]: "emits a total" and "caps at
/// `default_limit` and takes `--limit`" are different sets (M-8).
pub(crate) const LIMITED_COMMANDS_PLACEHOLDER: &str = "{LIMITED_COMMANDS}";

/// Shared `--file` doc string used on every command that accepts `--file`,
/// `--glob`, and `--files-from` as mutually exclusive input sources (NEW-4).
/// Keeping it in one place prevents future help-text drift across `find`,
/// `set`, `remove`, and `append`.
pub(crate) const FILE_FLAG_DOC: &str = "Target file(s) (repeatable; falls back to case-insensitive matching \
     per `[links] case_insensitive`, default auto). Mutually exclusive with --glob and --files-from";

/// One-line `-h` form of [`FILE_FLAG_DOC`] (iter-251). The case-folding and
/// mutual-exclusion detail stays on `--help`, where there is room for it.
pub(crate) const FILE_FLAG_SHORT_DOC: &str =
    "Target file(s), repeatable (excludes --glob / --files-from)";

/// Shared `--glob` doc string for the `--file` / `--glob` / `--files-from`
/// input trio.
pub(crate) const GLOB_FLAG_DOC: &str = "Glob pattern(s) to select files, relative to --dir (repeatable); \
     prefix '!' to negate (e.g. '!**/draft-*'). Mutually exclusive with --file and --files-from";

/// One-line `-h` form of [`GLOB_FLAG_DOC`] (iter-254, HELP-2). Identical to the
/// line `find -h` shows, so the trio reads the same on every command.
pub(crate) const GLOB_FLAG_SHORT_DOC: &str = "Glob(s) relative to --dir, repeatable; '!' negates \
     ('!**/draft-*')";

/// Shared `--files-from` doc string for the input trio.
pub(crate) const FILES_FROM_FLAG_DOC: &str = "Read file paths from PATH (one per line); use '-' to read from \
     stdin. Mutually exclusive with --file and --glob. Non-.md paths and paths outside the vault \
     are silently skipped. Repo-relative paths with the configured vault dir prefix are resolved \
     automatically. Input is deduplicated; results follow first-seen order.";

/// One-line `-h` form of [`FILES_FROM_FLAG_DOC`] (iter-254, HELP-2).
pub(crate) const FILES_FROM_FLAG_SHORT_DOC: &str =
    "Read paths from PATH, one per line ('-' = stdin)";

/// Shared `task --section` doc string: unlike `find --section`, a heading that
/// matches more than once is an error rather than a union.
pub(crate) const TASK_SECTION_FLAG_DOC: &str = "Select all tasks under a heading: case-insensitive substring, \
     '##' pins the level, or /regex/. Refuses with an error naming every matched heading's line \
     number when more than one distinct heading matches (unlike `find --section`, which unions \
     them).";

/// One-line `-h` form of [`TASK_SECTION_FLAG_DOC`] (iter-254, HELP-2).
pub(crate) const TASK_SECTION_FLAG_SHORT_DOC: &str = "Heading substring, '##' pins the level, or /regex/ \
     (refuses if ambiguous)";

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if requires &bool
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}

pub(crate) fn parse_limit(s: &str) -> Result<usize, String> {
    s.parse()
        .map_err(|_| format!("'{s}' is not a valid number"))
}

/// Value parser for `--threshold`: accepts a `f64` in `[0.0, 1.0]`.
pub(crate) fn parse_threshold(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid floating-point number"))?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!(
            "threshold must be between 0.0 and 1.0 (inclusive), got {v}"
        ))
    }
}

/// Index flags, flattened into subcommands that can consume a snapshot index.
#[derive(Args, Debug, Default, Clone)]
pub(crate) struct IndexFlags {
    // COH-8 (iter-254): the maintenance note below is for whoever edits this
    // list, not for the ~15 `--help` pages it used to be printed on. A rustdoc
    // intra-doc link renders as literal `[`crate::…`]` in a terminal, which is
    // noise at best and a dangling reference at worst.
    //
    // This set is *index support*, not the *list commands* of
    // [`crate::list_commands::LIST_COMMANDS`] — it includes `summary` (which
    // emits no `total`) and excludes the config-only listings. The two lists
    // answer different questions and are deliberately maintained apart.
    /// Use the `.hyalo-index` snapshot in the vault dir
    ///
    /// Read-only commands (find, summary, tags, properties, backlinks) skip
    /// the disk scan entirely when the index is present. On `tags` and
    /// `properties` the flag is accepted on the bare command as well as on
    /// the `summary`/`rename` subcommand (iter-266).
    ///
    /// Mutation commands (set, remove, append, task, mv, tags rename,
    /// properties rename, links fix) still read/write individual files on disk
    /// but also patch the index in-place after each mutation — keeping
    /// the index current for subsequent queries. A file the index has never
    /// seen (created by an editor or Obsidian since the last create-index)
    /// is *upserted*: its full entry and outgoing links are inserted, not
    /// dropped, so indexed reads match a disk scan after the mutation.
    /// `set`/`append`/`remove` go further: every file they read whose
    /// `(mtime, size)` no longer matches the snapshot is rescanned, even when
    /// the mutation itself changes nothing (`0 modified`) — so a body edited
    /// by hand between `create-index` and the mutation cannot leave the entry
    /// describing bytes that are gone. `--dry-run` writes nothing, stale
    /// entry or not.
    /// `links fix`/`links auto` additionally mtime-check every indexed entry
    /// before their discovery pass, rescan files that changed on disk since
    /// create-index, and upsert files the index does not know yet (with a
    /// warning), so an externally edited vault is not silently trusted.
    ///
    /// If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
    /// falls back to a full disk scan automatically.
    ///
    /// STALENESS PROBE: on load, hyalo compares directory mtimes in the
    /// vault (the root and every directory up to 3 levels below it — cheap,
    /// directory-only stats, no file reads) against the snapshot's creation
    /// time and warns `index older than vault` when one postdates it. Two
    /// blind spots: in-place edits of existing notes (content changes that
    /// don't add, remove, or rename any file), and files added or removed
    /// inside a directory more than 3 levels deep — see `create-index
    /// --help` for the full contract. The warning never stops the run:
    /// stale results are still served.
    #[arg(long)]
    pub index: bool,

    /// Use the snapshot index at PATH instead of `.hyalo-index`
    ///
    /// Implies `--index`. Relative paths are resolved against the current
    /// working directory (not the vault dir); absolute paths are used as-is.
    ///
    /// Reading a snapshot from anywhere on disk is allowed. *Writing* one is
    /// not: on `create-index` / `drop-index` this flag is an alias for the
    /// output path, and a path outside the vault is refused unless
    /// `--allow-outside-vault` is also passed.
    ///
    /// Read-only commands skip the disk scan entirely. Mutation commands
    /// patch the index in-place after each write — see `--index` for details.
    ///
    /// If the index file is incompatible hyalo falls back to a disk scan.
    #[arg(long, value_name = "PATH")]
    pub index_file: Option<PathBuf>,
}

impl IndexFlags {
    /// Return the effective index path given the vault directory.
    ///
    /// - `--index-file PATH` wins; relative paths are returned as-is
    ///   (caller resolves against CWD).
    /// - Bare `--index` returns `vault_dir/.hyalo-index` (relative to vault,
    ///   not CWD; caller should not CWD-resolve this).
    /// - Neither flag → `None`.
    pub(crate) fn effective_index_path(&self, vault_dir: &Path) -> Option<PathBuf> {
        if let Some(ref p) = self.index_file {
            Some(p.clone())
        } else if self.index {
            Some(vault_dir.join(".hyalo-index"))
        } else {
            None
        }
    }
}

/// Resolve a file argument that can be passed as positional or --file flag.
/// Returns an error if neither is provided.
pub(crate) fn resolve_single_file(
    positional: Option<String>,
    flag: Option<String>,
) -> anyhow::Result<String> {
    match (positional, flag) {
        (Some(f), None) | (None, Some(f)) => Ok(f),
        (None, None) => anyhow::bail!("required argument missing: provide <FILE> or --file <FILE>"),
        // conflicts_with prevents this at parse time; defensive fallback.
        (Some(_), Some(_)) => anyhow::bail!("cannot specify both <FILE> and --file"),
    }
}

/// Compose a version string in the form `"{PKG} ({SHA} {DATE})"`.
///
/// Returns the bare `pkg` string when `sha` is empty (tarball / offline build).
fn format_version_string(pkg: &str, sha: &str, date: &str) -> String {
    if sha.is_empty() {
        pkg.to_string()
    } else {
        format!("{pkg} ({sha} {date})")
    }
}

/// Return the long-form version string used by `--version` / `-V`.
///
/// Memoized in a `OnceLock` because clap's `version =` attribute requires a
/// `&'static str`. The SHA and date come from env vars set by `build.rs`
/// (`HYALO_BUILD_VERSION_SHA`, `HYALO_BUILD_DATE`).
pub(crate) fn build_version_string() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        format_version_string(
            env!("CARGO_PKG_VERSION"),
            env!("HYALO_BUILD_VERSION_SHA"),
            env!("HYALO_BUILD_DATE"),
        )
    })
}

/// Build the top-level `--help` prose.
///
/// A function rather than a string literal because the OUTPUT paragraph names
/// the list commands, which are owned by [`crate::list_commands::LIST_COMMANDS`]
/// (iter-192: generate, don't restate).
pub(crate) fn build_long_about() -> &'static str {
    static LONG_ABOUT: OnceLock<String> = OnceLock::new();
    LONG_ABOUT.get_or_init(|| {
        LONG_ABOUT_TEMPLATE.replace(LIST_COMMANDS_PLACEHOLDER, list_commands_phrase())
    })
}

/// Build the `--count` flag's long help.
///
/// Same rationale as [`build_long_about`]: the list of commands accepting
/// `--count` is derived, never restated.
pub(crate) fn build_count_long_help() -> &'static str {
    static COUNT_HELP: OnceLock<String> = OnceLock::new();
    COUNT_HELP.get_or_init(|| {
        format!(
            "Print only the total count as a bare integer for list commands.\n\
             List commands (those whose envelope carries a `total`): {}.\n\
             Shortcut for --jq '.total'. Incompatible with --jq.\n\
             Any other command exits 1 with an explanatory error rather than \
             silently printing its full output.",
            list_commands_phrase()
        )
    })
}

/// Template for [`build_long_about`]; [`LIST_COMMANDS_PLACEHOLDER`] is
/// substituted at runtime.
const LONG_ABOUT_TEMPLATE: &str = "Hyalo — query, filter, and mutate YAML frontmatter across markdown file collections.\n\n\
        Compatible with Obsidian vaults, Zettelkasten systems, and any directory of .md files \
        with YAML frontmatter. Also resolves [[wikilinks]] and manages task checkboxes.\n\n\
        SCOPE: Hyalo operates on a directory of .md files. It can query and mutate frontmatter \
        properties, tags, tasks, and links.\n\n\
        PATH RESOLUTION: All file and --glob paths are relative to --dir (defaults to \".\"). \
        If a file path starts with the --dir prefix, it is stripped automatically \
        (e.g. --file docs/note.md resolves to note.md when --dir is docs). \
        Globs use standard syntax: '**/*.md' matches recursively, 'notes/*.md' matches one level.\n\n\
        OUTPUT: Default format is \"text\" when stdout is a terminal, \"json\" when piped. \
        All JSON is wrapped in a consistent envelope:\n\
          {\"results\": <payload>, \"total\": N, \"hints\": [...]}\n\
        total is present for list commands ({LIST_COMMANDS}). \
        hints is always present (empty [] when --no-hints). \
        --jq operates on the full envelope, e.g. --jq '.results[].file' or --jq '.total'.\n\
        --count prints just the total as a bare integer (shortcut for --jq '.total').\n\
        RESULTS CONVENTIONS: the envelope owns \"total\". When a command repeats \"total\" inside \
        results it always means the number of items that command considered (a denominator) — \
        never a count of findings, which is named for what it counts (lint: violations, \
        links auto: matched). Top-level results keys are always present, including when the \
        value is 0, false, [] or null; only per-item records inside arrays omit absent optional \
        keys. Every mutating command whose results is an object reports dry_run; the apply-style \
        generators (madr, okf, changelog) and batch mv also keep an older apply/applied key that \
        is always its exact inverse. skipped_count is reported by the bulk-mutation family only \
        (set, remove, append, properties rename, tags rename) — a single-target command has no \
        scanned-but-unchanged set. task toggle/task set return an array of per-task records with \
        no top-level object; their dry-run records carry old_status, applied records do not.\n\
        Use --format text for human-readable output, --format json for machine-readable output. \
        Successful output goes to stdout; errors go to stderr with exit code 1 (user error) or 2 (internal error).\n\n\
        ABSOLUTE LINKS: Links like `/docs/page.md` are resolved by stripping a site prefix. \
        By default the prefix is auto-derived from --dir's last path component (e.g. --dir ../my-site/docs → prefix \"docs\"). \
        Override with --site-prefix <PREFIX>, or --site-prefix \"\" to resolve absolute links from the vault/bundle root (strip only the leading `/`). Also settable in .hyalo.toml. \
        For bundle-root resolution (e.g. OKF bundles where `/x/y.md` is relative to the bundle root), set `site_prefix = \"\"` so only the leading `/` is stripped — this also avoids mis-stripping when a bundle subdir shares its name with the vault dir.\n\n\
        CONFIG: Place a .hyalo.toml in the working directory to set defaults:\n\
          dir = \"vault/\"        # default --dir\n\
          format = \"text\"       # pin format regardless of TTY detection\n\
          hints = false         # disable hints (CLI default is on)\n\
          site_prefix = \"docs\"  # override auto-derived site prefix for absolute links\n\
        CLI flags always take precedence.\n\n\
        See COMMAND REFERENCE below for full syntax of each command.";

#[derive(Parser)]
#[command(
    name = "hyalo",
    version = build_version_string(),
    about = "Query, filter, and mutate YAML frontmatter across markdown file collections",
    long_about = build_long_about(),
    // iter-256 HELP-5: clap's generated `help` subcommand renders the LONG
    // help. Agents type `hyalo help find` out of habit and get ~28 KB where
    // they wanted the ~3 KB `-h` page. Suppressing the generated subcommand
    // lets `Commands::Help` take the name and forward to the short page; the
    // short page's own footer points at `--help` for the long one.
    disable_help_subcommand = true
)]
pub(crate) struct Cli {
    /// Vault root for file and glob paths (default: .)
    ///
    /// Root directory for resolving all file and --glob paths.
    /// Default: "." (Override via .hyalo.toml)
    #[arg(
        short,
        long,
        global = true,
        long_help = concat!(
            "Root directory for resolving all file and --glob paths. ",
            "Default: \".\" (override via .hyalo.toml).\n\n",
            "--dir names a VAULT, not a config file. Naming the directory .hyalo.toml already ",
            "resolves to keeps that config in effect (the flag is redundant, and hyalo says so). ",
            "Naming a different directory switches to that directory's own .hyalo.toml if it has ",
            "one, else built-in defaults \u{2014} reported on stderr, because the config in your ",
            "working directory then no longer applies.\n\n",
            "A project-local .hyalo.toml's own `dir` value must resolve at-or-below the directory ",
            "the file lives in \u{2014} an absolute path or one that nets above it via `..` refuses ",
            "every command until fixed. This flag is the escape hatch: pass --dir explicitly to ",
            "point hyalo at a location the config itself is not allowed to claim.\n\n",
            "Run `hyalo config --dir <path>` to see which config file an invocation would use."
        )
    )]
    pub dir: Option<PathBuf>,

    /// Output format (default: text on a terminal, json when piped)
    ///
    /// Output format: "json" or "text".
    /// Default: "text" when stdout is a terminal, "json" when piped.
    /// Override for a session via .hyalo.toml: format = "text"
    #[arg(long, global = true)]
    pub format: Option<Format>,

    /// jq filter over the JSON envelope
    ///
    /// Apply a jq filter expression to the JSON output of any command.
    /// Operates on the full JSON envelope: {"results": ..., "total": N, "hints": [...]}.
    /// The filtered result is printed as plain text. Incompatible with --format text
    /// (combining them is a user error and exits 1).
    /// Example: --jq '.results[].file' or --jq '.results | map(.properties.status) | unique'.
    /// LIMITS: a filter is given 3 seconds of wall-clock time (a pathological filter —
    /// infinite recursion with no output, or building a huge intermediate array before
    /// ever yielding a value, e.g. '[range(3e8)]' — errors out instead of hanging or
    /// exhausting memory), may emit at most 1,000,000 output values, and the total
    /// emitted text is capped at 10 MiB. Any limit breach exits 1 with a clean error,
    /// never a hang or an OOM.
    #[arg(long, global = true, value_name = "FILTER")]
    pub jq: Option<String>,

    /// Print just the total as a bare integer (list commands)
    ///
    /// Print only the total count as a bare integer for list commands.
    /// Shortcut for --jq '.total'. Incompatible with --jq.
    // The full command list lives in `build_count_long_help()` so it is derived
    // from LIST_COMMANDS rather than restated here (iter-192).
    #[arg(long, global = true, long_help = build_count_long_help())]
    pub count: bool,

    /// Force hints on (already the default)
    ///
    /// Force hints on (already the default).
    /// Text mode: '-> hyalo ...  # description' lines — concrete, copy-pasteable commands with descriptions.
    /// JSON mode: populates the "hints" array in the envelope (always present, empty when suppressed).
    /// Suppressed when --jq is active.
    //
    // iter-254 (HELP-9): hidden from `-h`. A flag that forces on what is
    // already on teaches nothing on the page an agent reads first, and
    // `--no-hints` sitting right beside it already says which way the default
    // points. `--help` still documents it.
    #[arg(long, global = true, hide_short_help = true)]
    pub hints: bool,

    /// Disable drill-down hints (on by default)
    ///
    /// Disable drill-down command hints (enabled by default).
    /// Override via .hyalo.toml: hints = false
    /// When both --hints and --no-hints are present, --hints takes precedence.
    #[arg(long, global = true)]
    pub no_hints: bool,

    /// Prefix stripped from /root-absolute/links
    ///
    /// Site prefix for resolving root-absolute links like `/docs/page.md`.
    /// When a markdown file contains a link like `/docs/guides/setup.md`, hyalo strips the
    /// leading `/<prefix>/` to get the vault-relative path `guides/setup.md`. This is how
    /// documentation sites (GitHub Pages, VuePress, Docusaurus) map URL paths to file paths.
    ///
    /// By default, hyalo auto-derives the prefix from --dir's last path component:
    ///   --dir ../vscode-docs/docs  →  prefix = "docs"
    ///   --dir /home/me/wiki        →  prefix = "wiki"
    ///   --dir .                    →  prefix = name of the current directory
    ///
    /// Use --site-prefix to override when the directory name doesn't match the URL prefix,
    /// or pass --site-prefix "" to resolve absolute links from the vault/bundle root:
    /// only the leading `/` is stripped, so `/guides/setup.md` → `guides/setup.md`.
    ///
    /// Also settable via `site_prefix = "docs"` in .hyalo.toml.
    /// Precedence: --site-prefix flag > .hyalo.toml > auto-derived from --dir.
    /// Run `hyalo config` to see the effective value and where it came from.
    /// `hyalo links fix` warns on stderr when the effective prefix stripped
    /// 0 of N site-absolute links to a plausible vault path — a real MDN
    /// checkout with the auto-derived one-segment prefix left every
    /// `/en-US/docs/...` link unresolved, since `docs` names no real
    /// top-level entry once `en-US` alone is stripped.
    #[arg(long, global = true, value_name = "PREFIX")]
    pub site_prefix: Option<String>,

    /// Suppress warnings on stderr
    ///
    /// Suppress all warnings printed to stderr.
    /// Useful in scripts or CI pipelines where warning noise is undesirable.
    /// Identical warnings are always deduplicated regardless of this flag;
    /// use `--quiet` to suppress them entirely.
    ///
    /// One exception: a `.hyalo.toml` that could not be parsed is always
    /// reported. That warning means the run is using a different vault and a
    /// different rule set than the config asked for, which is not noise.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Snapshot index path (alias for the subcommand flag)
    ///
    /// Use the snapshot index at PATH (global alias for the per-subcommand `--index-file`).
    /// Equivalent to passing `--index-file PATH` after the subcommand.
    /// When both the global flag and the subcommand flag are provided, the
    /// subcommand value takes precedence.
    ///
    /// Relative paths are resolved against the current working directory.
    /// Reading is unrestricted; writing one outside the vault (`create-index`,
    /// `drop-index`) additionally needs `--allow-outside-vault`.
    #[arg(long, global = true, value_name = "PATH")]
    pub index_file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// All filter arguments for `hyalo find`, extracted so they can be serialized as views.
#[derive(Debug, Clone, Default, clap::Args, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct FindFilters {
    /// BM25 search pattern (stored in views, not a CLI arg on find — find uses a positional arg instead)
    #[arg(skip)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Regex body search, case-insensitive (excludes PATTERN)
    ///
    /// Regex body text search (case-insensitive by default; use (?-i) to override).
    /// Mutually exclusive with PATTERN.
    #[arg(long, short = 'e', value_name = "REGEX", help_heading = "Filters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regexp: Option<String>,
    /// K=V|K!=V|K>V|K>=V|K<V|K<=V|K|!K|K~=/re/i|K=null; AND; K may be a dot-path
    ///
    /// Property filter: K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists), !K (absent),
    /// K~=pat or K~=/pat/i (regex). Repeatable (AND). K may be a dot-path into nested maps and
    /// sequences (contact.email, contacts.0.email, contacts.email = any element).
    ///
    /// Value syntax: K=null matches a property present with a YAML null (`~`, `null`, or an
    /// empty value) and K!=null a present non-null one; K=[] matches an empty list, K!=[] a
    /// non-empty one. A list *containing* a null does not match K=null.
    /// Ordering ops (>, >=, <, <=) compare numerically when both sides are numbers, by date
    /// when both are ISO dates, and as text only when both are plain strings — a value of a
    /// different kind never matches, so `last>=2023-09-01` skips `last: "[[2022-04]]"`.
    /// The regex operator is ~= (not =~, which is rejected), and its pattern must not be empty.
    #[arg(
        short,
        long = "property",
        value_name = "FILTER",
        help_heading = "Filters"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<String>,
    /// Tag, exact or prefix ('project' matches 'project/backend'); repeatable (AND)
    ///
    /// Tag filter: exact or prefix match (e.g. 'project' matches 'project/backend' but not
    /// 'projects'). Repeatable (AND).
    #[arg(short, long, value_name = "TAG", help_heading = "Filters")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tag: Vec<String>,
    /// Task presence: 'todo', 'done', 'any', or a single status character
    #[arg(long, value_name = "STATUS", help_heading = "Filters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// Heading substring, '##' pins the level, or /regex/; repeatable (OR)
    ///
    /// Section heading filter: case-insensitive substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
    /// prefix '##' to pin heading level; use '/regex/' for regex (e.g. '/DEC-03[12]/'). Repeatable (OR).
    /// A file with more than one matching heading unions all of them (unlike `task --section`, which refuses)
    #[arg(
        short,
        long = "section",
        value_name = "HEADING",
        help_heading = "Filters"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[arg(
        short,
        long,
        conflicts_with_all = ["glob", "files_from"],
        help = FILE_FLAG_SHORT_DOC,
        long_help = FILE_FLAG_DOC,
        help_heading = "Filters"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file: Vec<String>,
    /// Glob(s) relative to --dir, repeatable; '!' negates ('!**/draft-*')
    ///
    /// Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate
    /// (e.g. '!**/draft-*').
    #[arg(short, long, conflicts_with_all = ["file", "files_from"], help_heading = "Filters")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub glob: Vec<String>,
    /// Read paths from PATH, one per line ('-' = stdin)
    ///
    /// Read file paths from PATH (one per line); use '-' to read from stdin.
    /// Mutually exclusive with --file and --glob.
    /// Non-.md paths and paths outside the vault are silently skipped (counters appear in JSON envelope).
    /// Repo-relative paths with the configured vault dir prefix (e.g. files/en-us/x.md with --dir files/en-us)
    /// are resolved by trying vault-relative first, then stripping the full dir prefix and retrying.
    /// Input is deduplicated; results follow first-seen order.
    /// CHANGED-FILES RECIPE: `git diff --name-only origin/main | hyalo <cmd> --files-from -`
    /// restricts a run to what a branch touched (any VCS or `find`/`fd`/`rg -l` works the same way;
    /// hyalo shells out to nothing and has no VCS-specific flag).
    #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "glob"], help_heading = "Filters")]
    #[serde(skip)]
    pub files_from: Option<String>,
    /// all|file|modified|size|lines|title|properties|properties-typed|tags|sections|tasks|links|backlinks — exact projection
    ///
    /// Without --fields: file, modified, size, lines, title, properties, tags. With --fields:
    /// exactly the named fields plus file (filters add what they need).
    ///
    /// `file` is the only unconditional key — it names the result — so `--fields title` returns
    /// {file, title} and `--fields size,lines` returns {file, size, lines}. `modified`, `size`
    /// and `lines` are ordinary members of the default set: cheap enough to always pay for, and
    /// the inputs an agent uses to choose its next call (`read --lines`, recency), but dropped
    /// when an explicit --fields does not name them. `--fields file` is accepted and means
    /// {file}; `--fields all` selects everything. A saved view's pinned `fields` behaves exactly
    /// like an explicit --fields; a CLI --fields on top replaces the pin rather than adding to it.
    ///
    /// A filter that implies a field still returns it, on top of whatever set is in force:
    /// --section adds sections, --task adds tasks, --broken-links adds links,
    /// --orphan/--dead-end add links and backlinks, and --sort links_count/backlinks_count add
    /// the field they rank on.
    ///
    /// 'properties' is a {key: value} map WITHOUT the promoted 'title' property (which has its
    /// own field whenever 'title' is included, and stays in the map when the frontmatter value
    /// is a list or a map and so cannot be promoted); 'properties-typed' is a
    /// [{name, type, value}] array; 'backlinks' requires scanning all files; 'title' is the
    /// frontmatter title property — any scalar, stringified as written — or the first H1
    /// heading (null if neither found). 'outline' is an alias for 'sections'. Note: in JSON
    /// output, `properties-typed` is serialized as `properties_typed` (underscore).
    #[arg(
        long,
        value_name = "FIELDS",
        use_value_delimiter = true,
        help_heading = "Output"
    )]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// file (default)|modified|backlinks_count|links_count|title|date|score|property:K
    ///
    /// Sort order: 'file' / 'path' (default), 'modified', 'backlinks_count', 'links_count',
    /// 'title', 'date', 'score', or 'property:<KEY>' for any frontmatter property.
    ///
    /// DIRECTION: every key sorts ascending and --reverse inverts it, so
    /// `--sort backlinks_count --reverse` is "most linked first" exactly as
    /// `--sort modified --reverse` is "newest first". 'score' is the one exception: it ranks
    /// best-match-first (descending relevance), and --reverse puts the weakest match first.
    /// Files whose sort property is missing or null always sort last, in both directions.
    ///
    /// For 'property:<KEY>',
    /// values of different JSON types (e.g. some files have a string, others a number) compare by
    /// raw JSON text -- grouped by type but not sensibly ordered within a numeric group -- and a
    /// stderr warning names the property when this happens; use a consistent type in frontmatter
    /// for a meaningful sort.
    #[arg(long, help_heading = "Output")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    /// Reverse the sort order [alias: --desc]
    ///
    /// Reverse the sort order (ascending becomes descending and vice versa). Alias: --desc.
    #[arg(long, alias = "desc", help_heading = "Output")]
    #[serde(skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Max results, 0 = unlimited (default cap: 50)
    ///
    /// Maximum number of results to return (0 = unlimited).
    /// Default cap is bypassed when --jq or --count is used.
    #[arg(short = 'n', long, value_parser = parse_limit, help_heading = "Output")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Only files with an unresolved link or dead heading anchor
    ///
    /// Only return files with at least one unresolved link or dead heading anchor
    /// (auto-includes links field).
    /// Targets that resolve above the vault root are out of scope, not broken: they are
    /// flagged `out_of_vault` on the link and do not qualify a file here.
    /// A `#fragment` matches either the raw heading text or the rendered GitHub slug
    /// (`#sub-section` for `### Sub Section`); same-file fragments (`[b](#nope)`) are
    /// checked against the file's own headings and reported with an empty target.
    /// A heading carrying a template expression (`## {% data variables.x %}`, `{{ y }}`)
    /// renders to an anchor hyalo cannot compute, so anchors into such a file are never
    /// reported broken.
    /// Every listed link carries its 1-based source `line`, the same one `lint` (HYALO006)
    /// reports, and links are listed in document order.
    /// An external URI (`obsidian://`, `mailto:`, `https:`) and a link that resolves to a
    /// non-`.md` vault file (an image, a `.base`) are never broken — they are reported with
    /// `kind` `external` / `attachment` and never qualify a file here.
    #[arg(long, help_heading = "Filters")]
    #[serde(skip_serializing_if = "is_false")]
    pub broken_links: bool,
    /// Exit 1 if any results, 0 if empty — a CI gate
    ///
    /// A CI gate for any find query, most commonly `find --broken-links --strict` to fail a build
    /// on a dead heading anchor. Before this, `find --broken-links` always exited 0 even when it
    /// reported findings, so a vault whose only defect was a dead anchor passed CI silently.
    #[arg(long, help_heading = "Output")]
    #[serde(skip_serializing_if = "is_false")]
    pub strict: bool,
    /// Only orphan files: no inbound and no outbound links (auto-includes links and backlinks)
    ///
    /// Deciding orphanhood needs both directions of the graph, so both fields come back
    /// whether or not --fields names them.
    #[arg(long, help_heading = "Filters")]
    #[serde(skip_serializing_if = "is_false")]
    pub orphan: bool,
    /// Only dead-end files: inbound links but no outbound links (auto-includes links and backlinks)
    ///
    /// Deciding dead-endedness needs both directions of the graph, so both fields come back
    /// whether or not --fields names them.
    #[arg(long, help_heading = "Filters")]
    #[serde(skip_serializing_if = "is_false")]
    pub dead_end: bool,
    /// Title substring (case-insensitive) or /regex/[i]
    ///
    /// Filter by title: case-insensitive substring match against the displayed title
    /// (frontmatter 'title' property or first H1 heading). Use /regex/ for regex
    /// (e.g. '/^The/' or '/^The/i').
    #[arg(long, help_heading = "Filters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// BM25 Snowball stemmer language (default: english) [alias: --stemmer]
    ///
    /// Stemmer language for BM25 body search (also --stemmer). Selects Snowball stemmer for BM25
    /// tokenization — NOT markdown code-block language.
    /// Default: english. Accepts full names (english, german, …) or ISO 639-1 codes (en, de, …).
    /// Supported: arabic (ar), danish (da), dutch (nl), english (en), finnish (fi), french (fr),
    /// german (de), greek (el), hungarian (hu), italian (it), norwegian (no, nb, nn),
    /// portuguese (pt), romanian (ro), russian (ru), spanish (es), swedish (sv), tamil (ta),
    /// turkish (tr).
    #[arg(long, alias = "stemmer", value_name = "LANG", help_heading = "Filters")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Print matching paths only, one per line — no envelope, no hints
    ///
    /// Print only the file path of each matching entry, one per line — no JSON,
    /// no envelope, no count, no hints. grep `-l` precedent: the agent/
    /// pipeline projection of a find result set, usable in `sort`, `xargs`,
    /// and `while read` loops. Zero results → empty output, exit 0.
    ///
    /// Conflicts with `--jq`, `--count`, and an explicit `--format json`
    /// (mutually exclusive projections — pick one). `--strict` still flips
    /// the exit code (1 when results exist), so `find --property status=planned
    /// --filenames-only --strict` is a CI gate that lists the offenders and
    /// fails. Combines with every other filter (`--property`, `--tag`,
    /// `--glob`, `--broken-links`, …) exactly as `find`
    /// normally does.
    #[arg(
        long,
        conflicts_with_all = ["jq", "count", "filenames0"],
        help_heading = "Output"
    )]
    #[serde(skip_serializing_if = "is_false")]
    pub filenames_only: bool,
    /// Like --filenames-only but NUL-separated, for `xargs -0`
    ///
    /// NUL-delimited sibling of `--filenames-only` (iter-238): each matching
    /// file path is printed terminated by a NUL byte instead of a newline,
    /// exactly like GNU `find -print0`. Safe for filenames that contain
    /// newlines (which are legal in POSIX filenames, though not on Windows),
    /// and composes
    /// with `xargs -0` / `while IFS= read -r -d ''`. Same semantics as
    /// `--filenames-only` otherwise: no JSON, no envelope, no count, no hints;
    /// zero results → empty output, exit 0; `--strict` still flips the exit
    /// code when results exist.
    ///
    /// Mutually exclusive with `--filenames-only`, `--jq`, `--count`, and an
    /// explicit `--format json` (pick one projection).
    #[arg(long, conflicts_with_all = ["jq", "count", "filenames_only"], help_heading = "Output")]
    #[serde(skip_serializing_if = "is_false")]
    pub filenames0: bool,
}

impl FindFilters {
    /// Merge CLI overrides onto a view's filters.
    /// - Vec fields: CLI extends the view
    /// - Option fields: CLI overrides if Some
    /// - Bool fields: OR (CLI can turn on, not off)
    pub(crate) fn merge_from(&mut self, overlay: &Self) {
        if overlay.pattern.is_some() {
            self.pattern.clone_from(&overlay.pattern);
        }
        if overlay.regexp.is_some() {
            self.regexp.clone_from(&overlay.regexp);
        }
        self.properties.extend(overlay.properties.iter().cloned());
        self.tag.extend(overlay.tag.iter().cloned());
        if overlay.task.is_some() {
            self.task.clone_from(&overlay.task);
        }
        self.sections.extend(overlay.sections.iter().cloned());
        // file, glob, and files_from are mutually exclusive (clap enforces at parse time).
        // If the overlay provides any, it replaces the base to avoid invalid combinations.
        if overlay.files_from.is_some() {
            self.files_from.clone_from(&overlay.files_from);
            self.file.clear();
            self.glob.clear();
        } else if !overlay.file.is_empty() {
            self.file.extend(overlay.file.iter().cloned());
            self.glob.clear();
        } else if !overlay.glob.is_empty() {
            self.glob.extend(overlay.glob.iter().cloned());
            self.file.clear();
        }
        // iter-254 (DEC-254): --fields is a projection, not an accumulator.
        // A pinned `fields` behaves exactly like an explicit --fields, so a CLI
        // --fields REPLACES the pin — extending it would make
        // `find --view titles --fields tags` return more than either alone
        // asked for, and there would be no way to narrow a view.
        if !overlay.fields.is_empty() {
            self.fields.clone_from(&overlay.fields);
        }
        if overlay.sort.is_some() {
            self.sort.clone_from(&overlay.sort);
        }
        self.reverse = self.reverse || overlay.reverse;
        if overlay.limit.is_some() {
            self.limit = overlay.limit;
        }
        self.broken_links = self.broken_links || overlay.broken_links;
        self.strict = self.strict || overlay.strict;
        self.orphan = self.orphan || overlay.orphan;
        self.dead_end = self.dead_end || overlay.dead_end;
        if overlay.title.is_some() {
            self.title.clone_from(&overlay.title);
        }
        if overlay.language.is_some() {
            self.language.clone_from(&overlay.language);
        }
        // --filenames-only / --filenames0 are output-shaping bools (like
        // --strict): the overlay can turn them on, never off. A view may carry
        // one, and a CLI flag turns on on top of any view. clap rejects the
        // two projections together, so at most one can ever be set.
        self.filenames_only = self.filenames_only || overlay.filenames_only;
        self.filenames0 = self.filenames0 || overlay.filenames0;
    }
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Search and filter markdown files — returns one compact object per file (see --fields)
    #[command(long_about = "Search and filter markdown files.\n\n\
            Returns a JSON envelope: {\"results\": [...], \"total\": N, \"hints\": [...]}.\n\
            Each item carries the default field set — file, modified, size, lines, title, \
            properties, tags — where `title` is promoted out of `properties`. \
            --fields adds sections, tasks, links, backlinks and properties-typed, and an \
            explicit --fields is an exact projection: exactly the named fields plus `file`, \
            which is the one key no projection can drop. A filter that needs a field adds it \
            regardless.\n\n\
            SEARCH MODES:\n\
            - PATTERN (positional): BM25 ranked full-text search with stemming. Results are sorted by \
            relevance score (highest first) unless --sort is specified. Each result includes a numeric \
            'score' field in the output. Stemming normalises words to their root: 'running' matches \
            documents containing 'run', 'runner', 'running', etc.\n\
            - --regexp/-e REGEX: regex body text search (case-insensitive by default; unranked; \
            results include per-line 'matches' instead of 'score'). Mutually exclusive with PATTERN.\n\n\
            QUERY SYNTAX (for PATTERN):\n\
            - Multiple words: implicit AND — all terms required (e.g. 'rust programming' returns \
            only documents containing both words)\n\
            - OR keyword: explicit OR — either term matches (e.g. 'rust OR golang' returns docs with \
            either word, ranked by combined BM25 score). Case-insensitive ('or' also works). When OR \
            is present, all non-negated terms become OR alternatives.\n\
            - \"quoted phrase\": exact consecutive match after stemming (e.g. '\"javascript promises\"' \
            matches only documents with that exact phrase)\n\
            - -term: exclude documents containing this term (e.g. 'rust -javascript' finds Rust docs \
            that don't mention javascript; stemming applies, so '-running' also excludes 'run')\n\
            - AND keyword: accepted but optional (implicit between terms)\n\
            - Combine freely: 'rust -java', 'rust OR golang', '\"error handling\" -panic'\n\n\
            LANGUAGE: The --language flag (or [search] language in .hyalo.toml, or frontmatter \
            'language' property per file) selects the Snowball stemmer for tokenization. Default: english. \
            Accepts full names or ISO 639-1 codes (e.g. 'en' for english, 'de' for german). \
            Supported: arabic (ar), danish (da), dutch (nl), english (en), finnish (fi), french (fr), \
            german (de), greek (el), hungarian (hu), italian (it), norwegian (no, nb, nn), portuguese (pt), \
            romanian (ro), russian (ru), spanish (es), swedish (sv), tamil (ta), turkish (tr). \
            Language precedence: frontmatter > --language > config > english.\n\
            CJK: Chinese/Japanese/Korean text (and other scripts written without spaces between \
            words) is tokenized as overlapping 2-character bigrams rather than whole words, since \
            there is no dictionary-based segmenter. A query is tokenized the same way, so a CJK \
            substring query matches, but this is an approximation, not true word segmentation -- it \
            can occasionally over-match (two bigrams from unrelated parts of a document both present) \
            but should not under-match a real substring.\n\n\
            FILTERS: All filters are AND'd together.\n\
            - --property K=V: frontmatter property filter (supports =, !=, >, >=, <, <=, bare K for existence, !K for absence, K~=pattern or K~=/pattern/i for regex)\n\
              OPERATOR TABLE:\n\
                K=V / K!=V        equality / inequality (case-insensitive; any element of a list)\n\
                K> K>= K< K<=     ordered comparison, typed (see below)\n\
                K / !K            present / absent\n\
                K~=pat K~=/pat/i  regex over the value (empty pattern rejected; `=~` is not an operator)\n\
                K=null / K!=null  present with a YAML null (`~`, `null`, empty value) / present and non-null\n\
                K=[] / K!=[]      present and an empty list / present and not an empty list\n\
              A list CONTAINING a null (`aliases: [null]`) does not match `K=null` — the value's own\n\
              type is what is tested, so `K=null` and `--fields properties-typed` (type \"null\") agree.\n\
              TYPED COMPARISONS: >, >=, < and <= compare numerically when both sides parse as numbers\n\
              (so `rating>=6` matches `rating: \"7\"`), by date when both parse as ISO dates, and as text\n\
              only when both are plain strings. A value of any other kind never matches, so\n\
              `last>=2023-09-01` skips `last: \"[[2022-04]]\"` instead of comparing it as text.\n\
              Dot-paths traverse nested frontmatter: a literal dotted key in a flat map wins first, \
            then `contact.email=x` descends the map `contact: {email: x}`. Sequences are descended too: \
            a numeric segment indexes one element (`contacts.0.email`), any other segment auto-descends into \
            EVERY element and collects the hits (`contacts.email=x` matches when any contact has that email; \
            `!=` matches when none does).\n\
            - --tag T: tag filter (exact or prefix via '/': 'project' matches 'project/backend' but NOT 'projects' — no substring or fuzzy matching)\n\
            - --task STATUS: task presence filter ('todo', 'done', 'any', or a single status char)\n\
            - --section HEADING: section scope filter (exclude files without a matching section; within \
            matching files, restrict tasks and content matches to the section scope; case-insensitive \
            substring (contains) match by default, e.g. 'Tasks' matches 'Tasks [4/4]'; use leading '#' \
            to pin heading level, e.g. '## Tasks'; use '/regex/' for regex matching). Repeatable (OR). \
            Nested subsections are included. When a file has more than one heading matching --section, \
            find UNIONS all of them (tasks/content from every matched section are included) -- unlike \
            `task toggle`/`read`/`set --section`, which refuse an ambiguous multi-heading match. This is \
            deliberate: find is a vault-wide read-only query where different files legitimately have \
            different heading sets, so there is no single 'the' match to disambiguate against, unlike a \
            single-file mutation. A stderr warning names how many result files hit this (not per-file, \
            to avoid spamming a large result set).\n\n\
            FIELDS: Every item carries file, modified, size (bytes) and lines. The default optional \
            fields are title, properties and tags; sections, tasks, links, backlinks and \
            properties-typed are opt-in via --fields (or --fields all), or come automatically from \
            the filter that implies them (--section, --task, --broken-links, --orphan, --dead-end, \
            --sort links_count|backlinks_count). Properties are a {key: value} map and do NOT repeat \
            the promoted title; use --fields properties-typed for a [{name, type, value}] array. \
            --format text prints the included field names under the results.\n\
            LINK KINDS: every entry in --fields links carries kind — wikilink (plain [[note]]), \
            embed (![[note]] / ![[img.png]]), markdown ([text](note.md)), external (any scheme: \
            URI: https, obsidian://, mailto:, file://) or attachment (resolved to a non-.md vault \
            file: an image, a PDF, an Obsidian .base). external and attachment links never count \
            as broken, never appear under --broken-links or HYALO006, and are not graph edges for \
            --orphan/--dead-end. Text mode prints the kind after the arrow unless it is wikilink. \
            A broken #anchor whose text is the prefix of exactly one heading in the target file \
            also carries suggested_fragment, the full heading to write instead (never applied \
            automatically).\n\
            SIZE: size/lines let a caller budget before reading -- pair them with \
            `read --lines A:B` or `read --section H` instead of pulling a large body whole.\n\
            RESULT SHAPE: `results` is always the array of matched files, whichever way the file \
            list was supplied -- `--file`, `--glob`, `--files-from` or a full scan all answer at \
            `.results[0]`. The three --files-from counters (files_missing, files_skipped_non_md, \
            files_skipped_outside_vault) are TOP-LEVEL envelope keys beside `total` and `hints`, \
            present on every find and zero when --files-from was not used. In JSON, \
            `--fields properties-typed` is emitted under the snake_case key `properties_typed` \
            (like every other envelope key); `--fields properties_typed` is accepted as a spelling \
            of the same field so a printed field list round-trips.\n\
            JQ: --jq operates on the full envelope. Examples: --jq '.results[].file', --jq '.total'.\n\
            VIEWS: --view <name> loads a saved filter set from .hyalo.toml. Additional CLI flags \
            merge on top: list filters (--property, --tag, --section, --glob) extend the view's \
            lists; scalar filters (--regexp, --sort, --limit, --title, --task, --language) override; bool \
            flags (--broken-links, --orphan, --dead-end, --reverse) OR. Example: hyalo find --view drafts --limit 5\n\
            PROJECTIONS: --filenames-only prints one raw file path per line (no JSON, no\n\
            envelope, no count, no hints) — the agent/pipeline counterpart to --format text's\n\
            human layout, usable in `sort`, `xargs`, and `while read` loops. Zero results →\n\
            empty output, exit 0. Conflicts with --jq, --count, and an explicit --format json.\n\
            --strict still flips the exit code (1 when results exist), so `find --filenames-only\n\
            --strict` is a CI gate that lists the offenders and fails.\n\
            --filenames0 is the NUL-delimited sibling (GNU `find -print0` precedent): each\n\
            path ends in a NUL byte instead of a newline, safe for filenames containing\n\
            newlines; pair it with `xargs -0`. Same zero-results/conflict/--strict rules as --filenames-only.\n\
            SEQUENCE-KEYED FILES (iterations, decisions, ...): address them by glob, and remember\n\
            the number may be zero-padded and the file archived in a subdirectory —\n\
            `find --glob '**/iteration-02-*.md'` reaches both `iterations/iteration-2-*.md`\n\
            and `iterations/done/iteration-02-links.md`.\n\
            COMMON MISTAKES:\n\
            - Property regex uses ~= (tilde-equals), NOT =~ (Perl-style). 'title=~/pat/' is a hard\n\
              error naming ~=; write 'title~=/pat/'. (Before iteration 264 it was silently accepted\n\
              as an equality test against the literal value '~/pat/', which matched YAML nulls.)\n\
            - An empty property regex ('title~=' or 'title~=//') is rejected: it matched every file.\n\
              Use bare 'title' to test presence, or 'title=null' for a present-but-null value.\n\
            - --title and --property title~= search the SAME promoted title: the frontmatter\n\
              `title` when it is a scalar, else the first H1, else the filename stem. Neither is\n\
              frontmatter-only. Use --property 'title' / '!title' to test the raw frontmatter key.\n\
            - --tag uses prefix matching: 'project' matches 'project/backend' but NOT 'projects'.\n\
            - For sequence-keyed lookups, prefer a filename glob (`--glob '**/iteration-206-*.md'`) over\n\
              `--property 'title~=206'` — the frontmatter title is typically `Iteration 206: …`,\n\
              which does not contain `iteration-206`.\n\
            POSITIONAL ARGUMENTS: The first positional argument is always PATTERN (body text search), not a file path. \
            Subsequent positional arguments are treated as FILE targets. \
            To filter by file without a body search, use --file instead of a positional argument.\n\
            A FILE target that names nothing is a user error at exit 1 — the same `file not found` \
            envelope `read` and `lint` emit, with the same `did you mean` / `--glob '<dir>/*'` \
            hints — so a typo can never be mistaken for a query that legitimately matched nothing. \
            A path already present in an `--index-file` snapshot is accepted without touching disk.\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            hyalo find 'error handling'\n\
            hyalo find --property status=draft --tag project\n\
            hyalo find --property 'title~=/^Design/i'\n\
            hyalo find --property aliases=null            # present, but the value is a YAML null\n\
            hyalo find --sort backlinks_count --reverse   # most linked first\n\
            hyalo find --property contacts.email=team@example.com   # dot-path into a list of maps\n\
            hyalo find --section 'Tasks' --task todo\n\
            hyalo find --broken-links --jq '[.results[] | .links[] | select(.path == null)]'\n\
            hyalo find --property status=planned --filenames-only   # agent/pipeline projection\n\
            git diff --name-only origin/main | hyalo find --files-from -")]
    Find {
        /// BM25 ranked full-text body search (stemmed; sorted by relevance)
        ///
        /// BM25 ranked body text search with stemming (e.g. "running" matches "run", "ran");
        /// results sorted by relevance.
        #[arg(value_name = "PATTERN", conflicts_with = "regexp")]
        pattern: Option<String>,
        /// Target file(s), positional form of --file
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file"])]
        file_positional: Vec<String>,
        /// Start from a saved view; CLI filters merge on top
        ///
        /// Use a saved view (named filter set from .hyalo.toml). Additional CLI filters
        /// are merged on top: list filters (--property, --tag, --section, --glob) extend
        /// the view; scalar filters (--sort, --limit, --regexp, --title, --task) override it.
        #[arg(long, value_name = "NAME", help_heading = "Filters")]
        view: Option<String>,
        #[command(flatten)]
        filters: FindFilters,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Read file body content, optionally filtered by section or line range (read-only)
    #[command(
        alias = "show",
        long_about = "Read the body content of a markdown file.\n\n\
            Returns the raw text after the YAML frontmatter block. Use --section to extract a \
            specific section by heading (case-insensitive substring match; use leading '#' to \
            pin heading level, e.g. '## Tasks'; use '/regex/' for regex matching; nested subsections are included), \
            --lines to slice a line range, and --frontmatter to include the YAML frontmatter.\n\n\
            OUTPUT: Defaults to plain text (unlike all other commands which default to JSON). \
            Pass --format json to get \
            {\"results\": {\"file\": \"...\", \"size\": N, \"lines\": N, \"content\": \"...\"}, \"hints\": [...]}. \
            `size` (body bytes) and `lines` (body line count) are the same numbers `find` reports, \
            so the two commands agree on what a read will cost. Text mode prints the body and \
            nothing else \u{2014} a header line would corrupt `hyalo read x.md > x.txt` and every \
            pipe into another tool \u{2014} so the size shows up there only in the hint below.\n\
            BUDGETING A LARGE READ: over 8 KiB of body, `read` stops offering the whole file as a \
            cheap lookup and hints at two narrower reads instead \u{2014} `read --lines 1:80` for a \
            leading slice, or `find --file <path> --fields sections` to pick one section and then \
            `read --section`. The hint is suppressed once --lines or --section is already in play.\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            hyalo read notes/todo.md\n\
            hyalo read --file notes/todo.md --section Tasks\n\
            hyalo read --file notes/todo.md --lines 1:20\n\
            hyalo read --file notes/todo.md --frontmatter --format json"
    )]
    Read {
        #[command(flatten)]
        selection: InputSelection,
        /// Heading substring, '##' pins the level, or /regex/ (nested subsections included)
        ///
        /// Extract section(s) by case-insensitive substring match (e.g. 'Tasks' matches
        /// 'Tasks [4/4]'); prefix '##' to pin the heading level; use '/regex/' for a regex.
        /// Nested subsections are included.
        #[arg(short, long, value_name = "HEADING")]
        section: Option<String>,
        /// Slice by line range: 5:10, 5:, :10, or 5 (1-based, inclusive, relative to the body)
        ///
        /// The frontmatter block is not counted, so line 1 is the first line after it — even
        /// with --frontmatter. Note that `task --line` counts differently: those numbers are
        /// file-absolute, with the frontmatter included.
        #[arg(short, long, value_name = "RANGE")]
        lines: Option<String>,
        /// Include the YAML frontmatter in output
        ///
        /// Text output echoes the block's own bytes between its `---` fences —
        /// indentation, quote style and comments exactly as on disk; no YAML is
        /// re-serialized on a read path. JSON keeps the parsed map under
        /// `frontmatter` and adds the raw text as `frontmatter_raw` (null for a
        /// file with no frontmatter block).
        #[arg(long)]
        frontmatter: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Property operations: summary or bulk rename
    #[command(long_about = "Property operations across matched files.\n\n\
        Subcommands:\n\
        - summary: Unique property names, types, and file counts (read-only).\n\
        - rename: Rename a property key across files (mutates files).\n\n\
        EXAMPLES:\n\
        hyalo properties summary\n\
        hyalo properties summary --glob 'research/**/*.md'\n\
        hyalo properties rename --from old-key --to new-key")]
    Properties {
        /// Glob pattern(s) to filter which files to scan, relative to --dir
        ///
        /// (repeatable); prefix '!' to negate. Bare-group form of
        /// `properties summary --glob`.
        #[arg(short, long)]
        glob: Vec<String>,
        /// Maximum number of results to return (0 = unlimited). Bare-group
        ///
        /// form of `properties summary --limit`.
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        // iter-266 IDX-1 (BUG-11): `--index` reaches the bare group too, so
        // `hyalo properties --index` works like every other reading command
        // instead of erroring with "a similar argument exists: --index-file".
        #[command(flatten)]
        index_flags: IndexFlags,
        #[command(subcommand)]
        action: Option<PropertiesAction>,
    },
    /// Tag operations: summary or bulk rename
    #[command(long_about = "Tag operations across matched files.\n\n\
        Subcommands:\n\
        - summary: Unique tags with file counts (read-only).\n\
        - rename: Rename a tag across files (mutates files).\n\n\
        EXAMPLES:\n\
        hyalo tags summary\n\
        hyalo tags summary --glob 'research/**/*.md'\n\
        hyalo tags rename --from old-tag --to new-tag")]
    Tags {
        /// Glob pattern(s) to filter which files to scan, relative to --dir
        ///
        /// (repeatable); prefix '!' to negate. Bare-group form of
        /// `tags summary --glob`.
        #[arg(short, long)]
        glob: Vec<String>,
        /// Maximum number of results to return (0 = unlimited). Bare-group
        ///
        /// form of `tags summary --limit`.
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        // iter-266 IDX-1 (BUG-11): see `Properties`.
        #[command(flatten)]
        index_flags: IndexFlags,
        #[command(subcommand)]
        action: Option<TagsAction>,
    },
    /// Read, toggle, or set status on task checkboxes (single, bulk, or by section)
    #[command(long_about = "Read, toggle, or set status on task checkboxes.\n\n\
            Subcommands:\n\
            - read: Show task details for one or more tasks.\n\
            - toggle: Flip completion state ([ ] <-> [x], custom -> [x]).\n\
            - set: Set an arbitrary single-character status.\n\n\
            INPUT: FILE (positional or --file) and one of: --line (repeatable/comma-separated), --section <heading>, or --all.\n\
            SCOPE: Single file only.\n\
            SIDE EFFECTS: 'toggle' and 'set' modify the file on disk. 'read' is read-only.\n\n\
            EXAMPLES:\n\
            hyalo task toggle todo.md --all\n\
            hyalo task toggle todo.md --section Tasks\n\
            hyalo task toggle todo.md --line 5,7,9\n\
            hyalo task read todo.md --line 5\n\
            hyalo task set todo.md --line 5 --status '-'")]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Show a compact vault summary: file counts, property/tag/status counts, tasks, links, orphans, dead-ends (read-only)
    #[command(
        // iter-267 (HELP-14): the short page names the JSON result keys. A
        // caller reaching for `--jq` had to run the command once and read the
        // envelope back to learn them; `-h` is where that belongs.
        about = "Show a compact vault summary: file counts, property/tag/status counts, tasks, links, orphans, dead-ends (read-only)\n\
            RESULT KEYS (all under `results`): files.total, files.skipped, files.excluded, \
            files.directories, properties, tags, status, tasks.total, tasks.done, links.total, \
            links.broken, links.broken_anchors, orphans, dead_ends, recent_files, schema.\n\
            Example: hyalo summary --jq '.results.links.broken'",
        long_about = "Show a compact vault summary (~20-30 lines regardless of vault size).\n\n\
            RESULT KEYS (all under `results`): files.total, files.skipped (unparseable\n\
            frontmatter), files.excluded ([scan] exclude), files.directories, properties, tags,\n\
            status, tasks.total, tasks.done, links.total, links.broken, links.broken_anchors,\n\
            orphans, dead_ends, recent_files, schema. Example:\n\
            hyalo summary --jq '.results.links.broken'\n\n\
            OUTPUT: A 'VaultSummary' object with file counts (total + top-level directories), \
            property summary (unique names/types/counts), tag summary (unique tags/counts), \
            status grouping (value + count, no file lists), \
            task counts (total/done), link health (total/broken count, plus a distinct \
            broken_anchors count — a link whose target resolves but whose #fragment names no \
            heading there; omitted from JSON when zero, NEW-15), \
            orphan count, dead-end count, and recently modified files. When a `[schema]` \
            block is configured it also carries `schema` with `errors`, `warnings` and \
            `files_with_violations` \u{2014} the same key `lint` uses for that quantity.\n\
            Drill down with: hyalo find --orphan, --dead-end, --broken-links, --property status=X, \
            or --broken-links --strict to fail CI on any finding.\n\
            PROPERTIES: one entry per property NAME. A property that appears with more than one\n\
            type across the vault reads `type: \"mixed\"` with a `mixed_types` breakdown\n\
            ('published (103: 79 datetime, 24 date)' in text), so the property count is the\n\
            number of distinct names and matches `hyalo properties --count`.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need a quick overview of a vault's metadata landscape.\n\
            UNUSABLE FILES: `results.files.total` counts only the notes hyalo could read.\n\
            `results.files.skipped` counts files whose YAML frontmatter would not parse (list\n\
            them with `hyalo lint --rule HYALO005`) and `results.files.excluded` counts files\n\
            dropped by `[scan] exclude` in .hyalo.toml; each entry in\n\
            `results.files.directories` carries its own `skipped` (omitted when zero) so the\n\
            unusable files can be located. Text mode renders both inline as\n\
            'Files: 75 (28 skipped, 0 excluded)', and prints the bare 'Files: N' when there is\n\
            nothing to report. Every scanning command summarises the skips as ONE stderr line\n\
            rather than one YAML excerpt per file; -q silences it and\n\
            `[scan] verbose_skips = true` (or RUST_LOG=hyalo=debug) restores the detail.\n\
            VAULT DIR: with --format text the resolved vault dir is announced on stderr as\n\
            'note: kb dir: <path>', so stdout carries only the report; -q suppresses the note\n\
            and --format json keeps the dir in the payload as `.dir`.\n\n\
            FLAG NOTE: `-n / --recent` sizes the 'recently modified' list only. This differs\n\
            from `-n` on find and backlinks, where `-n` is `--limit` and caps the returned\n\
            result set. `summary` has no --limit; its stats always cover every scanned file.\n\n\
            EXAMPLES:\n\
            hyalo summary\n\
            hyalo summary -n 25   # 25 recent files (not a result limit)\n\
            hyalo summary --format text\n\
            hyalo summary --jq '.results.tasks.total'\n\
            hyalo summary --jq '.results.links.broken'"
    )]
    Summary {
        #[arg(
            short,
            long,
            value_name = "GLOB",
            help = GLOB_FLAG_SHORT_DOC,
            long_help = GLOB_FLAG_DOC,
        )]
        glob: Vec<String>,
        /// Number of recent files to show
        ///
        /// NOTE: on this command -n means --recent, not --limit as on find and backlinks —
        /// it caps only the "recently modified" list, never the summary's stats.
        #[arg(short = 'n', long, value_name = "N", default_value = "10")]
        recent: usize,
        /// Limit directory listing depth (0 = root only; stats are always full)
        #[arg(long)]
        depth: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// List all files that link to a given file (read-only)
    #[command(
        long_about = "List all files that link to a given file (reverse link lookup).\n\n\
            Builds an in-memory link graph by scanning all .md files in the vault, \
            then returns every file that contains a [[wikilink]] or [markdown](link) \
            pointing to the target file.\n\n\
            OUTPUT: JSON object with file, backlinks array (source, line, target, written_target,\n\
            label), and total count. `target` is the queried file's own canonical resolved path,\n\
            reported identically on every entry — not each occurrence's own written spelling,\n\
            which could differ by `.md` presence or relative-path form even though every entry\n\
            points at the same file (NEW-18). `written_target` is that per-occurrence spelling —\n\
            path resolved but casing and `.md` presence exactly as the author typed — so a case\n\
            mismatch (`[[NOTE]]` vs `[[note]]`) stays visible even though `target` is uniform\n\
            (PR #251 review L8).\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            hyalo backlinks decision-log.md\n\
            hyalo backlinks --file notes/design.md\n\
            hyalo backlinks --file notes/design.md --limit 20"
    )]
    Backlinks {
        #[command(flatten)]
        selection: InputSelection,
        /// Maximum number of backlinks to return (0 = unlimited).
        ///
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Move/rename a file and update all inbound and outbound links
    #[command(
        long_about = "Move or rename a markdown file and update all links across the vault.\n\n\
            Builds an in-memory link graph, then:\n\
            1. Moves the file on disk.\n\
            2. Rewrites all [[wikilinks]] and [markdown](links) in other files that pointed to the old path.\n\
            3. Rewrites self-referencing [[wikilinks]] inside the moved file (e.g. [[x]] in x.md → [[y]] after mv x.md y.md).\n\
            4. Rewrites relative markdown links inside the moved file whose targets changed due to the new directory context.\n\n\
            WIKILINK FORM PREFERENCE:\n\
            When the new basename is unique vault-wide (case-insensitively), rewritten [[wikilinks]] use\n\
            short-form [[stem]] (Obsidian-compatible). When the basename is ambiguous (multiple files share\n\
            the same stem), path-form [[new/path/stem]] is used for disambiguation.\n\n\
            Written form is preserved: [[./note]], [[note.md]], [[note|alias]], and [[note#section]] forms\n\
            are kept intact with updated targets after the move.\n\n\
            AMBIGUOUS BARE WIKILINKS:\n\
            A bare [[stem]] that matches multiple vault files is skipped by default (logged to stderr and\n\
            included in the 'skipped_ambiguous' JSON field). Pass --allow-ambiguous to rewrite it anyway\n\
            based on stem matching. Reported for every same-stemmed candidate, including when none of\n\
            them sits at the vault root.\n\n\
            SINGLE-FILE MODE:\n\
            Provide a positional FILE or --file. The destination is a .md path or an existing\n\
            directory (basename of source is appended), given either as --to <dest> or as a second\n\
            positional DEST (`hyalo mv old.md new.md`, requires the positional source). Applied\n\
            immediately unless --dry-run is passed; --apply is rejected here (it would be a no-op\n\
            and hide the mode asymmetry).\n\n\
            BATCH MODE (when --glob, --property, --tag, or --type is given):\n\
            Resolves a set of source files via the given selectors (intersection). --to must be a\n\
            directory (existing or trailing '/', no .md suffix). Defaults to dry-run; pass --apply\n\
            to commit changes. A single link-graph build covers all files.\n\n\
            EXAMPLES:\n\
            hyalo mv old.md --to new.md\n\
            hyalo mv old.md new.md   # positional DEST, alias for --to\n\
            hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/\n\
            hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/ --apply\n\
            hyalo mv --tag archive --to archive/ --apply\n\n\
            OUTPUT: JSON object with moves, updated_files (with per-file replacements), totals, applied flag,\n\
            and skipped_ambiguous (list of links skipped due to ambiguous stem resolution).\n\
            Text output prints `files updated: N, links updated: M` under the `Moved ...` line in both\n\
            modes, so a rewrite that matched nothing is visible rather than silent.\n\n\
            FRONTMATTER LINKS: `[[wikilinks]]` written in any frontmatter value (`categories:`,\n\
            `type:`, `related:`, a nested map) are rewritten in place — the target text inside the\n\
            existing YAML scalar is replaced, so quoting style and every other byte of the block\n\
            survive and `git diff` shows one changed target per line. A link whose `[[...]]` spans a\n\
            line break (a folded or literal block scalar) has no single-line span to replace: it is\n\
            left alone, counted in a stderr warning, and listed under `frontmatter_links_skipped`.\n\
            Single-file mode finds those links anywhere in the vault, including in files that hold\n\
            no other link to the moved target — a split link is not a backlink, so the link graph\n\
            alone would never surface it. Batch mode reports no frontmatter skips.\n\n\
            INDEX NOTE: When `--index` or `--index-file` is active, the snapshot index is patched\n\
            in-place after a successful move: the moved entry is renamed, files whose links were\n\
            rewritten are re-scanned, and the link graph (target keys + backlink sources) is\n\
            updated. Index path keys are vault-relative and use forward slashes on all platforms.\n\
            In batch mode the index is saved once at the end, not per move.\n\
            SIDE EFFECTS: Moves files and modifies files containing links (unless dry-run)."
    )]
    Mv {
        /// Source file to move (relative to --dir) — positional form (single-file mode only)
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "properties", "tag", "type", "file", "files_from"])]
        file_positional: Option<String>,
        /// Source file to move (relative to --dir) — flag form (single-file mode only)
        #[arg(short, long, value_name = "FILE", conflicts_with_all = ["file_positional", "glob", "properties", "tag", "type", "files_from"])]
        file: Option<String>,
        /// Destination path — positional form of --to (single-file mode only)
        ///
        /// `hyalo mv old.md new.md` is equivalent to `hyalo mv old.md --to new.md`.
        /// Requires the positional source FILE (not --file); mutually exclusive with --to.
        #[arg(
            value_name = "DEST",
            requires = "file_positional",
            conflicts_with = "to"
        )]
        to_positional: Option<String>,
        /// Destination: a .md path or an existing directory in single-file mode; a directory in batch mode
        ///
        /// In single-file mode a directory destination appends the source basename.
        #[arg(long, value_name = "DEST")]
        to: Option<String>,
        #[arg(
            short,
            long,
            value_name = "GLOB",
            conflicts_with = "files_from",
            help = GLOB_FLAG_SHORT_DOC,
            long_help = GLOB_FLAG_DOC,
        )]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin (batch mode).
        ///
        /// Mutually exclusive with --file, positional FILE, and --glob.
        /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
        /// Input is deduplicated; results follow first-seen order.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Property filter for source selection. Same syntax as `find --property`; repeatable (AND)
        ///
        /// The full operator set: K=V, K!=V, K>V, K>=V, K<V, K<=V, K (exists), !K (absent),
        /// K~=re and K~=/re/i (regex), K=null / K!=null and K=[] / K!=[], and K may be a
        /// dot-path into a nested map — exactly what `find --property` accepts, parsed by the
        /// same code, including its rejection of `=~` and of an empty regex.
        #[arg(short, long = "property", value_name = "FILTER")]
        properties: Vec<String>,
        /// Tag filter: exact or prefix match. Repeatable (AND)
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        /// Type filter: match files where frontmatter 'type' equals TYPE. Repeatable (AND)
        #[arg(long = "type", value_name = "TYPE")]
        r#type: Vec<String>,
        /// Preview changes without writing any files (the default in batch mode; --apply writes)
        #[arg(long)]
        dry_run: bool,
        /// Commit changes in batch mode (required when using --glob/--property/--tag/--type).
        ///
        /// Rejected in single-file mode, which applies by default — use --dry-run to preview.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// How to handle destination basename collisions: 'error' (default) or 'skip'
        #[arg(long = "on-conflict", value_name = "POLICY", default_value = "error")]
        on_conflict: String,
        /// Allow rewriting bare wikilinks ([[note]]) even when the stem is ambiguous
        ///
        /// (matches multiple vault files). By default, ambiguous bare wikilinks are
        /// skipped with a warning to avoid silent retargeting (BUG-2 prevention).
        #[arg(long)]
        allow_ambiguous: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Set (create or overwrite) frontmatter properties and/or add tags across file(s)
    #[command(
        long_about = "Set (create or overwrite) frontmatter properties and/or add tags across file(s).\n\n\
            INPUT: One or more --property K=V arguments and/or --tag T arguments, with FILE (positional or --file) or --glob.\n\
            BEHAVIOR:\n\
            - --property K=V: creates or overwrites the property. Type is auto-inferred from V \
              (number, bool, text). Use K=[a,b,c] to create a YAML list; values are comma-split and trimmed. \
              A file is skipped if the stored value is already identical.\n\
            GUARD: --property accepts only plain K=V assignments. Filter syntax (>=, <=, !=, ~=) \
            is rejected — use --where-property for filtering.\n\
            - --tag T: idempotent tag add. Creates the 'tags' list if absent. Skips files that already have the tag.\n\
            OUTPUT: A single result object if one mutation was requested; an array if multiple.\n\
            Each result: {\"property\": K, \"value\": V, \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            or:          {\"tag\": T, \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            `value` echoes the coerced value actually written (e.g. a JSON list for K=[a,b,c], \
            a number for K=3), not the raw input string.\n\
            ADVISORY: an optional \"note\" field is added when the value would violate an \
            enum/pattern constraint in the effective schema, or when a date-typed property (date, \
            created, ...) gets a non-date value (it will sort lexicographically). The write still \
            proceeds — lint (or --validate) remains the enforcement gate.\n\
            LIST -> SCALAR: `set` means replace, so assigning a scalar to a property that holds a \
            YAML list writes the scalar and changes the property's type. The files where that \
            happened are listed under \"list_collapsed\" and named in a stderr note pointing at \
            `hyalo append`, which adds to a list instead of replacing it. With --validate (or \
            validate_on_write), a schema declaring the property as `list` rejects the scalar \
            before anything is written.\n\
            UNUSABLE SCHEMA: when [schema] is present but could not be loaded (an uncompilable \
            regex, a key on the wrong property type), --validate and validate_on_write REFUSE \
            with exit 1 and write nothing — the schema fell back to empty, so validating \
            against it would reject nothing. Fix [schema] (hyalo lint reports it as \
            schema/malformed) or drop --validate to write unvalidated.\n\
            FILTERS (optional, narrow which files are mutated):\n\
            - --where-property FILTER: only mutate files whose frontmatter matches (same syntax as find --property: \
K=V, K!=V, K>=V, K<=V, K>V, K<V, K for existence, K~=/re/ for a regex, K=null / K=[] for a null or \
empty-list value). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            FORMATTING: only the lines of the keys you change are rewritten. Every other \
            frontmatter line — quote style, block scalars, flow collections, indentation, \
            blank lines and comments — is preserved byte for byte. A block that cannot be \
            mapped to per-key line spans (explicit `? key` syntax, top-level flow collections, \
            invalid UTF-8, mixed line endings) is rewritten in full, with a warning on stderr \
            naming the file and the reason.\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error \
            {\"error\": \"frontmatter would exceed size budget\", \"limit_bytes\": ..., \"would_be_bytes\": ..., \"file\": ...}.\n\
            USE WHEN: You need to create or overwrite frontmatter properties or add tags, \
            possibly across many files at once.\n\n\
            EXAMPLES:\n\
            hyalo set --property status=completed --file notes/todo.md\n\
            hyalo set --property priority=3 --property reviewed=true --file notes/todo.md\n\
            hyalo set --property 'tags=[a,b,c]' --file notes/todo.md\n\
            hyalo set --tag reviewed --glob 'research/**/*.md'\n\
            hyalo set --property status=in-progress --where-property status=draft --glob '**/*.md'\n\
            hyalo set --property due=2026-12-31 --validate --file notes/todo.md"
    )]
    Set {
        /// Target file(s) as positional argument(s) — alternative to --file
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file", "files_from"])]
        file_positional: Vec<String>,
        /// Property to set: K=V (type inferred from V). Repeatable
        #[arg(short, long = "property", value_name = "K=V")]
        properties: Vec<String>,
        /// Tag to add (idempotent). Repeatable
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        #[arg(
            short,
            long,
            conflicts_with_all = ["glob", "files_from"],
            help = FILE_FLAG_SHORT_DOC,
            long_help = FILE_FLAG_DOC,
        )]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        ///
        /// Mutually exclusive with --file, positional FILE, and --glob.
        /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
        /// Input is deduplicated; results follow first-seen order.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        /// Reject writes that would create lint errors under the .hyalo.toml schema
        ///
        /// Validates the new values against the schema before writing. Implied by
        /// `validate_on_write = true` in the [schema] config.
        ///
        /// Refuses with exit 1 (writing nothing) when `[schema]` exists but could
        /// not be loaded: the schema falls back to empty, so validating against
        /// it would reject nothing.
        #[arg(long, alias = "strict")]
        validate: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Remove frontmatter properties and/or tags from file(s)
    #[command(
        long_about = "Remove frontmatter properties and/or tags from file(s).\n\n\
            INPUT: One or more --property K or K=V arguments and/or --tag T arguments, with FILE (positional or --file) or --glob.\n\
            BEHAVIOR:\n\
            - --property K: removes the entire key from frontmatter. Skips files where it is absent.\n\
            - --property K=V: if the property is a list, removes V from the list; if it is a scalar \
              that matches V (case-insensitive), removes the key entirely; otherwise skips the file.\n\
            GUARD: --property accepts only plain K or K=V arguments. Filter syntax (>=, <=, !=, ~=) \
            is rejected — use --where-property for filtering.\n\
            - --tag T: removes the tag from the 'tags' list. Skips files where the tag is not present.\n\
            OUTPUT: A single result object if one mutation was requested; an array if multiple.\n\
            Each result: {\"property\": K, [\"value\": V,] \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            or:          {\"tag\": T, \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            FILTERS (optional, narrow which files are mutated):\n\
            - --where-property FILTER: only mutate files whose frontmatter matches (same syntax as find --property: \
K=V, K!=V, K>=V, K<=V, K>V, K<V, K for existence, K~=/re/ for a regex, K=null / K=[] for a null or \
empty-list value). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            FORMATTING: only the lines of the keys you change are rewritten. Every other \
            frontmatter line — quote style, block scalars, flow collections, indentation, \
            blank lines and comments — is preserved byte for byte. A block that cannot be \
            mapped to per-key line spans (explicit `? key` syntax, top-level flow collections, \
            invalid UTF-8, mixed line endings) is rewritten in full, with a warning on stderr \
            naming the file and the reason.\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error (see `hyalo set --help`).\n\
            USE WHEN: You need to delete properties or remove tags from one or more files.\n\n\
            EXAMPLES:\n\
            hyalo remove --property status --file notes/todo.md\n\
            hyalo remove --property aliases=old-name --file notes/todo.md\n\
            hyalo remove --tag draft --glob '**/*.md'\n\
            hyalo remove --property status --where-tag archive --glob '**/*.md'"
    )]
    Remove {
        /// Target file(s) as positional argument(s) — alternative to --file
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file", "files_from"])]
        file_positional: Vec<String>,
        /// Property to remove: K (removes key) or K=V (removes value from list/scalar). Repeatable
        #[arg(short, long = "property", value_name = "K or K=V")]
        properties: Vec<String>,
        /// Tag to remove. Repeatable
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        #[arg(
            short,
            long,
            conflicts_with_all = ["glob", "files_from"],
            help = FILE_FLAG_SHORT_DOC,
            long_help = FILE_FLAG_DOC,
        )]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        ///
        /// Mutually exclusive with --file, positional FILE, and --glob.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Initialize hyalo configuration and optional tool integrations
    #[command(
        long_about = "Create .hyalo.toml and optionally set up Claude Code and pi integrations.\n\n\
            Without flags, creates a .hyalo.toml config file.\n\
            With --claude, also installs the hyalo skill for Claude Code.\n\
            With --pi, also installs the hyalo skill for pi.\n\
            With --profile <name>, scaffolds a preset vault flavour by merging an\n\
            embedded config fragment into .hyalo.toml (available: okf, madr, skills,\n\
            changelog). Multiple profiles compose in one vault: array keys (exempt,\n\
            binds, [lint] profiles) union instead of clobbering, comments survive, and\n\
            re-running is idempotent. A changed scalar prints a `conflict:` line to\n\
            stderr — nothing is lost silently.\n\
            With --profile <name> --claude, also installs the bundled skill for it.\n\n\
            Use the global --dir flag to name the markdown directory. A vault at or below the\n\
            current directory keeps .hyalo.toml here and records `dir` relative to it; a vault\n\
            outside it (an absolute path elsewhere, ../sibling) makes that tree its own project\n\
            root, writing .hyalo.toml there with dir = \".\" — so the config written is always\n\
            one hyalo can read back (a project-local .hyalo.toml may not set an absolute dir).\n\n\
            The summary prints as text even when piped; pass --format json (or --jq) for a\n\
            machine-readable envelope of what was written.\n\n\
            EXAMPLES:\n\
            hyalo init\n\
            hyalo --dir kb init\n\
            hyalo --dir /elsewhere/vault init\n\
            hyalo init --dir kb --format json\n\
            hyalo --dir kb init --claude\n\
            hyalo --dir kb init --pi\n\
            hyalo init --profile okf\n\
            hyalo init --profile okf --claude\n\
            hyalo init --profile madr\n\
            hyalo init --profile skills\n\
            hyalo init --profile changelog"
    )]
    Init {
        /// Set up Claude Code integration (skill + CLAUDE.md hint)
        #[arg(long)]
        claude: bool,
        /// Set up pi integration (skill + extension)
        #[arg(long)]
        pi: bool,
        /// Scaffold a preset vault flavour: okf, madr, skills, or changelog
        ///
        /// Merges the profile's embedded config fragment into .hyalo.toml.
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
    },
    /// Remove hyalo configuration and tool integration artifacts
    #[command(
        long_about = "Remove .hyalo.toml and all Claude Code / pi integration artifacts created by `init`.\n\n\
            Removes skills, rules, and the managed section from .claude/CLAUDE.md, and .pi/ directory.\n\
            Safe to run when artifacts are already absent (idempotent).\n\n\
            The global --dir flag selects the tree to clean, exactly as it does for `init`: a\n\
            vault inside the current directory still cleans the current project, while --dir\n\
            naming a tree outside it cleans that tree instead (and the summary leads with a\n\
            `target` line).\n\n\
            The summary prints as text even when piped; pass --format json (or --jq) for a\n\
            machine-readable envelope of what was removed.\n\n\
            EXAMPLES:\n\
            hyalo deinit\n\
            hyalo --dir /elsewhere/vault deinit\n\
            hyalo deinit --format json"
    )]
    Deinit,
    /// Build a snapshot index for faster repeated read-only queries
    #[command(
        name = "create-index",
        long_about = "Scan the vault and write a binary snapshot index to disk.\n\n\
            The index captures a point-in-time snapshot of all vault metadata.\n\
            Delete it after use via `hyalo drop-index`.\n\n\
            The index file can be passed to any supported command via `--index-file <PATH>`.\n\
            Read-only commands skip the disk scan entirely. Mutation commands\n\
            (set, remove, append, task, mv, tags rename, properties rename) still\n\
            read/write files on disk but also patch the index in-place after each\n\
            mutation — keeping it current for subsequent queries. This is safe as\n\
            long as no external tool modifies vault files while the index is active.\n\
            `set`, `remove` and `append` are the exception: they rescan any file they\n\
            read whose (mtime, size) has drifted from the snapshot, even when the\n\
            mutation writes nothing, so an externally edited file they touch is\n\
            repaired rather than trusted.\n\n\
            SNAPSHOT CONTRACT: the index is a point-in-time copy, not a live view.\n\
            Edits made while it exists — by hand, by another tool, or by hyalo run\n\
            without an index flag — are invisible to indexed queries, which still\n\
            exit 0. Commands that load an index cheaply compare directory mtimes\n\
            (the vault root and every directory up to 3 levels below it; deeper\n\
            levels are skipped because a full walk of a 14k-file vault costs more\n\
            than the indexed query itself) against the snapshot's creation time\n\
            and warn `index older than vault` when one postdates it. Two blind\n\
            spots: in-place edits of existing notes (an edit that changes a file's\n\
            content without adding/removing/renaming any file leaves every\n\
            directory's mtime untouched), and files added or removed inside a\n\
            directory more than 3 levels below the root. Re-run create-index\n\
            whenever the vault may have changed.\n\
            The warning does not stop the run: stale results are still served and\n\
            still exit 0. Re-run `create-index`, or omit `--index` to force a\n\
            disk scan instead.\n\n\
            PERFORMANCE: a body-text query combined with a narrow metadata filter\n\
            (e.g. `find \"query\" --property status=x`) still reads the whole vault\n\
            without an index, because BM25 relevance is ranked against full-vault\n\
            statistics. On large vaults, create an index for this workload.\n\n\
            OUTPUT: JSON object with `path`, `files_indexed`, and `warnings`.\n\
            SIDE EFFECTS: Writes a binary file (default: .hyalo-index in --dir).\n\n\
            FLAG ALIASES: on this subcommand, `--index-file PATH` (the global flag) is\n\
            accepted as a synonym for `-o / --output PATH`. If both are provided and\n\
            differ, create-index returns an error.\n\n\
            VAULT BOUNDARY: an explicit output path must land inside --dir unless\n\
            --allow-outside-vault is passed; without it the write is refused with exit 1.\n\
            The path itself is resolved against the current working directory, while the\n\
            boundary is --dir — from a repo root configured with dir = \"kb\", an in-vault\n\
            output path is `-o kb/index.bin`, not `-o index.bin`.\n\
            The boundary applies to the *writers* (create-index, drop-index) only —\n\
            reading a snapshot with `--index-file` is unrestricted.\n\n\
            READ-ONLY CORPORA: to index a vault you cannot (or would rather not) write\n\
            into, keep the snapshot elsewhere and name it on every query:\n\
            hyalo create-index --dir /corpus -o ~/.cache/corpus.idx --allow-outside-vault\n\
            hyalo find \"query\" --dir /corpus --index-file ~/.cache/corpus.idx\n\
            The snapshot records the vault it was built for, so a query pointed at a\n\
            different --dir refuses the index and falls back to a disk scan.\n\n\
            EXAMPLES:\n\
            hyalo create-index\n\
            hyalo create-index -o .hyalo-index-draft   # in-vault when dir = \".\"\n\
            hyalo create-index -o /tmp/my-index --allow-outside-vault\n\
            hyalo find --property status=draft --index"
    )]
    CreateIndex {
        /// Output path for the index file (default: .hyalo-index in --dir).
        ///
        /// Equivalent to the global --index-file flag on this subcommand.
        /// Also accepted as --path, matching `drop-index --path`.
        #[arg(short, long, value_name = "PATH", visible_alias = "path")]
        output: Option<PathBuf>,
        /// Allow writing the index file outside the vault directory
        #[arg(long)]
        allow_outside_vault: bool,
    },
    /// Delete a snapshot index file created with create-index
    #[command(
        name = "drop-index",
        long_about = "Delete a snapshot index file.\n\n\
            Drop the index when your session is complete. The index should\n\
            not outlive its session.\n\n\
            If --path is omitted, deletes .hyalo-index in --dir.\n\n\
            OUTPUT: JSON object with `deleted` path.\n\
            SIDE EFFECTS: Deletes the index file from disk."
    )]
    DropIndex {
        /// Path to the index file to delete (default: .hyalo-index in --dir).
        ///
        /// Also accepted as --output, matching `create-index --output`.
        #[arg(short, long, visible_alias = "output")]
        path: Option<PathBuf>,
        /// Allow deleting an index file outside the vault directory
        #[arg(long)]
        allow_outside_vault: bool,
    },
    /// Append values to list properties in file(s) frontmatter, promoting scalars to lists
    #[command(
        long_about = "Append values to list properties in file(s) frontmatter.\n\n\
            INPUT: One or more --property K=V arguments, with FILE (positional or --file) or --glob.\n\
            Note: --tag is not available on append (tags are atomic, not lists). Use 'hyalo set --tag T' to add tags.\n\
            BEHAVIOR:\n\
            - Property absent or null: creates it as a single-element list [V].\n\
            - Property is a list: appends V if not already present (case-insensitive duplicate check).\n\
            - Property is a scalar (string, number, bool): promotes to [existing, V].\n\
            - Property is a mapping: returns an error.\n\
            GUARD: --property accepts only plain K=V assignments. Filter syntax (>=, <=, !=, ~=) \
            is rejected — use --where-property for filtering.\n\
            OUTPUT: A single result object if one mutation was requested; an array if multiple.\n\
            Each result: {\"property\": K, \"value\": V, \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            FILTERS (optional, narrow which files are mutated):\n\
            - --where-property FILTER: only mutate files whose frontmatter matches (same syntax as find --property: \
K=V, K!=V, K>=V, K<=V, K>V, K<V, K for existence, K~=/re/ for a regex, K=null / K=[] for a null or \
empty-list value). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            FORMATTING: only the lines of the keys you change are rewritten. Every other \
            frontmatter line — quote style, block scalars, flow collections, indentation, \
            blank lines and comments — is preserved byte for byte. A block that cannot be \
            mapped to per-key line spans (explicit `? key` syntax, top-level flow collections, \
            invalid UTF-8, mixed line endings) is rewritten in full, with a warning on stderr \
            naming the file and the reason.\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error (see `hyalo set --help`).\n\
            UNUSABLE SCHEMA: when [schema] is present but could not be loaded (an uncompilable \
            regex, a key on the wrong property type), --validate and validate_on_write REFUSE \
            with exit 1 and write nothing — the schema fell back to empty, so validating \
            against it would reject nothing. Fix [schema] (hyalo lint reports it as \
            schema/malformed) or drop --validate to write unvalidated.\n\
            USE WHEN: You need to append items to list-type properties such as 'aliases' or 'authors' \
            without overwriting the existing list.\n\n\
            EXAMPLES:\n\
            hyalo append --property aliases='My Note' --file note.md\n\
            hyalo append --property authors=alice --glob 'research/**/*.md'\n\
            hyalo append --property related=other.md --where-tag project --glob '**/*.md'"
    )]
    Append {
        /// Target file(s) as positional argument(s) — alternative to --file
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file", "files_from"])]
        file_positional: Vec<String>,
        /// Property to append to: K=V. Repeatable
        #[arg(short, long = "property", value_name = "K=V", required = true)]
        properties: Vec<String>,
        #[arg(
            short,
            long,
            conflicts_with_all = ["glob", "files_from"],
            help = FILE_FLAG_SHORT_DOC,
            long_help = FILE_FLAG_DOC,
        )]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        ///
        /// Mutually exclusive with --file, positional FILE, and --glob.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        /// Reject writes that would create lint errors under the .hyalo.toml schema
        ///
        /// Validates the new values against the schema before writing. Implied by
        /// `validate_on_write = true` in the [schema] config.
        ///
        /// Refuses with exit 1 (writing nothing) when `[schema]` exists but could
        /// not be loaded: the schema falls back to empty, so validating against
        /// it would reject nothing.
        #[arg(long, alias = "strict")]
        validate: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Manage saved views (named find filter sets stored in .hyalo.toml)
    #[command(
        long_about = "Manage saved views — named find queries stored in .hyalo.toml.\n\n\
            Views let you save frequently used filter combinations under a name\n\
            and recall them with `hyalo find --view <name>`. CLI flags passed alongside\n\
            --view are merged on top — list filters extend, scalars override.\n\n\
            Calling `hyalo views` without a subcommand defaults to `hyalo views list`.\n\n\
            Subcommands:\n\
            - list: Show all saved views and their filters (default).\n\
            - set: Create or update a view.\n\
            - remove: Delete a view.\n\n\
            FIELDS: a view may pin --fields, and running it then uses that shape instead of the \
            compact default (file, modified, size, lines, title, properties, tags). Pin the fields \
            a query is actually about -- `views set orphans --orphan --fields backlinks` -- so the \
            saved query stays cheap; a view with no --fields gets the default shape, plus whatever \
            its own filters imply.\n\n\
            SIDE EFFECTS: 'set' and 'remove' modify .hyalo.toml. 'list' is read-only.\n\n\
            EXAMPLES:\n\
            hyalo views list\n\
            hyalo views set drafts --property status=draft --tag project\n\
            hyalo views set audit --broken-links --fields links\n\
            hyalo find --view drafts --limit 5\n\
            hyalo views remove drafts"
    )]
    Views {
        #[command(subcommand)]
        action: Option<ViewsAction>,
    },
    /// Detect and repair broken links across the vault
    #[command(
        long_about = "Detect and repair broken wikilinks and markdown links.\n\n\
            Scans the vault for links that cannot be resolved to an existing file, \
            then uses fuzzy matching (case-insensitive, extension mismatch, shortest-path, \
            Jaro-Winkler) to find the best candidate replacement.\n\n\
            WIKILINK RESOLUTION:\n\
            Wikilinks accept an optional .md suffix — [[foo.md]], [[foo.md#heading]], and [[foo.md|alias]]\n\
            are treated identically to [[foo]], [[foo#heading]], and [[foo|alias]] respectively.\n\
            This matches Obsidian's behavior when copy-pasting note names that include the extension.\n\n\
            Default behavior (no subcommand): dry-run of `links fix` — shows what would be\n\
            repaired without modifying files. Equivalent to `hyalo links fix --dry-run`.\n\n\
            OUTPUT: JSON object with broken/fixable/fuzzy/unfixable counts, per-fix details \
            (source, line, old_target, new_target, strategy, confidence) under fixes, \
            fuzzy_fixes, case_mismatch_fixes and relocation_fixes — populated in dry-run too, \
            so a proposal can be audited before anything is written — and the list of links \
            that could not be matched. With --apply it also \
            reports applied_fixes (fixes actually written to disk) plus \
            unapplied/unapplied_fixes for plans whose on-disk text no longer \
            matched — only applied_fixes were durably written.\n\
            BUCKETS: every broken link lands in exactly one of fixable (plain --apply writes it), \
            fuzzy (low-confidence guess, needs --apply-fuzzy), unfixable (no candidate at all) or \
            templated, so those four counts add up to broken. case_mismatches, relocations, \
            ambiguous and out_of_vault are counted separately — those links are not broken. \
            relocations is a bare-stem link (no directory in the written target) whose stem \
            resolved to a file in a different directory — a move, not a casing fix, so it is \
            reported apart from case_mismatches (both are written by plain --apply).\n\
            ANCHORS: broken_anchors (and its one-line note in --format text output) is populated only when \
            broken is 0 — a link whose target resolves but whose #fragment names no heading is \
            not a broken *link* in this command's sense, and the count only runs the extra check \
            when targets are otherwise clean. `find --broken-links --strict` is the CI gate for \
            anchors; this command does not fix them (NEW-15 / UX-2).\n\
            CONFIDENCE FLOOR: fuzzy_min_confidence reports the floor in force (0.8 unless \
            --min-confidence or `[links] fuzzy_min_confidence` moves it) and fuzzy_below_floor \
            counts the proposals it suppresses — those have a candidate but are never written, \
            so `fuzzy - fuzzy_below_floor` is what --apply-fuzzy would apply. Each entry in \
            fuzzy_fixes also carries rule (kebab-case strategy), below_floor, and — when the \
            on-disk line still matches — col: the 1-based byte column of old_target on that line \
            (NEW-18), omitted when the file/line is unreadable or the target no longer appears \
            there (a stale proposal against text that already changed).\n\
            TEXT LAYOUT: the counts come first, then the fixes that would be (or were) written, \
            then the actionable buckets (unfixable, out-of-vault, case mismatches, ambiguous, \
            templated) capped at 20 entries each, and finally the fuzzy proposals — the longest \
            section. Use --format json for uncapped lists.\n\
            OUT OF VAULT: a target that normalizes above the vault root \
            (../../CONTRIBUTING.md) can never resolve to a scanned file, so it is \
            counted under out_of_vault / out_of_vault_links instead of broken and is \
            never offered a fix. Site-absolute targets (/src/foo.md) stay in broken.\n\
            TEMPLATED: a destination containing {% , {{ or ${ is a template expression, not a \
            path ({% ifversion ghes %}/admin{% endif %}/guides). hyalo cannot know what it \
            renders to, so it is counted under templated / templated_links and never rewritten \
            — a fuzzy match on the literal text would silently drop the conditional.\n\
            DRY RUN: `dry_run` is true on a preview and false under --apply, on both \
            `links fix` and `links auto`, so one key tells preview from apply.\n\
            SIDE EFFECTS: None unless `links fix --apply` is passed.\n\n\
            TIP: For read-only auditing, use 'hyalo summary' (link health overview)\n\
            or 'hyalo find --broken-links' (list files with unresolved links).\n\n\
            EXAMPLES:\n\
            hyalo links fix\n\
            hyalo links fix --apply\n\
            hyalo links auto --first-only --apply\n\
            hyalo links auto --min-length 5 --exclude-target-glob 'templates/*' --apply"
    )]
    Links {
        #[command(subcommand)]
        action: Option<LinksAction>,
    },
    /// Validate frontmatter (schema) and markdown body (mdbook-lint + HYALO native rules)
    #[command(
        long_about = "Validate frontmatter properties against the `.hyalo.toml` schema and lint the\n\
            markdown body against bundled rules (mdbook-lint MD001..MD059 + HYALO native rules).\n\n\
            FRONTMATTER PASS: schema violations from `[schema.default]` / `[schema.types.*]`.\n\
            - error: missing required property, wrong type, invalid enum value, pattern mismatch,\n\
                      `item_pattern` violation on `string-list` items, missing `required-sections`,\n\
                      `object-list` shape violation (item is not a map, missing or unknown key,\n\
                      `key-patterns` mismatch),\n\
                      empty value on a required property (see REQUIRED EMPTINESS below)\n\
            - warn:  no 'type' property, property not declared in schema\n\
            When no `[schema]` section exists, this pass exits 0 with zero violations.\n\
            Schema extensions `item_pattern` (per-item regex on `string-list` properties),\n\
            `object-list` (`required-keys` / `allowed-keys` / `key-patterns` on a list of maps)\n\
            and `required-sections` (required body outline on type schemas) are validated here.\n\
            Constraint violations report `autofixable: false`: `--fix` has no fixer for a\n\
            `pattern`, `item_pattern` or `object-list` mismatch.\n\n\
            EXEMPT FILES: `[schema] exempt = [\"**/index.md\", \"**/log.md\"]` lists vault-relative\n\
            globs for reserved files that are bound to no schema — they skip the missing-`type`\n\
            warning, required-property checks, and undeclared-property warnings (useful for OKF\n\
            bundle reserved files). Matching is cross-platform (forward-slash normalized) and\n\
            honors the resolved `[links] case_insensitive` mode: on a case-insensitive filesystem\n\
            (macOS/Windows default, or `case_insensitive = \"true\"`), `INDEX.md` is exempted by\n\
            `**/index.md` the same way `hyalo okf index` treats it as the reserved index file.\n\n\
            REQUIRED EMPTINESS: a required property whose value is YAML null (`tags: ~`) or\n\
            an empty array (`tags: []`) is treated as semantically equivalent to absent and\n\
            reported as an error (e.g. `required property \"tags\" must not be empty`). The\n\
            rule fires regardless of declared constraint type — vacuous values convey no\n\
            information for a required field. Atomic-typed required properties (`string`,\n\
            `date`, `number`, ...) are unaffected: an empty string or zero still satisfies\n\
            them (checking those is a separate constraint not done here). To require tags on\n\
            a document type, list `tags` in that type's `required` array; no separate\n\
            `min_items` knob needed.\n\n\
            BODY PASS: ~14 default-on stock rules from mdbook-lint plus the HYALO native\n\
            cross-cutting rules:\n\
              - HYALO001: bare `[]` should be `- [ ]` (autofixable)\n\
              - HYALO002: `status: completed` requires all task checkboxes ticked\n\
                         (only fires when the schema declares `status` as an enum\n\
                         containing `completed`)\n\
              - HYALO007: frontmatter `title` is a list or a map, so it cannot be promoted\n\
                         to the `find --fields title` value (usually a quoting typo such as\n\
                         `title: [Draft] Notes`) \u{2014} the item falls back to its first H1\n\
              - HYALO005: frontmatter that cannot be parsed (invalid YAML, duplicate keys,\n\
                         oversized scalar) — error by default; the file still counts in\n\
                         `files_checked` so a corrupt file can never leave a green lint.\n\
                         Severity is configurable via `[lint.rules.HYALO005]` but no\n\
                         profile downgrades it.\n\
            Severity is hyalo-controlled. Manage rule enable/severity with `hyalo lint-rules`.\n\
            Override defaults via `[lint]` and `[lint.rules]` in `.hyalo.toml`.\n\n\
            OBSIDIAN GRAMMAR: four stock rules are narrowed so `--fix` cannot corrupt a vault\n\
            (`hyalo lint-rules show <ID>` spells each one out):\n\
              - MD018 exempts tag lines — a single `#` plus a tag token (letters, digits,\n\
                       `_`, `-`, `/`, non-ASCII word chars, at least one non-digit) is\n\
                       `#todo`, not a heading missing its space. `##Heading`, `#1` and a\n\
                       capitalized word followed by prose (`#Heading typo`) still fire.\n\
              - MD034 ignores URLs already inside link markup (a markdown link or image\n\
                       destination, an autolink, a wikilink, a reference definition).\n\
              - MD042 accepts an image as link text (`[![](img.png)](https://…)`).\n\
              - MD001 reports skipped heading levels but never autofixes them: renumbering\n\
                       a deliberate `######` caption rewrites authored structure. Turn the\n\
                       warning off with `hyalo lint-rules set MD001 --enabled false`.\n\n\
            INPUT: Optional FILE (positional or --file) or --glob to narrow scope.\n\
            Without any file arguments, the entire vault is linted.\n\n\
            OUTPUT: Text by default — summary mode groups violations by `(file, rule)` and caps\n\
            output at 3 violations per rule and 50 files (configurable via `[lint]` and\n\
            `--max-per-rule`). Files are listed error-first (then by violation count),\n\
            so a display cap can never hide the run's errors behind warnings-only\n\
            files; when errors are still truncated away, the show-all hint names how\n\
            many. Use --detailed for full per-violation output. Use --format json\n\
            for a JSON payload with `rule_groups`, `violations`, `rules_fired`,\n\
            `files_with_violations`, `files_truncated`, and `files_ignored` (files dropped by\n\
            `[lint] ignore`, appended to the text summary line as \"(N ignored by [lint]\n\
            ignore)\" so a bare sweep never reads as a clean bill of health for files it never\n\
            looked at — UX-1). EVERY counter in that payload —\n\
            `violations`, `rules_fired`, `errors`, `warnings`, `files_with_violations`,\n\
            `files_checked` — and the exit code describe the WHOLE vault, never just the\n\
            displayed slice: a file cap can never mask an error, and `violations`\n\
            reconciles against `errors + warnings`. The run-level finding count is named\n\
            `violations`, not `total`, because the envelope's own `total` on the same\n\
            payload is the count of files with violations. `files_truncated` is about the displayed `files[]`\n\
            list — true only when there were more violating files than the cap, not merely\n\
            when the vault is bigger than it.\n\
            LIMIT: --limit/-n N caps the displayed files[]; `--limit 0` means UNLIMITED (lift\n\
            the cap entirely, matching `--count --limit 0`) — it never empties the list.\n\n\
            SKIP VISIBILITY: with `--files-from`, dropped input paths (missing / non-markdown)\n\
            are reported as a `note:` line (--format text, on stderr) or a `::notice::` (--format\n\
            github), matching the `files_missing`/`files_skipped_*` counters in the JSON envelope.\n\
            NAMED FILES vs [lint] ignore (DEC-284, iteration 267): a path named explicitly —\n\
            positionally, with `--file`, or through `--files-from` — is linted even when\n\
            `[lint] ignore` matches it. Naming a file is a stronger signal than a glob written\n\
            once in .hyalo.toml, and the previous behaviour (drop it, then warn that it was\n\
            dropped) left no way to lint an ignored file at all. `--glob` and the bare vault\n\
            sweep keep honouring the ignore list, and a `--glob` whose matches are ENTIRELY\n\
            ignored still says so. CI implication: `git diff --name-only | hyalo lint\n\
            --files-from -` now lints changed files that the ignore list covers — the right\n\
            behaviour for a diff gate. To have the ignore list applied to a set of paths, select\n\
            them with `--glob` instead of naming them.\n\n\
            GITHUB ANNOTATIONS: --format github (lint-only) emits one GitHub Actions workflow\n\
            command per violation — `::error file=<path>,line=<line>,title=<RULE_ID>::<message>`\n\
            (warnings use `::warning`) — so findings render as inline annotations on the PR diff,\n\
            followed by a one-line `N errors, M warnings in K of T files checked` summary, whose\n\
            denominator is the same `files_checked` count --format text prints. Annotations are\n\
            never truncated (the display caps are lifted for github so every finding lands on\n\
            the PR). Under `--fix --dry-run`, would-be-fixed violations are emitted as `::notice`\n\
            with a `[fixable]` title prefix and the summary becomes `N fixable, M remaining`, so\n\
            a dry-run preview reads distinctly from a plain lint run. Message data is\n\
            escaped per the workflow-command spec. Paths are emitted RELATIVE TO THE REPO ROOT:\n\
            vault-relative paths are prefixed with the vault dir's path relative to the current\n\
            directory, so CI must run `hyalo lint` from the repository root for annotations to\n\
            resolve. Composes with --strict, --rule/--rule-prefix, --max-per-rule, and\n\
            `[lint] ignore`; exit codes are unchanged. Other subcommands reject `--format github`.\n\n\
            FILTER FLAGS:\n\
              --rule <ID>             restrict to a single rule\n\
              --rule-prefix <PREFIX>  restrict to rules with this prefix (e.g. HYALO)\n\
              --max-per-rule <N>      override per-rule cap (0 = unlimited)\n\
            --rule and --rule-prefix are both case-insensitive and both validated: an id or a\n\
            prefix that selects no rule is a user error at exit 1, never a silent full-vault\n\
            lint that reads as green. `hyalo lint-rules list` shows what exists.\n\n\
            CONFORMANCE PROFILES: --profile <NAME> overlays an embedded ruleset for this\n\
            invocation without touching `.hyalo.toml` — useful for CI or third-party bundles.\n\
            `--profile okf` encodes the Open Knowledge Format §9 conformance rules: it requires\n\
            a parseable frontmatter block with a non-empty `type` on every non-reserved `.md`\n\
            (error), and warns — never rejects — on reserved-file (`index.md`/`log.md`) structure,\n\
            broken cross-links, missing/malformed `# Citations`, and augmentation regressions.\n\
            The overlay reuses the same fragment `hyalo init --profile okf` materializes, so on a\n\
            vault already initialized that way plain `hyalo lint` behaves identically. When the\n\
            vault's `.hyalo.toml` already activates profiles via `[lint] profiles`, a `--profile`\n\
            flag *composes* with them (adds, never replaces) and honors user `[schema] exempt`\n\
            additions exactly like file activation. Unknown `type` values and extra frontmatter\n\
            keys are always accepted (permissive model).\n\
            `--profile madr` binds an `adr` schema to `docs/decisions/**` and warns on dangling\n\
            supersedes / duplicate ADR numbers. `--profile skills` binds a `skill` schema to\n\
            `**/SKILL.md` (Agent Skills spec): it errors on `name` regex/length/reserved-word\n\
            violations and out-of-bounds `description` length, and warns on a name↔directory\n\
            mismatch and a >500-line body.\n\
            `--profile changelog` binds a frontmatter-less `changelog` type to `CHANGELOG.md`\n\
            (Keep a Changelog 1.1.0): it errors on a missing `# Changelog` title, malformed\n\
            version/category headings, and out-of-order versions/dates, and warns on empty\n\
            sections and mismatched footer link references.\n\
            Composes with --fix, --rule, --strict, and --files-from.\n\n\
            AUTO-FIX: With --fix, hyalo applies frontmatter fixes (insert defaults, correct enum\n\
            typos, normalize dates, infer type) and body fixes from autofixable rules. Body fixes\n\
            are applied in `(start, end, rule_id)` order; overlapping fixes are deferred and\n\
            reported as conflicts — text output prints one `conflict <RULE> line <N>: range\n\
            overlap with <RULE>` line per deferred fix (first 20 per file; --detailed shows all).\n\
            Use --fix-rule <ID> (repeatable) to limit which rules autofix,\n\
            or --dry-run to preview without writing. JSON under --fix uses `total_fixed`,\n\
            `total_remaining`, and `total_conflicts` in place of plain lint's `total` —\n\
            all three, like every counter above, describe the WHOLE run and are identical at\n\
            any --limit; only the displayed `files[]` list shrinks. `remaining_errors`/\n\
            `remaining_warnings` replace `errors`/`warnings` in this shape (they count what is\n\
            left unfixed, not the whole-run count those keys mean on plain lint) so a script\n\
            reading the same key off both shapes never silently answers two different\n\
            questions.\n\n\
            EXIT CODES: 0 = clean (after fixes), 1 = errors remain, 2 = internal error.\n\n\
            EXAMPLES:\n\
            hyalo lint\n\
            hyalo lint --detailed\n\
            hyalo lint --rule MD013 --detailed\n\
            hyalo lint --rule-prefix HYALO\n\
            hyalo lint --max-per-rule 0\n\
            hyalo lint --fix --dry-run\n\
            hyalo lint --fix --fix-rule HYALO001\n\
            hyalo lint --fix\n\
            hyalo lint --profile okf         # validate OKF bundle conformance\n\
            hyalo lint --profile skills      # validate a directory of SKILL.md skills\n\
            hyalo lint --profile changelog   # validate CHANGELOG.md (Keep a Changelog)\n\
            hyalo lint --strict --format github   # inline PR annotations in GitHub Actions\n\n\
            INDEX NOTE: The snapshot index does not accelerate the body pass — body bytes are\n\
            not indexed. The frontmatter pass and file enumeration still benefit from --index.\n\n\
            SIDE EFFECTS: None without --fix. With --fix (and without --dry-run), mutated files\n\
            are rewritten atomically and the snapshot index is patched in-place.\n\n\
            TIP: Run `hyalo summary` to see a one-line lint count across the whole vault."
    )]
    Lint {
        /// Target file(s) (relative to --dir) — positional form, repeatable.
        ///
        /// `hyalo lint a.md b.md` lints both, matching `--files-from` semantics.
        #[arg(value_name = "FILE", conflicts_with_all = ["file", "glob", "type", "files_from"])]
        file_positional: Vec<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with_all = ["glob", "type", "files_from"])]
        file: Vec<String>,
        /// Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "type", "files_from"])]
        glob: Vec<String>,
        /// Restrict linting to files matching the named type's filename template.
        ///
        /// Equivalent to --glob <template-as-glob>. Mutually exclusive with --file, --glob, and --files-from.
        #[arg(long = "type", conflicts_with_all = ["file", "glob", "file_positional", "files_from"])]
        r#type: Option<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        ///
        /// Mutually exclusive with --file, positional FILE, --glob, and --type.
        /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
        /// Input is deduplicated; results follow first-seen order.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob", "type"])]
        files_from: Option<String>,
        /// Auto-remediate fixable violations (defaults, enum typos, date format, type inference)
        #[arg(long)]
        fix: bool,
        /// With --fix, preview changes without writing any files
        #[arg(long, requires = "fix")]
        dry_run: bool,
        /// Maximum number of files to include in output.
        ///
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        /// Show full per-violation details (default: summary counts only)
        #[arg(long)]
        detailed: bool,
        /// Restrict to a single rule ID (e.g. --rule MD013)
        #[arg(long, value_name = "RULE_ID")]
        rule: Option<String>,
        /// Restrict to rules with this prefix (e.g. --rule-prefix HYALO)
        #[arg(long, value_name = "PREFIX")]
        rule_prefix: Option<String>,
        /// Override per-rule violation cap (0 = unlimited; default from config or 3)
        #[arg(long, value_name = "N", value_parser = parse_limit)]
        max_per_rule: Option<usize>,
        /// With --fix, only autofix the specified rule(s) (repeatable)
        ///
        /// Requires --fix: on its own it has nothing to restrict, and clap rejects it.
        #[arg(long, value_name = "RULE_ID", requires = "fix")]
        fix_rule: Vec<String>,
        /// Promote schema warnings (missing type, undeclared property, date format) to errors
        ///
        /// Promotes "no 'type' property", "undeclared property in frontmatter" and
        /// date-format violations (HYALO003) to errors, so lint exits non-zero when those
        /// issues are found.
        ///
        /// Note: missing-type and undeclared-property promotions require a
        /// `[schema.types.*]` block in `.hyalo.toml` — on a schema-less vault
        /// these warnings are never emitted and `--strict` has no visible effect
        /// for those checks.
        ///
        /// Overrides `[lint] strict` in `.hyalo.toml` for this invocation.
        #[arg(long)]
        strict: bool,
        /// Overlay a named conformance profile for this invocation only, without touching config
        ///
        /// No `.hyalo.toml` change. `okf` enables the Open Knowledge Format §9
        /// rules plus advisory citation / augmentation checks; `madr` enables
        /// the MADR ADR schema (path-bound to `docs/decisions/**`) plus the
        /// supersede / duplicate-number advisory rules; `skills` enables the
        /// Agent Skills `skill` schema (path-bound to `**/SKILL.md`) plus the
        /// reserved-name / name↔dirname / line-budget rules. Reuses the same
        /// embedded fragment as `hyalo init --profile <name>`, so on a vault
        /// already initialized that way it is a no-op overlay (idempotent).
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Manage markdown lint rule configuration in `.hyalo.toml`
    #[command(
        name = "lint-rules",
        long_about = "Manage the markdown lint rule catalog.\n\n\
            Lists, inspects, and overrides markdown lint rules stored in `[lint.rules]` in `.hyalo.toml`.\n\n\
            Subcommands:\n\
            - list:   Show all rules with their current effective settings (default).\n\
            - show:   Show full details for a single rule.\n\
            - set:    Enable/disable a rule or change its severity.\n\
            - remove: Remove a rule override (revert to default).\n\n\
            SIDE EFFECTS: set/remove modify .hyalo.toml. list and show are read-only.\n\n\
            EXAMPLES:\n\
            hyalo lint-rules list\n\
            hyalo lint-rules show MD013\n\
            hyalo lint-rules set MD013 --enabled false\n\
            hyalo lint-rules set HYALO002 --severity error\n\
            hyalo lint-rules remove MD013"
    )]
    LintRules {
        #[command(subcommand)]
        action: Option<LintRulesAction>,
    },
    /// Manage document-type schemas in `.hyalo.toml`
    #[command(
        long_about = "Manage document-type schemas stored in `.hyalo.toml`.\n\n            Type schemas define required properties, default values, property constraints,\n            and filename templates for each document type.\n\n            Calling `hyalo types` without a subcommand defaults to `hyalo types list`.\n\n            Subcommands:\n            - list:   Show all defined types and their required fields (default).\n            - show:   Show the full schema for a single type.\n            - remove: Delete a type entry.\n            - set:    Create or update a type schema (upsert). Auto-creates the type if it doesn't exist.\n\n            TOML editing preserves comments and formatting.\n\n            SIDE EFFECTS: remove/set modify .hyalo.toml. list and show are read-only.\n\n            EXAMPLES:\n            hyalo types list\n            hyalo types show iteration\n            hyalo types set note --required title,date\n            hyalo types set iteration --property-type status=enum --property-values status=planned,in-progress,completed\n            hyalo types remove draft"
    )]
    Types {
        #[command(subcommand)]
        action: Option<TypesAction>,
    },
    /// Create a new markdown file scaffolded from a schema type
    #[command(
        name = "new",
        long_about = "Create a new markdown file scaffolded from a schema type defined in `.hyalo.toml`.\n\n\
            Synthesises a skeleton file containing:\n\
            - Frontmatter: `type: <name>` plus all required properties with type-appropriate placeholders\n\
            - Body: required sections from `required-sections` (each with a `TBD` paragraph), if declared\n\n\
            The output is intentionally incomplete — `TBD` placeholders are designed to fail\n\
            `hyalo lint`, driving the agent fill-in loop. A pattern/length-constrained string\n\
            property whose default (or the generic `TBD`) would itself violate the constraint\n\
            (e.g. a `branch` property with pattern `^iter-\\d+[a-z]*/`) is OMITTED rather than\n\
            scaffolded with an invalid value — `hyalo lint` then flags it as missing-required.\n\
            PLACEHOLDERS (DEC-285, iteration 267): a required `string` gets `TBD`; a required\n\
            `number`, `date`, `datetime` or `boolean` with no schema default is written as an\n\
            EMPTY value (`rating:`), not as `0` / today / `false`. Those three would read as real\n\
            data and lint would accept them; an empty value is a schema error naming the exact\n\
            field, so the fill-in loop is told what is missing. A `default` declared in the\n\
            schema (including `$today`) is always emitted verbatim.\n\
            DRY RUN: --dry-run prints the scaffold and writes nothing (`dry_run: true`,\n\
            `created: false`, plus the `content` it would have written). Same flag, same\n\
            meaning as on every other writing command.\n\n\
            CONSTRAINTS:\n\
            - Refuses with an error if the target file already exists\n\
            - `--file` must be vault-relative (no leading `/`, no `..` components)\n\
            - Frontmatter is limited to 64 KiB / 2000 lines; schema templates exceeding this are rejected\n\n\
            PROPERTIES: `new` writes only what the type's schema declares — there is no\n\
            `--property` flag. Set anything else with `hyalo set` immediately after; the two\n\
            chain in one shell command (see the third example below).\n\n\
            EXAMPLES:\n\
            hyalo new --type iteration --file iterations/iter-99-example.md\n\
            hyalo new --type note --file notes/2026-05-24-standup.md\n\
            hyalo new --type note --file notes/draft.md && hyalo set notes/draft.md --property status=draft\n\
            hyalo new --type iteration --file iterations/iter-99-x.md --dry-run   # preview only\n\n\
            OUTPUT: JSON envelope `{\"results\": {\"type\": ..., \"file\": ..., \"created\": true,\n\
            \"dry_run\": false}}`; a --dry-run adds `content` (the scaffold it would write).\n\
            Text mode: `created <rel-path>`, or `[dry-run] would create <rel-path>` followed by\n\
            the scaffold.\n\n\
            SIDE EFFECTS: Writes one new file (nothing at all under --dry-run). When `--index` or `--index-file` is set,\n\
            also inserts a fresh entry into the snapshot index so subsequent `--index`\n\
            queries (find, summary, etc.) see the file without a full rebuild."
    )]
    New {
        /// Document type to scaffold (must exist in `[schema.types.*]`)
        #[arg(long, value_name = "TYPE", required = true)]
        r#type: String,
        /// Vault-relative path for the new file (must not already exist)
        ///
        /// Parent directories are created if missing; the path must still resolve inside
        /// the vault after symlinks.
        #[arg(long, value_name = "FILE", required = true)]
        file: String,
        /// Print the scaffold without creating the file
        ///
        /// iter-267 (UX-17, DEC-285): parity with every other writing command,
        /// not new surface — `dry_run` is already a universal key on
        /// object-shaped mutation results (DEC-257), and `new` was the only
        /// writer whose preview you had to get by creating the file and
        /// deleting it again.
        #[arg(long)]
        dry_run: bool,
        /// When `--index` or `--index-file` is set, the snapshot index is patched in place
        /// after the file is created, so later `--index` queries see it without a full
        /// rebuild.
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Open Knowledge Format (OKF) artifact generators: regenerate `index.md` / maintain `log.md`
    #[command(
        long_about = "Deterministically (re)generate the derived reserved files of an OKF bundle.\n\n\
            OKF bundles keep two reserved, frontmatter-free files that are otherwise\n\
            hand-maintained: `index.md` (a Markdown link list of the concepts in each\n\
            directory) and `log.md` (a date-grouped changelog). These generators produce\n\
            both deterministically — no LLM, no cloud — from the concepts' frontmatter.\n\n\
            Subcommands:\n\
            - index: Regenerate every directory's `index.md` from child concepts, grouped\n\
              by `type`, with relative links. Writes into a stable managed region\n\
              (delimited by `<!-- okf:index:begin -->` / `<!-- okf:index:end -->` markers)\n\
              so hand-written prose outside the markers is preserved. Idempotent.\n\
            - log: Prepend a dated entry under today's `YYYY-MM-DD` heading (newest first)\n\
              to a scope-selectable `log.md` (bundle-root by default; §7 directory-local).\n\n\
            Both default to --dry-run and mutate only with --apply (the `links fix`/`links\n\
            auto` convention). `okf index --dry-run` exits non-zero on drift, so it doubles\n\
            as a CI check that the committed `index.md` files are up to date.\n\n\
            VALIDATE: after (re)generating, run `hyalo lint --profile okf` to check the\n\
            bundle against the OKF §9 conformance rules (warn-not-reject per the spec).\n\n\
            EXAMPLES:\n\
            hyalo okf index --dry-run          # CI: fail if index.md files are stale\n\
            hyalo okf index --apply            # regenerate all index.md files\n\
            hyalo okf index tables --apply     # scope to a subtree\n\
            hyalo okf log --message \"Added blocks table\" --apply\n\
            hyalo okf log tables --action Update --message \"...\" --apply\n\
            hyalo lint --profile okf           # validate bundle conformance"
    )]
    Okf {
        #[command(subcommand)]
        action: OkfAction,
    },
    /// Markdown Architecture Decision Record (MADR) generators
    #[command(
        display_order = 811,
        long_about = "MADR (Markdown Architecture Decision Record) artifact generators.\n\n\
            Deterministic, LLM-free maintenance of the derived files an ADR directory\n\
            otherwise hand-maintains. Currently one subcommand:\n\n\
            - `madr toc` regenerates the ADR table of contents / status dashboard\n\
              (`<adr-dir>/README.md`) from each ADR's number, title, status and date,\n\
              inside a `<!-- madr:toc:begin -->` / `<!-- madr:toc:end -->` managed region\n\
              (prose outside is preserved). Defaults to --dry-run and exits non-zero on\n\
              drift, so it doubles as a CI check.\n\n\
            VALIDATE: after (re)generating, run `hyalo lint --profile madr` to check\n\
            ADR conformance (status pattern, required sections, supersede references).\n\n\
            EXAMPLES:\n\
            hyalo madr toc --dry-run              # CI: fail if the TOC is stale\n\
            hyalo madr toc --apply                # regenerate docs/decisions/README.md\n\
            hyalo madr toc docs/adr --apply       # scope to a custom ADR directory"
    )]
    Madr {
        #[command(subcommand)]
        action: MadrAction,
    },
    /// Keep a Changelog (CHANGELOG.md) release generators
    #[command(
        display_order = 812,
        long_about = "Keep a Changelog 1.1.0 (CHANGELOG.md) maintenance commands.\n\n\
            Deterministic, LLM-free maintenance of a `CHANGELOG.md` that follows the\n\
            https://keepachangelog.com/en/1.1.0/ grammar. Two subcommands:\n\n\
            - `changelog add --category <CAT> --message \"...\"` appends an entry under the\n\
              `### <CAT>` subsection of `## [Unreleased]` (creating the subsection if\n\
              needed). Categories: Added, Changed, Deprecated, Removed, Fixed, Security.\n\
            - `changelog release <X.Y.Z> [--date YYYY-MM-DD]` rotates the accumulated\n\
              `## [Unreleased]` content into a dated `## [X.Y.Z] - <date>` section, recreates\n\
              an empty `[Unreleased]` above it, and appends a placeholder footer link\n\
              reference (`[X.Y.Z]: TBD`). It refuses to release a version that already\n\
              exists (idempotency guard).\n\n\
            Both default to --dry-run and exit non-zero on drift (a CI signal); pass\n\
            --apply to write.\n\n\
            The target file is `<vault>/CHANGELOG.md` unless `[changelog] path` names one\n\
            (resolved against the config directory — that is how a repo-root CHANGELOG.md\n\
            is used with a docs-subdir vault). Either way the file must still resolve\n\
            inside that root: a CHANGELOG.md symlinked out of it is refused (exit 1).\n\n\
            VALIDATE: after releasing, replace the `TBD` link target with the real\n\
            compare/tag URL and run `hyalo lint --profile changelog`.\n\n\
            EXAMPLES:\n\
            hyalo changelog add --category Added --message \"New export format\" --apply\n\
            hyalo changelog release 1.2.0 --dry-run\n\
            hyalo changelog release 1.2.0 --apply\n\
            hyalo changelog release 1.2.0 --date 2026-07-17 --apply"
    )]
    Changelog {
        #[command(subcommand)]
        action: ChangelogAction,
    },
    /// Print the effective configuration (resolved .hyalo.toml path, dir, and core settings)
    #[command(
        name = "config",
        display_order = 899,
        long_about = "Print the effective configuration for the current working directory.\n\n\
            Shows which .hyalo.toml is active (or none) and the effective values:\n\
            config_path, cwd, dir, dir_salvaged, format, hints, site_prefix, exempt.\n\n\
            SCAN SETTINGS: `results.scan` reports the vault-walker configuration —\n\
            `include` (hidden dot-subtrees the walker descends into), `exclude` (globs whose\n\
            files NO command sees: dropped at discovery, so find/summary/tags/properties/\n\
            lint/links/mv/backlinks/create-index/views/types/okf/madr and every --index read\n\
            agree on the file set — naming one explicitly with --file is refused, with the\n\
            matching glob quoted), and `verbose_skips` (stream the per-file YAML diagnostics\n\
            instead of collapsing them into one end-of-run summary line).\n\n\
            MALFORMED CONFIG: when a .hyalo.toml exists but could not be parsed, `malformed`\n\
            is true and `parse_error` carries the diagnostic — every other value shown is a\n\
            built-in default, not what the file asked for, except `dir` when `dir_salvaged`\n\
            is true: a lenient re-read recovers just that key so read-only commands still\n\
            point at the configured vault. Detectable from the output alone, without\n\
            scraping stderr — this command does not print the diagnostic there too.\n\
            A malformed config also makes every MUTATING command and every GATE command\n\
            (`lint`, `find --strict`, `views run`) exit 1: their exit code is a verdict, and\n\
            a verdict computed without the file's [lint] ignore and schemas is not the one\n\
            the vault asked for. Plain reads still answer, with a -q-proof warning.\n\n\
            EXAMPLES:\n\
            hyalo config\n\
            hyalo config --raw\n\
            hyalo config --dir ../other-vault\n\
            hyalo config --jq '.results.dir'\n\
            hyalo config --jq '.results.malformed'\n\
            hyalo config --format json\n\n\
            OUTPUT: Line-by-line in text format; the standard JSON envelope with --format json —\n\
            the settings live under `results`, and the config's own hints switch is reported as\n\
            `results.hints_enabled` so it does not collide with the envelope's `hints` array.\n\
            --jq filters that envelope like it does for every other command.\n\
            The raw file text is opt-in via --raw: it is a multi-KB blob that dominated both\n\
            renderings and buried the resolved values it was printed next to.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Config {
        /// Also print the raw .hyalo.toml text (`results.raw_contents` in JSON)
        #[arg(long)]
        raw: bool,
    },
    /// Print the short help for a command — same page as `hyalo <cmd> -h`
    #[command(
        display_order = 901,
        long_about = "Print the short (-h) help for a command.\n\n\
            `hyalo help find` and `hyalo find -h` print the same page. This is \
            deliberate: the long `--help` page is 5-10x larger, and the short page's \
            footer names `hyalo <cmd> --help` when you want it.\n\n\
            Accepts a subcommand path, so `hyalo help task toggle` works.\n\
            An unknown name gets clap's usual did-you-mean suggestion.\n\n\
            EXAMPLES:\n\
              hyalo help              # same as hyalo -h\n\
              hyalo help find         # same as hyalo find -h\n\
              hyalo help task toggle  # same as hyalo task toggle -h\n\
              hyalo find --help       # the long reference page\n\n\
            SIDE EFFECTS: None (prints to stdout)."
    )]
    Help {
        /// Command to describe, e.g. `find` or `task toggle` (omit for `hyalo -h`)
        #[arg(value_name = "COMMAND")]
        command: Vec<String>,
    },
    /// Generate shell completions for the given shell
    #[command(
        name = "completions",
        // Original singular name, kept for backward compatibility (hoppy and
        // ff-rdp use the plural form; hyalo now matches).
        visible_alias = "completion",
        display_order = 900,
        long_about = "Generate shell completion scripts.\n\n\
            Prints a completion script for the specified shell to stdout.\n\
            Source or install the output in your shell's completion directory.\n\n\
            EXAMPLES:\n\
              bash:        hyalo completions bash  > ~/.local/share/bash-completion/completions/hyalo\n\
              zsh:         hyalo completions zsh   > ~/.local/share/zsh/site-functions/_hyalo\n\
              fish:        hyalo completions fish  > ~/.config/fish/completions/hyalo.fish\n\
              elvish:      hyalo completions elvish > ~/.config/elvish/lib/completions/hyalo.elv\n\
              powershell:  hyalo completions powershell > _hyalo.ps1\n\n\
            SIDE EFFECTS: None (prints to stdout)."
    )]
    Completion {
        /// Target shell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub(crate) enum OkfAction {
    /// Regenerate every directory's `index.md` from concept frontmatter
    #[command(
        long_about = "Regenerate the reserved `index.md` in every directory from the frontmatter\n\
            of its child concepts.\n\n\
            For each directory, concepts are grouped by their `type` (untyped concepts fall\n\
            under an `Other` group) and rendered as `* [title](relative-link) - description`\n\
            lines; the title falls back to the filename stem and the description is optional.\n\
            Immediate subdirectories are listed under a `Subdirectories` group. Links are\n\
            relative to the `index.md`'s own directory and always use forward slashes\n\
            (cross-platform).\n\n\
            The generated list lives inside a stable managed region delimited by\n\
            `<!-- okf:index:begin -->` and `<!-- okf:index:end -->` markers; any prose\n\
            outside those markers is preserved verbatim across runs. The bundle-root\n\
            `index.md`'s lone `okf_version` frontmatter key is preserved. Links are\n\
            CommonMark-valid: spaced destinations are angle-bracket wrapped, `[`/`]` in\n\
            titles are escaped, and multi-line descriptions are collapsed to one line.\n\n\
            NON-DESTRUCTIVE ADOPT: an existing `index.md` WITHOUT markers is *adopted* —\n\
            its entire hand-written body is preserved and the managed region is appended\n\
            after it (dry-run reports `adopt (preserving N existing lines)`). Pass\n\
            --replace to instead overwrite such a file with a fresh managed index,\n\
            discarding its body. On case-insensitive filesystems an existing `INDEX.md`\n\
            is recognized as the reserved file and adopted by its on-disk casing.\n\n\
            MALFORMED MARKERS: a file whose markers are dangling (begin with no end, or\n\
            end with no begin), reversed, or duplicated is left byte-identical and\n\
            reported as `skip` with a warning — never rewritten (splicing across a broken\n\
            marker would delete the prose after it). Fix the markers by hand; the\n\
            `OKF-INDEX-MARKERS` lint rule flags the same condition. An impossible or\n\
            unwritable target (a directory named `index.md`) is warned-and-skipped and\n\
            the run continues with the other files (no partial mid-run abort).\n\n\
            SCOPING: files matching a `[okf] ignore` glob in `.hyalo.toml` (e.g.\n\
            `_template/**`) are neither indexed nor generated into. A concept with\n\
            unparseable frontmatter is skipped with a stderr warning (suppressed by\n\
            -q/--quiet; the run continues and every other index is still generated). A\n\
            nonexistent scope directory is rejected (exit 1), not vacuously passed.\n\n\
            Running with --apply twice is a no-op (idempotent). In --dry-run (the default)\n\
            the command exits non-zero when any `index.md` would change — use this in CI.\n\
            EXIT CODES (dry-run): 0 = clean (no index.md would change), 1 = drift (at least\n\
            one index.md is stale — the CI failure signal), 2 = error (e.g. an unreadable\n\
            file or an invalid scope). --apply exits 0 on success, 2 on error.\n\n\
            SIDE EFFECTS: writes `index.md` files only with --apply.\n\n\
            EXAMPLES:\n\
            hyalo okf index --dry-run\n\
            hyalo okf index --apply\n\
            hyalo okf index tables --apply\n\
            hyalo okf index --apply --replace   # overwrite marker-less index.md"
    )]
    Index {
        /// Optional directory (vault-relative) to scope regeneration to a subtree
        #[arg(value_name = "DIR")]
        scope: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
        /// Overwrite a marker-less `index.md`, discarding its hand-written body.
        ///
        /// Without this flag such a file is *adopted*: its body is preserved and
        /// the managed region is appended (non-destructive default).
        #[arg(long)]
        replace: bool,
    },
    /// Prepend a dated entry to a scope-selectable `log.md`
    #[command(
        long_about = "Prepend a dated entry to the reserved `log.md` changelog.\n\n\
            Per SPEC §7 a `log.md` MAY appear at any level of the hierarchy and records the\n\
            history of that scope (directory-local, not bundle-wide). TARGET selects which\n\
            `log.md` is written:\n\
            - a directory  → writes/creates `TARGET/log.md`\n\
            - a `log.md` path → writes that file directly\n\
            - omitted      → the bundle-root `log.md`\n\n\
            The entry is inserted under today's `YYYY-MM-DD` heading, newest first: if the\n\
            heading already exists the entry becomes its first bullet, otherwise a fresh\n\
            dated section is inserted above older ones. `--action Update` prefixes a bold\n\
            action word (`- **Update:** ...`), a convention (not required per §7); an\n\
            empty `--action \"\"` is a user error, like an empty --message. A multi-line\n\
            --message stays a single valid list item — continuation lines are indented\n\
            under the bullet so an embedded `## heading` can't break the log structure.\n\
            The file is created (with no frontmatter — it is a reserved file) when absent.\n\n\
            TARGET is validated to stay inside the vault/bundle; paths that escape are\n\
            rejected, and a nonexistent directory target is rejected consistently by both\n\
            dry-run and apply (create the directory first). Defaults to --dry-run; pass\n\
            --apply to write.\n\n\
            SIDE EFFECTS: writes/creates one `log.md` only with --apply.\n\n\
            EXAMPLES:\n\
            hyalo okf log --message \"Added blocks table\" --apply\n\
            hyalo okf log tables --action Update --message \"Refreshed schema\" --apply\n\
            hyalo okf log tables/log.md --message \"...\" --apply"
    )]
    Log {
        /// Directory or `log.md` path selecting which log to write (default: bundle-root)
        #[arg(value_name = "TARGET")]
        target: Option<String>,
        /// The log entry text (required)
        #[arg(long, value_name = "TEXT", required = true)]
        message: String,
        /// Optional bold action word prefixing the entry (e.g. `Update`, `Add`)
        #[arg(long, value_name = "WORD")]
        action: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum MadrAction {
    /// Regenerate the ADR table of contents / status dashboard
    #[command(
        long_about = "Regenerate the ADR table of contents / status dashboard.\n\n\
            Scans the ADR directory (default `docs/decisions/`, or the DIR argument), reads\n\
            each ADR's number (from its `NNNN-slug.md` filename), title (frontmatter `title`\n\
            else the first `# ` heading else the filename stem), `status`, and `date`, and\n\
            renders a Markdown table into `<adr-dir>/README.md`.\n\n\
            The table lives inside a `<!-- madr:toc:begin -->` / `<!-- madr:toc:end -->`\n\
            managed region; any prose outside those markers is preserved verbatim across\n\
            runs. An existing marker-less `README.md` is *adopted* (its hand-written body\n\
            is preserved and the TOC region appended); pass --replace to overwrite it\n\
            instead. Running with --apply twice is a no-op (idempotent). In --dry-run (the\n\
            default) the command exits non-zero when the TOC would change — use this in CI.\n\n\
            SIDE EFFECTS: writes `<adr-dir>/README.md` only with --apply.\n\n\
            DIR must stay inside the vault once symlinks are resolved: a `../` traversal\n\
            or a symlinked ADR directory pointing out is refused (exit 1) in dry-run and\n\
            apply alike.\n\n\
            EXAMPLES:\n\
            hyalo madr toc --dry-run\n\
            hyalo madr toc --apply\n\
            hyalo madr toc docs/adr --apply"
    )]
    Toc {
        /// ADR directory, vault-relative (default: `docs/decisions`)
        ///
        /// Must resolve inside the vault.
        #[arg(value_name = "DIR")]
        adr_dir: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
        /// Overwrite a marker-less `README.md`, discarding its hand-written body.
        ///
        /// Without this flag such a file is *adopted*: its body is preserved and
        /// the managed region is appended (non-destructive default).
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum ChangelogAction {
    /// Rotate `## [Unreleased]` into a dated `## [X.Y.Z]` release section
    #[command(
        long_about = "Cut a release: rotate the accumulated `## [Unreleased]` content into a\n\
            dated `## [X.Y.Z] - <date>` version section.\n\n\
            The existing `## [Unreleased]` heading is relabelled to the new version (its\n\
            body — the categorised entries — moves with it), a fresh empty `## [Unreleased]`\n\
            is inserted above it, and a placeholder footer link reference `[X.Y.Z]: TBD` is\n\
            appended (replace `TBD` with the real compare/tag URL). The date defaults to\n\
            today; override with --date YYYY-MM-DD.\n\n\
            Refuses to release a version that already appears in the file (idempotency\n\
            guard). Defaults to --dry-run and exits non-zero when the file would change;\n\
            pass --apply to write.\n\n\
            SIDE EFFECTS: writes CHANGELOG.md only with --apply.\n\n\
            EXAMPLES:\n\
            hyalo changelog release 1.2.0 --dry-run\n\
            hyalo changelog release 1.2.0 --apply\n\
            hyalo changelog release 1.2.0 --date 2026-07-17 --apply"
    )]
    Release {
        /// The new version (MAJOR.MINOR.PATCH)
        #[arg(value_name = "VERSION")]
        version: String,
        /// Release date (YYYY-MM-DD); defaults to today
        #[arg(long, value_name = "DATE")]
        date: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
    },
    /// Append an entry under a category in `## [Unreleased]`
    #[command(
        long_about = "Append a changelog entry under a category of the `## [Unreleased]` section.\n\n\
            Inserts `- <message>` under the `### <category>` subsection of `## [Unreleased]`,\n\
            creating the `[Unreleased]` section and/or the category subsection if they are\n\
            missing (a fresh `# Changelog` skeleton is created when the file is absent).\n\
            Category must be one of the six Keep a Changelog kinds: Added, Changed,\n\
            Deprecated, Removed, Fixed, Security (case-insensitive).\n\n\
            Defaults to --dry-run and exits non-zero on drift; pass --apply to write.\n\n\
            SIDE EFFECTS: writes/creates CHANGELOG.md only with --apply.\n\n\
            EXAMPLES:\n\
            hyalo changelog add --category Added --message \"New export format\" --apply\n\
            hyalo changelog add --category Fixed --message \"Crash on empty input\" --apply"
    )]
    Add {
        /// Change category (Added/Changed/Deprecated/Removed/Fixed/Security)
        #[arg(long, value_name = "CATEGORY")]
        category: String,
        /// The entry text (required)
        #[arg(long, value_name = "TEXT", required = true)]
        message: String,
        /// Wrap the entry to COLS columns on word boundaries (omit for one unwrapped bullet)
        ///
        /// Continuation lines are hanging-indented under the bullet text (2 spaces).
        /// Useful for 80-column changelogs.
        #[arg(long, value_name = "COLS")]
        wrap: Option<usize>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // Set variant holds FindFilters by design; boxing would complicate dispatch
pub(crate) enum ViewsAction {
    /// List all saved views
    // `summary` alias — mirrors `tags summary` / `properties summary` so the
    // aggregate verb works across every subcommand group (iter-192).
    #[command(
        visible_alias = "summary",
        long_about = "Show all saved views and their filter configurations.\n\n\
        OUTPUT: JSON envelope with results (array of view objects) and total count.\n\
        SIDE EFFECTS: None (read-only)."
    )]
    List,
    /// Create or update a saved view
    #[command(long_about = "Save a combination of find filters under a name.\n\n\
        The view is stored in .hyalo.toml and can be recalled with `hyalo find --view <name>`.\n\
        You can combine --view with additional CLI filters to extend or override the saved set.\n\
        Overwrites if the view already exists.\n\n\
        WHAT IS PERSISTED: every flag shown below \u{2014} not just the filters. --sort, --reverse, \
        --limit and --fields are saved with the view, and so are the output-shaping switches \
        --strict, --filenames-only and --filenames0, so a saved view can be a complete CI gate \
        rather than a filter set someone still has to decorate. On recall, a CLI flag of the same \
        name overrides the saved value; the three bools can only be turned ON by the CLI, never \
        off. A pinned `fields` behaves exactly like an explicit --fields (an exact projection), \
        and a CLI --fields replaces the pin rather than adding to it.\n\n\
        SIDE EFFECTS: Modifies .hyalo.toml.")]
    Set {
        /// View name (first positional arg)
        #[arg(value_name = "NAME")]
        name: String,
        /// Optional BM25 search pattern to save with the view (second positional arg).
        ///
        /// Example: `hyalo views set my-view "search terms" --tag foo`
        #[arg(value_name = "PATTERN")]
        pattern: Option<String>,
        #[command(flatten)]
        filters: FindFilters,
    },
    /// Delete a saved view
    #[command(long_about = "Remove a saved view from .hyalo.toml.\n\n\
        SIDE EFFECTS: Modifies .hyalo.toml.")]
    Remove {
        /// View name to delete
        #[arg(value_name = "NAME")]
        name: String,
    },
    /// Run a saved view: equivalent to `hyalo find --view <NAME>`
    ///
    /// Any extra find flags passed after the view name are merged on top of
    /// the saved filter set (list filters extend, scalar flags override).
    ///
    /// Example: `hyalo views run open-tasks`
    ///          `hyalo views run drafts --tag project`
    ///          `hyalo views run drafts "search terms"`
    #[command(
        external_subcommand = false,
        long_about = "Run a saved view as if you called `hyalo find --view <NAME>`.\n\n\
            Extra find flags passed after the view name extend or override the saved filters.\n\n\
            The optional second positional argument is a BM25 PATTERN with exactly the\n\
            semantics `find` gives it — ranked full-text search, mutually exclusive with -e.\n\
            It overrides a pattern saved in the view.\n\n\
            EXAMPLES:\n\
            hyalo views run drafts\n\
            hyalo views run drafts \"search terms\"\n\
            hyalo views run drafts --tag project\n\n\
            SIDE EFFECTS: None (read-only find)."
    )]
    Run {
        /// View name to run
        #[arg(value_name = "NAME")]
        name: String,
        /// BM25 ranked full-text search pattern, same semantics as `find <PATTERN>`.
        ///
        /// Overrides a pattern saved in the view. Mutually exclusive with -e/--regexp
        #[arg(value_name = "PATTERN", conflicts_with = "regexp")]
        pattern: Option<String>,
        /// Additional find filters to merge on top of the view
        #[command(flatten)]
        filters: FindFilters,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum TypesAction {
    /// List all defined types and their required fields (default)
    // `summary` alias — see `ViewsAction::List` (iter-192).
    #[command(
        visible_alias = "summary",
        long_about = "List all type schemas defined in `.hyalo.toml`.\n\n          OUTPUT: JSON envelope with results array and total count.\n            SIDE EFFECTS: None (read-only)."
    )]
    List,
    /// Show the full schema for a single type
    #[command(
        long_about = "Display the full merged schema for a named type.\n\n            OUTPUT: JSON object with type name, required fields, defaults,\n            filename template, property constraints, and required_sections.\n\
            When declared, `item_pattern` is included in the constraint for `string-list` properties,\n\
            an `object-list` property carries `required-keys`, `allowed-keys` (omitted when any key\n\
            is allowed) and `key-patterns` (a key -> regex block, omitted when empty),\n\
            and `required_sections` lists the declared required body sections.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Show {
        /// Type name to display
        #[arg(value_name = "TYPE")]
        type_name: String,
    },
    /// Remove a type entry from `.hyalo.toml`
    #[command(
        long_about = "Remove a `[schema.types.<name>]` section from `.hyalo.toml`.\n\n            Fails with a user error if the type does not exist.\n\n            OUTPUT: JSON result with action and type name.\n            SIDE EFFECTS: Modifies .hyalo.toml."
    )]
    Remove {
        /// Type name to remove
        #[arg(value_name = "TYPE")]
        type_name: String,
    },
    /// Create or update a type schema's required fields, defaults, or property constraints
    #[command(
        long_about = "Create or update a type schema in `.hyalo.toml`. If the type doesn't exist, it is created automatically.\n\n            When creating the first type (i.e. the [schema] section is new), `validate_on_write = true` is set automatically so that `set`/`append` enforce schema constraints by default.\n\n            All mutation flags are optional and combinable in a single invocation.\n\n            FLAGS:\n            - --required <fields>: comma-separated required property names to add (repeatable).\n            - --default key=value: set a default; auto-applied to files missing the property.\n            - --property-type key=type: set a type constraint (string/date/datetime/datetime-tz/number/boolean/list/enum). `datetime-tz` accepts RFC 3339 timezone-aware values (e.g. 2026-05-28T22:44:47+00:00 or ...Z); `datetime` stays naive (no offset). `string-list` and `object-list` carry constraints and are configured in `.hyalo.toml` only; see `hyalo types show`.\n            - --property-values key=val1,val2,...: set enum values; implies type=enum.\n            - --filename-template <template>: set the filename template for this type.\n            - --dry-run: preview changes without writing anything.\n\n            TYPE BINDING: a file binds to a type when its `type:` frontmatter names it. The value may be a plain string, a [[Wikilink]] (bare or quoted, aliases and paths resolved to the note name), or a ONE-element list of either — the shape Obsidian's property editor writes for a link-typed property. A multi-element list names no type and is reported by `lint`.\n\n            A --required field with no constraint of its own gets one auto-declared; its type is inferred from the values the vault already holds for that key on files of this type (falling back to `string` when there are none).\n\n            OUTPUT: JSON result with action, dry_run, defaults_applied, constraint_violations.\n            SIDE EFFECTS: Modifies .hyalo.toml and may write to vault files (unless --dry-run)."
    )]
    Set {
        /// Type name to update
        #[arg(value_name = "TYPE")]
        type_name: String,
        /// Comma-separated list of required property names to add (repeatable)
        #[arg(long, value_name = "FIELDS")]
        required: Vec<String>,
        /// Set a default value: key=value (repeatable)
        #[arg(long, value_name = "KEY=VALUE")]
        default: Vec<String>,
        /// Set the property type constraint: key=type (repeatable)
        #[arg(long, value_name = "KEY=TYPE")]
        property_type: Vec<String>,
        /// Set enum values for a property: key=val1,val2,... (repeatable)
        #[arg(long, value_name = "KEY=VALUES")]
        property_values: Vec<String>,
        /// Set the filename template for new files of this type
        #[arg(long, value_name = "TEMPLATE")]
        filename_template: Option<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum LintRulesAction {
    /// List all available lint rules with their current settings (default)
    // `summary` alias — see `ViewsAction::List` (iter-192).
    #[command(visible_alias = "summary")]
    List {
        /// Only show enabled rules
        #[arg(long)]
        enabled_only: bool,
        /// Only show disabled rules
        #[arg(long, conflicts_with = "enabled_only")]
        disabled_only: bool,
        /// Filter by rule ID prefix (e.g. --rule-prefix HYALO)
        #[arg(long, value_name = "PREFIX")]
        rule_prefix: Option<String>,
    },
    /// Show full details for a single rule
    Show {
        /// Rule ID (e.g. MD013 or HYALO001)
        #[arg(value_name = "RULE_ID")]
        rule_id: String,
    },
    /// Enable, disable, or change severity of a rule
    Set {
        /// Rule ID to configure
        #[arg(value_name = "RULE_ID")]
        rule_id: String,
        /// Enable or disable the rule
        #[arg(long, value_name = "BOOL")]
        enabled: Option<bool>,
        /// Override severity: warn or error
        #[arg(long, value_name = "SEVERITY")]
        severity: Option<String>,
        /// Preview changes without writing .hyalo.toml
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a rule override (revert to default)
    Remove {
        /// Rule ID to reset
        #[arg(value_name = "RULE_ID")]
        rule_id: String,
        /// Preview changes without writing .hyalo.toml
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub(crate) enum LinksAction {
    /// Auto-repair broken links using fuzzy matching
    #[command(long_about = "Find broken links and propose (or apply) fixes.\n\n\
            Matching strategies (in priority order):\n\
            1. Case-insensitive exact match\n\
            2. Extension mismatch (.md present/absent)\n\
            3. Unique stem match anywhere in the vault (shortest-path); when the\n\
               target was written WITH a directory (/a/b or a/b) this is only a\n\
               basename guess and is reported as the separate\n\
               basename-fallback strategy\n\
            4. Fuzzy match: a candidate must clear --threshold on the\n\
               Jaro-Winkler similarity of the filename stem\n\n\
            Use --apply to write fixes to disk. Without --apply, only a dry-run report is printed.\n\n\
            CONFIDENCE: every low-confidence proposal carries a score in 0.0-1.0 that weights\n\
            the final path segment (the basename/slug) at 70% and the directory path at 30%.\n\
            The directory term is itself three-quarters shared leading components, so a\n\
            relocation inside a section (a/b/c/page -> a/b/d/page) scores far above a\n\
            same-name substitution across sections (/actions -> graphql/reference/actions.md,\n\
            which scores exactly 0.7). A target written with no directory at all asserts no\n\
            location, so only its basename is scored.\n\n\
            LOW-CONFIDENCE MATCHES ARE GATED TWICE: a broken [[foo]] can \"match\" an unrelated\n\
            bar.md, and /actions or guides/actions can \"match\" any actions.md anywhere in the\n\
            vault. Both fuzzy and basename-fallback fixes are reported in their own bucket and\n\
            are NOT written by plain --apply.\n\
              Gate 1 — opt in with --apply-fuzzy (or --min-confidence, which implies it).\n\
              Gate 2 — a confidence floor, 0.8 by default. Proposals below it stay reported\n\
                       but unapplied and are counted as fuzzy_below_floor. Move the floor with\n\
                       --min-confidence <0.0-1.0> or `[links] fuzzy_min_confidence` in\n\
                       .hyalo.toml (the flag wins); --min-confidence 0 accepts everything.\n\
            Measured on the GitHub Docs corpus (3,710 files, 6,099 broken links): the default\n\
            floor applies 2,253 rewrites at 99.3% correct, against 4,659 at 82.2% with no floor.\n\n\
            STRATEGY LABELS: the text report brackets each proposal with the strategy that\n\
            produced it — [basename-fallback 0.87] for a discarded directory, [fuzzy-match 0.91]\n\
            for path similarity — so the two are never confused. JSON carries both the\n\
            PascalCase `strategy` and the kebab-case `rule`.\n\n\
            THE BASENAME GATE KEYS ON THE WRITTEN DIRECTORY, NOT THE LEADING SLASH: a target\n\
            written with any directory component (/guides/actions, guides/actions,\n\
            [[sub/actions]]) asserts a location, so throwing it away and matching on the last\n\
            segment is a guess and needs --apply-fuzzy. A target written with no directory at\n\
            all ([[actions]], [x](actions.md)) asserts no location: resolving it by stem is the\n\
            documented short-form rule and plain --apply writes it.\n\n\
            FIXES ALWAYS ROUND-TRIP: a repair is written in the form the link was written in —\n\
            site-absolute stays site-absolute, a relative destination is computed from the source\n\
            file's own directory — and any fix whose emitted target would still not resolve is\n\
            refused and reported under unfixable rather than written.\n\n\
            SHORT-FORM WIKILINKS (Obsidian compatibility):\n\
            A bare [[Note]] that resolves to some **/Note.md anywhere in the vault is NOT\n\
            broken and is left untouched. Only a stem-casing mismatch ([[note]] for Note.md)\n\
            triggers a case-mismatch fix — and the fix preserves the short form ([[Note]],\n\
            never [[sub/Note]]). Links matching >=2 files are reported as ambiguous and\n\
            never auto-fixed.\n\n\
            Use --expand-short-form to opt into path expansion (Obsidian-incompatible).\n\n\
            Case-mismatch detection: when case-insensitive resolution is active (controlled by\n\
            `[links] case_insensitive` in .hyalo.toml — \"auto\", \"true\", or \"false\"), broken links\n\
            that differ only in casing from an on-disk file are reported as case_mismatches and\n\
            rewritten to the canonical casing when --apply is used. On macOS and Windows,\n\
            \"auto\" (the default) enables this automatically. A bare-stem link whose exact path\n\
            fails but whose stem resolves in a *different directory* is a relocation, not a\n\
            casing fix — reported separately as relocations/relocation_fixes, also written by\n\
            plain --apply.")]
    Fix {
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
        /// Apply fixes to files on disk
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Minimum stem similarity (0.0–1.0) for a file to be a fuzzy candidate at all
        ///
        /// Jaro-Winkler. Candidates that clear it are then scored and ranked by
        /// confidence — see --min-confidence.
        #[arg(long, value_name = "N", default_value = "0.8", value_parser = parse_threshold)]
        threshold: f64,
        /// Apply low-confidence fixes too (excluded from --apply by default)
        ///
        /// Fuzzy matches are guesses: a broken [[foo]] can "match" an
        /// unrelated bar.md. So is a basename fallback, where a target that
        /// wrote a directory (/actions or guides/actions) matches some
        /// actions.md elsewhere in the vault. Both are reported in a separate
        /// bucket and are NOT written by plain --apply. Pass --apply-fuzzy to
        /// opt in — which still only writes proposals at or above the
        /// confidence floor (0.8 by default; see --min-confidence).
        #[arg(long)]
        apply_fuzzy: bool,
        /// Confidence floor for applying low-confidence fixes (0.0–1.0); implies --apply-fuzzy
        ///
        ///
        /// Defaults to 0.8, or to `[links] fuzzy_min_confidence` in
        /// .hyalo.toml when set; this flag overrides both. Proposals below the
        /// floor stay in the reported-but-not-applied bucket and are counted
        /// as fuzzy_below_floor. Pass 0 to accept every proposal (the
        /// pre-0.21 behaviour), 0.99 to apply almost nothing.
        #[arg(long, value_name = "N", value_parser = parse_threshold)]
        min_confidence: Option<f64>,
        #[arg(
            short,
            long,
            value_name = "GLOB",
            help = GLOB_FLAG_SHORT_DOC,
            long_help = GLOB_FLAG_DOC,
        )]
        glob: Vec<String>,
        /// Ignore broken links whose target contains SUBSTR (repeatable)
        ///
        /// Useful for skipping Hugo template links, external paths, etc.
        #[arg(long, value_name = "SUBSTR")]
        ignore_target: Vec<String>,
        /// Expand short-form wikilinks ([[Name]]) to their full vault path when applying fixes
        ///
        /// By default, hyalo treats bare stem wikilinks as valid Obsidian short-form links:
        /// [[Corina]] that resolves to sub/Corina.md is left untouched. With this flag,
        /// such links are expanded to [[sub/Corina]] on --apply. NOTE: this breaks
        /// Obsidian compatibility — Obsidian resolves short-form links by stem across the
        /// whole vault and does not require the full path.
        #[arg(long)]
        expand_short_form: bool,
        /// Suppress the cosmetic case-mismatch rewrite plans
        ///
        /// (UX-6, iter-244; narrowed by DEC-267 in iter-261.)
        ///
        /// Since iter-261 link *resolution* folds case on every platform, so a
        /// case-only mismatch is never broken and never counted under
        /// `broken` — this flag no longer changes what resolves. What it still
        /// does is hide the `link-case-mismatch` fix plans themselves, for a
        /// vault that does not want its link spellings normalised.
        ///
        /// On case-folded vault layouts (MDN-style `en-US` vs `en-us`
        /// directories on macOS/Windows), a plain `links fix --dry-run`
        /// offers a `link-case-mismatch` rewrite plan for every such link —
        /// tens of thousands of no-op rewrites on a large checkout. With
        /// this flag those links count as resolved and the report comes back
        /// clean. Same effect persistently via
        /// `[links.case_insensitive] resolve = true` in `.hyalo.toml`.
        #[arg(long)]
        case_insensitive: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Auto-link unlinked mentions of known page titles
    #[command(
        long_about = "Scan body text for unlinked mentions of known page titles and convert them to [[wikilinks]].\n\n\
            Title sources (in priority order):\n\
            1. Filename stems (without .md)\n\
            2. Frontmatter `title` property\n\
            3. Frontmatter `aliases` property (list of alternate names)\n\n\
            Exclusion zones: frontmatter, fenced code blocks, inline code (CommonMark rules —\n\
            an unmatched backtick is literal text and a span never crosses a blank line),\n\
            existing [[wikilinks]] and [markdown](links) — label AND destination, whether the\n\
            destination is internal or external — bare URLs and <autolinks> in prose, headings,\n\
            comment fences (%%), Liquid/Jinja expressions ({% ... %} and {{ ... }}),\n\
            raw HTML tags including attribute values (<img src=\"...\">, <a name=\"...\">, HTML\n\
            comments) — text BETWEEN tags stays linkable — and ANY well-formed `[...]` bracket\n\
            span, not only real links. This covers CommonMark reference links (`[label][ref]`,\n\
            collapsed `[ref][]`, shortcut `[ref]`, `![ref][ref]` images, and the `[ref]: url\n\
            \"title\"` definition line itself) but goes wider: an undefined bracketed mention —\n\
            GitHub-Docs-style style-guide placeholders (`[ACCOUNT ROLE]`), vscode-docs-style PR\n\
            area tags (`[typescript-language-features]`) — is inert too, because writing\n\
            `[[target]]` touching or inside an unrelated bracket produces nested bracket soup\n\
            (`[[[typescript]]-language-features]`) that hyalo's own resolver then misreads as a\n\
            malformed link. A missed candidate inside decorative brackets costs nothing; that\n\
            corruption does not. Self-links are excluded too.\n\n\
            DOCUMENT-SCOPED ZONES: a wikilink, markdown link, or raw HTML tag that wraps across\n\
            a line boundary is inert across its whole span, not just the physical line the\n\
            match happens to sit on — e.g. `[[target\\n|alias text]]` or a tag's attributes\n\
            continuing onto the next line. Like inline code spans, these constructs never\n\
            reach across a blank line, heading, or fence: an unclosed `[[` at the end of a\n\
            paragraph does not swallow the next paragraph.\n\n\
            ALIAS EMISSION: when the matched surface text differs from the emitted target —\n\
            including by case alone (`Pulls` vs `pulls`) — the replacement is\n\
            `[[target|matched_text]]`, preserving what the page renders. A plain `[[target]]`\n\
            is only written when the matched text is byte-identical to the target.\n\n\
            Filtering options:\n\
            --first-only          Only emit the first mention of each target per source file. An\n\
                                   existing [[wikilink]] (or aliased [[target|label]]) to a target\n\
                                   anywhere in the file counts as its first mention, case-insensitively\n\
                                   — no new match is emitted for that target in that file even if a\n\
                                   plain-text mention appears earlier in the file than the link.\n\
            --no-first-only       Force first-only OFF for this run, even when [links.auto]\n\
                                   first_only = true is set in .hyalo.toml. Conflicts with\n\
                                   --first-only.\n\
            --exclude-title       Exclude specific titles (repeatable, case-insensitive)\n\
            --exclude-target-glob Exclude target pages by vault-relative path glob (repeatable,\n\
                                   case-insensitive — 'templates/*' also excludes 'Templates/X.md')\n\n\
            NOISY CANDIDATE TITLES (built-in stop-list): titles that look like a source of \
            over-linking are HELD BACK by default, not merely warned about. Two things flag a \
            title: it is an ordinary English word, a generic doc filename or a platform/format \
            name (\"permissions\", \"index\", \"README\", \"github\", \"markdown\"), or it is unusually \
            frequent for this run — at least 25 proposed links and at least 2.5% of the run. The \
            frequency trigger is language-independent, so non-English titles are covered too. \
            The report always carries default_excluded_titles (the lowercased titles held back) \
            and default_excluded_mentions (how many mentions that was), and one stderr note names \
            them with their counts plus the --exclude-title flags that reproduce the exclusion \
            explicitly; the prose list stops at the five noisiest and says so, while the flags \
            cover every one. Only titles that actually produced matches are ever flagged.\n\
            TURNING IT OFF: setting [links.auto] exclude_titles hands the decision to your own \
            list — the built-in stop-list steps aside entirely — and warn_common_titles = false \
            (or --no-warn-common-titles for one run) switches off both the exclusion and the \
            note, restoring the pre-iteration-267 all-candidates report.\n\n\
            PERSISTING THESE: put them in the [links.auto] section of .hyalo.toml so they apply to every run:\n\
              [links.auto]\n\
              exclude_titles = [\"permissions\", \"README\"]\n\
              exclude_target_globs = [\"templates/*\"]\n\
              first_only = true\n\
              warn_common_titles = false   # opt out of the built-in stop-list AND its note\n\
            The two lists are UNIONED with the flags — --exclude-title/--exclude-target-glob extend the \
            config, they never replace it. --first-only turns first-only on for a single run whatever the \
            config says, and --no-first-only turns it off for a single run whatever the config says. \
            When config exclusions actually remove candidates, the report adds \
            config_excluded_titles (how many candidate titles the config took away) and \
            config_excluded_mentions (how many unlinked mentions those titles accounted for), \
            so a bare run stays explainable — one excluded title routinely suppresses hundreds \
            of mentions.\n\n\
            Without --apply, prints a dry-run report. Pass --apply to write changes.\n\n\
            OUTPUT: each proposed match carries file, line, col, matched_text and link_target. \
            `line` and `col` are both 1-based, and `col` counts Unicode scalar values (characters), \
            not bytes — the same convention as `lint`'s `column`, so a mention after an accented or \
            CJK character reports the column an editor shows. \
            `matched` is the proposal count and `scanned` the files examined; `dry_run` \
            says whether this was a preview, which `applied` alone cannot — an --apply \
            run that finds nothing to link also reports applied: false.\n\n\
            COMMON MISTAKES:\n\
            - --exclude-target-glob filters by file path, --exclude-title filters by title text. \
            Use --exclude-target-glob for directories (e.g. 'templates/*'), --exclude-title for words.\n\
            - Ambiguous titles (same title from 2+ files) are automatically skipped. Ambiguity is checked \
            in the namespace actually emitted, not just the title: two files with distinct titles but \
            the same filename stem (e.g. two `pulls.md` in different directories) are skipped too, since \
            writing the shared stem would be a link hyalo's own resolver would then call ambiguous. \
            Use --exclude-title to suppress specific titles, or rename one of the source files.\n\
            - Short titles match too aggressively. Use --min-length (default 3) to skip common short words.\n\
            - Without --first-only, every mention is linked. This can over-link — use --first-only for prose. \
            If the vault persists first_only = true, --no-first-only gets the all-mentions behaviour back \
            for one run."
    )]
    Auto {
        /// Preview changes without writing any files (the default; --apply writes)
        #[arg(long)]
        dry_run: bool,
        /// Apply changes to files on disk
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Minimum title length to consider (skip short common words)
        #[arg(long, value_name = "N", default_value = "3")]
        min_length: usize,
        /// Titles to exclude from matching (repeatable, case-insensitive)
        #[arg(long, value_name = "TITLE")]
        exclude_title: Vec<String>,
        /// Only emit the first match of each target title per source file
        ///
        /// An existing [[wikilink]] to a target anywhere in the file counts
        /// as its first mention (case-insensitive; aliased links count too).
        #[arg(long)]
        first_only: bool,
        /// Link every mention for this run, overriding `[links.auto] first_only = true`
        ///
        /// The counter-flag to --first-only: it forces first-only OFF for a single
        /// run without editing the config. Cannot be combined with --first-only.
        #[arg(long, conflicts_with = "first_only")]
        no_first_only: bool,
        /// Exclude target pages whose vault-relative path matches GLOB (repeatable)
        ///
        /// Matched case-insensitively, mirroring --exclude-title.
        #[arg(long, value_name = "GLOB")]
        exclude_target_glob: Vec<String>,
        /// Switch off the built-in common-title stop-list and its advisory note
        ///
        /// By default hyalo holds back candidate titles that are common English words,
        /// generic doc filenames or platform names (e.g. "permissions", "index",
        /// "github"), plus any title that dominates the run (at least 25 matches and
        /// 2.5% of the proposed links); the report names them under
        /// default_excluded_titles. This flag proposes every candidate instead.
        ///
        /// Persist the opt-out with `warn_common_titles = false` under
        /// [links.auto] in .hyalo.toml.
        #[arg(long)]
        no_warn_common_titles: bool,
        /// Restrict to a single file (vault-relative path)
        #[arg(long, value_name = "FILE", conflicts_with = "glob")]
        file: Option<String>,
        #[arg(
            short,
            long,
            value_name = "GLOB",
            conflicts_with = "file",
            help = GLOB_FLAG_SHORT_DOC,
            long_help = GLOB_FLAG_DOC,
        )]
        glob: Vec<String>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum TaskAction {
    /// Show task details for one or more tasks (read-only)
    #[command(long_about = "Show task details for one or more tasks.\n\n\
        INPUT: FILE (positional or --file) and one of: --line (repeatable), --section <heading>, or --all.\n\
        OUTPUT: wrapped in {\"results\": <task>, ...} envelope; single object for one task, array for multiple.\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to inspect task status before toggling or updating.\n\n\
        EXAMPLES:\n  \
          hyalo task read note.md --line 5\n  \
          hyalo task read note.md --line 5,7,9\n  \
          hyalo task read note.md --section Tasks\n  \
          hyalo task read note.md --all\n  \
          hyalo task read --file note.md --line 5\n  \
          hyalo task read --file iterations/iteration-206-planning.md --section Tasks")]
    Read {
        #[command(flatten)]
        selection: InputSelection,
        /// 1-based line number(s) in the WHOLE file, frontmatter counted (repeatable: 5,7,9)
        ///
        /// Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7. The numbering is
        /// file-absolute — the same one `find --fields tasks`, `lint` and `backlinks` report —
        /// so a line number copied out of any of them can be pasted here unchanged. Note that
        /// `read --lines` counts differently: it is relative to the body, with the frontmatter
        /// block excluded.
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all"])]
        line: Vec<usize>,
        #[arg(
            long,
            value_name = "HEADING",
            conflicts_with_all = ["line", "all"],
            help = TASK_SECTION_FLAG_SHORT_DOC,
            long_help = TASK_SECTION_FLAG_DOC,
        )]
        section: Option<String>,
        /// Select all tasks in the file
        #[arg(long, conflicts_with_all = ["line", "section"])]
        all: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Toggle task completion: [ ] -> [x], [x]/[X] -> [ ], custom -> [x]
    #[command(
        long_about = "Toggle task completion: [ ] -> [x], [x]/[X] -> [ ], custom -> [x].\n\n\
        INPUT: FILE (positional or --file) or --glob or --files-from, and one of: --line (repeatable), --section <heading>, or --all.\n\
        OUTPUT: wrapped in {\"results\": <task>, ...} envelope; single object for one task, array for multiple.\n\
        SIDE EFFECTS: Modifies the file(s) on disk (rewrites the checkbox character).\n\
        USE WHEN: You need to mark tasks as done or re-open completed tasks.\n\n\
        EXAMPLES:\n  \
          hyalo task toggle note.md --line 5\n  \
          hyalo task toggle note.md --line 5,7,9\n  \
          hyalo task toggle note.md --section Tasks\n  \
          hyalo task toggle note.md --all\n  \
          hyalo task toggle --file note.md --line 5\n  \
          hyalo task toggle --files-from list.txt --section Tasks\n  \
          hyalo task toggle --glob 'iterations/*.md' --all"
    )]
    Toggle {
        #[command(flatten)]
        selection: InputSelection,
        /// 1-based line number(s) in the WHOLE file, frontmatter counted (repeatable: 5,7,9)
        ///
        /// Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7. The numbering is
        /// file-absolute — the same one `find --fields tasks`, `lint` and `backlinks` report —
        /// so a line number copied out of any of them can be pasted here unchanged. Note that
        /// `read --lines` counts differently: it is relative to the body, with the frontmatter
        /// block excluded.
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all", "files_from"])]
        line: Vec<usize>,
        #[arg(
            long,
            value_name = "HEADING",
            conflicts_with_all = ["line", "all"],
            help = TASK_SECTION_FLAG_SHORT_DOC,
            long_help = TASK_SECTION_FLAG_DOC,
        )]
        section: Option<String>,
        /// Select all tasks in the file
        #[arg(long, conflicts_with_all = ["line", "section"])]
        all: bool,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Set a custom single-character status on one or more tasks
    #[command(
        name = "set",
        alias = "set-status",
        long_about = "Set a custom single-character status on one or more task checkboxes.\n\n\
        INPUT: FILE (positional or --file) or --glob or --files-from, --status (single char), and one of: --line (repeatable), --section <heading>, or --all.\n\
        OUTPUT: wrapped in {\"results\": <task>, ...} envelope; single object for one task, array for multiple.\n\
        SIDE EFFECTS: Modifies the file(s) on disk unless --dry-run is passed.\n\
        USE WHEN: You need to set a non-standard status like '?' (question), '-' (cancelled), or '!' (important).\n\n\
        EXAMPLES:\n  \
          hyalo task set note.md --line 5 --status '?'\n  \
          hyalo task set note.md --line 5,7 --status '-'\n  \
          hyalo task set note.md --section Tasks --status '-'\n  \
          hyalo task set note.md --all --status x\n  \
          hyalo task set --files-from list.txt --section Tasks --status '-'\n  \
          hyalo task set --glob 'iterations/*.md' --all --status x\n  \
          hyalo task set note.md --line 5 --status '?' --dry-run"
    )]
    Set {
        #[command(flatten)]
        selection: InputSelection,
        /// 1-based line number(s) in the WHOLE file, frontmatter counted (repeatable: 5,7,9)
        ///
        /// Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7. The numbering is
        /// file-absolute — the same one `find --fields tasks`, `lint` and `backlinks` report —
        /// so a line number copied out of any of them can be pasted here unchanged. Note that
        /// `read --lines` counts differently: it is relative to the body, with the frontmatter
        /// block excluded.
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all", "files_from"])]
        line: Vec<usize>,
        #[arg(
            long,
            value_name = "HEADING",
            conflicts_with_all = ["line", "all"],
            help = TASK_SECTION_FLAG_SHORT_DOC,
            long_help = TASK_SECTION_FLAG_DOC,
        )]
        section: Option<String>,
        /// Select all tasks in the file
        #[arg(long, conflicts_with_all = ["line", "section"])]
        all: bool,
        /// Single character to set as the task status (e.g. '?', '-', '!')
        #[arg(short, long)]
        status: String,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum PropertiesAction {
    /// Show unique property names with types and file counts (read-only)
    // `list` alias: the same read verb `types list` / `views list` use, so a
    // user who learned one group's verb can use it in the other (iter-192).
    #[command(
        visible_alias = "list",
        long_about = "Aggregate summary of frontmatter properties across matched files.\n\n\
        OUTPUT: List of unique property names, their inferred type, and how many files contain them.\n\
        SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to discover what properties exist or audit frontmatter across a vault."
    )]
    Summary {
        /// Glob pattern(s) to select files (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Maximum number of results to return (0 = unlimited).
        ///
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Rename a property key across all matched files
    #[command(
        long_about = "Rename a frontmatter property key across matched files.\n\n\
        Renames the key IN PLACE: it keeps its position in the block and its\n\
        value's exact source text — quoting, spacing, comments, block-list\n\
        indentation, and an empty value stays empty rather than becoming\n\
        `null`. Skips files where the target key already exists (conflict).\n\
        SIDE EFFECTS: Modifies matched files on disk."
    )]
    Rename {
        /// Existing property key to rename
        #[arg(long)]
        from: String,
        /// New property key
        #[arg(long)]
        to: String,
        /// Glob pattern(s) to scope which files to scan (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum TagsAction {
    /// Show unique tags with file counts (read-only)
    // `list` alias — see `PropertiesAction::Summary` (iter-192).
    #[command(
        visible_alias = "list",
        long_about = "Aggregate summary of tags across matched files.\n\n\
        OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
        SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to see which tags exist, find popular/orphan tags, or audit tag taxonomy."
    )]
    Summary {
        /// Glob pattern(s) to filter which files to scan, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Maximum number of results to return (0 = unlimited).
        ///
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Rename a tag across all matched files
    #[command(long_about = "Rename a tag across all matched files.\n\n\
        NESTED TAGS: renaming a parent renames its whole subtree (Obsidian\n\
        semantics) — `--from music --to audio` also moves `music/genres` to\n\
        `audio/genres`, and works even when the bare `music` tag appears\n\
        nowhere. The match lands on a `/` boundary, so `music` never matches\n\
        `musical`. Every tag actually renamed is listed in the text output and\n\
        under `renamed_tags` in JSON.\n\
        Atomic per-file: if the new tag already exists on a file, the renamed\n\
        duplicate is dropped rather than written twice.\n\
        SIDE EFFECTS: Modifies matched files on disk.")]
    Rename {
        /// Existing tag to rename
        #[arg(long)]
        from: String,
        /// New tag name
        #[arg(long)]
        to: String,
        /// Glob pattern(s) to scope which files to scan (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Preview changes without writing any files
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[cfg(test)]
mod tests {
    use super::{build_version_string, format_version_string};

    #[test]
    fn format_version_string_with_sha() {
        let s = format_version_string("0.16.0", "abc123def456", "2026-05-26");
        assert_eq!(s, "0.16.0 (abc123def456 2026-05-26)");
    }

    #[test]
    fn format_version_string_with_dirty_sha() {
        let s = format_version_string("0.16.0", "abc123def456+dirty", "2026-05-26");
        assert_eq!(s, "0.16.0 (abc123def456+dirty 2026-05-26)");
    }

    #[test]
    fn format_version_string_returns_pkg_version_when_sha_empty() {
        let s = format_version_string("0.16.0", "", "");
        assert_eq!(s, "0.16.0");
    }

    #[test]
    fn build_version_string_starts_with_pkg_version() {
        let v = build_version_string();
        assert!(
            v.starts_with(env!("CARGO_PKG_VERSION")),
            "version string {v:?} should start with CARGO_PKG_VERSION"
        );
    }
}
