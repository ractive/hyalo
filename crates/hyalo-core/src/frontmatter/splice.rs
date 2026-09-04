//! Minimal-diff frontmatter writes (iter-214).
//!
//! Every frontmatter mutation (`set`, `remove`, `append`, `properties rename`,
//! `tags`, `types apply`, `lint --fix`) funnels through a single
//! parse → mutate → write cycle.  Until iter-214 that cycle **re-serialized
//! the whole block**: semantically lossless, but textually destructive.
//! Adding one property to a real GitHub Docs `index.md` rewrote 116 of its
//! 198 frontmatter lines (long list items refolded into `>-` block scalars,
//! `'` quote style flipped to `"`).  A one-key change producing a 116-line
//! diff is unreviewable and makes hyalo unusable in any repo where
//! frontmatter is under code review.
//!
//! This module implements the fix: [`splice_frontmatter`] segments the
//! *original raw YAML text* into top-level key spans, re-emits every span
//! whose value did not change **byte for byte**, and serializes only the keys
//! that were actually added or changed.
//!
//! # Safety model
//!
//! Splicing is a text transformation driven by heuristics (where does a
//! top-level key's span start and end?).  Rather than trust those heuristics,
//! every candidate result is **verified before it is returned**: the spliced
//! YAML is re-parsed with the standard `hyalo_options` parser and compared
//! against the exact property map the caller asked to write — same keys, same
//! order, same values.  Anything that does not verify returns
//! [`SpliceOutcome::Fallback`], and the caller re-serializes the whole block
//! and warns.  There is no silent churn, and no path where splicing can write
//! something the full serializer would not have written.

use indexmap::IndexMap;
use serde_json::Value;
use serde_saphyr::{Options, SerializerOptions};

use super::MAX_FRONTMATTER_BYTES;
use super::parse::{hyalo_options, is_closing_delimiter};

/// Why a minimal-diff write could not be performed.
///
/// Surfaced to the user as a warning so that unexpected full-block churn is
/// always explained rather than silent (iter-214 DEC-081).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FallbackReason {
    /// The original frontmatter bytes are not valid UTF-8.
    NotUtf8,
    /// The original frontmatter mixes line endings (a lone `\r`).
    MixedLineEndings,
    /// The original frontmatter is not parseable as a top-level mapping.
    Unparseable,
    /// The raw text could not be split into one span per top-level key
    /// (YAML constructs this splicer deliberately does not model: explicit
    /// `? key` syntax, top-level flow mappings or sequences, directives,
    /// multiple documents).
    NotSpanMappable,
    /// Spans were produced but re-parsing the spliced result did not
    /// reproduce the requested properties exactly.
    VerificationFailed,
    /// Serializing an individual changed key on its own failed.
    SerializeFailed,
    /// A single item was appended to or removed from a flow-style list
    /// (`key: [a, b]`), but the existing list or the new item doesn't fit
    /// this splicer's simple one-token-per-item model — the new item can't
    /// be written as a single inline flow token (e.g. it would require a
    /// multi-line block scalar), or the existing list itself has a shape
    /// the tokenizer can't round-trip (a trailing comment, a nested flow
    /// collection as an item). Iter-219 DEC-081/DEC-087 update: this falls
    /// back rather than silently converting the list to block style (or
    /// dropping a trailing comment) with no explanation.
    FlowListNotModellable,
    /// A single item was appended to or removed from a block-style list
    /// (`key:\n  - a\n  - b\n`), but the existing list has a shape this
    /// splicer's simple one-line-per-item model can't handle — most
    /// commonly a `#`-comment interleaved between items. Splicing through
    /// interleaved comments would need real CST support, which is out of
    /// scope; the honest alternative is to fall back and say so rather
    /// than silently re-serialize the whole list (losing the comment) with
    /// no explanation.
    BlockListNotModellable,
}

impl FallbackReason {
    /// Human-readable explanation, used in the `warning:` line.
    pub(super) fn as_str(self) -> &'static str {
        match self {
            FallbackReason::NotUtf8 => "existing frontmatter is not valid UTF-8",
            FallbackReason::MixedLineEndings => "existing frontmatter mixes line endings",
            FallbackReason::Unparseable => "existing frontmatter is not a plain YAML mapping",
            FallbackReason::NotSpanMappable => {
                "existing frontmatter uses YAML constructs that cannot be mapped to per-key line spans"
            }
            FallbackReason::VerificationFailed => {
                "the minimal-diff result did not round-trip to the requested properties"
            }
            FallbackReason::SerializeFailed => "a changed key could not be serialized on its own",
            FallbackReason::FlowListNotModellable => {
                "the existing flow-style list cannot be edited in place (an unrepresentable item, \
                 or a trailing comment/comma this splicer does not model)"
            }
            FallbackReason::BlockListNotModellable => {
                "the existing list has a comment between its items, which this splicer cannot edit in place"
            }
        }
    }
}

/// Result of attempting a minimal-diff frontmatter rewrite.
#[derive(Debug)]
pub(super) enum SpliceOutcome {
    /// Spliced YAML content (between the `---` delimiters, LF line endings,
    /// trailing newline included). Untouched keys are byte-identical to the
    /// original.
    Spliced(String),
    /// Splicing was not possible; the caller must re-serialize the whole
    /// block and warn the user with this reason.
    Fallback(FallbackReason),
}

/// One top-level key of the original frontmatter, with the exact source text
/// it occupies.
#[derive(Debug)]
struct Segment<'a> {
    key: String,
    value: Value,
    /// Blank/comment lines immediately preceding the key line. Kept with the
    /// key so that `# explains the next key` travels with — or dies with — it.
    pre_trivia: &'a str,
    /// The key line plus every continuation line belonging to it, verbatim.
    body: &'a str,
}

/// How a column-0 line relates to the flat top-level key structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineRole {
    /// Blank line or column-0 comment.
    Trivia,
    /// Starts a new top-level mapping key.
    Key,
    /// Belongs to the preceding key (compact-style sequence item `- x`).
    Continuation,
    /// A construct this splicer does not model — forces a fallback.
    Unsupported,
}

/// Attempt a minimal-diff rewrite of `original_yaml` so that it expresses
/// exactly `props`.
///
/// `original_yaml` is the raw text between the `---` delimiters (delimiters
/// excluded), exactly as it appears on disk — with the file's original line
/// endings. `props` is the full desired property map, in the order it should
/// be written. `compact_list_indent` mirrors the existing serializer option so
/// that newly written keys match the file's list-indentation style.
///
/// On success the returned YAML uses `\n` line endings; the caller converts to
/// CRLF if the file uses it, exactly as it does for full serialization.
pub(super) fn splice_frontmatter(
    original_yaml: &str,
    props: &IndexMap<String, Value>,
    compact_list_indent: bool,
) -> SpliceOutcome {
    // Normalize line endings to LF for the whole operation. A frontmatter
    // block is either LF or CRLF throughout (`find_body_offset` records
    // which); a stray lone `\r` means content the caller's CRLF re-expansion
    // would mangle, so refuse.
    let normalized;
    let yaml = if original_yaml.contains('\r') {
        normalized = original_yaml.replace("\r\n", "\n");
        if normalized.contains('\r') {
            return SpliceOutcome::Fallback(FallbackReason::MixedLineEndings);
        }
        normalized.as_str()
    } else {
        original_yaml
    };

    // The original must itself be a well-formed mapping; otherwise we have no
    // trustworthy baseline to diff against.
    let Ok(original_map) = parse_map(yaml) else {
        return SpliceOutcome::Fallback(FallbackReason::Unparseable);
    };

    let Some((header, segments, footer)) = segment(yaml) else {
        return SpliceOutcome::Fallback(FallbackReason::NotSpanMappable);
    };

    // Cross-check the segmentation against the authoritative parse: same keys,
    // same order, same values. If they disagree, our line spans do not model
    // this document and we must not touch it.
    if segments.len() != original_map.len()
        || !segments
            .iter()
            .zip(original_map.iter())
            .all(|(seg, (key, value))| seg.key == *key && seg.value == *value)
    {
        return SpliceOutcome::Fallback(FallbackReason::NotSpanMappable);
    }

    let mut out = String::with_capacity(yaml.len() + 64);
    out.push_str(header);
    for (key, new_value) in props {
        match segments.iter().find(|seg| seg.key == *key) {
            // Unchanged: emit the original bytes, untouched.
            Some(seg) if seg.value == *new_value => {
                out.push_str(seg.pre_trivia);
                out.push_str(seg.body);
            }
            // Changed: keep the key's comment block, re-serialize its value.
            Some(seg) => {
                out.push_str(seg.pre_trivia);
                match render_changed_segment(key, seg, new_value, compact_list_indent) {
                    Ok(text) => out.push_str(&text),
                    Err(reason) => return SpliceOutcome::Fallback(reason),
                }
            }
            // New key: emit it at the position `props` asks for.
            None => {
                let Some(rendered) = serialize_one(key, new_value, compact_list_indent) else {
                    return SpliceOutcome::Fallback(FallbackReason::SerializeFailed);
                };
                out.push_str(&rendered);
            }
        }
    }
    out.push_str(footer);

    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }

    // Final gate: the spliced text must parse back to exactly what the caller
    // asked to write. This is what makes every heuristic above safe.
    match parse_map_with(&out, verify_options()) {
        Ok(round_tripped) if map_eq(&round_tripped, props) => SpliceOutcome::Spliced(out),
        _ => SpliceOutcome::Fallback(FallbackReason::VerificationFailed),
    }
}

