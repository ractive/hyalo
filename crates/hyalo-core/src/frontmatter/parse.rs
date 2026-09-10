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

/// Serializer options that preserve the detected list indentation style, and
/// guard against emitting a hazardous block scalar (iter-271 FENCE-2, DEC-293).
///
/// The stock options, except when the value contains a
/// string whose lines include a YAML document marker (`---` or `...`). The
/// serializer's default `prefer_block_scalars` would render such a string as
///
/// ```yaml
/// k: |-
///   a
///   ---
///   b
/// ```
///
/// which is valid YAML but a well-known trap: any reader that closes
/// frontmatter on a *trimmed* `---` (hyalo itself did until Part A of this
/// iteration; Obsidian-adjacent tooling still does) truncates the block there
/// and silently drops every key after it. Turning `prefer_block_scalars` off
/// for exactly those values routes them through the quoting path instead, so
/// the value is written as a double-quoted scalar with escaped newlines
/// (`k: "a\n---\nb"`). That round-trips byte-for-byte through
/// `read_frontmatter` and asks nothing of the user.
///
/// The flag is decided per serialized value, not globally: a document with no
/// such string is emitted exactly as before, so this cannot churn formatting
/// on unrelated files.
pub(super) fn hyalo_serializer_options_for<'a>(
    compact_list_indent: bool,
    values: impl IntoIterator<Item = &'a Value>,
) -> SerializerOptions {
    let mut safe = true;
    let mut quote_all = false;
    for value in values {
        safe &= !has_document_marker_line(value) && !has_trailing_line_whitespace(value);
        quote_all |= has_trailing_line_whitespace(value);
    }
    SerializerOptions {
        compact_list_indent,
        prefer_block_scalars: safe,
        quote_all,
        ..SerializerOptions::default()
    }
}

/// Block-scalar rendering can discard spaces or tabs at physical line ends.
/// Keep such strings quoted so the value requested by a writer survives the
/// normal reader exactly.
fn has_trailing_line_whitespace(value: &Value) -> bool {
    match value {
        Value::String(text) => text.split('\n').any(|line| line.ends_with([' ', '\t'])),
        Value::Array(items) => items.iter().any(has_trailing_line_whitespace),
        Value::Object(map) => map.values().any(has_trailing_line_whitespace),
        _ => false,
    }
}

