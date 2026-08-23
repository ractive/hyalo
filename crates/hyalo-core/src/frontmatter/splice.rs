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
                let Some(rendered) = serialize_one(key, new_value, compact_list_indent) else {
                    return SpliceOutcome::Fallback(FallbackReason::SerializeFailed);
                };
                out.push_str(&rendered);
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
    if matches!(first, b'-' | b'?' | b'{' | b'[' | b'&' | b'*' | b'!' | b'|' | b'>' | b':') {
        return LineRole::Unsupported;
    }
    let rest = match first {
        b'"' => match scan_quoted(line, '"', true) {
            Some(rest) => rest,
            None => return LineRole::Unsupported,
        },
        b'\'' => match scan_quoted(line, '\'', false) {
            Some(rest) => rest,
            None => return LineRole::Unsupported,
        },
        _ => {
            // Plain key: everything up to the first `:` that is followed by a
            // space or end-of-line. A leading `#` would have made the line
            // trivia, which is handled above.
            let bytes = line.as_bytes();
            let mut found = None;
            for (i, b) in bytes.iter().enumerate() {
                if *b == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
                    found = Some(i);
                    break;
                }
            }
            match found {
                Some(0) | None => return LineRole::Unsupported,
                Some(i) => &line[i..],
            }
        }
    };
    // `rest` must start at the key separator.
    if rest.starts_with(':') && (rest.len() == 1 || rest.as_bytes()[1] == b' ') {
        LineRole::Key
    } else {
        LineRole::Unsupported
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
                LineRole::Trivia => {}
                // A compact sequence item before any key means the document is
                // a top-level sequence, not a mapping.
                LineRole::Continuation if key_starts.is_empty() => return None,
                LineRole::Continuation => {}
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
        a.iter().filter(|l| !b.contains(l)).count()
            + b.iter().filter(|l| !a.contains(l)).count()
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
}