/// Rename one top-level key **in the raw text**, touching nothing but the key
/// token itself (iter-266 PROP-1).
///
/// `properties rename` used to go through the ordinary props write path, which
/// meant the renamed key was removed and re-inserted: it moved to the end of
/// the block and its value was re-serialized, so an empty `rating:` came back
/// as `score: null`. This function instead rewrites the key token on its own
/// line and re-emits every other byte — including the value text, its quoting,
/// comments and block-list indentation — verbatim.
///
/// `original_yaml` is the raw text between the `---` delimiters. Returns `None`
/// when the block cannot be modeled (the caller then falls back to a
/// props-based write), when `from` is absent, or when `to` already exists.
pub(super) fn rename_key_in_place(original_yaml: &str, from: &str, to: &str) -> Option<String> {
    if from == to {
        return None;
    }
    // Same LF normalization as `splice_frontmatter`: a lone `\r` is content the
    // caller's CRLF re-expansion would mangle, so refuse to touch the block.
    let normalized;
    let yaml = if original_yaml.contains('\r') {
        normalized = original_yaml.replace("\r\n", "\n");
        if normalized.contains('\r') {
            return None;
        }
        normalized.as_str()
    } else {
        original_yaml
    };

    let original_map = parse_map(yaml).ok()?;
    if !original_map.contains_key(from) || original_map.contains_key(to) {
        return None;
    }

    let (header, segments, footer) = segment(yaml)?;
    // Same cross-check `splice_frontmatter` performs: if the line spans do not
    // reproduce the authoritative parse, this document is not ours to edit.
    if segments.len() != original_map.len()
        || !segments
            .iter()
            .zip(original_map.iter())
            .all(|(seg, (key, value))| seg.key == *key && seg.value == *value)
    {
        return None;
    }

    let key_text = render_key_token(to);
    let mut out = String::with_capacity(yaml.len() + key_text.len());
    out.push_str(header);
    for seg in &segments {
        out.push_str(seg.pre_trivia);
        if seg.key == from {
            let first_line_end = seg.body.find('\n').unwrap_or(seg.body.len());
            let token_len = key_token_len(&seg.body[..first_line_end])?;
            out.push_str(&key_text);
            out.push_str(&seg.body[token_len..]);
        } else {
            out.push_str(seg.body);
        }
    }
    out.push_str(footer);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }

    // Same final gate as `splice_frontmatter`: the result must parse back to
    // the original map with exactly one key renamed, in the same position.
    let expected: IndexMap<String, Value> = original_map
        .into_iter()
        .map(|(k, v)| if k == from { (to.to_owned(), v) } else { (k, v) })
        .collect();
    match parse_map_with(&out, verify_options()) {
        Ok(round_tripped) if map_eq(&round_tripped, &expected) => Some(out),
        _ => None,
    }
}

/// Render `key` as a YAML mapping-key token (no `:` separator).
///
/// Emits the key bare when it is unambiguously a plain scalar, and
/// double-quoted otherwise. A wrong guess cannot corrupt a file: the caller
/// re-parses the spliced result and falls back when it does not match.
fn render_key_token(key: &str) -> String {
    if is_plain_safe_key(key) {
        return key.to_owned();
    }
    let mut out = String::with_capacity(key.len() + 2);
    out.push('"');
    for c in key.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `true` when `key` can be written as a bare YAML scalar key.
///
/// Deliberately conservative — anything not obviously plain gets quoted.
fn is_plain_safe_key(key: &str) -> bool {
    if key.is_empty() || key.trim() != key {
        return false;
    }
    // Would re-read as a bool / null rather than as this string.
    const RESERVED: &[&str] = &[
        "true", "false", "yes", "no", "on", "off", "null", "~", "y", "n",
    ];
    if RESERVED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(key.trim_start_matches('-')))
    {
        return false;
    }
    // A key that parses as a number would come back as a non-string key.
    if key.parse::<f64>().is_ok() {
        return false;
    }
    if key
        .chars()
        .next()
        .is_some_and(|c| "-?:,[]{}#&*!|>'\"%@`".contains(c))
    {
        return false;
    }
    key.chars()
        .all(|c| c.is_alphanumeric() || " _-./+()".contains(c))
        && !key.contains(": ")
        && !key.contains(" #")
}

/// Parse YAML text into an ordered property map using hyalo's shared parser
/// options (strict booleans, no anchors/aliases, duplicate keys rejected).
///
/// Used for the *baseline* parse of the on-disk block, so that the splicer
/// sees exactly what the read path saw.
fn parse_map(yaml: &str) -> Result<IndexMap<String, Value>, ()> {
    parse_map_with(yaml, hyalo_options())
}

/// Parse YAML text for the post-splice verification pass.
///
/// Same hardening as [`hyalo_options`] where it matters (no aliases, no
/// anchors, one document, duplicate keys rejected) but with node and scalar
/// budgets scaled to the frontmatter size limit rather than to the much
/// tighter read-path defaults. A caller is allowed to *write* a value larger
/// than the read-path scalar budget — the size-budget pre-flight in
/// `write_frontmatter_impl` is what rejects those — and verification must not
/// mistake that for a splicing failure and warn about churn that did not
/// happen.
fn verify_options() -> Options {
    let base = hyalo_options();
    let budget = base.budget.map(|b| serde_saphyr::Budget {
        max_events: 200_000,
        max_nodes: 100_000,
        max_total_scalar_bytes: MAX_FRONTMATTER_BYTES * 2,
        ..b
    });
    Options { budget, ..base }
}

fn parse_map_with(yaml: &str, options: Options) -> Result<IndexMap<String, Value>, ()> {
    if yaml.trim().is_empty() {
        return Ok(IndexMap::new());
    }
    serde_saphyr::from_str_with_options::<IndexMap<String, Value>>(yaml, options).map_err(|_| ())
}

/// Order-sensitive map equality (`IndexMap`'s `PartialEq` ignores order).
fn map_eq(a: &IndexMap<String, Value>, b: &IndexMap<String, Value>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|((ak, av), (bk, bv))| ak == bk && av == bv)
}

/// Serialize a single `key: value` pair as standalone YAML.
fn serialize_one(key: &str, value: &Value, compact_list_indent: bool) -> Option<String> {
    let mut single: IndexMap<&str, &Value> = IndexMap::with_capacity(1);
    single.insert(key, value);
    let opts = SerializerOptions {
        compact_list_indent,
        ..SerializerOptions::default()
    };
    let mut yaml = serde_saphyr::to_string_with_options(&single, opts).ok()?;
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    Some(yaml)
}