/// Whether any string inside `value` has a line that YAML would read as a
/// document marker (`---` or `...`) once block-scalar indentation is stripped.
///
/// The trailing-whitespace-tolerant `trim` matches the readers this guards
/// against, which are the lenient ones.
pub(super) fn has_document_marker_line(value: &Value) -> bool {
    match value {
        Value::String(s) => s.lines().any(|line| matches!(line.trim(), "---" | "...")),
        Value::Array(items) => items.iter().any(has_document_marker_line),
        Value::Object(map) => map.values().any(has_document_marker_line),
        _ => false,
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
pub(super) struct OpeningDelimiter<'a> {
    /// Whether the line was prefixed with a UTF-8 BOM.
    pub(super) has_bom: bool,
    /// The line ending following the `---` (irrelevant at end-of-input).
    pub(super) line_ending: LineEnding,
    /// ASCII whitespace between the `---` and the line terminator, kept so a
    /// rewrite reproduces the opener byte for byte (BUG-33/34, iter-276).
    pub(super) trailing_ws: &'a str,
}

/// Single source of truth for "does this line open a frontmatter block?"
///
/// `line` is the first line of a document, terminator included (`\n`,
/// `\r\n`, or none at end-of-input), optionally prefixed by a single UTF-8
/// BOM. A line opens frontmatter only when — after stripping at most one
/// leading BOM — it is exactly `---` followed by a line terminator or
/// end-of-input, optionally followed by ASCII whitespace. Leading whitespace
/// before `---` is deliberately **not** accepted (e.g. `" ---"` does not open
/// frontmatter): this matches Obsidian/Jekyll and keeps the check unambiguous.
///
/// *Trailing* whitespace is accepted (BUG-34, iter-276, amending DEC-293).
/// `--- ` is a valid YAML directives-end marker, [`is_closing_delimiter`] has
/// always tolerated it, and the asymmetry meant a file opening with `--- ` was
/// read as having no frontmatter at all — so `set` prepended a *second* block
/// above the one already there. The tolerated bytes are carried in
/// [`OpeningDelimiter::trailing_ws`] and re-emitted verbatim.
///
/// `extract_frontmatter`, `read_frontmatter_from_reader`, and
/// `find_body_offset` all call this helper instead of hand-rolling their own
/// check, so the read and write paths can never disagree about whether a
/// file has frontmatter — a prior drift between the three caused file
/// corruption on `set`/`remove`/`append` for BOM-prefixed and
/// leading-whitespace files (iter-158 C-1).
pub(super) fn opening_delimiter(line: &str) -> Option<OpeningDelimiter<'_>> {
    let (has_bom, rest) = match line.strip_prefix(BOM) {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (dashes, line_ending) = match rest.strip_suffix("\r\n") {
        Some(r) => (r, LineEnding::CrLf),
        None => (rest.strip_suffix('\n').unwrap_or(rest), LineEnding::Lf),
    };
    let trailing_ws = dashes.strip_prefix("---")?;
    (trailing_ws.bytes().all(|b| b == b' ' || b == b'\t')).then_some(OpeningDelimiter {
        has_bom,
        line_ending,
        trailing_ws,
    })
}

/// Crate-visible form of [`opening_delimiter`] for sibling modules (the
/// scanner), so every parse path in the crate shares the same
/// opening-delimiter policy and cannot drift from the read/write paths.
pub(crate) fn is_opening_delimiter(line: &str) -> bool {
    opening_delimiter(line).is_some()
}

/// Canonical **closing** frontmatter delimiter policy (iter-183 L-4,
/// tightened in iter-271 / DEC-293).
///
/// A closing `---` must sit at **column 0**, exactly like the opening
/// delimiter ([`opening_delimiter`]): the line is a closing delimiter when,
/// after trimming *trailing* ASCII whitespace (which absorbs a `\r`, a
/// `\r\n`, and any trailing spaces or tabs), it is exactly `---`. Leading
/// whitespace disqualifies it.
///
/// Until iter-271 this was deliberately lenient (`line.trim() == "---"`), so
/// an indented `  ---` closed the block. That leniency loses: YAML and
/// Obsidian both close only at column 0, so an indented `  ---` *inside* a
/// block scalar
///
/// ```yaml
/// k: |-
///   a
///   ---
///   b
/// after: 1
/// ```
///
/// silently truncated the block — `after` vanished from every read, and the
/// next `set`/`append` spliced a new key over the block scalar's own text.
/// hyalo produced that shape itself (a multi-line value containing `---` was
/// emitted as a block scalar), so the leniency corrupted files hyalo wrote.
/// Strictness costs only genuinely malformed input, which now surfaces as
/// "unclosed frontmatter" / `HYALO005` instead of being silently truncated.
///
/// Every parse path in the crate (`read_frontmatter_from_reader`,
/// `find_body_offset`, `skip_frontmatter`, the multi-visitor `scanner`, the
/// body-scan loops and the splicer) routes through this single helper, so
/// `find` / `read` / `lint` / `mv` can never drift apart on where a
/// frontmatter block ends.
///
/// Callers normally pass a slice with the line ending already stripped, but
/// the trailing-whitespace trim also tolerates a raw line.
pub(crate) fn is_closing_delimiter(line: &str) -> bool {
    line.trim_end() == "---"
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
                hyalo_serializer_options_for(self.compact_list_indent, self.properties.values()),
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
    match super::DocumentFrame::parse(content) {
        Ok(frame) => &content[frame.body_offset()..],
        Err(_) => content, // malformed frontmatter: fall back to full content
    }
}

#[cfg(any(windows, test))]
const WINDOWS_OPEN_ATTEMPTS: usize = 5;

#[cfg(any(windows, test))]
const WINDOWS_OPEN_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(10);

/// Open a frontmatter file, tolerating selected transient failures observed
/// around concurrent atomic replacement on Windows.
///
/// The Windows policy is deliberately narrow and bounded: access-denied and
/// sharing-violation errors get at most five total attempts with 10 ms between
/// failures (40 ms maximum intentional delay). This mitigates a transient
/// replacement race without assuming its exact kernel cause; it is not a
/// general guarantee that every Windows open will succeed. Other platforms
/// pay no retry or sleep cost.
#[cfg(windows)]
fn open_frontmatter_file(path: &Path) -> std::io::Result<File> {
    reject_non_regular(path)?;
    let file = open_with_windows_retry(|| File::open(path), std::thread::sleep)?;
    validate_opened_regular(file, path)
}

#[cfg(not(windows))]
fn open_frontmatter_file(path: &Path) -> std::io::Result<File> {
    reject_non_regular(path)?;
    let file = File::open(path)?;
    validate_opened_regular(file, path)
}

fn reject_non_regular(path: &Path) -> std::io::Result<()> {
    if std::fs::metadata(path)?.is_file() {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing to read non-regular file {}", path.display()),
        ))
    }
}

