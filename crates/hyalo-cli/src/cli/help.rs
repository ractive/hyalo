/// Short help (shown by `-h`): one example per feature.
pub(crate) const HELP_EXAMPLES: &str = "EXAMPLES:
  Search for files:             hyalo find --property status=draft
  Filter by title:              hyalo find --title 'meeting'
  Filter by tag:                hyalo find --tag project
  Filter by task status:        hyalo find --task todo
  Full-text search:             hyalo find 'meeting notes'
  Regex body search:            hyalo find -e 'TODO|FIXME'
  Filter by section:            hyalo find --section 'Tasks' --task todo
  Files with broken links:      hyalo find --broken-links
  Sort and limit:               hyalo find --sort modified --reverse --limit 10
  Count matching files:         hyalo find --tag project --count
  Read file content:            hyalo read notes/todo.md
  Read a section:               hyalo read notes/todo.md --section Proposal
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
  Toggle a task:                hyalo task toggle --file todo.md --line 5
  Find backlinks:               hyalo backlinks decision-log.md
  Move a file (update links):   hyalo mv --file old.md --to new.md
  Move (dry-run preview):       hyalo mv --file old.md --to sub/new.md --dry-run
  Fix broken links (preview):   hyalo links fix
  Build a snapshot index:       hyalo create-index
  Query using the index:        hyalo find --property status=draft --index
  Delete the snapshot index:    hyalo drop-index
  Save a view:                  hyalo views set todo --task todo
  List saved views:             hyalo views list
  Use a view:                   hyalo find --view todo
  Use view with overrides:      hyalo find --view todo --limit 5
  Remove a view:                hyalo views remove todo
  Generate shell completions:   hyalo completions bash";