// ---------------------------------------------------------------------------
// List splicing (iter-219 NEW-5)
// ---------------------------------------------------------------------------
//
// `set` replacing a whole list value already had to re-serialize that key's
// span (the general "Changed" case above). But `append`/`remove <key>=<value>`
// only ever change a list by exactly one item — appending one, or deleting
// one — and DEC-080's own bar ("touch only what changed") applies just as
// much *within* a key's span as it does across keys. This section detects
// that specific shape (old/new values are both arrays differing by exactly
// one item, all items are plain scalars) and, when the body text matches the
// simple one-item-per-line model these functions assume, edits just that
// item's line(s) instead of re-serializing the whole list. Any shape this
// doesn't recognize — nested items, multi-line flow, unusual indentation —
// falls through to the existing whole-key re-serialize; nothing here can
// produce an outcome that isn't first caught by `splice_frontmatter`'s
// re-parse-and-compare verification gate.

/// Render the replacement text for a single changed key: `key`'s new span,
/// ready to append to the output (no `pre_trivia`; the caller already wrote
/// that). Prefers a minimal single-item list splice when the value's shape
/// allows it; otherwise falls back to serializing the whole key.
fn render_changed_segment(
    key: &str,
    seg: &Segment<'_>,
    new_value: &Value,
    compact_list_indent: bool,
) -> Result<String, FallbackReason> {
    if let (Value::Array(old_items), Value::Array(new_items)) = (&seg.value, new_value)
        && old_items.iter().all(is_inline_scalar)
        && new_items.iter().all(is_inline_scalar)
        && let Some(delta) = classify_list_delta(old_items, new_items)
    {
        match try_list_splice(seg.body, old_items, new_items, &delta) {
            ListSpliceResult::Spliced(text) => return Ok(text),
            ListSpliceResult::FlowNotModellable => {
                return Err(FallbackReason::FlowListNotModellable);
            }
            ListSpliceResult::BlockNotModellable => {
                return Err(FallbackReason::BlockListNotModellable);
            }
            ListSpliceResult::NotApplicable => {}
        }
    }
    serialize_one(key, new_value, compact_list_indent).ok_or(FallbackReason::SerializeFailed)
}

/// `true` for scalar YAML values (string/number/bool/null) — the only shapes
/// the one-line-per-item splice model below can reason about. A `false`
/// anywhere in a list (a nested mapping or sequence item) routes the whole
/// key through the existing whole-value re-serialize.
fn is_inline_scalar(v: &Value) -> bool {
    !matches!(v, Value::Array(_) | Value::Object(_))
}

/// What changed between an old and new list value, when the change is
/// exactly one item.
enum ListDelta {
    /// `new_items` is `old_items` with one item appended at the end.
    Append,
    /// `old_items[.0]` was removed; every other item kept its order.
    Remove(usize),
}

