#![allow(clippy::missing_errors_doc)]
use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde_json::Value;
use serde_saphyr::{Budget, DuplicateKeyPolicy, Options, SerializerOptions};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use super::splice::{FallbackReason, SpliceOutcome, rename_key_in_place, splice_frontmatter};
use super::{FrontmatterError, MAX_FRONTMATTER_BYTES, MAX_FRONTMATTER_LINES};

/// Convenience macro: return a [`FrontmatterError`] wrapped in `anyhow::Error`.
///
/// Use this for all parse/structural errors so that callers can distinguish them
/// from I/O errors via [`super::is_parse_error`].
macro_rules! parse_bail {
    ($($arg:tt)*) => {
        return Err(anyhow::Error::new(FrontmatterError(format!($($arg)*))))
    };
}

/// Shared parser options for all YAML frontmatter parsing in hyalo.
///
/// Enforces tight limits via a `Budget` to harden the parser against
/// pathological inputs (deep nesting, alias bombs, huge scalars).
/// Also enables strict YAML 1.2 booleans (`true`/`false` only) and
/// rejects duplicate keys.
pub fn hyalo_options() -> Options {
    Options {
        budget: Some(Budget {
            max_events: 10_000,
            max_depth: 20,
            max_aliases: 0,
            max_anchors: 0,
            max_nodes: 5_000,
            // Matches MAX_FRONTMATTER_BYTES (the documented 64 KiB frontmatter
            // limit): scalar content is a subset of the whole block, which is
            // already capped there by the pre-read line/byte guards in every
            // caller, so this can never be the tighter limit in practice
            // (iter-219 NEW-8 — it used to be 8192, well below the documented
            // ceiling, and the resulting parser error leaked raw budget-breach
            // internals; see `friendly_parse_error`).
            max_total_scalar_bytes: MAX_FRONTMATTER_BYTES,
            max_documents: 1,
            ..Budget::default()
        }),
        duplicate_keys: DuplicateKeyPolicy::Error,
        strict_booleans: true,
        ..Options::default()
    }
}

/// Turn a `serde_saphyr` parse error into a hyalo-voice message with no
/// leaked parser-internal type names (iter-219 NEW-8).
///
/// `serde_saphyr`'s own `Display` is mostly clean, but two variants render
/// their payload with `{:?}` and leak Rust struct syntax straight to the
/// user: a budget breach (`budget breached: ScalarBytes { total_scalar_bytes:
/// 8205 }`) and a duplicate key (`duplicate mapping key: x, set
/// DuplicateKeyPolicy in Options if acceptable` — a hint about *our* internal
/// `Options` type, not something the user can act on). Both are intercepted
/// here; everything else falls through to the crate's own message, which
/// does not have this problem.
///
/// `scalar_byte_limit` is the `max_total_scalar_bytes` the caller's
/// `Options` was actually built with — passed explicitly, rather than
/// hardcoding [`MAX_FRONTMATTER_BYTES`], because `splice_frontmatter`'s own
/// verification pass parses with a *different* (2x) budget; a caller
/// wiring that error path through here in the future must not get a
/// message naming the wrong limit.
pub(crate) fn friendly_parse_error(err: &serde_saphyr::Error, scalar_byte_limit: usize) -> String {
    match unwrap_snippet(err) {
        serde_saphyr::Error::Budget { breach, location } => with_location(
            &describe_budget_breach(breach, scalar_byte_limit),
            *location,
        ),
        serde_saphyr::Error::DuplicateMappingKey { key, location } => {
            let what = match key {
                Some(k) => format!("duplicate key '{k}' in frontmatter"),
                None => "duplicate key in frontmatter".to_owned(),
            };
            with_location(
                &format!("{what} — YAML mappings must have unique keys"),
                *location,
            )
        }
        // Every other variant's own `Display` is already clean — return the
        // *original* (still-`WithSnippet`-wrapped, when present) error, not
        // the unwrapped inner one, so its source-snippet caret/window
        // survives. Only the two rewritten variants above need unwrapping,
        // to reach their `location`/`breach`/`key` fields for matching.
        _ => err.to_string(),
    }
}

/// Append `" at line X, column Y"` — the same phrasing `serde_saphyr`'s own
/// localizer uses for every other error — unless `location` is
/// [`serde_saphyr::Location::UNKNOWN`] (no precise position was available),
/// in which case the suffix is omitted entirely rather than printing
/// "line 0, column 0".
fn with_location(message: &str, location: serde_saphyr::Location) -> String {
    if location == serde_saphyr::Location::UNKNOWN {
        message.to_owned()
    } else {
        format!(
            "{message} at line {}, column {}",
            location.line(),
            location.column()
        )
    }
}

/// Walk past `Error::WithSnippet` wrappers to the underlying error.
fn unwrap_snippet(err: &serde_saphyr::Error) -> &serde_saphyr::Error {
    let mut current = err;
    while let serde_saphyr::Error::WithSnippet { error, .. } = current {
        current = error;
    }
    current
}

