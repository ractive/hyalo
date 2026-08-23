//! Does this invocation write anything? (iter-201)
//!
//! Two features need the same answer and must never disagree:
//!
//! - **Config trust (M-2)** — a `.hyalo.toml` that failed to parse leaves the
//!   run on built-in defaults: a different vault root, no schema, no
//!   `[lint] ignore`. Reads may proceed on that (loudly); a write must not.
//! - **Hint marking (M-7)** — the `-> hyalo …` drill-down list mixed a
//!   `views set …` command, which *writes `.hyalo.toml`*, in among read-only
//!   suggestions with nothing to distinguish it.
//!
//! [`Commands::writes`] answers it from parsed arguments (exact, used by the
//! config gate). [`command_line_writes`] answers it from a command *string*
//! (used to mark hints, which are built as text). The two are kept in sync by
//! `writes_agrees_with_command_line_classifier` in the tests below, which runs
//! a corpus of command lines through both.

use crate::cli::args::{
    ChangelogAction, Commands, LinksAction, LintRulesAction, MadrAction, OkfAction,
    PropertiesAction, TagsAction, TaskAction, TypesAction, ViewsAction,
};

impl Commands {
    /// `true` when this invocation will modify the vault, `.hyalo.toml`, or a
    /// snapshot index on disk.
    ///
    /// `--dry-run` makes an otherwise-writing command return `false`: it
    /// reports what *would* change and touches nothing. Commands that only
    /// write behind `--apply` (`links fix`, `links auto`, the `okf`/`madr`/
    /// `changelog` generators) likewise return `false` in their preview form.
    ///
    /// `init` and `deinit` are deliberately **not** writers here even though
    /// they write: they are how a broken `.hyalo.toml` gets repaired, so the
    /// config gate must never block them. They also short-circuit before the
    /// gate runs.
    pub(crate) fn writes(&self) -> bool {
        match self {
            // Read-only.
            Self::Find { .. }
            | Self::Read { .. }
            | Self::Summary { .. }
            | Self::Backlinks { .. }
            | Self::Config { .. }
            | Self::Completion { .. }
            | Self::Init { .. }
            | Self::Deinit => false,

            // `create-index`/`drop-index` write a snapshot index file, `new`
            // scaffolds a markdown file from a schema type. None of the three
            // has a preview mode.
            Self::CreateIndex { .. } | Self::DropIndex { .. } | Self::New { .. } => true,

            Self::Set { dry_run, .. }
            | Self::Remove { dry_run, .. }
            | Self::Append { dry_run, .. }
            | Self::Mv { dry_run, .. } => !dry_run,

            Self::Lint {
                fix,
                fix_rule,
                dry_run,
                ..
            } => (*fix || !fix_rule.is_empty()) && !dry_run,

            Self::Task { action } => match action {
                TaskAction::Read { .. } => false,
                TaskAction::Toggle { dry_run, .. } | TaskAction::Set { dry_run, .. } => !dry_run,
            },

            // Groups whose `action` is optional default to their read-only
            // aggregate (`summary` / `list`) when omitted.
            Self::Properties { action, .. } => match action {
                None | Some(PropertiesAction::Summary { .. }) => false,
                Some(PropertiesAction::Rename { dry_run, .. }) => !dry_run,
            },

            Self::Tags { action, .. } => match action {
                None | Some(TagsAction::Summary { .. }) => false,
                Some(TagsAction::Rename { dry_run, .. }) => !dry_run,
            },

            Self::Views { action } => match action {
                None | Some(ViewsAction::List | ViewsAction::Run { .. }) => false,
                Some(ViewsAction::Set { .. } | ViewsAction::Remove { .. }) => true,
            },

            Self::Types { action } => match action {
                None | Some(TypesAction::List | TypesAction::Show { .. }) => false,
                Some(TypesAction::Set { dry_run, .. }) => !dry_run,
                Some(TypesAction::Remove { .. }) => true,
            },

            Self::LintRules { action } => match action {
                None | Some(LintRulesAction::List { .. } | LintRulesAction::Show { .. }) => false,
                Some(
                    LintRulesAction::Set { dry_run, .. } | LintRulesAction::Remove { dry_run, .. },
                ) => !dry_run,
            },

            // `hyalo links` with no subcommand defaults to the `fix` preview.
            Self::Links { action } => match action {
                None => false,
                Some(
                    LinksAction::Fix { apply, dry_run, .. }
                    | LinksAction::Auto { apply, dry_run, .. },
                ) => *apply && !dry_run,
            },

            Self::Okf { action } => match action {
                OkfAction::Index { apply, dry_run, .. } | OkfAction::Log { apply, dry_run, .. } => {
                    *apply && !dry_run
                }
            },

            Self::Madr { action } => match action {
                MadrAction::Toc { apply, dry_run, .. } => *apply && !dry_run,
            },

            Self::Changelog { action } => match action {
                ChangelogAction::Release { apply, dry_run, .. }
                | ChangelogAction::Add { apply, dry_run, .. } => *apply && !dry_run,
            },
        }
    }
}

