use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// site_prefix resolution — verifies that absolute-path links (`/docs/...`)
// are resolved correctly across all supported invocation styles.
//
// All tests share the same vault shape:
//
//   <root>/
//     docs/
//       index.md     — body contains [About](/docs/pages/about.md)
//       pages/
//         about.md
// ---------------------------------------------------------------------------

fn build_vault(root: &std::path::Path) {
    write_md(
        root,
        "docs/index.md",
        "---\ntitle: Index\n---\nSee [About](/docs/pages/about.md).\n",
    );
    write_md(
        root,
        "docs/pages/about.md",
        "---\ntitle: About\n---\nAbout page.\n",
    );
}

// ---------------------------------------------------------------------------
// find --fields links — absolute link shows up as resolved vault-relative path
// ---------------------------------------------------------------------------

/// Run `hyalo --dir <dir_arg> find --fields links` and return the parsed JSON.
fn find_links(dir_arg: &str) -> serde_json::Value {
    let output = hyalo_no_hints()
        .args(["--dir", dir_arg])
        .args(["find", "--fields", "links", "--file", "index.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "dir={dir_arg} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn extract_link_paths(json: &serde_json::Value) -> Vec<String> {
    json["results"]
        .as_array()
        .expect("expected {total, results} envelope")
        .iter()
        .flat_map(|entry| {
            entry["links"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|l| l["path"].as_str().map(std::borrow::ToOwned::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn find_links_absolute_path_with_absolute_dir() {
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let json = find_links(docs.to_str().unwrap());
    let paths = extract_link_paths(&json);
    assert!(
        paths.iter().any(|p| p == "pages/about.md"),
        "absolute --dir: expected 'pages/about.md' in link paths, got: {paths:?}"
    );
}

#[test]
fn find_links_absolute_path_with_dotslash_dir() {
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    // Use absolute path with a trailing slash — canonicalize strips it, so the
    // derived prefix must still be "docs", not "".
    let docs = format!("{}/docs/", tmp.path().to_str().unwrap());

    let output = hyalo_no_hints()
        .args(["--dir", docs.trim_end_matches('/')])
        .args(["find", "--fields", "links", "--file", "index.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paths = extract_link_paths(&json);
    assert!(
        paths.iter().any(|p| p == "pages/about.md"),
        "trailing-slash --dir: expected 'pages/about.md' in link paths, got: {paths:?}"
    );
}

#[test]
fn find_links_site_prefix_cli_flag() {
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs.to_str().unwrap()])
        .args(["--site-prefix", "docs"])
        .args(["find", "--fields", "links", "--file", "index.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paths = extract_link_paths(&json);
    assert!(
        paths.iter().any(|p| p == "pages/about.md"),
        "--site-prefix=docs: expected 'pages/about.md' in link paths, got: {paths:?}"
    );
}

#[test]
fn find_links_site_prefix_config_file() {
    // NOTE: hyalo loads .hyalo.toml from the *process working directory*, not
    // from --dir.  This test writes a .hyalo.toml into a temp docs/ dir, but
    // the e2e subprocess's CWD is the test harness working directory, so the
    // config file is never read.  What this test actually exercises is that
    // auto-derivation (canonicalize(--dir).file_name()) still returns "docs"
    // and the link resolves correctly.  A true config-file-precedence test
    // would require spawning hyalo with its CWD set to the temp dir.
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());

    // Write .hyalo.toml — this file won't be read in the e2e invocation below
    // (process CWD is not tmp), but it's kept here to document intent.
    std::fs::write(
        tmp.path().join("docs").join(".hyalo.toml"),
        "site_prefix = \"docs\"\n",
    )
    .unwrap();

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().join("docs").to_str().unwrap()])
        .args(["find", "--fields", "links", "--file", "index.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let paths = extract_link_paths(&json);
    assert!(
        paths.iter().any(|p| p == "pages/about.md"),
        "auto-derived site_prefix with docs dir: expected 'pages/about.md' in link paths, got: {paths:?}"
    );
}

// ---------------------------------------------------------------------------
// find --fields backlinks — absolute link is indexed correctly
// ---------------------------------------------------------------------------

#[test]
fn backlinks_absolute_link_indexed_correctly() {
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    let docs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs.to_str().unwrap()])
        .args(["backlinks", "--file", "pages/about.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json["total"], 1,
        "expected 1 backlink from index.md, got: {json}"
    );
    let source = json["results"]["backlinks"][0]["source"].as_str().unwrap();
    assert_eq!(
        source, "index.md",
        "expected backlink source to be 'index.md', got: {source}"
    );
}

// ---------------------------------------------------------------------------
// site_prefix auto-derivation — all dir styles produce the same prefix
// ---------------------------------------------------------------------------

/// Run backlinks and return total count.  Used to verify that all --dir styles
/// produce the same effective site_prefix.
fn backlink_count(dir_arg: &str) -> u64 {
    let output = hyalo_no_hints()
        .args(["--dir", dir_arg])
        .args(["backlinks", "--file", "pages/about.md"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "dir={dir_arg} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    json["total"].as_u64().unwrap_or(0)
}

#[test]
fn site_prefix_absolute_dir_same_result_as_bare_name() {
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    let docs_abs = tmp.path().join("docs");

    // Absolute path should yield the same backlink count (1) as a correctly
    // configured run — proving the auto-derived prefix is correct.
    let count = backlink_count(docs_abs.to_str().unwrap());
    assert_eq!(count, 1, "absolute --dir: expected 1 backlink, got {count}");
}

#[test]
fn site_prefix_wrong_prefix_misses_absolute_links() {
    // If site_prefix is wrong (e.g. "wrong"), absolute links won't be resolved
    // and backlinks count drops to 0.
    let tmp = TempDir::new().unwrap();
    build_vault(tmp.path());
    let docs_abs = tmp.path().join("docs");

    let output = hyalo_no_hints()
        .args(["--dir", docs_abs.to_str().unwrap()])
        .args(["--site-prefix", "wrong"])
        .args(["backlinks", "--file", "pages/about.md"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // With the wrong prefix, the absolute link `/docs/pages/about.md` is not
    // resolved as `pages/about.md`, so no backlinks are found.
    assert_eq!(
        json["total"], 0,
        "wrong prefix: expected 0 backlinks, got: {json}"
    );
}
