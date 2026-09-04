//! ARCH-3 (iter-226): every mutating command must refresh the persisted
//! snapshot index — entries AND link graph — through the single
//! `MutationJournal`. These e2e tests pin the stale-link-graph regression
//! class recorded at `hyalo-core/src/index.rs` (`refresh_links` doc): a
//! frontmatter link property mutated with `--index` used to leave the
//! persisted `LinkGraph` stale, so `backlinks`/`find --fields links` kept
//! returning pre-mutation results until a full `create-index` rebuild.
//!
//! Each test mutates via a *different* journal path (`set`, `append`,
//! `remove`, `properties rename`, `mv`) and then asserts the persisted
//! graph is current by querying with `--index` (which reads only the
//! snapshot — never the disk).

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

fn hyalo_no_hints() -> Command {
    crate::common::hyalo_no_hints()
}

fn setup() -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\ntags: [a]\n---\n\n# Alpha\n\nBody.\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\ntags: [a]\n---\n\n# Beta\n\nBody.\n",
    )
    .unwrap();
    tmp
}

fn run(tmp: &TempDir, args: &[&str]) -> (std::process::Output, serde_json::Value) {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout not JSON: {e}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    (output, json)
}

fn create_index(tmp: &TempDir) {
    run(tmp, &["create-index"]);
    assert!(tmp.path().join(".hyalo-index").exists());
}

/// Assert `target`'s persisted backlink sources (queried via `--index`,
/// snapshot-only) equal `expected`.
fn assert_backlink_sources(tmp: &TempDir, target: &str, expected: &[&str]) {
    let (_, json) = run(tmp, &["backlinks", "--file", target, "--index"]);
    let sources: Vec<String> = json["results"]["backlinks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["source"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        sources, expected,
        "persisted link graph is stale for {target}"
    );
}

// ---------------------------------------------------------------------------
// set / append / remove — frontmatter link-property mutations
// ---------------------------------------------------------------------------

#[test]
fn set_link_property_updates_persisted_graph() {
    let tmp = setup();
    create_index(&tmp);
    run(
        &tmp,
        &[
            "set",
            "--property",
            "related=[[beta]]",
            "--file",
            "alpha.md",
            "--index",
        ],
    );
    assert_backlink_sources(&tmp, "beta.md", &["alpha.md"]);
}

#[test]
fn append_link_property_updates_persisted_graph() {
    let tmp = setup();
    create_index(&tmp);
    run(
        &tmp,
        &[
            "append",
            "--property",
            "depends-on=[[beta]]",
            "--file",
            "alpha.md",
            "--index",
        ],
    );
    assert_backlink_sources(&tmp, "beta.md", &["alpha.md"]);
}

#[test]
fn remove_link_property_updates_persisted_graph() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\nrelated: '[[beta]]'\n---\n\n# Alpha\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n",
    )
    .unwrap();
    create_index(&tmp);
    assert_backlink_sources(&tmp, "beta.md", &["alpha.md"]);
    run(
        &tmp,
        &[
            "remove",
            "--property",
            "related",
            "--file",
            "alpha.md",
            "--index",
        ],
    );
    assert_backlink_sources(&tmp, "beta.md", &[]);
}

// ---------------------------------------------------------------------------
// properties rename / tags rename — the two commands whose pre-journal
// write path only patched entries, never the graph
// ---------------------------------------------------------------------------

#[test]
fn properties_rename_updates_persisted_graph() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\nrelated: '[[beta]]'\n---\n\n# Alpha\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n",
    )
    .unwrap();
    create_index(&tmp);
    run(
        &tmp,
        &[
            "properties",
            "rename",
            "--from",
            "related",
            "--to",
            "depends-on",
            "--glob",
            "**/*.md",
            "--index",
        ],
    );
    // `depends-on` is also a frontmatter link property: the edge survives
    // the rename in the persisted graph.
    assert_backlink_sources(&tmp, "beta.md", &["alpha.md"]);
}

/// Same bug class for `tags rename`: renaming a tag that is also a
/// frontmatter link property is not possible (tags are a list), but the
/// entry-only patch path is shared — pin that the graph survives a tags
/// rename that rewrites frontmatter.
#[test]
fn tags_rename_updates_persisted_graph() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\ntags: [oldtag]\nrelated: '[[beta]]'\n---\n\n# Alpha\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n",
    )
    .unwrap();
    create_index(&tmp);
    run(
        &tmp,
        &[
            "tags", "rename", "--from", "oldtag", "--to", "newtag", "--glob", "**/*.md", "--index",
        ],
    );
    // The tags rename rewrote alpha.md's frontmatter (tags) — the
    // `related: '[[beta]]'` edge must survive in the persisted graph.
    assert_backlink_sources(&tmp, "beta.md", &["alpha.md"]);
}

// ---------------------------------------------------------------------------
// mv — rename path
// ---------------------------------------------------------------------------

#[test]
fn mv_updates_persisted_graph() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\nrelated: '[[beta]]'\n---\n\n# Alpha\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n",
    )
    .unwrap();
    create_index(&tmp);
    run(&tmp, &["mv", "alpha.md", "moved.md", "--index"]);
    assert_backlink_sources(&tmp, "beta.md", &["moved.md"]);
}

