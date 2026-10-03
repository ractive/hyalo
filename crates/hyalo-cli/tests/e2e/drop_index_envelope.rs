//! Regression test for the codebase review (2026-10-03): a non-`NotFound`
//! failure deleting the index file used to escape `drop-index` as a bare
//! `anyhow` error (exit 2, no JSON error envelope), rather than going through
//! `hyalo_core::user_error_with` like every other hyalo-own user error
//! (DEC-307: 0/1/2, where 1 is the error-envelope exit code).
use super::common::hyalo_no_hints;
use std::fs;
use tempfile::TempDir;

/// A directory at the index path is rejected before the actual `remove_file`
/// call (the "is this a regular file" guard), but it exercises the exact
/// same non-`NotFound` error arm that an I/O failure during deletion would:
/// both must now surface as a proper exit-1 JSON envelope instead of an
/// internal (exit 2) crash-shaped error.
#[test]
fn drop_index_non_not_found_failure_uses_error_envelope() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".hyalo.toml"), "dir = \".\"\n").unwrap();
    fs::create_dir(dir.path().join(".hyalo-index")).unwrap();

    let output = hyalo_no_hints()
        .current_dir(dir.path())
        .args(["drop-index", "--format", "json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "a non-NotFound delete failure is a hyalo-own user error (DEC-307), not exit 2; \
         stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Error envelopes render to stderr, not stdout (see
    // `OutputPipeline::render` in `output_pipeline.rs`: a diagnostic is
    // always pushed to `rendered.stderr`, and an error path carries no
    // `report.payload`, so stdout is empty).
    assert!(
        output.stdout.is_empty(),
        "stdout must stay empty on an error path: {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert!(
        json.get("error").and_then(|e| e.as_str()).is_some(),
        "stderr must carry the standard error envelope: {json}"
    );
    // The directory must be left untouched — the command must not have
    // silently removed anything.
    assert!(dir.path().join(".hyalo-index").is_dir());
}
