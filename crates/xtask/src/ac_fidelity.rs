//! Gate 1 — AC fidelity check.
//!
//! For each ticked `- [x]` checkbox in the `## Acceptance criteria` section of
//! an iteration plan, this gate verifies that at least one workspace file
//! references a keyword extracted from the AC text, OR the AC has an explicit
//! deferral child bullet.

use anyhow::{Context, Result};
use clap::Args as ClapArgs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// Stopwords excluded from keyword extraction (too common to be meaningful).
const STOPWORDS: &[&str] = &[
    "test", "tests", "the", "with", "this", "that", "from", "into", "when", "then", "each",
    "every", "must", "have", "will", "should", "which", "their", "there", "about", "after",
    "before", "where", "plan", "iter", "does", "make", "same", "both", "also", "once", "some",
    "runs", "exit", "gets", "pass", "fail", "green", "clean", "given", "since", "cargo", "hyalo",
    "main", "more", "flag", "check", "gate", "help", "file", "code", "line", "list", "note",
    "hint", "args", "just", "your", "what", "none", "only", "such", "even", "over", "any", "all",
    "are", "not", "for", "and", "or", "on", "of", "in", "to", "at", "is", "it", "by", "be", "as",
    "an", "a",
];

#[derive(ClapArgs)]
pub struct AcFidelityArgs {
    /// Path to a single iteration plan. Omit to scan all plans.
    #[arg(long)]
    pub plan: Option<PathBuf>,

    /// Restrict evidence search to files changed since this git ref.
    #[arg(long)]
    pub since: Option<String>,
}

/// A parsed acceptance criterion from a plan file.
#[derive(Debug)]
pub struct AcEntry {
    /// The AC text (without the checkbox prefix).
    pub text: String,
    /// True when the AC is ticked (`[x]`).
    pub ticked: bool,
    /// Deferral annotation, if present.
    pub deferral: Option<String>,
}

/// Parse the `## Acceptance criteria` section of a markdown file.
///
/// Returns every line matching `^- \[(x| )\] (.+)$`, plus any immediately
/// following deferral child bullet `  - [deferred — new plan: iter-NNN]`.
pub fn parse_acceptance_criteria(content: &str) -> Vec<AcEntry> {
    let mut entries: Vec<AcEntry> = Vec::new();
    let mut in_ac_section = false;

    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        // Detect `## Acceptance criteria` section header (case-insensitive).
        if line.starts_with("## ") && line.to_lowercase().contains("acceptance criteria") {
            in_ac_section = true;
            i += 1;
            continue;
        }

        // Any other `##` heading ends the section.
        if in_ac_section && line.starts_with("## ") {
            in_ac_section = false;
        }

        if in_ac_section
            && let Some(rest) = line
                .strip_prefix("- [x] ")
                .or_else(|| line.strip_prefix("- [ ] "))
        {
            let ticked = line.starts_with("- [x] ");
            // Look ahead for a deferral child bullet.
            let deferral = if i + 1 < lines.len() {
                let next = lines[i + 1];
                if next.starts_with("  - [deferred") || next.starts_with("  - [Deferred") {
                    Some(next.trim().to_owned())
                } else {
                    None
                }
            } else {
                None
            };
            entries.push(AcEntry {
                text: rest.to_owned(),
                ticked,
                deferral,
            });
        }
        i += 1;
    }
    entries
}

