//! Iteration 271 — every case where a hyalo *mutation* touched bytes it should
//! have left alone and exited 0.
//!
//! - **Part A (BUG-2, DEC-293).** An indented `  ---` inside a block scalar is
//!   not a closing fence, and the emitter never writes a block scalar that
//!   would trip a lenient reader.
//! - **Part B (BUG-13).** `properties rename --to ''` / `--from ''` exit 1.
//! - **Part C (BUG-3).** MD031 stays quiet at the opener of an unterminated
//!   fence.
//! - **Part D (BUG-28).** No autofixable rule rewrites a code block.
//! - **Part E (DEC-294).** `<!-- markdownlint-disable … -->` is honoured.
//! - **Part F (BUG-4, DEC-295).** `links fix` produces no case plan for a
//!   `site_prefix` link and never appends `/index`.
//! - **Part G (BUG-7).** `mv`'s ambiguity guard covers frontmatter links.

use super::common::{hyalo_no_hints, write_md};
use tempfile::TempDir;

/// A vault with no schema requirements, so only the rules under test speak.
fn vault(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    for (name, content) in files {
        write_md(tmp.path(), name, content);
    }
    tmp
}

fn run(tmp: &TempDir, args: &[&str]) -> std::process::Output {
    hyalo_no_hints()
        .current_dir(tmp.path())
        .args(args)
        .output()
        .unwrap()
}

fn json(tmp: &TempDir, args: &[&str]) -> serde_json::Value {
    let out = hyalo_no_hints()
        .current_dir(tmp.path())
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "`hyalo {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json output")
}

fn read(tmp: &TempDir, rel: &str) -> String {
    std::fs::read_to_string(tmp.path().join(rel)).unwrap()
}

// ---------------------------------------------------------------------------
// Part A — BUG-2: strict column-0 closing fence (DEC-293)
// ---------------------------------------------------------------------------

/// The dogfood fixture, verbatim: an indented `  ---` inside a `|-` block
/// scalar used to close the frontmatter, so `after` disappeared from every
/// read and `REALBODY` was replaced by the scalar's own tail.
const INDENTED_FENCE: &str = "---\ntitle: Ind\nk: |-\n  a\n  ---\n  b\nafter: 1\n---\nREALBODY\n";

#[test]
fn read_frontmatter_sees_past_an_indented_dashes_line() {
    let tmp = vault(&[("ind.md", INDENTED_FENCE)]);
    let v = json(&tmp, &["read", "ind.md", "--frontmatter"]);
    let fm = &v["results"]["frontmatter"];
    assert_eq!(fm["k"], "a\n---\nb", "the block scalar keeps its own `---`");
    assert_eq!(fm["after"], 1, "keys after the block scalar survive");
    assert_eq!(fm["title"], "Ind");
}

