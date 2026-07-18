//! OKF conformance lint rules (`hyalo lint --profile okf`).
//!
//! These encode the *advisory* half of the Open Knowledge Format profile: the
//! SPEC §9 hard rules (parseable frontmatter + non-empty `type` on non-reserved
//! files) are already covered by the schema pass once the `okf` fragment is
//! overlaid (`required = ["type"]`, `exempt = ["**/index.md", "**/log.md"]`).
//! What lives here are the checks the schema layer cannot express and that the
//! spec says a consumer MUST NOT reject on — so every rule below is **warn**:
//!
//! - `OKF-INDEX-STRUCTURE` — a reserved `index.md` should be a Markdown link
//!   list (SPEC §6), either inside the `okf:index` managed region or as bare
//!   `* [Title](path)` / `- [Title](path)` lines.
//! - `OKF-LOG-STRUCTURE` — a reserved `log.md` should be date-grouped with
//!   `## YYYY-MM-DD` headings, newest first (SPEC §7).
//! - `OKF-CITATIONS-PRESENT` — a claim-bearing concept doc should carry a
//!   `# Citations` section (SPEC §8 convention, SHOULD).
//! - `OKF-CITATIONS-WELL-FORMED` — entries under `# Citations` should be a list
//!   of links, not free prose. Accepts both numbered (`1.`) and bullet (`-`/`*`)
//!   lists (§8 says numbered; every official sample bundle uses bullets).
//! - `OKF-CITATIONS-RESOLVE` — bundle-relative / `references/…` citation links
//!   should resolve to an existing file (broken links stay *warn* per §9).
//! - `OKF-AUGMENTATION-GUARD` — a concept's `# Schema` / `# Citations` section
//!   should not be present-but-empty (the observable, baseline-free proxy for
//!   the reference agent's "don't drop the schema/citations" augmentation guard).
//!
//! All rules are pure functions over the already-read file content; nothing here
//! touches the filesystem except `OKF-CITATIONS-RESOLVE`, which only stats the
//! candidate target path (no directory walk, no network).

use std::path::Path;

