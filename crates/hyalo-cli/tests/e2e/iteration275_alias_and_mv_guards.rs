//! Iteration 275 — alias semantics as Obsidian means them, `mv` ambiguity
//! guards for every layout, anchor and capture polish.
//!
//! - **Part A (ALIAS-1..6).** A bare `[[alias]]` is broken by default
//!   (DEC-308), fixable through the `alias_fixes` bucket, never a tie-breaker
//!   for an ambiguous stem, and an alias collision reports its candidates.
//!   The alias-resolution mode itself lives in
//!   `iteration272_resolution_completeness`.
//! - **Part B (MV-1..8).** The frontmatter ambiguity guard sees every
//!   directory layout, the moved file's own body self-links are guarded,
//!   flow lists rewrite, destinations resolve like sources, batch dry-run
//!   lists collisions.
//! - **Part C (ANCHOR-1..2, CAPTURE-1..2).** `_` folds like `-` on
//!   resolution, nested heading paths resolve, targets are trimmed and `./`
//!   is canonical.

use assert_cmd::Command;
use tempfile::TempDir;

use crate::common::hyalo_no_hints;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = hyalo_no_hints();
    cmd.current_dir(tmp.path());
    cmd
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

fn run(tmp: &TempDir, args: &[&str]) -> (i32, serde_json::Value, String) {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let json = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_str(&stderr))
        .unwrap_or(serde_json::Value::Null);
    (output.status.code().unwrap_or(-1), json, stderr)
}

fn links_of(tmp: &TempDir, file: &str) -> serde_json::Value {
    let (_, json, _) = run(tmp, &["find", "--file", file, "--fields", "links"]);
    json["results"][0]["links"].clone()
}

// ---------------------------------------------------------------------------
// Part B — MV-1 (BUG-3): the frontmatter guard keys on the stem, not the
// moved file's directory.
// ---------------------------------------------------------------------------

/// The report's matrix: moved dir × twin dir × source dir. Before iteration
/// 275 only a *root-level* moved file reached the guard, so the `271` fixture
/// passed while every real layout silently dropped the frontmatter link.
#[test]
fn mv_frontmatter_guard_fires_for_every_directory_layout() {
    let dirs = ["", "Categories/", "deep/nested/"];
    for moved_dir in dirs {
        for twin_dir in dirs {
            for source_dir in dirs {
                if moved_dir == twin_dir {
                    continue; // same path — not a twin
                }
                let tmp = TempDir::new().unwrap();
                std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
                let moved = format!("{moved_dir}Books.md");
                let twin = format!("{twin_dir}Books.md");
                let source = format!("{source_dir}src.md");
                write(&tmp, &moved, "---\ntitle: Books\n---\n");
                write(&tmp, &twin, "---\ntitle: twin\n---\n");
                write(
                    &tmp,
                    &source,
                    "---\ncategories:\n  - \"[[Books]]\"\n---\nbody [[Books]]\n",
                );

                let dest = format!("{moved_dir}Library.md");
                let (code, json, _) = run(&tmp, &["mv", &moved, &dest, "--dry-run"]);
                assert_eq!(code, 0, "{json}");
                let skipped = json["results"]["skipped_ambiguous"].as_array().unwrap();
                assert_eq!(
                    skipped.len(),
                    2,
                    "both the frontmatter and the body link must be reported \
                     (moved={moved}, twin={twin}, source={source}): {json}"
                );
                let fm = skipped
                    .iter()
                    .find(|s| s["property"] == "categories")
                    .unwrap_or_else(|| {
                        panic!("no frontmatter entry for moved={moved} twin={twin}: {json}")
                    });
                assert_eq!(fm["target"], "Books", "{json}");
                assert_eq!(fm["source"], source.as_str(), "{json}");

                // `--allow-ambiguous` rewrites them all.
                let (code, json, _) = run(
                    &tmp,
                    &["mv", &moved, &dest, "--dry-run", "--allow-ambiguous"],
                );
                assert_eq!(code, 0, "{json}");
                assert_eq!(
                    json["results"]["total_links_updated"].as_u64(),
                    Some(2),
                    "moved={moved} twin={twin} source={source}: {json}"
                );
            }
        }
    }
}

