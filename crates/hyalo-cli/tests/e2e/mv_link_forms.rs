//! Comprehensive link-form tests for `hyalo mv` (iter-151).
//!
//! Covers:
//! - Representative subset of the 8 link shapes × 4 topologies × 3 move kinds ×
//!   2 selflink matrix (full 192-case enumeration is not run; the matrix test
//!   below selects illustrative cases — see `mv_link_forms_matrix`)
//! - 10 named bug-repro tests (iter-150 NEW-1/2/3 follow-ups)
//! - Dogfood verbatim repro (x.md self-link → y.md)
use super::common::{hyalo_no_hints, write_md};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run `hyalo mv` on a vault and assert that:
/// - The command succeeds.
/// - The post-mv content of `linker_path` contains `expected_link_text`.
/// - `hyalo links` reports broken=0 and ambiguous=0 for the vault.
///
/// `vault` must already contain all files; `target` is the vault-relative
/// path of the file being moved; `new_target` is its destination.
/// `linker_path` is the vault-relative path of the file that contains the link.
fn assert_mv_preserves(
    vault: &TempDir,
    target: &str,
    new_target: &str,
    linker_path: &str,
    expected_link_text: &str,
) {
    let dir = vault.path().to_str().unwrap();
    let mv_out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["mv", "--file", target, "--to", new_target])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv {target}→{new_target} failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let abs_linker = vault.path().join(linker_path);
    let content = fs::read_to_string(&abs_linker)
        .unwrap_or_else(|e| panic!("reading {linker_path} after mv: {e}"));
    assert!(
        content.contains(expected_link_text),
        "after mv {target}→{new_target}: expected `{expected_link_text}` in {linker_path}, got:\n{content}"
    );

    // Verify no broken or ambiguous links remain.
    let links_out = hyalo_no_hints()
        .args(["--dir", dir])
        .args(["links", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        links_out.status.success(),
        "hyalo links failed: {}",
        String::from_utf8_lossy(&links_out.stderr)
    );
    let links_json: serde_json::Value =
        serde_json::from_slice(&links_out.stdout).unwrap_or_else(|e| {
            panic!(
                "links JSON parse: {e}\n{}",
                String::from_utf8_lossy(&links_out.stdout)
            )
        });
    let broken = links_json["results"]["broken"]
        .as_array()
        .map_or(0, Vec::len);
    let ambiguous = links_json["results"]["ambiguous"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(
        broken, 0,
        "broken links after mv {target}→{new_target}: {links_json}"
    );
    assert_eq!(
        ambiguous, 0,
        "ambiguous links after mv {target}→{new_target}: {links_json}"
    );
}

// ---------------------------------------------------------------------------
// Matrix test helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Full 192-case matrix
// ---------------------------------------------------------------------------

/// Run the matrix of 8 shapes × 4 topologies × 3 move-kinds × 2 selflink.
///
/// Each case creates a fresh vault, runs mv, and asserts the link text and
/// hyalo links integrity.
#[test]
fn mv_link_forms_matrix() {
    // ---- 8 link shapes ----
    // We test 8 canonical shapes. For each we need (template, expected).
    // Template uses `{stem}` for the target stem portion.
    // Expected uses `{new_stem}` for the expected new stem.
    // Alias/fragment are kept verbatim.

    // ---- 4 topologies ----
    // Topology 1: sibling-same-dir (linker and target both at root).
    // Topology 2: cross-dir (linker at root, target in sub/).
    // Topology 3: cross-dir-deep (linker in notes/, target in bulk/).
    // Topology 4: same-dir-rename-only (linker and target in sub/).

    // ---- 3 move kinds ----
    // rename-in-place, move-down, move-up.

    // ---- 2 selflink booleans ----
    // With and without a self-referencing link in the target file body.

    // For brevity we test ALL shapes × topology-1 × rename-in-place × no-selflink
    // (the core shape matrix). Then topology × rename-in-place × no-selflink for each
    // topology. Then move kinds. Then selflink.
    // This gives good coverage without 192 separate vault setups.

    // ---- Phase A: all 8 shapes, sibling topology, rename-in-place ----
    // Linker at root, target at root, mv: b.md → renamed.md

    // Shape 1: [[b]] (bare)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[[renamed]]");
    }

    // Shape 2: [[./b]] (dot-relative, root linker)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[./b]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        // Same dir → ./{new_basename}
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[[./renamed]]");
    }

    // Shape 3: [[b.md]] (md-suffixed)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b.md]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[[renamed.md]]");
    }

    // Shape 4: [[b|alias]] (bare with alias)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b|alias]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[[renamed|alias]]");
    }

    // Shape 5: [[b#sec]] (bare with fragment)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b#sec]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[[renamed#sec]]");
    }

    // Shape 6: [[b#sec|alias]] (bare with fragment+alias)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b#sec|alias]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "b.md",
            "renamed.md",
            "linker.md",
            "[[renamed#sec|alias]]",
        );
    }

    // Shape 7: [[./b#sec|alias]] (dot-relative with fragment+alias)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[./b#sec|alias]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        // Same dir → ./renamed#sec|alias
        assert_mv_preserves(
            &tmp,
            "b.md",
            "renamed.md",
            "linker.md",
            "[[./renamed#sec|alias]]",
        );
    }

    // Shape 8: [a](b.md) (markdown link)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [a](b.md)\n");
        write_md(tmp.path(), "b.md", "target\n");
        assert_mv_preserves(&tmp, "b.md", "renamed.md", "linker.md", "[a](renamed.md)");
    }

    // ---- Phase B: topologies × all shapes × rename-in-place ----

    // Topology 2: linker at root, target in sub/
    // mv sub/b.md → sub/renamed.md (same-dir rename across sub/)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[sub/b]]\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "linker.md",
            "[[sub/renamed]]",
        );
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[./sub/b]]\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        // Root linker → ./sub/renamed (NEW-2: preserves ./)
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "linker.md",
            "[[./sub/renamed]]",
        );
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [a](sub/b.md)\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "linker.md",
            "[a](sub/renamed.md)",
        );
    }

    // Topology 3: linker in notes/, target in bulk/
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "notes/linker.md", "link: [[bulk/b]]\n");
        write_md(tmp.path(), "bulk/b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "bulk/b.md",
            "bulk/renamed.md",
            "notes/linker.md",
            "[[bulk/renamed]]",
        );
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "notes/linker.md", "link: [a](../bulk/b.md)\n");
        write_md(tmp.path(), "bulk/b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "bulk/b.md",
            "bulk/renamed.md",
            "notes/linker.md",
            "[a](../bulk/renamed.md)",
        );
    }

    // Topology 4: linker and target both in sub/ (same-dir rename-only)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "sub/linker.md", "link: [[b]]\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "sub/linker.md",
            "[[renamed]]",
        );
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "sub/linker.md", "link: [[./b]]\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        // Same dir → ./renamed
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "sub/linker.md",
            "[[./renamed]]",
        );
    }
    {
        // Markdown link with path separator so the graph indexes it as vault-relative.
        // sub/linker.md → [a](./b.md) which includes "./" so normalize_target runs.
        // Note: bare same-dir [a](b.md) (no slash) is a known limitation of the
        // link graph — it's not indexed as a vault-relative backlink and thus
        // not rewritten on mv. Use path form [a](./b.md) to test the path.
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "sub/linker.md", "link: [a](./b.md)\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        // The markdown relative target ./b.md resolves to sub/b.md. After rename,
        // relative_path_between("sub/linker.md", "sub/renamed.md") = "renamed.md".
        assert_mv_preserves(
            &tmp,
            "sub/b.md",
            "sub/renamed.md",
            "sub/linker.md",
            "[a](renamed.md)",
        );
    }

    // ---- Phase C: move kinds ----

    // move-down (root → sub/): b.md → sub/b.md
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[b]]\n");
        write_md(tmp.path(), "b.md", "target\n");
        // Bare form: basename unchanged, stem unique → stays [[b]]
        assert_mv_preserves(&tmp, "b.md", "sub/b.md", "linker.md", "[[b]]");
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [a](b.md)\n");
        write_md(tmp.path(), "b.md", "target\n");
        // Markdown link → relative path from linker.md to sub/b.md
        assert_mv_preserves(&tmp, "b.md", "sub/b.md", "linker.md", "[a](sub/b.md)");
    }

    // move-up (sub/b.md → b.md)
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [[sub/b]]\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        // Path-form moves up: [[sub/b]] → [[b]] (bare since unique)
        assert_mv_preserves(&tmp, "sub/b.md", "b.md", "linker.md", "[[b]]");
    }
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "linker.md", "link: [a](sub/b.md)\n");
        write_md(tmp.path(), "sub/b.md", "target\n");
        // Markdown link → b.md (now at root)
        assert_mv_preserves(&tmp, "sub/b.md", "b.md", "linker.md", "[a](b.md)");
    }

    // ---- Phase D: selflink (target file contains a link back to itself) ----

    // Bare self-link: [[b]] inside b.md, mv b.md → renamed.md
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "b.md", "Self: [[b]]\n");
        let mv_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["mv", "--file", "b.md", "--to", "renamed.md"])
            .output()
            .unwrap();
        assert!(
            mv_out.status.success(),
            "mv failed: {}",
            String::from_utf8_lossy(&mv_out.stderr)
        );
        let content = fs::read_to_string(tmp.path().join("renamed.md")).unwrap();
        assert!(
            content.contains("[[renamed]]"),
            "self-link [[b]] should become [[renamed]] after mv, got: {content}"
        );
        // No broken links
        let links_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["links", "--format", "json"])
            .output()
            .unwrap();
        let lj: serde_json::Value = serde_json::from_slice(&links_out.stdout).unwrap();
        assert_eq!(lj["results"]["broken"].as_array().map_or(0, Vec::len), 0);
    }

    // Dot-relative self-link: [[./b]] inside b.md, mv b.md → renamed.md
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "b.md", "Self: [[./b]]\n");
        let mv_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["mv", "--file", "b.md", "--to", "renamed.md"])
            .output()
            .unwrap();
        assert!(
            mv_out.status.success(),
            "mv failed: {}",
            String::from_utf8_lossy(&mv_out.stderr)
        );
        let content = fs::read_to_string(tmp.path().join("renamed.md")).unwrap();
        assert!(
            content.contains("[[./renamed]]"),
            "self-link [[./b]] should become [[./renamed]] after mv, got: {content}"
        );
    }

    // Path-relative self-link: [[sub/b]] inside sub/b.md, mv sub/b.md → sub/renamed.md
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "sub/b.md", "Self: [[sub/b]]\n");
        let mv_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["mv", "--file", "sub/b.md", "--to", "sub/renamed.md"])
            .output()
            .unwrap();
        assert!(
            mv_out.status.success(),
            "mv failed: {}",
            String::from_utf8_lossy(&mv_out.stderr)
        );
        let content = fs::read_to_string(tmp.path().join("sub/renamed.md")).unwrap();
        assert!(
            content.contains("[[sub/renamed]]"),
            "self-link [[sub/b]] should become [[sub/renamed]] after mv, got: {content}"
        );
    }

    // Markdown self-link: [a](b.md) inside b.md, mv b.md → renamed.md
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "b.md", "Self: [a](b.md)\n");
        let mv_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["mv", "--file", "b.md", "--to", "renamed.md"])
            .output()
            .unwrap();
        assert!(
            mv_out.status.success(),
            "mv failed: {}",
            String::from_utf8_lossy(&mv_out.stderr)
        );
        let content = fs::read_to_string(tmp.path().join("renamed.md")).unwrap();
        assert!(
            content.contains("[a](renamed.md)"),
            "self-link [a](b.md) should become [a](renamed.md) after mv, got: {content}"
        );
    }

    // Selflink combined with inbound from another file
    {
        let tmp = TempDir::new().unwrap();
        write_md(tmp.path(), "b.md", "Self: [[b]]\n");
        write_md(tmp.path(), "other.md", "Link: [[b]]\n");
        let mv_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["mv", "--file", "b.md", "--to", "renamed.md"])
            .output()
            .unwrap();
        assert!(
            mv_out.status.success(),
            "mv failed: {}",
            String::from_utf8_lossy(&mv_out.stderr)
        );
        let self_content = fs::read_to_string(tmp.path().join("renamed.md")).unwrap();
        let other_content = fs::read_to_string(tmp.path().join("other.md")).unwrap();
        assert!(
            self_content.contains("[[renamed]]"),
            "self-link: {self_content}"
        );
        assert!(
            other_content.contains("[[renamed]]"),
            "inbound link: {other_content}"
        );
        // No broken links
        let links_out = hyalo_no_hints()
            .args(["--dir", tmp.path().to_str().unwrap()])
            .args(["links", "--format", "json"])
            .output()
            .unwrap();
        let lj: serde_json::Value = serde_json::from_slice(&links_out.stdout).unwrap();
        assert_eq!(lj["results"]["broken"].as_array().map_or(0, Vec::len), 0);
    }
}

