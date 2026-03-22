use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;

/// Outcome from processing a single file in parallel.
pub enum FileResult<T> {
    /// File produced a result.
    Ok(T),
    /// File was skipped (e.g. malformed YAML) — caller already emitted a warning.
    Skipped,
}

/// Process files in parallel, collect successful results.
///
/// Each file is processed by `f` which returns:
/// - `Ok(FileResult::Ok(value))` — file produced a result
/// - `Ok(FileResult::Skipped)` — file skipped (e.g. parse error, warning already emitted)
/// - `Err(e)` — hard I/O error, propagated after the parallel phase
///
/// Order of results matches the input order (rayon's indexed `par_iter`).
pub fn par_process_files<T, F>(files: &[(PathBuf, String)], f: F) -> Result<Vec<T>>
where
    T: Send,
    F: Fn(&Path, &str) -> Result<FileResult<T>> + Sync,
{
    let results: Vec<Result<FileResult<T>>> =
        files.par_iter().map(|(path, rel)| f(path, rel)).collect();

    let mut items = Vec::with_capacity(files.len());
    for r in results {
        match r? {
            FileResult::Ok(v) => items.push(v),
            FileResult::Skipped => {}
        }
    }
    Ok(items)
}