/// Top-level subcommands (and `group sub` pairs) that write unconditionally.
///
/// Kept as data rather than a `match` so the string classifier stays a table a
/// reviewer can read against `hyalo --help`.
const ALWAYS_WRITING: &[&[&str]] = &[
    &["set"],
    &["remove"],
    &["append"],
    &["mv"],
    &["new"],
    &["create-index"],
    &["drop-index"],
    &["task", "toggle"],
    &["task", "set"],
    &["properties", "rename"],
    &["tags", "rename"],
    &["views", "set"],
    &["views", "remove"],
    &["types", "set"],
    &["types", "remove"],
    &["lint-rules", "set"],
    &["lint-rules", "remove"],
];

/// Subcommand paths that write only when `--apply` is present.
const APPLY_GATED: &[&[&str]] = &[
    &["links", "fix"],
    &["links", "auto"],
    &["okf", "index"],
    &["okf", "log"],
    &["madr", "toc"],
    &["changelog", "release"],
    &["changelog", "add"],
];

/// Global and subcommand options that take a separate value argument.
///
/// Needed so `--dir writes` or `--property status=set` cannot be mistaken for a
/// subcommand name while scanning for the command path.
const VALUE_OPTIONS: &[&str] = &[
    "-d",
    "--dir",
    "--format",
    "--jq",
    "--site-prefix",
    "--files-from",
    "--index",
    "--index-file",
    "-f",
    "--file",
    "-g",
    "--glob",
    "-p",
    "--property",
    "-t",
    "--tag",
    "--section",
    "--lines",
    "--line",
    "--limit",
    "--sort",
    "--task",
    "--title",
    "--view",
    "--status",
    "--rule",
    "--rule-prefix",
    "--fix-rule",
    "--max-per-rule",
    "--profile",
    "--to",
    "--from",
    "--threshold",
    "--min-confidence",
    "--min-length",
    "--exclude-title",
    "--exclude-target-glob",
    "--ignore-target",
    "--on-conflict",
    "--depth",
    "--recent",
    "--fields",
    "--where-property",
    "--where-tag",
    "--date",
    "--category",
    "--message",
    "--wrap",
    "--scope",
    "--adr-dir",
    "--severity",
    "--required",
    "--default",
    "--property-type",
    "--property-values",
    "--filename-template",
    "--output",
    "--target",
    "--action",
    "--type",
];

/// Classify an already-tokenized `hyalo …` command line.
///
/// `tokens` is the full argv including the leading `hyalo`; quoting must already
/// be undone. Unknown or unparseable input is reported as writing — a hint the
/// classifier cannot understand must not be presented as safe.
#[must_use]
pub(crate) fn tokens_write(tokens: &[String]) -> bool {
    let mut path: Vec<&str> = Vec::new();
    let mut has_apply = false;
    let mut has_dry_run = false;
    let mut has_fix = false;
    let mut skip_value = false;

    for tok in tokens.iter().skip(usize::from(
        tokens
            .first()
            .is_some_and(|t| t == "hyalo" || t == "hyalo.exe"),
    )) {
        if skip_value {
            skip_value = false;
            continue;
        }
        if let Some(name) = tok.split('=').next().filter(|_| tok.starts_with('-')) {
            // `--opt=value` carries its value inline; only the bare form
            // consumes the next token.
            if VALUE_OPTIONS.contains(&name) && !tok.contains('=') {
                skip_value = true;
            }
            match name {
                "--apply" => has_apply = true,
                "--dry-run" => has_dry_run = true,
                "--fix" | "--fix-rule" => has_fix = true,
                _ => {}
            }
            continue;
        }
        // Only the first two bare words can form the subcommand path; anything
        // after that is a positional argument.
        if path.len() < 2 {
            path.push(tok);
        }
    }

    if has_dry_run {
        return false;
    }
    if path.first() == Some(&"lint") {
        return has_fix;
    }
    let matches = |table: &[&[&str]]| {
        table
            .iter()
            .any(|entry| entry.len() <= path.len() && entry.iter().zip(&path).all(|(a, b)| a == b))
    };
    if matches(ALWAYS_WRITING) {
        return true;
    }
    if matches(APPLY_GATED) {
        return has_apply;
    }
    false
}

/// Classify a `hyalo …` command *string* (as hints carry them).
///
/// Undoes shell quoting, then defers to [`tokens_write`].
#[must_use]
pub(crate) fn command_line_writes(cmd: &str) -> bool {
    tokens_write(&split_command_line(cmd))
}

