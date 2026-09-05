//! Iteration 272 — resolution completeness.
//!
//! - **Part A (BUG-5).** A one-element list `type:` binds *and* passes the
//!   implicit string constraint every declared type carries.
//! - **Part B (BUG-6, DEC-296).** Frontmatter `aliases:` resolve wikilinks,
//!   consistently across `find`, `backlinks`, `summary` and `links fix`.
//! - **Part C (BUG-8).** `[text](#fragment)` is a markdown link, not a
//!   wikilink.
//! - **Part D (BUG-15, BUG-16, BUG-21).** Scanner capture boundaries.
//! - **Part F (CASE-2).** `links fix` reports the string `--apply` writes.

use assert_cmd::Command;
use tempfile::TempDir;

fn hyalo(tmp: &TempDir) -> Command {
    let mut cmd = crate::common::hyalo_no_hints();
    cmd.arg("--dir").arg(tmp.path().to_str().unwrap());
    cmd
}

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let output = hyalo(tmp)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("json output")
}

fn write(tmp: &TempDir, rel: &str, body: &str) {
    let path = tmp.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Every link of `file`, as `(target, path, kind, via)` tuples.
fn links_of(tmp: &TempDir, file: &str) -> Vec<(String, Option<String>, String, Option<String>)> {
    let out = run_json(tmp, &["find", "--file", file, "--fields", "links"]);
    out["results"][0]["links"]
        .as_array()
        .expect("links array")
        .iter()
        .map(|l| {
            (
                l["target"].as_str().unwrap_or_default().to_owned(),
                l["path"].as_str().map(str::to_owned),
                l["kind"].as_str().unwrap_or_default().to_owned(),
                l["via"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Part A — a list-typed `type:` under a declared schema
// ---------------------------------------------------------------------------

#[test]
fn part_a_list_typed_type_lints_clean_under_a_declared_schema() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        ".hyalo.toml",
        "[schema.default]\nrequired = [\"title\", \"type\"]\n\n\
         [schema.types.Authors]\nrequired = [\"title\", \"type\"]\n",
    );
    // The shape Obsidian's own property editor writes for a link-typed
    // property. Binding always accepted it (DEC-281); the implicit
    // `type: string` constraint did not.
    write(
        &tmp,
        "l.md",
        "---\ntitle: L\ntype: [\"[[Authors]]\"]\n---\n\n# L\n",
    );
    // `lint` exits non-zero when it reports anything, so read the payload
    // rather than asserting success.
    let output = hyalo(&tmp)
        .args(["lint", "--file", "l.md", "--strict", "--format", "json"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        !text.contains("expected string"),
        "list-typed `type` must not trip the string constraint: {text}"
    );
}

#[test]
fn part_a_multi_element_and_empty_type_lists_are_still_rejected() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        ".hyalo.toml",
        "[schema.default]\nrequired = [\"title\", \"type\"]\n\n\
         [schema.types.Authors]\nrequired = [\"title\", \"type\"]\n",
    );
    write(
        &tmp,
        "multi.md",
        "---\ntitle: M\ntype: [\"a\", \"b\"]\n---\n",
    );
    let output = hyalo(&tmp)
        .args(["lint", "--file", "multi.md", "--format", "json"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        text.contains("must name one type"),
        "a two-element list must still be reported: {text}"
    );
}

// ---------------------------------------------------------------------------
// Part B — frontmatter `aliases:` as link targets (DEC-296)
// ---------------------------------------------------------------------------

/// A vault whose notes declare aliases. `aliases_resolve` opts the vault into
/// the Alias Linker mode (`[links] aliases = true`); without it the default —
/// Obsidian's own behaviour, DEC-308 — leaves a bare `[[alias]]` unresolved.
fn alias_vault_with(aliases_resolve: bool) -> TempDir {
    let tmp = alias_vault();
    if aliases_resolve {
        write(&tmp, ".hyalo.toml", "[links]\naliases = true\n");
    }
    tmp
}

fn alias_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "people/Leah Ferguson.md",
        "---\ntitle: Leah Ferguson\naliases:\n  - Leah\n  - \"L. Ferguson\"\n---\n\n# Leah Ferguson\n\n## Work\n",
    );
    write(
        &tmp,
        "src.md",
        "See [[Leah]], [[L. Ferguson|her]] and [[Nobody]].\n",
    );
    tmp
}

#[test]
fn part_b_a_declared_alias_resolves_and_is_labelled_via_alias() {
    // DEC-308 (iter-275): only under `[links] aliases = true`.
    let tmp = alias_vault_with(true);
    let links = links_of(&tmp, "src.md");
    assert_eq!(
        links[0],
        (
            "Leah".to_owned(),
            Some("people/Leah Ferguson.md".to_owned()),
            "wikilink".to_owned(),
            Some("alias".to_owned())
        )
    );
    // A multi-word alias with a label works the same way.
    assert_eq!(links[1].1.as_deref(), Some("people/Leah Ferguson.md"));
    assert_eq!(links[1].3.as_deref(), Some("alias"));
    // A genuinely unknown target is still broken and carries no `via`.
    assert_eq!(links[2].1, None);
    assert_eq!(links[2].3, None);
}

/// DEC-308 (iter-275, BUG-1): Obsidian does not resolve a bare `[[alias]]` —
/// aliases feed its link *suggester*, which writes `[[Note|alias]]`. hyalo
/// reports the link the way Obsidian renders it: broken, but labelled
/// `via: "alias"` so the reader knows an exact fix exists.
#[test]
fn part_b_a_bare_alias_is_broken_by_default_but_labelled_via_alias() {
    let tmp = alias_vault();
    let links = links_of(&tmp, "src.md");
    assert_eq!(links[0].0, "Leah");
    assert_eq!(links[0].1, None, "a bare alias does not resolve by default");
    assert_eq!(links[0].3.as_deref(), Some("alias"));
    assert_eq!(links[1].1, None);
    assert_eq!(links[1].3.as_deref(), Some("alias"));
    // The unknown target carries no `via` — nothing declares it.
    assert_eq!(links[2].1, None);
    assert_eq!(links[2].3, None);

    let cfg = run_json(&tmp, &["config"]);
    assert_eq!(cfg["results"]["links"]["aliases"].as_bool(), Some(false));
}

/// ALIAS-2 (iter-275): the alias map's job in the default mode is to give
/// `links fix` the one rewrite that is right — Obsidian's own suggester form.
#[test]
fn part_b_links_fix_proposes_the_alias_backed_rewrite() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "Leah Ferguson.md",
        "---\ntitle: Leah Ferguson\naliases:\n  - Leah\n---\n",
    );
    write(
        &tmp,
        "src.md",
        "See [[Leah]].\n\nAnd [[Leah|the boss]].\n\nBut not [x](Leah).\n",
    );

    let out = run_json(&tmp, &["links", "fix", "--dry-run"]);
    let r = &out["results"];
    assert_eq!(
        r["alias_fixes"].as_u64(),
        Some(2),
        "wikilinks only — a markdown `[x](Leah)` names a file beside its own \
         source, so an alias is not an answer for it: {r}"
    );
    let plans = r["alias_fix_plans"].as_array().unwrap();
    assert_eq!(plans[0]["strategy"], "Alias", "{r}");
    assert_eq!(plans[0]["confidence"].as_f64(), Some(1.0), "{r}");
    assert_eq!(plans[0]["new_target"], "Leah Ferguson.md", "{r}");
    assert_eq!(
        plans[0]["emitted_target"], "Leah Ferguson|Leah",
        "the label is the alias the author wrote: {r}"
    );
    // The author's own label survives.
    assert_eq!(plans[1]["emitted_target"], "Leah Ferguson", "{r}");

    // Plain `--apply` writes them; the fuzzy path is never involved.
    run_json(&tmp, &["links", "fix", "--apply"]);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
        "See [[Leah Ferguson|Leah]].\n\nAnd [[Leah Ferguson|the boss]].\n\nBut not [x](Leah).\n"
    );
}