#[test]
fn find_and_mutations_all_agree_on_the_indented_fixture() {
    let tmp = vault(&[("ind.md", INDENTED_FENCE)]);

    let v = json(
        &tmp,
        &["find", "--file", "ind.md", "--fields", "properties"],
    );
    let props = &v["results"][0]["properties"];
    assert_eq!(props["after"], 1, "`find` sees the same map as `read`");

    // `set` adds exactly one line and leaves the body alone.
    let before = read(&tmp, "ind.md");
    let out = run(&tmp, &["set", "ind.md", "--property", "z=1"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = read(&tmp, "ind.md");
    assert!(
        after.ends_with("REALBODY\n"),
        "the body must survive untouched:\n{after}"
    );
    assert_eq!(
        after.lines().count(),
        before.lines().count() + 1,
        "exactly one line added:\n{after}"
    );
    assert!(
        after.contains("  ---"),
        "the block scalar is intact:\n{after}"
    );

    // `append` and `remove` see the full map too.
    assert!(
        run(&tmp, &["append", "ind.md", "--property", "tags=x"])
            .status
            .success()
    );
    assert!(
        run(&tmp, &["remove", "ind.md", "--property", "z"])
            .status
            .success()
    );
    let v = json(&tmp, &["read", "ind.md", "--frontmatter"]);
    assert_eq!(v["results"]["frontmatter"]["after"], 1);
    assert_eq!(v["results"]["frontmatter"]["k"], "a\n---\nb");
    assert!(read(&tmp, "ind.md").ends_with("REALBODY\n"));
}

/// An indented `  ---` as the last frontmatter line closes nothing, so the
/// block is unclosed — HYALO005 says so instead of the file being silently
/// truncated.
#[test]
fn an_indented_last_line_is_reported_as_unclosed_frontmatter() {
    let tmp = vault(&[("bad.md", "---\ntitle: t\n  ---\nbody\n")]);
    let out = run(&tmp, &["lint", "--rule", "HYALO005", "--format", "text"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("HYALO005"),
        "expected HYALO005 on an unclosed block, got:\n{text}"
    );
}

/// FENCE-2: a multi-line value containing a `---` line round-trips through
/// `set` → `find` as a quoted scalar, and a second `set` on another key
/// changes exactly one line.
#[test]
fn a_multiline_value_with_a_dashes_line_round_trips() {
    let tmp = vault(&[("n.md", "---\ntitle: n\n---\nbody\n")]);
    let out = run(&tmp, &["set", "n.md", "--property", "k=a\n---\nb"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let text = read(&tmp, "n.md");
    assert!(
        !text.contains("k: |"),
        "the value must not be written as a block scalar:\n{text}"
    );

    let v = json(&tmp, &["find", "--file", "n.md", "--fields", "properties"]);
    assert_eq!(v["results"][0]["properties"]["k"], "a\n---\nb");

    let before = read(&tmp, "n.md");
    assert!(
        run(&tmp, &["set", "n.md", "--property", "other=1"])
            .status
            .success()
    );
    let after = read(&tmp, "n.md");
    // Every line of the original survives verbatim and exactly one is added:
    // the second `set` must not re-serialize the quoted scalar.
    let added: Vec<&str> = after
        .lines()
        .filter(|l| !before.lines().any(|b| b == *l))
        .collect();
    assert_eq!(
        added,
        vec!["other: 1"],
        "a second set must add one line and rewrite nothing else:\n{before}\n---\n{after}"
    );
    assert_eq!(after.lines().count(), before.lines().count() + 1);
    let v = json(&tmp, &["find", "--file", "n.md", "--fields", "properties"]);
    assert_eq!(v["results"][0]["properties"]["k"], "a\n---\nb");
}

// ---------------------------------------------------------------------------
// Part B — BUG-13: `properties rename` rejects a non-name
// ---------------------------------------------------------------------------

#[test]
fn properties_rename_rejects_an_empty_or_blank_key() {
    let tmp = vault(&[
        ("a.md", "---\ntitle: Note 1\n---\n"),
        ("b.md", "---\ntitle: Note 2\n---\n"),
    ]);
    let before = (read(&tmp, "a.md"), read(&tmp, "b.md"));

    for args in [
        vec!["properties", "rename", "--from", "title", "--to", ""],
        vec!["properties", "rename", "--from", "", "--to", "title"],
        vec!["properties", "rename", "--from", "title", "--to", "   "],
        // A dry run is refused too — nothing about it makes the key valid.
        vec![
            "properties",
            "rename",
            "--from",
            "title",
            "--to",
            "",
            "--dry-run",
        ],
    ] {
        for format in ["text", "json"] {
            let mut full = args.clone();
            full.extend(["--format", format]);
            let out = run(&tmp, &full);
            assert!(
                !out.status.success(),
                "`hyalo {}` must exit non-zero",
                full.join(" ")
            );
        }
    }

    assert_eq!(
        (read(&tmp, "a.md"), read(&tmp, "b.md")),
        before,
        "a refused rename writes nothing"
    );
}

// ---------------------------------------------------------------------------
// Part C — BUG-3: MD031 at an unterminated fence opener
// ---------------------------------------------------------------------------

/// The dogfood fixture: a fence that opens and never closes. `--fix` used to
/// insert a blank line *inside* the sample.
///
/// Verified against the real corpus (`../docs/content`, read-only `--dry-run`).
/// Seven files there have an odd count of column-0 ```` ``` ```` lines; after
/// this change **six of them get no MD031 proposal at all**:
///
/// - `actions/tutorials/build-and-test-code/rust.md` (the reported hit)
/// - `actions/how-tos/secure-your-work/use-artifact-attestations/enforce-artifact-attestations.md`
/// - `admin/administering-your-instance/administering-your-instance-from-the-command-line/command-line-utilities.md`
/// - `billing/tutorials/automate-usage-reporting.md`
/// - `code-security/how-tos/secure-your-supply-chain/manage-your-dependency-security/removing-dependabot-access-to-public-registries.md`
/// - `code-security/tutorials/customize-code-scanning/preparing-your-code-for-codeql-analysis.md`
///
/// The seventh, `copilot/how-tos/copilot-cli/set-up-copilot-cli/troubleshoot-copilot-cli-auth.md`,
/// still reports MD031 at lines 32, 35 and 86 — correctly. Its odd count comes
/// from a fence line nested *inside* another fence, so its real openers are all
/// terminated and the guard must not (and does not) silence them.
#[test]
fn md031_is_silent_at_the_opener_of_an_unterminated_fence() {
    const UNTERMINATED: &str =
        "---\ntitle: t\n---\n\n# T\n\nIntro.\n\n```yaml\n  - uses: x\n  - name: y\n";
    let tmp = vault(&[("unterm.md", UNTERMINATED)]);

    let v = json(&tmp, &["lint", "--file", "unterm.md"]);
    let violations = v["results"]["violations"].as_u64().unwrap_or(0);
    assert_eq!(violations, 0, "no proposal at an unterminated opener: {v}");

    assert!(
        run(&tmp, &["lint", "--fix", "--file", "unterm.md"])
            .status
            .success()
    );
    assert_eq!(
        read(&tmp, "unterm.md"),
        UNTERMINATED,
        "the file must be byte-identical after --fix"
    );
}

/// The guard is narrow: a *terminated* fence with no blank line before it is
/// still reported and still fixed.
#[test]
fn md031_still_fires_on_a_terminated_fence() {
    let tmp = vault(&[(
        "ok.md",
        "---\ntitle: t\n---\n\n# T\n\nIntro.\n```yaml\nx: 1\n```\n\nEnd.\n",
    )]);
    let v = json(&tmp, &["lint", "--file", "ok.md", "--rule", "MD031"]);
    assert_eq!(
        v["results"]["violations"].as_u64().unwrap_or(0),
        1,
        "a real MD031 must survive: {v}"
    );
    assert!(
        run(&tmp, &["lint", "--fix", "--file", "ok.md"])
            .status
            .success()
    );
    assert!(
        read(&tmp, "ok.md").contains("Intro.\n\n```yaml"),
        "the blank line is inserted where it belongs"
    );
}

// ---------------------------------------------------------------------------
// Part D — BUG-28: no autofixable rule rewrites a code block
// ---------------------------------------------------------------------------

/// The dogfood fixture: `#   x` inside a backtick fence, a tilde fence and an
/// indented block. MD019 rewrote all three.
#[test]
fn md019_leaves_every_kind_of_code_block_alone() {
    const D: &str = "---\ntitle: d\n---\n\n# T\n\n```text\n#   three\n```\n\n~~~sh\n#   tilde\n~~~\n\n    #   indented\n";
    let tmp = vault(&[("d.md", D)]);

    let v = json(&tmp, &["lint", "--file", "d.md", "--rule", "MD019"]);
    assert_eq!(
        v["results"]["violations"].as_u64().unwrap_or(0),
        0,
        "MD019 must not see code: {v}"
    );

    assert!(
        run(&tmp, &["lint", "--fix", "--file", "d.md"])
            .status
            .success()
    );
    assert_eq!(read(&tmp, "d.md"), D, "the file must be byte-identical");
}

/// MD019 is not exempted wholesale — a heading in prose is still fixed.
#[test]
fn md019_still_fixes_a_real_heading() {
    let tmp = vault(&[("h.md", "---\ntitle: h\n---\n\n#   Spaced\n\nBody.\n")]);
    assert!(
        run(&tmp, &["lint", "--fix", "--file", "h.md"])
            .status
            .success()
    );
    assert!(
        read(&tmp, "h.md").contains("# Spaced"),
        "prose headings are still normalised"
    );
}

/// The audit: one fixture placing a trigger for every autofixable stock rule
/// inside a backtick fence, a tilde fence, an indented block, an HTML comment
/// and the frontmatter block. `lint --fix` must leave it byte-identical.
///
/// The samples are deliberately "wrong" markdown — reversed links, bare URLs,
/// missing heading space, multiple blanks, trailing spaces, list-marker and
/// ordered-list oddities, emphasis-as-heading, trailing punctuation. The one
/// trigger deliberately **absent** is a hard tab: MD010 keeps checking code
/// blocks on purpose (markdownlint's own `code_blocks: true` default), and the
/// escape hatch for a page whose subject is tabs is the disable comment
/// exercised below, not a blanket exemption (DEC-294).
#[test]
fn no_autofixable_rule_rewrites_a_protected_region() {
    const SAMPLES: &str = concat!(
        "(text)[https://example.com/reversed]\n",
        "https://example.com/bare\n",
        "#missing space\n",
        "##   too many spaces\n",
        "   ### indented heading\n",
        "#### trailing punctuation.\n",
        "*  wide list marker\n",
        "1. one\n",
        "1. two\n",
        "trailing spaces here   \n",
        "\n",
        "\n",
        "\n",
        "**emphasis as heading**\n",
        "[](empty-link)\n",
    );

    let mut body = String::from(
        "---\ntitle: audit\nnote: \"(text)[https://example.com/x] #nospace\"\n---\n\n# Audit\n\n",
    );
    body.push_str("```text\n");
    body.push_str(SAMPLES);
    body.push_str("```\n\n");
    body.push_str("~~~\n");
    body.push_str(SAMPLES);
    body.push_str("~~~\n\n");
    for line in SAMPLES.lines() {
        body.push_str("    ");
        body.push_str(line);
        body.push('\n');
    }
    body.push_str("\n<!--\n");
    body.push_str(SAMPLES);
    body.push_str("-->\n");

    let tmp = vault(&[("audit.md", body.as_str())]);
    let out = run(&tmp, &["lint", "--fix", "--file", "audit.md"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        read(&tmp, "audit.md"),
        body,
        "no autofixable rule may rewrite a fence, an indented block, an HTML comment or frontmatter"
    );
}

// ---------------------------------------------------------------------------
// Part E — DEC-294: markdownlint-disable comments
// ---------------------------------------------------------------------------

/// MDN's shape: a deliberately tab-laden fence wrapped in
/// `<!-- markdownlint-disable no-hard-tabs -->`. The protected region is
/// byte-identical after `--fix`; the same trigger outside it still fires.
#[test]
fn markdownlint_disable_protects_a_region_by_alias() {
    let protected = "---\ntitle: t\n---\n\n# T\n\n<!-- markdownlint-disable no-hard-tabs -->\n\n```html\n\ttabbed sample\n```\n\n<!-- markdownlint-enable no-hard-tabs -->\n\n\tunprotected tab\n";
    let tmp = vault(&[("mdn.md", protected)]);

    let v = json(&tmp, &["lint", "--file", "mdn.md", "--rule", "MD010"]);
    assert_eq!(
        v["results"]["violations"].as_u64().unwrap_or(0),
        1,
        "only the tab outside the disabled region is reported: {v}"
    );

    assert!(
        run(&tmp, &["lint", "--fix", "--file", "mdn.md"])
            .status
            .success()
    );
    let after = read(&tmp, "mdn.md");
    assert!(
        after.contains("\ttabbed sample"),
        "the protected sample keeps its tab:\n{after}"
    );
    assert!(
        !after.contains("\tunprotected tab"),
        "the tab outside the region is still fixed:\n{after}"
    );
}

/// `disable-next-line` protects exactly one line. The target here is a
/// paragraph line rather than a heading on purpose: a heading immediately
/// after the comment also trips MD022 (blanks-around-headings), whose fix
/// inserts a blank line *between* the directive and its target and so moves
/// the target out from under it — markdownlint behaves the same way, and the
/// portable spelling for a heading is `disable-line` on the heading itself.
#[test]
fn markdownlint_disable_next_line_scopes_to_one_line() {
    let tmp = vault(&[(
        "n.md",
        "---\ntitle: t\n---\n\n# T\n\n<!-- markdownlint-disable-next-line MD009 -->\nkept   \nfixed   \n",
    )]);
    assert!(
        run(&tmp, &["lint", "--fix", "--file", "n.md"])
            .status
            .success()
    );
    let after = read(&tmp, "n.md");
    assert!(
        after.contains("kept   \n"),
        "the named line keeps its trailing spaces:\n{after:?}"
    );
    assert!(
        after.contains("fixed\n"),
        "the line after it is still fixed:\n{after:?}"
    );
}

// ---------------------------------------------------------------------------
// Part F — BUG-4 / DEC-295: case plans on a site_prefix vault
// ---------------------------------------------------------------------------

/// A site-absolute link written in the site's Title-case URL convention over
/// lowercase folders. On a copy of MDN's CSS tree this shape produced 5096
/// rewrites across 1049 files; it must now produce none.
#[test]
fn a_site_prefix_link_is_not_a_case_mismatch() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(
        tmp.path().join(".hyalo.toml"),
        "dir = \".\"\nsite_prefix = \"en-US/docs/Web/CSS\"\n",
    )
    .unwrap();
    write_md(
        tmp.path(),
        "guides/anchor_positioning/index.md",
        "---\ntitle: Anchor\n---\n",
    );
    let source =
        "---\ntitle: Page\n---\n\nSee [Anchor](/en-US/docs/Web/CSS/Guides/Anchor_positioning).\n";
    write_md(tmp.path(), "page.md", source);

    let v = json(&tmp, &["links", "fix", "--dry-run"]);
    assert_eq!(
        v["results"]["case_mismatches"].as_u64().unwrap_or(0),
        0,
        "a site-prefixed link is correct for the site: {v}"
    );

    let out = run(&tmp, &["links", "fix", "--apply"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(read(&tmp, "page.md"), source, "--apply must write nothing");
}

/// CASE-1 for every other strategy: a directory link that really does need a
/// fix keeps its form — no `/index`, no `.md`, and a trailing slash survives.
#[test]
fn a_directory_link_fix_never_grows_an_index_segment() {
    let tmp = vault(&[
        ("guides/anchor/index.md", "---\ntitle: Anchor\n---\n"),
        (
            "page.md",
            "---\ntitle: Page\n---\n\n[a](Guides/Anchor) and [b](Guides/Anchor/).\n",
        ),
    ]);

    let out = run(&tmp, &["links", "fix", "--apply"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = read(&tmp, "page.md");
    assert!(
        !after.contains("/index"),
        "`/index` must never be appended to a form that lacked it:\n{after}"
    );
    assert!(
        !after.contains(".md)"),
        "`.md` must not be appended either:\n{after}"
    );
    assert!(
        after.contains("[b](guides/anchor/)"),
        "a trailing slash is part of the incoming form:\n{after}"
    );
}

// ---------------------------------------------------------------------------
// Part G — BUG-7: `mv` guards frontmatter links too
// ---------------------------------------------------------------------------

fn ambiguous_vault() -> TempDir {
    vault(&[
        ("a.md", "---\ntitle: A\n---\n"),
        ("x/a.md", "---\ntitle: XA\n---\n"),
        (
            "c.md",
            "---\ntitle: C\nrelated: \"[[a]]\"\nrel2: [[a|al]]\n---\nbody [[a]] and [[a|al]]\n",
        ),
    ])
}

#[test]
fn mv_skips_ambiguous_frontmatter_links_like_body_links() {
    let tmp = ambiguous_vault();
    let before = read(&tmp, "c.md");

    let v = json(&tmp, &["mv", "a.md", "z.md"]);
    let skipped = v["results"]["skipped_ambiguous"]
        .as_array()
        .expect("skipped_ambiguous array");
    assert_eq!(
        skipped.len(),
        4,
        "two frontmatter links and two body links: {v}"
    );
    let properties: Vec<&str> = skipped
        .iter()
        .filter_map(|s| s["property"].as_str())
        .collect();
    assert!(
        properties.contains(&"related") && properties.contains(&"rel2"),
        "each frontmatter skip carries its property: {properties:?}"
    );
    assert_eq!(
        v["results"]["files_updated"].as_u64().unwrap_or(0),
        0,
        "nothing is rewritten: {v}"
    );
    assert_eq!(read(&tmp, "c.md"), before, "c.md is byte-identical");
}

#[test]
fn mv_allow_ambiguous_rewrites_all_four() {
    let tmp = ambiguous_vault();
    let out = run(&tmp, &["mv", "a.md", "z.md", "--allow-ambiguous"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = read(&tmp, "c.md");
    assert!(!after.contains("[[a]]"), "no old target remains:\n{after}");
    assert!(!after.contains("[[a|al]]"), "aliases too:\n{after}");
    assert_eq!(
        after.matches("[[z").count(),
        4,
        "all four links rewritten:\n{after}"
    );
}

/// Batch `mv` shares `plan_inbound_rewrites`, so it inherits the guard. It has
/// no JSON slot for the skips (they go to stderr, as line-spanning frontmatter
/// links do), but the substantive property is the same: nothing is rewritten.
#[test]
fn batch_mv_applies_the_same_frontmatter_guard() {
    let tmp = ambiguous_vault();
    let before = read(&tmp, "c.md");
    let out = run(&tmp, &["mv", "--glob", "a.md", "--to", "moved/", "--apply"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ambiguous"),
        "batch mv names the skipped links on stderr:\n{stderr}"
    );
    assert_eq!(read(&tmp, "c.md"), before, "c.md is byte-identical");
}