/// Minimal POSIX-ish tokenizer: splits on whitespace, honoring `'…'` and `"…"`.
///
/// The inverse of `hints::shell_quote`, which is the only quoter that produces
/// the strings this ever sees.
fn split_command_line(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some('\'') if c == '\\' => {
                // Inside single quotes a backslash is literal, except for the
                // `'\''` escape shell_quote emits, which the `Some(q)` arm
                // above already closed.
                cur.push(c);
            }
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => {
                started = true;
                quote = Some(c);
            }
            None if c == '\\' => {
                if let Some(next) = chars.next() {
                    started = true;
                    cur.push(next);
                }
            }
            None if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            None => {
                started = true;
                cur.push(c);
            }
        }
    }
    if started {
        out.push(cur);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::Cli;
    use clap::Parser as _;

    /// Command lines exercised by both classifiers, with the expected verdict.
    const CORPUS: &[(&str, bool)] = &[
        ("hyalo find --property status=draft", false),
        ("hyalo find 'set the property'", false),
        ("hyalo summary", false),
        ("hyalo read notes/a.md --section Tasks", false),
        ("hyalo backlinks notes/a.md", false),
        ("hyalo config", false),
        ("hyalo views list", false),
        ("hyalo views run planned", false),
        ("hyalo types list", false),
        ("hyalo types show iteration", false),
        ("hyalo lint-rules list", false),
        ("hyalo lint-rules show MD013", false),
        ("hyalo tags summary", false),
        ("hyalo properties summary", false),
        ("hyalo task read notes/a.md --line 5", false),
        ("hyalo lint", false),
        ("hyalo lint --detailed --rule MD013", false),
        ("hyalo lint --fix --dry-run", false),
        ("hyalo links fix", false),
        ("hyalo links auto", false),
        ("hyalo okf index", false),
        ("hyalo madr toc", false),
        ("hyalo changelog release 1.2.0", false),
        ("hyalo set --property status=done --file notes/a.md", true),
        (
            "hyalo set --property status=done --file notes/a.md --dry-run",
            false,
        ),
        ("hyalo remove --property owner --file notes/a.md", true),
        (
            "hyalo append --property aliases=Alt --file notes/a.md",
            true,
        ),
        ("hyalo mv notes/a.md notes/b.md", true),
        ("hyalo new --type note --file notes/c.md", true),
        ("hyalo create-index", true),
        ("hyalo task toggle notes/a.md --line 5", true),
        ("hyalo task set notes/a.md --line 5 --status doing", true),
        ("hyalo properties rename --from a --to b", true),
        ("hyalo tags rename --from a --to b", true),
        ("hyalo views set my-view --property status=draft", true),
        ("hyalo views remove my-view", true),
        ("hyalo types set note --required title", true),
        ("hyalo lint-rules set MD013 --enabled false", true),
        ("hyalo lint --fix", true),
        ("hyalo lint --fix --fix-rule HYALO001", true),
        ("hyalo links fix --apply", true),
        ("hyalo links auto --apply", true),
        ("hyalo okf index --apply", true),
        ("hyalo madr toc --apply", true),
        ("hyalo changelog release 1.2.0 --apply", true),
        ("hyalo --dir kb views set v --tag shared", true),
        ("hyalo --dir kb --format json find --tag shared", false),
    ];

    #[test]
    fn command_line_classifier_matches_the_corpus() {
        for (cmd, expected) in CORPUS {
            assert_eq!(
                command_line_writes(cmd),
                *expected,
                "command_line_writes({cmd:?})"
            );
        }
    }

    /// The two classifiers must never disagree: the hint marker would then
    /// promise "read-only" for something the config gate treats as a write.
    #[test]
    fn writes_agrees_with_command_line_classifier() {
        for (cmd, expected) in CORPUS {
            let argv = split_command_line(cmd);
            let Ok(cli) = Cli::try_parse_from(&argv) else {
                panic!("corpus entry does not parse: {cmd}");
            };
            assert_eq!(
                cli.command.writes(),
                *expected,
                "Commands::writes() for {cmd:?}"
            );
        }
    }

    #[test]
    fn quoted_arguments_do_not_leak_into_the_subcommand_path() {
        // A body search whose *pattern* is the word "set" must stay read-only.
        assert!(!command_line_writes("hyalo find 'set'"));
        assert!(!command_line_writes(r#"hyalo find "views set""#));
    }

    #[test]
    fn option_values_are_not_mistaken_for_subcommands() {
        assert!(!command_line_writes("hyalo --dir set find"));
        assert!(!command_line_writes("hyalo find --property title~=set"));
    }

    #[test]
    fn inline_option_values_are_handled() {
        assert!(!command_line_writes("hyalo --dir=set find"));
        assert!(command_line_writes("hyalo --dir=kb views set v"));
    }

    #[test]
    fn split_command_line_undoes_quoting() {
        assert_eq!(
            split_command_line("hyalo find --property 'status=in progress'"),
            vec!["hyalo", "find", "--property", "status=in progress"]
        );
    }
}