/// Describe a budget breach without leaking the `BudgetBreach` Debug format.
///
/// `scalar_byte_limit` names the actual configured `max_total_scalar_bytes`
/// for the `ScalarBytes` case — see [`friendly_parse_error`] for why this
/// isn't just [`MAX_FRONTMATTER_BYTES`].
fn describe_budget_breach(
    breach: &serde_saphyr::budget::BudgetBreach,
    scalar_byte_limit: usize,
) -> String {
    use serde_saphyr::budget::BudgetBreach;
    match breach {
        BudgetBreach::ScalarBytes { total_scalar_bytes } => format!(
            "frontmatter content is too large ({total_scalar_bytes} bytes of scalar text exceeds \
             the {scalar_byte_limit}-byte limit); trim large values or split them into separate files"
        ),
        BudgetBreach::Anchors { .. } | BudgetBreach::Aliases { .. } => {
            "frontmatter uses YAML anchors/aliases, which hyalo does not support — expand the \
             anchor into a literal value"
                .to_owned()
        }
        BudgetBreach::Depth { .. } => {
            "frontmatter nests too deeply for hyalo's parser limits — flatten the structure"
                .to_owned()
        }
        BudgetBreach::Documents { .. } => {
            "frontmatter contains more than one YAML document (a `---` document separator inside \
             the block) — hyalo expects exactly one"
                .to_owned()
        }
        BudgetBreach::Nodes { .. } | BudgetBreach::Events { .. } => {
            "frontmatter is too large or complex for hyalo's parser limits".to_owned()
        }
        BudgetBreach::MergeKeys { .. } => {
            "frontmatter uses too many YAML merge keys (`<<`) for hyalo's parser limits".to_owned()
        }
        _ => "frontmatter exceeds hyalo's parser limits".to_owned(),
    }
}

/// Build `SerializerOptions` that preserve the detected list indentation style.
fn hyalo_serializer_options(compact_list_indent: bool) -> SerializerOptions {
    SerializerOptions {
        compact_list_indent,
        ..SerializerOptions::default()
    }
}

/// Detect whether the YAML content uses compact list indentation.
///
/// Scans for the first sequence indicator (`- `) and checks whether it is indented
/// further than its parent mapping key. Returns `true` for compact (flush) style,
/// `false` for indented style. Defaults to `false` if no sequences are found.
pub(super) fn detect_list_indent_style(yaml: &str) -> bool {
    // Look for a mapping key followed by a newline and then a sequence indicator.
    // Pattern: a line like "key:\n- item" (compact) vs "key:\n  - item" (indented).
    let mut prev_key_indent: Option<usize> = None;

    for line in yaml.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Skip blank lines and comment-only lines — they don't reset the
        // preceding-key state (e.g. `tags:\n  # note\n  - a`).
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Check if this line is a sequence indicator
        if trimmed.starts_with("- ") || trimmed == "-" {
            if let Some(key_indent) = prev_key_indent {
                // Compact: sequence indicator is at the same level as the key
                // Indented: sequence indicator is indented further than the key
                return indent <= key_indent;
            }
            // Sequence without a preceding key — treat as compact
            return true;
        }

        // Check if this line is a mapping key (ends with `:` or `: value`)
        if let Some(colon_pos) = trimmed.find(':') {
            let before_colon = &trimmed[..colon_pos];
            // Basic check: the part before `:` looks like a key (no spaces except in quoted strings)
            if !before_colon.is_empty()
                && !before_colon.starts_with('-')
                && (trimmed.len() == colon_pos + 1 || trimmed.as_bytes()[colon_pos + 1] == b' ')
            {
                // If the value after `:` is empty or only a comment, next line might be a sequence
                let after_colon = trimmed[colon_pos + 1..].trim();
                if after_colon.is_empty() || after_colon.starts_with('#') {
                    prev_key_indent = Some(indent);
                    continue;
                }
            }
        }

        prev_key_indent = None;
    }

    // No sequences found — default to indented (non-compact)
    false
}

// ---------------------------------------------------------------------------
// Shared opening-delimiter policy
// ---------------------------------------------------------------------------

/// UTF-8 byte-order mark. Some editors (Notepad, Excel) prepend this to
/// files; hyalo recognizes it and preserves it verbatim on rewrite rather
/// than treating it as part of the frontmatter delimiter itself.
const BOM: &str = "\u{feff}";

/// Line-ending style of an existing frontmatter block, so a rewrite can keep
/// the block — and thus the whole file — on one consistent style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// What [`opening_delimiter`] reports about a recognized opening `---` line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OpeningDelimiter {
    /// Whether the line was prefixed with a UTF-8 BOM.
    pub(super) has_bom: bool,
    /// The line ending following the `---` (irrelevant at end-of-input).
    pub(super) line_ending: LineEnding,
}

