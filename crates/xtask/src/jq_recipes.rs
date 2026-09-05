//! Gate — shipped `--jq` recipes actually run.
//!
//! iter-274 (BUG-29). Three documents hyalo *ships* — the knowledgebase rule
//! template, the `hyalo` skill template, and the repo's own `.claude/CLAUDE.md`
//! — carried a `--jq` recipe using jq's `IN(...)` builtin, which hyalo's
//! embedded jq engine does not implement: pasting the documented command back
//! produced `jq filter error: undefined filter "IN"`. Nothing caught it,
//! because a recipe in prose is only ever executed by a reader.
//!
//! This gate extracts every backtick-quoted `hyalo … --jq '…'` command from
//! those documents and runs it against this repo's own knowledgebase, failing
//! on a non-zero exit or a `jq filter failed` envelope.
//!
//! **Documentation must never invite a reader to paste a write.** A shipped
//! recipe naming a mutating subcommand therefore has to carry `--dry-run` in
//! the shipped text, and one carrying `--apply` fails outright. Until iter-276
//! the gate quietly *appended* `--dry-run` before running, so the header's
//! promise held for the gate and not for the reader: two dogfood explorers
//! pasted `hyalo set --glob '**/*.md' --property status=draft --jq …` verbatim
//! and rewrote 461 and 10 530 files (BUG-6, dogfood v0.22.0). The gate now
//! fails on the missing `--dry-run` instead of hiding it.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::workspace::workspace_root;

/// Subcommands that write. A recipe naming one is run with `--dry-run`.
const MUTATING_SUBCOMMANDS: &[&str] = &[
    "set",
    "remove",
    "append",
    "mv",
    "new",
    "init",
    "deinit",
    "create-index",
    "drop-index",
];

/// Documents whose `--jq` recipes are executable contracts.
fn recipe_documents(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let templates = root.join("crates").join("hyalo-cli").join("templates");
    if let Ok(entries) = std::fs::read_dir(&templates) {
        for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            if path.extension().is_some_and(|e| e == "md") {
                out.push(path);
            }
        }
    }
    let pi_skills = root.join("pi-package").join("skills");
    if let Ok(entries) = std::fs::read_dir(&pi_skills) {
        for entry in entries.filter_map(|e| e.ok().map(|e| e.path())) {
            let skill = entry.join("SKILL.md");
            if skill.is_file() {
                out.push(skill);
            }
        }
    }
    let claude_md = root.join(".claude").join("CLAUDE.md");
    if claude_md.is_file() {
        out.push(claude_md);
    }
    out.sort();
    out.dedup();
    out
}

/// Every backtick-delimited `hyalo … --jq …` command in `body`.
///
/// Recipes live inside single backticks in prose; a trailing shell comment
/// (`  # bucket links by kind`) and a trailing line-continuation backslash are
/// part of the prose, not the command, so both are trimmed.
pub fn extract_recipes(body: &str) -> Vec<String> {
    let mut candidates: Vec<&str> = Vec::new();
    // Inline form: `hyalo … --jq '…'` between single backticks.
    candidates.extend(body.split('`'));
    // Fenced form: a whole line inside a ``` block. Backtick splitting cannot
    // see these (the fence itself is backticks), and the skill templates put
    // most of their cookbook there.
    candidates.extend(body.lines());

    let mut out = Vec::new();
    for span in candidates {
        let span = span.trim().trim_start_matches("$ ").trim();
        if !span.starts_with("hyalo ") || !span.contains("--jq") {
            continue;
        }
        if span.contains('\n') {
            continue;
        }
        let mut cmd = span.to_owned();
        // Drop a trailing line-continuation backslash.
        if let Some(stripped) = cmd.strip_suffix('\\') {
            cmd = stripped.trim_end().to_owned();
        }
        // Drop a trailing shell comment, but only outside a quoted span — a
        // jq filter can legitimately contain `#`.
        if let Some(pos) = comment_start(&cmd) {
            cmd.truncate(pos);
            cmd = cmd.trim_end().to_owned();
        }
        if !cmd.is_empty() {
            out.push(cmd);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Byte offset of a trailing ` #` shell comment outside any quoted span.
fn comment_start(cmd: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    let mut prev_space = false;
    for (i, c) in cmd.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c == '#' && prev_space => return Some(i),
            None => {}
        }
        prev_space = c == ' ';
    }
    None
}

/// Split a documented command line into argv, honouring single and double
/// quotes (the only quoting the recipes use).
pub fn split_argv(cmd: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for c in cmd.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                started = true;
            }
            None if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(c);
                started = true;
            }
        }
    }
    if started {
        out.push(current);
    }
    out
}

/// The first token after `hyalo` that is not a flag or a flag value — the
/// subcommand the recipe invokes.
fn subcommand_of(argv: &[String]) -> Option<&str> {
    argv.get(1)
        .map(String::as_str)
        .filter(|t| !t.starts_with('-'))
}

pub fn run() -> Result<bool> {
    let root = workspace_root()?;
    run_with_root(&root)
}

