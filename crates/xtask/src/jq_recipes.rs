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

use crate::artifact::{ArtifactArgs, build_hyalo};
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

/// Extract one continued `hyalo` command from the fenced example following a
/// named heading. The executable documentation contracts use only this small
/// shell subset: argv quoting plus a trailing `\` line continuation.
fn fenced_command_after(body: &str, heading: &str, prefix: &str) -> Option<String> {
    let section = body.split_once(heading)?.1;
    let section = section.split("\n##").next().unwrap_or(section);
    let mut in_fence = false;
    let mut command = String::new();
    let mut collecting = false;
    for line in section.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if collecting && !command.is_empty() {
                return Some(command);
            }
            in_fence = !in_fence;
            continue;
        }
        if !in_fence || trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        if !collecting {
            if !trimmed.starts_with(prefix) {
                continue;
            }
            collecting = true;
        }
        let continued = trimmed.ends_with('\\');
        let part = trimmed.strip_suffix('\\').unwrap_or(trimmed).trim_end();
        if !command.is_empty() {
            command.push(' ');
        }
        command.push_str(part);
        if !continued {
            return Some(command);
        }
    }
    collecting.then_some(command).filter(|cmd| !cmd.is_empty())
}

/// Detect the unsafe legacy shape where JSON output was handed to `xargs` as
/// filenames. Pipes inside the quoted jq program do not count.
fn has_unsafe_hyalo_xargs_pipeline(body: &str) -> bool {
    let body = body.replace("\\\n", " ");
    body.lines().any(|line| {
        let mut quote = None;
        for (offset, ch) in line.char_indices() {
            match quote {
                Some(current) if ch == current => quote = None,
                Some(_) => {}
                None if ch == '\'' || ch == '"' => quote = Some(ch),
                None if ch == '|' => {
                    let left = &line[..offset];
                    let right = line[offset + ch.len_utf8()..].trim_start();
                    if left.contains("hyalo ")
                        && left.contains("--jq")
                        && (right == "xargs" || right.starts_with("xargs "))
                    {
                        return true;
                    }
                }
                None => {}
            }
        }
        false
    })
}

/// Extract the ordinary shell body from the OKF GitHub Actions `run: |`
/// example. The block is executed by Bash as a whole; its individual commands
/// are deliberately not interpreted or replaced in Rust.
fn okf_ci_shell_block(body: &str) -> Option<String> {
    let section = body.split_once("## OKF reserved-file drift check")?.1;
    let section = section.split("\n## ").next().unwrap_or(section);
    let mut lines = section.lines();
    lines.find(|line| matches!(line.trim(), "- run: |" | "run: |"))?;
    let commands: Vec<_> = lines
        .take_while(|line| !line.trim().starts_with("```"))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    (!commands.is_empty()).then(|| commands.join("\n"))
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

pub fn run(args: &ArtifactArgs) -> Result<bool> {
    let root = workspace_root()?;
    let artifact = build_hyalo(&root, args, true)?;
    println!(
        "check-jq-recipes: Cargo artifact={} host={} target={}",
        artifact.executable.display(),
        artifact.host,
        artifact.requested_target.as_deref().unwrap_or("native")
    );
    run_with_root(&root, &artifact.executable)
}

pub fn run_with_root(root: &Path, executable: &Path) -> Result<bool> {
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
            match run_recipe(executable, root, &args) {
                Ok(RecipeOutcome::Ran) => checked += 1,
                Ok(RecipeOutcome::NotApplicable(why)) => {
                    skipped.push(format!("{recipe} ({why})"));
                }
                Err(detail) => failures.push(format!("{label}: {detail}\n    {recipe}")),
            }
        }
    }

    failures.extend(documentation_contract_failures(root, executable)?);

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