/// Single source of truth for "does this line open a frontmatter block?"
///
/// `line` is the first line of a document, terminator included (`\n`,
/// `\r\n`, or none at end-of-input), optionally prefixed by a single UTF-8
/// BOM. A line opens frontmatter only when — after stripping at most one
/// leading BOM — it is exactly `---` followed by a line terminator or
/// end-of-input. Leading whitespace before `---` is deliberately **not**
/// accepted (e.g. `" ---"` does not open frontmatter): this matches
/// Obsidian/Jekyll and keeps the check unambiguous.
///
/// `extract_frontmatter`, `read_frontmatter_from_reader`, and
/// `find_body_offset` all call this helper instead of hand-rolling their own
/// check, so the read and write paths can never disagree about whether a
/// file has frontmatter — a prior drift between the three caused file
/// corruption on `set`/`remove`/`append` for BOM-prefixed and
/// leading-whitespace files (iter-158 C-1).
pub(super) fn opening_delimiter(line: &str) -> Option<OpeningDelimiter> {
    let (has_bom, rest) = match line.strip_prefix(BOM) {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (dashes, line_ending) = match rest.strip_suffix("\r\n") {
        Some(r) => (r, LineEnding::CrLf),
        None => (rest.strip_suffix('\n').unwrap_or(rest), LineEnding::Lf),
    };
    (dashes == "---").then_some(OpeningDelimiter {
        has_bom,
        line_ending,
    })
}

/// Crate-visible form of [`opening_delimiter`] for sibling modules (the
/// scanner), so every parse path in the crate shares the same
/// opening-delimiter policy and cannot drift from the read/write paths.
pub(crate) fn is_opening_delimiter(line: &str) -> bool {
    opening_delimiter(line).is_some()
}

/// Canonical **closing** frontmatter delimiter policy (iter-183 L-4).
///
/// A closing `---` is recognized **leniently**: the line is a closing
/// delimiter when, after trimming leading and trailing ASCII whitespace, it
/// is exactly `---`. This is deliberately more permissive than the *opening*
/// delimiter ([`opening_delimiter`], which is strict-column-0) because every
/// streaming reader in the crate (`read_frontmatter_from_reader`,
/// `find_body_offset`, `skip_frontmatter`, and the multi-visitor `scanner`)
/// has always closed frontmatter on `line.trim() == "---"`. Consolidating the
/// three lenient sites (plus the scanner and the body-scan loops) onto this
/// single helper guarantees they can never drift, and documents the choice in
/// one place so `find` / `read` / `lint` / `mv` all agree on where a
/// frontmatter block ends — including the indented `  ---` edge case.
///
/// The `line` passed here must already have any trailing `\n` / `\r` stripped
/// (all callers pass a trimmed-of-line-ending slice), but a defensive
/// `trim()` also tolerates raw lines.
pub(crate) fn is_closing_delimiter(line: &str) -> bool {
    line.trim() == "---"
}

/// Represents parsed frontmatter and the remaining body content.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Used in tests only
pub(crate) struct Document {
    properties: IndexMap<String, Value>,
    body: String,
    /// Whether the original YAML used compact list indentation (flush `- item`).
    /// `false` means indented style (`  - item` under its parent key).
    /// Defaults to `false` (indented) when no sequences are present.
    compact_list_indent: bool,
}

#[allow(dead_code)] // All methods used in tests only
impl Document {
    #[must_use]
    pub fn properties(&self) -> &IndexMap<String, Value> {
        &self.properties
    }

    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Parse a markdown document, extracting YAML frontmatter if present.
    /// Returns an error if the file starts with `---` but has no closing delimiter,
    /// which would cause corruption on write (a new frontmatter block on top of an unclosed one).
    pub fn parse(content: &str) -> Result<Self> {
        let (yaml_str, body) = extract_frontmatter(content)?;

        let (properties, compact_list_indent) = match yaml_str {
            Some(yaml) if !yaml.trim().is_empty() => {
                let compact = detect_list_indent_style(yaml);
                let props: IndexMap<String, Value> =
                    serde_saphyr::from_str_with_options(yaml, hyalo_options()).map_err(|e| {
                        anyhow::Error::new(FrontmatterError(format!(
                            "failed to parse YAML frontmatter: {}",
                            friendly_parse_error(&e, MAX_FRONTMATTER_BYTES)
                        )))
                    })?;
                (props, compact)
            }
            _ => (IndexMap::new(), false),
        };

        Ok(Self {
            properties,
            body: body.to_owned(),
            compact_list_indent,
        })
    }

    /// Serialize the document back to a string with YAML frontmatter.
    pub fn serialize(&self) -> Result<String> {
        let mut out = String::new();

        if !self.properties.is_empty() {
            out.push_str("---\n");
            let yaml = serde_saphyr::to_string_with_options(
                &self.properties,
                hyalo_serializer_options(self.compact_list_indent),
            )
            .context("failed to serialize YAML")?;
            out.push_str(&yaml);
            // The YAML serializer adds a trailing newline, but let's ensure
            if !yaml.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("---\n");
        }

        out.push_str(&self.body);
        Ok(out)
    }

    /// Get a property value by name.
    #[must_use]
    pub fn get_property(&self, name: &str) -> Option<&Value> {
        self.properties.get(name)
    }

    /// Set a property value.
    pub fn set_property(&mut self, name: String, value: Value) {
        self.properties.insert(name, value);
    }

    /// Remove a property, returning the old value if it existed.
    pub fn remove_property(&mut self, name: &str) -> Option<Value> {
        self.properties.shift_remove(name)
    }
}

/// Return only the body portion of a markdown document (everything after the YAML frontmatter).
///
/// If the content has no frontmatter block (does not start with `---`), the entire content
/// is returned unchanged. If frontmatter is present but malformed (no closing `---`), the
/// full content string is returned as a fallback so that the caller can still index the file.
pub fn body_only(content: &str) -> &str {
    match extract_frontmatter(content) {
        Ok((_, body)) => body,
        Err(_) => content, // malformed frontmatter: fall back to full content
    }
}

/// Read only the YAML frontmatter from a file, stopping as soon as the closing `---` is found.
/// The body is never read into memory. Use this for read-only property operations.
pub fn read_frontmatter(path: &Path) -> Result<IndexMap<String, Value>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let reader = BufReader::new(file);
    read_frontmatter_from_reader(reader)
}

