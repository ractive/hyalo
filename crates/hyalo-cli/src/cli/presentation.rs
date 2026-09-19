//! Command-specific presentation of shared flags. Parsing and runtime validation
//! remain unchanged; help, completion, and invocation descriptions share this tree.

use clap::{Command, ValueEnum, builder::TypedValueParser};

use crate::output::Format;

/// Materialize inherited arguments before customizing them, leaving Clap's
/// parser unbuilt. Mutating a built tree invalidates its argument lookup tables.
pub(crate) fn apply(command: Command) -> Command {
    customize(command, &[], &[])
}

fn customize(mut command: Command, parent: &[String], globals: &[clap::Arg]) -> Command {
    for arg in globals {
        if !command
            .get_arguments()
            .any(|local| local.get_id() == arg.get_id())
        {
            command = command.arg(arg.clone());
        }
    }
    let mut path = parent.to_vec();
    if command.get_name() != "hyalo" {
        path.push(command.get_name().to_owned());
    }
    // Propagate before hiding: an unsupported group default must not hide a
    // supported flag on an action, e.g. `views --index-file` vs `views run`.
    let globals: Vec<_> = command
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .cloned()
        .collect();
    command = command.mut_subcommands(|child| customize(child, &path, &globals));
    if path.is_empty() {
        return command;
    }
    let name = path.join(" ");
    let count = crate::list_commands::LIST_COMMANDS.contains(&name.as_str())
        || matches!(
            name.as_str(),
            "properties" | "tags" | "types" | "views" | "lint-rules" | "views run"
        );
    let index = command.get_arguments().any(|arg| arg.get_id() == "index")
        || matches!(
            path[0].as_str(),
            "create-index" | "drop-index" | "okf" | "madr" | "changelog" | "links" | "task"
        );
    if !count {
        command = hide(command, "count");
    }
    if !index {
        command = hide(command, "index_file");
    }
    if (!index && path[0] != "config") || path[0] == "drop-index" {
        command = hide(command, "site_prefix");
    }
    if matches!(path[0].as_str(), "init" | "deinit" | "completions" | "help") {
        command = hide(hide(command, "hints"), "no_hints");
    }
    if path[0] == "completions" {
        command = hide(hide(command, "jq"), "dir");
    }
    let formats = if path[0] == "lint" {
        vec![Format::Json, Format::Text, Format::Github]
    } else if path[0] == "completions" {
        vec![Format::Text]
    } else {
        vec![Format::Json, Format::Text]
    };
    command = command.mut_arg("format", |arg| {
        let arg = if path[0] == "completions" {
            arg.help("Output format (text only)")
                .long_help("Shell completions support text output only.")
        } else {
            arg
        };
        arg.value_parser(VisibleFormats(formats))
    });
    if matches!(name.as_str(), "read" | "backlinks" | "task read") {
        command = hide(command, "glob")
            .mut_arg("file", |arg| arg
                .help("Target one file (excludes --files-from)")
                .long_help("Target one file relative to --dir. Mutually exclusive with --files-from and positional FILE."))
            .mut_arg("files_from", |arg| arg
                .help("Read paths from PATH ('-' = stdin); must resolve to one file")
                .long_help("Read file paths from PATH (one per line); use '-' to read from stdin. \
                    Input is deduplicated and must resolve to exactly one Markdown file. \
                    Non-.md paths, missing files, and paths outside the vault are skipped. \
                    Mutually exclusive with --file and positional FILE."));
    }
    if name == "views set" {
        command = hide(command, "files_from")
            .mut_arg("file", |arg| arg
                .help("Save target file(s), repeatable (excludes --glob)")
                .long_help("Save target file(s) with the view. Repeatable; mutually exclusive with --glob."))
            .mut_arg("glob", |arg| arg
                .long_help("Save glob patterns relative to --dir. Repeatable; prefix '!' to negate. Mutually exclusive with --file."));
    }
    command
}

fn hide(command: Command, id: &str) -> Command {
    command.mut_arg(id, |arg| arg.hide(true))
}

/// Limit advertised values without changing parsing: unsupported formats still
/// reach the existing runtime diagnostics (and their established exit codes).
#[derive(Clone)]
struct VisibleFormats(Vec<Format>);

impl TypedValueParser for VisibleFormats {
    type Value = Format;

    fn parse_ref(
        &self,
        command: &Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Format, clap::Error> {
        clap::builder::EnumValueParser::<Format>::new().parse_ref(command, arg, value)
    }

    fn possible_values(
        &self,
    ) -> Option<Box<dyn Iterator<Item = clap::builder::PossibleValue> + '_>> {
        Some(Box::new(
            self.0.iter().filter_map(Format::to_possible_value),
        ))
    }
}