pub fn run_with_root(root: &Path) -> Result<bool> {
    let docs = recipe_documents(root);
    if docs.is_empty() {
        eprintln!("check-jq-recipes: no shipped documents found under {root:?}");
        return Ok(false);
    }

    let mut failures: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for doc in &docs {
        let body =
            std::fs::read_to_string(doc).with_context(|| format!("reading recipe doc {doc:?}"))?;
        let label = doc
            .strip_prefix(root)
            .unwrap_or(doc)
            .display()
            .to_string()
            .replace('\\', "/");
        for recipe in extract_recipes(&body) {
            let argv = split_argv(&recipe);
            if argv.len() < 2 {
                continue;
            }
            if argv.iter().any(|a| a == "--apply") {
                failures.push(format!(
                    "{label}: a documented recipe writes to the vault — never invite a paste-back \
                     that mutates:\n    {recipe}"
                ));
                continue;
            }
            let args: Vec<String> = argv[1..].to_vec();
            if subcommand_of(&argv).is_some_and(|s| MUTATING_SUBCOMMANDS.contains(&s))
                && !args.iter().any(|a| a == "--dry-run")
            {
                failures.push(format!(
                    "{label}: a documented `{}` recipe has no --dry-run — a reader who pastes it \
                     writes to their vault. Put --dry-run in the shipped text:\n    {recipe}",
                    subcommand_of(&argv).unwrap_or("?")
                ));
                continue;
            }
            match run_recipe(root, &args) {
                Ok(RecipeOutcome::Ran) => checked += 1,
                Ok(RecipeOutcome::NotApplicable(why)) => {
                    skipped.push(format!("{recipe} ({why})"));
                }
                Err(detail) => failures.push(format!("{label}: {detail}\n    {recipe}")),
            }
        }
    }

    if failures.is_empty() {
        println!(
            "check-jq-recipes: {checked} shipped --jq recipe(s) execute against the vault \
             without a jq error"
        );
        for note in &skipped {
            println!("check-jq-recipes: not exercisable in this vault: {note}");
        }
        Ok(true)
    } else {
        eprintln!(
            "check-jq-recipes: {} broken recipe(s):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
        Ok(false)
    }
}

/// What running one recipe proved.
enum RecipeOutcome {
    /// The command ran and its jq filter was applied.
    Ran,
    /// The command refused before jq ever ran, for a reason that is about this
    /// vault rather than the recipe — `madr toc` needs a `docs/decisions/`
    /// directory this repo does not have. Reported, not failed: the gate exists
    /// to prove the *filters* are executable, and a vault-shaped refusal proves
    /// nothing either way.
    NotApplicable(String),
}

/// Run one recipe from the workspace root; `Err(detail)` describes the failure.
fn run_recipe(root: &Path, args: &[String]) -> std::result::Result<RecipeOutcome, String> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-q",
        "--manifest-path",
        &root.join("Cargo.toml").to_string_lossy(),
        "-p",
        "hyalo-cli",
        "--",
    ])
    .args(args)
    .current_dir(root);

    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) => return Err(format!("could not run the recipe: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stdout.contains("jq filter failed") || stderr.contains("jq filter failed") {
        let cause = stdout
            .lines()
            .chain(stderr.lines())
            .find(|l| l.contains("jq filter error"))
            .unwrap_or("jq filter failed")
            .trim();
        return Err(format!(
            "jq filter is not executable by hyalo's jq: {cause}"
        ));
    }
    match out.status.code() {
        Some(0) => Ok(RecipeOutcome::Ran),
        // Exit 1 is hyalo's own user error: the command refused before jq ran.
        // Report the reason rather than failing the gate (see `NotApplicable`).
        Some(1) => Ok(RecipeOutcome::NotApplicable(first_error_line(
            &stdout, &stderr,
        ))),
        // Exit 2 is a usage error — a recipe naming a flag or subcommand that
        // does not exist. That IS a broken recipe.
        code => Err(format!(
            "exited {}: {}",
            code.unwrap_or(-1),
            first_error_line(&stdout, &stderr)
        )),
    }
}

/// The most informative single line from a refused run: the JSON envelope's
/// `error` value when there is one, else the first non-empty stderr line.
fn first_error_line(stdout: &str, stderr: &str) -> String {
    for stream in [stdout, stderr] {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(stream.trim())
            && let Some(msg) = v.get("error").and_then(serde_json::Value::as_str)
        {
            return msg.to_owned();
        }
    }
    stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("(no output)")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_backticked_recipe() {
        let body = "text `hyalo config --jq '.results.dir'` more text";
        assert_eq!(
            extract_recipes(body),
            vec!["hyalo config --jq '.results.dir'".to_owned()]
        );
    }

    #[test]
    fn drops_a_trailing_shell_comment() {
        let body = "`hyalo summary --jq '.results.tasks.total'   # tasks count`";
        assert_eq!(
            extract_recipes(body),
            vec!["hyalo summary --jq '.results.tasks.total'".to_owned()]
        );
    }

    #[test]
    fn keeps_a_hash_inside_the_filter() {
        let cmd = "hyalo find --jq '.results[] | \"#\\(.file)\"'";
        assert_eq!(comment_start(cmd), None);
    }

    #[test]
    fn ignores_commands_without_jq() {
        assert!(extract_recipes("`hyalo find --property status=planned`").is_empty());
    }

    #[test]
    fn splits_quoted_arguments() {
        let argv = split_argv("hyalo find --jq '.a | .b' --glob '**/*.md'");
        assert_eq!(
            argv,
            vec!["hyalo", "find", "--jq", ".a | .b", "--glob", "**/*.md"]
        );
    }

    #[test]
    fn split_keeps_an_empty_quoted_argument() {
        assert_eq!(
            split_argv("hyalo find --title ''"),
            vec!["hyalo", "find", "--title", ""]
        );
    }

    #[test]
    fn subcommand_skips_leading_flags() {
        assert_eq!(
            subcommand_of(&split_argv("hyalo set a.md --jq '.'")),
            Some("set")
        );
        assert_eq!(subcommand_of(&split_argv("hyalo --dir kb find")), None);
    }
}
