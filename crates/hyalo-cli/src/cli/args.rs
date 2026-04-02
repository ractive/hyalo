use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::output::Format;

#[allow(clippy::trivially_copy_pass_by_ref)] // serde skip_serializing_if requires &bool
pub(crate) fn is_false(v: &bool) -> bool {
    !v
}

pub(crate) fn parse_limit(s: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;
    if n == 0 {
        return Err("limit must be at least 1".to_owned());
    }
    Ok(n)
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

#[derive(Parser)]
#[command(
    name = "hyalo",
    version,
    about = "Query, filter, and mutate YAML frontmatter across markdown file collections",
    long_about = "Hyalo — query, filter, and mutate YAML frontmatter across markdown file collections.\n\n\
        Compatible with Obsidian vaults, Zettelkasten systems, and any directory of .md files \
        with YAML frontmatter. Also resolves [[wikilinks]] and manages task checkboxes.\n\n\
        SCOPE: Hyalo operates on a directory of .md files. It can query and mutate frontmatter \
        properties, tags, tasks, and links.\n\n\
        PATH RESOLUTION: All --file and --glob paths are relative to --dir (defaults to \".\"). \
        Globs use standard syntax: '**/*.md' matches recursively, 'notes/*.md' matches one level.\n\n\
        OUTPUT: Returns JSON by default (--format json). All JSON is wrapped in a consistent envelope:\n\
        \u{00a0} {\"results\": <payload>, \"total\": N, \"hints\": [...]}\n\
        total is present for list commands (find, tags, properties, backlinks). \
        hints is always present (empty [] when --no-hints). \
        --jq operates on the full envelope, e.g. --jq '.results[].file' or --jq '.total'.\n\
        --count prints just the total as a bare integer (shortcut for --jq '.total').\n\
        Use --format text for human-readable output. \
        Successful output goes to stdout; errors go to stderr with exit code 1 (user error) or 2 (internal error).\n\n\
        ABSOLUTE LINKS: Links like `/docs/page.md` are resolved by stripping a site prefix. \
        By default the prefix is auto-derived from --dir's last path component (e.g. --dir ../my-site/docs → prefix \"docs\"). \
        Override with --site-prefix <PREFIX>, or --site-prefix \"\" to disable. Also settable in .hyalo.toml.\n\n\
        CONFIG: Place a .hyalo.toml in the working directory to set defaults:\n\
        \u{00a0} dir = \"vault/\"        # default --dir\n\
        \u{00a0} format = \"text\"       # default --format (CLI default is json)\n\
        \u{00a0} hints = false          # disable hints (CLI default is on)\n\
        \u{00a0} site_prefix = \"docs\"  # override auto-derived site prefix for absolute links\n\
        CLI flags always take precedence.\n\n\
        See COMMAND REFERENCE below for full syntax of each command."
)]
pub(crate) struct Cli {
    /// Root directory for resolving all --file and --glob paths.
    /// Default: "." (Override via .hyalo.toml)
    #[arg(short, long, global = true)]
    pub dir: Option<PathBuf>,

    /// Output format: "json" or "text".
    /// Default: "json" (Override via .hyalo.toml)
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

    /// Use a pre-built snapshot index instead of scanning files from disk.
    ///
    /// Read-only commands (find, summary, tags summary, properties summary,
    /// backlinks) use the index to skip disk scans entirely.
    ///
    /// Mutation commands (set, remove, append, task, mv, tags rename,
    /// properties rename) still read and write individual files on disk,
    /// but when --index is provided they also update the index entry
    /// in-place after each mutation and save it back — keeping the index
    /// current for subsequent queries. This is safe as long as no external
    /// tool modifies files in the vault while the index is active.
    ///
    /// If the index file is incompatible (e.g. after a hyalo upgrade) hyalo
    /// falls back to a full disk scan automatically.
    #[arg(long, global = true, value_name = "PATH")]
    pub index: Option<PathBuf>,