/// Classify a list value change as an append-one or remove-one delta.
/// Returns `None` for anything else (replacement, reorder, multi-item
/// change) — the caller falls back to re-serializing the whole value, which
/// is what `set` needs anyway.
fn classify_list_delta(old: &[Value], new: &[Value]) -> Option<ListDelta> {
    if new.len() == old.len() + 1 && new[..old.len()] == *old {
        return Some(ListDelta::Append);
    }
    if old.len() == new.len() + 1 {
        let idx = old
            .iter()
            .zip(new.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(new.len());
        let mut ni = 0usize;
        for (i, item) in old.iter().enumerate() {
            if i == idx {
                continue;
            }
            if new.get(ni) != Some(item) {
                return None;
            }
            ni += 1;
        }
        if ni == new.len() {
            return Some(ListDelta::Remove(idx));
        }
    }
    None
}

/// Result of attempting a single-item list splice.
enum ListSpliceResult {
    /// The key's new span text, ready to write.
    Spliced(String),
    /// The body *is* a single-line flow list, but either the existing list
    /// or the new item doesn't fit the tokenizer's simple model — must warn
    /// (DEC-081/DEC-087 fallback), never silently reformat to block style.
    FlowNotModellable,
    /// The body *is* a block-sequence-shaped key (bare `key:` followed by
    /// item lines), but those lines don't cleanly split one-per-item — most
    /// commonly a `#`-comment between items — must warn, never silently
    /// re-serialize (which would drop the comment with no explanation).
    BlockNotModellable,
    /// The body's shape doesn't match either model at all (e.g. it isn't a
    /// list-shaped value in the first place); the caller falls back to a
    /// whole-key re-serialize exactly as it always has for other changed
    /// values — not new churn, so no warning is needed here.
    NotApplicable,
}

/// Try to splice `delta` into `body` (the key's raw source text, including
/// its own key line). `old_items.len()` is used to cross-check that the raw
/// text's line count agrees with the parsed value before trusting it.
fn try_list_splice(
    body: &str,
    old_items: &[Value],
    new_items: &[Value],
    delta: &ListDelta,
) -> ListSpliceResult {
    match *delta {
        ListDelta::Remove(idx) => {
            if let Some(text) = remove_block_item(body, old_items.len(), idx) {
                return ListSpliceResult::Spliced(text);
            }
            if is_single_line_flow(body) {
                return match splice_flow_list(body, old_items.len(), FlowOp::Remove(idx)) {
                    Some(text) => ListSpliceResult::Spliced(text),
                    None => ListSpliceResult::FlowNotModellable,
                };
            }
            if is_unmodellable_block_list(body, old_items.len()) {
                return ListSpliceResult::BlockNotModellable;
            }
            ListSpliceResult::NotApplicable
        }
        ListDelta::Append => {
            let Some(new_item) = new_items.last() else {
                return ListSpliceResult::NotApplicable;
            };
            // Rendered eagerly (not just on the block-list path) because a
            // flow list that can't accept this item inline must still warn
            // even when the item itself is the whole problem — checking
            // `is_single_line_flow` first, below, is what makes that
            // reachable rather than bailing out here.
            let item_text = render_scalar_item(new_item);
            if is_single_line_flow(body) {
                return match item_text
                    .as_deref()
                    .and_then(|text| splice_flow_list(body, old_items.len(), FlowOp::Append(text)))
                {
                    Some(text) => ListSpliceResult::Spliced(text),
                    None => ListSpliceResult::FlowNotModellable,
                };
            }
            if let Some(item_text) = &item_text
                && let Some(text) = append_block_item(body, old_items.len(), item_text)
            {
                return ListSpliceResult::Spliced(text);
            }
            if is_unmodellable_block_list(body, old_items.len()) {
                return ListSpliceResult::BlockNotModellable;
            }
            ListSpliceResult::NotApplicable
        }
    }
}

/// `true` when `body`'s key line is a bare `key:` (block-sequence shaped,
/// i.e. not flow, not a scalar) with at least one line after it, but
/// [`append_block_item`]/[`remove_block_item`] already failed to model it —
/// almost always a `#`-comment interleaved between item lines. Distinguishes
/// "this genuinely isn't (or isn't yet) a block list" (silent, existing
/// `NotApplicable`) from "this IS one, just not one this splicer can edit
/// in place" (must warn, iter-219 M6/DEC-081).
fn is_unmodellable_block_list(body: &str, item_count: usize) -> bool {
    if item_count == 0 {
        // `key:` with nothing after parses as null, not `[]` — a real
        // array value with zero items is never block-sequence-shaped, so
        // failing to model it here isn't this case.
        return false;
    }
    let mut lines = body.split_inclusive('\n');
    let Some(key_line) = lines.next() else {
        return false;
    };
    if !key_line.trim_end().ends_with(':') {
        return false;
    }
    lines.next().is_some()
}

/// Render `value` as the token text for one YAML sequence item — no leading
/// `- `, no trailing newline. Returns `None` when the value can't be
/// rendered as a single-line token (e.g. it forces a multi-line block
/// scalar), which rules out both block and flow inlining.
fn render_scalar_item(value: &Value) -> Option<String> {
    let seq = [value];
    let opts = SerializerOptions {
        compact_list_indent: true,
        ..SerializerOptions::default()
    };
    let mut yaml = serde_saphyr::to_string_with_options(&seq, opts).ok()?;
    if !yaml.ends_with('\n') {
        yaml.push('\n');
    }
    let line = yaml.strip_prefix("- ")?.trim_end_matches('\n');
    if line.is_empty() || line.contains('\n') {
        return None;
    }
    Some(line.to_owned())
}

/// Split a block-sequence item line into `(indent, "- " + rest)`'s indent
/// prefix and the text following `"- "`. `None` if `line` isn't a
/// space-indented dash item.
fn split_dash_indent(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start_matches(' ');
    let indent_len = line.len() - trimmed.len();
    let rest = trimmed.strip_prefix("- ")?;
    Some((&line[..indent_len], rest))
}

/// Split off a single trailing `\n` (splice operates on LF-normalized text).
fn split_trailing_eol(line: &str) -> (&str, &str) {
    line.strip_suffix('\n').map_or((line, ""), |s| (s, "\n"))
}

/// Append one item to a block-style sequence (`key:\n  - a\n  - b\n`).
///
/// `item_count` is the number of items the authoritative parse found;
/// `body` must have exactly that many dash-item lines, all sharing the same
/// indentation, or this returns `None` (the shape doesn't match the simple
/// model and the caller falls back to a whole-key re-serialize).
fn append_block_item(body: &str, item_count: usize, new_item_text: &str) -> Option<String> {
    if item_count == 0 {
        // An empty block sequence isn't representable (`key:` alone parses
        // as null, not `[]`) — this must be a flow `key: []` instead.
        return None;
    }
    let mut lines = body.split_inclusive('\n');
    let key_line = lines.next()?;
    // Trailing whitespace before the newline (`key: \n`) is common in the
    // wild ahead of a block sequence and insignificant to YAML — trim all
    // trailing whitespace, not just the line terminator, before checking.
    if !key_line.trim_end().ends_with(':') {
        return None;
    }

    let item_lines: Vec<&str> = lines.collect();
    if item_lines.len() != item_count {
        return None;
    }

    let mut indent: Option<&str> = None;
    for line in &item_lines {
        let stripped = line.trim_end_matches(['\n', '\r']);
        let (ind, _) = split_dash_indent(stripped)?;
        match indent {
            Some(prev) if prev != ind => return None,
            Some(_) => {}
            None => indent = Some(ind),
        }
    }
    let indent = indent?;

    let mut out = body.to_owned();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(indent);
    out.push_str("- ");
    out.push_str(new_item_text);
    out.push('\n');
    Some(out)
}

/// Remove one item from a block-style sequence by dropping its line
/// wholesale. Every other line — including its exact indentation and dash
/// style — is untouched.
fn remove_block_item(body: &str, item_count: usize, removed_index: usize) -> Option<String> {
    let mut lines = body.split_inclusive('\n');
    let key_line = lines.next()?;
    // Trailing whitespace before the newline (`key: \n`) is common in the
    // wild ahead of a block sequence and insignificant to YAML — trim all
    // trailing whitespace, not just the line terminator, before checking.
    if !key_line.trim_end().ends_with(':') {
        return None;
    }

    let item_lines: Vec<&str> = lines.collect();
    if item_lines.len() != item_count || removed_index >= item_lines.len() {
        return None;
    }
    for line in &item_lines {
        let stripped = line.trim_end_matches(['\n', '\r']);
        split_dash_indent(stripped)?;
    }

    let mut out = String::with_capacity(body.len());
    out.push_str(key_line);
    for (i, line) in item_lines.iter().enumerate() {
        if i != removed_index {
            out.push_str(line);
        }
    }
    Some(out)
}

/// `true` when `body` is a single-line `key: [...]` flow list — used to
/// decide whether an append that couldn't be inlined deserves the
/// `FlowNotInlineable` warning (this *is* a flow list) versus silent
/// fallback (it isn't one at all).
fn is_single_line_flow(body: &str) -> bool {
    let mut lines = body.split_inclusive('\n');
    let Some(line) = lines.next() else {
        return false;
    };
    if lines.next().is_some() {
        return false;
    }
    let (content, _) = split_trailing_eol(line);
    let Some(colon) = content.find(':') else {
        return false;
    };
    content[colon + 1..].trim_start().starts_with('[')
}

/// Bracket byte positions and item spans of a single-line `key: [...]` flow
/// list. Each span in `items` is a byte range into the parsed `content`
/// string, relative to the start of the bracket interior (i.e. relative to
/// `open_pos + 1`).
struct FlowListLayout {
    open_pos: usize,
    close_pos: usize,
    items: Vec<(usize, usize)>,
}

/// Parse a single-line `key: [...]` body into its bracket layout. An empty
/// list normalizes to zero item spans. `None` when the line isn't this exact
/// shape, or an item contains a nested `[`/`{` (outside this splicer's
/// simple model).
fn parse_flow_list(content: &str) -> Option<FlowListLayout> {
    let colon = content.find(':')?;
    let value_part = content[colon + 1..].trim_start();
    if !value_part.starts_with('[') || !value_part.ends_with(']') {
        return None;
    }
    let open_pos = content.find('[')?;
    let close_pos = content.rfind(']')?;
    if open_pos >= close_pos {
        return None;
    }
    let inner = &content[open_pos + 1..close_pos];
    let items = split_flow_items(inner)?;
    let items = if items.len() == 1 && inner[items[0].0..items[0].1].trim().is_empty() {
        Vec::new()
    } else {
        items
    };
    Some(FlowListLayout {
        open_pos,
        close_pos,
        items,
    })
}

/// Split flow-sequence interior text into comma-separated item spans,
/// respecting single/double-quoted strings. `None` if an item contains a
/// nested `[` or `{` — flow collections nested inside a flow list item are
/// outside this splicer's simple model.
fn split_flow_items(inner: &str) -> Option<Vec<(usize, usize)>> {
    let bytes = inner.as_bytes();
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'[' | b'{' => return None,
            b',' => {
                items.push((start, i));
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    items.push((start, bytes.len()));
    Some(items)
}

/// Detect the separator style (`", "` vs `","`) already used between the
/// first two items of a flow list, defaulting to `", "` for zero/one items.
fn detect_flow_separator(inner: &str, items: &[(usize, usize)]) -> &'static str {
    if items.len() < 2 {
        return ", ";
    }
    let first_end = items[0].1;
    if inner.as_bytes().get(first_end + 1) == Some(&b' ') {
        ", "
    } else {
        ","
    }
}

/// One-item mutation to apply to a flow list's token set.
// `Copy` isn't just nice-to-have here: clippy's `needless_pass_by_value`
// requires it (or an `&FlowOp` signature) for `splice_flow_list`'s
// by-value `op` parameter — checked directly, not left to guesswork.
#[derive(Clone, Copy)]
enum FlowOp<'a> {
    Append(&'a str),
    Remove(usize),
}

/// Splice one item into or out of a single-line `key: [a, b, c]` flow list.
/// Rebuilds the bracket interior from trimmed tokens rather than preserving
/// exact inter-item whitespace — the whole line was already the "intended
/// span" for a flow-style value, so this is still a one-line diff.
fn splice_flow_list(body: &str, old_len: usize, op: FlowOp<'_>) -> Option<String> {
    let mut lines = body.split_inclusive('\n');
    let line = lines.next()?;
    if lines.next().is_some() {
        return None;
    }
    let (content, eol) = split_trailing_eol(line);
    let FlowListLayout {
        open_pos,
        close_pos,
        items,
    } = parse_flow_list(content)?;
    if items.len() != old_len {
        return None;
    }
    let inner = &content[open_pos + 1..close_pos];
    let sep = detect_flow_separator(inner, &items);
    let mut tokens: Vec<&str> = items.iter().map(|&(s, e)| inner[s..e].trim()).collect();
    match op {
        FlowOp::Remove(idx) => {
            if idx >= tokens.len() {
                return None;
            }
            tokens.remove(idx);
        }
        FlowOp::Append(text) => tokens.push(text),
    }

    let mut new_content = String::with_capacity(content.len() + 8);
    new_content.push_str(&content[..=open_pos]);
    new_content.push_str(&tokens.join(sep));
    new_content.push_str(&content[close_pos..]);
    new_content.push_str(eol);
    Some(new_content)
}

/// Is this column-0 line blank or a comment (i.e. trivia between keys)?
fn is_trivia(line: &str) -> bool {
    line.trim().is_empty() || line.starts_with('#')
}

/// Classify a column-0 line (line ending already stripped).
///
/// Deliberately conservative: only `key:`, `key: value`, `"key": value` and
/// `'key': value` count as keys. Compact-style sequence items (`- x`, which
/// hyalo already supports via `compact_list_indent`) continue the preceding
/// key. Everything else at column 0 — explicit `? key` syntax, top-level flow
/// collections, anchors, tags, directives, document markers — is
/// [`LineRole::Unsupported`], which the caller turns into a fallback rather
/// than a guess.
fn classify_line(line: &str) -> LineRole {
    debug_assert!(!line.starts_with([' ', '\t']));
    if is_trivia(line) {
        return LineRole::Trivia;
    }
    if line == "-" || line.starts_with("- ") {
        return LineRole::Continuation;
    }
    if line.starts_with("---") || line.starts_with("...") || line.starts_with('%') {
        return LineRole::Unsupported;
    }
    let first = line.as_bytes()[0];
    if matches!(
        first,
        b'-' | b'?' | b'{' | b'[' | b'&' | b'*' | b'!' | b'|' | b'>' | b':'
    ) {
        return LineRole::Unsupported;
    }
    if key_token_len(line).is_some() {
        LineRole::Key
    } else {
        LineRole::Unsupported
    }
}

/// Byte length of the key token at the start of a column-0 key line — the text
/// before the `:` separator, quotes included.
///
/// Returns `None` when `line` does not start with a key token recognized by
/// [`classify_line`]. Splitting this out lets [`rename_key_in_place`] replace
/// exactly the key token and keep the rest of the line byte-identical, using
/// the same recognition rule the segmenter itself applies.
fn key_token_len(line: &str) -> Option<usize> {
    let first = *line.as_bytes().first()?;
    let rest = match first {
        b'"' => scan_quoted(line, '"', true)?,
        b'\'' => scan_quoted(line, '\'', false)?,
        _ => {
            // Plain key: everything up to the first `:` that is followed by a
            // space or end-of-line. A leading `#` would have made the line
            // trivia, which the caller handles before reaching here.
            let bytes = line.as_bytes();
            let found = bytes.iter().enumerate().find_map(|(i, b)| {
                (*b == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ')).then_some(i)
            })?;
            if found == 0 {
                return None;
            }
            &line[found..]
        }
    };
    // `rest` must start at the key separator.
    if rest.starts_with(':') && (rest.len() == 1 || rest.as_bytes()[1] == b' ') {
        Some(line.len() - rest.len())
    } else {
        None
    }
}

/// Skip a quoted scalar starting at index 0 of `line`, returning the remainder
/// after the closing quote. `escapes` enables `\`-escaping (double quotes);
/// single quotes use `''` doubling instead.
fn scan_quoted(line: &str, quote: char, escapes: bool) -> Option<&str> {
    let mut chars = line.char_indices();
    chars.next()?; // opening quote
    while let Some((i, c)) = chars.next() {
        if escapes && c == '\\' {
            chars.next();
            continue;
        }
        if c == quote {
            if !escapes && line[i + 1..].starts_with(quote) {
                chars.next();
                continue;
            }
            return Some(&line[i + c.len_utf8()..]);
        }
    }
    None
}

/// Split raw frontmatter YAML into `(header trivia, per-key segments, footer trivia)`.
///
/// Returns `None` when the text cannot be modeled as a flat sequence of
/// top-level keys.
fn segment(yaml: &str) -> Option<(&str, Vec<Segment<'_>>, &str)> {
    // Byte offsets at which each top-level key line begins.
    let mut key_starts: Vec<usize> = Vec::new();
    let mut offset = 0usize;
    for line in yaml.split_inclusive('\n') {
        let stripped = line.trim_end_matches(['\n', '\r']);
        if !stripped.starts_with([' ', '\t']) {
            // A closing delimiter must never appear inside the content we were
            // handed; if it does, our framing is wrong.
            if is_closing_delimiter(stripped) {
                return None;
            }
            match classify_line(stripped) {
                LineRole::Key => key_starts.push(offset),
                // A compact sequence item before any key means the document is
                // a top-level sequence, not a mapping.
                LineRole::Continuation if key_starts.is_empty() => return None,
                LineRole::Trivia | LineRole::Continuation => {}
                LineRole::Unsupported => return None,
            }
        }
        offset += line.len();
    }

    // No keys at all (empty or comment-only block): nothing to splice against.
    let first_key = *key_starts.first()?;
    let header = &yaml[..first_key];

    // Where each key's own text ends: the region up to the next key, minus any
    // trailing run of column-0 trivia (which belongs to whatever follows).
    let body_ends: Vec<usize> = key_starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let region_end = key_starts.get(i + 1).copied().unwrap_or(yaml.len());
            trailing_trivia_start(yaml, start, region_end)
        })
        .collect();

    let mut segments: Vec<Segment<'_>> = Vec::with_capacity(key_starts.len());
    for (i, &start) in key_starts.iter().enumerate() {
        let pre_start = if i == 0 { start } else { body_ends[i - 1] };
        let pre_trivia = &yaml[pre_start..start];
        let body = &yaml[start..body_ends[i]];
        let parsed = parse_map(body).ok()?;
        if parsed.len() != 1 {
            return None;
        }
        let (key, value) = parsed.into_iter().next()?;
        segments.push(Segment {
            key,
            value,
            pre_trivia,
            body,
        });
    }

    // Whatever follows the last key's body is document footer trivia. It is
    // only legitimate if it really is pure trivia — otherwise our span model
    // dropped real content.
    let footer = &yaml[*body_ends.last()?..];
    if !footer
        .split_inclusive('\n')
        .all(|l| is_trivia(l.trim_end_matches(['\n', '\r'])))
    {
        return None;
    }

    Some((header, segments, footer))
}

