/// Returns `true` when a clap `UnknownArgument` error was triggered by `flag`.
///
/// Inspects `ContextKind::InvalidArg` in the error's context chain.
/// The `flag` parameter must include the leading `--` (e.g. `"--filter"`).
pub fn unknown_arg_is(err: &clap::Error, flag: &str) -> bool {
    use clap::error::{ContextKind, ContextValue};
    err.context().any(|(kind, value)| {
        kind == ContextKind::InvalidArg && matches!(value, ContextValue::String(s) if s == flag)
    })
}

/// Identify the first top-level subcommand token in `args`, accounting for
/// global value-taking flags (e.g. `--dir <path>`).
///
/// Returns `None` if no known top-level subcommand appears before `--` / end.
///
/// This is used by callers that need to show subcommand-specific error hints
/// (e.g. `hyalo append --tag …`) without being fooled by argv positions that
/// merely *contain* the subcommand name as a value (`hyalo read append …` or
/// `hyalo --dir append …`).
pub fn top_level_subcommand<'a>(args: &'a [String], cmd: &clap::Command) -> Option<&'a str> {
    top_level_subcommand_index(args, cmd).map(|i| args[i].as_str())
}

/// Index into `args` of the first token that names a top-level subcommand.
///
/// Same scan as [`top_level_subcommand`], but returns the position rather than
/// the name — iter-256 needs it to rewrite `hyalo help <path>` into
/// `hyalo <path> -h` before clap parses, and the position is what says where
/// the subcommand path starts.
pub fn top_level_subcommand_index(args: &[String], cmd: &clap::Command) -> Option<usize> {
    // Value-taking root flags whose next token must be skipped (e.g. `--dir`).
    let value_flags: Vec<&str> = cmd
        .get_arguments()
        .filter(|a| a.get_num_args().is_some_and(|r| r.min_values() > 0))
        .filter_map(|a| a.get_long())
        .collect();

    let top_level_names: Vec<&str> = cmd.get_subcommands().map(clap::Command::get_name).collect();

    let mut skip_next = false;
    for (idx, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            break;
        }
        if let Some(flag) = arg.strip_prefix("--") {
            if value_flags.contains(&flag) {
                skip_next = true;
            }
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        if top_level_names.contains(&arg.as_str()) {
            return Some(idx);
        }
    }
    None
}

/// Extract the long-flag name from a clap `UnknownArgument` error.
///
/// Returns the flag without leading dashes and without any `=value`
/// suffix (e.g. `hyalo find --status=planned` → `Some("status")`).
/// Short flags (single-dash) and `--` return `None` — the property hint
/// only makes sense for long flags that read like property names.
pub fn unknown_long_flag_name(err: &clap::Error) -> Option<String> {
    use clap::error::{ContextKind, ContextValue};
    err.context().find_map(|(kind, value)| {
        if kind != ContextKind::InvalidArg {
            return None;
        }
        let ContextValue::String(s) = value else {
            return None;
        };
        let name = s.strip_prefix("--")?;
        if name.is_empty() || name.starts_with('-') {
            return None;
        }
        Some(name.split('=').next().unwrap_or(name).to_owned())
    })
}

/// Whether `name` is a property declared in the effective schema:
/// any type's `properties`, `required`, or `defaults` keys — or the
/// `[schema.default]` type's. This powers the `--status` →
/// `--property status=…` unknown-flag hint: only flags that name a real
/// property get the suggestion; everything else keeps clap's normal error.
pub fn is_schema_property(schema: &hyalo_core::schema::SchemaConfig, name: &str) -> bool {
    let check = |ts: &hyalo_core::schema::TypeSchema| {
        ts.properties.contains_key(name)
            || ts.required.iter().any(|r| r == name)
            || ts.defaults.contains_key(name)
    };
    check(&schema.default) || schema.types.values().any(check)
}

/// The clap `Command` for the (sub)command `args` actually invoked, walking as
/// deep as the tokens name real subcommands (`types set` → the `set` action).
///
/// Returns the root command when no subcommand token is present.
pub fn invoked_command(args: &[String], root: &clap::Command) -> clap::Command {
    let Some(start) = top_level_subcommand_index(args, root) else {
        return root.clone();
    };
    let mut cmd = root.clone();
    for token in args.iter().skip(start) {
        if token == "--" {
            break;
        }
        if token.starts_with('-') {
            continue;
        }
        let Some(sub) = cmd
            .get_subcommands()
            .find(|s| s.get_name() == token || s.get_all_aliases().any(|a| a == token))
            .cloned()
        else {
            break;
        };
        cmd = sub;
    }
    cmd
}