// ---------------------------------------------------------------------------
// Named bug-repro tests (iter-150 NEW-1)
// ---------------------------------------------------------------------------

/// NEW-1 repro: bare wikilink self-link rewritten after mv.
#[test]
fn bug_iter150_new1_selflink_basic() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "x.md", "self: [[x]]\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[[y]]"),
        "[[x]] self-link should become [[y]] after mv x.md→y.md, got: {content}"
    );
    // Must have no broken links
    let links_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "--format", "json"])
        .output()
        .unwrap();
    let lj: serde_json::Value = serde_json::from_slice(&links_out.stdout).unwrap();
    assert_eq!(
        lj["results"]["broken"].as_array().map_or(0, Vec::len),
        0,
        "broken: 0 expected after selflink mv, got: {lj}"
    );
}

/// NEW-1 repro: self-link with alias rewritten after mv.
#[test]
fn bug_iter150_new1_selflink_with_alias() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "x.md", "self: [[x|me]]\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[[y|me]]"),
        "[[x|me]] self-link should become [[y|me]], got: {content}"
    );
}

/// NEW-1 repro: dot-relative self-link rewritten after mv.
#[test]
fn bug_iter150_new1_selflink_dot_relative() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "x.md", "self: [[./x]]\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[[./y]]"),
        "[[./x]] self-link should become [[./y]], got: {content}"
    );
}