/// ALIAS-4 (iter-275, BUG-2): an ambiguous filename match is still a filename
/// match — an alias never breaks the tie, in either mode.
#[test]
fn part_b_an_ambiguous_stem_is_never_tie_broken_by_an_alias() {
    for aliases_resolve in [false, true] {
        let tmp = TempDir::new().unwrap();
        if aliases_resolve {
            write(&tmp, ".hyalo.toml", "[links]\naliases = true\n");
        }
        write(
            &tmp,
            "Plugins/avatar.md",
            "---\ntitle: Avatar plugin\naliases:\n  - Avatar\n---\n",
        );
        write(&tmp, "Themes/Avatar.md", "---\ntitle: Avatar theme\n---\n");
        write(&tmp, "src.md", "See [[avatar]].\n");

        let links = links_of(&tmp, "src.md");
        assert_eq!(
            links[0].1, None,
            "two files carry the stem, so the link names neither \
             (aliases_resolve = {aliases_resolve})"
        );
    }
}

#[test]
fn part_b_alias_links_are_graph_edges_and_are_not_counted_broken() {
    let tmp = alias_vault_with(true);
    let backlinks = run_json(&tmp, &["backlinks", "people/Leah Ferguson.md"]);
    assert_eq!(
        backlinks["results"]["backlinks"][0]["source"]
            .as_str()
            .unwrap(),
        "src.md",
        "an alias-resolved link is a real edge: {backlinks}"
    );
    let summary = run_json(&tmp, &["summary"]);
    // Three links, one genuinely broken (`[[Nobody]]`).
    assert_eq!(summary["results"]["links"]["broken"].as_u64(), Some(1));
}

