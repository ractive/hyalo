use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use clap::{Args, Parser, Subcommand};

use crate::cli::inputs::InputSelection;
use crate::output::Format;

/// Shared `--file` doc string used on every command that accepts `--file`,
/// `--glob`, and `--files-from` as mutually exclusive input sources (NEW-4).
/// Keeping it in one place prevents future help-text drift across `find`,
/// `set`, `remove`, and `append`.
const FILE_FLAG_DOC: &str =
    "Target file(s) (repeatable). Mutually exclusive with --glob and --files-from";

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
    /// Use the snapshot index at `.hyalo-index` in the vault directory.
    ///
    /// Read-only commands (find, summary, tags summary, properties summary,
    /// backlinks) skip the disk scan entirely when the index is present.
    ///
    /// Mutation commands (set, remove, append, task, mv, tags rename,
    /// properties rename, links fix) still read/write individual files on disk
    /// but also patch the index entry in-place after each mutation — keeping
    /// the index current for subsequent queries.
    ///
    /// If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
    /// falls back to a full disk scan automatically.
    #[arg(long)]
    pub index: bool,

    /// Use the snapshot index at PATH instead of the default `.hyalo-index`.
    ///
    /// Implies `--index`. Relative paths are resolved against the current
    /// working directory (not the vault dir). Absolute paths are used as-is.
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

#[derive(Parser)]
#[command(
    name = "hyalo",
    version = build_version_string(),
    about = "Query, filter, and mutate YAML frontmatter across markdown file collections",
    long_about = "Hyalo — query, filter, and mutate YAML frontmatter across markdown file collections.\n\n\
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
        \u{00a0} {\"results\": <payload>, \"total\": N, \"hints\": [...]}\n\
        total is present for list commands (find, tags, properties, backlinks). \
        hints is always present (empty [] when --no-hints). \
        --jq operates on the full envelope, e.g. --jq '.results[].file' or --jq '.total'.\n\
        --count prints just the total as a bare integer (shortcut for --jq '.total').\n\
        Use --format text for human-readable output, --format json for machine-readable output. \
        Successful output goes to stdout; errors go to stderr with exit code 1 (user error) or 2 (internal error).\n\n\
        ABSOLUTE LINKS: Links like `/docs/page.md` are resolved by stripping a site prefix. \
        By default the prefix is auto-derived from --dir's last path component (e.g. --dir ../my-site/docs → prefix \"docs\"). \
        Override with --site-prefix <PREFIX>, or --site-prefix \"\" to disable. Also settable in .hyalo.toml. \
        For bundle-root resolution (e.g. OKF bundles where `/x/y.md` is relative to the bundle root), set `site_prefix = \"\"` so only the leading `/` is stripped — this also avoids mis-stripping when a bundle subdir shares its name with the vault dir.\n\n\
        CONFIG: Place a .hyalo.toml in the working directory to set defaults:\n\
        \u{00a0} dir = \"vault/\"        # default --dir\n\
        \u{00a0} format = \"text\"       # pin format regardless of TTY detection\n\
        \u{00a0} hints = false          # disable hints (CLI default is on)\n\
        \u{00a0} site_prefix = \"docs\"  # override auto-derived site prefix for absolute links\n\
        CLI flags always take precedence.\n\n\
        See COMMAND REFERENCE below for full syntax of each command."
)]
pub(crate) struct Cli {
    /// Root directory for resolving all file and --glob paths.
    /// Default: "." (Override via .hyalo.toml)
    #[arg(short, long, global = true)]
    pub dir: Option<PathBuf>,

    /// Output format: "json" or "text".
    /// Default: "text" when stdout is a terminal, "json" when piped.
    /// Override for a session via .hyalo.toml: format = "text"
    #[arg(long, global = true)]
    pub format: Option<Format>,

    /// Apply a jq filter expression to the JSON output of any command.
    /// Operates on the full JSON envelope: {"results": ..., "total": N, "hints": [...]}.
    /// The filtered result is printed as plain text. Incompatible with --format text.
    /// Example: --jq '.results[].file' or --jq '.results | map(.properties.status) | unique'.
    /// Note: recursive filters (e.g. 'recurse', '..') on large inputs may run indefinitely
    #[arg(long, global = true, value_name = "FILTER")]
    pub jq: Option<String>,

    /// Print only the total count as a bare integer for list commands
    /// (find, tags summary, properties summary, backlinks).
    /// Shortcut for --jq '.total'. Incompatible with --jq.
    #[arg(long, global = true)]
    pub count: bool,

    /// Force hints on (already the default).
    /// Text mode: '-> hyalo ...  # description' lines — concrete, copy-pasteable commands with descriptions.
    /// JSON mode: populates the "hints" array in the envelope (always present, empty when suppressed).
    /// Suppressed when --jq is active.
    #[arg(long, global = true)]
    pub hints: bool,

    /// Disable drill-down command hints (enabled by default).
    /// Override via .hyalo.toml: hints = false
    /// When both --hints and --no-hints are present, --hints takes precedence.
    #[arg(long, global = true)]
    pub no_hints: bool,

    /// Site prefix for resolving root-absolute links like `/docs/page.md`.
    ///
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
    /// or pass --site-prefix "" to disable absolute-link resolution entirely.
    ///
    /// Also settable via `site_prefix = "docs"` in .hyalo.toml.
    /// Precedence: --site-prefix flag > .hyalo.toml > auto-derived from --dir.
    #[arg(long, global = true, value_name = "PREFIX")]
    pub site_prefix: Option<String>,