/// Write updated frontmatter to a file while leaving the body bytes untouched.
///
/// This is the preferred mutation path: it reads only the frontmatter portion, finds
/// the byte offset where the body starts, serializes the new properties, and writes
/// `new_frontmatter + original_body_bytes` back to the file.  The body is never
/// decoded as UTF-8, so there is no risk of re-encoding corruption.
///
/// If `props` is empty (all properties removed), no frontmatter block is written —
/// the file starts directly with the body.
///
/// Writes via the boundary-unchecked [`crate::fs_util::atomic_write`] — a
/// symlinked destination is still followed and replaced, but its target is
/// *not* re-verified against a vault root. Only safe for callers whose path
/// has already passed `discovery::resolve_file`'s canonicalizing boundary
/// check (true of every CLI call site) or that have no vault to check against
/// (e.g. tests). CLI call sites should prefer
/// [`write_frontmatter_within`] (iter-191 follow-up).
pub fn write_frontmatter(path: &Path, props: &IndexMap<String, Value>) -> Result<()> {
    write_frontmatter_impl(None, path, &WriteOp::Props(props)).map(|_| ())
}

/// Like [`write_frontmatter`], but re-checks the vault boundary against the
/// *resolved* destination — the same defense-in-depth
/// [`crate::fs_util::atomic_write_within`] gives every other mutation path.
/// Use this from any CLI call site that has the vault directory in scope.
pub fn write_frontmatter_within(
    vault_root: &Path,
    path: &Path,
    props: &IndexMap<String, Value>,
) -> Result<()> {
    write_frontmatter_impl(Some(vault_root), path, &WriteOp::Props(props)).map(|_| ())
}

/// Read a file's frontmatter as **raw text**, exactly as it sits on disk
/// (iter-266 OUT-1).
///
/// Returns the bytes between the `---` delimiters — delimiters excluded, the
/// file's own line endings, quoting, comments and indentation preserved. This
/// is what a read path should show: re-serializing a parsed map through the
/// YAML writer changes indentation and quote style on a command that changed
/// nothing.
///
/// `Ok(None)` when the file has no frontmatter block, when the block is empty,
/// or when its bytes are not valid UTF-8 (the caller then falls back to its
/// parsed-map rendering). A block whose framing is malformed — an unclosed
/// `---` — is an `Err`, the same parse error [`read_frontmatter`] reports.
pub fn read_frontmatter_raw(path: &Path) -> Result<Option<String>> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let span = find_body_offset(&mut file)?;
    if span.body_offset == 0 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(0))
        .with_context(|| format!("failed to seek in {}", path.display()))?;
    // `find_body_offset` enforces MAX_FRONTMATTER_BYTES/LINES before returning
    // an offset, so this allocation is bounded.
    #[allow(clippy::cast_possible_truncation)]
    let mut fm_bytes = vec![0u8; span.body_offset as usize];
    file.read_exact(&mut fm_bytes)
        .with_context(|| format!("failed to read frontmatter of {}", path.display()))?;
    let Ok(fm_str) = std::str::from_utf8(&fm_bytes) else {
        return Ok(None);
    };
    let opening_prefix = format!(
        "{}---{}",
        if span.has_bom { BOM } else { "" },
        span.line_ending.as_str()
    );
    let after_open = fm_str
        .strip_prefix(opening_prefix.as_str())
        .unwrap_or(fm_str);
    Ok(strip_closing_delimiter(after_open)
        .filter(|yaml| !yaml.is_empty())
        .map(ToOwned::to_owned))
}

/// Rename one top-level frontmatter key **in place**, preserving its position
/// in the block and the exact bytes of its value (iter-266 PROP-1).
///
/// Returns `Ok(true)` when the rename was applied, and `Ok(false)` when the
/// block's shape rules out a text-level rename (no frontmatter, `from` absent,
/// `to` already present, or YAML this splicer does not model). On `Ok(false)`
/// **nothing was written** — the caller decides whether to fall back to a
/// props-based [`write_frontmatter_within`].
pub fn rename_frontmatter_key_within(
    vault_root: &Path,
    path: &Path,
    from: &str,
    to: &str,
) -> Result<bool> {
    write_frontmatter_impl(Some(vault_root), path, &WriteOp::RenameKey { from, to })
}

/// What a frontmatter write is asked to do.
enum WriteOp<'a> {
    /// Make the block express exactly this property map.
    Props(&'a IndexMap<String, Value>),
    /// Rewrite one key token, leaving every other byte alone.
    RenameKey { from: &'a str, to: &'a str },
}