/// The kepano repro: a note whose only reference is the frontmatter list.
#[test]
fn mv_kepano_repro_reports_the_frontmatter_only_reference() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "Categories/Books.md", "---\ntitle: Books\n---\n");
    write(&tmp, "Notes/Books.md", "---\ntitle: other Books\n---\n");
    write(
        &tmp,
        "Out of Control.md",
        "---\ncategories:\n  - \"[[Books]]\"\n---\n\nbody\n",
    );

    let (code, json, stderr) = run(
        &tmp,
        &[
            "mv",
            "Categories/Books.md",
            "Categories/Library.md",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0, "{json}");
    let skipped = json["results"]["skipped_ambiguous"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{json}");
    assert_eq!(skipped[0]["source"], "Out of Control.md", "{json}");
    assert_eq!(skipped[0]["property"], "categories", "{json}");
    assert_eq!(
        skipped[0]["candidates"],
        serde_json::json!(["Categories/Books.md", "Notes/Books.md"]),
        "{json}"
    );
    let _ = stderr;
}

/// UX-2 (MV-8): text mode names the property the skipped link came from.
#[test]
fn mv_text_mode_names_the_property_of_a_skipped_frontmatter_link() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "Categories/Books.md", "---\ntitle: Books\n---\n");
    write(&tmp, "Notes/Books.md", "---\ntitle: other\n---\n");
    write(&tmp, "src.md", "---\nrelated: \"[[Books]]\"\n---\n\nbody\n");

    let output = hyalo(&tmp)
        .args([
            "mv",
            "Categories/Books.md",
            "Categories/Library.md",
            "--dry-run",
            "--format",
            "text",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("(property: related)"),
        "text mode must name the property: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Part B — MV-2 (BUG-8): the moved file's own body self-links.
// ---------------------------------------------------------------------------

#[test]
fn mv_guards_the_moved_files_own_ambiguous_body_self_links() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "kb/a.md",
        "---\nrelated: \"[[a]]\"\n---\n\nself [[a]] and [[a#Top]]\n",
    );
    write(&tmp, "kb/sub/a.md", "---\ntitle: twin\n---\n");

    let (code, json, _) = run(&tmp, &["mv", "kb/a.md", "kb/z.md", "--dry-run"]);
    assert_eq!(code, 0, "{json}");
    let skipped = json["results"]["skipped_ambiguous"].as_array().unwrap();
    assert_eq!(skipped.len(), 2, "both body self-links: {json}");
    for s in skipped {
        assert_eq!(s["self"], true, "{json}");
        assert_eq!(s["source"], "kb/a.md", "{json}");
    }
    // The frontmatter self-link still rewrites — it names the moved file
    // itself, not a shared stem someone else could mean.
    let updated = json["results"]["updated_files"].as_array().unwrap();
    assert_eq!(updated[0]["replacements"].as_array().unwrap().len(), 1);
}

#[test]
fn mv_rewrites_body_self_links_when_the_stem_is_unique() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "kb/a.md",
        "---\nrelated: \"[[a]]\"\n---\n\nself [[a]] and [[a#Top]]\n",
    );

    let (code, json, _) = run(&tmp, &["mv", "kb/a.md", "kb/z.md", "--dry-run"]);
    assert_eq!(code, 0, "{json}");
    assert!(json["results"]["skipped_ambiguous"].is_null(), "{json}");
    assert_eq!(json["results"]["total_links_updated"].as_u64(), Some(3));
}

// ---------------------------------------------------------------------------
// Part B — MV-3 (BUG-9): a `[[[x]], [[y]]]` frontmatter flow list.
// ---------------------------------------------------------------------------

#[test]
fn mv_rewrites_a_frontmatter_flow_list_of_wikilinks() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "iterations/iteration-206.md", "---\ntitle: it\n---\n");
    write(&tmp, "Target.md", "---\ntitle: T\n---\n");
    write(
        &tmp,
        "src.md",
        "---\nrelated: [[[iterations/iteration-206]], [[Target]]]\n---\n\nbody\n",
    );

    let (code, json, _) = run(
        &tmp,
        &[
            "mv",
            "iterations/iteration-206.md",
            "iterations/done.md",
            "--dry-run",
        ],
    );
    assert_eq!(code, 0, "{json}");
    let updated = json["results"]["updated_files"].as_array().unwrap();
    assert_eq!(updated.len(), 1, "{json}");
    let repl = &updated[0]["replacements"][0];
    assert_eq!(repl["old_text"], "[[iterations/iteration-206]]", "{json}");
    assert_eq!(repl["new_text"], "[[iterations/done]]", "{json}");
}

// ---------------------------------------------------------------------------
// Part B — MV-4 (BUG-10): destinations resolve exactly like sources.
// ---------------------------------------------------------------------------