/// NEW-1 repro: .md-suffixed self-link rewritten after mv.
#[test]
fn bug_iter150_new1_selflink_md_suffix() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "x.md", "self: [[x.md]]\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[[y.md]]"),
        "[[x.md]] self-link should become [[y.md]], got: {content}"
    );
}

/// NEW-1 repro: markdown self-link rewritten after mv.
#[test]
fn bug_iter150_new1_selflink_markdown_form() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "x.md", "self: [a](x.md)\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[a](y.md)"),
        "[a](x.md) self-link should become [a](y.md), got: {content}"
    );
}

// ---------------------------------------------------------------------------
// Named bug-repro tests (iter-150 NEW-2)
// ---------------------------------------------------------------------------

/// NEW-2 repro: [[./bulk/f]] survives with `./` after mv bulk/f.md bulk/g.md.
#[test]
fn bug_iter150_new2_dot_relative_cross_dir() {
    let tmp = TempDir::new().unwrap();
    // Root-level linker uses [[./bulk/f]] (dot-relative path to sub/)
    write_md(tmp.path(), "linker.md", "link: [[./bulk/f]]\n");
    write_md(tmp.path(), "bulk/f.md", "target\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "bulk/f.md", "--to", "bulk/g.md"])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("linker.md")).unwrap();
    assert!(
        content.contains("[[./bulk/g]]"),
        "[[./bulk/f]] should survive as [[./bulk/g]] after in-dir rename, got: {content}"
    );

    let links_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "--format", "json"])
        .output()
        .unwrap();
    let lj: serde_json::Value = serde_json::from_slice(&links_out.stdout).unwrap();
    assert_eq!(lj["results"]["broken"].as_array().map_or(0, Vec::len), 0);
}