/// Returns whether anything was written (always `true` for
/// [`WriteOp::Props`]; `false` for a rename the splicer could not model).
fn write_frontmatter_impl(
    vault_root: Option<&Path>,
    path: &Path,
    op: &WriteOp<'_>,
) -> Result<bool> {
    // --- Step 0: open the file and guard against unbounded memory use ---
    // Step 2 below reads the whole body into memory; refuse up front rather
    // than let `read_to_end` allocate without bound for a huge file.
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let file_size = file
        .metadata()
        .with_context(|| format!("failed to stat {}", path.display()))?
        .len();
    if file_size > crate::scanner::MAX_FILE_SIZE {
        parse_bail!(
            "refusing to rewrite {}: {} MiB exceeds {} MiB limit",
            path.display(),
            file_size / (1024 * 1024),
            crate::scanner::MAX_FILE_SIZE / (1024 * 1024)
        );
    }

    // --- Step 1: find the byte offset where the body starts, detect indent
    // style, and capture the original YAML text for the minimal-diff splice ---
    let span = find_body_offset(&mut file)?;

    let mut compact_list_indent = false;
    // The frontmatter's original YAML text (delimiters excluded), kept so that
    // `splice_frontmatter` can re-emit untouched keys byte-for-byte (iter-214).
    let mut original_yaml: Option<String> = None;
    // Set when the original block exists but cannot be spliced; the user is
    // warned before the whole block is re-serialized.
    let mut splice_blocked: Option<FallbackReason> = None;

    if span.body_offset > 0 {
        file.seek(SeekFrom::Start(0))
            .with_context(|| format!("failed to seek in {}", path.display()))?;
        // body_offset is the byte position within the file; on 32-bit targets a
        // frontmatter section larger than 4 GiB would truncate here, but that is
        // unreachable in practice.
        #[allow(clippy::cast_possible_truncation)]
        let mut fm_bytes = vec![0u8; span.body_offset as usize];
        file.read_exact(&mut fm_bytes)
            .with_context(|| format!("failed to read frontmatter of {}", path.display()))?;
        let is_utf8 = std::str::from_utf8(&fm_bytes).is_ok();
        let fm_str = String::from_utf8_lossy(&fm_bytes);
        // Extract just the YAML content between the --- delimiters, using the
        // exact opening prefix `find_body_offset` recognized (BOM + line ending).
        let opening_prefix = format!(
            "{}---{}",
            if span.has_bom { BOM } else { "" },
            span.line_ending.as_str()
        );
        let after_open = fm_str
            .strip_prefix(opening_prefix.as_str())
            .unwrap_or(&fm_str);
        let yaml_content = strip_closing_delimiter(after_open);
        compact_list_indent = detect_list_indent_style(yaml_content.unwrap_or(after_open));

        match yaml_content {
            // Invalid UTF-8 in the frontmatter: `fm_str` is lossy, so splicing
            // it would write replacement characters. Re-serialize instead.
            _ if !is_utf8 => splice_blocked = Some(FallbackReason::NotUtf8),
            // iter-219 NEW-7: the block mixes line-ending styles per line.
            // Splicing (and the CRLF re-expansion below) both assume one
            // consistent style for the whole block, so silently proceeding
            // would coerce every line to `span.line_ending`'s style without
            // telling the user their `\r`s just moved or vanished. Take the
            // full-serialization path with an explicit warning instead —
            // still a normalization, but an announced one (DEC-081/DEC-086).
            _ if span.mixed_line_endings => {
                splice_blocked = Some(FallbackReason::MixedLineEndings);
            }
            // An empty block (`---\n---`) has nothing to preserve — take the
            // full-serialization path silently.
            Some(yaml) if yaml.trim().is_empty() => {}
            Some(yaml) => original_yaml = Some(yaml.to_owned()),
            None => splice_blocked = Some(FallbackReason::NotSpanMappable),
        }
    }

    // --- Step 2: read the body bytes from that offset ---
    file.seek(SeekFrom::Start(span.body_offset))
        .with_context(|| format!("failed to seek in {}", path.display()))?;
    let mut body_bytes = Vec::new();
    file.read_to_end(&mut body_bytes)
        .with_context(|| format!("failed to read body of {}", path.display()))?;
    drop(file);

    // --- Step 3: serialize new frontmatter ---
    let mut out: Vec<u8> = Vec::new();
    // Preserve a leading BOM the original file had. When `body_offset == 0`
    // (no recognized frontmatter) any BOM is already part of `body_bytes`
    // untouched; this only matters when frontmatter was recognized and thus
    // excluded from `body_bytes`.
    if span.has_bom {
        out.extend_from_slice(BOM.as_bytes());
    }
    // The YAML block to write, or `None` when the file must end up with no
    // frontmatter at all (an empty property map).
    let yaml_out: Option<String> = match op {
        WriteOp::Props(props) if props.is_empty() => None,
        WriteOp::Props(props) => {
            // Minimal-diff write (iter-214): re-emit every key whose value did
            // not change byte-for-byte and serialize only what actually
            // changed. Falls back to a full re-serialization — with a warning —
            // when the original block cannot be mapped to per-key line spans.
            let spliced = match original_yaml
                .as_deref()
                .map(|orig| splice_frontmatter(orig, props, compact_list_indent))
            {
                Some(SpliceOutcome::Spliced(yaml)) => Some(yaml),
                Some(SpliceOutcome::Fallback(reason)) => {
                    warn_full_frontmatter_rewrite(path, reason);
                    None
                }
                None => {
                    if let Some(reason) = splice_blocked {
                        warn_full_frontmatter_rewrite(path, reason);
                    }
                    None
                }
            };
            Some(match spliced {
                Some(yaml) => yaml,
                None => serde_saphyr::to_string_with_options(
                    props,
                    hyalo_serializer_options(compact_list_indent),
                )
                .context("failed to serialize YAML")?,
            })
        }
        // iter-266 PROP-1: a key rename is a text edit, never a re-serialize.
        // When the block's shape rules it out we write nothing and let the
        // caller decide — silently, because the caller's fallback write does
        // its own warning.
        WriteOp::RenameKey { from, to } => {
            let Some(renamed) = original_yaml
                .as_deref()
                .and_then(|orig| rename_key_in_place(orig, from, to))
            else {
                return Ok(false);
            };
            Some(renamed)
        }
    };

    if let Some(mut yaml) = yaml_out {
        if !yaml.ends_with('\n') {
            yaml.push('\n');
        }
        // Match the original frontmatter's line ending so the block (and
        // thus the whole file) doesn't end up with mixed CRLF/LF lines. Do
        // this before the budget check below so the check sees the exact
        // bytes about to be written — otherwise a YAML of exactly
        // MAX_FRONTMATTER_BYTES could pass the check yet be written larger.
        let eol = span.line_ending.as_str();
        if eol == "\r\n" {
            yaml = yaml.replace('\n', "\r\n");
        }

        // Pre-flight budget check: reject before touching the file.
        check_frontmatter_size_budget(&yaml, path).map_err(anyhow::Error::new)?;

        out.extend_from_slice(b"---");
        out.extend_from_slice(eol.as_bytes());
        out.extend_from_slice(yaml.as_bytes());
        out.extend_from_slice(b"---");
        // iter-219 NEW-16a: a file whose last bytes are literally `---` with
        // no trailing newline (no body, closing delimiter unterminated)
        // must round-trip that way — not gain a newline it never had. Any
        // other shape (there's a body, or the original had a newline here)
        // keeps the separator as before.
        if !body_bytes.is_empty() || span.closing_has_trailing_newline {
            out.extend_from_slice(eol.as_bytes());
        }
    }
    out.extend_from_slice(&body_bytes);

    // --- Step 4: write atomically ---
    match vault_root {
        Some(root) => crate::fs_util::atomic_write_within(root, path, &out),
        None => crate::fs_util::atomic_write(path, &out),
    }
    .with_context(|| format!("failed to write {}", path.display()))?;

    Ok(true)
}

