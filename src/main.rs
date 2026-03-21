use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use hyalo::commands::{
    links as link_commands, outline as outline_commands, properties, tags as tag_commands,
};
use hyalo::output::{CommandOutcome, Format, apply_jq_filter_result};

#[derive(Parser)]
#[command(
    name = "hyalo",
    version,
    about = "CLI tool for reading and modifying YAML frontmatter and [[wikilinks]] in Obsidian-compatible markdown files",
    long_about = "Hyalo is a CLI tool for reading and modifying YAML frontmatter and [[wikilinks]] \
        in Obsidian-compatible markdown (.md) files.\n\n\
        SCOPE: Hyalo operates on a directory of .md files. It can query and mutate frontmatter \
        properties and tags, and inspect wikilink resolution.\n\n\
        PATH RESOLUTION: All --file and --glob paths are relative to --dir (defaults to \".\"). \
        Globs use standard syntax: '**/*.md' matches recursively, 'notes/*.md' matches one level.\n\n\
        OUTPUT: Returns JSON by default (--format json). Use --format text for human-readable output. \
        Successful output goes to stdout; errors go to stderr with exit code 1 (user error) or 2 (internal error).\n\n\
        COMMANDS: Use 'properties'/'tags' to list across files. Use 'property' for read/set/remove/find \
        and list operations (add-to-list, remove-from-list). Use 'tag' for tag-specific find/add/remove. \
        Use 'links' to inspect wikilink targets.",
    after_help = "EXAMPLES:\n  \
        Aggregate property summary: hyalo properties summary\n  \
        Per-file property detail:   hyalo properties list --glob '**/*.md'\n  \
        Set a property:             hyalo property set --name status --value done --file notes/todo.md\n  \
        Find files by property:     hyalo property find --name status --value draft\n  \
        Add to a list property:     hyalo property add-to-list --name aliases --value \"My Note\" --file note.md\n  \
        Remove from a list:         hyalo property remove-from-list --name authors --value Alice --file note.md\n  \
        Aggregate tag summary:      hyalo tags summary\n  \
        Per-file tag detail:        hyalo tags list --glob 'notes/**/*.md'\n  \
        Find files by tag:          hyalo tag find --name project/backend\n  \
        Find broken wikilinks:      hyalo links --file index.md --unresolved",
    after_long_help = "\
COMMAND REFERENCE:\n  \
  Properties (list across files):\n  \
    hyalo properties summary  [--file F | --glob G]       Unique names, types, file counts\n  \
    hyalo properties list     [--file F | --glob G]       Per-file key/value detail\n\n  \
  Property (single-property operations):\n  \
    hyalo property read       --name N --file F           Read one property value\n  \
    hyalo property set        --name N --value V [--type T] --file F\n  \
    hyalo property remove     --name N --file F           Delete a property\n  \
    hyalo property find       --name N [--value V] [--file F | --glob G]\n  \
    hyalo property add-to-list       --name N --value V [--value ...] --file F | --glob G\n  \
    hyalo property remove-from-list  --name N --value V [--value ...] --file F | --glob G\n\n  \
  Tags (list across files):\n  \
    hyalo tags summary        [--file F | --glob G]       Unique tags with file counts\n  \
    hyalo tags list           [--file F | --glob G]       Per-file tag arrays\n\n  \
  Tag (single-tag operations):\n  \
    hyalo tag find            --name N [--file F | --glob G]   Supports nested matching\n  \
    hyalo tag add             --name N --file F | --glob G     Idempotent\n  \
    hyalo tag remove          --name N --file F | --glob G\n\n  \
  Links:\n  \
    hyalo links               --file F [--unresolved | --resolved]\n\n  \
  Outline:\n  \
    hyalo outline             [--file F | --glob G]       Structure, tasks, links per section\n\n  \
  Global flags (apply to all commands):\n  \
    --dir <DIR>         Root directory (default: .)\n  \
    --format json|text  Output format (default: json)\n  \
    --jq <FILTER>       Apply a jq expression to JSON output\n\n\
COOKBOOK:\n  \
  # Discover what metadata exists in a vault\n  \
  hyalo properties summary\n  \
  hyalo tags summary\n\n  \
  # See all properties of a specific file\n  \
  hyalo properties list --file notes/todo.md\n\n  \
  # Find all files with status=draft\n  \
  hyalo property find --name status --value draft\n\n  \
  # Find files tagged 'project' (matches project/backend, project/frontend, etc.)\n  \
  hyalo tag find --name project\n\n  \
  # Tag all research notes in a folder\n  \
  hyalo tag add --name reviewed --glob 'research/**/*.md'\n\n  \
  # Bulk-update a property across files\n  \
  hyalo property find --name status --value draft --jq '.files[]' \\\n    \
    | xargs -I{} hyalo property set --name status --value in-progress --file {}\n\n  \
  # Find broken [[wikilinks]] in a file\n  \
  hyalo links --file index.md --unresolved\n\n  \
  # Get document structure: headings, tasks, code blocks\n  \
  hyalo outline --file notes/meeting.md --format text\n\n  \
  # Count tasks across all files\n  \
  hyalo outline --glob '**/*.md' --jq '[.[].sections[].tasks // empty] | map(.total) | add'\n\n  \
  # Extract just file paths from a tag search\n  \
  hyalo tag find --name backlog --jq '.files[]'\n\n  \
  # List all property names as a flat list\n  \
  hyalo properties summary --jq '[.[].name] | join(\", \")'\n\n  \
  # Pipe JSON through external jq for complex queries\n  \
  hyalo outline --glob '**/*.md' | jq '[.[] | {file, headings: [.sections[].heading]}]'\n\n\
OUTPUT SHAPES (JSON, default):\n  \
  # properties summary\n  \
  [{\"name\": \"status\", \"type\": \"text\", \"count\": 21}, ...]\n\n  \
  # properties list (--file → bare object, --glob/default → array)\n  \
  {\"path\": \"notes/todo.md\", \"properties\": [{\"name\": \"status\", \"type\": \"text\", \"value\": \"draft\"}, ...]}\n\n  \
  # property read\n  \
  {\"name\": \"status\", \"type\": \"text\", \"value\": \"draft\"}\n\n  \
  # property set (echoes the written value)\n  \
  {\"name\": \"status\", \"type\": \"text\", \"value\": \"done\"}\n\n  \
  # property remove\n  \
  {\"path\": \"notes/todo.md\", \"removed\": \"status\"}\n\n  \
  # property find\n  \
  {\"property\": \"status\", \"value\": \"draft\", \"files\": [\"a.md\", \"b.md\"], \"total\": 2}\n\n  \
  # property add-to-list / remove-from-list\n  \
  {\"property\": \"tags\", \"values\": [\"rust\"], \"modified\": [\"a.md\"], \"skipped\": [\"b.md\"], \"total\": 2}\n\n  \
  # tags summary\n  \
  {\"tags\": [{\"name\": \"backlog\", \"count\": 10}, ...], \"total\": 31}\n\n  \
  # tags list (--file → bare object, --glob/default → array)\n  \
  {\"path\": \"notes/todo.md\", \"tags\": [\"backlog\", \"cli\"]}\n\n  \
  # tag find\n  \
  {\"tag\": \"backlog\", \"files\": [\"a.md\", \"b.md\"], \"total\": 2}\n\n  \
  # tag add / remove (mutation result)\n  \
  {\"tag\": \"reviewed\", \"modified\": [\"a.md\"], \"skipped\": [\"b.md\"], \"total\": 2}\n\n  \
  # links\n  \
  {\"path\": \"index.md\", \"links\": [{\"target\": \"notes/todo\", \"path\": \"notes/todo.md\", \"label\": null}, ...]}\n  \
  # (unresolved links have \"path\": null)\n\n  \
  # outline (--file → bare object, --glob/default → array)\n  \
  {\"file\": \"notes/todo.md\", \"properties\": [...], \"tags\": [...],\n   \
  \"sections\": [{\"level\": 1, \"heading\": \"Title\", \"line\": 5, \"links\": [],\n   \
                  \"tasks\": {\"total\": 3, \"done\": 1}, \"code_blocks\": [\"rust\"]}]}\n\n  \
  # errors (stderr, exit code 1 for user errors, 2 for internal)\n  \
  {\"error\": \"property not found\", \"path\": \"notes/todo.md\"}\n\n  \
  # --format text produces human-readable output on all commands"
)]
struct Cli {
    /// Root directory for resolving all --file and --glob paths. Defaults to current directory
    #[arg(long, global = true, default_value = ".")]
    dir: PathBuf,