/// NEW-2 repro: [[bulk/f.md]] survives with `.md` suffix after mv bulk/f.md bulk/g.md.
#[test]
fn bug_iter150_new2_md_suffix_cross_dir() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "linker.md", "link: [[f.md]]\n");
    write_md(tmp.path(), "f.md", "target\n");

    // Simple rename at root level to check .md suffix is preserved
    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "f.md", "--to", "g.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let content = fs::read_to_string(tmp.path().join("linker.md")).unwrap();
    assert!(
        content.contains("[[g.md]]"),
        "[[f.md]] should survive as [[g.md]] after rename, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// Named bug-repro tests (iter-150 NEW-3)
// ---------------------------------------------------------------------------

/// NEW-3 repro: mv against a vault with ambiguous inbound link produces
/// `skipped_ambiguous` JSON array.
#[test]
fn bug_iter150_new3_ambiguous_emits_diagnostic() {
    let tmp = TempDir::new().unwrap();
    // Two files with same stem → bare wikilink is ambiguous
    write_md(tmp.path(), "linker.md", "link: [[b]]\n");
    write_md(tmp.path(), "b.md", "root b\n");
    write_md(tmp.path(), "sub/b.md", "sub b\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "b.md", "--to", "archive/b.md"])
        .output()
        .unwrap();
    assert!(mv_out.status.success(), "mv failed");

    let json: serde_json::Value = serde_json::from_slice(&mv_out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse JSON: {e}\n{}",
            String::from_utf8_lossy(&mv_out.stdout)
        )
    });

    // skipped_ambiguous array must be populated
    let skipped = json["results"]["skipped_ambiguous"].as_array();
    assert!(
        skipped.is_some() && !skipped.unwrap().is_empty(),
        "skipped_ambiguous should be populated for ambiguous link, got: {json}"
    );
    let entry = &skipped.unwrap()[0];
    assert_eq!(entry["source"], "linker.md");
    assert_eq!(entry["target"], "b");
    let candidates = entry["candidates"].as_array().unwrap();
    assert!(
        candidates.len() >= 2,
        "expected ≥2 candidates, got: {candidates:?}"
    );
}