    /// Suppress all warnings printed to stderr.
    ///
    /// Useful in scripts or CI pipelines where warning noise is undesirable.
    /// Identical warnings are always deduplicated regardless of this flag;
    /// use `--quiet` to suppress them entirely.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    /// Use the snapshot index at PATH (global alias for the per-subcommand `--index-file`).
    ///
    /// Equivalent to passing `--index-file PATH` after the subcommand.
    /// When both the global flag and the subcommand flag are provided, the
    /// subcommand value takes precedence.
    ///
    /// Relative paths are resolved against the current working directory.
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
    /// Regex body text search (case-insensitive by default; use (?-i) to override). Mutually exclusive with PATTERN
    #[arg(long, short = 'e', value_name = "REGEX")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regexp: Option<String>,
    /// Property filter: K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists), !K (absent), K~=pat or K~=/pat/i (regex). Repeatable (AND)
    #[arg(short, long = "property", value_name = "FILTER")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<String>,
    /// Tag filter: exact or prefix match (e.g. 'project' matches 'project/backend' but not 'projects'). Repeatable (AND)
    #[arg(short, long, value_name = "TAG")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tag: Vec<String>,
    /// Task presence filter: 'todo', 'done', 'any', or a single status character
    #[arg(long, value_name = "STATUS")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// Section heading filter: case-insensitive substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
    /// prefix '##' to pin heading level; use '/regex/' for regex (e.g. '/DEC-03[12]/'). Repeatable (OR)
    #[arg(short, long = "section", value_name = "HEADING")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<String>,
    #[arg(short, long, conflicts_with_all = ["glob", "files_from"], help = FILE_FLAG_DOC)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file: Vec<String>,
    /// Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate (e.g. '!**/draft-*')
    #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub glob: Vec<String>,
    /// Read file paths from PATH (one per line); use '-' to read from stdin.
    /// Mutually exclusive with --file and --glob.
    /// Non-.md paths and paths outside the vault are silently skipped (counters appear in JSON envelope).
    /// Repo-relative paths with the configured vault dir prefix (e.g. files/en-us/x.md with --dir files/en-us)
    /// are resolved by trying vault-relative first, then stripping the full dir prefix and retrying.
    /// Input is deduplicated; results follow first-seen order.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "glob"])]
    #[serde(skip)]
    pub files_from: Option<String>,
    /// Comma-separated list of optional fields to include: all, properties, properties-typed, tags, sections (alias: outline), tasks, links, backlinks, title (default: properties, tags, sections, links — excludes tasks, properties-typed, backlinks, and title). Use 'all' to include every field. 'file' and 'modified' are always included. 'properties' is a {key: value} map; 'properties-typed' is a [{name, type, value}] array; 'backlinks' requires scanning all files; 'title' is the frontmatter title property or first H1 heading (null if neither found). Note: in JSON output, `properties-typed` is serialized as `properties_typed` (underscore)
    #[arg(long, value_name = "FIELDS", use_value_delimiter = true)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Sort order: 'file' / 'path' (default), 'modified', 'backlinks_count', 'links_count', 'title', 'date', or 'property:<KEY>' for any frontmatter property
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    /// Reverse the sort order (ascending becomes descending and vice versa). Alias: --desc
    #[arg(long, alias = "desc")]
    #[serde(skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Maximum number of results to return (0 = unlimited).
    /// Default cap is bypassed when --jq or --count is used
    #[arg(short = 'n', long, value_parser = parse_limit)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Only return files with at least one unresolved link (auto-includes links field)
    #[arg(long)]
    #[serde(skip_serializing_if = "is_false")]
    pub broken_links: bool,
    /// Only return orphan files: no inbound links and no outbound links (auto-includes backlinks field)
    #[arg(long)]
    #[serde(skip_serializing_if = "is_false")]
    pub orphan: bool,
    /// Only return dead-end files: have inbound links but no outbound links (auto-includes links field)
    #[arg(long)]
    #[serde(skip_serializing_if = "is_false")]
    pub dead_end: bool,
    /// Filter by title: case-insensitive substring match against the displayed title
    /// (frontmatter 'title' property or first H1 heading). Use /regex/ for regex
    /// (e.g. '/^The/' or '/^The/i').
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Stemmer language for BM25 body search (also --stemmer). Selects Snowball stemmer for BM25
    /// tokenization — NOT markdown code-block language.
    /// Default: english. Accepts full names (english, german, …) or ISO 639-1 codes (en, de, …).
    /// Supported: arabic (ar), danish (da), dutch (nl), english (en), finnish (fi), french (fr),
    /// german (de), greek (el), hungarian (hu), italian (it), norwegian (no, nb, nn),
    /// portuguese (pt), romanian (ro), russian (ru), spanish (es), swedish (sv), tamil (ta),
    /// turkish (tr)
    #[arg(long, alias = "stemmer", value_name = "LANG")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
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
        self.fields.extend(overlay.fields.iter().cloned());
        if overlay.sort.is_some() {
            self.sort.clone_from(&overlay.sort);
        }
        self.reverse = self.reverse || overlay.reverse;
        if overlay.limit.is_some() {
            self.limit = overlay.limit;
        }
        self.broken_links = self.broken_links || overlay.broken_links;
        self.orphan = self.orphan || overlay.orphan;
        self.dead_end = self.dead_end || overlay.dead_end;
        if overlay.title.is_some() {
            self.title.clone_from(&overlay.title);
        }
        if overlay.language.is_some() {
            self.language.clone_from(&overlay.language);
        }
    }
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Search and filter markdown files — returns file objects with metadata, structure, tasks, and links
    #[command(long_about = "Search and filter markdown files.\n\n\
            Returns a JSON envelope: {\"results\": [...], \"total\": N, \"hints\": [...]}.\n\
            Each item in results contains the file path, modified time, \
            and optionally: frontmatter properties, tags, document sections, tasks, and links.\n\n\
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
            Language precedence: frontmatter > --language > config > english.\n\n\
            FILTERS: All filters are AND'd together.\n\
            - --property K=V: frontmatter property filter (supports =, !=, >, >=, <, <=, bare K for existence, !K for absence, K~=pattern or K~=/pattern/i for regex)\n\
            - --tag T: tag filter (exact or prefix via '/': 'project' matches 'project/backend' but NOT 'projects' — no substring or fuzzy matching)\n\
            - --task STATUS: task presence filter ('todo', 'done', 'any', or a single status char)\n\
            - --section HEADING: section scope filter (exclude files without a matching section; within \
            matching files, restrict tasks and content matches to the section scope; case-insensitive \
            substring (contains) match by default, e.g. 'Tasks' matches 'Tasks [4/4]'; use leading '#' \
            to pin heading level, e.g. '## Tasks'; use '/regex/' for regex matching). Repeatable (OR). \
            Nested subsections are included.\n\n\
            FIELDS: Use --fields to limit which fields appear (default: all). \
            Properties are a {key: value} map; use --fields properties-typed for [{name, type, value}] array.\n\
            JQ: --jq operates on the full envelope. Examples: --jq '.results[].file', --jq '.total'.\n\
            VIEWS: --view <name> loads a saved filter set from .hyalo.toml. Additional CLI flags \
            merge on top: list filters (--property, --tag, --section, --glob) extend the view's \
            lists; scalar filters (--regexp, --sort, --limit, --title, --task, --language) override; bool \
            flags (--broken-links, --orphan, --dead-end, --reverse) OR. Example: hyalo find --view drafts --limit 5\n\
            COMMON MISTAKES:\n\
            - Property regex uses ~= (tilde-equals), NOT =~ (Perl-style). Wrong: 'title=~/pat/', right: 'title~=/pat/'.\n\
            - --title searches the displayed title (frontmatter or H1); --property title~= only searches frontmatter.\n\
            - --tag uses prefix matching: 'project' matches 'project/backend' but NOT 'projects'.\n\
            POSITIONAL ARGUMENTS: The first positional argument is always PATTERN (body text search), not a file path. \
            Subsequent positional arguments are treated as FILE targets. \
            To filter by file without a body search, use --file instead of a positional argument.\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo find 'error handling'\n\
            \u{00a0} hyalo find --property status=draft --tag project\n\
            \u{00a0} hyalo find --property 'title~=/^Design/i'\n\
            \u{00a0} hyalo find --section 'Tasks' --task todo\n\
            \u{00a0} hyalo find --fields links --jq '[.results[] | select(.links | map(select(.path == null)) | length > 0)]'\n\
            \u{00a0} git diff --name-only origin/main | hyalo find --files-from -")]
    Find {
        /// BM25 ranked body text search with stemming (e.g. "running" matches "run", "ran"); results sorted by relevance
        #[arg(value_name = "PATTERN", conflicts_with = "regexp")]
        pattern: Option<String>,
        /// Target file(s) as positional args — alternative to --file (repeatable after PATTERN)
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file"])]
        file_positional: Vec<String>,
        /// Use a saved view (named filter set from .hyalo.toml). Additional CLI filters
        /// are merged on top: list filters (--property, --tag, --section, --glob) extend
        /// the view; scalar filters (--sort, --limit, --regexp, --title, --task) override it
        #[arg(long, value_name = "NAME")]
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
            Pass --format json to get {\"results\": {\"file\": \"...\", \"content\": \"...\"}, \"hints\": [...]}.\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo read notes/todo.md\n\
            \u{00a0} hyalo read --file notes/todo.md --section Tasks\n\
            \u{00a0} hyalo read --file notes/todo.md --lines 1:20\n\
            \u{00a0} hyalo read --file notes/todo.md --frontmatter --format json"
    )]
    Read {
        #[command(flatten)]
        selection: InputSelection,
        /// Extract section(s) by substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
        /// prefix '##' to pin heading level; use '/regex/' for regex. Nested subsections included
        #[arg(short, long, value_name = "HEADING")]
        section: Option<String>,
        /// Slice output by line range: 5:10, 5:, :10, or 5 (1-based, inclusive, relative to body content)
        #[arg(short, long)]
        lines: Option<String>,
        /// Include the YAML frontmatter in output
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
        \u{00a0} hyalo properties summary\n\
        \u{00a0} hyalo properties summary --glob 'research/**/*.md'\n\
        \u{00a0} hyalo properties rename --from old-key --to new-key")]
    Properties {
        #[command(subcommand)]
        action: Option<PropertiesAction>,
    },
    /// Tag operations: summary or bulk rename
    #[command(long_about = "Tag operations across matched files.\n\n\
        Subcommands:\n\
        - summary: Unique tags with file counts (read-only).\n\
        - rename: Rename a tag across files (mutates files).\n\n\
        EXAMPLES:\n\
        \u{00a0} hyalo tags summary\n\
        \u{00a0} hyalo tags summary --glob 'research/**/*.md'\n\
        \u{00a0} hyalo tags rename --from old-tag --to new-tag")]
    Tags {
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
            \u{00a0} hyalo task toggle todo.md --all\n\
            \u{00a0} hyalo task toggle todo.md --section Tasks\n\
            \u{00a0} hyalo task toggle todo.md --line 5,7,9\n\
            \u{00a0} hyalo task read todo.md --line 5\n\
            \u{00a0} hyalo task set todo.md --line 5 --status '-'")]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Show a compact vault summary: file counts, property/tag/status counts, tasks, links, orphans, dead-ends (read-only)
    #[command(
        long_about = "Show a compact vault summary (~20-30 lines regardless of vault size).\n\n\
            OUTPUT: A 'VaultSummary' object with file counts (total + top-level directories), \
            property summary (unique names/types/counts), tag summary (unique tags/counts), \
            status grouping (value + count, no file lists), \
            task counts (total/done), link health (total/broken count), \
            orphan count, dead-end count, and recently modified files.\n\
            Drill down with: hyalo find --orphan, --dead-end, --broken-links, --property status=X.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need a quick overview of a vault's metadata landscape.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo summary\n\
            \u{00a0} hyalo summary --format text\n\
            \u{00a0} hyalo summary --jq '.results.tasks.total'\n\
            \u{00a0} hyalo summary --jq '.results.links.broken'"
    )]
    Summary {
        /// Glob pattern(s) to filter which files to include, relative to --dir (repeatable); prefix '!' to negate (e.g. '!**/draft-*')
        #[arg(short, long)]
        glob: Vec<String>,
        /// Number of recent files to show (default: 10)
        #[arg(short = 'n', long, default_value = "10")]
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
            OUTPUT: JSON object with file, backlinks array (source, line, target, label), and total count.\n\
            SIDE EFFECTS: None (read-only).\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo backlinks decision-log.md\n\
            \u{00a0} hyalo backlinks --file notes/design.md\n\
            \u{00a0} hyalo backlinks --file notes/design.md --limit 20"
    )]
    Backlinks {
        #[command(flatten)]
        selection: InputSelection,
        /// Maximum number of backlinks to return (0 = unlimited).
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
            based on stem matching.\n\n\
            SINGLE-FILE MODE:\n\
            Provide a positional FILE or --file. --to accepts a .md path or an existing directory\n\
            (basename of source is appended). Applied immediately unless --dry-run is passed.\n\n\
            BATCH MODE (when --glob, --property, --tag, or --type is given):\n\
            Resolves a set of source files via the given selectors (intersection). --to must be a\n\
            directory (existing or trailing '/', no .md suffix). Defaults to dry-run; pass --apply\n\
            to commit changes. A single link-graph build covers all files.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo mv old.md --to new.md\n\
            \u{00a0} hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/\n\
            \u{00a0} hyalo mv --glob 'iterations/*.md' --property status=completed --to iterations/done/ --apply\n\
            \u{00a0} hyalo mv --tag archive --to archive/ --apply\n\n\
            OUTPUT: JSON object with moves, updated_files (with per-file replacements), totals, applied flag,\n\
            and skipped_ambiguous (list of links skipped due to ambiguous stem resolution).\n\n\
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
        /// Destination path: a .md path or an existing directory (basename appended) in single-file mode; a directory path in batch mode
        #[arg(long)]
        to: String,
        /// Glob pattern(s) to select source files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, value_name = "GLOB", conflicts_with = "files_from")]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin (batch mode).
        /// Mutually exclusive with --file, positional FILE, and --glob.
        /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
        /// Input is deduplicated; results follow first-seen order.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Property filter for source selection: K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists). Repeatable (AND)
        #[arg(short, long = "property", value_name = "FILTER")]
        properties: Vec<String>,
        /// Tag filter: exact or prefix match. Repeatable (AND)
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        /// Type filter: match files where frontmatter 'type' equals TYPE. Repeatable (AND)
        #[arg(long = "type", value_name = "TYPE")]
        r#type: Vec<String>,
        /// Preview changes without modifying any files (default behavior in batch mode without --apply)
        #[arg(long)]
        dry_run: bool,
        /// Commit changes in batch mode (required when using --glob/--property/--tag/--type)
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// How to handle destination basename collisions: 'error' (default) or 'skip'
        #[arg(long = "on-conflict", value_name = "POLICY", default_value = "error")]
        on_conflict: String,
        /// Allow rewriting bare wikilinks ([[note]]) even when the stem is ambiguous
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
            FILTERS (optional, narrow which files are mutated):\n\
            - --where-property FILTER: only mutate files whose frontmatter matches (same syntax as find --property: \
K=V, K!=V, K>=V, K<=V, K>V, K<V, or K for existence). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error \
            {\"error\": \"frontmatter would exceed size budget\", \"limit_bytes\": ..., \"would_be_bytes\": ..., \"file\": ...}.\n\
            USE WHEN: You need to create or overwrite frontmatter properties or add tags, \
            possibly across many files at once.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo set --property status=completed --file notes/todo.md\n\
            \u{00a0} hyalo set --property priority=3 --property reviewed=true --file notes/todo.md\n\
            \u{00a0} hyalo set --property tags=[a,b,c] --file notes/todo.md\n\
            \u{00a0} hyalo set --tag reviewed --glob 'research/**/*.md'\n\
            \u{00a0} hyalo set --property status=in-progress --where-property status=draft --glob '**/*.md'\n\
            \u{00a0} hyalo set --property due=2026-12-31 --validate --file notes/todo.md"
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
        #[arg(short, long, conflicts_with_all = ["glob", "files_from"], help = FILE_FLAG_DOC)]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        /// Mutually exclusive with --file, positional FILE, and --glob.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
        /// Validate new values against the schema from .hyalo.toml; reject writes that would
        /// create lint errors. Implied by `validate_on_write = true` in [schema] config.
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
K=V, K!=V, K>=V, K<=V, K>V, K<V, or K for existence). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error (see `hyalo set --help`).\n\
            USE WHEN: You need to delete properties or remove tags from one or more files.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo remove --property status --file notes/todo.md\n\
            \u{00a0} hyalo remove --property aliases=old-name --file notes/todo.md\n\
            \u{00a0} hyalo remove --tag draft --glob '**/*.md'\n\
            \u{00a0} hyalo remove --property status --where-tag archive --glob '**/*.md'"
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
        #[arg(short, long, conflicts_with_all = ["glob", "files_from"], help = FILE_FLAG_DOC)]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        /// Mutually exclusive with --file, positional FILE, and --glob.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
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
            Use the global --dir flag to specify the markdown directory to record in .hyalo.toml.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo init\n\
            \u{00a0} hyalo --dir kb init\n\
            \u{00a0} hyalo --dir kb init --claude\n\
            \u{00a0} hyalo --dir kb init --pi\n\
            \u{00a0} hyalo init --profile okf\n\
            \u{00a0} hyalo init --profile okf --claude\n\
            \u{00a0} hyalo init --profile madr\n\
            \u{00a0} hyalo init --profile skills\n\
            \u{00a0} hyalo init --profile changelog"
    )]
    Init {
        /// Set up Claude Code integration (skill + CLAUDE.md hint)
        #[arg(long)]
        claude: bool,
        /// Set up pi integration (skill + extension)
        #[arg(long)]
        pi: bool,
        /// Scaffold a preset vault flavour (okf, madr, skills, changelog) by
        /// merging an embedded config fragment into .hyalo.toml
        #[arg(long, value_name = "PROFILE")]
        profile: Option<String>,
    },
    /// Remove hyalo configuration and tool integration artifacts
    #[command(
        long_about = "Remove .hyalo.toml and all Claude Code / pi integration artifacts created by `init`.\n\n\
            Removes skills, rules, and the managed section from .claude/CLAUDE.md, and .pi/ directory.\n\
            Safe to run when artifacts are already absent (idempotent)."
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
            long as no external tool modifies vault files while the index is active.\n\n\
            PERFORMANCE: a body-text query combined with a narrow metadata filter\n\
            (e.g. `find \"query\" --property status=x`) still reads the whole vault\n\
            without an index, because BM25 relevance is ranked against full-vault\n\
            statistics. On large vaults, create an index for this workload.\n\n\
            OUTPUT: JSON object with `path`, `files_indexed`, and `warnings`.\n\
            SIDE EFFECTS: Writes a binary file (default: .hyalo-index in --dir).\n\n\
            FLAG ALIASES: on this subcommand, `--index-file PATH` (the global flag) is\n\
            accepted as a synonym for `-o / --output PATH`. If both are provided and\n\
            differ, create-index returns an error.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo create-index\n\
            \u{00a0} hyalo create-index -o /tmp/my-index\n\
            \u{00a0} hyalo find --property status=draft --index"
    )]
    CreateIndex {
        /// Output path for the index file (default: .hyalo-index in --dir).
        /// Equivalent to the global --index-file flag on this subcommand.
        #[arg(short, long)]
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
        /// Path to the index file to delete (default: .hyalo-index in --dir)
        #[arg(short, long)]
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
K=V, K!=V, K>=V, K<=V, K>V, K<V, or K for existence). Quote filters containing > or < to prevent \
shell redirection (e.g. --where-property 'priority>=3'). If the property is a list, matches if any \
element matches. Repeatable (AND).\n\
            - --where-tag T: only mutate files with this tag (nested matching: 'project' matches 'project/backend'). \