    /// Suppress all warnings printed to stderr.
    ///
    /// Useful in scripts or CI pipelines where warning noise is undesirable.
    /// Identical warnings are always deduplicated regardless of this flag;
    /// use `--quiet` to suppress them entirely.
    #[arg(short = 'q', long, global = true)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

/// All filter arguments for `hyalo find`, extracted so they can be serialized as views.
#[derive(Debug, Clone, Default, clap::Args, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct FindFilters {
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
    /// Target file(s) (repeatable). Mutually exclusive with --glob
    #[arg(short, long, conflicts_with = "glob")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file: Vec<String>,
    /// Glob pattern(s) to select files, relative to --dir (repeatable); prefix '!' to negate (e.g. '!**/draft-*')
    #[arg(short, long, conflicts_with = "file")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub glob: Vec<String>,
    /// Comma-separated list of optional fields to include: all, properties, properties-typed, tags, sections, tasks, links, backlinks, title (default: properties, tags, sections, links — excludes tasks, properties-typed, backlinks, and title). Use 'all' to include every field. 'file' and 'modified' are always included. 'properties' is a {key: value} map; 'properties-typed' is a [{name, type, value}] array; 'backlinks' requires scanning all files; 'title' is the frontmatter title property or first H1 heading (null if neither found). Note: in JSON output, `properties-typed` is serialized as `properties_typed` (underscore)
    #[arg(long, value_name = "FIELDS", use_value_delimiter = true)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Sort order: 'file' (default), 'modified', 'backlinks_count', 'links_count', 'title', 'date', or 'property:<KEY>' for any frontmatter property
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
    /// Reverse the sort order (ascending becomes descending and vice versa)
    #[arg(long)]
    #[serde(skip_serializing_if = "is_false")]
    pub reverse: bool,
    /// Maximum number of results to return (must be at least 1)
    #[arg(short = 'n', long, value_parser = parse_limit)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Only return files with at least one unresolved link (auto-includes links field)
    #[arg(long)]
    #[serde(skip_serializing_if = "is_false")]
    pub broken_links: bool,
    /// Filter by title: case-insensitive substring match against the displayed title
    /// (frontmatter 'title' property or first H1 heading). Use /regex/ for regex
    /// (e.g. '/^The/' or '/^The/i').
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl FindFilters {
    /// Merge CLI overrides onto a view's filters.
    /// - Vec fields: CLI extends the view
    /// - Option fields: CLI overrides if Some
    /// - Bool fields: OR (CLI can turn on, not off)
    pub(crate) fn merge_from(&mut self, overlay: &Self) {
        if overlay.regexp.is_some() {
            self.regexp.clone_from(&overlay.regexp);
        }
        self.properties.extend(overlay.properties.iter().cloned());
        self.tag.extend(overlay.tag.iter().cloned());
        if overlay.task.is_some() {
            self.task.clone_from(&overlay.task);
        }
        self.sections.extend(overlay.sections.iter().cloned());
        self.file.extend(overlay.file.iter().cloned());
        self.glob.extend(overlay.glob.iter().cloned());
        self.fields.extend(overlay.fields.iter().cloned());
        if overlay.sort.is_some() {
            self.sort.clone_from(&overlay.sort);
        }
        self.reverse = self.reverse || overlay.reverse;
        if overlay.limit.is_some() {
            self.limit = overlay.limit;
        }
        self.broken_links = self.broken_links || overlay.broken_links;
        if overlay.title.is_some() {
            self.title.clone_from(&overlay.title);
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
            FILTERS: All filters are AND'd together.\n\
            - PATTERN (positional): case-insensitive body text search\n\
            - --regexp/-e REGEX: regex body text search (case-insensitive by default; mutually exclusive with PATTERN)\n\
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
            COMMON MISTAKES:\n\
            - Property regex uses ~= (tilde-equals), NOT =~ (Perl-style). Wrong: 'title=~/pat/', right: 'title~=/pat/'.\n\
            - --title searches the displayed title (frontmatter or H1); --property title~= only searches frontmatter.\n\
            - --tag uses prefix matching: 'project' matches 'project/backend' but NOT 'projects'.\n\
            SIDE EFFECTS: None (read-only).")]
    Find {
        /// Case-insensitive body text search (searches body only, not frontmatter)
        #[arg(value_name = "PATTERN", conflicts_with = "regexp")]
        pattern: Option<String>,
        /// Use a saved view (named filter set from .hyalo.toml)
        #[arg(long, value_name = "NAME")]
        view: Option<String>,
        #[command(flatten)]
        filters: FindFilters,
    },
    /// Read file body content, optionally filtered by section or line range (read-only)
    #[command(long_about = "Read the body content of a markdown file.\n\n\
            Returns the raw text after the YAML frontmatter block. Use --section to extract a \
            specific section by heading (case-insensitive whole-string match; use leading '#' to \
            pin heading level, e.g. '## Tasks'; use '/regex/' for regex matching; nested subsections are included), \
            --lines to slice a line range, and --frontmatter to include the YAML frontmatter.\n\n\
            OUTPUT: Defaults to plain text (unlike all other commands which default to JSON). \
            Pass --format json to get {\"results\": {\"file\": \"...\", \"content\": \"...\"}, \"hints\": [...]}.\n\
            SIDE EFFECTS: None (read-only).")]
    Read {
        /// Target file (relative to --dir)
        #[arg(short, long)]
        file: String,
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
    },
    /// Property operations: summary or bulk rename
    #[command(long_about = "Property operations across matched files.\n\n\
        Subcommands:\n\
        - summary: Unique property names, types, and file counts (read-only).\n\
        - rename: Rename a property key across files (mutates files).")]
    Properties {
        #[command(subcommand)]
        action: Option<PropertiesAction>,
    },
    /// Tag operations: summary or bulk rename
    #[command(long_about = "Tag operations across matched files.\n\n\
        Subcommands:\n\
        - summary: Unique tags with file counts (read-only).\n\
        - rename: Rename a tag across files (mutates files).")]
    Tags {
        #[command(subcommand)]
        action: Option<TagsAction>,
    },
    /// Read, toggle, or set status on a single task checkbox
    #[command(
        long_about = "Read, toggle, or set status on a single task checkbox.\n\n\
            Subcommands:\n\
            - read: Show task details at a specific line number.\n\
            - toggle: Flip completion state ([ ] <-> [x], custom -> [x]).\n\
            - set-status: Set an arbitrary single-character status.\n\n\
            INPUT: File (--file) and line number (--line).\n\
            SCOPE: Single file only.\n\
            SIDE EFFECTS: 'toggle' and 'set-status' modify the file on disk. 'read' is read-only."
    )]
    Task {
        #[command(subcommand)]
        action: TaskAction,
    },
    /// Show a high-level vault summary: file counts, property/tag/status aggregation, tasks, recent files (read-only)
    #[command(long_about = "Show a high-level vault summary.\n\n\
            OUTPUT: A single 'VaultSummary' object with file counts (total + by directory), \
            property summary (unique names/types/counts), tag summary (unique tags/counts), \
            status grouping (files grouped by frontmatter 'status' value), \
            task counts (total/done), link health (total/broken links with source locations), \
            orphan files, and recently modified files.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need a quick overview of a vault's metadata landscape.")]
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
    },
    /// List all files that link to a given file (read-only)
    #[command(
        long_about = "List all files that link to a given file (reverse link lookup).\n\n\
            Builds an in-memory link graph by scanning all .md files in the vault, \
            then returns every file that contains a [[wikilink]] or [markdown](link) \
            pointing to the target file.\n\n\
            OUTPUT: JSON object with file, backlinks array (source, line, target, label), and total count.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Backlinks {
        /// Target file to find backlinks for (relative to --dir)
        #[arg(short, long)]
        file: String,
    },
    /// Move/rename a file and update all inbound and outbound links
    #[command(
        long_about = "Move or rename a markdown file and update all links across the vault.\n\n\
            Builds an in-memory link graph, then:\n\
            1. Moves the file on disk.\n\
            2. Rewrites all [[wikilinks]] and [markdown](links) in other files that pointed to the old path.\n\
            3. Rewrites relative markdown links inside the moved file whose targets changed due to the new directory context.\n\n\
            Use --dry-run to preview changes without writing.\n\n\
            OUTPUT: JSON object with from, to, updated_files (with per-file replacements), and totals.\n\
            SIDE EFFECTS: Moves the file and modifies files containing links (unless --dry-run)."
    )]
    Mv {
        /// Source file to move (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// Destination path (relative to --dir, must end with .md)
        #[arg(long)]
        to: String,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Set (create or overwrite) frontmatter properties and/or add tags across file(s)
    #[command(
        long_about = "Set (create or overwrite) frontmatter properties and/or add tags across file(s).\n\n\
            INPUT: One or more --property K=V arguments and/or --tag T arguments, with --file or --glob.\n\
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
            USE WHEN: You need to create or overwrite frontmatter properties or add tags, \
            possibly across many files at once."
    )]
    Set {
        /// Property to set: K=V (type inferred from V). Repeatable
        #[arg(short, long = "property", value_name = "K=V")]
        properties: Vec<String>,
        /// Tag to add (idempotent). Repeatable
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with = "glob")]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove frontmatter properties and/or tags from file(s)
    #[command(
        long_about = "Remove frontmatter properties and/or tags from file(s).\n\n\
            INPUT: One or more --property K or K=V arguments and/or --tag T arguments, with --file or --glob.\n\
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
            USE WHEN: You need to delete properties or remove tags from one or more files."
    )]
    Remove {
        /// Property to remove: K (removes key) or K=V (removes value from list/scalar). Repeatable
        #[arg(short, long = "property", value_name = "K or K=V")]
        properties: Vec<String>,
        /// Tag to remove. Repeatable
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with = "glob")]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Initialize hyalo configuration and optional tool integrations
    #[command(
        long_about = "Create .hyalo.toml and optionally set up Claude Code integration.\n\n\
            Without flags, creates a .hyalo.toml config file.\n\
            With --claude, also installs the hyalo skill for Claude Code.\n\n\
            Use the global --dir flag to specify the markdown directory to record in .hyalo.toml."
    )]
    Init {
        /// Set up Claude Code integration (skill + CLAUDE.md hint)
        #[arg(long)]
        claude: bool,
    },
    /// Remove hyalo configuration and Claude Code integration artifacts
    #[command(
        long_about = "Remove .hyalo.toml and all Claude Code integration artifacts created by `init`.\n\n\
            Removes skills, rules, and the managed section from .claude/CLAUDE.md.\n\
            Safe to run when artifacts are already absent (idempotent)."
    )]
    Deinit,
    /// Build a snapshot index for faster repeated read-only queries
    #[command(
        name = "create-index",
        long_about = "Scan the vault and write a binary snapshot index to disk.\n\n\
            The index captures a point-in-time snapshot of all vault metadata.\n\
            Delete it after use via `hyalo drop-index`.\n\n\
            The index file can be passed to any command via `--index <PATH>`.\n\
            Read-only commands skip the disk scan entirely. Mutation commands\n\
            (set, remove, append, task, mv, tags rename, properties rename) still\n\
            read/write files on disk but also patch the index in-place after each\n\
            mutation — keeping it current for subsequent queries. This is safe as\n\
            long as no external tool modifies vault files while the index is active.\n\n\
            OUTPUT: JSON object with `path`, `files_indexed`, and `warnings`.\n\
            SIDE EFFECTS: Writes a binary file (default: .hyalo-index in --dir)."
    )]
    CreateIndex {
        /// Output path for the index file (default: .hyalo-index in --dir)
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
            INPUT: One or more --property K=V arguments, with --file or --glob.\n\
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
            USE WHEN: You need to append items to list-type properties such as 'aliases' or 'authors' \
            without overwriting the existing list."
    )]
    Append {
        /// Property to append to: K=V. Repeatable
        #[arg(short, long = "property", value_name = "K=V", required = true)]
        properties: Vec<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with = "glob")]
        file: Vec<String>,
        /// Glob pattern(s) for multiple files, relative to --dir (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
        /// Preview changes without modifying any files
        #[arg(long)]
        dry_run: bool,
    },
    /// Manage saved views (named find filter sets stored in .hyalo.toml)
    #[command(
        long_about = "Manage saved views — named find queries stored in .hyalo.toml.\n\n\
            Views let you save frequently used filter combinations under a name\n\
            and recall them with `hyalo find --view <name>`.\n\n\
            Subcommands:\n\
            - list: Show all saved views and their filters.\n\
            - set: Create or update a view.\n\
            - remove: Delete a view.\n\n\
            SIDE EFFECTS: 'set' and 'remove' modify .hyalo.toml. 'list' is read-only."
    )]
    Views {
        #[command(subcommand)]
        action: ViewsAction,
    },
    /// Detect and repair broken links across the vault
    #[command(
        long_about = "Detect and repair broken wikilinks and markdown links.\n\n\
            Scans the vault for links that cannot be resolved to an existing file, \
            then uses fuzzy matching (case-insensitive, extension mismatch, shortest-path, \
            Jaro-Winkler) to find the best candidate replacement.\n\n\
            Default behaviour is a dry run — no files are modified. Pass --apply to write fixes.\n\n\
            OUTPUT: JSON object with broken/fixable/unfixable counts, per-fix details \
            (source, line, old_target, new_target, strategy, confidence), and \
            the list of links that could not be matched.\n\
            SIDE EFFECTS: None unless --apply is passed.\n\n\
            TIP: For read-only auditing, use 'hyalo summary' (link health overview)\n\
            or 'hyalo find --broken-links' (list files with unresolved links)."
    )]
    Links {
        #[command(subcommand)]
        action: LinksAction,
    },
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)] // Set variant holds FindFilters by design; boxing would complicate dispatch
pub(crate) enum ViewsAction {
    /// List all saved views
    #[command(
        long_about = "Show all saved views and their filter configurations.\n\n\
        OUTPUT: JSON array of view objects with name and filters.\n\
        SIDE EFFECTS: None (read-only)."
    )]
    List,
    /// Create or update a saved view
    #[command(long_about = "Save a combination of find filters under a name.\n\n\
        The view is stored in .hyalo.toml and can be recalled with `hyalo find --view <name>`.\n\
        Overwrites if the view already exists.\n\n\
        SIDE EFFECTS: Modifies .hyalo.toml.")]
    Set {
        /// View name
        #[arg(value_name = "NAME")]
        name: String,
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
            Use --apply to write fixes to disk. Without --apply, only a dry-run report is printed.")]
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
    },
}

