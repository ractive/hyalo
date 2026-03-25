use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use hyalo_cli::commands::{
    append as append_commands, find as find_commands, init as init_commands, properties,
    read as read_commands, remove as remove_commands, set as set_commands,
    summary as summary_commands, tags as tag_commands, tasks as task_commands,
};
use hyalo_cli::hints::{HintContext, HintSource, generate_hints};
use hyalo_cli::output::{CommandOutcome, Format, apply_jq_filter_result, format_with_hints};
use hyalo_core::filter;

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
        CONFIG: Place a .hyalo.toml in the working directory to set defaults:\n\
        \u{00a0} dir = \"vault/\"        # default --dir\n\
        \u{00a0} format = \"text\"       # example: override --format (CLI default is json)\n\
        \u{00a0} hints = true           # example: override --hints on (CLI default is off)\n\
        CLI flags always take precedence.\n\n\
        See COMMAND REFERENCE below for full syntax of each command.",
    after_help = "EXAMPLES:\n  \
        Search for files:             hyalo find --property status=draft\n  \
        Filter by tag:                hyalo find --tag project\n  \
        Filter by task status:        hyalo find --task todo\n  \
        Full-text search:             hyalo find 'meeting notes'\n  \
        Filter by section:            hyalo find --section 'Tasks' --task todo\n  \
        Read file content:            hyalo read --file notes/todo.md\n  \
        Read a section:               hyalo read --file notes/todo.md --section Proposal\n  \
        Set a property:               hyalo set --property status=completed --file notes/todo.md\n  \
        Bulk-set with filter:         hyalo set --property status=completed --where-property status=draft --glob '**/*.md'\n  \
        Add a tag across files:       hyalo set --tag reviewed --glob 'research/**/*.md'\n  \
        Remove a property:            hyalo remove --property status --file notes/todo.md\n  \
        Remove a tag from files:      hyalo remove --tag draft --glob '**/*.md'\n  \
        Append to a list property:    hyalo append --property aliases='My Note' --file note.md\n  \
        Aggregate property summary:   hyalo properties\n  \
        Aggregate tag summary:        hyalo tags\n  \
        Vault overview:               hyalo summary --format text\n  \
        Overview with drill-down:     hyalo summary --format text --hints\n  \
        Toggle a task:                hyalo task toggle --file todo.md --line 5",
    after_long_help = "\
COMMAND REFERENCE:\n  \
  Find (search and filter, read-only):\n  \
    hyalo find [PATTERN | -e/--regexp REGEX] [-p/--property K=V ...] [-t/--tag T ...] [--task STATUS]\n  \
               [-s/--section HEADING ...] [-f/--file F | -g/--glob G] [--fields ...] [--sort ...] [-n/--limit N]\n\n  \
  Read (display file body content, read-only):\n  \
    hyalo read -f/--file F [-s/--section HEADING] [-l/--lines RANGE] [--frontmatter]\n\n  \
  Set (create or overwrite, mutates files):\n  \
    hyalo set  -p/--property K=V [-p ...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]\n\n  \
  Remove (delete properties/tags, mutates files):\n  \
    hyalo remove -p/--property K|K=V [...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]\n\n  \
  Append (add to list properties, mutates files):\n  \
    hyalo append -p/--property K=V [-p ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...]\n\n  \
  Properties (aggregate summary, read-only):\n  \
    hyalo properties [-g/--glob G]   Unique property names, types, and file counts\n\n  \
  Tags (aggregate summary, read-only):\n  \
    hyalo tags [-g/--glob G]         Unique tags with file counts\n\n  \
  Summary (vault overview, read-only):\n  \
    hyalo summary [-g/--glob G] [-n/--recent N]\n\n  \
  Task (single-task operations):\n  \
    hyalo task read       -f/--file F -l/--line N           Read task at a line\n  \
    hyalo task toggle     -f/--file F -l/--line N           Toggle completion\n  \
    hyalo task set-status -f/--file F -l/--line N -s/--status C\n\n  \
  Init (configuration, one-time setup):\n  \
    hyalo init [--claude] [-d/--dir DIR]\n\n  \
  Global flags (apply to all commands):\n  \
    -d/--dir <DIR>      Root directory (default: ., override via .hyalo.toml)\n  \
    --format json|text  Output format (default: json, override via .hyalo.toml)\n  \
    --jq <FILTER>       Apply a jq expression to JSON output\n  \
    --hints             Append drill-down hints (default: off, override via .hyalo.toml)\n  \
    --no-hints          Disable hints (overrides .hyalo.toml)\n\n\
COOKBOOK:\n  \
  # Discover what metadata exists in a vault\n  \
  hyalo properties\n  \
  hyalo tags\n\n  \
  # Get a vault overview with drill-down hints\n  \
  hyalo summary --format text --hints\n\n  \
  # Find all files with status=draft\n  \
  hyalo find --property status=draft\n\n  \
  # Find files tagged 'project' (matches project/backend, project/frontend, etc.)\n  \
  hyalo find --tag project\n\n  \
  # Find files with open tasks\n  \
  hyalo find --task todo\n\n  \
  # Find files with a specific section heading\n  \
  hyalo find --section 'Tasks'\n\n  \
  # Find open tasks within a specific section\n  \
  hyalo find --section '## Sprint' --task todo\n\n  \
  # Find broken [[wikilinks]] (fields=links, then filter in jq)\n  \
  hyalo find --fields links --jq '[.[] | select(.links | map(select(.path == null)) | length > 0)]'\n\n  \
  # Tag all research notes in a folder\n  \
  hyalo set --tag reviewed --glob 'research/**/*.md'\n\n  \
  # Bulk-update a property across matching files\n  \
  hyalo set --property status=in-progress --where-property status=draft --glob '**/*.md'\n\n  \
  # Add a tag to files matching a tag filter\n  \
  hyalo set --tag reviewed --where-tag research --glob '**/*.md'\n\n  \
  # Append to a list property\n  \
  hyalo append --property aliases='My Note' --file note.md\n\n  \
  # Quick vault overview\n  \
  hyalo summary --format text\n\n  \
  # Count tasks across all files\n  \
  hyalo summary --jq '.tasks.total'\n\n  \
  # List all property names as a flat list\n  \
  hyalo properties --jq '[.[].name] | join(\", \")'\n\n  \
  # Get just file paths (no metadata)\n  \
  hyalo find --property status=draft --jq '[.[].file]'\n\n  \
  # Pipe file paths for scripting (Unix)\n  \
  hyalo find --tag research --jq '.[].file' | xargs -I{} hyalo set --property reviewed=true --file {}\n\n\
OUTPUT SHAPES (JSON, default):\n  \
  # find\n  \
  [{\"file\": \"notes/todo.md\", \"modified\": \"2026-03-21T...\",\n   \
    \"properties\": {\"status\": \"draft\", \"title\": \"My Note\"},\n   \
    \"tags\": [...], \"sections\": [...], \"tasks\": [...], \"links\": [...]}]\n\n  \
  # set / remove / append (mutation result)\n  \
  {\"property\": \"status\", \"value\": \"completed\", \"modified\": [...], \"skipped\": [...], \"total\": N}\n  \
  {\"tag\": \"reviewed\", \"modified\": [...], \"skipped\": [...], \"total\": N}\n\n  \
  # properties\n  \
  [{\"name\": \"status\", \"type\": \"text\", \"count\": 21}, ...]\n\n  \
  # tags\n  \
  {\"tags\": [{\"name\": \"backlog\", \"count\": 10}, ...], \"total\": 31}\n\n  \
  # task read / toggle / set-status\n  \
  {\"file\": \"todo.md\", \"line\": 5, \"status\": \"x\", \"text\": \"Fix bug\", \"done\": true}\n\n  \
  # summary\n  \
  {\"files\": {\"total\": 31, \"by_directory\": [...]}, \"properties\": [...], \"tags\": {...},\n   \
  \"status\": [{\"value\": \"draft\", \"files\": [...]}], \"tasks\": {\"total\": 50, \"done\": 30},\n   \
  \"recent_files\": [{\"path\": \"note.md\", \"modified\": \"2026-03-21T...\"}]}\n\n  \
  # --hints wraps JSON output in an envelope with drill-down commands\n  \
  {\"data\": { ... original output ... }, \"hints\": [\"hyalo properties\", ...]}\n\n  \
  # errors (stderr, exit code 1 for user errors, 2 for internal)\n  \
  {\"error\": \"property not found\", \"path\": \"notes/todo.md\"}\n\n  \
  # --format text produces human-readable output on all commands"
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
    /// Example: --jq '.files[]' or --jq 'map(.name) | join(", ")'
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
            whole-string match; use leading '#' to pin heading level, e.g. '## Tasks'). Repeatable (OR). \
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
        /// Section heading filter: restrict body results to matching sections (case-insensitive whole-string match;
        /// use leading '#' to pin heading level, e.g. '## Tasks'). Repeatable (OR)
        #[arg(short, long = "section", value_name = "HEADING")]
        sections: Vec<String>,
        /// Scan only this file (still returns an array)
        #[arg(short, long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select files
        #[arg(short, long, conflicts_with = "file")]
        glob: Option<String>,
        /// Comma-separated list of optional fields to include: properties, properties-typed, tags, sections, tasks, links (default: all). 'file' and 'modified' are always included. 'properties' is a {key: value} map; 'properties-typed' is a [{name, type, value}] array
        #[arg(long, value_name = "FIELDS", use_value_delimiter = true)]
        fields: Vec<String>,
        /// Sort order: 'file' (default) or 'modified'
        #[arg(long)]
        sort: Option<String>,
        /// Maximum number of results to return
        #[arg(short = 'n', long)]
        limit: Option<usize>,
    },
    /// Read file body content, optionally filtered by section or line range (read-only)
    #[command(long_about = "Read the body content of a markdown file.\n\n\
            Returns the raw text after the YAML frontmatter block. Use --section to extract a \
            specific section by heading (case-insensitive whole-string match; use leading '#' to \
            pin heading level, e.g. '## Tasks'; nested subsections are included), \
            --lines to slice a line range, and --frontmatter to include the YAML frontmatter.\n\n\
            OUTPUT: Defaults to plain text (overrides the global json default). \
            Pass --format json explicitly to get \
            {\"file\": \"...\", \"content\": \"...\"}.\n\
            SIDE EFFECTS: None (read-only).")]
    Read {
        /// Target file (relative to --dir)
        #[arg(short, long)]
        file: String,
        /// Extract the section(s) matching this heading (case-insensitive whole-string match;
        /// use leading '#' to pin heading level, e.g. '## Tasks'). Nested subsections are included
        #[arg(short, long, value_name = "HEADING")]
        section: Option<String>,
        /// Slice output by line range: 5:10, 5:, :10, or 5 (1-based, inclusive, relative to body content)
        #[arg(short, long)]
        lines: Option<String>,
        /// Include the YAML frontmatter in output
        #[arg(long)]
        frontmatter: bool,
    },
    /// Show unique property names with types and file counts across matched files (read-only)
    #[command(
        long_about = "Aggregate summary of frontmatter properties across matched files.\n\n\
            OUTPUT: List of unique property names, their inferred type, and how many files contain them.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to discover what properties exist or audit frontmatter across a vault."
    )]
    Properties {
        /// Glob pattern to select files (e.g. '**/*.md', 'notes/*.md')
        #[arg(short, long)]
        glob: Option<String>,
    },
    /// Show unique tags with file counts across matched files (read-only)
    #[command(long_about = "Aggregate summary of tags across matched files.\n\n\
            OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to see which tags exist, find popular/orphan tags, or audit tag taxonomy.")]
    Tags {
        /// Glob pattern to filter which files to scan (e.g. 'notes/**/*.md')
        #[arg(short, long)]
        glob: Option<String>,
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
            task counts (total/done), and recently modified files.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need a quick overview of a vault's metadata landscape.")]
    Summary {
        /// Glob pattern to filter which files to include
        #[arg(short, long)]
        glob: Option<String>,
        /// Number of recent files to show (default: 10)
        #[arg(short = 'n', long, default_value = "10")]
        recent: usize,
        /// Limit directory listing depth (0 = root only; stats are always full)
        #[arg(long)]
        depth: Option<usize>,
    },
    /// Set (create or overwrite) frontmatter properties and/or add tags across file(s)
    #[command(
        long_about = "Set (create or overwrite) frontmatter properties and/or add tags across file(s).\n\n\
            INPUT: One or more --property K=V arguments and/or --tag T arguments, with --file or --glob.\n\
            BEHAVIOR:\n\
            - --property K=V: creates or overwrites the property. Type is auto-inferred from V \
              (number, bool, date, text). A file is skipped if the stored value is already identical.\n\
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
        /// Target a single file
        #[arg(short, long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern for multiple files
        #[arg(short, long, conflicts_with = "file")]
        glob: Option<String>,
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
        /// Target a single file
        #[arg(short, long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern for multiple files
        #[arg(short, long, conflicts_with = "file")]
        glob: Option<String>,
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
        /// Target a single file
        #[arg(short, long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern for multiple files
        #[arg(short, long, conflicts_with = "file")]
        glob: Option<String>,
        /// Filter: only mutate files whose frontmatter property matches (repeatable, AND). Same syntax as find --property
        #[arg(long = "where-property", value_name = "FILTER")]
        where_properties: Vec<String>,
        /// Filter: only mutate files with this tag (repeatable, AND). Same syntax as find --tag
        #[arg(long = "where-tag", value_name = "TAG")]
        where_tags: Vec<String>,
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
            process::exit(1);
        }
    };
    for tag in where_tags {
        if let Err(msg) = hyalo_cli::commands::tags::validate_tag(tag) {
            eprintln!("Error: {msg}");
            process::exit(1);
        }
    }
    filters
}