/// A single OKF advisory finding, in the same shape the lint pipeline converts
/// into an `InternalViolation`.
pub(crate) struct OkfFinding {
    pub(crate) rule_id: &'static str,
    /// 1-based line within the file (frontmatter + body), for display.
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// Rule IDs exposed by the OKF profile. Kept in one place so the catalog
/// (`lint-rules list`) and the runtime stay in lock-step; the parity test in
/// this module asserts every emitted id is listed here.
#[cfg(test)]
pub(crate) const OKF_RULE_IDS: &[&str] = &[
    "OKF-INDEX-STRUCTURE",
    "OKF-INDEX-MARKERS",
    "OKF-LOG-STRUCTURE",
    "OKF-CITATIONS-PRESENT",
    "OKF-CITATIONS-WELL-FORMED",
    "OKF-CITATIONS-RESOLVE",
    "OKF-AUGMENTATION-GUARD",
];

/// Managed-region markers, mirrored from `okf.rs` (kept in sync deliberately —
/// the structure check accepts a file that uses the generator's managed region).
const INDEX_BEGIN: &str = "<!-- okf:index:begin -->";
const INDEX_END: &str = "<!-- okf:index:end -->";

/// Is `rel_path` a reserved `index.md` (root or any subdirectory)?
///
/// When `case_insensitive` is set (mirrors the vault's resolved `[links]
/// case_insensitive` mode, same as `ExemptGlobs::is_exempt_ci`), the match
/// folds case so an adopted `INDEX.md` is recognized as reserved exactly
/// like `hyalo okf index` and the SCHEMA exempt-glob pass already do.
fn is_index_file(rel_path: &str, case_insensitive: bool) -> bool {
    has_reserved_name(rel_path, "index.md", case_insensitive)
}

/// Is `rel_path` a reserved `log.md`? See [`is_index_file`] for the
/// `case_insensitive` contract.
fn is_log_file(rel_path: &str, case_insensitive: bool) -> bool {
    has_reserved_name(rel_path, "log.md", case_insensitive)
}

/// Basename comparison behind the reserved-file predicates, allocation-free
/// (runs once per file per lint invocation). Case folding is ASCII-only
/// (`eq_ignore_ascii_case`) while the SCHEMA exempt-glob pass folds via
/// globset's Unicode-aware `(?i)`; the reserved names are pure ASCII, so the
/// two can only diverge on pathological non-ASCII lookalikes (e.g. U+212A
/// KELVIN SIGN case-folding to `k`) — accepted, not worth matching exactly.
fn has_reserved_name(rel_path: &str, name: &str, case_insensitive: bool) -> bool {
    let base = rel_path.rsplit(['/', '\\']).next().unwrap_or(rel_path);
    if case_insensitive {
        base.eq_ignore_ascii_case(name)
    } else {
        base == name
    }
}

/// Strip a trailing `\r` (CRLF tolerance) and any trailing `\n` already removed
/// by the line iterator.
fn trim_cr(line: &str) -> &str {
    line.strip_suffix('\r').unwrap_or(line)
}

/// Case-insensitive `.md` suffix test — a heuristic for "this token names a
/// Markdown file" (so `.MD`/`.Md` count too).
///
/// Byte-slicing on a fixed offset (`s.len() - 3`) is unsafe on arbitrary
/// UTF-8 input: a token ending in a multi-byte character (e.g. a single
/// 4-byte emoji) can put that offset mid-character and panic. `str::ends_with`
/// does its own boundary-safe matching, so use that instead.
fn ends_with_md(s: &str) -> bool {
    s.len() >= 3 && s.to_ascii_lowercase().ends_with(".md")
}

/// Run every enabled OKF rule against one file.
///
/// * `rel_path` — vault-relative path (used for reserved-file dispatch and
///   citation link resolution).
/// * `full_path` — absolute path, used only to resolve citation link targets
///   relative to the file's own directory.
/// * `content` — the whole file (frontmatter included).
/// * `body` — the post-frontmatter body slice of `content`.
/// * `body_line_offset` — 1-based file line number on which `body` starts, so
///   findings report file-absolute line numbers.
/// * `doc_type` — the frontmatter `type`, if any.
/// * `is_enabled` — predicate deciding whether a given rule id runs (honors
///   `[lint.rules]` overrides and `--rule`/`--rule-prefix` filters).
/// * `case_insensitive` — resolved `[links] case_insensitive` mode for the
///   vault, mirroring `ExemptGlobs::is_exempt_ci` so a case-folded `INDEX.md`
///   / `LOG.md` is recognized as the reserved file the same way the SCHEMA
///   exempt-glob pass and `hyalo okf index` already do.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_okf_rules(
    rel_path: &str,
    full_path: &Path,
    content: &str,
    body: &str,
    body_line_offset: usize,
    doc_type: Option<&str>,
    is_enabled: &dyn Fn(&str) -> bool,
    vault_dir: &Path,
    case_insensitive: bool,
) -> Vec<OkfFinding> {
    let mut out = Vec::new();

    if is_index_file(rel_path, case_insensitive) {
        if is_enabled("OKF-INDEX-MARKERS") {
            check_index_markers(content, body_line_offset, &mut out);
        }
        if is_enabled("OKF-INDEX-STRUCTURE") {
            check_index_structure(content, body, body_line_offset, &mut out);
        }
        // Reserved files are not concept docs — no citation/augmentation checks.
        return out;
    }
    if is_log_file(rel_path, case_insensitive) {
        if is_enabled("OKF-LOG-STRUCTURE") {
            check_log_structure(body, body_line_offset, &mut out);
        }
        return out;
    }

    // Concept-doc rules.
    let sections = scan_sections(body, body_line_offset);

    let citations = sections.iter().find(|s| s.is_heading("Citations", 1));

    if is_enabled("OKF-CITATIONS-PRESENT") && citations.is_none() && concept_makes_claims(doc_type)
    {
        out.push(OkfFinding {
            rule_id: "OKF-CITATIONS-PRESENT",
            line: body_line_offset,
            message:
                "concept doc has no `# Citations` section (OKF §8 convention: cite factual claims)"
                    .to_owned(),
        });
    }

    if let Some(cit) = citations {
        let entries = &body[cit.body_start..cit.body_end];
        if is_enabled("OKF-CITATIONS-WELL-FORMED") {
            check_citations_well_formed(entries, cit.content_first_line, &mut out);
        }
        if is_enabled("OKF-CITATIONS-RESOLVE") {
            check_citations_resolve(
                entries,
                cit.content_first_line,
                full_path,
                vault_dir,
                &mut out,
            );
        }
    }

    if is_enabled("OKF-AUGMENTATION-GUARD") {
        for name in ["Schema", "Citations"] {
            if let Some(sec) = sections.iter().find(|s| s.is_heading(name, 1))
                && body[sec.body_start..sec.body_end].trim().is_empty()
            {
                out.push(OkfFinding {
                    rule_id: "OKF-AUGMENTATION-GUARD",
                    line: sec.heading_line,
                    message: format!(
                        "`# {name}` section is present but empty — augmentation must not drop its content"
                    ),
                });
            }
        }
    }

    out
}

