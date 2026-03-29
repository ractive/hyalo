use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

use hyalo_cli::commands::{
    append as append_commands, backlinks as backlinks_commands,
    create_index as create_index_commands, drop_index as drop_index_commands,
    find as find_commands, init as init_commands, links as links_commands, mv as mv_commands,
    properties, read as read_commands, remove as remove_commands, set as set_commands,
    summary as summary_commands, tags as tag_commands, tasks as task_commands,
};
use hyalo_cli::hints::{HintContext, HintSource, generate_hints};
use hyalo_cli::output::{
    CommandOutcome, Format, apply_jq_filter_result, format_success, format_with_hints,
};
use hyalo_core::filter;
use hyalo_core::index::{SnapshotIndex, VaultIndex};

fn parse_limit(s: &str) -> Result<usize, String> {
    let n: usize = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;
    if n == 0 {
        return Err("limit must be at least 1".to_owned());
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// Static help text — extracted from derive attributes so they can be filtered
// at runtime based on loaded .hyalo.toml config.
// ---------------------------------------------------------------------------

/// Short help (shown by `-h`): one example per feature.
const HELP_EXAMPLES: &str = "EXAMPLES:
  Search for files:             hyalo find --property status=draft
  Filter by tag:                hyalo find --tag project
  Filter by task status:        hyalo find --task todo
  Full-text search:             hyalo find 'meeting notes'
  Filter by section:            hyalo find --section 'Tasks' --task todo
  Read file content:            hyalo read --file notes/todo.md
  Read a section:               hyalo read --file notes/todo.md --section Proposal
  Set a property:               hyalo set --property status=completed --file notes/todo.md
  Bulk-set with filter:         hyalo set --property status=completed --where-property status=draft --glob '**/*.md'
  Add a tag across files:       hyalo set --tag reviewed --glob 'research/**/*.md'
  Remove a property:            hyalo remove --property status --file notes/todo.md
  Remove a tag from files:      hyalo remove --tag draft --glob '**/*.md'
  Append to a list property:    hyalo append --property aliases='My Note' --file note.md
  Aggregate property summary:   hyalo properties summary
  Rename a property key:        hyalo properties rename --from old-key --to new-key
  Aggregate tag summary:        hyalo tags summary
  Rename a tag across files:    hyalo tags rename --from old-tag --to new-tag
  Vault overview:               hyalo summary --format text
  Overview with drill-down:     hyalo summary --format text --hints
  Toggle a task:                hyalo task toggle --file todo.md --line 5
  Find backlinks:               hyalo backlinks --file decision-log.md
  Move a file (update links):   hyalo mv --file old.md --to new.md
  Move (dry-run preview):       hyalo mv --file old.md --to sub/new.md --dry-run
  Build a snapshot index:       hyalo create-index
  Query using the index:        hyalo find --property status=draft --index .hyalo-index
  Delete the snapshot index:    hyalo drop-index";

/// Long help (shown by `--help`): command reference, cookbook, and output shapes.
const HELP_LONG: &str = "COMMAND REFERENCE:
  Find (search and filter, read-only):
    hyalo find [PATTERN | -e/--regexp REGEX] [-p/--property K=V ...] [-t/--tag T ...] [--task STATUS]
               [-s/--section HEADING ...] [-f/--file F | -g/--glob G] [--fields ...] [--sort ...] [-n/--limit N]

  Read (display file body content, read-only):
    hyalo read -f/--file F [-s/--section HEADING] [-l/--lines RANGE] [--frontmatter]

  Set (create or overwrite, mutates files):
    hyalo set  -p/--property K=V [-p ...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]

  Remove (delete properties/tags, mutates files):
    hyalo remove -p/--property K|K=V [...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]

  Append (add to list properties, mutates files):
    hyalo append -p/--property K=V [-p ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]

  Properties (subcommand group):
    hyalo properties summary [-g/--glob G]                        Unique property names, types, and file counts (read-only)
    hyalo properties rename --from OLD --to NEW [-g/--glob G]     Rename a property key across files (mutates files)

  Tags (subcommand group):
    hyalo tags summary [-g/--glob G]                              Unique tags with file counts (read-only)
    hyalo tags rename --from OLD --to NEW [-g/--glob G]           Rename a tag across files (mutates files)

  Summary (vault overview, read-only):
    hyalo summary [-g/--glob G] [-n/--recent N]

  Task (single-task operations):
    hyalo task read       -f/--file F -l/--line N           Read task at a line
    hyalo task toggle     -f/--file F -l/--line N           Toggle completion
    hyalo task set-status -f/--file F -l/--line N -s/--status C

  Backlinks (reverse link lookup, read-only):
    hyalo backlinks -f/--file F

  Links (link operations):
    hyalo links fix [--apply] [--threshold T] [-g/--glob G]   Detect and fix broken links (default: dry-run)

  Mv (move/rename file, updates links, mutates files):
    hyalo mv -f/--file F --to NEW [--dry-run]

  Init (configuration, one-time setup):
    hyalo init [--claude] [-d/--dir DIR]

  Create-index (build snapshot for faster queries):
    hyalo create-index [-o/--output PATH]

  Drop-index (delete snapshot index):
    hyalo drop-index [-p/--path PATH]

  Global flags (apply to all commands):
    -d/--dir <DIR>          Root directory (default: ., override via .hyalo.toml)
    --format json|text      Output format (default: json, override via .hyalo.toml)
    --jq <FILTER>           Apply a jq expression to JSON output
    --hints                 Append drill-down hints (default: off, override via .hyalo.toml)
    --no-hints              Disable hints (overrides .hyalo.toml)
    --site-prefix <PREFIX>  Override site prefix for absolute link resolution (auto-derived from --dir)

COOKBOOK:
  # Discover what metadata exists in a vault
  hyalo properties summary
  hyalo tags summary

  # Rename a property key across all files
  hyalo properties rename --from old-key --to new-key

  # Rename a tag across all files
  hyalo tags rename --from old-tag --to new-tag

  # Get a vault overview with drill-down hints
  hyalo summary --format text --hints

  # Find all files with status=draft
  hyalo find --property status=draft

  # Find files missing the 'status' property (absence filter)
  hyalo find --property '!status'

  # Find files where title contains 'draft' (property value regex)
  hyalo find --property 'title~=draft'

  # Case-insensitive regex on a property value
  hyalo find --property 'title~=/^Draft/i'

  # Find files tagged 'project' (matches project/backend, project/frontend, etc.)
  hyalo find --tag project

  # Find files with open tasks
  hyalo find --task todo

  # Find files with a specific section heading (substring match: 'Tasks' matches 'Tasks [4/4]')
  hyalo find --section 'Tasks'

  # Find open tasks within a specific section
  hyalo find --section '## Sprint' --task todo

  # Find broken [[wikilinks]] (fields=links, then filter in jq)
  hyalo find --fields links --jq '[.[] | select(.links | map(select(.path == null)) | length > 0)]'

  # Exclude draft files with glob negation
  hyalo find --glob '!**/draft-*'

  # Tag all research notes in a folder
  hyalo set --tag reviewed --glob 'research/**/*.md'

  # Bulk-update a property across matching files
  hyalo set --property status=in-progress --where-property status=draft --glob '**/*.md'

  # Add a tag to files matching a tag filter
  hyalo set --tag reviewed --where-tag research --glob '**/*.md'

  # Append to a list property
  hyalo append --property aliases='My Note' --file note.md

  # Quick vault overview
  hyalo summary --format text

  # Count tasks across all files
  hyalo summary --jq '.tasks.total'

  # List all property names as a flat list
  hyalo properties summary --jq '[.[].name] | join(\", \")'

  # Get just file paths (no metadata)
  hyalo find --property status=draft --jq '[.[].file]'

  # Pipe file paths for scripting (Unix)
  hyalo find --tag research --jq '.[].file' | xargs -I{} hyalo set --property reviewed=true --file {}

  # Find all files that link to a given note
  hyalo backlinks --file decision-log.md

  # Move a file and update all links
  hyalo mv --file backlog/old.md --to backlog/done/old.md

  # Preview a move without writing
  hyalo mv --file note.md --to archive/note.md --dry-run

  # Override site prefix for absolute link resolution
  hyalo --site-prefix docs mv --file old.md --to new.md --dry-run

  # Disable absolute-link resolution entirely
  hyalo --site-prefix '' find --fields links

  # Read file body content
  hyalo read --file notes/todo.md

  # Read a specific section
  hyalo read --file notes/todo.md --section Tasks

  # Read a line range
  hyalo read --file notes/todo.md --lines 1:10

  # Read a task's current status
  hyalo task read --file todo.md --line 5

  # Toggle a task checkbox
  hyalo task toggle --file todo.md --line 5

  # Set a custom task status (e.g. cancelled)
  hyalo task set-status --file todo.md --line 5 --status -

  # Build a snapshot index for faster repeated queries
  hyalo create-index

  # Use the index for a find query
  hyalo find --property status=draft --index .hyalo-index

  # Clean up the index after use
  hyalo drop-index

OUTPUT SHAPES (JSON, default):
  # find
  [{\"file\": \"notes/todo.md\", \"modified\": \"2026-03-21T...\",
   \"properties\": {\"status\": \"draft\", \"title\": \"My Note\"},
   \"tags\": [...], \"sections\": [...], \"tasks\": [...], \"links\": [...]}]

  # read
  {\"file\": \"notes/todo.md\", \"content\": \"...body text...\"}

  # set / remove / append (mutation result)
  {\"property\": \"status\", \"value\": \"completed\", \"modified\": [...], \"skipped\": [...], \"total\": N}
  {\"tag\": \"reviewed\", \"modified\": [...], \"skipped\": [...], \"total\": N}

  # properties summary
  [{\"name\": \"status\", \"type\": \"text\", \"count\": 21}, ...]

  # properties rename
  {\"from\": \"old\", \"to\": \"new\", \"modified\": [...], \"skipped\": [...], \"conflicts\": [...], \"total\": N}

  # tags summary
  {\"tags\": [{\"name\": \"backlog\", \"count\": 10}, ...], \"total\": 31}

  # tags rename
  {\"from\": \"old\", \"to\": \"new\", \"modified\": [...], \"skipped\": [...], \"total\": N}

  # task read / toggle / set-status
  {\"file\": \"todo.md\", \"line\": 5, \"status\": \"x\", \"text\": \"Fix bug\", \"done\": true}

  # summary
  {\"files\": {\"total\": 31, \"by_directory\": [...]}, \"properties\": [...], \"tags\": {...},
  \"status\": [{\"value\": \"draft\", \"files\": [...]}], \"tasks\": {\"total\": 50, \"done\": 30},
  \"orphans\": [\"orphan.md\", ...],
  \"recent_files\": [{\"path\": \"note.md\", \"modified\": \"2026-03-21T...\"}]}

  # backlinks
  {\"file\": \"target.md\", \"backlinks\": [{\"source\": \"a.md\", \"line\": 5, \"target\": \"target\"}], \"total\": 1}

  # mv
  {\"from\": \"old.md\", \"to\": \"new.md\", \"dry_run\": false,
  \"updated_files\": [{\"file\": \"a.md\", \"replacements\": [{\"line\": 5, \"old_text\": \"[[old]]\", \"new_text\": \"[[new]]\"}]}],
  \"total_files_updated\": 1, \"total_links_updated\": 1}

  # create-index
  {\"path\": \".hyalo-index\", \"files_indexed\": 142, \"warnings\": 0}

  # drop-index
  {\"deleted\": \".hyalo-index\"}

  # --hints wraps JSON output in an envelope with drill-down commands
  {\"data\": { ... original output ... }, \"hints\": [\"hyalo properties\", ...]}

  # errors (stderr, exit code 1 for user errors, 2 for internal)
  {\"error\": \"property not found\", \"path\": \"notes/todo.md\"}

  # --format text produces human-readable output on all commands";

/// Build a filtered version of `HELP_EXAMPLES` (the `-h` EXAMPLES block).
///
/// Each example is a single line.  Drop any line that references a flag whose
/// value is already provided by `.hyalo.toml` so it does not clutter the output.
///
/// Rules:
/// - `hide_dir`    → drop lines that contain `-d/--dir` or ` --dir `
/// - `hide_format` → drop lines that contain `--format`
fn filter_examples(hide_dir: bool, hide_format: bool) -> String {
    if !hide_dir && !hide_format {
        return HELP_EXAMPLES.to_owned();
    }
    let filtered: Vec<&str> = HELP_EXAMPLES
        .lines()
        .filter(|line| {
            if hide_format && line.contains(" --format") {
                return false;
            }
            if hide_dir && (line.contains("-d/--dir") || line.contains(" --dir ")) {
                return false;
            }
            true
        })
        .collect();
    filtered.join("\n")
}

/// Build a filtered version of `HELP_LONG` (the `--help` long help block).
///
/// The long help contains three sections: COMMAND REFERENCE, COOKBOOK, and
/// OUTPUT SHAPES.  The filtering strategy differs per section:
///
/// - **COMMAND REFERENCE / Global flags**: line-level — drop the specific flag
///   rows (`-d/--dir` and/or `--format json|text`) when they are config-defaulted.
/// - **COOKBOOK**: paragraph-level — each recipe is separated by a blank line.
///   Drop an entire recipe (comment + command) when the command line contains a
///   config-defaulted flag (drops the whole example, not just the flag).
///
/// This keeps the help focused on flags the user actually needs to type.
fn filter_long_help(hide_dir: bool, hide_format: bool) -> String {
    if !hide_dir && !hide_format {
        return HELP_LONG.to_owned();
    }

    // Split into paragraphs separated by blank lines.  Process each paragraph
    // individually, then rejoin.
    let paragraphs: Vec<&str> = HELP_LONG.split("\n\n").collect();
    let mut out: Vec<String> = Vec::with_capacity(paragraphs.len());

    for para in &paragraphs {
        // The Global flags paragraph needs line-level filtering (we want to keep
        // the paragraph but drop individual flag rows).
        if para.contains("  Global flags (apply to all commands):") {
            let filtered: String = para
                .lines()
                .filter(|line| {
                    let trimmed = line.trim_start();
                    if hide_dir && trimmed.starts_with("-d/--dir") {
                        return false;
                    }
                    if hide_format && trimmed.starts_with("--format ") {
                        return false;
                    }
                    true
                })
                .collect::<Vec<&str>>()
                .join("\n");
            out.push(filtered);
            continue;
        }

        // For cookbook / output-shapes paragraphs: drop the entire paragraph
        // if any hyalo command line in it uses a config-defaulted flag.
        let should_drop = para.lines().any(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("hyalo ") {
                return false;
            }
            (hide_format && trimmed.contains(" --format"))
                || (hide_dir && (trimmed.contains(" --dir ") || trimmed.contains(" -d ")))
        });

        if !should_drop {
            out.push((*para).to_owned());
        }
    }

    out.join("\n\n")
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
        OUTPUT: Returns JSON by default (--format json). Use --format text for human-readable output. \
        Successful output goes to stdout; errors go to stderr with exit code 1 (user error) or 2 (internal error).\n\n\
        ABSOLUTE LINKS: Links like `/docs/page.md` are resolved by stripping a site prefix. \
        By default the prefix is auto-derived from --dir's last path component (e.g. --dir ../my-site/docs → prefix \"docs\"). \
        Override with --site-prefix <PREFIX>, or --site-prefix \"\" to disable. Also settable in .hyalo.toml.\n\n\
        CONFIG: Place a .hyalo.toml in the working directory to set defaults:\n\
        \u{00a0} dir = \"vault/\"        # default --dir\n\
        \u{00a0} format = \"text\"       # default --format (CLI default is json)\n\
        \u{00a0} hints = true           # default --hints on (CLI default is off)\n\
        \u{00a0} site_prefix = \"docs\"  # override auto-derived site prefix for absolute links\n\
        CLI flags always take precedence.\n\n\
        See COMMAND REFERENCE below for full syntax of each command."
)]
struct Cli {
    /// Root directory for resolving all --file and --glob paths.
    /// Default: "." (Override via .hyalo.toml)
    #[arg(short, long, global = true)]
    dir: Option<PathBuf>,

    /// Output format: "json" or "text".
    /// Default: "json" (Override via .hyalo.toml)
    #[arg(long, global = true)]
    format: Option<Format>,

    /// Apply a jq filter expression to the JSON output of any command.
    /// The filtered result is printed as plain text. Incompatible with non-JSON formats (--format text).
    /// Example: --jq '.files[]' or --jq 'map(.name) | join(", ")'.
    /// Note: recursive filters (e.g. 'recurse', '..') on large inputs may run indefinitely
    #[arg(long, global = true, value_name = "FILTER")]
    jq: Option<String>,

    /// Append drill-down command hints to the output.
    /// Text mode: '-> hyalo ...' lines — concrete, copy-pasteable commands.
    /// JSON mode: wraps in {"data": ..., "hints": [...]}.
    /// Suppressed when --jq is active.
    /// Override via .hyalo.toml
    #[arg(long, global = true)]
    hints: bool,

    /// Disable hints even when enabled in .hyalo.toml
    #[arg(long, global = true, conflicts_with = "hints")]
    no_hints: bool,

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
    site_prefix: Option<String>,

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
    index: Option<PathBuf>,

    /// Suppress all warnings printed to stderr.
    ///
    /// Useful in scripts or CI pipelines where warning noise is undesirable.
    /// Identical warnings are always deduplicated regardless of this flag;
    /// use `--quiet` to suppress them entirely.
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search and filter markdown files — returns an array of file objects with metadata, structure, tasks, and links
    #[command(long_about = "Search and filter markdown files.\n\n\
            Returns an array of file objects. Each object contains the file path, modified time, \
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
            to pin heading level, e.g. '## Tasks'; use '~=/regex/' for regex matching). Repeatable (OR). \
            Nested subsections are included.\n\n\
            OUTPUT: Always returns a JSON array of file objects, even with --file.\n\
            FIELDS: Use --fields to limit which fields appear (default: all). \
            Properties are a {key: value} map; use --fields properties-typed for [{name, type, value}] array.\n\
            SIDE EFFECTS: None (read-only).")]
    Find {
        /// Case-insensitive body text search (searches body only, not frontmatter)
        #[arg(value_name = "PATTERN", conflicts_with = "regexp")]
        pattern: Option<String>,
        /// Regex body text search (case-insensitive by default; use (?-i) to override). Mutually exclusive with PATTERN
        #[arg(long, short = 'e', value_name = "REGEX")]
        regexp: Option<String>,
        /// Property filter: K=V (eq), K!=V (neq), K>=V, K<=V, K>V, K<V, K (exists), !K (absent), K~=pat or K~=/pat/i (regex). Repeatable (AND)
        #[arg(short, long = "property", value_name = "FILTER")]
        properties: Vec<String>,
        /// Tag filter: exact or prefix match (e.g. 'project' matches 'project/backend' but not 'projects'). Repeatable (AND)
        #[arg(short, long, value_name = "TAG")]
        tag: Vec<String>,
        /// Task presence filter: 'todo', 'done', 'any', or a single status character
        #[arg(long, value_name = "STATUS")]
        task: Option<String>,
        /// Section heading filter: case-insensitive substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
        /// prefix '##' to pin heading level; prefix '~=' for regex (e.g. '~=/DEC-03[12]/'). Repeatable (OR)
        #[arg(short, long = "section", value_name = "HEADING")]
        sections: Vec<String>,
        /// Target file(s) (repeatable). Mutually exclusive with --glob
        #[arg(short, long, conflicts_with = "glob")]
        file: Vec<String>,
        /// Glob pattern(s) to select files (repeatable); prefix '!' to negate (e.g. '!**/draft-*')
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Comma-separated list of optional fields to include: all, properties, properties-typed, tags, sections, tasks, links, backlinks, title (default: all standard fields except properties-typed, backlinks, and title). Use 'all' to include every field. 'file' and 'modified' are always included. 'properties' is a {key: value} map; 'properties-typed' is a [{name, type, value}] array; 'backlinks' requires scanning all files; 'title' is the frontmatter title property or first H1 heading (null if neither found). Note: in JSON output, `properties-typed` is serialized as `properties_typed` (underscore)
        #[arg(long, value_name = "FIELDS", use_value_delimiter = true)]
        fields: Vec<String>,
        /// Sort order: 'file' (default), 'modified', 'backlinks_count', 'links_count', 'title', 'date', or 'property:<KEY>' for any frontmatter property
        #[arg(long)]
        sort: Option<String>,
        /// Reverse the sort order (ascending becomes descending and vice versa)
        #[arg(long)]
        reverse: bool,
        /// Maximum number of results to return (must be at least 1)
        #[arg(short = 'n', long, value_parser = parse_limit)]
        limit: Option<usize>,
        /// Only return files with at least one unresolved link (auto-includes links field)
        #[arg(long)]
        broken_links: bool,
        /// Filter by title: case-insensitive substring match against the displayed title
        /// (frontmatter 'title' property or first H1 heading). Prefix with '~=' for regex.
        #[arg(long)]
        title: Option<String>,
    },
    /// Read file body content, optionally filtered by section or line range (read-only)
    #[command(long_about = "Read the body content of a markdown file.\n\n\
            Returns the raw text after the YAML frontmatter block. Use --section to extract a \
            specific section by heading (case-insensitive whole-string match; use leading '#' to \
            pin heading level, e.g. '## Tasks'; nested subsections are included), \
            --lines to slice a line range, and --frontmatter to include the YAML frontmatter.\n\n\
            OUTPUT: Defaults to plain text (note: this overrides the global --format json default). \
            Pass --format json explicitly to get \
            {\"file\": \"...\", \"content\": \"...\"}.\n\
            SIDE EFFECTS: None (read-only).\n\
            FORMAT DEFAULT: Unlike other commands, `read` outputs plain text by default \
            — the --format flag shown below says 'Default: json' because it is a global flag, \
            but `read` overrides this to text.")]
    Read {
        /// Target file (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// Extract section(s) by substring match (e.g. 'Tasks' matches 'Tasks [4/4]');
        /// prefix '##' to pin heading level; prefix '~=' for regex. Nested subsections included
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
        /// Glob pattern(s) to filter which files to include (repeatable); prefix '!' to negate (e.g. '!**/draft-*')
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
            SIDE EFFECTS: Modifies matched files on disk.\n\
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
        /// Glob pattern(s) for multiple files (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
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
            SIDE EFFECTS: Modifies matched files on disk.\n\
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
        /// Glob pattern(s) for multiple files (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
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
            SIDE EFFECTS: Modifies matched files on disk.\n\
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
        /// Glob pattern(s) for multiple files (repeatable); prefix '!' to negate
        #[arg(short, long, conflicts_with = "file")]
        glob: Vec<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
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
enum LinksAction {
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
        /// Glob pattern(s) to filter which files to check (repeatable); prefix '!' to negate
        #[arg(short, long)]
        glob: Vec<String>,
        /// Ignore broken links whose target contains any of these substrings (repeatable).
        /// Useful for skipping Hugo template links, external paths, etc.
        #[arg(long)]
        ignore_target: Vec<String>,
    },
}

#[derive(Subcommand)]
enum TaskAction {
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
enum PropertiesAction {
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
enum TagsAction {
    /// Show unique tags with file counts (read-only)
    #[command(long_about = "Aggregate summary of tags across matched files.\n\n\
        OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
        SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
        SIDE EFFECTS: None (read-only).\n\
        USE WHEN: You need to see which tags exist, find popular/orphan tags, or audit tag taxonomy.")]
    Summary {
        /// Glob pattern(s) to filter which files to scan (repeatable); prefix '!' to negate
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

/// Value parser for `--threshold`: accepts a `f64` in `[0.0, 1.0]`.
fn parse_threshold(s: &str) -> Result<f64, String> {
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
        if let Err(msg) = hyalo_cli::commands::tags::validate_tag(tag) {
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
    hyalo_cli::warn::flush_summary();
    process::exit(code)
}

#[allow(clippy::too_many_lines)]
fn main() {
    // Pre-scan for --quiet / -q so config-loading warnings are also suppressed.
    let early_quiet = std::env::args().any(|a| a == "--quiet" || a == "-q");
    hyalo_cli::warn::init(early_quiet);

    // Load per-project config from .hyalo.toml in CWD before parsing args.
    // This lets us hide flags that already have config-provided defaults,
    // keeping `--help` output focused on what the user actually needs to set.
    let config = hyalo_cli::config::load_config();

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
                && hyalo_cli::suggest::unknown_arg_is(&e, "--filter")
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
                hyalo_cli::suggest::suggest_subcommand_correction(&raw_args, &Cli::command())
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

            e.exit();
        }
    };
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    // Re-apply quiet flag from the fully-parsed CLI (the early pre-scan
    // covers the common case but this ensures correctness after full parsing).
    hyalo_cli::warn::init(cli.quiet);

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
                .map(|s| s.to_owned()),
            Err(_) => {
                // Fallback for non-existent paths: use file_name() on the raw path.
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| *s != ".")
                    .map(|s| s.to_owned())
            }
        }
    };
    let site_prefix = site_prefix_owned.as_deref();
    // CLI --format is already validated by Clap; fall back to config (String) with runtime parse.
    let format = if let Some(f) = cli.format {
        f
    } else {
        match Format::from_str_opt(&config.format) {
            Some(fmt) => fmt,
            None => {
                eprintln!(
                    "Invalid output format '{}' in .hyalo.toml; supported formats are: json, text",
                    config.format
                );
                die(2);
            }
        }
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
        eprintln!("Error: --jq cannot be combined with --format {}", format);
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
            dir.to_str().map(|s| s.to_owned()).filter(|s| s != ".")
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
        hyalo_cli::warn::warn("--hints has no effect on mutation commands");
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
                    if !idx.validate(&vault_dir_str, site_prefix) {
                        let (hdr_vault, hdr_prefix, _, _) = idx.header_info();
                        hyalo_cli::warn::warn(format!(
                            "index was built for vault '{}' (prefix {:?}) but current \
                             vault is '{}' (prefix {:?}); falling back to disk scan",
                            hdr_vault, hdr_prefix, vault_dir_str, site_prefix,
                        ));
                        None
                    } else {
                        Some(idx)
                    }
                }
                Ok(None) => None, // incompatible schema — already warned; fall back to disk scan
                Err(e) => {
                    hyalo_cli::warn::warn(format!(
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
                if let Err(msg) = hyalo_cli::commands::tags::validate_tag(t) {
                    eprintln!("Error: {msg}");
                    die(1);
                }
            }

            if let Some(ref idx) = snapshot_index {
                find_commands::find_from_index(
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
                find_commands::find(
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
                                properties::properties_summary_from_index(
                                    idx,
                                    file_filter,
                                    effective_format,
                                )
                            }
                        }
                    } else {
                        properties::properties_summary(&dir, None, glob, effective_format)
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
                                tag_commands::tags_summary_from_index(
                                    idx,
                                    file_filter,
                                    effective_format,
                                )
                            }
                        }
                    } else {
                        tag_commands::tags_summary(&dir, None, glob, effective_format)
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
                    let out = hyalo_cli::output::format_error(
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
                summary_commands::summary_from_index(
                    &dir,
                    idx,
                    glob,
                    recent,
                    depth,
                    site_prefix,
                    effective_format,
                )
            } else {
                summary_commands::summary(&dir, glob, recent, depth, site_prefix, effective_format)
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
                backlinks_commands::backlinks_from_index(idx, file, &dir, effective_format)
            } else {
                backlinks_commands::backlinks(&dir, site_prefix, file, effective_format)
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
                    links_commands::links_fix_from_index(
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
                    links_commands::links_fix(
                        &dir,
                        site_prefix,
                        glob,
                        !apply,
                        threshold,
                        ignore_target,
                        effective_format,
                    )
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
                        let msg = hyalo_cli::output::format_error(
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
                        let msg = hyalo_cli::output::format_error(
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
                let value: serde_json::Value = match serde_json::from_str(&output) {
                    Ok(v) => v,
                    Err(_) => {
                        // Should not happen since effective_format is forced to JSON,
                        // but fall through to plain output if it does.
                        println!("{output}");
                        die(0);
                    }
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
            let msg = hyalo_cli::output::format_error(
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
    hyalo_cli::warn::flush_summary();
}
