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
