use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use hyalo::commands::{links as link_commands, properties, tags as tag_commands};
use hyalo::output::{CommandOutcome, Format};

#[derive(Parser)]
#[command(
    name = "hyalo",
    version,
    about = "CLI for managing Obsidian-compatible markdown files"
)]
struct Cli {
    /// Base directory (default: current directory)
    #[arg(long, global = true, default_value = ".")]
    dir: PathBuf,

    /// Output format: json or text
    #[arg(long, global = true, default_value = "json")]
    format: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List properties of files
    Properties {
        /// Glob pattern (relative to --dir)
        #[arg(long)]
        glob: Option<String>,
    },
    /// Read, set, or remove a single property
    Property {
        #[command(subcommand)]
        action: PropertyAction,
    },
    /// List outgoing links from a file
    Links {
        /// File path (relative to --dir)
        #[arg(long)]
        file: String,
        /// Only show links that don't resolve to any file
        #[arg(long, conflicts_with = "resolved")]
        unresolved: bool,
        /// Only show links that resolve to a file
        #[arg(long, conflicts_with = "unresolved")]
        resolved: bool,
    },
    /// List all unique tags with occurrence counts across matched files
    Tags {
        /// Specific file (relative to --dir)
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern (relative to --dir)
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Inspect or modify tags
    Tag {
        #[command(subcommand)]
        action: TagAction,
    },
}

#[derive(Subcommand)]
enum TagAction {
    /// Find files containing a specific tag (supports nested matching)
    Find {
        /// Tag name to search for
        #[arg(long)]
        name: String,
        /// Specific file (relative to --dir)
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern (relative to --dir)
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Add a tag to file(s) frontmatter
    Add {
        /// Tag name to add
        #[arg(long)]
        name: String,
        /// Specific file (relative to --dir)
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern (relative to --dir)
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
    /// Remove a tag from file(s) frontmatter
    Remove {
        /// Tag name to remove
        #[arg(long)]
        name: String,
        /// Specific file (relative to --dir)
        #[arg(long, conflicts_with = "glob")]
        file: Option<String>,
        /// Glob pattern (relative to --dir)
        #[arg(long, conflicts_with = "file")]
        glob: Option<String>,
    },
}

#[derive(Subcommand)]
enum PropertyAction {
    /// Read a property value
    Read {
        /// Property name
        #[arg(long)]
        name: String,
        /// File path (relative to --dir)
        #[arg(long)]
        file: String,
    },
    /// Set a property value
    Set {
        /// Property name
        #[arg(long)]
        name: String,
        /// Property value
        #[arg(long)]
        value: String,
        /// Force type: text, number, checkbox, date, datetime, list
        #[arg(long = "type")]
        prop_type: Option<String>,
        /// File path (relative to --dir)
        #[arg(long)]
        file: String,
    },
    /// Remove a property
    Remove {
        /// Property name
        #[arg(long)]
        name: String,
        /// File path (relative to --dir)
        #[arg(long)]
        file: String,
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