    /// Output format: "json" (structured, default) or "text" (human-readable). Applies to both stdout and stderr
    #[arg(long, global = true, default_value = "json")]
    format: String,

    /// Apply a jq filter expression to the JSON output of any command.
    /// The filtered result is printed as plain text. Incompatible with non-JSON formats (--format text).
    /// Example: --jq '.files[]' or --jq 'map(.name) | join(", ")'
    #[arg(long, global = true, value_name = "FILTER")]
    jq: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List frontmatter properties across files — aggregate summary or per-file detail
    #[command(long_about = "List frontmatter properties across files.\n\n\
            Subcommands:\n\
            - summary (default): aggregate unique property names with types and file counts.\n\
            - list: per-file detail — each file with its full key/value pairs.\n\n\
            INPUT: Reads .md files filtered by --file or --glob (or all .md files if omitted).\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --file or --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to discover what properties exist, audit frontmatter, or inspect per-file metadata.")]
    Properties {
        #[command(subcommand)]
        action: Option<PropertiesAction>,
    },
    /// Read, set, find, or remove frontmatter properties; add/remove items from list properties
    #[command(
        long_about = "Read, set, find, or remove frontmatter properties; add/remove items from list properties.\n\n\
        Subcommands:\n\
        - read: Get a single property value from one file (read-only).\n\
        - set: Create or overwrite a property on one file.\n\
        - remove: Delete a property from one file.\n\
        - find: Search files by property existence or value (read-only).\n\
        - add-to-list: Append values to a list property across file(s).\n\
        - remove-from-list: Remove values from a list property across file(s).\n\n\
        INPUT: Property name (--name) and file (--file) or scope (--glob) depending on subcommand.\n\
        SCOPE: 'read', 'set', and 'remove' operate on a single --file. 'find', 'add-to-list', and 'remove-from-list' support --file or --glob.\n\
        SIDE EFFECTS: 'set', 'remove', 'add-to-list', and 'remove-from-list' modify files on disk. 'read' and 'find' are read-only.\n\
        USE WHEN: You need to read, write, or search frontmatter properties in one or more files."
    )]
    Property {
        #[command(subcommand)]
        action: PropertyAction,
    },
    /// List outgoing [[wikilinks]] from a file and their resolution status (read-only)
    #[command(
        long_about = "List outgoing [[wikilinks]] from a file and their resolution status.\n\n\
            INPUT: A single markdown file (--file).\n\
            OUTPUT: Each [[wikilink]] found in the file body. The 'path' field is the resolved \
            file path (string) if the link target exists under --dir, or null if unresolved.\n\
            FILTERS: --unresolved returns only broken links. --resolved returns only valid links. \
            Without either flag, returns all links.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to find broken links, audit cross-references, or build a link graph."
    )]
    Links {
        /// Markdown file to scan for [[wikilinks]]
        #[arg(long)]
        file: String,
        /// Filter: only return links whose target file does NOT exist (broken links)
        #[arg(long, conflicts_with = "resolved")]
        unresolved: bool,
        /// Filter: only return links whose target file exists
        #[arg(long, conflicts_with = "unresolved")]
        resolved: bool,
    },
    /// List tags across files — aggregate summary or per-file detail
    #[command(long_about = "List tags across files.\n\n\
            Subcommands:\n\
            - summary (default): aggregate unique tag names with file counts. Tags are compared case-insensitively.\n\
            - list: per-file detail — each file with its tags array.\n\n\
            INPUT: Reads the 'tags' field from YAML frontmatter in matched files.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --file or --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to see which tags exist, find popular/orphan tags, audit tag taxonomy, or inspect per-file tags.")]
    Tags {
        #[command(subcommand)]
        action: Option<TagsAction>,
    },
    /// Find, add, or remove tags in file frontmatter
    #[command(long_about = "Find, add, or remove tags in file frontmatter.\n\n\
        Subcommands:\n\
        - find: Search files by tag name or prefix (read-only).\n\
        - add: Append a tag to file(s) frontmatter.\n\
        - remove: Delete a tag from file(s) frontmatter.\n\n\
        INPUT: Tag name (--name) and file (--file) or scope (--glob).\n\
        SCOPE: 'find', 'add', and 'remove' support --file or --glob; defaults to all .md files.\n\
        SIDE EFFECTS: 'add' and 'remove' modify files on disk. 'find' is read-only.\n\
        USE WHEN: You need to find files by tag, or add/remove tags across one or more files.\n\
        NESTED TAG MATCHING: Tag names can be hierarchical (e.g. 'project/backend'). \
        Searching for a parent tag like 'project' matches all children ('project/backend', 'project/frontend').")]
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
    /// Build a structural outline of one or more markdown files (read-only)
    #[command(
        long_about = "Build a structural outline of one or more markdown files.\n\n\
            OUTPUT: For each file, returns a 'FileOutline' object containing:\n\
            - 'file': relative path to the file\n\
            - 'properties': frontmatter key/value pairs with inferred types\n\
            - 'tags': tag list extracted from frontmatter\n\
            - 'sections': ordered list of document sections, each with:\n\
                - 'level': heading depth (1-6); 0 = pre-heading content\n\
                - 'heading': heading text (null for level-0 pre-heading section)\n\
                - 'line': 1-based line number of the heading\n\
                - 'links': internal [[wikilinks]] and [label](target) links found in the section\n\
                - 'tasks': checkbox counts ({total, done}) — omitted if the section has no tasks\n\
                - 'code_blocks': list of fenced code block language tags found in the section\n\
            INPUT: Single file via --file (returns bare object), glob via --glob (returns array),\n\
            or all .md files under --dir when neither is provided (returns array).\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to understand document structure, extract navigation data, \
            audit which sections contain tasks or links, or build a document map."
    )]
    Outline {
        /// Markdown file to outline (relative to --dir); returns a bare object
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select multiple files (e.g. '**/*.md'); returns an array
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

/// Subcommands for `hyalo properties`
#[derive(Subcommand)]
enum PropertiesAction {
    /// Aggregate: unique property names with types and file counts (default)
    #[command(
        long_about = "Aggregate summary of frontmatter properties across matched files.\n\n\
            OUTPUT: List of unique property names, their inferred type, and how many files contain them.\n\
            SCOPE: Filtered by --file or --glob; defaults to all .md files under --dir.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Summary {
        /// Scan only this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select files (e.g. '**/*.md', 'notes/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Per-file detail: each file with its property key/value pairs
    #[command(
        long_about = "Per-file detail: each matched file with its full frontmatter key/value pairs.\n\n\
            OUTPUT: Array of objects, each with 'path' and 'properties' (key → {value, type}).\n\
            SCOPE: Filtered by --file or --glob; defaults to all .md files under --dir.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    List {
        /// Scan only this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select files (e.g. '**/*.md', 'notes/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

/// Subcommands for `hyalo tags`
#[derive(Subcommand)]
enum TagsAction {
    /// Aggregate: unique tag names with file counts (default)
    #[command(long_about = "Aggregate summary of tags across matched files.\n\n\
            OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
            SCOPE: Filtered by --file or --glob; defaults to all .md files under --dir.\n\
            SIDE EFFECTS: None (read-only).")]
    Summary {
        /// Scan only this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to filter which files to scan (e.g. 'notes/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Per-file detail: each file with its tags array
    #[command(
        long_about = "Per-file detail: each matched file with its tags array.\n\n\
            OUTPUT: Array of objects, each with 'path' and 'tags' (list of tag strings).\n\
            Files without frontmatter or without a 'tags' key appear with an empty tags array.\n\
            SCOPE: Filtered by --file or --glob; defaults to all .md files under --dir.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    List {
        /// Scan only this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to filter which files to scan (e.g. 'notes/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

#[derive(Subcommand)]
enum TagAction {
    /// Find files containing a specific tag (read-only, supports nested tag matching)
    #[command(long_about = "Find files containing a specific tag.\n\n\
            INPUT: A tag name and optionally a file scope (--file or --glob).\n\
            OUTPUT: List of files whose frontmatter 'tags' contain the given tag.\n\
            NESTED MATCHING: Searching for 'project' also matches 'project/backend', \
            'project/frontend', etc. Exact match on 'project/backend' does NOT match 'project'.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to find all files with a given tag or tag prefix.")]
    Find {
        /// Tag name or prefix to search for (e.g. 'status', 'project/backend')
        #[arg(long)]
        name: String,
        /// Search only in this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to limit which files to search (e.g. 'docs/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Add a tag to file(s) frontmatter (mutates files on disk)
    #[command(long_about = "Add a tag to file(s) frontmatter.\n\n\
            INPUT: A tag name and target file(s) via --file or --glob.\n\
            BEHAVIOR: Appends the tag to the 'tags' list in YAML frontmatter. \
            Creates the 'tags' field if it doesn't exist. Idempotent: skips files that already have the tag.\n\
            SIDE EFFECTS: Modifies matched files on disk.\n\
            USE WHEN: You need to categorize or label files by adding a tag.")]
    Add {
        /// Tag name to add (e.g. 'reviewed', 'project/backend')
        #[arg(long)]
        name: String,
        /// Add the tag to this single file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select multiple files to tag
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Remove a tag from file(s) frontmatter (mutates files on disk)
    #[command(long_about = "Remove a tag from file(s) frontmatter.\n\n\
            INPUT: A tag name and target file(s) via --file or --glob.\n\
            BEHAVIOR: Removes the exact tag from the 'tags' list. \
            Reports an error if the tag is not present in a file.\n\
            SIDE EFFECTS: Modifies matched files on disk.\n\
            USE WHEN: You need to un-tag or re-categorize files.")]
    Remove {
        /// Tag name to remove (must match exactly)
        #[arg(long)]
        name: String,
        /// Remove the tag from this single file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to select multiple files to untag
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

#[derive(Subcommand)]
enum PropertyAction {
    /// Read a single frontmatter property value from a file (read-only)
    #[command(
        long_about = "Read a single frontmatter property value from a file.\n\n\
            INPUT: A property name (--name) and file path (--file).\n\
            OUTPUT: The property's value and inferred type.\n\
            ERROR: Returns an error if the file has no frontmatter or the property does not exist.\n\
            SIDE EFFECTS: None (read-only)."
    )]
    Read {
        /// Frontmatter property name to read (e.g. 'title', 'status', 'date')
        #[arg(long)]
        name: String,
        /// Markdown file to read from (relative to --dir)
        #[arg(long)]
        file: String,
    },
    /// Set (create or overwrite) a frontmatter property (mutates file on disk)
    #[command(long_about = "Set (create or overwrite) a frontmatter property.\n\n\
            INPUT: Property name (--name), value (--value), optional type (--type), and file (--file).\n\
            BEHAVIOR: Creates the property if absent, overwrites if present. \
            Creates YAML frontmatter if the file has none.\n\
            TYPE INFERENCE: Without --type, the value type is auto-detected: \
            'true'/'false' → checkbox, '2024-01-15' → date, '42' → number, comma-separated → list, else text. \
            Use --type to override (one of: text, number, checkbox, date, datetime, list).\n\
            SIDE EFFECTS: Modifies the file on disk.")]
    Set {
        /// Property name to create or overwrite (e.g. 'status', 'priority')
        #[arg(long)]
        name: String,
        /// Value to assign (interpreted according to --type or auto-detected)
        #[arg(long)]
        value: String,
        /// Force value type instead of auto-detecting. One of: text, number, checkbox, date, datetime, list
        #[arg(long = "type")]
        prop_type: Option<String>,
        /// Markdown file to modify (relative to --dir)
        #[arg(long)]
        file: String,
    },
    /// Remove a frontmatter property from a file (mutates file on disk)
    #[command(long_about = "Remove a frontmatter property from a file.\n\n\
            INPUT: Property name (--name) and file path (--file).\n\
            BEHAVIOR: Deletes the named key from YAML frontmatter.\n\
            ERROR: Returns an error if the property does not exist.\n\
            SIDE EFFECTS: Modifies the file on disk.")]
    Remove {
        /// Property name to delete from frontmatter
        #[arg(long)]
        name: String,
        /// Markdown file to modify (relative to --dir)
        #[arg(long)]
        file: String,
    },
    /// Find files containing a specific frontmatter property (read-only)
    #[command(
        long_about = "Find files that contain a specific frontmatter property, optionally matching a value.\n\n\
            INPUT: A property name (--name), an optional value filter (--value), and an optional file scope (--file or --glob).\n\
            OUTPUT: List of files whose frontmatter contains the given property (and matching value if --value is provided).\n\
            VALUE MATCHING: If --value is given, the comparison is type-aware:\n\
              - String properties: case-insensitive string comparison.\n\
              - Number properties: numeric comparison (parse --value as a number).\n\
              - Boolean properties: parse --value as true/false/yes/no/1/0 (case-insensitive for words).\n\
              - List properties: match if any element equals --value (case-insensitive for strings).\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to find files with a particular metadata property or a specific value for that property."
    )]
    Find {
        /// Property name to search for (e.g. 'status', 'priority', 'draft')
        #[arg(long)]
        name: String,
        /// Optional value to match (e.g. 'draft', '3', 'true'). If omitted, matches any file that has the property
        #[arg(long)]
        value: Option<String>,
        /// Search only in this file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to limit which files to search (e.g. 'docs/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Add values to a list property in file(s) frontmatter (mutates files on disk)
    #[command(
        long_about = "Add values to a list property in file(s) frontmatter.\n\n\
            INPUT: A property name (--name), one or more values (--value, repeatable), and target file(s) via --file or --glob.\n\
            BEHAVIOR: Reads the current list for the named property (or treats it as empty if absent). \
            Appends each value that is not already present (comparison is case-insensitive for strings). \
            If the property does not exist it is created. If it exists as a scalar string it is promoted to a list.\n\
            IDEMPOTENT: Files where all requested values already exist are reported as 'skipped', not modified.\n\
            OUTPUT: {\"property\": name, \"values\": [...], \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            SIDE EFFECTS: Modifies matched files on disk.\n\
            USE WHEN: You need to append items to any list-type frontmatter property such as 'tags', 'aliases', or 'authors'."
    )]
    AddToList {
        /// Property name (e.g. 'tags', 'aliases', 'authors')
        #[arg(long)]
        name: String,
        /// Values to add (can be specified multiple times, e.g. --value rust --value cli)
        #[arg(long, required = true)]
        value: Vec<String>,
        /// Target a single file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern for multiple files (e.g. 'notes/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Remove values from a list property in file(s) frontmatter (mutates files on disk)
    #[command(
        long_about = "Remove values from a list property in file(s) frontmatter.\n\n\
            INPUT: A property name (--name), one or more values (--value, repeatable), and target file(s) via --file or --glob.\n\
            BEHAVIOR: Reads the current list for the named property and removes any matching values \
            (comparison is case-insensitive for strings). If the list becomes empty after removal, \
            the entire property key is deleted from frontmatter.\n\
            IDEMPOTENT: Files where none of the requested values are present are reported as 'skipped', not an error.\n\
            OUTPUT: {\"property\": name, \"values\": [...], \"modified\": [...], \"skipped\": [...], \"total\": N}\n\
            SIDE EFFECTS: Modifies matched files on disk.\n\
            USE WHEN: You need to remove items from any list-type frontmatter property such as 'tags', 'aliases', or 'authors'."
    )]
    RemoveFromList {
        /// Property name (e.g. 'tags', 'aliases', 'authors')
        #[arg(long)]
        name: String,
        /// Values to remove (can be specified multiple times, e.g. --value rust --value cli)
        #[arg(long, required = true)]
        value: Vec<String>,
        /// Target a single file
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern for multiple files (e.g. 'notes/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

#[allow(clippy::too_many_lines)]
fn main() {
    let cli = Cli::parse();

    let Some(format) = Format::from_str_opt(&cli.format) else {
        eprintln!(
            "Error: invalid format '{}', expected 'json' or 'text'",
            cli.format
        );
        process::exit(2);
    };

    // --jq operates on JSON, so it conflicts with an explicit --format text.
    let jq_filter = cli.jq.as_deref();
    if jq_filter.is_some() && format != Format::Json {
        eprintln!(
            "Error: --jq cannot be combined with --format {}",
            cli.format
        );
        eprintln!("  --jq always operates on JSON output; drop --format or use --format json");
        process::exit(2);
    }
    let effective_format = if jq_filter.is_some() {
        Format::Json
    } else {
        format
    };

    let dir = &cli.dir;

    let result = match cli.command {
        Commands::Properties { action: ref pa } => match pa {
            None
            | Some(PropertiesAction::Summary {
                file: None,
                glob: None,
            }) => properties::properties_summary(dir, None, None, effective_format),
            Some(PropertiesAction::Summary { file, glob }) => properties::properties_summary(
                dir,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
            Some(PropertiesAction::List { file, glob }) => {
                properties::properties_list(dir, file.as_deref(), glob.as_deref(), effective_format)
            }
        },
        Commands::Property { action } => match action {
            PropertyAction::Read { ref name, ref file } => {
                properties::property_read(dir, name, file, effective_format)
            }
            PropertyAction::Set {
                ref name,
                ref value,
                ref prop_type,
                ref file,
            } => properties::property_set(
                dir,
                name,
                value,
                prop_type.as_deref(),
                file,
                effective_format,
            ),
            PropertyAction::Remove { ref name, ref file } => {
                properties::property_remove(dir, name, file, effective_format)
            }
            PropertyAction::Find {
                ref name,
                ref value,
                ref file,
                ref glob,
            } => properties::property_find(
                dir,
                name,
                value.as_deref(),
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
            PropertyAction::AddToList {
                ref name,
                ref value,
                ref file,
                ref glob,
            } => properties::property_add_to_list(
                dir,
                name,
                value,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
            PropertyAction::RemoveFromList {
                ref name,
                ref value,
                ref file,
                ref glob,
            } => properties::property_remove_from_list(
                dir,
                name,
                value,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
        },
        Commands::Links {
            ref file,
            unresolved,
            resolved,
        } => {
            let filter = if unresolved {
                link_commands::LinkFilter::Unresolved
            } else if resolved {
                link_commands::LinkFilter::Resolved
            } else {
                link_commands::LinkFilter::All
            };
            link_commands::links(dir, file, filter, effective_format)
        }
        Commands::Tags { action: ref ta } => match ta {
            None
            | Some(TagsAction::Summary {
                file: None,
                glob: None,
            }) => tag_commands::tags_summary(dir, None, None, effective_format),
            Some(TagsAction::Summary { file, glob }) => {
                tag_commands::tags_summary(dir, file.as_deref(), glob.as_deref(), effective_format)
            }
            Some(TagsAction::List { file, glob }) => {
                tag_commands::tags_list(dir, file.as_deref(), glob.as_deref(), effective_format)
            }
        },
        Commands::Tag { action } => match action {
            TagAction::Find {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_find(
                dir,
                name,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
            TagAction::Add {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_add(
                dir,
                name,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
            TagAction::Remove {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_remove(
                dir,
                name,
                file.as_deref(),
                glob.as_deref(),
                effective_format,
            ),
        },
        Commands::Outline { ref file, ref glob } => {
            outline_commands::outline(dir, file.as_deref(), glob.as_deref(), effective_format)
        }
    };

    match result {
        Ok(CommandOutcome::Success(output)) => {
            if let Some(filter) = jq_filter {
                // Parse the JSON output we forced above, then apply the user filter.
                let value: serde_json::Value = match serde_json::from_str(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = hyalo::output::format_error(
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
                        let msg = hyalo::output::format_error(
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
            } else {
                println!("{output}");
            }
        }
        Ok(CommandOutcome::UserError(output)) => {
            eprintln!("{output}");
            process::exit(1);
        }
        Err(e) => {
            let msg = hyalo::output::format_error(
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