/// Within `yaml[start..end]`, find the offset where the trailing run of
/// column-0 trivia lines begins. Used to detach a comment block from the key
/// above it so it can travel with the key below.
///
/// `start` must point at a key line (never trivia), so the returned offset is
/// always `> start`.
fn trailing_trivia_start(yaml: &str, start: usize, end: usize) -> usize {
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut offset = start;
    for line in yaml[start..end].split_inclusive('\n') {
        lines.push((offset, offset + line.len()));
        offset += line.len();
    }
    let mut cut = end;
    for &(ls, le) in lines.iter().rev() {
        let stripped = yaml[ls..le].trim_end_matches(['\n', '\r']);
        if stripped.starts_with([' ', '\t']) || !is_trivia(stripped) {
            break;
        }
        cut = ls;
    }
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn props(pairs: &[(&str, Value)]) -> IndexMap<String, Value> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn spliced(yaml: &str, p: &IndexMap<String, Value>) -> String {
        match splice_frontmatter(yaml, p, false) {
            SpliceOutcome::Spliced(s) => s,
            SpliceOutcome::Fallback(reason) => {
                panic!("expected splice, got fallback: {}", reason.as_str())
            }
        }
    }

    fn fallback(yaml: &str, p: &IndexMap<String, Value>) -> FallbackReason {
        match splice_frontmatter(yaml, p, false) {
            SpliceOutcome::Spliced(s) => panic!("expected fallback, got splice:\n{s}"),
            SpliceOutcome::Fallback(reason) => reason,
        }
    }

    /// Count lines that differ between `before` and `after` (in either
    /// direction) — a coarse stand-in for a diff line count.
    fn changed_lines(before: &str, after: &str) -> usize {
        let b: Vec<&str> = before.lines().collect();
        let a: Vec<&str> = after.lines().collect();
        a.iter().filter(|l| !b.contains(l)).count() + b.iter().filter(|l| !a.contains(l)).count()
    }

    #[test]
    fn adding_a_key_leaves_every_other_line_byte_identical() {
        let yaml = "title: 'Single quoted title'\ntags:\n  - a\n  - b\nintro: >-\n  A long folded\n  intro paragraph.\n";
        let p = props(&[
            ("title", json!("Single quoted title")),
            ("tags", json!(["a", "b"])),
            ("intro", json!("A long folded intro paragraph.")),
            ("status", json!("draft")),
        ]);
        let out = spliced(yaml, &p);
        assert_eq!(out, format!("{yaml}status: draft\n"));
        assert_eq!(changed_lines(yaml, &out), 1);
    }

    #[test]
    fn changing_one_key_touches_only_that_key() {
        let yaml = "title: 'Keep me'\nstatus: planned\ntags:\n  - x\n";
        let p = props(&[
            ("title", json!("Keep me")),
            ("status", json!("completed")),
            ("tags", json!(["x"])),
        ]);
        let out = spliced(yaml, &p);
        assert_eq!(out, "title: 'Keep me'\nstatus: completed\ntags:\n  - x\n");
    }

    #[test]
    fn removing_a_key_drops_its_lines_and_its_comment() {
        let yaml = "title: 'Keep me'\n# explains status\nstatus: planned\ntags:\n  - x\n";
        let p = props(&[("title", json!("Keep me")), ("tags", json!(["x"]))]);
        let out = spliced(yaml, &p);
        assert_eq!(out, "title: 'Keep me'\ntags:\n  - x\n");
    }

    #[test]
    fn header_and_footer_comments_survive() {
        let yaml = "# document header\ntitle: 'T'\nstatus: planned\n# trailing note\n";
        let p = props(&[("title", json!("T")), ("status", json!("in-progress"))]);
        let out = spliced(yaml, &p);
        assert_eq!(
            out,
            "# document header\ntitle: 'T'\nstatus: in-progress\n# trailing note\n"
        );
    }

    #[test]
    fn block_scalars_and_quote_styles_are_preserved() {
        let yaml = concat!(
            "title: \"Double quoted\"\n",
            "shortTitle: 'Single quoted'\n",
            "plain: unquoted value\n",
            "literal: |\n",
            "  line one\n",
            "  line two\n",
            "folded: >-\n",
            "  folded one\n",
            "  folded two\n",
            "flowList: [a, b, c]\n",
            "blockList:\n",
            "  - one\n",
            "  - two\n",
        );
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("added".into(), json!(1));
        let out = spliced(yaml, &p);
        assert_eq!(out, format!("{yaml}added: 1\n"));
    }

    #[test]
    fn compact_list_indentation_is_preserved() {
        let yaml = "tags:\n- one\n- two\nstatus: planned\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("status".into(), json!("done"));
        let out = match splice_frontmatter(yaml, &p, true) {
            SpliceOutcome::Spliced(s) => s,
            SpliceOutcome::Fallback(r) => panic!("unexpected fallback: {}", r.as_str()),
        };
        assert_eq!(out, "tags:\n- one\n- two\nstatus: done\n");
    }

    #[test]
    fn nested_objects_and_unusual_indentation_are_preserved() {
        let yaml = concat!(
            "versions:\n",
            "    fpt: '*'\n",
            "    ghec: '*'\n",
            "children:\n",
            "- /first\n",
            "- /second\n",
        );
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("topic".into(), json!(["API"]));
        let out = spliced(yaml, &p);
        assert!(out.starts_with(yaml), "original text must survive:\n{out}");
    }

    #[test]
    fn crlf_input_is_normalized_and_untouched_keys_keep_their_text() {
        let yaml = "title: 'T'\r\nstatus: planned\r\n";
        let p = props(&[("title", json!("T")), ("status", json!("done"))]);
        let out = spliced(yaml, &p);
        assert_eq!(out, "title: 'T'\nstatus: done\n");
    }

    #[test]
    fn reordering_moves_original_text_rather_than_rewriting_it() {
        let yaml = "b: 'second'\na: 'first'\n";
        let p = props(&[("a", json!("first")), ("b", json!("second"))]);
        let out = spliced(yaml, &p);
        assert_eq!(out, "a: 'first'\nb: 'second'\n");
    }

    #[test]
    fn quoted_keys_are_recognized() {
        let yaml = "\"quoted key\": 1\n'other key': 2\n";
        let p = props(&[("quoted key", json!(1)), ("other key", json!(3))]);
        let out = spliced(yaml, &p);
        assert_eq!(out, "\"quoted key\": 1\nother key: 3\n");
    }

    #[test]
    fn top_level_sequence_falls_back() {
        // A top-level sequence is not a property map at all, so it is rejected
        // by the baseline parse before segmentation even runs.
        let yaml = "- a\n- b\n";
        let p = props(&[("a", json!(1))]);
        assert_eq!(fallback(yaml, &p), FallbackReason::Unparseable);
    }

    #[test]
    fn compact_sequence_before_any_key_is_not_span_mappable() {
        // Segmentation-level guard for the same shape, reached directly.
        assert!(segment("- a\n- b\n").is_none());
    }

    #[test]
    fn explicit_key_syntax_falls_back() {
        let yaml = "? complex\n: value\n";
        let p = props(&[("complex", json!("value"))]);
        assert_eq!(fallback(yaml, &p), FallbackReason::NotSpanMappable);
    }

    #[test]
    fn unparseable_original_falls_back() {
        let yaml = "title: [unclosed\n";
        let p = props(&[("title", json!("x"))]);
        assert_eq!(fallback(yaml, &p), FallbackReason::Unparseable);
    }

    #[test]
    fn comment_only_block_falls_back() {
        let yaml = "# nothing but a comment\n";
        let p = props(&[("title", json!("x"))]);
        assert_eq!(fallback(yaml, &p), FallbackReason::NotSpanMappable);
    }

    #[test]
    fn lone_carriage_return_falls_back() {
        let yaml = "title: 'a\rb'\n";
        let p = props(&[("title", json!("x"))]);
        assert_eq!(fallback(yaml, &p), FallbackReason::MixedLineEndings);
    }

    #[test]
    fn everything_removed_yields_empty_output() {
        let yaml = "title: 'T'\n";
        let p: IndexMap<String, Value> = IndexMap::new();
        let out = spliced(yaml, &p);
        assert_eq!(out, "");
    }

    #[test]
    fn github_docs_shaped_frontmatter_changes_only_the_added_line() {
        // Shape lifted from a real GitHub Docs `index.md`: long values, `>-`
        // folded intros, deep child lists, nested version maps.
        let yaml = concat!(
            "title: GitHub Actions documentation\n",
            "shortTitle: GitHub Actions\n",
            "intro: >-\n",
            "  Automate, customize, and execute your software development workflows\n",
            "  right in your repository with GitHub Actions.\n",
            "introLinks:\n",
            "  quickstart: /actions/quickstart\n",
            "  reference: /actions/reference\n",
            "featuredLinks:\n",
            "  startHere:\n",
            "    - /actions/learn-github-actions/understanding-github-actions\n",
            "    - /actions/learn-github-actions/finding-and-customizing-actions\n",
            "  guideCards:\n",
            "    - /actions/deployment/deploying-to-amazon-elastic-container-service\n",
            "changelog:\n",
            "  label: actions\n",
            "  prefix: 'GitHub Actions: '\n",
            "redirect_from:\n",
            "  - /articles/automating-your-workflow-with-github-actions\n",
            "  - /articles/customizing-your-project-with-github-actions\n",
            "versions:\n",
            "  fpt: '*'\n",
            "  ghes: '*'\n",
            "  ghec: '*'\n",
            "children:\n",
            "  - /concepts\n",
            "  - /tutorials\n",
            "  - /how-tos\n",
            "  - /reference\n",
        );
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("status".into(), json!("reviewed"));
        let out = spliced(yaml, &p);
        assert_eq!(out, format!("{yaml}status: reviewed\n"));
        assert_eq!(
            changed_lines(yaml, &out),
            1,
            "adding one property must change exactly one line"
        );
    }

    // -------------------------------------------------------------------
    // List splicing (iter-219 NEW-5)
    // -------------------------------------------------------------------

    #[test]
    fn append_to_indented_block_list_touches_only_one_line() {
        let yaml = "title: 'Keep me'\naliases:\n  - old-name\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.push(json!("new-name"));
        let out = spliced(yaml, &p);
        assert_eq!(
            out,
            "title: 'Keep me'\naliases:\n  - old-name\n  - new-name\nstatus: draft\n"
        );
        assert_eq!(changed_lines(yaml, &out), 1);
    }

    #[test]
    fn append_to_block_list_with_trailing_space_after_colon_touches_only_one_line() {
        // Real GH Docs shape: `redirect_from: \n` — a trailing space before
        // the block sequence starts, insignificant to YAML but originally
        // broke the `ends_with(':')` detection and fell back to a whole-key
        // reserialize (corpus verification finding, iter-219).
        let yaml = "redirect_from: \n  - /old-path\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("redirect_from") else {
            panic!("expected array")
        };
        seq.push(json!("/new-path"));
        let out = spliced(yaml, &p);
        assert_eq!(
            out,
            "redirect_from: \n  - /old-path\n  - /new-path\nstatus: draft\n"
        );
        assert_eq!(changed_lines(yaml, &out), 1);
    }

    #[test]
    fn append_to_compact_block_list_touches_only_one_line() {
        let yaml = "tags:\n- one\n- two\nstatus: planned\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("tags") else {
            panic!("expected array")
        };
        seq.push(json!("three"));
        let out = spliced(yaml, &p);
        assert_eq!(out, "tags:\n- one\n- two\n- three\nstatus: planned\n");
        assert_eq!(changed_lines(yaml, &out), 1);
    }

    #[test]
    fn remove_from_block_list_drops_only_that_line() {
        let yaml = "aliases:\n  - old-name\n  - other\n  - third\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.remove(1); // "other"
        let out = spliced(yaml, &p);
        assert_eq!(out, "aliases:\n  - old-name\n  - third\nstatus: draft\n");
    }

    #[test]
    fn append_to_flow_list_stays_flow() {
        let yaml = "tags: [a, b]\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("tags") else {
            panic!("expected array")
        };
        seq.push(json!("c"));
        let out = spliced(yaml, &p);
        assert_eq!(out, "tags: [a, b, c]\nstatus: draft\n");
    }

    #[test]
    fn remove_from_flow_list_stays_flow() {
        let yaml = "tags: [a, b, c]\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("tags") else {
            panic!("expected array")
        };
        seq.remove(1); // "b"
        let out = spliced(yaml, &p);
        assert_eq!(out, "tags: [a, c]\nstatus: draft\n");
    }

    #[test]
    fn append_to_empty_flow_list_produces_single_item() {
        let yaml = "tags: []\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!(["a"]));
        let out = spliced(yaml, &p);
        assert_eq!(out, "tags: [a]\nstatus: draft\n");
    }

    #[test]
    fn appending_a_new_key_still_creates_a_fresh_list() {
        // Not a list-splice case: the key doesn't exist yet, so this must
        // still go through ordinary key serialization.
        let yaml = "title: 'T'\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("aliases".into(), json!(["first"]));
        let out = spliced(yaml, &p);
        assert!(out.starts_with(yaml));
        assert!(out.contains("aliases"));
    }

    #[test]
    fn full_list_replacement_falls_back_to_whole_key_reserialize() {
        // `set --property tags=[a,b,c]` replaces the whole value — not an
        // append or removal of exactly one item — so the list-splice path
        // must not fire; the existing whole-key re-serialize still handles
        // it correctly.
        let yaml = "tags:\n  - a\n  - b\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!(["x", "y", "z"]));
        let out = spliced(yaml, &p);
        let parsed = parse_map(&out).expect("output re-parses");
        assert_eq!(parsed.get("tags"), Some(&json!(["x", "y", "z"])));
        assert!(!out.contains("- a"));
        assert!(out.contains("status: draft"));
    }

    #[test]
    fn reordering_list_items_falls_back_to_whole_key_reserialize() {
        let yaml = "tags:\n  - a\n  - b\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!(["b", "a"]));
        let out = spliced(yaml, &p);
        // Not a supported single-item delta — must still round-trip correctly
        // via the whole-key fallback (verification gate would catch a bug).
        let parsed = parse_map(&out).expect("output re-parses");
        assert_eq!(parsed.get("tags"), Some(&json!(["b", "a"])));
    }

    #[test]
    fn append_to_list_alongside_other_unchanged_keys_touches_one_line() {
        // GitHub-Docs shaped repro (NEW-5): a single appended redirect_from
        // entry must not touch any other key's lines, nor any other
        // sibling list item.
        let yaml = concat!(
            "title: GitHub Actions documentation\n",
            "redirect_from:\n",
            "  - /articles/automating-your-workflow-with-github-actions\n",
            "  - /articles/customizing-your-project-with-github-actions\n",
            "versions:\n",
            "  fpt: '*'\n",
            "  ghec: '*'\n",
        );
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("redirect_from") else {
            panic!("expected array")
        };
        seq.push(json!("/dogfood-probe"));
        let out = spliced(yaml, &p);
        assert_eq!(
            out,
            concat!(
                "title: GitHub Actions documentation\n",
                "redirect_from:\n",
                "  - /articles/automating-your-workflow-with-github-actions\n",
                "  - /articles/customizing-your-project-with-github-actions\n",
                "  - /dogfood-probe\n",
                "versions:\n",
                "  fpt: '*'\n",
                "  ghec: '*'\n",
            )
        );
        assert_eq!(changed_lines(yaml, &out), 1);
    }

    #[test]
    fn removing_last_item_from_list_removes_the_key_entirely() {
        // The CLI layer already drops the key from `props` when a removal
        // empties the list; the splicer just needs to honor a key's
        // *absence*, which the existing removal path already does.
        let yaml = "title: 'T'\naliases:\n  - only-one\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.shift_remove("aliases");
        let out = spliced(yaml, &p);
        assert_eq!(out, "title: 'T'\nstatus: draft\n");
    }

    // -------------------------------------------------------------------
    // Review round: fallback-routing coverage (iter-219 PR #250 findings)
    // -------------------------------------------------------------------

    #[test]
    fn append_non_inlineable_item_to_flow_list_falls_back_with_warning() {
        // M1: a new item that forces a multi-line block scalar (embedded
        // newline) cannot be written as a flow token. Before the fix this
        // silently fell through to a whole-key re-serialize that converts
        // `tags: [a, b]` to block style with no warning — exactly the
        // DEC-081/DEC-087 violation FlowListNotModellable exists to catch.
        let yaml = "tags: [a, b]\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!(["a", "b", "line one\nline two"]));
        assert_eq!(
            fallback(yaml, &p),
            FallbackReason::FlowListNotModellable,
            "must warn, not silently convert flow to block"
        );
    }

    #[test]
    fn append_to_flow_list_with_trailing_comment_falls_back_with_warning() {
        // M2/M3: `is_single_line_flow` recognizes this as flow-shaped (looser
        // check, by design), but `parse_flow_list` correctly refuses it (the
        // line doesn't end with `]`) — the gap between those two checks is
        // exactly what routes this to an explicit warning instead of a
        // silent flow-to-block reformat that would also drop the comment.
        let yaml = "tags: [a, b] # keep these\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!(["a", "b", "c"]));
        assert_eq!(fallback(yaml, &p), FallbackReason::FlowListNotModellable);
    }

    #[test]
    fn remove_from_flow_list_with_trailing_comment_falls_back_with_warning() {
        // M3: append and remove must be symmetric — both warn, neither
        // silently reformats+drops the comment.
        let yaml = "tags: [a, b, c] # keep these\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("tags") else {
            panic!("expected array")
        };
        seq.remove(1);
        assert_eq!(fallback(yaml, &p), FallbackReason::FlowListNotModellable);
    }

    #[test]
    fn append_to_block_list_with_interleaved_comment_falls_back_with_warning() {
        // M6: comments between block-list items are out of scope for this
        // splicer (no full-CST support) — but the honest response is an
        // explicit fallback+warning, not a silent whole-key re-serialize
        // that discards the comment with no explanation.
        let yaml = "aliases:\n  - old-name\n  # keep this note\n  - other\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.push(json!("new-name"));
        assert_eq!(fallback(yaml, &p), FallbackReason::BlockListNotModellable);
    }

    #[test]
    fn remove_from_block_list_with_interleaved_comment_falls_back_with_warning() {
        let yaml = "aliases:\n  - old-name\n  # keep this note\n  - other\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.remove(0); // "old-name"
        assert_eq!(fallback(yaml, &p), FallbackReason::BlockListNotModellable);
    }

    #[test]
    fn list_splice_works_under_pure_crlf() {
        // Nothing previously exercised the list-splice path specifically
        // under a consistently-CRLF file (as opposed to scalar-value
        // changes, already covered elsewhere).
        let yaml = "aliases:\r\n  - old-name\r\nstatus: draft\r\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.push(json!("new-name"));
        let out = spliced(yaml, &p);
        // splice_frontmatter always returns LF-normalized text; CRLF
        // re-expansion is the caller's job (write_frontmatter_impl).
        assert_eq!(out, "aliases:\n  - old-name\n  - new-name\nstatus: draft\n");
    }

    #[test]
    fn list_splice_remove_works_under_pure_crlf() {
        let yaml = "aliases:\r\n  - old-name\r\n  - other\r\nstatus: draft\r\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        let Some(Value::Array(seq)) = p.get_mut("aliases") else {
            panic!("expected array")
        };
        seq.remove(0); // "old-name"
        let out = spliced(yaml, &p);
        assert_eq!(out, "aliases:\n  - other\nstatus: draft\n");
    }

    #[test]
    fn removing_the_only_item_from_a_flow_list_splices_to_empty_brackets() {
        let yaml = "tags: [a]\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("tags".into(), json!([]));
        let out = spliced(yaml, &p);
        assert_eq!(out, "tags: []\nstatus: draft\n");
    }

    #[test]
    fn removing_the_only_item_from_a_block_list_via_explicit_empty_array_is_safe() {
        // Not the CLI's real shape (remove.rs drops the key entirely when a
        // list empties — see `removing_last_item_from_list_removes_the_key_entirely`
        // above) but a defensive check on the splicer itself: a block
        // sequence's only item removed via an explicit `key: []` request has
        // no line-level representation (`key:` alone parses as null, not an
        // empty list), so the verification gate must catch the mismatch and
        // fall back safely rather than writing something that doesn't
        // round-trip to what was asked for.
        let yaml = "aliases:\n  - only-one\nstatus: draft\n";
        let mut p = parse_map(yaml).expect("fixture parses");
        p.insert("aliases".into(), json!([]));
        let out = match splice_frontmatter(yaml, &p, false) {
            SpliceOutcome::Spliced(s) => s,
            // Whichever fallback fired, the caller re-serializes the whole
            // block from `p` directly — simulate that here.
            SpliceOutcome::Fallback(_) => {
                serde_saphyr::to_string_with_options(&p, SerializerOptions::default())
                    .expect("full re-serialize must succeed")
            }
        };
        let reparsed = parse_map(&out).expect("output must still be valid YAML");
        assert_eq!(
            reparsed.get("aliases"),
            Some(&json!([])),
            "must round-trip to the requested empty list, however it got there:\n{out}"
        );
    }
}