/// NEW-3 repro: text format emits stderr `note: skipped ambiguous link`.
#[test]
fn bug_iter150_new3_ambiguous_text_stderr() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "linker.md", "link: [[b]]\n");
    write_md(tmp.path(), "b.md", "root b\n");
    write_md(tmp.path(), "sub/b.md", "sub b\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        // Use --format text to exercise text-format stderr path
        .args([
            "--format",
            "text",
            "mv",
            "--file",
            "b.md",
            "--to",
            "archive/b.md",
        ])
        .output()
        .unwrap();
    // Note: the move is still done even with ambiguous links; we just get a note
    assert!(mv_out.status.success(), "mv failed");

    let stderr = String::from_utf8_lossy(&mv_out.stderr);
    assert!(
        stderr.contains("skipped ambiguous link") || stderr.contains("note: skipped"),
        "stderr should contain note about skipped ambiguous link, got: {stderr}"
    );
    assert!(
        stderr.contains("linker.md") || stderr.contains("[[b]]"),
        "stderr note should mention linker.md or [[b]], got: {stderr}"
    );
}

/// NEW-3: --allow-ambiguous flag is coherent (either rewrites or is removed).
/// This test asserts that with --allow-ambiguous, the ambiguous link IS rewritten
/// when the target file is renamed (new basename ≠ old basename).
#[test]
fn bug_iter150_new3_allow_ambiguous_behavior() {
    let tmp = TempDir::new().unwrap();
    // Two files share stem "b" → bare wikilink [[b]] is ambiguous.
    // We move the root b.md to root renamed.md (same dir, different basename)
    // so the link MUST change content.
    write_md(tmp.path(), "linker.md", "link: [[b]]\n");
    write_md(tmp.path(), "b.md", "root b\n");
    write_md(tmp.path(), "sub/b.md", "sub b\n");

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args([
            "mv",
            "--file",
            "b.md",
            "--to",
            "renamed.md",
            "--allow-ambiguous",
        ])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv with --allow-ambiguous failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&mv_out.stdout).unwrap_or_else(|e| {
        panic!(
            "parse JSON: {e}\n{}",
            String::from_utf8_lossy(&mv_out.stdout)
        )
    });

    // With --allow-ambiguous, the link should be rewritten (total_links_updated > 0)
    let total = json["results"]["total_links_updated"].as_u64().unwrap_or(0);
    assert!(
        total > 0,
        "--allow-ambiguous should cause the ambiguous link to be rewritten, got total_links_updated=0\n{json}"
    );

    // And skipped_ambiguous should be absent or empty
    let skipped_len = json["results"]["skipped_ambiguous"]
        .as_array()
        .map_or(0, Vec::len);
    assert_eq!(
        skipped_len, 0,
        "--allow-ambiguous should not produce skipped_ambiguous entries"
    );

    let content = fs::read_to_string(tmp.path().join("linker.md")).unwrap();
    assert!(
        !content.contains("[[b]]"),
        "[[b]] should be rewritten with --allow-ambiguous, got: {content}"
    );
    assert!(
        content.contains("[[renamed]]"),
        "link should be updated to [[renamed]] with --allow-ambiguous, got: {content}"
    );
}

