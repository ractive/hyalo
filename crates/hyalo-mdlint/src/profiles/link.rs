//! Vault-aware HYALO006 (broken-link) and HYALO008 (broken-heading-anchor).
//!
//! The stateless body engine supplies the catalog and configuration. CLI lint
//! builds one [`LinkLintContext`] per invocation and shares it across workers.
//! Heading validation is opt-in on that context so a disabled or filtered-out
//! anchor rule performs no heading work. Targets use the shared resolver and
//! heading matcher; cached outlines are never rebuilt per link.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use hyalo_core::types::OutlineSection;

type CachedSections = Arc<OnceLock<Option<Vec<OutlineSection>>>>;

use hyalo_core::CaseInsensitiveIndex;
use hyalo_core::discovery;
use hyalo_core::links::{self, Link, LinkKind};
use hyalo_core::scanner::{FileVisitor, ScanAction, scan_slice_multi};

/// Vault-wide context needed to resolve file targets and heading anchors.
///
/// Built once per invocation and borrowed by every worker. Cheap to share:
/// resolution reads the [`CaseInsensitiveIndex`] and touches the filesystem
/// only through `resolve_target`'s `is_file` probe (never re-walks the vault).
pub struct LinkLintContext {
    /// Pre-canonicalized vault root (see `discovery::canonicalize_vault_dir`).
    canonical_dir: PathBuf,
    /// Resolved `[links] site_prefix`, if any.
    site_prefix: Option<String>,
    /// Case/stem index over every vault file.
    case_index: CaseInsensitiveIndex,
    /// Frontmatter properties scanned for `[[wikilink]]` values (iter-262).
    /// `None` scans every frontmatter value — the default; `Some(list)` is the
    /// `[links] frontmatter = false` / `frontmatter_properties` opt-out.
    frontmatter_props: Option<Vec<String>>,
    /// One lazy heading read per distinct target, including failed reads.
    /// Absent when HYALO008 is disabled or filtered out.
    anchor_sections: Option<Mutex<HashMap<String, CachedSections>>>,
}

impl LinkLintContext {
    /// Build a context from the vault directory, site prefix, and a prepared
    /// case index (typically from `dispatch::maybe_case_index`, which seeds it
    /// from the snapshot when `--index` is active — no disk walk).
    #[must_use]
    pub fn new(
        vault_dir: &Path,
        site_prefix: Option<String>,
        case_index: CaseInsensitiveIndex,
        frontmatter_props: Option<Vec<String>>,
    ) -> Option<Self> {
        let canonical_dir = discovery::canonicalize_vault_dir(vault_dir).ok()?;
        Some(Self {
            canonical_dir,
            site_prefix,
            case_index,
            frontmatter_props,
            anchor_sections: None,
        })
    }
    /// Enable heading validation, reusing snapshot outlines where available.
    /// Only called when the anchor rule is selected.
    #[must_use]
    pub fn with_anchors(mut self, snapshot: Option<&dyn hyalo_core::index::VaultIndex>) -> Self {
        let mut cache = HashMap::new();
        if let Some(snapshot) = snapshot {
            for entry in snapshot.entries() {
                cache.insert(
                    entry.rel_path.clone(),
                    Arc::new(OnceLock::from(Some(entry.sections.clone()))),
                );
            }
        }
        self.anchor_sections = Some(Mutex::new(cache));
        self
    }

    fn target_sections(&self, target: &str) -> Option<CachedSections> {
        let cache = self.anchor_sections.as_ref()?;
        let mut cache = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Some(Arc::clone(cache.entry(target.to_owned()).or_default()))
    }
}

/// A single broken-link finding: the 1-based body line and a human message.
pub struct BrokenLinkFinding {
    pub line: usize,
    pub message: String,
}

/// Visitor that collects `(body_line, Link)` pairs for every real link.
///
/// Uses the scanner's `cleaned` line (inline code / comments stripped) so that
/// links inside backtick spans or HTML comments are not treated as real links,
/// matching how the link graph and `find` index links.
struct LinkCollector<'a> {
    links: Vec<(usize, Link)>,
    anchors: Vec<links::SelfAnchor>,
    collect_anchors: bool,
    scratch: Vec<Link>,
    /// Frontmatter property allow-list, or `None` to scan every value.
    frontmatter_props: Option<&'a [String]>,
}

impl<'a> LinkCollector<'a> {
    fn new(frontmatter_props: Option<&'a [String]>) -> Self {
        Self {
            links: Vec::new(),
            anchors: Vec::new(),
            collect_anchors: false,
            scratch: Vec::new(),
            frontmatter_props,
        }
    }
}