/// Whether the invoked (sub)command declares the long flag `long` (without
/// dashes), counting clap's global flags inherited from the root.
///
/// iter-274 (UX-8): the unknown-flag tips used to assume every command has the
/// flag they recommend. `hyalo changelog add --type entry` was answered with
/// "did you mean '--property type=entry'?" — `changelog add` has no
/// `--property`, so the corrected command clap suggested fails too. A tip that
/// cannot be pasted back is worse than clap's own error, which at least names
/// the real neighbour (`--category`).
pub fn command_has_long_flag(args: &[String], root: &clap::Command, long: &str) -> bool {
    let cmd = invoked_command(args, root);
    if cmd.get_arguments().any(|a| a.get_long() == Some(long)) {
        return true;
    }
    // Globals are declared once on the root and inherited by every subcommand;
    // `get_arguments` on the child does not list them.
    root.get_arguments()
        .any(|a| a.is_global_set() && a.get_long() == Some(long))
}

/// The space-joined subcommand path `args` invoked (`"changelog add"`), or an
/// empty string when no subcommand token is present.
pub fn invoked_command_path(args: &[String], root: &clap::Command) -> String {
    let Some(start) = top_level_subcommand_index(args, root) else {
        return String::new();
    };
    let mut cmd = root.clone();
    let mut path: Vec<String> = Vec::new();
    for token in args.iter().skip(start) {
        if token == "--" {
            break;
        }
        if token.starts_with('-') {
            continue;
        }
        let Some(sub) = cmd
            .get_subcommands()
            .find(|s| s.get_name() == token || s.get_all_aliases().any(|a| a == token))
            .cloned()
        else {
            break;
        };
        path.push(sub.get_name().to_owned());
        cmd = sub;
    }
    path.join(" ")
}