#[allow(clippy::too_many_lines)]
fn main() {
    let cli = Cli::parse();

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
        process::exit(code);
    }

    // Load per-project config from .hyalo.toml in CWD
    let config = hyalo_cli::config::load_config();

    // Merge: CLI args override config, config overrides hardcoded defaults.
    // Track whether --dir was explicitly passed (not from config) so hints
    // can omit it when the user relies on .hyalo.toml.
    let dir_from_cli = cli.dir.is_some();
    let format_from_cli = cli.format.is_some();
    let hints_from_cli = cli.hints;
    let dir = cli.dir.unwrap_or(config.dir);
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
                process::exit(2);
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
        process::exit(2);
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
            Commands::Properties { glob } => Some(HintContext {
                source: HintSource::PropertiesSummary,
                dir: dir_hint,
                glob: glob.clone(),
                format: format_hint,
                hints: hints_from_cli,
            }),
            Commands::Tags { glob } => Some(HintContext {
                source: HintSource::TagsSummary,
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
            limit,
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
                    process::exit(1);
                }
            };
            // Parse task filter
            let task_filter = match task.as_deref().map(filter::parse_task_filter) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
                None => None,
            };
            // Parse fields
            let parsed_fields = match filter::Fields::parse(fields) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
                }
            };
            // Parse sort
            let sort_field = match sort.as_deref().map(filter::parse_sort) {
                Some(Ok(f)) => Some(f),
                Some(Err(e)) => {
                    eprintln!("Error: {e}");
                    process::exit(1);
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
                    process::exit(1);
                }
            };

            find_commands::find(
                &dir,
                pattern.as_deref(),
                regexp.as_deref(),
                &prop_filters,
                tag,
                task_filter.as_ref(),
                &section_filters,
                file.as_deref(),
                glob.as_deref(),
                &parsed_fields,
                sort_field.as_ref(),
                limit,
                effective_format,
            )
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
        Commands::Properties { ref glob } => {
            properties::properties_summary(&dir, None, glob.as_deref(), effective_format)
        }
        Commands::Tags { ref glob } => {
            tag_commands::tags_summary(&dir, None, glob.as_deref(), effective_format)
        }
        Commands::Task { action } => match action {
            TaskAction::Read { ref file, line } => {
                task_commands::task_read(&dir, file, line, effective_format)
            }
            TaskAction::Toggle { ref file, line } => {
                task_commands::task_toggle(&dir, file, line, effective_format)
            }
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
                    process::exit(1);
                }
                task_commands::task_set_status(
                    &dir,
                    file,
                    line,
                    status.chars().next().unwrap(),
                    effective_format,
                )
            }
        },
        Commands::Summary {
            ref glob,
            recent,
            depth,
        } => summary_commands::summary(&dir, glob.as_deref(), recent, depth, effective_format),
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
                file.as_deref(),
                glob.as_deref(),
                &where_prop_filters,
                where_tags,
                effective_format,
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
                file.as_deref(),
                glob.as_deref(),
                &where_prop_filters,
                where_tags,
                effective_format,
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
                file.as_deref(),
                glob.as_deref(),
                &where_prop_filters,
                where_tags,
                effective_format,
            )
        }
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
                        process::exit(2);
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
                        process::exit(1);
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
                        process::exit(0);
                    }
                };
                let hints = generate_hints(ctx, &value);
                let formatted = format_with_hints(format, &value, &hints);
                println!("{formatted}");
            } else {
                println!("{output}");
            }
        }
        Ok(CommandOutcome::UserError(output)) => {
            eprintln!("{output}");
            process::exit(1);
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
            process::exit(2);
        }
    }
}