#[test]
fn mv_accepts_an_absolute_in_vault_destination_and_the_vault_root() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "sub/a.md", "---\ntitle: A\n---\n\nbody\n");

    let abs = tmp.path().join("a3.md");
    let (code, json, _) = run(
        &tmp,
        &["mv", "sub/a.md", abs.to_str().unwrap(), "--dry-run"],
    );
    assert_eq!(
        code, 0,
        "an absolute in-vault destination is accepted: {json}"
    );
    assert_eq!(json["results"]["to"], "a3.md", "{json}");

    for form in ["./", ".", "./."] {
        let (code, json, _) = run(&tmp, &["mv", "sub/a.md", "--to", form, "--dry-run"]);
        assert_eq!(code, 0, "`--to {form}` is the vault root: {json}");
        assert_eq!(json["results"]["to"], "a.md", "{json}");
    }
}

/// The destination hint must never tell the caller to create the vault they
/// are already inside.
#[test]
fn mv_to_the_configured_vault_dir_means_the_vault_root() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \"kb\"\n").unwrap();
    write(&tmp, "kb/sub/a.md", "---\ntitle: A\n---\n\nbody\n");

    let (code, json, _) = run(&tmp, &["mv", "kb/sub/a.md", "--to", "kb/", "--dry-run"]);
    assert_eq!(code, 0, "{json}");
    assert_eq!(json["results"]["to"], "a.md", "{json}");
}

// ---------------------------------------------------------------------------
// Part B — MV-5 (BUG-25) and MV-7 (BUG-31).
// ---------------------------------------------------------------------------

#[test]
fn batch_mv_dry_run_lists_collisions_and_carries_the_single_file_counters() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "a/x.md", "---\nt: 1\n---\n");
    write(&tmp, "b/x.md", "---\nt: 2\n---\n");
    write(&tmp, "a/y.md", "---\nt: 3\n---\n");

    let (code, json, _) = run(&tmp, &["mv", "--glob", "{a,b}/*.md", "--to", "out"]);
    assert_eq!(
        code, 0,
        "a dry run lists the collision instead of aborting: {json}"
    );
    let collisions = json["results"]["collisions"].as_array().unwrap();
    assert_eq!(collisions.len(), 2, "{json}");
    assert_eq!(collisions[0]["destination"], "out/x.md", "{json}");
    // The non-colliding move is still planned.
    let moves = json["results"]["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 1, "{json}");
    assert_eq!(moves[0]["from"], "a/y.md", "{json}");
    // Batch JSON answers the same counter names single-file mode does.
    assert!(json["results"]["total_files_updated"].is_number(), "{json}");
    assert!(json["results"]["total_links_updated"].is_number(), "{json}");

    // `--apply` still refuses: half a batch is not what `mv --glob` promises.
    let (code, json, _) = run(
        &tmp,
        &["mv", "--glob", "{a,b}/*.md", "--to", "out", "--apply"],
    );
    assert_eq!(code, 1, "{json}");
}

#[test]
fn split_frontmatter_link_targets_are_trimmed() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "t1.md", "---\ntitle: t1\n---\n");
    write(
        &tmp,
        "src.md",
        "---\nrelated: >\n  [[t1\n  ]]\n---\n\nbody\n",
    );

    let (code, json, _) = run(&tmp, &["mv", "t1.md", "t2.md", "--dry-run"]);
    assert_eq!(code, 0, "{json}");
    let skipped = json["results"]["frontmatter_links_skipped"]
        .as_array()
        .unwrap();
    assert_eq!(
        skipped[0]["target"], "t1",
        "no trailing join artefact: {json}"
    );
}

// ---------------------------------------------------------------------------
// Part B — MV-6 (BUG-39): one warning per ambiguous link, not two.
// ---------------------------------------------------------------------------

