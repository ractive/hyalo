use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use hyalo::commands::{links as link_commands, properties, tags as tag_commands};
use hyalo::output::{CommandOutcome, Format};

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
        COMMANDS: Use 'properties'/'tags' to list across files. Use 'property'/'tag' for single-item mutations. \
        Use 'links' to inspect wikilink targets.",
    after_help = "EXAMPLES:\n  \
        List all properties:        hyalo properties --glob '**/*.md'\n  \
        Set a property:             hyalo property set --name status --value done --file notes/todo.md\n  \
        List tags with counts:      hyalo tags\n  \
        Find files by tag:          hyalo tag find --name project/backend\n  \
        Find broken wikilinks:      hyalo links --file index.md --unresolved"
)]
struct Cli {
    /// Root directory for resolving all --file and --glob paths. Defaults to current directory
    #[arg(long, global = true, default_value = ".")]
    dir: PathBuf,

    /// Output format: "json" (structured, default) or "text" (human-readable). Applies to both stdout and stderr
    #[arg(long, global = true, default_value = "json")]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all frontmatter properties across matched files (read-only, no side effects)
    #[command(
        long_about = "List all frontmatter properties across matched files.\n\n\
            INPUT: Reads .md files matching --glob (or all .md files under --dir if omitted).\n\
            OUTPUT: For each file, emits every YAML frontmatter key-value pair with its inferred type.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to discover what properties exist, audit frontmatter, or find files with specific metadata."
    )]
    Properties {
        /// Glob pattern to select files (e.g. '**/*.md', 'notes/*.md'). Omit to scan all .md files
        #[arg(long)]
        glob: Option<String>,
    },
    /// Read, set, or remove a single frontmatter property on one file
    #[command(
        long_about = "Read, set, or remove a single frontmatter property on one file.\n\n\
        Subcommands: read, set, remove. Each operates on exactly one property in one file.\n\
        USE 'read' to get a value, 'set' to create/overwrite, 'remove' to delete.\n\
        SIDE EFFECTS: 'set' and 'remove' modify the file on disk. 'read' is read-only."
    )]
    Property {
        #[command(subcommand)]
        action: PropertyAction,
    },
    /// List outgoing [[wikilinks]] from a file and their resolution status (read-only)
    #[command(
        long_about = "List outgoing [[wikilinks]] from a file and their resolution status.\n\n\
            INPUT: A single markdown file (--file).\n\
            OUTPUT: Each [[wikilink]] found in the file body, with a boolean indicating whether \
            the link target resolves to an existing .md file under --dir.\n\
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
    /// List all unique tags with per-tag file counts across matched files (read-only)
    #[command(
        long_about = "List all unique tags with per-tag file counts across matched files.\n\n\
            INPUT: Reads the 'tags' field from YAML frontmatter in matched files.\n\
            OUTPUT: Each unique tag and how many files contain it. Tags are compared case-insensitively.\n\
            SCOPE: Scans all .md files under --dir unless narrowed with --file or --glob.\n\
            SIDE EFFECTS: None (read-only).\n\
            USE WHEN: You need to see which tags exist, find popular/orphan tags, or audit tag taxonomy."
    )]
    Tags {
        /// Scan only this file instead of all files
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern to filter which files to scan (e.g. 'notes/**/*.md')
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Find, add, or remove tags in file frontmatter
    #[command(long_about = "Find, add, or remove tags in file frontmatter.\n\n\
        Subcommands: find, add, remove.\n\
        NESTED TAG MATCHING: Tag names can be hierarchical (e.g. 'project/backend'). \
        Searching for a parent tag like 'project' matches all children ('project/backend', 'project/frontend').\n\
        SIDE EFFECTS: 'add' and 'remove' modify files on disk. 'find' is read-only.")]
    Tag {
        #[command(subcommand)]
        action: TagAction,
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
              - Boolean properties: parse --value as 'true' or 'false'.\n\
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
}

fn main() {
    let cli = Cli::parse();

    let format = match Format::from_str_opt(&cli.format) {
        Some(f) => f,
        None => {
            eprintln!(
                "Error: invalid format '{}', expected 'json' or 'text'",
                cli.format
            );
            process::exit(2);
        }
    };

    let dir = &cli.dir;

    let result = match cli.command {
        Commands::Properties { ref glob } => properties::properties(dir, glob.as_deref(), format),
        Commands::Property { action } => match action {
            PropertyAction::Read { ref name, ref file } => {
                properties::property_read(dir, name, file, format)
            }
            PropertyAction::Set {
                ref name,
                ref value,
                ref prop_type,
                ref file,
            } => properties::property_set(dir, name, value, prop_type.as_deref(), file, format),
            PropertyAction::Remove { ref name, ref file } => {
                properties::property_remove(dir, name, file, format)
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
                format,
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
            link_commands::links(dir, file, filter, format)
        }
        Commands::Tags { ref file, ref glob } => {
            tag_commands::tags_list(dir, file.as_deref(), glob.as_deref(), format)
        }
        Commands::Tag { action } => match action {
            TagAction::Find {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_find(dir, name, file.as_deref(), glob.as_deref(), format),
            TagAction::Add {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_add(dir, name, file.as_deref(), glob.as_deref(), format),
            TagAction::Remove {
                ref name,
                ref file,
                ref glob,
            } => tag_commands::tag_remove(dir, name, file.as_deref(), glob.as_deref(), format),
        },
    };

    match result {
        Ok(CommandOutcome::Success(output)) => {
            println!("{output}");
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