impl FileVisitor for LinkCollector<'_> {
    /// iter-262 (BUG-1): a `[[wikilink]]` in a frontmatter value is a real
    /// vault reference, so a broken one is a broken link — HYALO006 gates it
    /// exactly like a body link, on the same file-absolute line number.
    fn on_frontmatter_text(&mut self, yaml: &str, first_line: usize) {
        hyalo_core::frontmatter_links::extract_frontmatter_links(
            yaml,
            first_line,
            self.frontmatter_props,
            &mut self.links,
        );
    }

    fn on_body_line(&mut self, raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
        // Resolution only needs the target, not the label, so scanning the
        // inline-code-stripped `cleaned` line as both text and original is
        // sufficient (label fidelity is irrelevant to HYALO006).
        self.scratch.clear();
        if self.collect_anchors {
            let start = self.anchors.len();
            links::extract_links_and_self_anchors(
                cleaned,
                raw,
                &mut self.scratch,
                &mut self.anchors,
            );
            for anchor in &mut self.anchors[start..] {
                anchor.line = line_num;
            }
        } else {
            links::extract_links_from_text(cleaned, &mut self.scratch);
        }
        for link in self.scratch.drain(..) {
            self.links.push((line_num, link));
        }
        ScanAction::Continue
    }

    fn needs_frontmatter(&self) -> bool {
        // `on_frontmatter_text` only fires when some visitor asks for
        // frontmatter — the scanner does not accumulate the block otherwise.
        // An empty allow-list means frontmatter links are off entirely.
        self.frontmatter_props.is_none_or(|props| !props.is_empty())
    }
}

/// The vault files a bare, unresolved target is ambiguous between — two files
/// sharing a basename stem, or two notes declaring the same frontmatter alias
/// (iter-275, ALIAS-5 / BUG-26).
///
/// Empty for everything else, including a path-form target (which asserts a
/// location and is simply missing) and a markdown destination (whose `/`-less
/// form is resolved against the source folder, not by stem).
fn ambiguous_candidates(
    case_index: &CaseInsensitiveIndex,
    kind: LinkKind,
    target: &str,
) -> Vec<String> {
    if kind != LinkKind::Wikilink {
        return Vec::new();
    }
    let target = target.trim();
    if target.is_empty() || target.contains('/') || target.contains('\\') {
        return Vec::new();
    }
    let stem = target
        .strip_suffix(".md")
        .or_else(|| target.strip_suffix(".MD"))
        .unwrap_or(target);
    let stem = stem.split('#').next().unwrap_or(stem).trim_end();
    if stem.is_empty() {
        return Vec::new();
    }
    let by_stem = case_index.lookup_stem_all(stem);
    if by_stem.len() > 1 {
        return by_stem.to_vec();
    }
    let by_alias = case_index.lookup_alias_all(stem);
    if by_stem.is_empty() && by_alias.len() > 1 {
        return by_alias.to_vec();
    }
    Vec::new()
}

/// Scan `content` (the already-read file bytes) and return one finding per link
/// whose target does not resolve to a known vault file.
///
/// `rel_path` is the vault-relative path of the file being linted (used to
/// resolve source-relative markdown links).
///
/// `content` is the **whole file** (frontmatter included), so the line numbers
/// the scanner hands the visitor — and therefore the ones on the returned
/// findings — are already **file-absolute**. Callers must not add a
/// frontmatter offset on top (iter-211 / BUG-9: doing so reported a link on
/// line 5 of a 3-line-frontmatter file at line 8).
#[must_use]
pub fn check_broken_links(
    ctx: &LinkLintContext,
    content: &[u8],
    rel_path: &str,
) -> Vec<BrokenLinkFinding> {
    let mut collector = LinkCollector::new(ctx.frontmatter_props.as_deref());
    // In-memory scan over the already-read content — no extra file I/O.
    if scan_slice_multi(content, &mut [&mut collector]).is_err() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for (line, link) in collector.links {
        // iter-261 / BUG-2: an external URI (`obsidian://`, `mailto:`, `http`)
        // is inventoried by the parser but names nothing in the vault, so it is
        // never a broken link. Fragment-only links are still dropped at parse
        // time, so every remaining link is a real file reference.
        if link.external {
            continue;
        }
        let resolved = discovery::resolve_link_from_source(
            &ctx.canonical_dir,
            rel_path,
            link.kind,
            &link.target,
            ctx.site_prefix.as_deref(),
            Some(&ctx.case_index),
        );
        if resolved.is_none() {
            let kind = if link.is_frontmatter() {
                "frontmatter wikilink"
            } else {
                match link.kind {
                    LinkKind::Wikilink => "wikilink",
                    LinkKind::Markdown => "markdown link",
                }
            };
            // BUG-26 (dogfood v0.22.0), iter-275 ALIAS-5: "does not resolve"
            // is the wrong diagnosis for a target two files — or two
            // `aliases:` declarations — both answer to. The fix is to
            // disambiguate, not to create the note, so the message names the
            // candidates the way `mv`'s `skipped_ambiguous` does.
            let candidates = ambiguous_candidates(&ctx.case_index, link.kind, &link.target);
            let message = if candidates.is_empty() {
                format!(
                    "broken {kind}: `{}` does not resolve to a vault file",
                    link.target
                )
            } else {
                format!(
                    "ambiguous {kind}: `{}` matches {} candidates: {}",
                    link.target,
                    candidates.len(),
                    candidates.join(", ")
                )
            };
            findings.push(BrokenLinkFinding { line, message });
        }
    }
    findings
}