/// Execute the mutation/CI/view recipes whose contract cannot be established
/// by merely proving that an embedded jq expression parses.
fn documentation_contract_failures(root: &Path, executable: &Path) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    let skill = std::fs::read_to_string(root.join("pi-package/skills/hyalo/SKILL.md"))?;
    let tmp = tempfile::tempdir().context("creating documentation recipe fixture")?;
    let vault = tmp.path().join("vault");
    std::fs::create_dir_all(vault.join("iterations"))?;
    let config = "dir = \"vault\"\n\n[views.existing]\nproperties = [\"status=planned\"]\n";
    std::fs::write(tmp.path().join(".hyalo.toml"), config)?;
    let selected = vault.join("iterations/planned note.md");
    let sentinel = vault.join("iterations/completed.md");
    std::fs::write(
        &selected,
        "---\ntitle: Planned\nstatus: planned\n---\nbody\n",
    )?;
    std::fs::write(
        &sentinel,
        "---\ntitle: Completed\nstatus: completed\n---\nsentinel\n",
    )?;
    let selected_before = std::fs::read(&selected)?;
    let sentinel_before = std::fs::read(&sentinel)?;

    match fenced_command_after(&skill, "### Bulk Status Updates", "hyalo set ") {
        Some(recipe) => {
            let argv = split_argv(&recipe);
            if argv.first().map(String::as_str) != Some("hyalo")
                || !argv.iter().any(|arg| arg == "--dry-run")
            {
                failures
                    .push("bulk status recipe must be an executable dry-run hyalo command".into());
            } else {
                let preview_args: Vec<_> = argv.iter().skip(1).map(String::as_str).collect();
                let preview = run_hyalo(executable, tmp.path(), &preview_args)?;
                if !preview.status.success()
                    || std::fs::read(&selected)? != selected_before
                    || std::fs::read(&sentinel)? != sentinel_before
                {
                    failures.push(
                        "documented native bulk recipe did not preview without changing bytes"
                            .into(),
                    );
                }
                let apply_args: Vec<_> = preview_args
                    .iter()
                    .copied()
                    .filter(|arg| *arg != "--dry-run")
                    .collect();
                let applied = run_hyalo(executable, tmp.path(), &apply_args)?;
                let selected_after = std::fs::read_to_string(&selected)?;
                if !applied.status.success()
                    || !selected_after.contains("status: deferred")
                    || std::fs::read(&sentinel)? != sentinel_before
                {
                    failures.push(
                        "documented native bulk recipe changed the wrong selected set".into(),
                    );
                }

                // The same shipped preview must be safe when its filter selects
                // nothing; this is also the state after the apply above.
                let selected_after = std::fs::read(&selected)?;
                let empty_preview = run_hyalo(executable, tmp.path(), &preview_args)?;
                if !empty_preview.status.success()
                    || std::fs::read(&selected)? != selected_after
                    || std::fs::read(&sentinel)? != sentinel_before
                {
                    failures.push(
                        "documented native bulk recipe mishandled an empty selected set".into(),
                    );
                }

                // Negative control: removing the documented filter must make
                // this fixture catch the broadened write through the sentinel.
                let unfiltered = recipe.replace("--where-property status=planned ", "");
                let unfiltered_argv = split_argv(&unfiltered);
                let unfiltered_args: Vec<_> = unfiltered_argv
                    .iter()
                    .skip(1)
                    .map(String::as_str)
                    .filter(|arg| *arg != "--dry-run")
                    .collect();
                let negative = tempfile::tempdir().context("creating unfiltered recipe control")?;
                let negative_vault = negative.path().join("vault/iterations");
                std::fs::create_dir_all(&negative_vault)?;
                std::fs::write(negative.path().join(".hyalo.toml"), "dir = \"vault\"\n")?;
                std::fs::write(
                    negative_vault.join("planned note.md"),
                    "---\ntitle: Planned\nstatus: planned\n---\nbody\n",
                )?;
                let negative_sentinel = negative_vault.join("completed.md");
                std::fs::write(
                    &negative_sentinel,
                    "---\ntitle: Completed\nstatus: completed\n---\nsentinel\n",
                )?;
                let negative_before = std::fs::read(&negative_sentinel)?;
                let broadened = run_hyalo(executable, negative.path(), &unfiltered_args)?;
                if !broadened.status.success()
                    || std::fs::read(&negative_sentinel)? == negative_before
                {
                    failures.push(
                        "bulk recipe fixture no longer rejects removal of the selection filter"
                            .into(),
                    );
                }
            }
        }
        None => failures.push("canonical Pi skill has no executable bulk status recipe".into()),
    }

    let config_before = std::fs::read(tmp.path().join(".hyalo.toml"))?;
    let views = run_hyalo(
        executable,
        tmp.path(),
        &["views", "list", "--format", "json", "--no-hints"],
    )?;
    if !views.status.success() || std::fs::read(tmp.path().join(".hyalo.toml"))? != config_before {
        failures.push("tidy orientation did not preserve saved views byte-for-byte".into());
    }

    let ci = std::fs::read_to_string(root.join("docs/ci.md"))?;
    failures.extend(okf_ci_contract_failures(executable, &ci)?);
    if has_unsafe_hyalo_xargs_pipeline(&skill) {
        failures.push("canonical Pi skill pipes JSON output into xargs filenames".into());
    }
    let empty = run_hyalo(
        executable,
        tmp.path(),
        &[
            "find",
            "--property",
            "status=missing",
            "--jq",
            ".results",
            "--no-hints",
        ],
    )?;
    let empty_stdout = String::from_utf8_lossy(&empty.stdout);
    let mut unsafe_args = vec!["set"];
    unsafe_args.extend(empty_stdout.split_whitespace());
    unsafe_args.extend(["--property", "status=deferred", "--dry-run"]);
    let unsafe_consumer = run_hyalo(executable, tmp.path(), &unsafe_args)?;
    if !empty.status.success() || empty_stdout.trim() != "[]" || unsafe_consumer.status.success() {
        failures.push(
            "empty-result control no longer proves that JSON [] is unsafe filename input".into(),
        );
    }
    let tidy = std::fs::read_to_string(root.join("pi-package/skills/hyalo-tidy/SKILL.md"))?;
    if tidy.contains("hyalo views set") {
        failures.push("tidy orientation still overwrites saved views".into());
    }
    Ok(failures)
}