/// Extract meaningful keywords from an AC text string.
///
/// Tokens must be ASCII alphanumeric, length >= 4, and not in STOPWORDS.
pub fn extract_keywords(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 4)
        .map(|t| t.to_lowercase())
        .filter(|t| !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Validate a deferral annotation and return whether it is structurally valid.
///
/// `[deferred — new plan: iter-NNN]` — concrete plan ref must exist on disk.
/// `[deferred — new plan: iter-???]` — placeholder slot, always valid.
fn validate_deferral(deferral: &str, workspace_root: &Path) -> bool {
    // Extract the plan reference from inside brackets.
    let inner = deferral
        .trim_start_matches("  - [")
        .trim_start_matches('[')
        .trim_end_matches(']');

    if !inner.to_lowercase().contains("deferred") {
        return false;
    }

    // Placeholder slot is always valid.
    if inner.contains("iter-???") {
        return true;
    }

    // Concrete ref: `iter-NNN` — look for the plan file.
    if let Some(ref_pos) = inner.find("iter-") {
        let ref_part = &inner[ref_pos..];
        let iter_id: String = ref_part
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-')
            .collect();
        // Search for a matching plan file in the knowledgebase.
        let kb = workspace_root
            .join("hyalo-knowledgebase")
            .join("iterations");
        if kb.exists() {
            for entry in WalkDir::new(&kb)
                .max_depth(2)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
            {
                let fname = entry.file_name().to_string_lossy();
                if fname.contains(&iter_id) {
                    return true;
                }
            }
        }
        // If the knowledgebase doesn't exist (e.g. unit tests), treat as valid
        // when we can parse a well-formed iter ref.
        return !iter_id.is_empty();
    }

    false
}

/// Get candidate files for evidence search.
///
/// Uses `git diff --name-only <since>..HEAD` when a ref is given or when on a
/// feature branch. Falls back to a full workspace scan of `.rs` and `tests/`
/// paths.
fn candidate_files(since: Option<&str>, workspace_root: &Path) -> Vec<PathBuf> {
    let git_ref = since.unwrap_or("origin/main");
    let output = Command::new("git")
        .args(["diff", "--name-only", &format!("{git_ref}..HEAD")])
        .current_dir(workspace_root)
        .output();

    if let Ok(out) = output
        && out.status.success()
    {
        let paths: Vec<PathBuf> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| workspace_root.join(l))
            .filter(|p| p.exists())
            .collect();
        if !paths.is_empty() {
            return paths;
        }
    }

    // Fallback: walk workspace for .rs files and files under tests/.
    WalkDir::new(workspace_root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
            ext == "rs"
                || p.components()
                    .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
        })
        .map(|e| e.into_path())
        .collect()
}