#[test]
fn part_b_links_fix_never_proposes_a_rewrite_for_a_declared_alias() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, ".hyalo.toml", "[links]\naliases = true\n");
    // `Lewuathe.md` is the Obsidian Hub's real fuzzy trap for `[[Leah]]`.
    write(&tmp, "Lewuathe.md", "---\ntitle: Lewuathe\n---\n");
    write(
        &tmp,
        "Leah Ferguson.md",
        "---\ntitle: Leah Ferguson\naliases:\n  - Leah\n---\n",
    );
    write(&tmp, "src.md", "See [[Leah]].\n");
    let out = run_json(&tmp, &["links", "fix", "--apply-fuzzy", "--dry-run"]);
    let r = &out["results"];
    assert_eq!(r["broken"].as_u64(), Some(0), "{r}");
    assert_eq!(r["fuzzy"].as_u64(), Some(0), "{r}");
    assert_eq!(r["unfixable"].as_u64(), Some(0), "{r}");
    // And the file is untouched.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
        "See [[Leah]].\n"
    );
}

#[test]
fn part_b_a_filename_beats_an_alias_and_a_shared_alias_is_ambiguous() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, ".hyalo.toml", "[links]\naliases = true\n");
    write(&tmp, "Leah.md", "---\ntitle: The real Leah\n---\n");
    write(
        &tmp,
        "Leah Ferguson.md",
        "---\ntitle: Leah Ferguson\naliases:\n  - Leah\n---\n",
    );
    write(&tmp, "a.md", "---\ntitle: A\naliases:\n  - Shared\n---\n");
    write(&tmp, "b.md", "---\ntitle: B\naliases: Shared\n---\n");
    write(&tmp, "src.md", "See [[Leah]] and [[Shared]].\n");

    let links = links_of(&tmp, "src.md");
    assert_eq!(links[0].1.as_deref(), Some("Leah.md"));
    assert_eq!(links[0].3, None, "a filename match is not `via: alias`");
    assert_eq!(
        links[1].1, None,
        "an alias claimed twice resolves to nothing"
    );
}

#[test]
fn part_b_mv_does_not_rewrite_a_link_written_through_an_alias() {
    let tmp = alias_vault_with(true);
    let out = run_json(
        &tmp,
        &["mv", "people/Leah Ferguson.md", "archive/Leah Ferguson.md"],
    );
    assert_eq!(out["results"]["total_links_updated"].as_u64(), Some(0));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src.md")).unwrap(),
        "See [[Leah]], [[L. Ferguson|her]] and [[Nobody]].\n",
        "an alias link needs no rewrite — the alias moved with the note"
    );
    // And it still resolves at the new location.
    assert_eq!(
        links_of(&tmp, "src.md")[0].1.as_deref(),
        Some("archive/Leah Ferguson.md")
    );
}

#[test]
fn part_b_links_aliases_false_restores_filename_only_resolution() {
    let tmp = alias_vault();
    write(&tmp, ".hyalo.toml", "[links]\naliases = false\n");
    let links = links_of(&tmp, "src.md");
    assert_eq!(links[0].1, None, "opt-out disables alias resolution");
    let cfg = run_json(&tmp, &["config"]);
    assert_eq!(cfg["results"]["links"]["aliases"].as_bool(), Some(false));
}

#[test]
fn part_b_index_and_disk_agree_on_alias_resolution() {
    let tmp = alias_vault_with(true);
    let index = tmp.path().join("vault.hyalo-index");
    let index_str = index.to_str().unwrap();
    hyalo(&tmp)
        .args(["create-index", "--index-file", index_str])
        .assert()
        .success();
    let disk = run_json(&tmp, &["find", "--file", "src.md", "--fields", "links"]);
    let indexed = run_json(
        &tmp,
        &[
            "find",
            "--file",
            "src.md",
            "--fields",
            "links",
            "--index-file",
            index_str,
        ],
    );
    assert_eq!(disk["results"], indexed["results"]);
}

