use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use hyalo::commands::properties;
use hyalo::output::Format;

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
        /// File path or glob pattern (relative to --dir)
        #[arg(long)]
        path: Option<String>,
    },
    /// Read, set, or remove a single property
    Property {
        #[command(subcommand)]
        action: PropertyAction,
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
        path: String,
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
        path: String,
    },
    /// Remove a property
    Remove {
        /// Property name
        #[arg(long)]
        name: String,
        /// File path (relative to --dir)
        #[arg(long)]
        path: String,
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
        Commands::Properties { ref path } => properties::properties(dir, path.as_deref(), format),
        Commands::Property { action } => match action {
            PropertyAction::Read { ref name, ref path } => {
                properties::property_read(dir, name, path, format)
            }
            PropertyAction::Set {
                ref name,
                ref value,
                ref prop_type,
                ref path,
            } => properties::property_set(dir, name, value, prop_type.as_deref(), path, format),
            PropertyAction::Remove { ref name, ref path } => {
                properties::property_remove(dir, name, path, format)
            }
        },
    };

    match result {
        Ok((output, exit_code)) => {
            if exit_code == 0 {
                println!("{output}");
            } else {
                eprintln!("{output}");
            }
            process::exit(exit_code);
        }
        Err(e) => {
            let msg = hyalo::output::format_error(
                format,
                &e.to_string(),
                None,
                None,
                e.chain().nth(1).map(|s| s.to_string()).as_deref(),
            );
            eprintln!("{msg}");
            process::exit(2);
        }
    }
}
