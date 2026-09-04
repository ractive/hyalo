//! HYALO006 (`broken-link`) lint rule support.
//!
//! The rule fires when a wikilink or markdown link in a linted file points at a
//! vault file that does not exist. The catalog entry lives in `hyalo-mdlint`
//! (severity/default-on/description); the resolution logic lives here in
//! `hyalo-cli` because it needs vault-wide context (the set of files that
//! exist) which the stateless mdlint engine does not have.
//!
//! The vault-wide [`LinkLintContext`] is built **once** per `hyalo lint`
//! invocation (in the dispatch arm) and shared by reference across the rayon
//! workers — the graph is never rebuilt per file.

use std::path::{Path, PathBuf};

use hyalo_core::CaseInsensitiveIndex;
use hyalo_core::discovery;
use hyalo_core::links::{self, Link, LinkKind};
use hyalo_core::scanner::{FileVisitor, ScanAction, scan_slice_multi};

/// Vault-wide context needed to resolve links for the HYALO006 rule.
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
        })
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
    scratch: Vec<Link>,
    /// Frontmatter property allow-list, or `None` to scan every value.
    frontmatter_props: Option<&'a [String]>,
}

impl<'a> LinkCollector<'a> {
    fn new(frontmatter_props: Option<&'a [String]>) -> Self {
        Self {
            links: Vec::new(),
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

    fn on_body_line(&mut self, _raw: &str, cleaned: &str, line_num: usize) -> ScanAction {
        // Resolution only needs the target, not the label, so scanning the
        // inline-code-stripped `cleaned` line as both text and original is
        // sufficient (label fidelity is irrelevant to HYALO006).
        self.scratch.clear();
        links::extract_links_from_text(cleaned, &mut self.scratch);
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
            findings.push(BrokenLinkFinding {
                line,
                message: format!(
                    "broken {kind}: `{}` does not resolve to a vault file",
                    link.target
                ),
            });
        }
    }
    findings
}