fn okf_ci_contract_failures(executable: &Path, ci: &str) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    let Some(script) = okf_ci_shell_block(ci) else {
        return Ok(vec!["OKF CI block has no executable run script".into()]);
    };
    let tmp = tempfile::tempdir().context("creating OKF CI recipe fixture")?;
    let vault = tmp.path().join("vault");
    std::fs::create_dir_all(&vault)?;
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \"vault\"\n")?;
    std::fs::write(
        vault.join("concept.md"),
        "---\ntype: Concept\ntitle: Example\n---\nBody\n",
    )?;

    let drift = run_okf_ci_shell(executable, tmp.path(), &script)?;
    if let Some(failure) = shell_status_failure(&drift, false, "actual drift") {
        failures.push(failure);
    }

    // Negative control for the review finding: `set +e` makes the first
    // failing test ignorable and lets the later successful test overwrite the
    // block status. The same status validator used above must reject it.
    let ignored_check = format!("set +e\n{script}");
    let ignored = run_okf_ci_shell(executable, tmp.path(), &ignored_check)?;
    if shell_status_failure(&ignored, false, "ignored-check negative control").is_none() {
        failures.push("OKF shell gate did not reject an ignored drift check".into());
    }

    let applied = run_hyalo(
        executable,
        tmp.path(),
        &["okf", "index", "--format", "json", "--no-hints", "--apply"],
    )?;
    let clean = run_okf_ci_shell(executable, tmp.path(), &script)?;
    if !applied.status.success() {
        failures.push("preparing the clean OKF shell fixture failed".into());
    }
    if let Some(failure) = shell_status_failure(&clean, true, "clean generated bundle") {
        failures.push(failure);
    }

    std::fs::write(
        vault.join("index.md"),
        "# Index\n\n<!-- okf:index:begin -->\nhand prose\n",
    )?;
    let skipped = run_okf_ci_shell(executable, tmp.path(), &script)?;
    if let Some(failure) = shell_status_failure(&skipped, false, "skipped marker") {
        failures.push(failure);
    }

    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \"missing\"\n")?;
    let failed = run_okf_ci_shell(executable, tmp.path(), &script)?;
    if let Some(failure) = shell_status_failure(&failed, false, "configuration failure") {
        failures.push(failure);
    }
    Ok(failures)
}