#[derive(Subcommand)]
pub(crate) enum TaskAction {
    /// Show task details at a specific line number (read-only)
    #[command(long_about = "Show task details at a specific line number.\n\n\
        INPUT: --file and --line (1-based, counting from line 1 of the file including frontmatter).\n\
        OUTPUT: {\"file\": \"...\", \"line\": N, \"status\": \"x\", \"text\": \"...\", \"done\": true}\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to inspect a task's current status before toggling or updating it.")]
    Read {
        /// File containing the task (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// 1-based line number of the task (counted from line 1 of the file, including frontmatter). Use 'hyalo find --task todo' to discover task line numbers
        #[arg(short, long)]
        line: usize,
    },
    /// Toggle task completion: [ ] -> [x], [x]/[X] -> [ ], custom -> [x]
    #[command(
        long_about = "Toggle task completion: [ ] -> [x], [x]/[X] -> [ ], custom -> [x].\n\n\
        INPUT: --file and --line (1-based, counting from line 1 of the file including frontmatter).\n\
        OUTPUT: {\"file\": \"...\", \"line\": N, \"status\": \"x\", \"text\": \"...\", \"done\": true}\n\
        SIDE EFFECTS: Modifies the file on disk (rewrites the checkbox character).\n\
        USE WHEN: You need to mark a task as done or re-open a completed task."
    )]
    Toggle {
        /// File containing the task (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// 1-based line number of the task (counted from line 1 of the file, including frontmatter). Use 'hyalo find --task todo' to discover task line numbers
        #[arg(short, long)]
        line: usize,
    },
    /// Set a custom single-character status on a task
    #[command(
        name = "set-status",
        long_about = "Set a custom single-character status on a task checkbox.\n\n\
        INPUT: --file, --line (1-based, counting from line 1 of the file including frontmatter), and --status (single char).\n\
        OUTPUT: {\"file\": \"...\", \"line\": N, \"status\": \"?\", \"text\": \"...\", \"done\": false}\n\
        SIDE EFFECTS: Modifies the file on disk (rewrites the checkbox character).\n\
        USE WHEN: You need to set a non-standard status like '?' (question), '-' (cancelled), or '!' (important)."
    )]
    SetStatus {
        /// File containing the task (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// 1-based line number of the task (counted from line 1 of the file, including frontmatter). Use 'hyalo find --task todo' to discover task line numbers
        #[arg(short, long)]
        line: usize,
        /// Single character to set as the task status (e.g. '?', '-', '!')
        #[arg(short, long)]
        status: String,
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
    },
}