/// A concept doc "makes claims" (and thus should cite) unless it is a pure
/// pointer with no prose of its own. Heuristic: everything that is not a
/// `Reference`-typed stub. Kept permissive — this is a SHOULD, warn-only.
fn concept_makes_claims(doc_type: Option<&str>) -> bool {
    // `Reference` docs are themselves citations/pointers; don't demand a
    // nested `# Citations` from them. Everything else is claim-bearing.
    !matches!(doc_type, Some(t) if t.eq_ignore_ascii_case("reference"))
}

// ---------------------------------------------------------------------------
// Reserved-file structure
// ---------------------------------------------------------------------------

/// Flag malformed `okf:index` managed-region markers so CI surfaces the
/// precondition instead of `hyalo okf index --apply` silently skipping the file
/// (BUG-3). A single well-formed begin→end pair (or none at all) passes; any
/// dangling begin, dangling end, reversed pair, or duplicate markers warn.
fn check_index_markers(content: &str, body_line_offset: usize, out: &mut Vec<OkfFinding>) {
    let begins = content.matches(INDEX_BEGIN).count();
    let ends = content.matches(INDEX_END).count();
    // An ordered pair exists when some END follows some BEGIN.
    let ordered = content
        .find(INDEX_BEGIN)
        .is_some_and(|b| content[b + INDEX_BEGIN.len()..].contains(INDEX_END));

    let problem = match (begins, ends) {
        (0, 0) => None,
        (1, 1) if ordered => None,
        (1, 1) => Some(
            "reversed `okf:index` markers (`end` appears before `begin`) — regeneration will skip this file"
                .to_owned(),
        ),
        (0, _) => Some(
            "dangling `<!-- okf:index:end -->` with no preceding `<!-- okf:index:begin -->`"
                .to_owned(),
        ),
        (_, 0) => Some(
            "dangling `<!-- okf:index:begin -->` with no matching `<!-- okf:index:end -->`"
                .to_owned(),
        ),
        (b, e) => Some(format!(
            "duplicate `okf:index` markers ({b} begin, {e} end) — only a single begin/end pair is managed"
        )),
    };

    if let Some(message) = problem {
        out.push(OkfFinding {
            rule_id: "OKF-INDEX-MARKERS",
            line: body_line_offset,
            message,
        });
    }
}

fn check_index_structure(
    content: &str,
    body: &str,
    body_line_offset: usize,
    out: &mut Vec<OkfFinding>,
) {
    // A file produced by `hyalo okf index` is trivially conformant: it carries
    // the managed region. Anchor on structural position (END after BEGIN) so a
    // stray marker mention in prose can't fool the check (mirrors okf.rs).
    if let Some(begin) = content.find(INDEX_BEGIN)
        && content[begin + INDEX_BEGIN.len()..].contains(INDEX_END)
    {
        return;
    }

    // Otherwise require at least one Markdown link-list line: `* [T](p)` or
    // `- [T](p)` (SPEC §6). Empty index files are allowed (a fresh section).
    let has_any_content = body.lines().any(|l| !trim_cr(l).trim().is_empty());
    if !has_any_content {
        return;
    }
    let has_link_line = body.lines().any(|l| is_link_list_line(trim_cr(l)));
    if !has_link_line {
        out.push(OkfFinding {
            rule_id: "OKF-INDEX-STRUCTURE",
            line: body_line_offset,
            message: "reserved `index.md` is not a Markdown link list (OKF §6: `* [Title](path) - description`)".to_owned(),
        });
    }
}

