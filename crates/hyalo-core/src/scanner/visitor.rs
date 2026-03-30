use indexmap::IndexMap;
use serde_json::Value;

use super::ScanAction;

/// Callback-based scanner that streams through a markdown file.
/// Skips frontmatter, fenced code blocks, and inline code spans.
/// Calls the visitor function for each text segment with its 1-based line number.
#[cfg(test)]
pub(crate) fn scan_file<F>(path: &std::path::Path, visitor: F) -> anyhow::Result<()>
where
    F: FnMut(&str, usize) -> ScanAction,
{
    use anyhow::Context as _;
    use std::fs::File;
    use std::io::BufReader;
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    scan_reader(reader, visitor)
}

/// Scan from any buffered reader (useful for testing without file I/O).
#[cfg(test)]
pub(crate) fn scan_reader<R: std::io::BufRead, F>(reader: R, visitor: F) -> anyhow::Result<()>
where
    F: FnMut(&str, usize) -> ScanAction,
{
    let mut wrapper = ClosureVisitor { visitor };
    super::scan_reader_multi(reader, &mut [&mut wrapper])
}

/// Wraps a closure as a [`FileVisitor`].
///
/// [`dispatch_body_line`] strips both inline code spans and inline comments
/// before calling visitors, so this wrapper is a trivial passthrough.
#[cfg(test)]
struct ClosureVisitor<F: FnMut(&str, usize) -> ScanAction> {
    visitor: F,
}

#[cfg(test)]
impl<F: FnMut(&str, usize) -> ScanAction> FileVisitor for ClosureVisitor<F> {
    fn on_body_line(&mut self, _raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
        // Legacy closure-based API receives cleaned text for backward compatibility.
        (self.visitor)(cleaned, line_num)
    }
}

/// Trait for visitors that receive events from a single-pass file scan.
///
/// All methods have default no-op implementations, so visitors only need
/// to override the events they care about.
pub trait FileVisitor {
    /// Called with parsed frontmatter properties (empty `IndexMap` if none).
    ///
    /// The scanner passes ownership of the map to avoid a clone in the common
    /// single-visitor case. When multiple visitors are present, the scanner
    /// clones for all but the last, so only N-1 allocations occur for N visitors.
    ///
    /// Return `ScanAction::Stop` to skip the body scan for this visitor.
    fn on_frontmatter(&mut self, _props: IndexMap<String, Value>) -> ScanAction {
        ScanAction::Continue
    }

    /// Called for each body line outside fenced code blocks and comment blocks.
    ///
    /// `raw` is the original line text (code spans and comments intact).
    /// `cleaned` has inline code spans and `%%comment%%` spans replaced with spaces
    /// so that `[[links]]` inside backticks or comments are not extracted.
    ///
    /// Use `raw` for heading text extraction (to preserve code span content).
    /// Use `cleaned` for link and task extraction (to skip backtick-escaped markup).
    fn on_body_line(&mut self, _raw: &str, _cleaned: &str, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Called when a fenced code block opens (e.g. `` ```rust ``).
    fn on_code_fence_open(&mut self, _raw: &str, _language: &str, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Called when a fenced code block closes.
    fn on_code_fence_close(&mut self, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Called for each line inside a fenced code block (between open/close fences).
    /// Default: no-op. Override this to receive code block content.
    fn on_code_block_line(&mut self, _raw: &str, _line_num: usize) -> ScanAction {
        ScanAction::Continue
    }

    /// Whether this visitor needs body events (`on_body_line`, `on_code_block_line`,
    /// `on_code_fence_*`). If `false`, the visitor only receives `on_frontmatter`
    /// and is then stopped. Default: `true`.
    fn needs_body(&self) -> bool {
        true
    }

    /// Whether this visitor needs parsed frontmatter properties.
    /// If **no** visitor needs frontmatter, the scanner skips YAML accumulation
    /// and `serde_saphyr` parsing (but still reads past the `---` delimiters).
    /// Default: `true`.
    fn needs_frontmatter(&self) -> bool {
        true
    }
}