/// A structured error returned when serialized frontmatter would exceed the size budget.
///
/// Returned by [`check_frontmatter_size_budget`] so that callers (write commands)
/// can emit a structured JSON error rather than an opaque anyhow error. Both
/// byte and line dimensions are reported so the user error can identify which
/// limit was crossed when only one of the two is exceeded.
#[derive(Debug)]
pub struct FrontmatterBudgetError {
    pub limit_bytes: usize,
    pub would_be_bytes: usize,
    pub limit_lines: usize,
    pub would_be_lines: usize,
    pub file: String,
}

impl std::fmt::Display for FrontmatterBudgetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.would_be_bytes > self.limit_bytes {
            parts.push(format!(
                "{} bytes > {} byte limit",
                self.would_be_bytes, self.limit_bytes
            ));
        }
        if self.would_be_lines > self.limit_lines {
            parts.push(format!(
                "{} lines > {} line limit",
                self.would_be_lines, self.limit_lines
            ));
        }
        write!(
            f,
            "frontmatter would exceed size budget ({}) in {}",
            parts.join(", "),
            self.file
        )
    }
}

impl std::error::Error for FrontmatterBudgetError {}

/// Check whether `yaml_content` (the YAML between the `---` delimiters, without
/// the delimiters themselves) would exceed the size budget.
///
/// Call this **before** writing serialized frontmatter to disk.  If the check
/// fails, return the [`FrontmatterBudgetError`] — callers should turn it into a
/// structured user error (exit 1) rather than an internal error (exit 2).
///
/// The `path` parameter is used only for the error message.
pub fn check_frontmatter_size_budget(
    yaml_content: &str,
    path: &Path,
) -> std::result::Result<(), FrontmatterBudgetError> {
    let byte_len = yaml_content.len();
    let line_count = yaml_content.lines().count();
    if byte_len > MAX_FRONTMATTER_BYTES || line_count > MAX_FRONTMATTER_LINES {
        return Err(FrontmatterBudgetError {
            limit_bytes: MAX_FRONTMATTER_BYTES,
            would_be_bytes: byte_len,
            limit_lines: MAX_FRONTMATTER_LINES,
            would_be_lines: line_count,
            file: path.display().to_string(),
        });
    }
    Ok(())
}

/// Strip the trailing closing `---` line from a frontmatter block's text.
///
/// `s` is everything after the opening delimiter line up to (and including)
/// the closing delimiter line, i.e. exactly what [`find_body_offset`] framed.
/// Returns the YAML content between the delimiters, or `None` when the last
/// line is not a closing delimiter (which means our framing assumption is
/// wrong and the block must not be spliced).
fn strip_closing_delimiter(s: &str) -> Option<&str> {
    let without_eol = s
        .strip_suffix('\n')
        .map_or(s, |t| t.strip_suffix('\r').unwrap_or(t));
    let last_start = without_eol.rfind('\n').map_or(0, |i| i + 1);
    is_closing_delimiter(&without_eol[last_start..]).then(|| &s[..last_start])
}

/// Warn that a frontmatter write could not be limited to the changed keys.
///
/// iter-214 DEC-081: a full-block rewrite is allowed as a fallback, but never
/// silently — the user must be able to tell reformatting churn from a bug.
fn warn_full_frontmatter_rewrite(path: &Path, reason: FallbackReason) {
    crate::warn::warn(format!(
        "{}: rewriting the entire frontmatter block because {}; formatting of untouched keys may change",
        path.display(),
        reason.as_str()
    ));
}

/// Byte offset and framing of the frontmatter block found by [`find_body_offset`].
struct FrontmatterSpan {
    /// Byte offset where the body starts. `0` means the file has no
    /// frontmatter — the entire file is body.
    body_offset: u64,
    /// Whether the file began with a UTF-8 BOM (only meaningful when
    /// `body_offset > 0`; preserved verbatim on rewrite).
    has_bom: bool,
    /// Line ending used by the opening `---` line (only meaningful when
    /// `body_offset > 0`).
    line_ending: LineEnding,
    /// `true` when at least one line within the block used a different
    /// terminator than `line_ending` (iter-219 NEW-7). A rewrite always
    /// normalizes the whole block to one style; when this is set, that
    /// normalization is a real, user-visible change and must be announced
    /// via [`FallbackReason::MixedLineEndings`] rather than applied
    /// silently.
    mixed_line_endings: bool,
    /// `true` when the closing `---` line itself ends with a newline.
    /// `false` only for a file whose last three bytes are literally `---`
    /// with nothing after them (iter-219 NEW-16a) — rewriting such a file
    /// must not invent a trailing newline that was never there.
    closing_has_trailing_newline: bool,
}