/// A bullet line that contains a Markdown link: `* [text](target)` or
/// `- [text](target)` (leading whitespace allowed).
fn is_link_list_line(line: &str) -> bool {
    let t = line.trim_start();
    let Some(rest) = t.strip_prefix("* ").or_else(|| t.strip_prefix("- ")) else {
        return false;
    };
    // Cheap `[..](..)` detection.
    if let Some(open) = rest.find("](") {
        rest[..open].contains('[') && rest[open + 2..].contains(')')
    } else {
        false
    }
}

fn check_log_structure(body: &str, body_line_offset: usize, out: &mut Vec<OkfFinding>) {
    // Collect `## YYYY-MM-DD` date headings in document order.
    let mut dates: Vec<(usize, String)> = Vec::new();
    for (i, raw) in body.lines().enumerate() {
        let line = trim_cr(raw).trim_end();
        if let Some(date) = parse_date_heading(line) {
            dates.push((body_line_offset + i, date));
        }
    }

    let has_any_content = body.lines().any(|l| !trim_cr(l).trim().is_empty());
    if !has_any_content {
        // Empty log — allowed (freshly created).
        return;
    }

    if dates.is_empty() {
        out.push(OkfFinding {
            rule_id: "OKF-LOG-STRUCTURE",
            line: body_line_offset,
            message: "reserved `log.md` has no `## YYYY-MM-DD` date headings (OKF §7: date-grouped history)".to_owned(),
        });
        return;
    }

    // Newest-first: each successive date heading should be <= the previous.
    for pair in dates.windows(2) {
        let (_, prev) = &pair[0];
        let (line, cur) = &pair[1];
        if cur > prev {
            out.push(OkfFinding {
                rule_id: "OKF-LOG-STRUCTURE",
                line: *line,
                message: format!(
                    "`log.md` date headings are not newest-first ({cur} appears after {prev})"
                ),
            });
        }
    }
}

/// Parse a `## YYYY-MM-DD` heading, returning the date string when the heading
/// text is exactly an ISO date. Accepts levels 2..6 (§7 uses `##`).
fn parse_date_heading(line: &str) -> Option<String> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(2..=6).contains(&hashes) {
        return None;
    }
    let text = line[hashes..].trim();
    if is_iso_date(text) {
        Some(text.to_owned())
    } else {
        None
    }
}

/// `YYYY-MM-DD` shape check (digits and dashes only; not a full calendar
/// validation — good enough for ordering and structure).
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..].iter().all(u8::is_ascii_digit)
}

// ---------------------------------------------------------------------------
// Citations
// ---------------------------------------------------------------------------

fn check_citations_well_formed(entries: &str, first_line: usize, out: &mut Vec<OkfFinding>) {
    let mut saw_entry = false;
    for (i, raw) in entries.lines().enumerate() {
        let line = trim_cr(raw).trim();
        if line.is_empty() {
            continue;
        }
        // Nested sub-headings inside Citations end the entry list; ignore.
        if line.starts_with('#') {
            break;
        }
        if let Some(item) = strip_list_marker(line) {
            saw_entry = true;
            if !item_is_link(item) {
                out.push(OkfFinding {
                    rule_id: "OKF-CITATIONS-WELL-FORMED",
                    line: first_line + i,
                    message: format!(
                        "citation entry is not a link (OKF §8: entries should be links): {}",
                        truncate(item, 60)
                    ),
                });
            }
        } else {
            // Free prose under `# Citations` — not a list entry.
            saw_entry = true;
            out.push(OkfFinding {
                rule_id: "OKF-CITATIONS-WELL-FORMED",
                line: first_line + i,
                message: format!(
                    "citation entry is free prose, not a list of links (OKF §8): {}",
                    truncate(line, 60)
                ),
            });
        }
    }
    let _ = saw_entry;
}