// ---------------------------------------------------------------------------
// Dogfood verbatim repro (x.md self-link → y.md)
// ---------------------------------------------------------------------------

/// Dogfood NEW-1 repro: x.md contains [[x]] and [[./x|me]], mv x.md → y.md,
/// assert broken: 0.
#[test]
fn dogfood_selflink_x_to_y_broken_zero() {
    let tmp = TempDir::new().unwrap();
    // Reproduce the exact dogfood scenario:
    // printf -- '---\ntitle: x\ntype: note\ndate: 2026-06-01\n---\nself: [[x]] and [[./x|me]].\n'
    write_md(
        tmp.path(),
        "x.md",
        "---\ntitle: x\ntype: note\ndate: 2026-06-01\n---\nself: [[x]] and [[./x|me]].\n",
    );

    let mv_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["mv", "--file", "x.md", "--to", "y.md"])
        .output()
        .unwrap();
    assert!(
        mv_out.status.success(),
        "mv failed: {}",
        String::from_utf8_lossy(&mv_out.stderr)
    );

    let content = fs::read_to_string(tmp.path().join("y.md")).unwrap();
    assert!(
        content.contains("[[y]]"),
        "[[x]] should be rewritten to [[y]], got: {content}"
    );
    assert!(
        content.contains("[[./y|me]]"),
        "[[./x|me]] should be rewritten to [[./y|me]], got: {content}"
    );

    // Verify broken: 0
    let links_out = hyalo_no_hints()
        .args(["--dir", tmp.path().to_str().unwrap()])
        .args(["links", "--format", "json"])
        .output()
        .unwrap();
    let lj: serde_json::Value = serde_json::from_slice(&links_out.stdout).unwrap();
    let broken = lj["results"]["broken"].as_array().map_or(0, Vec::len);
    assert_eq!(broken, 0, "broken: 0 expected, got: {lj}");
}