/// `true` when `raw_line` (a line as returned by `BufRead::read_line`,
/// terminator included) ends with the same line-ending style as `expected`.
/// A line with no terminator at all (only possible at EOF) is never a
/// mismatch here — that is a distinct, separately handled concern
/// ([`FrontmatterSpan::closing_has_trailing_newline`]), not a mixed-ending one.
fn line_ending_matches(raw_line: &str, expected: LineEnding) -> bool {
    if !raw_line.ends_with('\n') {
        return true;
    }
    match expected {
        LineEnding::CrLf => raw_line.ends_with("\r\n"),
        LineEnding::Lf => !raw_line.ends_with("\r\n"),
    }
}

/// Find the byte offset in `file` where the body starts (i.e. the byte immediately
/// after the closing `---` line of the frontmatter block), along with the BOM
/// and line-ending style the block was written with.
///
/// Returns `body_offset: 0` if the file has no frontmatter, which means the
/// entire file is body. Uses [`opening_delimiter`] — the exact predicate
/// [`extract_frontmatter`] and [`read_frontmatter_from_reader`] also use — so
/// the read and write paths can never disagree about whether a file has
/// frontmatter.
fn find_body_offset(file: &mut File) -> Result<FrontmatterSpan> {
    let mut reader = BufReader::new(&mut *file);
    let mut line = String::new();

    let no_frontmatter = FrontmatterSpan {
        body_offset: 0,
        has_bom: false,
        line_ending: LineEnding::Lf,
        mixed_line_endings: false,
        closing_has_trailing_newline: true,
    };

    // Peek at the first line.
    let n = reader.read_line(&mut line).context("failed to read line")?;
    if n == 0 {
        return Ok(no_frontmatter);
    }
    let Some(opening) = opening_delimiter(&line) else {
        // No frontmatter — body starts at offset 0
        return Ok(no_frontmatter);
    };

    let mut content_bytes: usize = 0;
    let mut line_count: usize = 0;
    let mut mixed_line_endings = false;
    let closing_has_trailing_newline;

    loop {
        line.clear();
        let n = reader.read_line(&mut line).context("failed to read line")?;
        if n == 0 {
            if line_count == 0 {
                // The opening `---` was the only line in the file (e.g. a
                // file that is exactly `---` or `---\n`) — no content and no
                // closing delimiter follow, so treat this as "no
                // frontmatter" rather than an error. This matches
                // `read_frontmatter_from_reader`'s bare-dash handling; if the
                // two disagreed here, `set`/`remove`/`append` would fail on
                // a file that `read_frontmatter` reports as having no
                // properties at all.
                return Ok(no_frontmatter);
            }
            parse_bail!(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found"
            );
        }
        if !line_ending_matches(&line, opening.line_ending) {
            mixed_line_endings = true;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if is_closing_delimiter(trimmed) {
            // Consumed up to and including the closing `---\n`
            closing_has_trailing_newline = line.ends_with('\n');
            break;
        }
        line_count += 1;
        content_bytes += n;
        if line_count > MAX_FRONTMATTER_LINES || content_bytes > MAX_FRONTMATTER_BYTES {
            parse_bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes); run `hyalo lint <file>` for details"
            );
        }
    }

    // The body offset is whatever position the BufReader is now at
    let pos = reader
        .stream_position()
        .context("failed to get stream position")?;
    Ok(FrontmatterSpan {
        body_offset: pos,
        has_bom: opening.has_bom,
        line_ending: opening.line_ending,
        mixed_line_endings,
        closing_has_trailing_newline,
    })
}

/// Skip past frontmatter in a buffered reader. Returns the number of lines consumed
/// (including the opening and closing `---` delimiters). Returns 0 if no frontmatter is present.
/// The reader is left positioned at the first line after the closing `---`.
pub fn skip_frontmatter<R: BufRead>(reader: &mut R, first_line: &str) -> Result<usize> {
    if opening_delimiter(first_line).is_none() {
        return Ok(0);
    }

    let mut line_count = 1; // count the opening `---`
    let mut total_bytes = 0;
    let mut buf = String::new();
    loop {
        buf.clear();
        let n = reader.read_line(&mut buf).context("failed to read line")?;
        if n == 0 {
            parse_bail!(
                "unclosed frontmatter: file starts with `---` but no closing `---` was found"
            );
        }
        line_count += 1;
        let trimmed = buf.trim_end_matches(['\n', '\r']);
        if is_closing_delimiter(trimmed) {
            break;
        }
        total_bytes += n;
        if line_count - 1 > MAX_FRONTMATTER_LINES || total_bytes > MAX_FRONTMATTER_BYTES {
            parse_bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes); run `hyalo lint <file>` for details"
            );
        }
    }

    Ok(line_count)
}

