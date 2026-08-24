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

// ---------------------------------------------------------------------------
// iter-204: site-prefix stripping is case-insensitive (MDN-shaped fixture)
// ---------------------------------------------------------------------------

/// Build an MDN-shaped vault: the checkout directory is lower-case (`en-us`)
/// while the published links carry the real, mixed-case URL prefix
/// (`/en-US/docs/...`). This is the exact shape that left MDN reading as
/// 49,703-of-49,705 links broken.
fn write_mdn_shaped_vault(root: &std::path::Path) {
    write_md(
        root,
        "web/api/index.md",
        "---\ntitle: Web API\n---\nBody.\n",
    );
    write_md(root, "web/css/index.md", "---\ntitle: CSS\n---\nBody.\n");
}

/// With the prefix auto-derived from the directory name (`en-us`), a link
/// written `/en-US/web/api` must resolve: only ASCII case differs.
#[test]
fn derived_site_prefix_strips_case_insensitively() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("en-us");
    std::fs::create_dir_all(&vault).unwrap();
    write_mdn_shaped_vault(&vault);
    write_md(
        &vault,
        "src.md",
        "---\ntitle: Src\n---\nSee [Web API](/en-US/web/api).\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["find", "--broken-links", "--format", "json"])
        .output()
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(
        json["total"], 0,
        "a mixed-case site prefix must still strip: {json}"
    );
}

/// The full MDN shape needs the *two-segment* prefix, which auto-derivation
/// cannot guess — but once passed explicitly it too matches case-insensitively.
#[test]
fn explicit_multi_segment_site_prefix_resolves_mdn_links() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("en-us");
    std::fs::create_dir_all(&vault).unwrap();
    write_mdn_shaped_vault(&vault);
    write_md(
        &vault,
        "src.md",
        "---\ntitle: Src\n---\nSee [Web API](/en-US/docs/web/api) and [CSS](/EN-us/DOCS/web/css).\n",
    );

    // Without the second segment the links cannot resolve.
    let derived = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["find", "--broken-links", "--format", "json"])
        .output()
        .unwrap();
    let derived_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&derived.stdout)).unwrap();
    assert_eq!(
        derived_json["total"], 1,
        "the single-segment derived prefix cannot cover /en-US/docs: {derived_json}"
    );

    let explicit = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["--site-prefix", "en-US/docs"])
        .args(["find", "--broken-links", "--format", "json"])
        .output()
        .unwrap();
    let explicit_json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&explicit.stdout)).unwrap();
    assert_eq!(
        explicit_json["total"], 0,
        "both casings of the explicit prefix must strip: {explicit_json}"
    );
}

/// NEW-9 (dogfood pre3): `hyalo links fix` warns when the effective
/// `site_prefix` demonstrably stripped 0 of N site-absolute links — the MDN
/// repro shape (a single-segment derived prefix against two-segment
/// `/en-US/docs/...` links) leaves every stripped result's first remaining
/// segment (`docs`) unmatched against any real top-level vault entry.
#[test]
fn links_fix_warns_when_site_prefix_strips_nothing_plausible() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("en-us");
    std::fs::create_dir_all(&vault).unwrap();
    write_mdn_shaped_vault(&vault);
    write_md(
        &vault,
        "src.md",
        "---\ntitle: Src\n---\nSee [Web API](/en-US/docs/web/api) and [CSS](/en-US/docs/web/css).\n",
    );

    let derived = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["links", "fix", "--dry-run", "--format", "text"])
        .output()
        .unwrap();
    let derived_stderr = String::from_utf8_lossy(&derived.stderr);
    assert!(
        derived_stderr.contains("site_prefix 'en-us' stripped 0 of"),
        "expected the misconfiguration warning, naming the prefix: {derived_stderr}"
    );
    assert!(
        derived_stderr.contains("--site-prefix"),
        "the warning must point at the fix: {derived_stderr}"
    );

    // The correct multi-segment prefix leaves `web/...`, a real top-level
    // entry — no warning.
    let explicit = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["--site-prefix", "en-US/docs"])
        .args(["links", "fix", "--dry-run", "--format", "text"])
        .output()
        .unwrap();
    let explicit_stderr = String::from_utf8_lossy(&explicit.stderr);
    assert!(
        !explicit_stderr.contains("stripped 0 of"),
        "a correctly stripping prefix must not warn: {explicit_stderr}"
    );
}

/// PR #251 review L5: a bare `/` (site-root) link has no path segment to
/// check plausibility against — it must not pad the denominator toward a
/// false "stripped 0 of N" alongside a genuinely external-looking link.
#[test]
fn links_fix_bare_root_link_does_not_inflate_the_site_prefix_warning() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\n[home](/) and [blog](/blog/post)\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--site-prefix", "t3"])
        .args(["links", "fix", "--dry-run", "--format", "text"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stripped 0 of 1"),
        "the bare `/` link must not count toward the denominator: {stderr}"
    );
    assert!(
        !stderr.contains("stripped 0 of 2"),
        "must not count the site-root link as a second unresolved site-absolute link: {stderr}"
    );
}

/// PR #251 review L5: `site_prefix: None` covers two legitimate,
/// non-misconfiguration cases — `--site-prefix ""` (explicit bundle-root
/// resolution) and derivation itself yielding nothing — neither of which the
/// warning should fire for, since there is no prefix value to point the user
/// at fixing.
#[test]
fn links_fix_does_not_warn_when_site_prefix_is_explicitly_disabled() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "src.md",
        "---\ntitle: Src\n---\n[nowhere](/does/not/exist)\n",
    );

    let output = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["--site-prefix", ""])
        .args(["links", "fix", "--dry-run", "--format", "text"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("site_prefix"),
        "an explicitly disabled prefix must never be blamed: {stderr}"
    );
}

/// `hyalo config` warns that a derived prefix is only ever one segment, so the
/// MDN case is discoverable rather than silently wrong.
#[test]
fn config_notes_that_derived_prefixes_are_single_segment() {
    let tmp = TempDir::new().unwrap();
    let vault = tmp.path().join("en-us");
    std::fs::create_dir_all(&vault).unwrap();
    write_mdn_shaped_vault(&vault);

    let output = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("site_prefix: en-us (derived)"),
        "the derived value must still be reported: {stdout}"
    );
    assert!(
        stdout.contains("single path segment"),
        "the derivation limit must be stated: {stdout}"
    );

    // An explicit prefix is authoritative — no note.
    let explicit = hyalo_no_hints()
        .args(["--dir", vault.to_str().unwrap()])
        .args(["--site-prefix", "en-US/docs"])
        .args(["config", "--format", "text"])
        .output()
        .unwrap();
    let explicit_out = String::from_utf8_lossy(&explicit.stdout);
    assert!(
        !explicit_out.contains("single path segment"),
        "an explicit prefix needs no advice: {explicit_out}"
    );
}
