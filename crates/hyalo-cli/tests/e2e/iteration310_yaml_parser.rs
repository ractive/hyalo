//! Iteration 310: serde-saphyr 1.x (DEC-350).
//!
//! - A leading-zero plain scalar is still an integer, for reads, typed
//!   filters and the value `set` writes back.
//! - `.nan` keeps its string fallback; the words `NaN` / `Infinity` stay
//!   as written.
//! - A value with an inner `#` is written double-quoted, as before.
use super::common::{hyalo_no_hints, write_md};
use serde_json::{Value, json};
use tempfile::TempDir;

fn properties(dir: &std::path::Path, file: &str) -> Value {
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(dir)
        .args(["find", "--file", file, "--fields", "properties"])
        .args(["--format", "json"])
        .assert()
        .success()
        .get_output()
        .clone();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    json["results"][0]["properties"].clone()
}

#[test]
fn leading_zero_scalars_read_and_filter_as_integers() {
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nzip: 01234\nlist: [01, 02, 10]\ntitle: NaN\nf: .nan\n---\nbody\n",
    );
    let props = properties(tmp.path(), "a.md");
    assert_eq!(props["zip"], json!(1234));
    assert!(props["zip"].is_u64(), "integer, not float: {props}");
    assert_eq!(props["list"], json!([1, 2, 10]));
    assert_eq!(props["title"], json!("NaN"));
    assert_eq!(props["f"], json!(".nan"));

    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--property", "zip=1234", "--count"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");
}

#[test]
fn yaml11_integer_forms_read_as_strings() {
    // BUG-14 (dogfood-v0250, iter-314, DEC-364): YAML 1.2 core only has
    // `[-+]?[0-9]+` as an integer literal. serde-saphyr still resolves the
    // YAML 1.1 extensions — digit-group underscores, hex and binary
    // prefixes — to plain integers with no float detour, so they need their
    // own re-check (unlike the leading-zero case above, which arrives as an
    // integral float and was already covered by iteration 310).
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nd: 1_000\np: 0X1F\nq: 0b101\nkey_1: ok\ncreated_at: 2026-01-01\n---\nbody\n",
    );
    let props = properties(tmp.path(), "a.md");
    assert_eq!(props["d"], json!("1_000"));
    assert_eq!(props["p"], json!("0X1F"));
    assert_eq!(props["q"], json!("0b101"));
    // A snake_case key/value with no digit-adjacent underscore, and an
    // underscore adjacent to a digit in a KEY rather than a value, must stay
    // on the fast path / be unaffected — these pin the pre-check against
    // over-triggering on ordinary vault content.
    assert_eq!(props["key_1"], json!("ok"));
    assert_eq!(props["created_at"], json!("2026-01-01"));

    // `find --property p=31` must NOT match: `0X1F` is the string "0X1F",
    // never the integer 31.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--property", "p=31", "--count"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "0");
}

#[test]
fn core_hex_and_octal_int_forms_stay_numbers() {
    // dogfood PR #378 review follow-up: `0x1F`/`0o17` (lowercase prefix)
    // are YAML 1.2 core integers, not a YAML 1.1 extension, and must stay
    // numbers even in the same file as a true YAML 1.1 form that triggers
    // the slow re-check path.
    let tmp = TempDir::new().unwrap();
    write_md(
        tmp.path(),
        "a.md",
        "---\nh: 0x1F\no: 0o17\nneg: -0x1F\nbad: 0X1F\n---\nbody\n",
    );
    let props = properties(tmp.path(), "a.md");
    assert_eq!(props["h"], json!(31));
    assert_eq!(props["o"], json!(15));
    assert_eq!(props["neg"], json!(-31));
    assert_eq!(props["bad"], json!("0X1F"));

    // `find --property h=31` must match: `0x1F` really is the integer 31.
    let output = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["find", "--property", "h=31", "--count"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "1");

    // `set --property fresh=0x1F`: `set`'s own CLI-value coercion
    // (`frontmatter::types::infer_value`) only recognizes a plain decimal
    // integer, unchanged by this fix — hex/octal CLI input was never
    // special-cased there, before or after BUG-14. So "0x1F" typed on the
    // command line is coerced to the *string* "0x1F", and the emitter
    // correctly quotes it (`fresh: "0x1F"`): writing that string plain
    // would now (post-fix) read back as the integer 31, corrupting the
    // round trip of the string value the user actually typed. This is the
    // existing `set x=0X1F`/`z=0b101` pattern, extended automatically by
    // serde-saphyr's own plain-scalar safety check — not something this
    // fix added.
    hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["set", "a.md", "--property", "fresh=0x1F"])
        .assert()
        .success();
    let text = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert!(
        text.contains("fresh: \"0x1F\"\n"),
        "a CLI-typed hex string must stay quoted so it round-trips as the \
         string it was typed as, not the integer it would read back as \
         plain: {text:?}"
    );
    let props = properties(tmp.path(), "a.md");
    assert_eq!(props["fresh"], json!("0x1F"));

    // The pre-existing hex `h: 0x1F` property already written to this file's
    // frontmatter (unquoted, a real core int) is untouched by this `set`.
    let props_h = properties(tmp.path(), "a.md");
    assert_eq!(props_h["h"], json!(31));
}

#[test]
fn set_property_round_trips_yaml11_int_forms_as_text() {
    // AC: `hyalo set f.md --property x=1_000` round-trips unchanged — `set`
    // already quotes `1_000`/`0X1F` on write (DEC-350), and reading them
    // back must agree they're strings (BUG-14).
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: t\n---\nbody\n");
    for property in ["x=1_000", "hex=0X1F", "z=0b101"] {
        hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["set", "a.md", "--property", property])
            .assert()
            .success();
    }
    let text = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert_eq!(
        text,
        "---\ntitle: t\nx: \"1_000\"\nhex: \"0X1F\"\nz: \"0b101\"\n---\nbody\n"
    );
    let props = properties(tmp.path(), "a.md");
    assert_eq!(props["x"], json!("1_000"));
    assert_eq!(props["hex"], json!("0X1F"));
    assert_eq!(props["z"], json!("0b101"));

    // Re-running the same `set` is a no-op (the value already round-trips).
    let out = hyalo_no_hints()
        .arg("--dir")
        .arg(tmp.path())
        .args(["set", "a.md", "--property", "x=1_000"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text2 = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert_eq!(text2, text);
}

#[test]
fn set_writes_hash_values_quoted_and_dates_plain() {
    let tmp = TempDir::new().unwrap();
    write_md(tmp.path(), "a.md", "---\ntitle: t\n---\nbody\n");
    for property in ["lang=C#", "date=2026-01-01", "flag=yes"] {
        hyalo_no_hints()
            .arg("--dir")
            .arg(tmp.path())
            .args(["set", "a.md", "--property", property])
            .assert()
            .success();
    }
    let text = std::fs::read_to_string(tmp.path().join("a.md")).unwrap();
    assert_eq!(
        text,
        "---\ntitle: t\nlang: \"C#\"\ndate: 2026-01-01\nflag: \"yes\"\n---\nbody\n"
    );
}