#[test]
fn batch_mv_prints_each_ambiguous_warning_once() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "dup.md", "---\nt: 1\n---\n");
    write(&tmp, "b/dup.md", "---\nt: 2\n---\n");
    write(
        &tmp,
        "src.md",
        "---\nt: 3\n---\n\nsee [[dup]] and again [[dup]]\n",
    );

    let output = hyalo(&tmp)
        .args(["mv", "--glob", "dup.md", "--to", "out", "--format", "json"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let warnings = stderr
        .lines()
        .filter(|l| l.contains("skipping ambiguous bare wikilink"))
        .count();
    assert_eq!(warnings, 2, "two links, two warnings — not four: {stderr}");
}

// ---------------------------------------------------------------------------
// Part A — ALIAS-6 (BUG-32): text mode prints `(via alias)`.
// ---------------------------------------------------------------------------

/// `find --help` has promised `(via alias)` in text mode since iteration 272;
/// the file-object renderer never carried the field, so it never appeared. In
/// the default mode it marks a broken-but-fixable link.
#[test]
fn text_mode_prints_via_alias_on_a_broken_but_fixable_link() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "People/Leah Ferguson.md",
        "---\ntitle: Leah Ferguson\naliases:\n  - Leah\n---\n\nbody\n",
    );
    write(&tmp, "s.md", "# S\n\nSee [[Leah]].\n");

    let stdout = String::from_utf8_lossy(
        &hyalo(&tmp)
            .args([
                "find", "--file", "s.md", "--fields", "links", "--format", "text",
            ])
            .output()
            .unwrap()
            .stdout,
    )
    .into_owned();
    assert!(
        stdout.contains("(unresolved) (via alias)"),
        "a bare alias is broken AND labelled: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// Part A — ALIAS-5 (BUG-26): an alias collision reports its candidates.
// ---------------------------------------------------------------------------

#[test]
fn an_alias_collision_is_ambiguous_with_candidates() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "A.md", "---\ntitle: A\naliases: [Twin]\n---\n");
    write(&tmp, "B.md", "---\ntitle: B\naliases: [Twin]\n---\n");
    write(&tmp, "x/dup.md", "---\ntitle: X\n---\n");
    write(&tmp, "y/dup.md", "---\ntitle: Y\n---\n");
    write(&tmp, "src.md", "# S\n\n[[Twin]] and [[dup]] and [[nope]]\n");

    let (_, json, _) = run(&tmp, &["links", "fix", "--dry-run"]);
    let ambiguous = json["results"]["ambiguous_links"].as_array().unwrap();
    let twin = ambiguous.iter().find(|a| a["target"] == "Twin").unwrap();
    assert_eq!(
        twin["candidates"],
        serde_json::json!(["A.md", "B.md"]),
        "an alias collision lists its declarers, like a stem collision: {json}"
    );

    let (_, json, _) = run(&tmp, &["lint", "--rule", "HYALO006"]);
    let messages: Vec<String> = json["results"]["files"][0]["rule_groups"][0]["violations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["message"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("ambiguous wikilink: `Twin`") && m.contains("A.md, B.md")),
        "HYALO006 says ambiguous, not \"does not resolve\": {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("`nope` does not resolve")),
        "a genuinely missing target keeps its own wording: {messages:?}"
    );
}

// ---------------------------------------------------------------------------
// Part C — ANCHOR-1/2 and CAPTURE-1/2.
// ---------------------------------------------------------------------------

#[test]
fn an_underscored_fragment_resolves_to_a_space_separated_heading() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "t.md",
        "# T\n\n## Predefined fallback options\n\n\
         [[#predefined_fallback_options]] and [[#predefined_fallback_option]]\n",
    );

    let links = links_of(&tmp, "t.md");
    let links = links.as_array().unwrap();
    assert!(
        links[0]["broken_anchor"].as_bool() != Some(true),
        "the folded fragment names the whole heading: {links:?}"
    );
    assert_eq!(links[1]["broken_anchor"], true, "{links:?}");
    assert_eq!(
        links[1]["suggested_fragment"], "Predefined fallback options",
        "{links:?}"
    );
}

#[test]
fn a_nested_heading_path_resolves_and_respects_nesting() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(
        &tmp,
        "t.md",
        "# T\n\n## Heading One\n\n### Sub Two\n\n## Other\n\n### Elsewhere\n",
    );
    write(
        &tmp,
        "s.md",
        "[[t#Heading One#Sub Two]] and [[t#Heading One#Elsewhere]]\n",
    );

    let links = links_of(&tmp, "s.md");
    let links = links.as_array().unwrap();
    assert!(
        links[0]["broken_anchor"].as_bool() != Some(true),
        "{links:?}"
    );
    assert_eq!(
        links[1]["broken_anchor"], true,
        "`Elsewhere` exists, but not under `Heading One`: {links:?}"
    );
}

#[test]
fn wikilink_targets_are_trimmed_and_dot_slash_is_canonical() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    write(&tmp, "a.md", "---\ntitle: A\n---\n\nbody\n");
    write(&tmp, "s.md", "# S\n\n[[ a ]] [[a ]] [[./a]]\n");

    let links = links_of(&tmp, "s.md");
    for link in links.as_array().unwrap() {
        assert_eq!(
            link["path"], "a.md",
            "every spelling names the same file: {links:?}"
        );
    }
    // And HYALO006 stops firing on them.
    let (_, json, _) = run(&tmp, &["lint", "--rule", "HYALO006"]);
    assert_eq!(
        json["results"]["files_with_violations"].as_u64(),
        Some(0),
        "{json}"
    );
}
