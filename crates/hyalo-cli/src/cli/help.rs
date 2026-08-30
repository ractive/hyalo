/// The `-h` command list: every command grouped by intent, one line each
/// (iteration 251).
///
/// Clap's generated `Commands:` block lists 27 subcommands alphabetically with
/// their `about` text, which told an agent the names but not the *capability
/// families* behind them — the measured consequence was agents reaching for
/// bare `find` plus `grep` instead of the filters that already exist. This
/// block is rendered instead of `{subcommands}` on the top-level `-h` (see
/// [`run::run_inner`](crate::run)); `--help` keeps the full COMMAND REFERENCE.
pub(crate) const HELP_COMMANDS: &str = "COMMANDS \u{2014} read (no writes):
  find              BM25 text, regex, property, tag, task, section, title, glob,
                    link-graph filters; --sort --fields --view --count --limit
  read summary      File body (--section/--lines) | vault overview counts
  properties tags   Keys / tags with counts (bare = summary; also rename)
  backlinks lint    Inbound links | frontmatter schema + markdown rules (--fix)
  config            Effective .hyalo.toml: dir, format, hints, site_prefix
COMMANDS \u{2014} write (mutates files):
  set remove append   Properties/tags on --file/--glob + --where-property/-tag
  task mv             Checkbox read/toggle/set | move-rename, rewriting links
  links new           fix broken / auto-link mentions | scaffold from a type
  madr okf changelog  ADR toc | OKF index+log | Keep a Changelog add/release
COMMANDS \u{2014} config and scaffolds (write .hyalo.toml):
  init deinit              Create/remove .hyalo.toml (--claude --pi --profile)
  types lint-rules views   Schemas | lint rule catalog | saved find queries
  create-index drop-index  Snapshot index for faster repeated queries
  completions              Shell completion script";

/// The `-h` examples block: every line composes two or three features.
///
/// The 40 single-flag examples this replaced taught one flag per line and so
/// never showed that filters compose — the exact gap that sent agents to
/// `grep`. They still exist verbatim in `--help`'s COOKBOOK.
pub(crate) const HELP_EXAMPLES_SHORT: &str = "EXAMPLES (each composes 2-3 features):
  hyalo find --property status=planned --tag iteration --sort modified --reverse
  hyalo find 'broken links' --property 'title~=/^Iter/i' --fields tags,tasks
  hyalo find --section Tasks --task todo --filenames-only
  hyalo find --broken-links --strict --glob 'docs/**/*.md'
  hyalo set --property status=done --where-property status=draft --glob '**/*.md'
Everything else:  hyalo <cmd> -h  ==  hyalo help <cmd>  |  full reference: hyalo --help";

/// `find -h` tail: composed examples, the global-options pointer, and the
/// pointer at `find --help` (iteration 251).
///
/// `find` is the command agents live in and the one whose `-h` was worst
/// (12.3 KB). Examples here compose filters rather than demonstrating one flag
/// each, because "these compose" is precisely what the old page failed to say.
pub(crate) fn find_after_short_help(global_pointer: &str) -> String {
    format!(
        "EXAMPLES (filters compose \u{2014} AND across kinds):\n\
         \u{20}\u{20}hyalo find --property status=planned --tag iteration --sort modified --reverse\n\
         \u{20}\u{20}hyalo find 'broken links' --section Tasks --task todo --fields tasks\n\
         \u{20}\u{20}hyalo find --broken-links --strict --glob 'docs/**/*.md' --filenames-only\n\
         \n{global_pointer}\n\
         Full reference (operators, sort keys, fields):  hyalo find --help"
    )
}

/// The whole `-h` body below the usage line: the grouped command list and the
/// composed examples, with config-defaulted flags filtered out of the examples
/// exactly as [`filter_examples`] does for the long-form list.
pub(crate) fn short_help_body(hide_dir: bool, hide_format: bool) -> String {
    let examples: Vec<&str> = HELP_EXAMPLES_SHORT
        .lines()
        .filter(|line| {
            if hide_format && line.contains(" --format") {
                return false;
            }
            !(hide_dir && (line.contains("-d/--dir") || line.contains(" --dir ")))
        })
        .collect();
    format!("{HELP_COMMANDS}\n\n{}", examples.join("\n"))
}

