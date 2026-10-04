//! `links auto`'s "Apply N auto-links" hint must not add `--no-first-only`
//! (or `--first-only`) to the rebuilt command when doing so changes nothing.
//!
//! The rebuilt command already passes neither flag by default, which yields
//! `first_only = false` unless `[links.auto] first_only = true` is
//! configured. The hint used to always restate whichever value was
//! *effectively* in force, so with the all-default config it added
//! `--no-first-only` to a command that already behaved that way.

use super::common::{hyalo, write_md};

#[test]
fn apply_hint_omits_first_only_flags_under_default_config() {
    let tmp = tempfile::tempdir().unwrap();
    // A target note and a plain-text mention of its title elsewhere, so
    // `links auto` (dry run by default) finds at least one candidate and
    // emits the "Apply N auto-links" hint. The title is deliberately
    // distinctive — a common word or generic doc filename is held back by
    // DEC-286's default exclusions and would never produce a match.
    write_md(tmp.path(), "Zylofoop.md", "# Zylofoop\n\nSome content.\n");
    write_md(
        tmp.path(),
        "mentions.md",
        "# Mentions\n\nThis note talks about Zylofoop in prose.\n",
    );

    let output = hyalo()
        .arg("--dir")
        .arg(tmp.path())
        .args(["links", "auto", "--format", "json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "links auto failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json output");
    let hints = json["hints"].as_array().cloned().unwrap_or_default();
    let apply_hint = hints
        .iter()
        .find(|h| {
            h["description"]
                .as_str()
                .is_some_and(|d| d.starts_with("Apply "))
        })
        .unwrap_or_else(|| panic!("expected an 'Apply N auto-links' hint, got: {json}"));

    let cmd = apply_hint["cmd"].as_str().unwrap_or_default();
    assert!(
        !cmd.contains("--no-first-only") && !cmd.contains("--first-only"),
        "default config needs no first-only flag restated: {cmd}"
    );
}