/// Strip a leading list marker (`-`, `*`, `+`, or `N.`) and return the entry
/// text, or `None` if the line is not a list item.
fn strip_list_marker(line: &str) -> Option<&str> {
    if let Some(rest) = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
    {
        return Some(rest.trim());
    }
    // Numbered list: `12. text`.
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &line[digits..];
        if let Some(rest) = after.strip_prefix(". ") {
            return Some(rest.trim());
        }
    }
    None
}

/// An entry counts as a link when it is a Markdown link, a bare URL, or a
/// bundle/relative path (containing `/` or ending `.md`).
fn item_is_link(item: &str) -> bool {
    // Markdown link `[text](target)`.
    if let Some(open) = item.find("](")
        && item[..open].contains('[')
        && item[open + 2..].contains(')')
    {
        return true;
    }
    // Bare URL scheme.
    if item.contains("://") {
        return true;
    }
    // Bundle-absolute / relative path.
    let first_token = item.split_whitespace().next().unwrap_or("");
    first_token.starts_with('/')
        || first_token.starts_with("./")
        || first_token.starts_with("../")
        || first_token.starts_with("references/")
        || ends_with_md(first_token)
}

fn check_citations_resolve(
    entries: &str,
    first_line: usize,
    full_path: &Path,
    vault_dir: &Path,
    out: &mut Vec<OkfFinding>,
) {
    for (i, raw) in entries.lines().enumerate() {
        let line = trim_cr(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            break;
        }
        let Some(item) = strip_list_marker(line) else {
            continue;
        };
        let Some(target) = extract_link_target(item) else {
            continue;
        };
        // External URLs and non-.md anchors are out of scope (determinism).
        if target.contains("://") {
            continue;
        }
        // Only resolve bundle-relative / references paths (skip pure fragments,
        // mailto:, etc.).
        let path_part = target.split('#').next().unwrap_or(target);
        if path_part.is_empty() {
            continue;
        }
        if !looks_like_bundle_path(path_part) {
            continue;
        }
        if !resolves(path_part, full_path, vault_dir) {
            out.push(OkfFinding {
                rule_id: "OKF-CITATIONS-RESOLVE",
                line: first_line + i,
                message: format!("citation link does not resolve to an existing file: {path_part}"),
            });
        }
    }
}

/// Extract a link target from a citation entry: the `(target)` of a Markdown
/// link, else the first whitespace-delimited token.
fn extract_link_target(item: &str) -> Option<&str> {
    if let Some(open) = item.find("](") {
        let after = &item[open + 2..];
        if let Some(close) = after.find(')') {
            return Some(after[..close].trim());
        }
    }
    item.split_whitespace().next()
}

/// Is this a path we should try to resolve on disk (bundle-relative or
/// `references/…`), as opposed to a bare URL or scheme?
fn looks_like_bundle_path(p: &str) -> bool {
    !p.contains("://")
        && (p.starts_with('/')
            || p.starts_with("./")
            || p.starts_with("../")
            || p.starts_with("references/")
            || p.contains('/')
            || ends_with_md(p))
}

/// Resolve a bundle-relative or bundle-absolute citation path against the vault
/// root (for `/`-leading) or the file's own directory (otherwise). Stats the
/// candidate — no directory walk, no network.
///
/// Requires the target to be a *file*, not just any filesystem entry: the rule
/// message promises resolution "to an existing file", so a citation pointing
/// at a directory must still warn rather than being reported as resolved.
fn resolves(path_part: &str, full_path: &Path, vault_dir: &Path) -> bool {
    let normalized = path_part.replace('\\', "/");
    let candidate = if let Some(rest) = normalized.strip_prefix('/') {
        // Bundle-absolute: resolve from the vault root.
        vault_dir.join(rest)
    } else {
        // Relative to the citing file's directory.
        let base = full_path.parent().unwrap_or(vault_dir);
        base.join(&normalized)
    };
    candidate.is_file()
}

// ---------------------------------------------------------------------------
// Lightweight section scan (body-only, CRLF-tolerant)
// ---------------------------------------------------------------------------