/// Parse frontmatter from any buffered reader. Stops reading after the closing `---`.
/// Bails out if the frontmatter exceeds a reasonable size (200 lines / 8 KB) to avoid
/// buffering an entire file when the closing delimiter is missing.
///
/// Defense-in-depth: the pre-read line/byte cap is kept even though the parser's
/// `Budget` now enforces its own limits. The pre-read cap stops reading early for
/// files with a missing closing `---`, which the parser budget cannot detect (it
/// only sees the YAML string that was already read).
pub(crate) fn read_frontmatter_from_reader<R: BufRead>(
    reader: R,
) -> Result<IndexMap<String, Value>> {
    let mut lines = reader.lines();

    // First line must open a frontmatter block (see `opening_delimiter`).
    match lines.next() {
        Some(Ok(line)) if opening_delimiter(&line).is_some() => {}
        _ => return Ok(IndexMap::new()),
    }

    let mut yaml = String::new();
    let mut line_count = 0;
    let mut closed = false;
    let mut has_content_lines = false;
    for line in lines {
        has_content_lines = true;
        let line = line.context("failed to read line")?;
        if is_closing_delimiter(&line) {
            closed = true;
            break;
        }
        line_count += 1;
        if line_count > MAX_FRONTMATTER_LINES || yaml.len() + line.len() > MAX_FRONTMATTER_BYTES {
            parse_bail!(
                "frontmatter too large (no closing `---` found within {MAX_FRONTMATTER_LINES} lines / {MAX_FRONTMATTER_BYTES} bytes); run `hyalo lint <file>` for details"
            );
        }
        yaml.push_str(&line);
        yaml.push('\n');
    }

    if !closed {
        // A file whose entire content is exactly `---` (with or without a trailing
        // newline) has no lines after the opening delimiter.  Both `"---"` and `"---\n"`
        // produce zero iterations in the `lines()` loop because the line iterator
        // consumes the terminator but yields nothing more.  This mirrors
        // `extract_frontmatter`'s treatment of those inputs as "no frontmatter".
        if !has_content_lines {
            return Ok(IndexMap::new());
        }
        parse_bail!("unclosed frontmatter: file starts with `---` but no closing `---` was found");
    }

    if yaml.trim().is_empty() {
        return Ok(IndexMap::new());
    }

    serde_saphyr::from_str_with_options(&yaml, hyalo_options()).map_err(|e| {
        anyhow::Error::new(FrontmatterError(format!(
            "failed to parse YAML frontmatter: {}",
            friendly_parse_error(&e, MAX_FRONTMATTER_BYTES)
        )))
    })
}

/// Extract frontmatter YAML string and the body from a markdown document.
/// Returns `Ok((Some(yaml_content), body))` if frontmatter is found,
/// `Ok((None, full_content))` if no frontmatter is present, or an error if the file
/// starts with `---` but has no closing delimiter (which would cause corruption on write).
///
/// Used by both `Document::parse` (tests only) and the public `body_only`.
#[allow(dead_code)] // Also called by body_only; extract_frontmatter itself is exercised via tests.
fn extract_frontmatter(content: &str) -> Result<(Option<&str>, &str)> {
    // Recognize the opening delimiter on the first line — same predicate as
    // `read_frontmatter_from_reader` and `find_body_offset` (see
    // `opening_delimiter`), so this never disagrees with them about whether
    // `content` has frontmatter.
    let first_line_end = content.find('\n').map_or(content.len(), |i| i + 1);
    let first_line = &content[..first_line_end];
    if opening_delimiter(first_line).is_none() {
        return Ok((None, content));
    }

    let after_opening = &content[first_line_end..];
    if after_opening.is_empty() {
        // The opening `---` (optionally BOM-prefixed) was the only line in
        // the document — no content and no closing delimiter follow, so
        // treat as "no frontmatter" (consistent with the streaming reader
        // path's bare-dash handling).
        return Ok((None, content));
    }

    // Find the closing `---` line. `pos` is the byte offset of the start of
    // the delimiter line (which may carry leading whitespace for an indented
    // `  ---`, per the lenient `is_closing_delimiter` policy).
    if let Some(pos) = find_closing_delimiter(after_opening) {
        let yaml = &after_opening[..pos];
        // The body starts just after the delimiter line's terminator. Find the
        // end of the delimiter line rather than assuming it is exactly `---`
        // at `pos`, so an indented `  ---` (or a trailing-space `--- `) does
        // not leave delimiter bytes in the returned body.
        let delim_line = &after_opening[pos..];
        let body = match delim_line.find('\n') {
            Some(nl) => &delim_line[nl + 1..],
            None => "", // delimiter is the last line; no body follows
        };
        Ok((Some(yaml), body))
    } else {
        // Opening `---` found but no closing delimiter — this is malformed frontmatter.
        // Returning an error prevents mutation commands from corrupting the file by
        // writing a new frontmatter block on top of the unclosed one.
        parse_bail!("unclosed frontmatter: file starts with `---` but no closing `---` was found")
    }
}

/// Find the byte offset of the closing `---` line in the given string.
///
/// Shares the closing-delimiter policy with the streaming readers via
/// [`is_closing_delimiter`] (iter-183 L-4): a line is a closing delimiter when
/// its trimmed content is exactly `---`, so an indented `  ---` closes the
/// block here just as it does in `read_frontmatter_from_reader` /
/// `skip_frontmatter` / the scanner. The returned offset points at the first
/// byte of that line (after any leading whitespace is *not* stripped — the
/// caller slices from the line start), and `extract_frontmatter` slices the
/// YAML up to it.
#[allow(dead_code)] // Called by extract_frontmatter, which is used in tests only
fn find_closing_delimiter(s: &str) -> Option<usize> {
    // Walk each line, tracking its start offset, and return the first whose
    // trimmed content is exactly `---`.
    let mut line_start = 0usize;
    loop {
        let line_end = s[line_start..]
            .find('\n')
            .map_or(s.len(), |i| line_start + i);
        let line = &s[line_start..line_end];
        if is_closing_delimiter(line) {
            return Some(line_start);
        }
        if line_end >= s.len() {
            return None;
        }
        line_start = line_end + 1;
    }
}