/// iter-256 HELP-5: rewrite `hyalo [globals] help <path>...` into
/// `hyalo <path>... -h` before clap parses.
///
/// Clap's generated `help` subcommand renders the LONG page. Measured on
/// v0.22.0: `hyalo help find` is 28.7 KB where `hyalo find -h` is 3.0 KB — a
/// 9.6x tax on the phrasing agents reach for first. Rewriting the argv (rather
/// than rendering a page ourselves from a `Commands::Help` arm) is what makes
/// `hyalo help <cmd>` byte-for-byte identical to `hyalo <cmd> -h`: it inherits
/// the collapsed globals pointer, `find`'s composed examples, and the
/// `--help` footer, and it inherits clap's did-you-mean for an unknown name,
/// which the generated `help` subcommand never had (HELP-13).
///
/// Root globals are dropped along with the `help` token: none of them change
/// a help page (`hide_dir` / `hide_format` come from `.hyalo.toml`, not argv).
pub(crate) fn rewrite_help_to_short_page(args: Vec<String>, cmd: &clap::Command) -> Vec<String> {
    let Some(idx) = crate::suggest::top_level_subcommand_index(&args, cmd) else {
        return args;
    };
    if args[idx] != "help" {
        return args;
    }
    let mut out = Vec::with_capacity(args.len() + 1);
    out.push(args[0].clone());
    out.extend(args[idx + 1..].iter().cloned());
    out.push("-h".to_string());
    out
}

/// The one list of global flags (iteration 254, COH-1/COH-2/HELP-4).
///
/// Before this the same set was written down three times — the root `-h`
/// GLOBAL OPTIONS (clap-generated), the root `--help` COMMAND REFERENCE
/// "Global flags" block, and the 52 `Global: …` pointer lines — and the three
/// disagreed: the pointer omitted `--dir`, the reference block omitted
/// `--hints`. Both renderings now come from here.
///
/// Each entry is `(pointer name, reference row)`: the compact name the pointer
/// line prints, and the aligned `flag  description` row the reference block
/// prints.
const GLOBAL_FLAGS: &[(&str, &str)] = &[
    (
        "--dir",
        "-d/--dir <DIR>            Root directory (default: ., override via .hyalo.toml)",
    ),
    (
        "--format",
        "--format json|text|github Output format (default: text on a terminal, json when piped;\n                              override via .hyalo.toml. github is lint-only)",
    ),
    (
        "--jq",
        "--jq <FILTER>             Apply a jq expression to JSON output (incompatible with --format text)",
    ),
    (
        "--count",
        "--count                   Print total as bare integer (shortcut for --jq '.total'; list commands only)",
    ),
    (
        "--hints",
        "--hints                   Force hints on (already the default; suppressed by --jq)",
    ),
    (
        "--no-hints",
        "--no-hints                Disable drill-down hints (enabled by default, override via .hyalo.toml)",
    ),
    (
        "--site-prefix",
        "--site-prefix <PREFIX>    Override site prefix for absolute link resolution (auto-derived from --dir)",
    ),
    (
        "-q",
        "-q/--quiet                Suppress all warnings to stderr",
    ),
];

/// Flags the compact pointer line leaves out.
///
/// `--hints` forces on what is already on (HELP-9): naming it on 52 pages
/// costs more than it teaches, and `--no-hints` right beside it already says
/// which way the default points. It stays in the reference block.
const POINTER_OMITS: &[&str] = &["--hints"];

/// Token in [`HELP_LONG_TEMPLATE`] replaced with [`global_flags_block`].
const GLOBAL_FLAGS_PLACEHOLDER: &str = "{GLOBAL_FLAGS}";

