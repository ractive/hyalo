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