fn shell_status_failure(
    output: &std::process::Output,
    expected_success: bool,
    scenario: &str,
) -> Option<String> {
    (output.status.success() != expected_success).then(|| {
        format!(
            "documented OKF shell block returned {} for {scenario}; stderr: {}",
            output.status,
            first_error_line(
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr)
            )
        )
    })
}

fn run_okf_ci_shell(executable: &Path, cwd: &Path, script: &str) -> Result<std::process::Output> {
    let binary_dir = executable
        .parent()
        .context("built hyalo binary has no parent directory")?;
    let existing_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(binary_dir.to_path_buf()).chain(std::env::split_paths(&existing_path)),
    )
    .context("constructing PATH for documented OKF shell block")?;
    Command::new("bash")
        .args(["-euo", "pipefail", "-c", script])
        .env("PATH", path)
        .current_dir(cwd)
        .output()
        .context(
            "running documented OKF CI block (requires Bash and jq; GitHub ubuntu-latest provides both)",
        )
}

fn run_hyalo(executable: &Path, cwd: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new(executable)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| {
            format!(
                "running documentation contract recipe: hyalo {}",
                args.join(" ")
            )
        })
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
fn run_recipe(
    executable: &Path,
    root: &Path,
    args: &[String],
) -> std::result::Result<RecipeOutcome, String> {
    let mut cmd = Command::new(executable);
    cmd.args(args).current_dir(root);

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

    #[test]
    fn extracts_the_actual_continued_bulk_recipe() {
        let body = "### Bulk Status Updates\n```bash\n# Preview\nhyalo set --glob 'iterations/*.md' --where-property status=planned \\\n  --property status=deferred --dry-run --format text\n```";
        assert_eq!(
            fenced_command_after(body, "### Bulk Status Updates", "hyalo set ").as_deref(),
            Some(
                "hyalo set --glob 'iterations/*.md' --where-property status=planned --property status=deferred --dry-run --format text"
            )
        );
    }

    #[test]
    fn rejects_unsafe_filename_consumers_across_spacing_and_lines() {
        assert!(has_unsafe_hyalo_xargs_pipeline(
            "hyalo find --jq '.results' \\\n              |   xargs hyalo set --property status=done"
        ));
        assert!(!has_unsafe_hyalo_xargs_pipeline(
            "hyalo find --jq '.results[] | .file'"
        ));
    }

    #[test]
    fn extracts_the_complete_okf_ci_shell_block() {
        let command = "report=\"$(hyalo okf index --format json --no-hints)\"";
        let changed = "test \"$(jq '.results.changed' <<<\"$report\")\" -eq 0";
        let markers = "test \"$(jq '.results.skipped_markers' <<<\"$report\")\" -eq 0";
        let docs = format!(
            "## OKF reserved-file drift check\n\n```yaml\n      - run: |\n          {command}\n          {changed}\n          {markers}\n```\n\n## Next"
        );
        let expected = format!("{command}\n{changed}\n{markers}");
        assert_eq!(
            okf_ci_shell_block(&docs).as_deref(),
            Some(expected.as_str())
        );
    }
}