fn validate_opened_regular(file: File, path: &Path) -> std::io::Result<File> {
    if file.metadata()?.is_file() {
        Ok(file)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("refusing opened non-regular file {}", path.display()),
        ))
    }
}

/// Run the Windows open policy with injected operations so its attempt and
/// wait bounds can be tested deterministically without sleeping.
#[cfg(any(windows, test))]
fn open_with_windows_retry<T>(
    mut open: impl FnMut() -> std::io::Result<T>,
    mut wait: impl FnMut(std::time::Duration),
) -> std::io::Result<T> {
    for _ in 1..WINDOWS_OPEN_ATTEMPTS {
        match open() {
            Ok(file) => return Ok(file),
            Err(err) if matches!(err.raw_os_error(), Some(5 | 32)) => {
                wait(WINDOWS_OPEN_RETRY_DELAY);
            }
            Err(err) => return Err(err),
        }
    }

    open()
}

/// Read only the YAML frontmatter from a file, stopping as soon as the closing `---` is found.
/// The body is never read into memory. Use this for read-only property operations.
pub fn read_frontmatter(path: &Path) -> Result<IndexMap<String, Value>> {
    let file = open_frontmatter_file(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
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
/// The compatibility adapter roots the resolved existing referent at its
/// containing directory, captures the exact source bytes and identity used for
/// rendering, then publishes through an explicit write session. This retains
/// the legacy no-vault symlink-following contract. Callers with a vault root
/// should prefer [`write_frontmatter_within`] for confinement.
pub fn write_frontmatter(path: &Path, props: &IndexMap<String, Value>) -> Result<()> {
    write_frontmatter_impl(None, path, &WriteOp::Props(props)).map(|_| ())
}

/// Like [`write_frontmatter`], but captures and publishes through the supplied
/// vault root. Use this from any caller that has the vault directory in scope.
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
    let mut file = open_frontmatter_file(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    let span = find_body_offset(&mut file)?;
    if span.body_offset == 0 {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(span.content_start))
        .with_context(|| format!("failed to seek in {}", path.display()))?;
    // `find_body_offset` enforces MAX_FRONTMATTER_BYTES/LINES before returning
    // an offset, so this allocation is bounded.
    #[allow(clippy::cast_possible_truncation)]
    let mut fm_bytes = vec![0u8; (span.content_end - span.content_start) as usize];
    file.read_exact(&mut fm_bytes)
        .with_context(|| format!("failed to read frontmatter of {}", path.display()))?;
    let Ok(yaml) = std::str::from_utf8(&fm_bytes) else {
        return Ok(None);
    };
    Ok((!yaml.is_empty()).then(|| yaml.to_owned()))
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
    write_frontmatter_impl_with_before_commit(vault_root, path, op, || Ok(()))
}

fn write_frontmatter_impl_with_before_commit(
    vault_root: Option<&Path>,
    path: &Path,
    op: &WriteOp<'_>,
    before_commit: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    use crate::rooted::{ConfigRoot, Durability, RelativeName, VaultRoot, WriteSession};
    let captured = if let Some(vault_root) = vault_root {
        let root = VaultRoot::new(vault_root)?;
        let relative = path
            .strip_prefix(vault_root)
            .or_else(|_| path.strip_prefix(root.path()))
            .map_or_else(|_| path.to_path_buf(), Path::to_path_buf);
        root.capture(&RelativeName::new(relative)?)?
    } else {
        let referent = dunce::canonicalize(path)
            .with_context(|| format!("failed to resolve {}", path.display()))?;
        let parent = referent
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let leaf = referent
            .file_name()
            .context("frontmatter path has no file name")?;
        ConfigRoot::new(parent)?.capture(&RelativeName::new(leaf)?)?
    };
    let mut file = captured.reader()?;
    let Some(out) = render_frontmatter_impl(&mut file, path, op)? else {
        return Ok(false);
    };
    drop(file);
    before_commit()?;
    let mut session = WriteSession::new(Durability::PerFile);
    let effect = captured.prepare(&out, &session)?.commit(&mut session)?;
    if let Some(error) = effect.finalization_error() {
        anyhow::bail!("frontmatter committed but finalization failed: {error}");
    }
    session.finish().with_context(|| {
        format!(
            "frontmatter committed but finalization failed for {}",
            path.display()
        )
    })?;
    Ok(true)
}

/// Render from the captured source handle without publishing any bytes.
/// Exact emitted YAML is checked using the normal reader's parser budgets.
pub fn render_frontmatter(
    file: &mut File,
    path: &Path,
    props: &IndexMap<String, Value>,
) -> Result<Vec<u8>> {
    render_frontmatter_impl(file, path, &WriteOp::Props(props))?
        .context("property rendering produced no output")
}

/// Render a top-level key rename from an already captured source without
/// publishing it. `None` means the byte-preserving splicer cannot model the
/// source and the caller should render the validated property map instead.
pub fn render_frontmatter_key_rename(
    file: &mut File,
    path: &Path,
    from: &str,
    to: &str,
) -> Result<Option<Vec<u8>>> {
    render_frontmatter_impl(file, path, &WriteOp::RenameKey { from, to })
}

fn render_frontmatter_impl(
    file: &mut File,
    path: &Path,
    op: &WriteOp<'_>,
) -> Result<Option<Vec<u8>>> {
    // Guard the captured source before rendering.
    // Step 2 below reads the whole body into memory; refuse up front rather
    // than let `read_to_end` allocate without bound for a huge file.
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
    let span = find_body_offset(file)?;

    let mut compact_list_indent = false;
    // The frontmatter's original YAML text (delimiters excluded), kept so that
    // `splice_frontmatter` can re-emit untouched keys byte-for-byte (iter-214).
    let mut original_yaml: Option<String> = None;
    // Set when the original block exists but cannot be spliced; the user is
    // warned before the whole block is re-serialized.
    let mut splice_blocked: Option<FallbackReason> = None;

    if span.body_offset > 0 {
        file.seek(SeekFrom::Start(span.content_start))
            .with_context(|| format!("failed to seek in {}", path.display()))?;
        // body_offset is the byte position within the file; on 32-bit targets a
        // frontmatter section larger than 4 GiB would truncate here, but that is
        // unreachable in practice.
        #[allow(clippy::cast_possible_truncation)]
        let mut fm_bytes = vec![0u8; (span.content_end - span.content_start) as usize];
        file.read_exact(&mut fm_bytes)
            .with_context(|| format!("failed to read frontmatter of {}", path.display()))?;
        let is_utf8 = std::str::from_utf8(&fm_bytes).is_ok();
        let fm_str = String::from_utf8_lossy(&fm_bytes);
        let yaml_content = Some(fm_str.as_ref());
        compact_list_indent = detect_list_indent_style(fm_str.as_ref());

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
    let mut fallback_warning = None;
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
                    fallback_warning = Some(reason);
                    None
                }
                None => {
                    if let Some(reason) = splice_blocked {
                        fallback_warning = Some(reason);
                    }
                    None
                }
            };
            Some(match spliced {
                Some(yaml) => yaml,
                None => serde_saphyr::to_string_with_options(
                    props,
                    hyalo_serializer_options_for(compact_list_indent, props.values()),
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
                return Ok(None);
            };
            Some(renamed)
        }
    };

    // Removing the original frame exposes its authored body at byte zero. If
    // that body itself begins with a complete frontmatter-shaped block, a
    // successful write would silently promote ordinary prose between two
    // horizontal rules into metadata. Refuse the ambiguous transformation;
    // the caller retains the original bytes and can edit the separators
    // explicitly if that reinterpretation was intended.
    if yaml_out.is_none() && span.body_offset > 0 {
        let mut body_reader = BufReader::new(std::io::Cursor::new(&body_bytes));
        if super::read_frame(&mut body_reader)
            .is_ok_and(|framed| framed.frame().frontmatter().is_some())
        {
            parse_bail!(
                "refusing to remove the final frontmatter property from {}: the body would be reinterpreted as frontmatter",
                path.display()
            );
        }
    }

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
        out.extend_from_slice(span.opening_trailing_ws.as_bytes());
        out.extend_from_slice(eol.as_bytes());
        out.extend_from_slice(yaml.as_bytes());
        out.extend_from_slice(b"---");
        out.extend_from_slice(span.closing_trailing_ws.as_bytes());
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

    // Validate the complete final document, including the body bytes. This is
    // required even when the last property was removed: an authored body that
    // begins with `---` can otherwise become an unclosed or differently
    // framed frontmatter block after the old block disappears.
    read_frontmatter_from_reader(BufReader::new(std::io::Cursor::new(&out)))
        .with_context(|| format!("rendered document {} is invalid", path.display()))?;
    if let Some(reason) = fallback_warning {
        warn_full_frontmatter_rewrite(path, reason);
    }

    Ok(Some(out))
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
    content_start: u64,
    content_end: u64,
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
    /// ASCII whitespace between the opening `---` and its line terminator.
    /// Re-emitted verbatim so a rewrite touches only the lines the caller
    /// addressed (BUG-34, iter-276).
    opening_trailing_ws: String,
    /// The same for the closing `---` (BUG-33): `--- ` stays `--- `.
    closing_trailing_ws: String,
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
    let no_frontmatter = FrontmatterSpan {
        body_offset: 0,
        content_start: 0,
        content_end: 0,
        has_bom: false,
        line_ending: LineEnding::Lf,
        mixed_line_endings: false,
        closing_has_trailing_newline: true,
        opening_trailing_ws: String::new(),
        closing_trailing_ws: String::new(),
    };

    file.seek(SeekFrom::Start(0))
        .context("failed to seek in file")?;
    let mut reader = BufReader::new(&mut *file);
    let framed = super::read_frame(&mut reader)?;
    let frame = framed.frame();
    let Some(fm) = frame.frontmatter() else {
        return Ok(no_frontmatter);
    };
    let bytes = framed.bytes();
    let opening_trailing_ws = std::str::from_utf8(&bytes[fm.opening_trailing_ws_span()])
        .context("frontmatter opening delimiter is not UTF-8")?
        .to_owned();
    let closing_trailing_ws = std::str::from_utf8(&bytes[fm.closing_trailing_ws_span()])
        .context("frontmatter closing delimiter is not UTF-8")?
        .to_owned();
    let line_ending = match frame.newline_style() {
        super::NewlineStyle::Lf => LineEnding::Lf,
        super::NewlineStyle::CrLf => LineEnding::CrLf,
    };
    #[allow(clippy::cast_possible_truncation)]
    let pos = frame.body_offset() as u64;
    let content = fm.content_span();
    Ok(FrontmatterSpan {
        body_offset: pos,
        content_start: content.start as u64,
        content_end: content.end as u64,
        has_bom: frame.has_bom(),
        line_ending,
        mixed_line_endings: frame
            .diagnostics()
            .contains(&super::FramingDiagnostic::MixedNewlines),
        closing_has_trailing_newline: fm.closing_has_newline(),
        opening_trailing_ws,
        closing_trailing_ws,
    })
}

/// Skip past frontmatter after the caller has consumed its complete first line.
/// `first_line` must contain the authored line ending when one was present.
/// Returns the number of lines consumed (including the opening and closing
/// `---` delimiters), or 0 when no frontmatter is present. The reader is left
/// positioned at the first line after the closing delimiter.
pub fn skip_frontmatter<R: BufRead>(reader: &mut R, first_line: &str) -> Result<usize> {
    let framed = super::frame::read_frame_after_first_line(reader, first_line)?;
    if framed.frame().frontmatter().is_none() {
        return Ok(0);
    }
    let bytes = framed.bytes();
    Ok(memchr::memchr_iter(b'\n', bytes).count() + usize::from(!bytes.ends_with(b"\n")))
}

/// Parse frontmatter from any buffered reader. Stops reading after the closing `---`.
/// Bails out if the frontmatter exceeds a reasonable size (200 lines / 8 KB) to avoid
/// buffering an entire file when the closing delimiter is missing.
///
/// Defense-in-depth: the pre-read line/byte cap is kept even though the parser's
/// `Budget` now enforces its own limits. The pre-read cap stops reading early for
/// files with a missing closing `---`, which the parser budget cannot detect (it
/// only sees the YAML string that was already read).
pub fn read_frontmatter_from_reader<R: BufRead>(mut reader: R) -> Result<IndexMap<String, Value>> {
    let framed = super::read_frame(&mut reader)?;
    let Some(yaml_bytes) = framed.frame().yaml(framed.bytes()) else {
        return Ok(IndexMap::new());
    };
    let yaml = std::str::from_utf8(yaml_bytes).context("frontmatter is not valid UTF-8")?;
    if yaml.trim().is_empty() {
        return Ok(IndexMap::new());
    }

    serde_saphyr::from_str_with_options(yaml, hyalo_options()).map_err(|e| {
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
    let frame = super::DocumentFrame::parse(content)?;
    let yaml = frame
        .frontmatter()
        .and_then(|fm| content.get(fm.content_span()));
    Ok((yaml, &content[frame.body_offset()..]))
}

#[cfg(test)]
mod open_tests {
    use super::{
        WINDOWS_OPEN_ATTEMPTS, WINDOWS_OPEN_RETRY_DELAY, WriteOp, open_with_windows_retry,
        write_frontmatter_impl_with_before_commit,
    };
    use std::cell::{Cell, RefCell};
    use std::io;

    #[test]
    fn public_frontmatter_adapter_refuses_same_length_lost_update() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "---\ntitle: old\nother: aa\n---\nbody\n").unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut props = super::read_frontmatter(&path).unwrap();
        props.insert("title".to_owned(), serde_json::json!("new"));
        let result =
            write_frontmatter_impl_with_before_commit(None, &path, &WriteOp::Props(&props), || {
                std::fs::write(&path, "---\ntitle: old\nother: bb\n---\nbody\n")?;
                std::fs::File::options()
                    .write(true)
                    .open(&path)?
                    .set_times(std::fs::FileTimes::new().set_modified(modified))?;
                Ok(())
            });
        let error = result.expect_err("stale compatibility write must fail");
        assert!(
            error
                .downcast_ref::<crate::rooted::SourceConflict>()
                .is_some(),
            "{error:#}"
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "---\ntitle: old\nother: bb\n---\nbody\n"
        );
    }

    #[test]
    fn windows_open_retries_access_denied_and_sharing_violation_then_succeeds() {
        let attempts = Cell::new(0);
        let waits = RefCell::new(Vec::new());

        let result = open_with_windows_retry(
            || {
                let attempt = attempts.get() + 1;
                attempts.set(attempt);
                match attempt {
                    1 => Err(io::Error::from_raw_os_error(5)),
                    2 => Err(io::Error::from_raw_os_error(32)),
                    _ => Ok(17),
                }
            },
            |delay| waits.borrow_mut().push(delay),
        );

        assert_eq!(result.expect("third open should succeed"), 17);
        assert_eq!(attempts.get(), 3);
        assert_eq!(waits.into_inner(), vec![WINDOWS_OPEN_RETRY_DELAY; 2]);
    }

    #[test]
    fn windows_open_returns_last_error_after_bounded_retries() {
        let attempts = Cell::new(0);
        let waits = Cell::new(0);

        let result: io::Result<()> = open_with_windows_retry(
            || {
                let attempt = attempts.get() + 1;
                attempts.set(attempt);
                let code = if attempt == WINDOWS_OPEN_ATTEMPTS {
                    32
                } else {
                    5
                };
                Err(io::Error::from_raw_os_error(code))
            },
            |_| waits.set(waits.get() + 1),
        );

        let err = result.expect_err("persistent sharing failures must be returned");
        assert_eq!(err.raw_os_error(), Some(32));
        assert_eq!(attempts.get(), WINDOWS_OPEN_ATTEMPTS);
        assert_eq!(waits.get(), WINDOWS_OPEN_ATTEMPTS - 1);
    }

    #[test]
    fn windows_open_returns_unrelated_os_errors_immediately() {
        let attempts = Cell::new(0);
        let waits = Cell::new(0);

        let result: io::Result<()> = open_with_windows_retry(
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::from_raw_os_error(2))
            },
            |_| waits.set(waits.get() + 1),
        );

        let err = result.expect_err("an unrelated OS error must be returned");
        assert_eq!(err.raw_os_error(), Some(2));
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), 0);
    }

    #[test]
    fn windows_open_does_not_retry_permission_denied_without_raw_code() {
        let attempts = Cell::new(0);
        let waits = Cell::new(0);

        let result: io::Result<()> = open_with_windows_retry(
            || {
                attempts.set(attempts.get() + 1);
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"))
            },
            |_| waits.set(waits.get() + 1),
        );

        let err = result.expect_err("a synthetic permission error must be returned");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(err.raw_os_error(), None);
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), 0);
    }

    #[test]
    fn windows_open_first_success_does_not_wait() {
        let attempts = Cell::new(0);
        let waits = Cell::new(0);

        let result = open_with_windows_retry(
            || {
                attempts.set(attempts.get() + 1);
                Ok(23)
            },
            |_| waits.set(waits.get() + 1),
        );

        assert_eq!(result.expect("first open should succeed"), 23);
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), 0);
    }
}