/// A top-level (or nested) ATX section within the body.
struct BodySection {
    /// Heading text, trimmed (e.g. `"Citations"`).
    heading: String,
    /// Heading level (number of leading `#`).
    level: usize,
    /// 1-based file line of the heading itself.
    heading_line: usize,
    /// 1-based file line of the first content line after the heading.
    content_first_line: usize,
    /// Byte range within `body` of the section's content (exclusive of the
    /// heading line, up to the next heading of level <= this one, or EOF).
    body_start: usize,
    body_end: usize,
}

impl BodySection {
    fn is_heading(&self, text: &str, level: usize) -> bool {
        self.level == level && self.heading.eq_ignore_ascii_case(text)
    }
}

/// Scan the body into a flat list of ATX-heading sections. Fenced code blocks
/// are skipped so a `#` inside a code fence is not mistaken for a heading.
fn scan_sections(body: &str, body_line_offset: usize) -> Vec<BodySection> {
    let mut sections: Vec<BodySection> = Vec::new();
    let mut in_fence = false;
    let mut fence_marker = "";
    let mut byte = 0usize;
    // Stack of indices into `sections` whose content is still open.
    let mut open: Vec<usize> = Vec::new();

    let mut file_line = body_line_offset;
    for raw in body.split_inclusive('\n') {
        let line_len = raw.len();
        let line = trim_cr(raw.strip_suffix('\n').unwrap_or(raw));
        let trimmed = line.trim_start();

        // Fenced-code tracking (``` or ~~~).
        let fence = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(f) = fence {
            if in_fence {
                if f == fence_marker {
                    in_fence = false;
                }
            } else {
                in_fence = true;
                fence_marker = f;
            }
            byte += line_len;
            file_line += 1;
            continue;
        }

        if !in_fence {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
                let heading = line[hashes..].trim().to_owned();
                // Close any open sections at level >= this heading.
                while let Some(&idx) = open.last() {
                    if sections[idx].level >= hashes {
                        sections[idx].body_end = byte;
                        open.pop();
                    } else {
                        break;
                    }
                }
                let content_start = byte + line_len;
                sections.push(BodySection {
                    heading,
                    level: hashes,
                    heading_line: file_line,
                    content_first_line: file_line + 1,
                    body_start: content_start,
                    body_end: body.len(),
                });
                open.push(sections.len() - 1);
            }
        }

        byte += line_len;
        file_line += 1;
    }
    // Any still-open sections extend to EOF (already set to body.len()).
    sections
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let taken: String = s.chars().take(max).collect();
        format!("{taken}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn always_enabled(_: &str) -> bool {
        true
    }

    // -------------------------------------------------------------------
    // is_index_file / is_log_file: case-sensitivity contract
    // -------------------------------------------------------------------

    #[test]
    fn is_index_file_case_sensitive_by_default() {
        assert!(!is_index_file("INDEX.md", false));
        assert!(is_index_file("index.md", false));
        assert!(is_index_file("sub/index.md", false));
        assert!(!is_index_file("sub/INDEX.md", false));
    }

    #[test]
    fn is_index_file_case_insensitive_when_enabled() {
        assert!(is_index_file("INDEX.md", true));
        assert!(is_index_file("index.md", true));
        assert!(is_index_file("sub/INDEX.md", true));
        assert!(is_index_file("sub\\INDEX.md", true), "Windows separator");
    }

    #[test]
    fn is_log_file_case_sensitive_by_default() {
        assert!(!is_log_file("LOG.md", false));
        assert!(is_log_file("log.md", false));
        assert!(is_log_file("sub/log.md", false));
        assert!(!is_log_file("sub/LOG.md", false));
    }

    #[test]
    fn is_log_file_case_insensitive_when_enabled() {
        assert!(is_log_file("LOG.md", true));
        assert!(is_log_file("log.md", true));
        assert!(is_log_file("sub/LOG.md", true));
        assert!(is_log_file("sub\\LOG.md", true), "Windows separator");
    }

    #[test]
    fn every_emitted_rule_id_is_registered() {
        // Any rule id the runtime can emit must appear in OKF_RULE_IDS so the
        // catalog (`lint-rules list`) and runtime stay in lock-step.
        let body = "# Schema\n\n# Citations\n\nprose not a link\n- [x](missing.md)\n";
        let out = run_okf_rules(
            "tables/x.md",
            Path::new("/vault/tables/x.md"),
            body,
            body,
            1,
            Some("BigQuery Table"),
            &always_enabled,
            Path::new("/vault"),
            false,
        );
        for f in &out {
            assert!(
                OKF_RULE_IDS.contains(&f.rule_id),
                "unregistered rule id emitted: {}",
                f.rule_id
            );
        }
    }

    #[test]
    fn index_markers_healthy_and_absent_pass() {
        let mut out = Vec::new();
        check_index_markers("no markers here\n", 1, &mut out);
        assert!(out.is_empty(), "no markers is fine");
        let healthy = "\n<!-- okf:index:begin -->\n* [x](x.md)\n<!-- okf:index:end -->\n";
        let mut out2 = Vec::new();
        check_index_markers(healthy, 1, &mut out2);
        assert!(
            out2.is_empty(),
            "healthy pair passes: {out2:?}",
            out2 = out2.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn index_markers_dangling_begin_warns() {
        let mut out = Vec::new();
        check_index_markers("prose\n<!-- okf:index:begin -->\nlist\n", 1, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "OKF-INDEX-MARKERS");
        assert!(out[0].message.contains("dangling"));
    }

    #[test]
    fn index_markers_dangling_end_warns() {
        let mut out = Vec::new();
        check_index_markers("<!-- okf:index:end -->\nprose\n", 1, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("dangling"));
    }

    #[test]
    fn index_markers_reversed_warns() {
        let mut out = Vec::new();
        check_index_markers(
            "<!-- okf:index:end -->\n\n<!-- okf:index:begin -->\n",
            1,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("reversed"));
    }

    #[test]
    fn index_markers_duplicate_warns() {
        let content = "<!-- okf:index:begin -->\na\n<!-- okf:index:end -->\n<!-- okf:index:begin -->\nb\n<!-- okf:index:end -->\n";
        let mut out = Vec::new();
        check_index_markers(content, 1, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("duplicate"));
    }

    #[test]
    fn index_managed_region_is_conformant() {
        let content = "\n<!-- okf:index:begin -->\n* [X](x.md)\n<!-- okf:index:end -->\n";
        let mut out = Vec::new();
        check_index_structure(content, content, 1, &mut out);
        assert!(
            out.is_empty(),
            "managed region should pass: {out:?}",
            out = out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn index_link_list_is_conformant() {
        let body = "# Tables\n\n* [Blocks](blocks.md) - the blocks\n- [Accounts](accounts.md)\n";
        let mut out = Vec::new();
        check_index_structure(body, body, 1, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn index_prose_only_warns() {
        let body = "This index has no links, just prose.\n";
        let mut out = Vec::new();
        check_index_structure(body, body, 1, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "OKF-INDEX-STRUCTURE");
    }

    #[test]
    fn empty_index_is_allowed() {
        let body = "\n\n";
        let mut out = Vec::new();
        check_index_structure(body, body, 1, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn log_newest_first_is_conformant() {
        let body = "## 2026-07-17\n- **Added** x\n\n## 2026-07-10\n- **Fixed** y\n";
        let mut out = Vec::new();
        check_log_structure(body, 1, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn log_out_of_order_warns() {
        let body = "## 2026-07-10\n- a\n\n## 2026-07-17\n- b\n";
        let mut out = Vec::new();
        check_log_structure(body, 1, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].message.contains("newest-first"));
    }

    #[test]
    fn log_without_dates_warns() {
        let body = "Just some text, no dates.\n";
        let mut out = Vec::new();
        check_log_structure(body, 1, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn log_crlf_date_heading_parses() {
        // A CRLF-terminated date heading must still be recognized (iter-165
        // retrospective: CRLF is a recurring blind spot in new okf code).
        let body = "## 2026-07-17\r\n- a\r\n\r\n## 2026-07-10\r\n- b\r\n";
        let mut out = Vec::new();
        check_log_structure(body, 1, &mut out);
        assert!(
            out.is_empty(),
            "CRLF log should be conformant: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn citations_bullets_and_numbered_ok() {
        let entries =
            "- [Wiki](https://en.wikipedia.org/x)\n1. [Spec](spec.md)\n* https://example.com\n";
        let mut out = Vec::new();
        check_citations_well_formed(entries, 1, &mut out);
        assert!(
            out.is_empty(),
            "{:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn citations_prose_warns() {
        let entries = "See the Bitcoin whitepaper for details.\n";
        let mut out = Vec::new();
        check_citations_well_formed(entries, 1, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].rule_id, "OKF-CITATIONS-WELL-FORMED");
    }

    #[test]
    fn citations_bullet_without_link_warns() {
        let entries = "- just some words, no link here\n";
        let mut out = Vec::new();
        check_citations_well_formed(entries, 1, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn reference_type_skips_citations_present() {
        assert!(!concept_makes_claims(Some("Reference")));
        assert!(!concept_makes_claims(Some("reference")));
        assert!(concept_makes_claims(Some("BigQuery Table")));
        assert!(concept_makes_claims(None));
    }

    #[test]
    fn scan_sections_finds_citations_and_content() {
        let body = "# Overview\n\nprose\n\n# Citations\n\n- [a](a.md)\n";
        let secs = scan_sections(body, 1);
        let cit = secs.iter().find(|s| s.is_heading("Citations", 1)).unwrap();
        let content = &body[cit.body_start..cit.body_end];
        assert!(content.contains("[a](a.md)"));
        assert!(!content.contains("Overview"));
    }

    #[test]
    fn scan_sections_ignores_headings_in_code_fence() {
        let body = "# Real\n\n```\n# not a heading\n```\n\n# Citations\n- [a](a.md)\n";
        let secs = scan_sections(body, 1);
        let headings: Vec<_> = secs.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(headings, vec!["Real", "Citations"]);
    }

    #[test]
    fn empty_schema_section_flags_augmentation_guard() {
        let body = "# Schema\n\n# Citations\n\n- [a](a.md)\n";
        let out = run_okf_rules(
            "tables/x.md",
            Path::new("/vault/tables/x.md"),
            body,
            body,
            1,
            Some("BigQuery Table"),
            &always_enabled,
            Path::new("/vault"),
            false,
        );
        assert!(
            out.iter()
                .any(|f| f.rule_id == "OKF-AUGMENTATION-GUARD" && f.message.contains("Schema")),
            "expected empty-Schema guard: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
    }

    // -------------------------------------------------------------------
    // Regression tests for review findings (PR #194)
    // -------------------------------------------------------------------

    #[test]
    fn ends_with_md_does_not_panic_on_multibyte_tail() {
        // A token that is (or ends with) a single multi-byte character must not
        // panic when byte-length is `< 3` after the character's start, or when
        // the naive `len() - 3` offset would land mid-character.
        assert!(!ends_with_md("😀"));
        assert!(!ends_with_md("café"));
        assert!(!ends_with_md("€"));
        assert!(ends_with_md("a.md"));
        assert!(ends_with_md("A.MD"));
        assert!(!ends_with_md(""));
    }

    #[test]
    fn item_is_link_does_not_panic_on_multibyte_token() {
        // Regression: `item_is_link` (via `ends_with_md`) used to panic on a
        // citation entry whose first token is a multi-byte, non-`.md` string.
        assert!(!item_is_link("😀 not a link"));
    }

    #[test]
    fn citations_resolve_rejects_directory_target() {
        // A citation pointing at a directory must not be reported as resolved
        // — the rule promises resolution to an existing *file*.
        let tmp = std::env::temp_dir().join(format!(
            "hyalo-okf-lint-test-{}-{}",
            std::process::id(),
            line!()
        ));
        let sub = tmp.join("references");
        std::fs::create_dir_all(&sub).unwrap();
        let entries = "- [Dir](references/)\n";
        let mut out = Vec::new();
        check_citations_resolve(entries, 1, &tmp.join("concept.md"), &tmp, &mut out);
        std::fs::remove_dir_all(&tmp).ok();
        assert_eq!(
            out.len(),
            1,
            "citation pointing at a directory must warn as unresolved: {:?}",
            out.iter().map(|f| &f.message).collect::<Vec<_>>()
        );
        assert_eq!(out[0].rule_id, "OKF-CITATIONS-RESOLVE");
    }
}