// ---------------------------------------------------------------------------
// task toggle — entry (task status) refresh through the journal
// ---------------------------------------------------------------------------

#[test]
fn task_toggle_updates_persisted_entry() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\n---\n\n# Alpha\n\n- [ ] todo\n",
    )
    .unwrap();
    create_index(&tmp);
    run(&tmp, &["task", "toggle", "--file", "alpha.md", "--all"]);
    // The persisted entry must now show the task done — `find --fields`
    // reads the snapshot.
    let (_, json) = run(
        &tmp,
        &["task", "read", "--file", "alpha.md", "--all", "--index"],
    );
    assert_eq!(json["results"]["done"], serde_json::json!(true));
}

// ---------------------------------------------------------------------------
// links fix --apply — body rewrite path (rescan_modified)
// ---------------------------------------------------------------------------

#[test]
fn links_fix_apply_updates_persisted_graph() {
    let tmp = tempfile::tempdir().unwrap();
    // alpha links to beta by a misspelling so `links fix` has a rewrite to
    // apply. (It used to write `[[beta.markdown]]`; since DEC-266 in iter-261
    // an explicit non-`.md` extension is an attachment reference and is never
    // matched against a `.md` note, so that spelling is deliberately
    // unfixable now — see `matcher_never_crosses_an_explicit_non_md_extension`.)
    std::fs::write(
        tmp.path().join("alpha.md"),
        "---\ntitle: Alpha\n---\n\n# Alpha\n\nSee [[betaa]].\n",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("beta.md"),
        "---\ntitle: Beta\n---\n\n# Beta\n",
    )
    .unwrap();
    create_index(&tmp);
    run(
        &tmp,
        &[
            "links",
            "fix",
            "--apply",
            "--min-confidence",
            "0",
            "--index",
        ],
    );
    // After the rewrite (beta.markdown -> beta), the persisted graph
    // must show alpha.md's outbound edge to beta.md.
    let (_, json) = run(
        &tmp,
        &["find", "--file", "alpha.md", "--fields", "links", "--index"],
    );
    let links: Vec<String> = json["results"][0]["links"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|l| l["path"].as_str().map(str::to_owned))
        .collect();
    assert!(links.contains(&"beta.md".to_string()), "links: {links:?}");
}

// ---------------------------------------------------------------------------
// guard: a `task` mutation without --index is a no-op, not a crash
// ---------------------------------------------------------------------------

#[test]
fn mutations_without_index_still_succeed() {
    let tmp = setup();
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path().to_str().unwrap())
        .args([
            "set",
            "--property",
            "related=[[beta]]",
            "--file",
            "alpha.md",
        ])
        .assert()
        .success()
        .stdout(contains("alpha.md"))
        .get_output()
        .to_owned();
    assert!(output.status.success());
}

// ---------------------------------------------------------------------------
// BUG-1: a mutation on a file the index has never seen must upsert it into
// the persisted index (entry AND link graph) instead of silently dropping
// the mutation. Repro: `create-index`, then create a new file afterwards
// and mutate it with `--index` — the pre-fix journal only patched entries
// that were already present, so `find --file ... --index` returned 0
// results even though the on-disk file had just been written.
// ---------------------------------------------------------------------------

#[test]
fn set_upserts_unindexed_file_into_persisted_index() {
    let tmp = setup();
    create_index(&tmp);
    // Created *after* create-index — the snapshot has never seen this file.
    std::fs::write(
        tmp.path().join("ext.md"),
        "---\ntitle: External\nrelated: '[[beta]]'\n---\n\n# External\n",
    )
    .unwrap();
    run(
        &tmp,
        &[
            "set",
            "--property",
            "status=done",
            "--file",
            "ext.md",
            "--index",
        ],
    );

    let (_, json) = run(
        &tmp,
        &[
            "find",
            "--file",
            "ext.md",
            "--fields",
            "properties",
            "--index",
        ],
    );
    assert_eq!(
        json["total"],
        serde_json::json!(1),
        "ext.md was not upserted into the persisted index by `set --index`: {json}"
    );
    assert_eq!(
        json["results"][0]["properties"]["status"],
        serde_json::json!("done")
    );
    // The file's frontmatter `related` link must also be registered in the
    // persisted link graph, not just the entry.
    assert_backlink_sources(&tmp, "beta.md", &["ext.md"]);
}

#[test]
fn task_toggle_upserts_unindexed_file_into_persisted_index() {
    let tmp = setup();
    create_index(&tmp);
    // Created *after* create-index — the snapshot has never seen this file.
    std::fs::write(
        tmp.path().join("ext.md"),
        "---\ntitle: External\n---\n\n# External\n\n- [ ] todo\n",
    )
    .unwrap();
    run(&tmp, &["task", "toggle", "--file", "ext.md", "--all"]);

    let (_, json) = run(
        &tmp,
        &["task", "read", "--file", "ext.md", "--all", "--index"],
    );
    assert_eq!(
        json["results"]["done"],
        serde_json::json!(true),
        "ext.md's task was not upserted into the persisted index: {json}"
    );
}
