#![allow(clippy::missing_errors_doc)]
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::output::{CommandOutcome, Format};
use hyalo_core::discovery;
use hyalo_core::link_rewrite::{self, Replacement, RewritePlan};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MvResult {
    from: String,
    to: String,
    dry_run: bool,
    updated_files: Vec<UpdatedFile>,
    total_files_updated: usize,
    total_links_updated: usize,
}

#[derive(Serialize)]
struct UpdatedFile {
    file: String,
    replacements: Vec<Replacement>,
}

// ---------------------------------------------------------------------------
// Command entry point
// ---------------------------------------------------------------------------

/// Run `hyalo mv --file <old> --to <new> [--dry-run]`.
pub fn mv(
    dir: &Path,
    file_arg: &str,
    to_arg: &str,
    dry_run: bool,
    format: Format,
) -> Result<CommandOutcome> {
    // 1. Validate source exists
    let (_src_full, old_rel) = match discovery::resolve_file(dir, file_arg) {
        Ok(r) => r,
        Err(e) => return Ok(crate::commands::resolve_error_to_outcome(e, format)),
    };

    // 2. Validate target path
    let new_rel = validate_target(dir, to_arg, format)?;
    let new_rel = match new_rel {
        Ok(rel) => rel,
        Err(outcome) => return Ok(outcome),
    };

    // 3. Plan all rewrites
    let plans = link_rewrite::plan_mv(dir, &old_rel, &new_rel)?;

    // 4. Build result
    let updated_files: Vec<UpdatedFile> = plans
        .iter()
        .map(|p| UpdatedFile {
            file: p.rel_path.clone(),
            replacements: p.replacements.clone(),
        })
        .collect();
    let total_links: usize = updated_files.iter().map(|f| f.replacements.len()).sum();

    let result = MvResult {
        from: old_rel.clone(),
        to: new_rel.clone(),
        dry_run,
        updated_files,
        total_files_updated: plans.len(),
        total_links_updated: total_links,
    };

    // 5. If not dry-run, execute the move and rewrites
    if !dry_run {
        execute_mv(dir, &old_rel, &new_rel, &plans)?;
    }

    // 6. Format output
    let output = match format {
        Format::Json => serde_json::to_string_pretty(&result)?,
        Format::Text => format_text(&result),
    };

    Ok(CommandOutcome::Success(output))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Validate the `--to` argument and return a normalized vault-relative path.
/// Returns `Err(CommandOutcome)` for user-facing errors, `Ok(String)` on success.
fn validate_target(
    dir: &Path,
    to_arg: &str,
    format: Format,
) -> Result<Result<String, CommandOutcome>> {
    // Normalize forward slashes
    let normalized = to_arg.replace('\\', "/");

    // Must end with .md
    if !normalized.ends_with(".md") {
        let out = crate::output::format_error(
            format,
            "target path must end with .md",
            Some(&normalized),
            Some(&format!("did you mean {normalized}.md?")),
            None,
        );
        return Ok(Err(CommandOutcome::UserError(out)));
    }

    // Reject path traversal (component-based, consistent with discovery::resolve_file)
    let has_traversal = std::path::Path::new(&normalized).components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir | std::path::Component::RootDir
        )
    }) || std::path::Path::new(&normalized).is_absolute();
    if has_traversal {
        let out = crate::output::format_error(
            format,
            "target path must be relative and within the vault",
            Some(&normalized),
            None,
            None,
        );
        return Ok(Err(CommandOutcome::UserError(out)));
    }

    // Target must not already exist
    let target_path = dir.join(&normalized);
    if target_path.exists() {
        let out = crate::output::format_error(
            format,
            "target file already exists",
            Some(&normalized),
            None,
            None,
        );
        return Ok(Err(CommandOutcome::UserError(out)));
    }

    Ok(Ok(normalized))
}

/// Execute the file move and apply all rewrite plans.
fn execute_mv(dir: &Path, old_rel: &str, new_rel: &str, plans: &[RewritePlan]) -> Result<()> {
    let src = dir.join(old_rel);
    let dst = dir.join(new_rel);

    // Create destination directory if needed
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    // Move the file
    fs::rename(&src, &dst)
        .with_context(|| format!("failed to move {} to {}", src.display(), dst.display()))?;

    // Apply link rewrites
    link_rewrite::execute_plans(plans)?;

    Ok(())
}

fn format_text(result: &MvResult) -> String {
    use std::fmt::Write;
    let mut out = String::new();

    let prefix = if result.dry_run {
        "Would move"
    } else {
        "Moved"
    };
    writeln!(out, "{prefix} {} → {}", result.from, result.to).unwrap();

    if result.updated_files.is_empty() {
        write!(out, "No links to update.").unwrap();
        return out;
    }

    let verb = if result.dry_run {
        "Would update"
    } else {
        "Updated"
    };
    writeln!(
        out,
        "{verb} {} link{} in {} file{}:",
        result.total_links_updated,
        if result.total_links_updated == 1 {
            ""
        } else {
            "s"
        },
        result.total_files_updated,
        if result.total_files_updated == 1 {
            ""
        } else {
            "s"
        },
    )
    .unwrap();

    for file in &result.updated_files {
        for r in &file.replacements {
            writeln!(
                out,
                "  {}:{}  {} → {}",
                file.file, r.line, r.old_text, r.new_text
            )
            .unwrap();
        }
    }

    out.trim_end().to_owned()
}
