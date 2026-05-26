use regex::Regex;
use tempfile::TempDir;

use super::common::hyalo;

/// Pattern matching `hyalo <semver> (<sha7-12>[+dirty] <YYYY-MM-DD>)`.
/// The bare-semver fallback (tarball / `CARGO_HYALO_FORCE_NO_GIT=1`) is also accepted.
fn version_re() -> Regex {
    Regex::new(
        r"^hyalo \d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?: \([0-9a-f]{7,12}(?:\+dirty)? \d{4}-\d{2}-\d{2}\))?\n?$",
    )
    .unwrap()
}

#[test]
fn version_long_flag_matches_provenance_shape() {
    // Run from a clean tempdir so the cwd-config `(kb dir: ...)` suffix doesn't trip the regex.
    let tmp = TempDir::new().unwrap();
    let output = hyalo()
        .current_dir(tmp.path())
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success(), "hyalo --version should succeed");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let re = version_re();
    assert!(
        re.is_match(&stdout),
        "--version output {stdout:?} does not match expected shape"
    );
}

#[test]
fn version_short_flag_matches_long_flag() {
    let tmp = TempDir::new().unwrap();
    let long = hyalo()
        .current_dir(tmp.path())
        .arg("--version")
        .output()
        .unwrap();
    let short = hyalo().current_dir(tmp.path()).arg("-V").output().unwrap();
    assert!(long.status.success());
    assert!(short.status.success());
    assert_eq!(
        long.stdout, short.stdout,
        "-V should match --version output exactly"
    );
}