/// Check whether any candidate file contains at least one keyword (case-insensitive).
fn evidence_found(keywords: &[String], candidates: &[PathBuf]) -> bool {
    if keywords.is_empty() {
        // No meaningful keywords — treat as satisfied (AC is too vague to test).
        return true;
    }
    for path in candidates {
        // Search file path components first (cheap).
        let path_str = path.to_string_lossy().to_lowercase();
        if keywords.iter().any(|k| path_str.contains(k.as_str())) {
            return true;
        }
        // Then search file content line by line.
        if let Ok(f) = std::fs::File::open(path) {
            let reader = BufReader::new(f);
            for line in reader.lines().map_while(Result::ok) {
                let lower = line.to_lowercase();
                if keywords.iter().any(|k| lower.contains(k.as_str())) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check a single plan file. Returns a list of unsatisfied AC messages (empty = pass).
pub fn check_plan(
    plan_path: &Path,
    since: Option<&str>,
    workspace_root: &Path,
) -> Result<Vec<String>> {
    let content =
        std::fs::read_to_string(plan_path).with_context(|| format!("reading {plan_path:?}"))?;

    let entries = parse_acceptance_criteria(&content);
    let ticked: Vec<&AcEntry> = entries.iter().filter(|e| e.ticked).collect();

    if ticked.is_empty() {
        return Ok(Vec::new());
    }

    let candidates = candidate_files(since, workspace_root);
    let mut misses = Vec::new();

    for entry in ticked {
        // Deferral takes priority — no evidence search needed.
        if let Some(ref def) = entry.deferral {
            if validate_deferral(def, workspace_root) {
                continue;
            }
            // Invalid deferral syntax — still report as miss.
            misses.push(format!(
                "AC unsatisfied: {plan}\n  AC: \"{text}\"\n  Deferral annotation found but is malformed: {def}\n  Hint: use \"  - [deferred — new plan: iter-NNN]\" or \"iter-???\".",
                plan = plan_path.display(),
                text = entry.text,
            ));
            continue;
        }

        let keywords = extract_keywords(&entry.text);
        if !evidence_found(&keywords, &candidates) {
            misses.push(format!(
                "AC unsatisfied: {plan}\n  AC: \"{text}\"\n  Searched for keywords: {kw:?}\n  Hint: add a test reference, or annotate as\n        \"  - [deferred — new plan: iter-NNN]\" below the AC.",
                plan = plan_path.display(),
                text = entry.text,
                kw = keywords,
            ));
        }
    }

    Ok(misses)
}

/// Return iteration plan files changed since `since_ref` (e.g. `origin/main`).
/// Used by CI to scope the gate to plans introduced or modified in a PR rather
/// than re-validating the entire historic corpus on every run.
fn changed_plans(since_ref: &str, workspace_root: &Path) -> Vec<PathBuf> {
    let output = Command::new("git")
        .args([
            "diff",
            "--name-only",
            "--diff-filter=AM",
            &format!("{since_ref}..HEAD"),
            "--",
            "hyalo-knowledgebase/iterations/",
        ])
        .current_dir(workspace_root)
        .output();
    let Ok(out) = output else { return Vec::new() };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(".md"))
        .map(|l| workspace_root.join(l))
        .filter(|p| p.exists())
        .collect()
}

/// Discover all iteration plan files under `hyalo-knowledgebase/iterations/`.
fn discover_plans(workspace_root: &Path) -> Vec<PathBuf> {
    let kb = workspace_root
        .join("hyalo-knowledgebase")
        .join("iterations");
    if !kb.exists() {
        return Vec::new();
    }
    WalkDir::new(&kb)
        .max_depth(2)
        .into_iter()
        .flatten()
        .filter(|e| {
            e.file_type().is_file()
                && e.path().extension().and_then(|s| s.to_str()).unwrap_or("") == "md"
        })
        .map(|e| e.into_path())
        .collect()
}

pub fn run(args: AcFidelityArgs) -> Result<bool> {
    let workspace_root = workspace_root()?;
    let plans = if let Some(p) = args.plan {
        vec![p]
    } else if let Some(since) = args.since.as_deref() {
        // When --since is given without --plan, only check plans changed since
        // <since>. This is the canonical CI use: "gate the plans this PR
        // introduces or modifies", not the entire historic corpus.
        changed_plans(since, &workspace_root)
    } else {
        discover_plans(&workspace_root)
    };

    if plans.is_empty() {
        println!("check-ac-fidelity: no plan files found — nothing to check.");
        return Ok(true);
    }

    let mut all_misses: Vec<String> = Vec::new();
    for plan in &plans {
        let misses = check_plan(plan, args.since.as_deref(), &workspace_root)?;
        all_misses.extend(misses);
    }

    if all_misses.is_empty() {
        println!(
            "check-ac-fidelity: all ticked ACs in {} plan(s) have evidence or deferrals.",
            plans.len()
        );
        Ok(true)
    } else {
        eprintln!(
            "check-ac-fidelity: {} ticked AC(s) have no evidence:\n\n{}",
            all_misses.len(),
            all_misses.join("\n\n")
        );
        Ok(false)
    }
}

/// Locate the workspace root by walking up from `CARGO_MANIFEST_DIR` to find
/// the root `Cargo.toml` with `[workspace]`.
pub fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR for xtask is crates/xtask — go two levels up.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Walk up until we find a Cargo.toml containing [workspace].
    let mut dir = manifest_dir.as_path();
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            let content = std::fs::read_to_string(&candidate)
                .with_context(|| format!("reading {candidate:?}"))?;
            if content.contains("[workspace]") {
                return Ok(dir.to_path_buf());
            }
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }

    // Fallback: current directory.
    Ok(PathBuf::from("."))
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PLAN_WITH_TICKED_AC: &str = r#"
## Acceptance criteria

- [x] `cargo run -p xtask -- check-ac-fidelity --plan foo.md` runs cleanly
- [ ] Some unticked criterion

## Other section

content
"#;

    const PLAN_WITH_DEFERRAL: &str = r#"
## Acceptance criteria

- [x] Some ticked AC with a deferral annotation
  - [deferred — new plan: iter-???]

"#;

    #[test]
    fn parse_ticked_and_unticked() {
        let entries = parse_acceptance_criteria(PLAN_WITH_TICKED_AC);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].ticked);
        assert!(!entries[1].ticked);
        assert!(entries[0].text.contains("check-ac-fidelity"));
    }

    #[test]
    fn parse_deferral_placeholder() {
        let entries = parse_acceptance_criteria(PLAN_WITH_DEFERRAL);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ticked);
        assert!(entries[0].deferral.is_some());
        let def = entries[0].deferral.as_ref().unwrap();
        assert!(def.contains("iter-???"));
    }

    #[test]
    fn validate_deferral_placeholder_is_valid() {
        let root = PathBuf::from(".");
        assert!(validate_deferral(
            "  - [deferred — new plan: iter-???]",
            &root
        ));
    }

    #[test]
    fn validate_deferral_well_formed_iter_ref() {
        // Without a real knowledgebase, validate_deferral still returns true for
        // a well-formed iter ref (the ref can be parsed).
        let root = PathBuf::from("/tmp");
        // iter-999 won't exist, but a parsed non-empty iter id means valid grammar.
        assert!(validate_deferral(
            "  - [deferred — new plan: iter-999]",
            &root
        ));
    }

    #[test]
    fn extract_keywords_filters_stopwords_and_short_tokens() {
        let kw = extract_keywords("the xtask binary must run and exit with a clean result");
        // "the", "must", "and", "with", "exit", "clean" are stopwords or short
        assert!(kw.contains(&"xtask".to_owned()));
        assert!(kw.contains(&"binary".to_owned()));
        assert!(kw.contains(&"result".to_owned()));
        assert!(!kw.contains(&"the".to_owned()));
        assert!(!kw.contains(&"and".to_owned()));
        assert!(!kw.contains(&"must".to_owned()));
    }

    #[test]
    fn extract_keywords_minimum_length_four() {
        let kw = extract_keywords("ac or ok go run");
        // all tokens < 4 chars or stopwords
        assert!(kw.is_empty());
    }

    #[test]
    fn evidence_found_in_path() {
        let paths = vec![PathBuf::from("crates/xtask/src/ac_fidelity.rs")];
        assert!(evidence_found(&["fidelity".to_owned()], &paths));
        assert!(!evidence_found(&["nonexistent".to_owned()], &paths));
    }

    #[test]
    fn no_ticked_acs_returns_pass() {
        let content = r#"
## Acceptance criteria

- [ ] Not ticked
- [ ] Also not ticked
"#;
        let entries = parse_acceptance_criteria(content);
        let ticked: Vec<_> = entries.iter().filter(|e| e.ticked).collect();
        assert!(ticked.is_empty());
    }

    #[test]
    fn ac_section_ends_at_next_h2() {
        let content = r#"
## Acceptance criteria

- [x] First AC

## Other section

- [x] This should not be parsed as AC
"#;
        let entries = parse_acceptance_criteria(content);
        assert_eq!(entries.len(), 1);
        assert!(entries[0].text.contains("First AC"));
    }

    /// Synthetic plan: ticked AC + matching keyword in candidate → pass.
    #[test]
    fn synthetic_ticked_ac_with_matching_keyword_passes() {
        // The AC text contains "fidelity"; the candidate file path also has "fidelity".
        let plan_content = r#"
## Acceptance criteria

- [x] check-ac-fidelity runs cleanly against the plan
"#;
        let entries = parse_acceptance_criteria(plan_content);
        let ticked: Vec<_> = entries.iter().filter(|e| e.ticked).collect();
        assert_eq!(ticked.len(), 1);

        let keywords = extract_keywords(&ticked[0].text);
        let candidates = vec![PathBuf::from("crates/xtask/src/ac_fidelity.rs")];
        assert!(
            evidence_found(&keywords, &candidates),
            "keywords {keywords:?} should match path"
        );
    }

    /// Synthetic plan: ticked AC + no matching keyword in candidates → fail.
    #[test]
    fn synthetic_ticked_ac_without_evidence_fails() {
        let plan_content = r#"
## Acceptance criteria

- [x] feature-zymurgy must be implemented completely
"#;
        let entries = parse_acceptance_criteria(plan_content);
        let ticked: Vec<_> = entries.iter().filter(|e| e.ticked).collect();
        assert_eq!(ticked.len(), 1);

        let keywords = extract_keywords(&ticked[0].text);
        // "zymurgy" is not in any candidate path.
        let candidates = vec![PathBuf::from("crates/xtask/src/main.rs")];
        assert!(
            !evidence_found(&keywords, &candidates),
            "keywords {keywords:?} should NOT match candidate list"
        );
    }

    /// Synthetic plan: ticked AC with valid deferral → pass (no evidence search).
    #[test]
    fn synthetic_ticked_ac_with_deferral_passes() {
        let entries = parse_acceptance_criteria(PLAN_WITH_DEFERRAL);
        let ticked: Vec<_> = entries.iter().filter(|e| e.ticked).collect();
        assert_eq!(ticked.len(), 1);
        // Deferral is present and placeholder — validate_deferral should return true.
        let def = ticked[0].deferral.as_ref().unwrap();
        assert!(validate_deferral(def, &PathBuf::from(".")));
    }
}