Repeatable (AND).\n\
            SIDE EFFECTS: Modifies matched files on disk (unless --dry-run is passed).\n\
            SIZE LIMIT: frontmatter is limited to 64 KiB / 2000 lines. A write that would exceed \
            this limit is rejected with exit 1 and a JSON error (see `hyalo set --help`).\n\
            USE WHEN: You need to append items to list-type properties such as 'aliases' or 'authors' \
            without overwriting the existing list.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo append --property aliases='My Note' --file note.md\n\
            \u{00a0} hyalo append --property authors=alice --glob 'research/**/*.md'\n\
            \u{00a0} hyalo append --property related=other.md --where-tag project --glob '**/*.md'"
    )]
    Append {
        /// Target file(s) as positional argument(s) — alternative to --file
        #[arg(value_name = "FILE", conflicts_with_all = ["glob", "file", "files_from"])]
        file_positional: Vec<String>,
        /// Property to append to: K=V. Repeatable
        #[arg(short, long = "property", value_name = "K=V", required = true)]
        properties: Vec<String>,
        #[arg(short, long, conflicts_with_all = ["glob", "files_from"], help = FILE_FLAG_DOC)]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "files_from"])]
        glob: Vec<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        /// Mutually exclusive with --file, positional FILE, and --glob.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob"])]
        files_from: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
        /// Validate new values against the schema from .hyalo.toml; reject writes that would
        /// create lint errors. Implied by `validate_on_write = true` in [schema] config.
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
            SIDE EFFECTS: 'set' and 'remove' modify .hyalo.toml. 'list' is read-only.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo views list\n\
            \u{00a0} hyalo views set drafts --property status=draft --tag project\n\
            \u{00a0} hyalo find --view drafts --limit 5\n\
            \u{00a0} hyalo views remove drafts"
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
            OUTPUT: JSON object with broken/fixable/unfixable counts, per-fix details \
            (source, line, old_target, new_target, strategy, confidence), and \
            the list of links that could not be matched. With --apply it also \
            reports applied_fixes (fixes actually written to disk) plus \
            unapplied/unapplied_fixes for plans whose on-disk text no longer \
            matched — only applied_fixes were durably written.\n\
            SIDE EFFECTS: None unless `links fix --apply` is passed.\n\n\
            TIP: For read-only auditing, use 'hyalo summary' (link health overview)\n\
            or 'hyalo find --broken-links' (list files with unresolved links).\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo links fix\n\
            \u{00a0} hyalo links fix --apply\n\
            \u{00a0} hyalo links auto --first-only --apply\n\
            \u{00a0} hyalo links auto --min-length 5 --exclude-target-glob 'templates/*' --apply"
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
            \u{00a0}         `item_pattern` violation on `string-list` items, missing `required-sections`,\n\
            \u{00a0}         empty value on a required property (see REQUIRED EMPTINESS below)\n\
            - warn:  no 'type' property, property not declared in schema\n\
            When no `[schema]` section exists, this pass exits 0 with zero violations.\n\
            Schema extensions `item_pattern` (per-item regex on `string-list` properties) and\n\
            `required-sections` (required body outline on type schemas) are validated here.\n\n\
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
            \u{00a0} - HYALO001: bare `[]` should be `- [ ]` (autofixable)\n\
            \u{00a0} - HYALO002: `status: completed` requires all task checkboxes ticked\n\
            \u{00a0}            (only fires when the schema declares `status` as an enum\n\
            \u{00a0}            containing `completed`)\n\
            \u{00a0} - HYALO005: frontmatter that cannot be parsed (invalid YAML, duplicate keys,\n\
            \u{00a0}            oversized scalar) — error by default; the file still counts in\n\
            \u{00a0}            `files_checked` so a corrupt file can never leave a green lint.\n\
            \u{00a0}            Severity is configurable via `[lint.rules.HYALO005]` but no\n\
            \u{00a0}            profile downgrades it.\n\
            Severity is hyalo-controlled. Manage rule enable/severity with `hyalo lint-rules`.\n\
            Override defaults via `[lint]` and `[lint.rules]` in `.hyalo.toml`.\n\n\
            INPUT: Optional FILE (positional or --file) or --glob to narrow scope.\n\
            Without any file arguments, the entire vault is linted.\n\n\
            OUTPUT: Text by default — summary mode groups violations by `(file, rule)` and caps\n\
            output at 3 violations per rule and 50 files (configurable via `[lint]` and\n\
            `--max-per-rule`). Use --detailed for full per-violation output. Use --format json\n\
            for a JSON payload with `rule_groups`, `total`, `rules_fired`,\n\
            `files_with_violations`, and `files_truncated`. The `errors`/`warnings` counters\n\
            and the exit code always reflect the WHOLE vault, never just the displayed slice —\n\
            a file cap can never mask an error.\n\
            LIMIT: --limit/-n N caps the displayed files[]; `--limit 0` means UNLIMITED (lift\n\
            the cap entirely, matching `--count --limit 0`) — it never empties the list.\n\n\
            SKIP VISIBILITY: with `--files-from`, dropped input paths (missing / non-markdown)\n\
            are reported as a `note:` line (--format text, on stderr) or a `::notice::` (--format\n\
            github), matching the `files_missing`/`files_skipped_*` counters in the JSON envelope.\n\
            An explicitly named `--file` excluded by `[lint] ignore` also prints a notice rather\n\
            than silently reporting `0 files checked`.\n\n\
            GITHUB ANNOTATIONS: --format github (lint-only) emits one GitHub Actions workflow\n\
            command per violation — `::error file=<path>,line=<line>,title=<RULE_ID>::<message>`\n\
            (warnings use `::warning`) — so findings render as inline annotations on the PR diff,\n\
            followed by a one-line `N errors, M warnings in K files` summary. Annotations are\n\
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
            \u{00a0} --rule <ID>             restrict to a single rule\n\
            \u{00a0} --rule-prefix <PREFIX>  restrict to rules with this prefix (e.g. HYALO)\n\
            \u{00a0} --max-per-rule <N>      override per-rule cap (0 = unlimited)\n\n\
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
            reported as conflicts. Use --fix-rule <ID> (repeatable) to limit which rules autofix,\n\
            or --dry-run to preview without writing.\n\n\
            EXIT CODES: 0 = clean (after fixes), 1 = errors remain, 2 = internal error.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo lint\n\
            \u{00a0} hyalo lint --detailed\n\
            \u{00a0} hyalo lint --rule MD013 --detailed\n\
            \u{00a0} hyalo lint --rule-prefix HYALO\n\
            \u{00a0} hyalo lint --max-per-rule 0\n\
            \u{00a0} hyalo lint --fix --dry-run\n\
            \u{00a0} hyalo lint --fix-rule HYALO001\n\
            \u{00a0} hyalo lint --fix\n\
            \u{00a0} hyalo lint --profile okf         # validate OKF bundle conformance\n\
            \u{00a0} hyalo lint --profile skills      # validate a directory of SKILL.md skills\n\
            \u{00a0} hyalo lint --profile changelog   # validate CHANGELOG.md (Keep a Changelog)\n\
            \u{00a0} hyalo lint --strict --format github   # inline PR annotations in GitHub Actions\n\n\
            INDEX NOTE: The snapshot index does not accelerate the body pass — body bytes are\n\
            not indexed. The frontmatter pass and file enumeration still benefit from --index.\n\n\
            SIDE EFFECTS: None without --fix. With --fix (and without --dry-run), mutated files\n\
            are rewritten atomically and the snapshot index is patched in-place.\n\n\
            TIP: Run `hyalo summary` to see a one-line lint count across the whole vault."
    )]
    Lint {
        /// Target file (relative to --dir) — positional form
        #[arg(value_name = "FILE", conflicts_with_all = ["file", "glob", "type", "files_from"])]
        file_positional: Option<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with_all = ["glob", "type", "files_from"])]
        file: Vec<String>,
        /// Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with_all = ["file", "type", "files_from"])]
        glob: Vec<String>,
        /// Restrict linting to files matching the named type's filename template.
        /// Equivalent to --glob <template-as-glob>. Mutually exclusive with --file, --glob, and --files-from.
        #[arg(long = "type", conflicts_with_all = ["file", "glob", "file_positional", "files_from"])]
        r#type: Option<String>,
        /// Read file paths from PATH (one per line); use '-' to read from stdin.
        /// Mutually exclusive with --file, positional FILE, --glob, and --type.
        /// Repo-relative paths with the configured vault dir prefix are resolved automatically.
        /// Input is deduplicated; results follow first-seen order.
        #[arg(long, value_name = "PATH", conflicts_with_all = ["file", "file_positional", "glob", "type"])]
        files_from: Option<String>,
        /// Auto-remediate fixable violations (defaults, enum typos, date format, type inference)
        #[arg(long)]
        fix: bool,
        /// With --fix, preview changes without writing files
        #[arg(long, requires = "fix")]
        dry_run: bool,
        /// Maximum number of files to include in output.
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
        /// Only autofix the specified rule(s) — repeatable
        #[arg(long, value_name = "RULE_ID", requires = "fix")]
        fix_rule: Vec<String>,
        /// Promote schema warnings to errors: "no 'type' property",
        /// "undeclared property in frontmatter", and date-format violations
        /// (HYALO003), causing lint to exit non-zero when those issues are found.
        ///
        /// Note: missing-type and undeclared-property promotions require a
        /// `[schema.types.*]` block in `.hyalo.toml` — on a schema-less vault
        /// these warnings are never emitted and `--strict` has no visible effect
        /// for those checks.
        ///
        /// Overrides `[lint] strict` in `.hyalo.toml` for this invocation.
        #[arg(long)]
        strict: bool,
        /// Overlay a named conformance profile for this invocation only (no
        /// `.hyalo.toml` change). `okf` enables the Open Knowledge Format §9
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
            \u{00a0} hyalo lint-rules list\n\
            \u{00a0} hyalo lint-rules show MD013\n\
            \u{00a0} hyalo lint-rules set MD013 --enabled false\n\
            \u{00a0} hyalo lint-rules set HYALO002 --severity error\n\
            \u{00a0} hyalo lint-rules remove MD013"
    )]
    LintRules {
        #[command(subcommand)]
        action: Option<LintRulesAction>,
    },
    /// Manage document-type schemas in `.hyalo.toml`
    #[command(
        long_about = "Manage document-type schemas stored in `.hyalo.toml`.\n\n            Type schemas define required properties, default values, property constraints,\n            and filename templates for each document type.\n\n            Calling `hyalo types` without a subcommand defaults to `hyalo types list`.\n\n            Subcommands:\n            - list:   Show all defined types and their required fields (default).\n            - show:   Show the full schema for a single type.\n            - remove: Delete a type entry.\n            - set:    Create or update a type schema (upsert). Auto-creates the type if it doesn't exist.\n\n            TOML editing preserves comments and formatting.\n\n            SIDE EFFECTS: remove/set modify .hyalo.toml. list and show are read-only.\n\n            EXAMPLES:\n            \u{00a0} hyalo types list\n            \u{00a0} hyalo types show iteration\n            \u{00a0} hyalo types set note --required title,date\n            \u{00a0} hyalo types set iteration --property-type status=enum --property-values status=planned,in-progress,completed\n            \u{00a0} hyalo types remove draft"
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
            `hyalo lint`, driving the agent fill-in loop.\n\n\
            CONSTRAINTS:\n\
            - Refuses with an error if the target file already exists\n\
            - `--file` must be vault-relative (no leading `/`, no `..` components)\n\
            - Frontmatter is limited to 64 KiB / 2000 lines; schema templates exceeding this are rejected\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo new --type iteration --file iterations/iter-99-example.md\n\
            \u{00a0} hyalo new --type note --file notes/2026-05-24-standup.md\n\n\
            OUTPUT: JSON envelope `{\"results\": {\"type\": ..., \"file\": ..., \"created\": true}}`.\n\
            Text mode: `created <rel-path>`.\n\n\
            SIDE EFFECTS: Writes one new file. When `--index` or `--index-file` is set,\n\
            also inserts a fresh entry into the snapshot index so subsequent `--index`\n\
            queries (find, summary, etc.) see the file without a full rebuild."
    )]
    New {
        /// Document type to scaffold (must exist in `[schema.types.*]`)
        #[arg(long, value_name = "TYPE", required = true)]
        r#type: String,
        /// Vault-relative path for the new file (must not exist; parent dirs created if missing)
        #[arg(long, value_name = "FILE", required = true)]
        file: String,
        /// When `--index` or `--index-file` is set, patch the snapshot index in
        /// place after the file is created so later `--index` queries see it
        /// without a full rebuild.
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
            \u{00a0} hyalo okf index --dry-run          # CI: fail if index.md files are stale\n\
            \u{00a0} hyalo okf index --apply            # regenerate all index.md files\n\
            \u{00a0} hyalo okf index tables --apply     # scope to a subtree\n\
            \u{00a0} hyalo okf log --message \"Added blocks table\" --apply\n\
            \u{00a0} hyalo okf log tables --action Update --message \"...\" --apply\n\
            \u{00a0} hyalo lint --profile okf           # validate bundle conformance"
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
            \u{00a0} hyalo madr toc --dry-run              # CI: fail if the TOC is stale\n\
            \u{00a0} hyalo madr toc --apply                # regenerate docs/decisions/README.md\n\
            \u{00a0} hyalo madr toc docs/adr --apply       # scope to a custom ADR directory"
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
            VALIDATE: after releasing, replace the `TBD` link target with the real\n\
            compare/tag URL and run `hyalo lint --profile changelog`.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo changelog add --category Added --message \"New export format\" --apply\n\
            \u{00a0} hyalo changelog release 1.2.0 --dry-run\n\
            \u{00a0} hyalo changelog release 1.2.0 --apply\n\
            \u{00a0} hyalo changelog release 1.2.0 --date 2026-07-17 --apply"
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
            Shows which .hyalo.toml is active (or none), its raw contents, and the effective\n\
            values: config_path, cwd, dir, format, hints, site_prefix, exempt.\n\n\
            OUTPUT: Line-by-line in text format; JSON object with --format json.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Config,
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
            \u{00a0} bash:        hyalo completions bash  > ~/.local/share/bash-completion/completions/hyalo\n\
            \u{00a0} zsh:         hyalo completions zsh   > ~/.local/share/zsh/site-functions/_hyalo\n\
            \u{00a0} fish:        hyalo completions fish  > ~/.config/fish/completions/hyalo.fish\n\
            \u{00a0} elvish:      hyalo completions elvish > ~/.config/elvish/lib/completions/hyalo.elv\n\
            \u{00a0} powershell:  hyalo completions powershell > _hyalo.ps1\n\n\
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
            `index.md`'s lone `okf_version` frontmatter key is preserved.\n\n\
            NON-DESTRUCTIVE ADOPT: an existing `index.md` WITHOUT markers is *adopted* —\n\
            its entire hand-written body is preserved and the managed region is appended\n\
            after it (dry-run reports `adopt (preserving N existing lines)`). Pass\n\
            --replace to instead overwrite such a file with a fresh managed index,\n\
            discarding its body. On case-insensitive filesystems an existing `INDEX.md`\n\
            is recognized as the reserved file and adopted by its on-disk casing.\n\n\
            SCOPING: files matching a `[okf] ignore` glob in `.hyalo.toml` (e.g.\n\
            `_template/**`) are neither indexed nor generated into. A concept with\n\
            unparseable frontmatter is skipped with a stderr warning (the run continues\n\
            and every other index is still generated).\n\n\
            Running with --apply twice is a no-op (idempotent). In --dry-run (the default)\n\
            the command exits non-zero when any `index.md` would change — use this in CI.\n\n\
            SIDE EFFECTS: writes `index.md` files only with --apply.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo okf index --dry-run\n\
            \u{00a0} hyalo okf index --apply\n\
            \u{00a0} hyalo okf index tables --apply\n\
            \u{00a0} hyalo okf index --apply --replace   # overwrite marker-less index.md"
    )]
    Index {
        /// Optional directory (vault-relative) to scope regeneration to a subtree
        #[arg(value_name = "DIR")]
        scope: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing (default behaviour; explicit for clarity)
        #[arg(long)]
        dry_run: bool,
        /// Overwrite a marker-less `index.md`, discarding its hand-written body.
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
            action word (`- **Update:** ...`), a convention (not required per §7). The file\n\
            is created (with no frontmatter — it is a reserved file) when absent.\n\n\
            TARGET is validated to stay inside the vault/bundle; paths that escape are\n\
            rejected. Defaults to --dry-run; pass --apply to write.\n\n\
            SIDE EFFECTS: writes/creates one `log.md` only with --apply.\n\n\
            EXAMPLES:\n\
            \u{00a0} hyalo okf log --message \"Added blocks table\" --apply\n\
            \u{00a0} hyalo okf log tables --action Update --message \"Refreshed schema\" --apply\n\
            \u{00a0} hyalo okf log tables/log.md --message \"...\" --apply"
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
        /// Preview changes without writing (default behaviour; explicit for clarity)
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
            EXAMPLES:\n\
            \u{00a0} hyalo madr toc --dry-run\n\
            \u{00a0} hyalo madr toc --apply\n\
            \u{00a0} hyalo madr toc docs/adr --apply"
    )]
    Toc {
        /// ADR directory (vault-relative); defaults to `docs/decisions`
        #[arg(value_name = "DIR")]
        adr_dir: Option<String>,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing (default behaviour; explicit for clarity)
        #[arg(long)]
        dry_run: bool,
        /// Overwrite a marker-less `README.md`, discarding its hand-written body.
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
            \u{00a0} hyalo changelog release 1.2.0 --dry-run\n\
            \u{00a0} hyalo changelog release 1.2.0 --apply\n\
            \u{00a0} hyalo changelog release 1.2.0 --date 2026-07-17 --apply"
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
        /// Preview changes without writing (default behaviour; explicit for clarity)
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
            \u{00a0} hyalo changelog add --category Added --message \"New export format\" --apply\n\
            \u{00a0} hyalo changelog add --category Fixed --message \"Crash on empty input\" --apply"
    )]
    Add {
        /// Change category (Added/Changed/Deprecated/Removed/Fixed/Security)
        #[arg(long, value_name = "CATEGORY")]
        category: String,
        /// The entry text (required)
        #[arg(long, value_name = "TEXT", required = true)]
        message: String,
        /// Write changes to disk. Without this flag the command is a dry run.
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Preview changes without writing (default behaviour; explicit for clarity)
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // Set variant holds FindFilters by design; boxing would complicate dispatch
pub(crate) enum ViewsAction {
    /// List all saved views
    #[command(
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
        SIDE EFFECTS: Modifies .hyalo.toml.")]
    Set {
        /// View name (first positional arg)
        #[arg(value_name = "NAME")]
        name: String,
        /// Optional BM25 search pattern to save with the view (second positional arg).
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
    #[command(
        external_subcommand = false,
        long_about = "Run a saved view as if you called `hyalo find --view <NAME>`.\n\n\
            Extra find flags passed after the view name extend or override the saved filters.\n\n\
            SIDE EFFECTS: None (read-only find)."
    )]
    Run {
        /// View name to run
        #[arg(value_name = "NAME")]
        name: String,
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
    #[command(
        long_about = "List all type schemas defined in `.hyalo.toml`.\n\n            OUTPUT: JSON envelope with results array and total count.\n            SIDE EFFECTS: None (read-only)."
    )]
    List,
    /// Show the full schema for a single type
    #[command(
        long_about = "Display the full merged schema for a named type.\n\n            OUTPUT: JSON object with type name, required fields, defaults,\n            filename template, property constraints, and required_sections.\n\
            When declared, `item_pattern` is included in the constraint for `string-list` properties,\n\
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
        long_about = "Create or update a type schema in `.hyalo.toml`. If the type doesn't exist, it is created automatically.\n\n            When creating the first type (i.e. the [schema] section is new), `validate_on_write = true` is set automatically so that `set`/`append` enforce schema constraints by default.\n\n            All mutation flags are optional and combinable in a single invocation.\n\n            FLAGS:\n            - --required <fields>: comma-separated required property names to add (repeatable).\n            - --default key=value: set a default; auto-applied to files missing the property.\n            - --property-type key=type: set a type constraint (string/date/datetime/datetime-tz/number/boolean/list/enum). `datetime-tz` accepts RFC 3339 timezone-aware values (e.g. 2026-05-28T22:44:47+00:00 or ...Z); `datetime` stays naive (no offset).\n            - --property-values key=val1,val2,...: set enum values; implies type=enum.\n            - --filename-template <template>: set the filename template for this type.\n            - --dry-run: preview changes without writing anything.\n\n            OUTPUT: JSON result with action, dry_run, defaults_applied, constraint_violations.\n            SIDE EFFECTS: Modifies .hyalo.toml and may write to vault files (unless --dry-run)."
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
        /// Preview changes without writing to .hyalo.toml
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a rule override (revert to default)
    Remove {
        /// Rule ID to reset
        #[arg(value_name = "RULE_ID")]
        rule_id: String,
        /// Preview changes without writing to .hyalo.toml
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
            3. Unique stem match anywhere in the vault (shortest-path)\n\
            4. Jaro-Winkler fuzzy match above --threshold\n\n\
            Use --apply to write fixes to disk. Without --apply, only a dry-run report is printed.\n\n\
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
            \"auto\" (the default) enables this automatically.")]
    Fix {
        /// Preview changes without modifying files (default when --apply is omitted)
        #[arg(long)]
        dry_run: bool,
        /// Apply fixes to files on disk
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Minimum similarity threshold for fuzzy matching (0.0–1.0)
        #[arg(long, default_value = "0.8", value_parser = parse_threshold)]
        threshold: f64,
        /// Glob pattern(s) to filter which files to check, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Ignore broken links whose target contains any of these substrings (repeatable).
        /// Useful for skipping Hugo template links, external paths, etc.
        #[arg(long)]
        ignore_target: Vec<String>,
        /// Expand short-form wikilinks ([[Name]]) to their full vault path when applying fixes.
        ///
        /// By default, hyalo treats bare stem wikilinks as valid Obsidian short-form links:
        /// [[Corina]] that resolves to sub/Corina.md is left untouched. With this flag,
        /// such links are expanded to [[sub/Corina]] on --apply. NOTE: this breaks
        /// Obsidian compatibility — Obsidian resolves short-form links by stem across the
        /// whole vault and does not require the full path.
        #[arg(long)]
        expand_short_form: bool,
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
            Exclusion zones: frontmatter, fenced code blocks, inline code,\n\
            existing [[wikilinks]] and [markdown](links), headings, comment fences (%%), self-links.\n\n\
            Filtering options:\n\
            --first-only          Only emit the first mention of each target per source file. An\n\
            \u{00a0}                      existing [[wikilink]] (or aliased [[target|label]]) to a target\n\
            \u{00a0}                      anywhere in the file counts as its first mention, case-insensitively\n\
            \u{00a0}                      — no new match is emitted for that target in that file even if a\n\
            \u{00a0}                      plain-text mention appears earlier in the file than the link.\n\
            --exclude-title       Exclude specific titles (repeatable, case-insensitive)\n\
            --exclude-target-glob Exclude target pages by vault-relative path glob (repeatable)\n\n\
            Without --apply, prints a dry-run report. Pass --apply to write changes.\n\n\
            COMMON MISTAKES:\n\
            - --exclude-target-glob filters by file path, --exclude-title filters by title text. \
            Use --exclude-target-glob for directories (e.g. 'templates/*'), --exclude-title for words.\n\
            - Ambiguous titles (same title from 2+ files) are automatically skipped. Use --exclude-title \
            to suppress specific titles, or rename one of the source files.\n\
            - Short titles match too aggressively. Use --min-length (default 3) to skip common short words.\n\
            - Without --first-only, every mention is linked. This can over-link — use --first-only for prose."
    )]
    Auto {
        /// Preview changes without modifying files (default when --apply is omitted)
        #[arg(long)]
        dry_run: bool,
        /// Apply changes to files on disk
        #[arg(long, conflicts_with = "dry_run")]
        apply: bool,
        /// Minimum title length to consider (skip short common words)
        #[arg(long, default_value = "3")]
        min_length: usize,
        /// Titles to exclude from matching (repeatable, case-insensitive)
        #[arg(long)]
        exclude_title: Vec<String>,
        /// Only emit the first match of each target title per source file.
        /// An existing [[wikilink]] to a target anywhere in the file counts
        /// as its first mention (case-insensitive; aliased links count too).
        #[arg(long)]
        first_only: bool,
        /// Exclude target pages whose vault-relative path matches a glob pattern (repeatable)
        #[arg(long)]
        exclude_target_glob: Vec<String>,
        /// Restrict to a single file (vault-relative path)
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern(s) to filter which files to scan (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
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
          hyalo task read --file note.md --line 5")]
    Read {
        #[command(flatten)]
        selection: InputSelection,
        /// 1-based line number(s). Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all"])]
        line: Vec<usize>,
        /// Select all tasks under a heading (case-insensitive substring, ##-pinned, or /regex/)
        #[arg(long, conflicts_with_all = ["line", "all"])]
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
        /// 1-based line number(s). Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all", "files_from"])]
        line: Vec<usize>,
        /// Select all tasks under a heading (case-insensitive substring, ##-pinned, or /regex/)
        #[arg(long, conflicts_with_all = ["line", "all"])]
        section: Option<String>,
        /// Select all tasks in the file
        #[arg(long, conflicts_with_all = ["line", "section"])]
        all: bool,
        /// Preview the toggle result without writing the file
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
        /// 1-based line number(s). Comma-separated or repeatable: --line 5,7,9 or --line 5 --line 7
        #[arg(short, long, value_delimiter = ',', action = clap::ArgAction::Append, conflicts_with_all = ["section", "all", "files_from"])]
        line: Vec<usize>,
        /// Select all tasks under a heading (case-insensitive substring, ##-pinned, or /regex/)
        #[arg(long, conflicts_with_all = ["line", "all"])]
        section: Option<String>,
        /// Select all tasks in the file
        #[arg(long, conflicts_with_all = ["line", "section"])]
        all: bool,
        /// Single character to set as the task status (e.g. '?', '-', '!')
        #[arg(short, long)]
        status: String,
        /// Preview which tasks would be changed without modifying the file
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum PropertiesAction {
    /// Show unique property names with types and file counts (read-only)
    #[command(
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
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Rename a property key across all matched files
    #[command(
        long_about = "Rename a frontmatter property key across matched files.\n\n\
        Preserves the value and type. Skips files where the target key already exists (conflict).\n\
        SIDE EFFECTS: Modifies matched files on disk."
    )]
    Rename {
        /// Property key to rename from
        #[arg(long)]
        from: String,
        /// Property key to rename to
        #[arg(long)]
        to: String,
        /// Glob pattern(s) to scope which files to scan (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Preview changes without writing to disk
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
}

#[derive(Subcommand)]
pub(crate) enum TagsAction {
    /// Show unique tags with file counts (read-only)
    #[command(long_about = "Aggregate summary of tags across matched files.\n\n\
        OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
        SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to see which tags exist, find popular/orphan tags, or audit tag taxonomy.")]
    Summary {
        /// Glob pattern(s) to filter which files to scan, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Maximum number of results to return (0 = unlimited).
        /// Default cap is bypassed when --jq or --count is used
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        #[command(flatten)]
        index_flags: IndexFlags,
    },
    /// Rename a tag across all matched files
    #[command(long_about = "Rename a tag across all matched files.\n\n\
        Atomic per-file: if the new tag already exists on a file, only the old tag is removed.\n\
        SIDE EFFECTS: Modifies matched files on disk.")]
    Rename {
        /// Tag to rename from
        #[arg(long)]
        from: String,
        /// Tag to rename to
        #[arg(long)]
        to: String,
        /// Glob pattern(s) to scope which files to scan (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Preview changes without writing to disk
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
