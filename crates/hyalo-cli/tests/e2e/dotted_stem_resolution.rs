//! F3 (iteration 306 review): a path-form wikilink whose stem contains a dot
//! that is not a `.md` suffix (`[[sub/rel-1.2]]`) was reported unresolved
//! even though `sub/rel-1.2.md` exists, because the trailing `.2` was
//! mistaken for a real, non-`.md` file extension once a directory segment
//! was present. The bare form `[[rel-1.2]]` already worked, since it took a
//! different (stem-lookup) resolution path that never ran the extension
//! check.
//!
//! DEC-318 says `find`/`summary`/`links fix`/HYALO006/`backlinks` share one
//! edge predicate; this file pins that all five agree on the fixed
//! resolution, and that a genuinely unresolved non-`.md` attachment target
//! keeps reporting as an (unresolved) attachment rather than a broken note
//! link.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

fn run_json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let dir = tmp.path().to_str().expect("utf-8 path");
    let mut cmd_args = vec!["--dir", dir];
    cmd_args.extend_from_slice(args);
    cmd_args.extend_from_slice(&["--format", "json"]);
    let output = hyalo_no_hints()
        .args(&cmd_args)
        .output()
        .expect("hyalo should run");
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout should be JSON for {cmd_args:?}: {e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn vault_with_dotted_stem_link() -> TempDir {
    let tmp = TempDir::new().expect("tempdir");
    write_md(
        tmp.path(),
        "a.md",
        "# A\n\nSee [[sub/rel-1.2]] for details.\n",
    );
    write_md(tmp.path(), "sub/rel-1.2.md", "# Rel 1.2\n");
    tmp
}

#[test]
fn find_fields_links_resolves_dotted_path_stem() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(&tmp, &["find", "--file", "a.md", "--fields", "links"]);
    let link = &json["results"][0]["links"][0];
    assert_eq!(link["path"], "sub/rel-1.2.md", "full json: {json}");
    assert_eq!(link["target"], "sub/rel-1.2");
}

#[test]
fn broken_links_does_not_flag_the_dotted_stem() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(&tmp, &["find", "--broken-links"]);
    assert_eq!(
        json["total"], 0,
        "sub/rel-1.2.md exists, so a.md must not be reported as having a broken link: {json}"
    );
}

#[test]
fn hyalo006_does_not_flag_the_dotted_stem() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(&tmp, &["lint", "--rule", "HYALO006"]);
    assert_eq!(json["results"]["violations"], 0, "full json: {json}");
    assert_eq!(
        json["results"]["files_with_violations"], 0,
        "full json: {json}"
    );
}

#[test]
fn summary_reports_zero_broken_links_for_the_dotted_stem() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(&tmp, &["summary"]);
    assert_eq!(json["results"]["links"]["broken"], 0, "full json: {json}");
    assert_eq!(json["results"]["links"]["total"], 1, "full json: {json}");
}

#[test]
fn links_fix_reports_zero_broken_for_the_dotted_stem() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(&tmp, &["links", "fix"]);
    assert_eq!(json["results"]["broken"], 0, "full json: {json}");
}

#[test]
fn backlinks_sees_the_dotted_stem_inbound_link() {
    let tmp = vault_with_dotted_stem_link();
    let json = run_json(
        &tmp,
        &["find", "--file", "sub/rel-1.2.md", "--fields", "backlinks"],
    );
    let backlinks = json["results"][0]["backlinks"]
        .as_array()
        .unwrap_or_else(|| panic!("backlinks should be an array: {json}"));
    assert_eq!(backlinks.len(), 1, "full json: {json}");
    assert_eq!(backlinks[0]["source"], "a.md");
}

/// Control: a target that genuinely has no matching `.md` note anywhere and
/// carries a real (non-`.md`) extension must keep resolving to an
/// unresolved attachment rather than becoming a false-positive note match
/// now that the dotted-stem fallback always tries the `.md` suffix.
#[test]
fn genuinely_missing_attachment_stays_unresolved() {
    let tmp = TempDir::new().expect("tempdir");
    write_md(tmp.path(), "a.md", "# A\n\n![[sub/missing-diagram.png]]\n");
    let json = run_json(&tmp, &["find", "--file", "a.md", "--fields", "links"]);
    let link = &json["results"][0]["links"][0];
    assert_eq!(link["kind"], "embed", "full json: {json}");
    assert_eq!(link["path"], serde_json::Value::Null, "full json: {json}");
}