/// Render the COMMAND REFERENCE "Global flags" block from [`GLOBAL_FLAGS`].
fn global_flags_block(hide_dir: bool, hide_format: bool) -> String {
    GLOBAL_FLAGS
        .iter()
        .filter(|(name, _)| !(hide_dir && *name == "--dir"))
        .filter(|(name, _)| !(hide_format && *name == "--format"))
        .map(|(_, row)| format!("    {row}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The single line that stands in for the global-options block on a
/// subcommand's `-h` (iteration 251).
///
/// The block is ~1.9 KB and was repeated identically on all 27 subcommands —
/// the largest single contributor to subcommand `-h` size, and pure noise once
/// you have read it once. `--dir` and `--format` are omitted when
/// `.hyalo.toml` already supplies them, matching the existing hiding rule in
/// [`filter_examples`].
pub(crate) fn global_pointer(hide_dir: bool, hide_format: bool) -> String {
    let flags: Vec<&str> = GLOBAL_FLAGS
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| !POINTER_OMITS.contains(name))
        .filter(|name| !(hide_dir && *name == "--dir"))
        .filter(|name| !(hide_format && *name == "--format"))
        .collect();
    format!("Global: {} — see `hyalo -h`", flags.join(" "))
}

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
               [-f/--file F | -g/--glob G] [--filenames-only | --filenames0] [--fields ...] [--sort ...] [--reverse] [--strict] [--language LANG] [-n/--limit N]

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
    hyalo summary [-g/--glob G] [-n/--recent N] [--depth N]

  Task (single-task operations):
    hyalo task read       -f/--file F -l/--line N           Read task at a line
    hyalo task toggle     -f/--file F -l/--line N           Toggle completion
    hyalo task set        -f/--file F -l/--line N -s/--status C
    --line accepts comma-separated lists; --section H and --all select tasks without line numbers

  Backlinks (reverse link lookup, read-only):
    hyalo backlinks FILE [-n/--limit N]
    hyalo backlinks -f/--file F [...]                             Flag form; FILE positional is equivalent

  Links (link operations):
    hyalo links fix [--apply] [--apply-fuzzy] [--min-confidence F] [--case-insensitive]
                    [--expand-short-form] [--threshold T] [-g/--glob G] [--ignore-target S ...]   Detect and fix broken links (default: dry-run)
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
    hyalo changelog add --category CAT --message TEXT     Append an entry under `## [Unreleased]`
    hyalo changelog release <VERSION>                      Rotate `## [Unreleased]` into a dated release section

  Okf (Open Knowledge Format artifact generators):
    hyalo okf index [--apply]                              Regenerate each directory's index.md from concept frontmatter
    hyalo okf log --message TEXT [TARGET] [--apply]       Prepend a dated entry to a scope-selectable log.md

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

  Help (print a command's SHORT help \u{2014} the same page as `hyalo <cmd> -h`):
    hyalo help [COMMAND]...   # `hyalo help find` == `hyalo find -h`; `hyalo find --help` for this reference

  Global flags (apply to all commands \u{2014} the same set every `Global:` pointer line names):
{GLOBAL_FLAGS}
    (Not global: --index / --index-file are per-subcommand. They appear in the
    Options block of every subcommand that can read a snapshot index.)

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

  # Every broken link target (--broken-links auto-includes the links field)
  hyalo find --broken-links --jq '[.results[] | .links[] | select(.path == null)]'

  # Every broken link as file:line — each link carries the source line lint reports
  hyalo find --broken-links --jq '.results[] as $f | $f.links[] | select((.path == null and (.out_of_vault | not)) or .broken_anchor) | \"\\($f.file):\\(.line) \\(.target)\"'

  # Filter by title (substring or regex)
  hyalo find --title 'meeting'
  hyalo find --title '/^Design/i'

  # Sort by modification time, newest first
  hyalo find --sort modified --reverse --limit 5

  # Full result shape for one file (sections, tasks, links, backlinks)
  hyalo find --file note.md --fields all

  # Biggest matches first — size (bytes) and lines are on every result item
  hyalo find --tag research --jq '[.results[] | {file, size, lines}] | sort_by(-.size)'

  # Budget a read: the first 80 lines of a large file
  hyalo read notes/todo.md --lines 1:80

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

  # find — results is an array of file objects; these seven keys are the
  # default set, and `title` is promoted OUT of `properties`. --fields adds
  # sections, tasks, links, backlinks, properties-typed; an explicit --fields
  # is an exact projection, where only `file` always survives.
  {\"results\": [{\"file\": \"notes/todo.md\", \"modified\": \"2026-03-21T...\",
   \"size\": 1093, \"lines\": 35, \"title\": \"My Note\",
   \"properties\": {\"status\": \"draft\"}, \"tags\": [...]}],
  \"total\": N, \"hints\": [...]}

  # read — size/lines are the same numbers find reports, so a large body can
  # be sliced with --lines A:B or --section instead of read whole
  {\"results\": {\"file\": \"notes/todo.md\", \"size\": 1093, \"lines\": 35,
   \"content\": \"...body text...\"}, \"hints\": [...]}

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

  # task read / toggle / set — one object for a single --line, an ARRAY of the
  # same objects for --all / --section / a multi-value --line; toggle and set
  # add \"old_status\"
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
        // Still expand {GLOBAL_FLAGS}: the block is generated even when
        // nothing is hidden, so the reference and the pointer lines cannot
        // drift apart (iter-254).
        return help_long().replace(GLOBAL_FLAGS_PLACEHOLDER, &global_flags_block(false, false));
    }

    // Split into paragraphs separated by blank lines.  Process each paragraph
    // individually, then rejoin.
    let long = help_long();
    let paragraphs: Vec<&str> = long.split("\n\n").collect();
    let mut out: Vec<String> = Vec::with_capacity(paragraphs.len());

    for para in &paragraphs {
        // The Global flags paragraph needs line-level filtering (we want to keep
        // the paragraph but drop individual flag rows).
        // iter-254: the rows themselves come from GLOBAL_FLAGS via the
        // {GLOBAL_FLAGS} placeholder, so hiding a config-defaulted flag is a
        // filter on that one list rather than a second pass over rendered text.
        if para.contains("  Global flags (apply to all commands") {
            out.push(para.replace(
                GLOBAL_FLAGS_PLACEHOLDER,
                &global_flags_block(hide_dir, hide_format),
            ));
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

// ---------------------------------------------------------------------------
// Unit tests — iteration 254 (COH-1/COH-2/HELP-4)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Both renderings must name the same flags, in the same order, minus the
    /// deliberate `POINTER_OMITS` — that equality is the whole point of the
    /// shared list, and it is exactly what drifted before.
    #[test]
    fn the_pointer_and_the_reference_block_name_the_same_flags() {
        let pointer = global_pointer(false, false);
        let block = global_flags_block(false, false);
        for (name, row) in GLOBAL_FLAGS {
            assert!(
                block.contains(row),
                "the reference block is missing {name}:\n{block}"
            );
            if POINTER_OMITS.contains(name) {
                assert!(
                    !pointer.contains(name),
                    "{name} is omitted from the pointer but appeared: {pointer}"
                );
            } else {
                assert!(pointer.contains(name), "{name} missing from {pointer}");
            }
        }
    }

    #[test]
    fn hiding_a_config_defaulted_flag_hides_it_in_both_renderings() {
        for (hide_dir, hide_format) in [(true, false), (false, true), (true, true)] {
            let pointer = global_pointer(hide_dir, hide_format);
            let block = global_flags_block(hide_dir, hide_format);
            assert_eq!(pointer.contains("--dir"), !hide_dir, "{pointer}");
            assert_eq!(block.contains("-d/--dir"), !hide_dir, "{block}");
            assert_eq!(pointer.contains("--format"), !hide_format, "{pointer}");
            assert_eq!(block.contains("--format json"), !hide_format, "{block}");
        }
    }

    /// `--index`/`--index-file` are per-subcommand, so the reference must not
    /// list them among the globals — the contradiction COH-2 reported.
    #[test]
    fn the_globals_block_claims_no_index_flags() {
        let block = global_flags_block(false, false);
        assert!(!block.contains("--index"), "{block}");
    }

    #[test]
    fn the_long_help_expands_the_globals_placeholder() {
        for (hide_dir, hide_format) in [(false, false), (true, true)] {
            let rendered = filter_long_help(hide_dir, hide_format);
            assert!(
                !rendered.contains(GLOBAL_FLAGS_PLACEHOLDER),
                "the placeholder leaked into --help (hide_dir={hide_dir}, hide_format={hide_format})"
            );
            assert!(rendered.contains("Global flags (apply to all commands"));
        }
    }
}
