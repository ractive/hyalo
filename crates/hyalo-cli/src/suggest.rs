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
    let top_level_names: Vec<&str> = cmd.get_subcommands().map(|s| s.get_name()).collect();

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

    // Collect sub-subcommand names from the parent.
    let sub_names: Vec<&str> = parent_cmd.get_subcommands().map(|s| s.get_name()).collect();

    if sub_names.is_empty() {
        return None;
    }

    // Scan args after the parent for `--<name>` where `<name>` matches a sub-subcommand.
    // Also skip flag values here for consistency.
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
            if let Some(name) = sub_names.iter().find(|&&n| n == flag_value) {
                found_flag_pos = Some(i);
                found_sub_name = Some(name);
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
            continue;
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
                    .subcommand(Command::new("set-status")),
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
        // hyalo task --set-status --file f --line 1 --status ? -> hyalo task set-status --file f --line 1 --status ?
        let cmd = make_cmd();
        let result = suggest_subcommand_correction(
            &args("hyalo task --set-status --file f --line 1 --status ?"),
            &cmd,
        );
        assert_eq!(
            result,
            Some("hyalo task set-status --file f --line 1 --status '?'".to_owned())
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