// ---------------------------------------------------------------------------
// Part C — an anchor-only markdown link is markdown
// ---------------------------------------------------------------------------

#[test]
fn part_c_anchor_only_markdown_links_are_not_reported_as_wikilinks() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "page.md",
        "# Page\n\nSee [Browser compatibility](#browser-compat) and [[#Notes]].\n\n\
         ## Browser compat\n\n## Notes\n",
    );
    let links = links_of(&tmp, "page.md");
    let anchors: Vec<_> = links.iter().filter(|l| l.0.is_empty()).collect();
    assert_eq!(anchors.len(), 2, "{links:?}");
    assert_eq!(anchors[0].2, "markdown", "{links:?}");
    assert_eq!(anchors[1].2, "wikilink", "{links:?}");

    // The markdown one carries its link text.
    let out = run_json(&tmp, &["find", "--file", "page.md", "--fields", "links"]);
    let first = &out["results"][0]["links"][0];
    assert_eq!(first["label"].as_str(), Some("Browser compatibility"));
    assert_eq!(first["fragment"].as_str(), Some("browser-compat"));
}

// ---------------------------------------------------------------------------
// Part D — scanner capture boundaries
// ---------------------------------------------------------------------------

#[test]
fn part_d_bound1_a_stray_close_bracket_ends_the_wikilink_capture() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "Target.md", "# Target\n");
    write(&tmp, "src.md", "see [[Leah] here and [[Target]]\n");
    let links = links_of(&tmp, "src.md");
    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].0, "Target");
    assert_eq!(links[0].1.as_deref(), Some("Target.md"));
}

#[test]
fn part_d_bound2_a_flow_list_opening_with_three_brackets_yields_both_links() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "iterations/x.md", "# X\n");
    write(&tmp, "research/y.md", "# Y\n");
    write(
        &tmp,
        "src.md",
        "---\ntitle: S\nrelated: [[[iterations/x]], [[research/y]]]\n---\n\nBody.\n",
    );
    let links = links_of(&tmp, "src.md");
    assert_eq!(
        links.iter().map(|l| l.0.as_str()).collect::<Vec<_>>(),
        vec!["iterations/x", "research/y"],
        "{links:?}"
    );
    assert!(links.iter().all(|l| l.1.is_some()), "{links:?}");
}

#[test]
fn part_d_bound3_a_parenthesised_url_in_an_angle_destination_is_external() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "roundup.md",
        "# Roundup\n\nSee [y](<(https://example.com/2021)>) for the list.\n",
    );
    let links = links_of(&tmp, "roundup.md");
    assert_eq!(links.len(), 1, "{links:?}");
    assert_eq!(links[0].2, "external", "{links:?}");

    // …and it is never offered as a fix.
    let out = run_json(&tmp, &["links", "fix", "--apply-fuzzy", "--dry-run"]);
    assert_eq!(out["results"]["broken"].as_u64(), Some(0));
    assert_eq!(out["results"]["fuzzy"].as_u64(), Some(0));
}

// ---------------------------------------------------------------------------
// Part F — dry-run reports the string apply writes
// ---------------------------------------------------------------------------

#[test]
fn part_f_dry_run_reports_the_emitted_target_apply_writes() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "sub/Real-Page.md", "# Real page\n");
    write(&tmp, "sub/src.md", "See [x](real-page.md).\n");

    let dry = run_json(&tmp, &["links", "fix", "--dry-run"]);
    let plan = &dry["results"]["case_mismatch_fixes"][0];
    // `new_target` stays vault-relative for consumers that resolve paths…
    assert_eq!(plan["new_target"].as_str(), Some("sub/Real-Page.md"));
    // …while `emitted_target` is the text that lands on disk.
    let emitted = plan["emitted_target"]
        .as_str()
        .expect("emitted_target reported")
        .to_owned();
    assert_eq!(emitted, "Real-Page.md");

    let applied = run_json(&tmp, &["links", "fix", "--apply"]);
    assert_eq!(
        applied["results"]["applied_fixes"][0]["emitted_target"].as_str(),
        Some(emitted.as_str()),
        "apply must report the same string the dry run promised"
    );
    let written = std::fs::read_to_string(tmp.path().join("sub/src.md")).unwrap();
    assert!(
        written.contains(&format!("({emitted})")),
        "the reported string is what was written: {written}"
    );
}