/// The long flags `cmd` declares itself, `--`-prefixed and comma-joined, with
/// `--help` and `--version` dropped (every command has them, so naming them
/// tells the reader nothing). Globals are excluded for the same reason.
pub fn long_flag_list(cmd: &clap::Command) -> String {
    cmd.get_arguments()
        .filter(|a| !a.is_global_set())
        .filter_map(clap::Arg::get_long)
        .filter(|l| *l != "help" && *l != "version")
        .map(|l| format!("--{l}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Given the raw CLI args and the clap Command tree, detect when an unknown
/// `--flag` matches a known subcommand name and return a corrected command suggestion.
///
/// Returns `Some(suggestion_string)` if a correction was found, `None` otherwise.
pub fn suggest_subcommand_correction(args: &[String], cmd: &clap::Command) -> Option<String> {
    // args[0] is the binary name; find the first positional that matches a top-level subcommand.
    // Ensure args is non-empty (args[0] is the binary name).
    args.first()?;

    // Build a set of long flags that consume the next token as a value (e.g. --dir, --format).
    // Without this, `--dir task` would cause `task` to be misidentified as a parent subcommand.
    let value_flags: Vec<&str> = cmd
        .get_arguments()
        .filter(|a| a.get_num_args().is_some_and(|r| r.min_values() > 0))
        .filter_map(|a| a.get_long())
        .collect();

    // Walk args (skipping bin) to find the top-level subcommand and its position.
    // We stop at `--` (end-of-flags marker) and skip tokens that are values of
    // value-taking flags (e.g. the `foo` in `--dir foo`).
    let top_level_names: Vec<&str> = cmd.get_subcommands().map(clap::Command::get_name).collect();

    let mut parent_name: Option<&str> = None;
    let mut parent_pos: Option<usize> = None; // index into args (0-based, including bin)
    let mut skip_next = false;

    for (i, arg) in args.iter().enumerate().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            break;
        }
        if let Some(flag) = arg.strip_prefix("--") {
            if value_flags.contains(&flag) {
                skip_next = true;
            }
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        if let Some(name) = top_level_names.iter().find(|&&n| n == arg.as_str()) {
            parent_name = Some(name);
            parent_pos = Some(i);
            break;
        }
    }

    let parent_name = parent_name?;
    let parent_pos = parent_pos?;

    // Find the subcommand Command node for the parent.
    let parent_cmd = cmd
        .get_subcommands()
        .find(|s| s.get_name() == parent_name)?;

    parent_cmd.get_subcommands().next()?;

    // Scan args after the parent for `--<name>` where `<name>` matches a sub-subcommand
    // name or alias. Also skip flag values here for consistency.
    let mut found_flag_pos: Option<usize> = None;
    let mut found_sub_name: Option<&str> = None;
    skip_next = false;

    for (i, arg) in args.iter().enumerate().skip(parent_pos + 1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--" {
            break;
        }
        if let Some(flag_value) = arg.strip_prefix("--") {
            // Match against the canonical name first, then any aliases (including hidden).
            let matched = parent_cmd.get_subcommands().find(|sub| {
                sub.get_name() == flag_value
                    || sub.get_all_aliases().any(|alias| alias == flag_value)
            });
            if let Some(sub) = matched {
                found_flag_pos = Some(i);
                found_sub_name = Some(sub.get_name());
                break;
            }
            // Check if this flag takes a value (look in parent_cmd's args too)
            let parent_value_flags: Vec<&str> = parent_cmd
                .get_arguments()
                .filter(|a| a.get_num_args().is_some_and(|r| r.min_values() > 0))
                .filter_map(|a| a.get_long())
                .collect();
            if parent_value_flags.contains(&flag_value) {
                skip_next = true;
            }
        }
    }

    let flag_pos = found_flag_pos?;
    let sub_name = found_sub_name?;

    // Reconstruct the corrected command:
    // - Remove the `--<name>` flag from its position
    // - Insert `<name>` immediately after the parent subcommand
    // - Shell-quote args that contain spaces or special characters
    let mut corrected: Vec<String> = Vec::with_capacity(args.len());

    for (i, arg) in args.iter().enumerate() {
        if i == flag_pos {
            // Skip the misplaced --<sub> flag
            continue;
        }
        corrected.push(crate::hints::shell_quote(arg));
        if i == parent_pos {
            // Insert the sub-subcommand name right after the parent
            corrected.push(sub_name.to_owned());
        }
    }

    Some(corrected.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    // We build a minimal command tree that mirrors hyalo's real structure.
    // The Cli struct lives in the binary (main.rs), not in the lib, so we
    // construct an equivalent Command inline to keep the unit tests self-contained.

    fn make_cmd() -> clap::Command {
        use clap::{Arg, Command};

        Command::new("hyalo")
            .arg(Arg::new("dir").short('d').long("dir").num_args(1))
            .arg(Arg::new("format").long("format").num_args(1))
            .subcommand(
                Command::new("task")
                    .arg(Arg::new("file").short('f').long("file").num_args(1))
                    .arg(Arg::new("line").short('l').long("line").num_args(1))
                    .subcommand(Command::new("read"))
                    .subcommand(Command::new("toggle"))
                    .subcommand(Command::new("set").alias("set-status")),
            )
            .subcommand(
                Command::new("properties")
                    .subcommand(Command::new("summary"))
                    .subcommand(Command::new("rename")),
            )
            .subcommand(
                Command::new("tags")
                    .subcommand(Command::new("summary"))
                    .subcommand(Command::new("rename")),
            )
            .subcommand(Command::new("find").arg(Arg::new("property").short('p').long("property")))
    }

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn toggle_before_file_flag() {
        // hyalo task --toggle --file f --line 1 -> hyalo task toggle --file f --line 1
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --toggle --file f --line 1"), &cmd);
        assert_eq!(
            result,
            Some("hyalo task toggle --file f --line 1".to_owned())
        );
    }

    #[test]
    fn toggle_after_other_flags() {
        // hyalo task --file f --line 1 --toggle -> hyalo task toggle --file f --line 1
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --file f --line 1 --toggle"), &cmd);
        assert_eq!(
            result,
            Some("hyalo task toggle --file f --line 1".to_owned())
        );
    }

    #[test]
    fn toggle_between_flags() {
        // hyalo task --file f --toggle --line 1 -> hyalo task toggle --file f --line 1
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --file f --toggle --line 1"), &cmd);
        assert_eq!(
            result,
            Some("hyalo task toggle --file f --line 1".to_owned())
        );
    }

    #[test]
    fn set_status_hyphenated() {
        // hyalo task --set-status --file f --line 1 --status ? -> hyalo task set --file f --line 1 --status ?
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(
            &args("hyalo task --set-status --file f --line 1 --status ?"),
            &cmd,
        );
        assert_eq!(
            result,
            Some("hyalo task set --file f --line 1 --status '?'".to_owned())
        );
    }

    #[test]
    fn properties_rename() {
        // hyalo properties --rename --from a --to b -> hyalo properties rename --from a --to b
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo properties --rename --from a --to b"), &cmd);
        assert_eq!(
            result,
            Some("hyalo properties rename --from a --to b".to_owned())
        );
    }

    #[test]
    fn properties_summary() {
        // hyalo properties --summary -> hyalo properties summary
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(&args("hyalo properties --summary"), &cmd);
        assert_eq!(result, Some("hyalo properties summary".to_owned()));
    }

    #[test]
    fn tags_rename() {
        // hyalo tags --rename --from a --to b -> hyalo tags rename --from a --to b
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo tags --rename --from a --to b"), &cmd);
        assert_eq!(result, Some("hyalo tags rename --from a --to b".to_owned()));
    }

    #[test]
    fn tags_summary() {
        // hyalo tags --summary -> hyalo tags summary
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(&args("hyalo tags --summary"), &cmd);
        assert_eq!(result, Some("hyalo tags summary".to_owned()));
    }

    #[test]
    fn task_read() {
        // hyalo task --read --file f --line 1 -> hyalo task read --file f --line 1
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --read --file f --line 1"), &cmd);
        assert_eq!(result, Some("hyalo task read --file f --line 1".to_owned()));
    }

    #[test]
    fn unknown_flag_no_suggestion() {
        // hyalo task --verbose --file f toggle -> None (--verbose doesn't match any sub-subcommand)
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --verbose --file f toggle"), &cmd);
        assert_eq!(result, None);
    }

    #[test]
    fn find_has_no_subcommands() {
        // hyalo find --property status=done -> None (find has no subcommands)
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo find --property status=done"), &cmd);
        assert_eq!(result, None);
    }

    #[test]
    fn short_flags_preserved() {
        // hyalo task --toggle -f foo.md -l 28 -> hyalo task toggle -f foo.md -l 28
        let cmd = make_cmd();
        let result =
            suggest_subcommand_correction(&args("hyalo task --toggle -f foo.md -l 28"), &cmd);
        assert_eq!(result, Some("hyalo task toggle -f foo.md -l 28".to_owned()));
    }

    #[test]
    fn dir_value_not_confused_with_subcommand() {
        // hyalo --dir task --toggle --file f --line 1
        // Here "task" is the value of --dir, not a subcommand.
        // No parent subcommand is found, so no suggestion.
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(
            &args("hyalo --dir task --toggle --file f --line 1"),
            &cmd,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn dir_value_with_real_subcommand_after() {
        // hyalo --dir mydir task --toggle --file f --line 1
        // "mydir" is --dir's value, "task" is the real subcommand
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(
            &args("hyalo --dir mydir task --toggle --file f --line 1"),
            &cmd,
        );
        assert_eq!(
            result,
            Some("hyalo --dir mydir task toggle --file f --line 1".to_owned())
        );
    }

    #[test]
    fn no_parent_subcommand_at_all() {
        // hyalo --toggle (no parent subcommand recognized)
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(&args("hyalo --toggle"), &cmd);
        assert_eq!(result, None);
    }

    #[test]
    fn format_value_not_confused() {
        // hyalo --format json task --toggle --file f --line 1
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(
            &args("hyalo --format json task --toggle --file f --line 1"),
            &cmd,
        );
        assert_eq!(
            result,
            Some("hyalo --format json task toggle --file f --line 1".to_owned())
        );
    }

    #[test]
    fn args_with_spaces_are_quoted() {
        // File path with spaces should be shell-quoted in the suggestion
        let cmd = make_cmd();
        let input = vec![
            "hyalo".to_owned(),
            "task".to_owned(),
            "--toggle".to_owned(),
            "--file".to_owned(),
            "My Notes.md".to_owned(),
            "--line".to_owned(),
            "1".to_owned(),
        ];
        let result = suggest_subcommand_correction(&input, &cmd);
        assert_eq!(
            result,
            Some("hyalo task toggle --file 'My Notes.md' --line 1".to_owned())
        );
    }
}