/// Check heading fragments only after the file target resolves. The outline
/// cache is shared across workers and each target is parsed at most once.
#[must_use]
pub fn check_broken_anchors(
    ctx: &LinkLintContext,
    content: &[u8],
    rel_path: &str,
) -> Vec<BrokenLinkFinding> {
    if ctx.anchor_sections.is_none() {
        return Vec::new();
    }
    let mut collector = LinkCollector::new(ctx.frontmatter_props.as_deref());
    collector.collect_anchors = true;
    if scan_slice_multi(content, &mut [&mut collector]).is_err() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let source_sections = OnceLock::new();
    let mut check = |line: usize, target: &str, fragment: &str| {
        // The matcher handles encoded blocks/templates too; no headings are
        // needed when the fragment is unconditionally accepted.
        if hyalo_core::anchor::fragment_matches_headings(fragment, &[]) {
            return;
        }
        let broken = if target == rel_path {
            source_sections
                .get_or_init(|| hyalo_core::index::scan_slice_sections(content).ok())
                .as_deref()
                .is_some_and(|sections| {
                    !hyalo_core::anchor::fragment_matches_headings(fragment, sections)
                })
        } else if let Some(cell) = ctx.target_sections(target) {
            cell.get_or_init(|| {
                hyalo_core::index::scan_file_sections(&ctx.canonical_dir.join(target)).ok()
            })
            .as_deref()
            .is_some_and(|sections| {
                !hyalo_core::anchor::fragment_matches_headings(fragment, sections)
            })
        } else {
            false
        };
        if broken {
            findings.push(BrokenLinkFinding {
                line,
                message: format!(
                    "broken heading anchor: `#{fragment}` does not match a heading in `{target}`"
                ),
            });
        }
    };
    for anchor in collector.anchors {
        check(anchor.line, rel_path, &anchor.fragment);
    }
    for (line, link) in collector.links {
        if link.external {
            continue;
        }
        let Some(fragment) = link.fragment.as_deref() else {
            continue;
        };
        // Only discovered Markdown documents have checkable outlines.
        // Attachments and omitted hidden/excluded files remain existence-only
        // targets; a missing file is HYALO006, never an anchor failure too.
        if let hyalo_core::catalog::Resolution::Resolved(target) = hyalo_core::catalog::resolve(
            &ctx.case_index,
            rel_path,
            link.kind,
            &link.target,
            hyalo_core::catalog::ResolutionOptions {
                aliases: ctx.case_index.aliases_enabled(),
                site_prefix: ctx.site_prefix.as_deref(),
            },
        ) {
            check(line, &target, fragment);
        }
    }
    findings.sort_by_key(|finding| finding.line);
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(dir: &Path) -> LinkLintContext {
        let mut index = CaseInsensitiveIndex::default();
        index.insert("source.md");
        index.insert("target.md");
        index.insert("attachment.txt");
        LinkLintContext::new(dir, None, index, None).unwrap()
    }

    #[test]
    fn anchor_targets_share_one_cached_outline() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("target.md"), "## Real\n").unwrap();
        let ctx = context(dir.path()).with_anchors(None);
        let content = b"[one](target.md#absent)\n[two](target.md#absent)\n";
        assert_eq!(check_broken_anchors(&ctx, content, "source.md").len(), 2);
        // If a second file/occurrence re-read the target, this newly-added
        // heading would incorrectly make the invocation's verdict change.
        std::fs::write(dir.path().join("target.md"), "## absent\n").unwrap();
        assert_eq!(check_broken_anchors(&ctx, content, "source.md").len(), 2);
        assert_eq!(
            ctx.anchor_sections.as_ref().unwrap().lock().unwrap().len(),
            1
        );
    }

    #[test]
    fn disabled_rule_and_non_document_targets_need_no_heading_cache() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = context(dir.path());
        assert!(check_broken_anchors(&ctx, b"[x](#absent)", "source.md").is_empty());
        let ctx = ctx.with_anchors(None);
        std::fs::write(dir.path().join("attachment.txt"), "text\n").unwrap();
        std::fs::write(dir.path().join(".hidden.md"), "## Present\n").unwrap();
        assert!(
            check_broken_anchors(
                &ctx,
                b"[a](attachment.txt#absent)\n[b](.hidden.md#absent)",
                "source.md"
            )
            .is_empty()
        );
        assert!(
            ctx.anchor_sections
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .is_empty()
        );
    }
}