/// Long help (shown by `--help`): command reference, cookbook, and output shapes.
///
/// Template — [`crate::cli::args::LIST_COMMANDS_PLACEHOLDER`] is substituted with
/// the rendered [`crate::list_commands::LIST_COMMANDS`] at runtime by
/// [`help_long`]. Never hand-write the list-command enumeration here (iter-192).
const HELP_LONG_TEMPLATE: &str = "COMMAND REFERENCE:
  Find (search and filter, read-only):
    hyalo find [PATTERN | -e/--regexp REGEX] [-p/--property K=V ...] [-t/--tag T ...] [--task STATUS]
               [-s/--section HEADING ...] [--title PAT] [--broken-links] [--orphan] [--dead-end]
               [-f/--file F | -g/--glob G] [--fields ...] [--sort ...] [--reverse] [--strict] [-n/--limit N]

  Read (display file body content, read-only):
    hyalo read FILE [-s/--section HEADING] [-l/--lines RANGE] [--frontmatter]
    hyalo read -f/--file F [...]                                  Flag form; FILE positional is equivalent

  Set (create or overwrite, mutates files):
    hyalo set  -p/--property K=V [-p ...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...] [--dry-run] [--validate]

  Remove (delete properties/tags, mutates files):
    hyalo remove -p/--property K|K=V [...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...] [--dry-run]

  Append (add to list properties, mutates files):
    hyalo append -p/--property K=V [-p ...] [-t/--tag T ...] [-f/--file F | -g/--glob G] [--where-property FILTER ...] [--where-tag T ...] [--dry-run] [--validate]

  Properties (subcommand group; bare `hyalo properties` = summary):
    hyalo properties summary [-g/--glob G] [-n/--limit N]         Unique property names, types, and file counts (read-only) [alias: list]
    hyalo properties rename --from OLD --to NEW [-g/--glob G]     Rename a property key across files (mutates files)

  Tags (subcommand group; bare `hyalo tags` = summary):
    hyalo tags summary [-g/--glob G] [-n/--limit N]               Unique tags with file counts (read-only) [alias: list]
    hyalo tags rename --from OLD --to NEW [-g/--glob G]           Rename a tag across files (mutates files)

  Summary (vault overview, read-only):
    hyalo summary [-g/--glob G] [-n/--recent N] [--depth N] [--limit N]

  Task (single-task operations):
    hyalo task read       -f/--file F -l/--line N           Read task at a line
    hyalo task toggle     -f/--file F -l/--line N           Toggle completion
    hyalo task set        -f/--file F -l/--line N -s/--status C

  Backlinks (reverse link lookup, read-only):
    hyalo backlinks FILE [-n/--limit N]
    hyalo backlinks -f/--file F [...]                             Flag form; FILE positional is equivalent

  Links (link operations):
    hyalo links fix [--apply] [--threshold T] [-g/--glob G] [--ignore-target S ...]   Detect and fix broken links (default: dry-run)
    hyalo links auto [--apply] [--first-only | --no-first-only] [--min-length N] [--exclude-title T ...] [--exclude-target-glob G ...] [--file F | -g/--glob G ...]   Auto-link unlinked title mentions (default: dry-run)
    Persist the exclusions in .hyalo.toml:  [links.auto] exclude_titles / exclude_target_globs / first_only (flags extend the lists; --no-first-only overrides first_only for one run)

  Mv (move/rename file or batch, updates links, mutates files):
    hyalo mv FILE DEST [--dry-run] [--on-conflict POLICY] [--allow-ambiguous]   Single file; positional DEST aliases --to
    hyalo mv -f/--file F --to DEST [...]                          Flag form; DEST is a .md path or an existing directory
    hyalo mv [-g/--glob G] [-p/--property F] [-t/--tag T] [--type T] --to DIR/ [--apply]   Batch mode (selector intersection); dry-run unless --apply
    --on-conflict POLICY: what to do when DEST already exists (see `hyalo mv --help`)
    --allow-ambiguous: rewrite bare [[stem]] links that match several files instead of skipping them

  Views (manage saved find queries; bare `hyalo views` = list):
    hyalo views list                                       List all saved views [alias: summary]
    hyalo views set <NAME> [find filters...]               Save a view (overwrites existing)
    hyalo views remove <NAME>                              Delete a view
    hyalo find --view <NAME> [additional filters...]       Use a saved view

  Lint (validate frontmatter against schemas + lint the markdown body, read-only):
    hyalo lint [-f/--file F | -g/--glob G] [--type T] [--files-from PATH] [--rule ID] [--rule-prefix PREFIX]
               [--detailed] [--strict] [--fix | --fix-rule ID] [--dry-run] [-n/--limit N]
               [--max-per-rule N] [--profile okf|madr|skills|changelog]

  Lint-rules (manage the markdown lint rule catalog; bare `hyalo lint-rules` = list):
    hyalo lint-rules list [--enabled-only | --disabled-only] [--rule-prefix PREFIX]   List rules and settings [alias: summary]
    hyalo lint-rules show <RULE_ID>                        Full details for one rule
    hyalo lint-rules set <RULE_ID> [--enabled BOOL] [--severity S]   Configure a rule (mutates .hyalo.toml)
    hyalo lint-rules remove <RULE_ID>                      Drop a rule override (mutates .hyalo.toml)

  Types (manage document-type schemas in .hyalo.toml; bare `hyalo types` = list):
    hyalo types list                                       All defined types and required fields [alias: summary]
    hyalo types show <TYPE>                                Full merged schema for one type
    hyalo types set <TYPE> [--required K,K] [...]          Create or update a type (mutates .hyalo.toml)
    hyalo types remove <TYPE>                              Delete a type entry (mutates .hyalo.toml)

  New (scaffold a file from a schema type, mutates files):
    hyalo new --type <TYPE> -f/--file PATH

  Madr (Markdown Architecture Decision Record generators):
    hyalo madr toc [--apply]                               Regenerate the ADR table of contents / status dashboard

  Changelog (Keep a Changelog 1.1.0 maintenance):
    hyalo changelog add <CATEGORY> <TEXT>                  Append an entry under `## [Unreleased]`
    hyalo changelog release <VERSION>                      Rotate `## [Unreleased]` into a dated release section

  Okf (Open Knowledge Format artifact generators):
    hyalo okf index [--apply]                              Regenerate each directory's index.md from concept frontmatter
    hyalo okf log <TEXT> [--apply]                         Prepend a dated entry to a scope-selectable log.md

  Config (print the effective configuration, read-only):
    hyalo config [--raw] [-d/--dir DIR]                    # --raw also prints the .hyalo.toml text

  Init (configuration, one-time setup):
    hyalo init [--claude] [--pi] [--profile <PROFILE>] [-d/--dir DIR]

  Deinit (remove hyalo configuration):
    hyalo deinit

  Create-index (build snapshot for faster queries):
    hyalo create-index [-o/--output PATH] [--allow-outside-vault]   # --path is an alias for --output

  Drop-index (delete snapshot index):
    hyalo drop-index [-p/--path PATH] [--allow-outside-vault]       # --output is an alias for --path

  Completions (generate shell completions):
    hyalo completions <SHELL>   # bash, zsh, fish, elvish, powershell

  Global flags (apply to all commands):
    -d/--dir <DIR>            Root directory (default: ., override via .hyalo.toml)
    --format json|text|github Output format (default: text on a terminal, json when piped;
                              override via .hyalo.toml. github is lint-only)
    --jq <FILTER>             Apply a jq expression to JSON output (incompatible with --format text)
    --count                   Print total as bare integer (shortcut for --jq '.total'; list commands only)
    --hints                   Force hints on (already the default; suppressed by --jq)
    --no-hints                Disable drill-down hints (enabled by default, override via .hyalo.toml)
    --site-prefix <PREFIX>    Override site prefix for absolute link resolution (auto-derived from --dir)
    -q/--quiet                Suppress all warnings to stderr

  Per-subcommand index flags (see each subcommand's --help):
    --index                   Use the default snapshot index (.hyalo-index in the vault dir)
    --index-file <PATH>       Use a snapshot index at an explicit path

  Default output limits:
    Capped commands ({LIMITED_COMMANDS}) return
    at most 50 results by default. Use --limit 0 for unlimited output.
    The default cap is bypassed when --jq or --count is used (pipelines
    need complete data). An explicit --limit is always honoured.
    Override the default in .hyalo.toml:  default_limit = 100

COOKBOOK:
  # Discover what metadata exists in a vault
  hyalo properties summary
  hyalo tags summary

  # Rename a property key across all files
  hyalo properties rename --from old-key --to new-key

  # Rename a tag across all files
  hyalo tags rename --from old-tag --to new-tag

  # Get a vault overview with drill-down hints
  hyalo summary --format text

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

  # Regex body search (standalone)
  hyalo find -e 'TODO|FIXME'

  # Regex body search combined with filters
  hyalo find -e 'perf(ormance)?' --tag iteration --property status=completed

  # Count matching files (bare integer output)
  hyalo find --property status=draft --count

  # Count matching files (alternative via jq)
  hyalo find --property status=draft --jq '.total'

  # Find files with open tasks
  hyalo find --task todo

  # Find files with a specific section heading (substring match: 'Tasks' matches 'Tasks [4/4]')
  hyalo find --section 'Tasks'

  # Find open tasks within a specific section
  hyalo find --section '## Sprint' --task todo

  # Find orphan files (no inbound or outbound links)
  hyalo find --orphan

  # Find dead-end files (have inbound links but no outbound)
  hyalo find --dead-end

  # Find broken [[wikilinks]] (fields=links, then filter in jq)
  hyalo find --fields links --jq '[.results[] | select(.links | map(select(.path == null)) | length > 0)]'

  # Every broken link as file:line — each link carries the source line lint reports
  hyalo find --broken-links --jq '.results[] as $f | $f.links[] | select((.path == null and (.out_of_vault | not)) or .broken_anchor) | \"\\($f.file):\\(.line) \\(.target)\"'

  # Filter by title (substring or regex)
  hyalo find --title 'meeting'
  hyalo find --title '/^Design/i'

  # Sort by modification time, newest first
  hyalo find --sort modified --reverse --limit 5

  # Exclude draft files with glob negation
  hyalo find --glob '!**/draft-*'

  # Diff-aware lint: lint only files changed on this branch
  git diff --name-only origin/main | hyalo lint --files-from -

  # Scope find to a caller-supplied file list (file path or stdin '-')
  hyalo find --files-from changed-files.txt

  # Tag all research notes in a folder
  hyalo set --tag reviewed --glob 'research/**/*.md'

  # Bulk-update a property across matching files
  hyalo set --property status=in-progress --where-property status=draft --glob '**/*.md'

  # Add a tag to files matching a tag filter
  hyalo set --tag reviewed --where-tag research --glob '**/*.md'

  # Append to a list property
  hyalo append --property aliases='My Note' --file note.md

  # Count tasks across all files
  hyalo summary --jq '.results.tasks.total'

  # List all property names as a flat list
  hyalo properties summary --jq '[.results[].name] | join(\", \")'

  # Get just file paths (no metadata)
  hyalo find --property status=draft --jq '[.results[].file]'

  # Pipe file paths for scripting (Unix)
  hyalo find --tag research --jq '.results[].file' | xargs -I{} hyalo set --property reviewed=true --file {}

  # Find all files that link to a given note (positional FILE)
  hyalo backlinks decision-log.md

  # Find all files that link to a given note (equivalent flag form)
  hyalo backlinks --file decision-log.md

  # Move a file and update all links
  hyalo mv --file backlog/old.md --to backlog/done/old.md

  # Preview a move without writing
  hyalo mv --file note.md --to archive/note.md --dry-run

  # Override site prefix for absolute link resolution
  hyalo --site-prefix docs mv --file old.md --to new.md --dry-run

  # Disable absolute-link resolution entirely
  hyalo --site-prefix '' find --fields links

  # Read file body content (positional FILE — the form every hint emits)
  hyalo read notes/todo.md

  # Read file body content (equivalent flag form)
  hyalo read --file notes/todo.md

  # Read a specific section
  hyalo read notes/todo.md --section Tasks

  # Read a line range
  hyalo read --file notes/todo.md --lines 1:10

  # Read a task's current status
  hyalo task read --file todo.md --line 5

  # Toggle a task checkbox
  hyalo task toggle --file todo.md --line 5

  # Set a custom task status (e.g. cancelled)
  hyalo task set --file todo.md --line 5 --status -

  # Fix broken links (dry-run preview)
  hyalo links fix

  # Fix broken links, skip Hugo template paths
  hyalo links fix --ignore-target '{{ ref' --apply

  # Auto-link: preview which unlinked mentions would become [[wikilinks]]
  hyalo links auto

  # Auto-link: write changes, keep only first mention per target
  hyalo links auto --first-only --apply

  # Auto-link: exclude short words and template pages
  hyalo links auto --min-length 5 --exclude-target-glob 'templates/*' --apply

  # Auto-link: link every mention for one run, ignoring [links.auto] first_only = true
  hyalo links auto --no-first-only

  # Auto-link: restrict to a single file
  hyalo links auto --file notes/todo.md --apply

  # Build a snapshot index for faster repeated queries
  hyalo create-index

  # Use the index for a find query (defaults to .hyalo-index in vault dir)
  hyalo find --property status=draft --index

  # Clean up the index after use
  hyalo drop-index

OUTPUT SHAPES (JSON, default):
  # All commands wrap output in a consistent envelope:
  {\"results\": <payload>, \"total\": N, \"hints\": [...]}
  # total: present for list commands ({LIST_COMMANDS}); omitted elsewhere
  # hints: always present (empty [] when --no-hints or --jq)
  # --jq operates on the full envelope: --jq '.results[].file', --jq '.total'
  # Conventions inside results:
  #   - a `total` inside results always counts items the command considered;
  #     a count of findings is named for what it counts (lint: violations,
  #     links auto: matched)
  #   - top-level results keys are always present (0 / false / [] / null
  #     included); only per-item records inside arrays omit optional keys
  #   - every mutating command reports dry_run and skipped_count

  # find — results is an array of file objects
  {\"results\": [{\"file\": \"notes/todo.md\", \"modified\": \"2026-03-21T...\",
   \"properties\": {\"status\": \"draft\", \"title\": \"My Note\"},
   \"tags\": [...], \"sections\": [...], \"tasks\": [...], \"links\": [...]}],
  \"total\": N, \"hints\": [...]}

  # read
  {\"results\": {\"file\": \"notes/todo.md\", \"content\": \"...body text...\"}, \"hints\": [...]}

  # set / remove / append (mutation result)
  {\"results\": {\"property\": \"status\", \"value\": \"completed\", \"modified\": [...], \"skipped\": [...],
  \"skipped_count\": N, \"scanned\": N, \"total\": N, \"dry_run\": false}, \"hints\": [...]}
  {\"results\": {\"tag\": \"reviewed\", \"modified\": [...], \"skipped\": [...],
  \"skipped_count\": N, \"scanned\": N, \"total\": N, \"dry_run\": false}, \"hints\": [...]}

  # properties summary — results is an array
  {\"results\": [{\"name\": \"status\", \"type\": \"text\", \"count\": 21}, ...], \"total\": N, \"hints\": [...]}

  # properties rename
  {\"results\": {\"from\": \"old\", \"to\": \"new\", \"modified\": [...], \"skipped_count\": N, \"conflicts\": [...], \"total\": N, \"dry_run\": false}, \"hints\": [...]}

  # tags summary — results is an array
  {\"results\": [{\"name\": \"backlog\", \"count\": 10}, ...], \"total\": 31, \"hints\": [...]}

  # tags rename
  {\"results\": {\"from\": \"old\", \"to\": \"new\", \"modified\": [...], \"skipped_count\": N, \"total\": N, \"dry_run\": false}, \"hints\": [...]}

  # lint (read-only) — violations is the run-level finding count
  {\"results\": {\"files\": [...], \"violations\": N, \"errors\": N, \"warnings\": N,
  \"files_checked\": N, \"files_with_violations\": N, \"files_ignored\": N,
  \"rules_fired\": N, \"files_truncated\": false, \"dry_run\": false}, \"total\": N, \"hints\": [...]}

  # links auto — matched is the proposal count; scanned is the denominator
  {\"results\": {\"matches\": [...], \"matched\": N, \"scanned\": N, \"applied\": false,
  \"dry_run\": true, \"files_applied\": 0, \"files_skipped\": 0, \"files_failed\": 0,
  \"apply_outcomes\": [...], \"ambiguous_titles\": [...]}, \"hints\": [...]}

  # task read / toggle / set
  {\"results\": {\"file\": \"todo.md\", \"line\": 5, \"status\": \"x\", \"text\": \"Fix bug\", \"done\": true}, \"hints\": [...]}

  # summary (compact: counts only, no file lists)
  {\"results\": {\"files\": {\"total\": 31, \"directories\": [...]}, \"properties\": [...], \"tags\": {...},
  \"status\": [{\"value\": \"draft\", \"count\": 5}], \"tasks\": {\"total\": 50, \"done\": 30},
  \"orphans\": 7, \"dead_ends\": 3, \"links\": {\"total\": 166, \"broken\": 5},
  \"schema\": {\"errors\": 2, \"warnings\": 3, \"files_with_violations\": 4},
  \"recent_files\": [...]}, \"hints\": [...]}

  # backlinks
  {\"results\": {\"file\": \"target.md\", \"backlinks\": [{\"source\": \"a.md\", \"line\": 5, \"target\": \"target\"}]},
  \"total\": 1, \"hints\": [...]}

  # mv
  {\"results\": {\"from\": \"old.md\", \"to\": \"new.md\", \"dry_run\": false,
  \"updated_files\": [{\"file\": \"a.md\", \"replacements\": [{\"line\": 5, \"old_text\": \"[[old]]\", \"new_text\": \"[[new]]\"}]}],
  \"total_files_updated\": 1, \"total_links_updated\": 1}, \"hints\": [...]}

  # create-index
  {\"results\": {\"path\": \".hyalo-index\", \"files_indexed\": 142, \"warnings\": 0}, \"hints\": [...]}

  # drop-index
  {\"results\": {\"deleted\": \".hyalo-index\"}, \"hints\": [...]}

  # errors (stderr, exit code 1 for user errors, 2 for internal)
  {\"error\": \"property not found\", \"path\": \"notes/todo.md\"}

  # --format text produces human-readable output on all commands";

/// The rendered long help: [`HELP_LONG_TEMPLATE`] with the list-command
/// placeholder resolved against [`crate::list_commands::LIST_COMMANDS`].
pub(crate) fn help_long() -> &'static str {
    static RENDERED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    RENDERED.get_or_init(|| {
        HELP_LONG_TEMPLATE
            .replace(
                crate::cli::args::LIST_COMMANDS_PLACEHOLDER,
                crate::list_commands::list_commands_phrase(),
            )
            .replace(
                crate::cli::args::LIMITED_COMMANDS_PLACEHOLDER,
                crate::list_commands::limited_commands_phrase(),
            )
    })
}

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
        return help_long().to_owned();
    }

    // Split into paragraphs separated by blank lines.  Process each paragraph
    // individually, then rejoin.
    let long = help_long();
    let paragraphs: Vec<&str> = long.split("\n\n").collect();
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
