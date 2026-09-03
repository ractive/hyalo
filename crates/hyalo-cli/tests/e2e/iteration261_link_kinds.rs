//! Iteration 261 — link resolution agrees with Obsidian.
//!
//! Four dogfood findings from two real Obsidian vaults, plus the reporting
//! affordance they all needed:
//!
//! - **BUG-2.** Any `scheme:` target (`obsidian://`, `mailto:`, `file://`) is
//!   external, exactly like `https://` — 2897 `obsidian://show-plugin` links
//!   counted as broken on the Obsidian Hub vault.
//! - **BUG-5 / BUG-6.** A target with an explicit non-`.md` extension resolves
//!   against every vault file the way Obsidian's shortest-path setting does,
//!   and is classified as an `attachment` rather than reported broken.
//! - **BUG-7.** `[[target\|alias]]` — the form Obsidian writes inside a table —
//!   splits at the escaped pipe, and a rewrite keeps the `\|` bytes.
//! - **BUG-10 / DEC-267.** Resolution folds case for every consumer, not only
//!   under `links fix --case-insensitive`.
//! - **UX-6.** Every `--fields links` entry carries a `kind`.

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

/// Every link of one file, as `(target, kind, path)` triples in document order.
fn links_of(tmp: &TempDir, file: &str) -> Vec<(String, String, Option<String>)> {
    let json = run_json(tmp, &["find", "--file", file, "--fields", "links"]);
    json["results"][0]["links"]
        .as_array()
        .expect("links array")
        .iter()
        .map(|l| {
            (
                l["target"].as_str().unwrap_or_default().to_owned(),
                l["kind"].as_str().unwrap_or_default().to_owned(),
                l["path"].as_str().map(str::to_owned),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// BUG-2 — external URI schemes
// ---------------------------------------------------------------------------

#[test]
fn obsidian_uris_are_external_not_broken() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "note.md",
        "---\ntitle: Note\n---\n\n[Install](obsidian://show-plugin?id=dataview)\n",
    );

    // `summary` counts nothing broken …
    let summary = run_json(&tmp, &["summary"]);
    assert_eq!(summary["results"]["links"]["broken"], 0);

    // … `lint` emits no HYALO006 …
    let lint = run_json(&tmp, &["lint", "--rule", "HYALO006"]);
    assert_eq!(lint["results"]["violations"], 0);

    // … `find --broken-links` matches no file …
    let broken = run_json(&tmp, &["find", "--broken-links"]);
    assert_eq!(broken["total"], 0);

    // … and `links fix` lists nothing under `unfixable`.
    let fix = run_json(&tmp, &["links", "fix"]);
    assert_eq!(fix["results"]["unfixable"], 0);
    assert_eq!(fix["results"]["broken"], 0);

    // The URI is still inventoried, verbatim (no `?query` truncation) and
    // labelled — that is the whole point of `kind`.
    let links = links_of(&tmp, "note.md");
    assert_eq!(
        links,
        vec![(
            "obsidian://show-plugin?id=dataview".to_owned(),
            "external".to_owned(),
            None
        )]
    );
}

// ---------------------------------------------------------------------------
// BUG-5 / BUG-6 — attachments
// ---------------------------------------------------------------------------

/// A vault shaped like the Obsidian Hub: attachments in their own folder, a
/// `.base` under `Templates/`, and links written from three different folders.
fn attachment_vault() -> TempDir {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "Templates/Bases/Books.base", "filters: []\n");
    write(&tmp, "02 Attachments/x.png", "not really a png");
    write(&tmp, "notes/deep/y.png", "not really a png either");
    write(
        &tmp,
        "root.md",
        "---\ntitle: Root\n---\n\n![[x.png]]\n\n[[Books.base]]\n",
    );
    write(
        &tmp,
        "notes/mid.md",
        "---\ntitle: Mid\n---\n\n![[deep/y.png]]\n\n[[Templates/Bases/Books.base]]\n",
    );
    write(
        &tmp,
        "notes/deep/leaf.md",
        "---\ntitle: Leaf\n---\n\n![[x.png]]\n\n[[templates/bases/books.BASE]]\n",
    );
    tmp
}

#[test]
fn attachments_resolve_from_every_folder_and_are_never_broken() {
    let tmp = attachment_vault();

    assert_eq!(
        links_of(&tmp, "root.md"),
        vec![
            (
                "x.png".to_owned(),
                "attachment".to_owned(),
                Some("02 Attachments/x.png".to_owned())
            ),
            (
                "Books.base".to_owned(),
                "attachment".to_owned(),
                Some("Templates/Bases/Books.base".to_owned())
            ),
        ]
    );
    // A partially-qualified embed resolves relative to the source folder.
    assert_eq!(
        links_of(&tmp, "notes/mid.md")[0],
        (
            "deep/y.png".to_owned(),
            "attachment".to_owned(),
            Some("notes/deep/y.png".to_owned())
        )
    );
    // …and a case-folded full path resolves too (DEC-267).
    assert_eq!(
        links_of(&tmp, "notes/deep/leaf.md")[1],
        (
            "templates/bases/books.BASE".to_owned(),
            "attachment".to_owned(),
            Some("Templates/Bases/Books.base".to_owned())
        )
    );

    assert_eq!(run_json(&tmp, &["summary"])["results"]["links"]["broken"], 0);
    assert_eq!(
        run_json(&tmp, &["lint", "--rule", "HYALO006"])["results"]["violations"],
        0
    );
    assert_eq!(run_json(&tmp, &["find", "--broken-links"])["total"], 0);
}

#[test]
fn links_fix_never_proposes_a_base_to_md_rewrite() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "Categories/Posts.md", "---\ntitle: Posts\n---\n");
    write(
        &tmp,
        "note.md",
        "---\ntitle: Note\n---\n\n[[Posts.base]]\n[[Companies.base]]\n",
    );

    let fix = run_json(&tmp, &["links", "fix", "--min-confidence", "0"]);
    let proposed: Vec<&str> = fix["results"]["fuzzy_fixes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["new_target"].as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        proposed.is_empty(),
        "a `.base` target must never be matched against a `.md` note: {proposed:?}"
    );
    // No candidate anywhere may be reported at confidence 0.0 (UX-8).
    for bucket in ["fuzzy_fixes", "fixes"] {
        for fix in fix["results"][bucket].as_array().unwrap_or(&Vec::new()) {
            if let Some(c) = fix["confidence"].as_f64() {
                assert!(c > 0.0, "confidence 0.0 candidate reported: {fix}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// BUG-7 — table-escaped alias pipe
// ---------------------------------------------------------------------------

#[test]
fn escaped_alias_pipe_resolves_and_survives_a_move() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "obsidian-advanced-uri.md", "---\ntitle: URI\n---\n");
    write(
        &tmp,
        "table.md",
        "---\ntitle: Table\n---\n\n| Plugin | Note |\n| --- | --- |\n\
         | [[obsidian-advanced-uri\\|Advanced URI Plugin]] | yes |\n",
    );

    let links = links_of(&tmp, "table.md");
    assert_eq!(
        links,
        vec![(
            "obsidian-advanced-uri".to_owned(),
            "wikilink".to_owned(),
            Some("obsidian-advanced-uri.md".to_owned())
        )],
        "the trailing backslash is part of the alias escape, not the target"
    );

    // `mv` rewrites the target and leaves the `\|alias` — and therefore the
    // table row — byte-for-byte intact.
    let out = hyalo(&tmp)
        .args([
            "mv",
            "obsidian-advanced-uri.md",
            "--to",
            "plugins/advanced-uri.md",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let row = std::fs::read_to_string(tmp.path().join("table.md")).unwrap();
    assert!(
        row.contains("| [[advanced-uri\\|Advanced URI Plugin]] | yes |"),
        "the table row must stay a table row and keep the `\\|` escape \
         (the bare written form is preserved, so the target stays a stem), got:\n{row}"
    );
}

// ---------------------------------------------------------------------------
// BUG-10 / DEC-267 — case-insensitive resolution everywhere
// ---------------------------------------------------------------------------

#[test]
fn case_only_mismatches_resolve_for_every_consumer() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "People/aidenlx.md", "---\ntitle: AidenLx\n---\n");
    write(
        &tmp,
        "note.md",
        "---\ntitle: Note\n---\n\n[[AidenLx]] and [[people/AidenLX]]\n",
    );

    assert_eq!(run_json(&tmp, &["find", "--broken-links"])["total"], 0);
    assert_eq!(run_json(&tmp, &["summary"])["results"]["links"]["broken"], 0);
    assert_eq!(
        run_json(&tmp, &["lint", "--rule", "HYALO006"])["results"]["violations"],
        0
    );
    for (_, _, path) in links_of(&tmp, "note.md") {
        assert_eq!(path.as_deref(), Some("People/aidenlx.md"));
    }
}

// ---------------------------------------------------------------------------
// UX-6 — every link carries a kind
// ---------------------------------------------------------------------------

#[test]
fn all_five_link_kinds_are_reported() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "target.md", "---\ntitle: Target\n---\n");
    write(&tmp, "img.png", "not really a png");
    write(
        &tmp,
        "note.md",
        "---\ntitle: Note\n---\n\n[[target]]\n![[target]]\n[md](target.md)\n\
         [ext](https://example.invalid)\n![[img.png]]\n",
    );

    let kinds: Vec<String> = links_of(&tmp, "note.md")
        .into_iter()
        .map(|(_, kind, _)| kind)
        .collect();
    assert_eq!(
        kinds,
        vec!["wikilink", "embed", "markdown", "external", "attachment"]
    );

    // `has("kind")` on the first link of an arbitrary query — the shape
    // contract a caller can rely on.
    let json = run_json(&tmp, &["find", "--fields", "links", "--limit", "1"]);
    assert!(json["results"][0]["links"][0].get("kind").is_some());
}

