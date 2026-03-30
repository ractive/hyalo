/// Short help (shown by `-h`): one example per feature.
pub(crate) const HELP_EXAMPLES: &str = "EXAMPLES:
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
pub(crate) const HELP_LONG: &str = "COMMAND REFERENCE:
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
  hyalo find --fields links --jq '[.results[] | select(.links | map(select(.path == null)) | length > 0)]'

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
  hyalo find --property status=draft --jq '[.results[].file]'

  # Pipe file paths for scripting (Unix)
  hyalo find --tag research --jq '.results[].file' | xargs -I{} hyalo set --property reviewed=true --file {}

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
  {\"total\": N, \"results\": [{\"file\": \"notes/todo.md\", \"modified\": \"2026-03-21T...\",
   \"properties\": {\"status\": \"draft\", \"title\": \"My Note\"},
   \"tags\": [...], \"sections\": [...], \"tasks\": [...], \"links\": [...]}]}

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
  \"orphans\": {\"total\": N, \"files\": [...]}, \"dead_ends\": {\"total\": N, \"files\": [...]},
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
/// - `hide_dir`    -> drop lines that contain `-d/--dir` or ` --dir `
/// - `hide_format` -> drop lines that contain `--format`
pub(crate) fn filter_examples(hide_dir: bool, hide_format: bool) -> String {
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
/// - **COMMAND REFERENCE / Global flags**: line-level -- drop the specific flag
///   rows (`-d/--dir` and/or `--format json|text`) when they are config-defaulted.
/// - **COOKBOOK**: paragraph-level -- each recipe is separated by a blank line.
///   Drop an entire recipe (comment + command) when the command line contains a
///   config-defaulted flag (drops the whole example, not just the flag).
///
/// This keeps the help focused on flags the user actually needs to type.
pub(crate) fn filter_long_help(hide_dir: bool, hide_format: bool) -> String {
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