// ---------------------------------------------------------------------------
// In-place key rename (iter-266 PROP-1)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod rename_tests {
    use super::*;

    /// Rename `from` → `to` and assert the block is byte-identical except for
    /// the key token itself.
    fn renamed(yaml: &str, from: &str, to: &str) -> String {
        rename_key_in_place(yaml, from, to)
            .unwrap_or_else(|| panic!("rename {from} -> {to} must splice in place:\n{yaml}"))
    }

    #[test]
    fn renames_scalar_in_place() {
        let out = renamed("title: Note\nrating: 7\ntags:\n  - a\n", "rating", "score");
        assert_eq!(out, "title: Note\nscore: 7\ntags:\n  - a\n");
    }

    #[test]
    fn empty_value_stays_empty_not_null() {
        // BUG-12: the props path turned `rating:` into `score: null`.
        let out = renamed("title: Note\nrating:\n", "rating", "score");
        assert_eq!(out, "title: Note\nscore:\n");
        assert!(!out.contains("null"), "empty value must stay empty: {out}");
    }

    #[test]
    fn preserves_position_first_middle_last() {
        assert_eq!(
            renamed("a: 1\nb: 2\nc: 3\n", "a", "z"),
            "z: 1\nb: 2\nc: 3\n"
        );
        assert_eq!(
            renamed("a: 1\nb: 2\nc: 3\n", "b", "z"),
            "a: 1\nz: 2\nc: 3\n"
        );
        assert_eq!(
            renamed("a: 1\nb: 2\nc: 3\n", "c", "z"),
            "a: 1\nb: 2\nz: 3\n"
        );
    }

    #[test]
    fn preserves_quoting_and_spacing() {
        let yaml = "rating:   \"7\"  \ntitle: 'x'\n";
        assert_eq!(
            renamed(yaml, "rating", "score"),
            "score:   \"7\"  \ntitle: 'x'\n"
        );
    }

    #[test]
    fn preserves_block_list_indentation() {
        let yaml = "kw:\n- one\n- two\nafter: x\n";
        assert_eq!(renamed(yaml, "kw", "keywords"), "keywords:\n- one\n- two\nafter: x\n");
        let two_space = "kw:\n  - one\n  - two\n";
        assert_eq!(renamed(two_space, "kw", "keywords"), "keywords:\n  - one\n  - two\n");
    }

    #[test]
    fn preserves_flow_list() {
        let yaml = "kw: [one,  two]\n";
        assert_eq!(renamed(yaml, "kw", "keywords"), "keywords: [one,  two]\n");
    }

    #[test]
    fn preserves_trailing_comment_and_comment_block() {
        let yaml = "# leading\nrating: 7 # why\n# trailing\n";
        assert_eq!(
            renamed(yaml, "rating", "score"),
            "# leading\nscore: 7 # why\n# trailing\n"
        );
    }

    #[test]
    fn preserves_nested_map_value() {
        let yaml = "meta:\n  a: 1\n  b:\n    c: 2\nend: x\n";
        assert_eq!(
            renamed(yaml, "meta", "metadata"),
            "metadata:\n  a: 1\n  b:\n    c: 2\nend: x\n"
        );
    }

    #[test]
    fn renames_quoted_key() {
        let yaml = "\"my key\": 1\nb: 2\n";
        // The target is a plain scalar, so it is written bare even though the
        // source was quoted — quoting follows the new key, not the old one.
        assert_eq!(renamed(yaml, "my key", "other key"), "other key: 1\nb: 2\n");
        let single = "'my key': 1\n";
        assert_eq!(renamed(single, "my key", "plain"), "plain: 1\n");
    }

    #[test]
    fn quotes_a_target_that_needs_it() {
        let out = renamed("a: 1\n", "a", "true");
        assert_eq!(out, "\"true\": 1\n");
        assert_eq!(renamed("a: 1\n", "a", "1.5"), "\"1.5\": 1\n");
        assert_eq!(renamed("a: 1\n", "a", "x: y"), "\"x: y\": 1\n");
    }

    #[test]
    fn preserves_crlf_by_normalizing_to_lf_for_the_caller() {
        // The caller re-expands to CRLF; the splicer works in LF.
        let out = renamed("a: 1\r\nrating: 7\r\n", "rating", "score");
        assert_eq!(out, "a: 1\nscore: 7\n");
    }

    #[test]
    fn refuses_lone_cr() {
        assert!(rename_key_in_place("a: 1\rrating: 7\n", "rating", "score").is_none());
    }

    #[test]
    fn refuses_when_source_absent_or_target_present() {
        assert!(rename_key_in_place("a: 1\n", "missing", "z").is_none());
        assert!(rename_key_in_place("a: 1\nz: 2\n", "a", "z").is_none());
        assert!(rename_key_in_place("a: 1\n", "a", "a").is_none());
    }

    #[test]
    fn refuses_unmappable_document() {
        // A top-level flow mapping is not span-mappable.
        assert!(rename_key_in_place("{a: 1, b: 2}\n", "a", "z").is_none());
    }

    #[test]
    fn preserves_multiline_block_scalar() {
        let yaml = "desc: |\n  line one\n  line two\nafter: x\n";
        assert_eq!(
            renamed(yaml, "desc", "description"),
            "description: |\n  line one\n  line two\nafter: x\n"
        );
    }
}
