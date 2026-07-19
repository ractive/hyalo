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

use hyalo_core::case_index::CaseInsensitiveIndex;
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
    ) -> Option<Self> {
        let canonical_dir = discovery::canonicalize_vault_dir(vault_dir).ok()?;
        Some(Self {
            canonical_dir,
            site_prefix,
            case_index,
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
struct LinkCollector {
    links: Vec<(usize, Link)>,
    scratch: Vec<Link>,
}

impl LinkCollector {
    fn new() -> Self {
        Self {
            links: Vec::new(),
            scratch: Vec::new(),
        }
    }
}

impl FileVisitor for LinkCollector {
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
        // Body links only; frontmatter wikilinks (related/depends-on) are not
        // gated by HYALO006 in this iteration.
        false
    }
}

/// Scan `content` (the already-read file bytes) and return one finding per link
/// whose target does not resolve to a known vault file.
///
/// `rel_path` is the vault-relative path of the file being linted (used to
/// resolve source-relative markdown links). Line numbers are body-relative and
/// translated to file-absolute by the caller.
#[must_use]
pub fn check_broken_links(
    ctx: &LinkLintContext,
    content: &[u8],
    rel_path: &str,
) -> Vec<BrokenLinkFinding> {
    let mut collector = LinkCollector::new();
    // In-memory scan over the already-read content — no extra file I/O.
    if scan_slice_multi(content, &mut [&mut collector]).is_err() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    for (line, link) in collector.links {
        // Fragment-only and external links never reach here (they are dropped at
        // parse time), so every collected link is a real file reference.
        let resolved = discovery::resolve_link_from_source(
            &ctx.canonical_dir,
            rel_path,
            link.kind,
            &link.target,
            ctx.site_prefix.as_deref(),
            Some(&ctx.case_index),
        );
        if resolved.is_none() {
            let kind = match link.kind {
                LinkKind::Wikilink => "wikilink",
                LinkKind::Markdown => "markdown link",
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