#[test]
fn an_attachment_only_note_is_still_a_dead_end() {
    let tmp = TempDir::new().unwrap();
    write(&tmp, "img.png", "not really a png");
    write(&tmp, "hub.md", "---\ntitle: Hub\n---\n\n[[leaf]]\n");
    write(
        &tmp,
        "leaf.md",
        "---\ntitle: Leaf\n---\n\n![[img.png]] and [x](https://example.invalid)\n",
    );

    let dead = run_json(&tmp, &["find", "--dead-end", "--fields", "file"]);
    let files: Vec<&str> = dead["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["file"].as_str())
        .collect();
    assert_eq!(
        files,
        vec!["leaf.md"],
        "an attachment embed and an external URI are not outbound note edges"
    );
}

// ---------------------------------------------------------------------------
// DEC-268 — unique-prefix anchor suggestion
// ---------------------------------------------------------------------------

#[test]
fn a_dead_anchor_with_one_prefix_match_suggests_the_full_heading() {
    let tmp = TempDir::new().unwrap();
    write(
        &tmp,
        "decision-log.md",
        "---\ntitle: Decisions\n---\n\n## DEC-068: Snapshot index format\n\n\
         ## DEC-070: Something else\n\n## DEC-08: first\n\n## DEC-08: second\n",
    );
    write(
        &tmp,
        "note.md",
        "---\ntitle: Note\n---\n\n[[decision-log#DEC-068]]\n[[decision-log#DEC-08]]\n",
    );

    // Vault-wide, not `--file note.md`: the anchor check needs the *target*
    // file's headings, which a single-file scope does not load.
    let json = run_json(&tmp, &["find", "--broken-links", "--fields", "links"]);
    let links = json["results"][0]["links"].as_array().unwrap();
    assert_eq!(json["results"][0]["file"], "note.md");
    assert_eq!(links[0]["broken_anchor"], true);
    assert_eq!(
        links[0]["suggested_fragment"],
        "DEC-068: Snapshot index format"
    );
    // Two headings share the `DEC-08` prefix, so there is nothing to suggest.
    assert_eq!(links[1]["broken_anchor"], true);
    assert!(links[1].get("suggested_fragment").is_none());
}

// ---------------------------------------------------------------------------
// The `--index` path answers exactly like the disk path
// ---------------------------------------------------------------------------

#[test]
fn attachments_round_trip_through_the_index_snapshot() {
    let tmp = attachment_vault();
    let out = hyalo(&tmp).arg("create-index").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let json = run_json(
        &tmp,
        &["find", "--file", "root.md", "--fields", "links", "--index"],
    );
    let links = json["results"][0]["links"].as_array().unwrap();
    assert_eq!(links[0]["path"], "02 Attachments/x.png");
    assert_eq!(links[0]["kind"], "attachment");
    assert_eq!(links[1]["path"], "Templates/Bases/Books.base");

    let summary = run_json(&tmp, &["summary", "--index"]);
    assert_eq!(summary["results"]["links"]["broken"], 0);
}

#[test]
fn an_index_without_attachments_still_loads() {
    // A snapshot written before iter-261 has no `attachments` key at all. The
    // `#[serde(default)]` on the header field is what keeps it loading; the
    // attachment links simply do not resolve on that stale index, exactly as
    // they did not before.
    let tmp = attachment_vault();
    let out = hyalo(&tmp).arg("create-index").output().unwrap();
    assert!(out.status.success());
    // A mutation re-saves the snapshot through `save_to`, which must carry the
    // attachment list forward rather than dropping it.
    let out = hyalo(&tmp)
        .args(["set", "root.md", "--property", "status=done", "--index"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let json = run_json(
        &tmp,
        &["find", "--file", "root.md", "--fields", "links", "--index"],
    );
    assert_eq!(
        json["results"][0]["links"][0]["path"],
        "02 Attachments/x.png",
        "a re-saved snapshot must keep its attachment list"
    );
}
