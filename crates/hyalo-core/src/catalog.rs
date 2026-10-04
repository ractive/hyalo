//! Immutable semantic link identities. Catalog paths are never filesystem write authority.
use crate::case_index::CaseInsensitiveIndex;
use crate::links::LinkKind;
use std::path::Path;

/// Resolution policy is invocation data, independent from filesystem collision policy.
#[derive(Debug, Clone, Copy)]
pub struct ResolutionOptions<'a> {
    /// Permit declared aliases after all filename candidates fail.
    pub aliases: bool,
    /// Prefix stripped from site-absolute targets.
    pub site_prefix: Option<&'a str>,
}

/// A source-aware semantic result retaining ambiguity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Canonical catalog note path.
    Resolved(String),
    /// An external URI or a target outside the vault.
    External,
    /// Attachment target, with its canonical path when known.
    Attachment(Option<String>),
    /// No candidate in this catalog.
    Missing,
    /// Candidates with equal precedence; never choose one arbitrarily.
    Ambiguous(Vec<String>),
}
impl Resolution {
    /// Canonical note or attachment path, when uniquely known.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::Resolved(path) | Self::Attachment(Some(path)) => Some(path),
            _ => None,
        }
    }
}

/// Frozen paths, stems, directory candidates and alias declarations for a generation.
#[derive(Debug)]
pub struct VaultCatalog {
    index: CaseInsensitiveIndex,
    generation: u64,
}
impl VaultCatalog {
    /// Freeze a complete or partial lookup independently from selected scan coverage.
    pub fn new(index: CaseInsensitiveIndex, generation: u64) -> Self {
        Self { index, generation }
    }
    /// Generation of the retained catalog inputs.
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Whether a missing result proves absence from the whole vault.
    pub fn is_complete(&self) -> bool {
        self.index.is_complete()
    }
    /// Compatibility view for callers migrating from the public case index.
    pub fn case_index(&self) -> &CaseInsensitiveIndex {
        &self.index
    }
    /// Resolve authored target text without probing the filesystem.
    pub fn resolve(
        &self,
        source: &str,
        kind: LinkKind,
        target: &str,
        options: ResolutionOptions<'_>,
    ) -> Resolution {
        resolve(&self.index, source, kind, target, options)
    }
}

/// Shared resolver for catalog owners and compatibility case-index callers.
/// The authored occurrence (kind, text, fragment and source line) stays with its caller.
pub fn resolve(
    index: &CaseInsensitiveIndex,
    source: &str,
    kind: LinkKind,
    target: &str,
    options: ResolutionOptions<'_>,
) -> Resolution {
    resolve_with_explicit_paths(index, source, kind, target, options, |_| None)
}

/// Read-side adapter for explicitly named files omitted from discovery. The
/// supplied lookup must not expand the catalog: such files are attachments,
/// even when their extension is Markdown, because they are not scanned notes.
pub(crate) fn resolve_with_explicit_paths(
    index: &CaseInsensitiveIndex,
    source: &str,
    kind: LinkKind,
    target: &str,
    options: ResolutionOptions<'_>,
    explicit_path: impl Fn(&str) -> Option<String>,
) -> Resolution {
    if crate::links::is_external_target(target) {
        return Resolution::External;
    }
    let raw = target.trim().replace('\\', "/");
    let raw = raw.split(['#', '?']).next().unwrap_or("").trim_end();
    if raw.is_empty() {
        return Resolution::Missing;
    }
    let decoded = crate::discovery::percent_decode_path(raw);
    let raw = decoded.as_deref().unwrap_or(raw);
    let absolute = raw.starts_with('/');
    let trailing = raw.ends_with('/');
    let mut candidates = Vec::with_capacity(2);
    if absolute {
        candidates.push(crate::link_graph::strip_site_prefix(
            raw,
            options.site_prefix,
        ));
    } else if kind == LinkKind::Markdown || raw.starts_with("./") {
        candidates.push(crate::link_graph::normalize_target(Path::new(source), raw));
        // Markdown file links can name a path from the vault root without a
        // leading slash (Obsidian's "Absolute path in vault" format). Prefer
        // the source-relative candidate, preserving existing precedence.
        if kind == LinkKind::Markdown && crate::discovery::allows_vault_relative_fallback(raw) {
            candidates.push(raw.to_owned());
        }
    } else {
        candidates.push(raw.to_owned());
    }
    // Obsidian permits partially qualified attachment embeds relative to the source.
    if kind == LinkKind::Wikilink
        && !absolute
        && raw.contains('/')
        && crate::discovery::has_non_md_extension(raw)
    {
        candidates.push(crate::link_graph::normalize_target(Path::new(source), raw));
    }
    for candidate in &candidates {
        let candidate = candidate.trim_end_matches('/');
        if crate::discovery::normalized_target_escapes_vault(candidate) {
            return Resolution::External;
        }
        let resolved = resolve_path(index, candidate, trailing, &explicit_path);
        if !matches!(resolved, Resolution::Missing) {
            return resolved;
        }
    }
    let bare = raw.trim_end_matches('/');
    if !absolute && !bare.contains('/') {
        let stem = strip_md(bare);
        let paths = index.lookup_stem_all(stem);
        if !paths.is_empty() {
            return candidates_result(paths);
        }
        if options.aliases {
            let paths = index.lookup_alias_all(stem);
            if !paths.is_empty() {
                return candidates_result(paths);
            }
        }
    }
    if crate::discovery::has_non_md_extension(raw) || crate::discovery::has_hidden_component(raw) {
        Resolution::Attachment(None)
    } else {
        Resolution::Missing
    }
}
fn strip_md(path: &str) -> &str {
    if path
        .as_bytes()
        .get(path.len().saturating_sub(3)..)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(b".md"))
    {
        &path[..path.len() - 3]
    } else {
        path
    }
}
fn hit(path: String) -> Resolution {
    if crate::discovery::has_non_md_extension(&path) {
        Resolution::Attachment(Some(path))
    } else {
        Resolution::Resolved(path)
    }
}
fn candidates_result(paths: &[String]) -> Resolution {
    match paths {
        [] => Resolution::Missing,
        [only] => hit(only.to_owned()),
        _ => {
            let mut paths = paths.to_vec();
            paths.sort();
            Resolution::Ambiguous(paths)
        }
    }
}
fn literal(index: &CaseInsensitiveIndex, path: &str) -> Resolution {
    if index.contains_path(path) {
        return hit(path.to_owned());
    }
    candidates_result(index.lookup_all(path))
}
fn resolve_path(
    index: &CaseInsensitiveIndex,
    target: &str,
    trailing: bool,
    explicit_path: &impl Fn(&str) -> Option<String>,
) -> Resolution {
    let direct = literal(index, target);
    if !matches!(direct, Resolution::Missing) {
        return direct;
    }
    if let Some(path) = explicit_path(target) {
        return Resolution::Attachment(Some(path));
    }
    // `target` already failed the literal (and case-insensitive) lookup above,
    // so it is not a *known* file under its own spelling — attachment or
    // note. A target already ending in `.md` has nothing left to append
    // (`foo.md.md` / `foo.md/index.md` are never useful), but a dotted stem
    // whose trailing segment merely *looks* like a non-`.md` extension
    // (`sub/rel-1.2`, `v1.2.3`) must still get the `.md`-suffix attempt below:
    // it may be a note whose stem happens to contain a dot, and no existing
    // attachment has claimed the unsuffixed spelling (bug: F3, iteration 306 review).
    if strip_md(target) != target {
        return Resolution::Missing;
    }
    for suffix in if trailing {
        ["/index.md", ".md"]
    } else {
        [".md", "/index.md"]
    } {
        let result = literal(index, &format!("{target}{suffix}"));
        if !matches!(result, Resolution::Missing) {
            return result;
        }
    }
    Resolution::Missing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn omitted_hidden_paths_are_not_document_graph_edges() {
        let mut index = CaseInsensitiveIndex::new();
        index.set_complete(true);
        let options = ResolutionOptions {
            aliases: false,
            site_prefix: None,
        };
        for target in [".gitignore", ".hidden/note.md", ".github/lint.yml"] {
            assert_eq!(
                resolve(&index, "README.md", LinkKind::Markdown, target, options),
                Resolution::Attachment(None)
            );
        }
        let result = resolve_with_explicit_paths(
            &index,
            "notes/note.md",
            LinkKind::Markdown,
            ".gitignore",
            options,
            |candidate| {
                (candidate == "notes/.gitignore" || candidate == ".gitignore")
                    .then(|| candidate.to_owned())
            },
        );
        assert_eq!(
            result,
            Resolution::Attachment(Some("notes/.gitignore".to_owned()))
        );
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn source_relative_precedence_and_explicit_policy() {
        let mut index = CaseInsensitiveIndex::new();
        for path in ["b.md", "sub/b.md", "sub/a.md", "page/index.md"] {
            index.insert(path);
        }
        index.insert_aliases("sub/b.md", ["Nick"]);
        index.set_complete(true);
        let catalog = VaultCatalog::new(index, 7);
        let options = ResolutionOptions {
            aliases: true,
            site_prefix: Some("docs"),
        };
        for (kind, target, expected) in [
            (LinkKind::Markdown, "b.md", "sub/b.md"),
            (LinkKind::Markdown, "/docs/b.md", "b.md"),
            (LinkKind::Wikilink, " b ", "b.md"),
            (LinkKind::Wikilink, "./b", "sub/b.md"),
            (LinkKind::Wikilink, "Nick", "sub/b.md"),
            (LinkKind::Markdown, "/docs/page/", "page/index.md"),
        ] {
            assert_eq!(
                catalog.resolve("sub/a.md", kind, target, options).path(),
                Some(expected)
            );
        }
        assert_eq!(
            catalog.resolve(
                "sub/a.md",
                LinkKind::Wikilink,
                "Nick",
                ResolutionOptions {
                    aliases: false,
                    ..options
                }
            ),
            Resolution::Missing
        );
        assert!(catalog.is_complete());
        assert_eq!(catalog.generation(), 7);
    }
    #[test]
    fn dotted_stem_with_path_segment_still_tries_md_suffix() {
        // F3 (iteration 306 review): `[[sub/rel-1.2]]` was reported broken
        // because `.2` was mistaken for a non-`.md` extension once a
        // directory segment made `has_non_md_extension` apply; the bare form
        // `[[rel-1.2]]` already resolved via the stem-lookup path, which
        // never ran this extension check.
        let mut index = CaseInsensitiveIndex::new();
        index.insert("sub/rel-1.2.md");
        index.set_complete(true);
        let options = ResolutionOptions {
            aliases: false,
            site_prefix: None,
        };
        assert_eq!(
            resolve(&index, "a.md", LinkKind::Wikilink, "sub/rel-1.2", options).path(),
            Some("sub/rel-1.2.md")
        );
        assert_eq!(
            resolve(&index, "a.md", LinkKind::Markdown, "sub/rel-1.2", options).path(),
            Some("sub/rel-1.2.md")
        );
    }

    #[test]
    fn genuine_non_md_extension_in_a_subpath_stays_an_attachment() {
        // A real attachment reference (no matching `.md` anywhere) must keep
        // reporting as an unresolved attachment, not a broken note link.
        let mut index = CaseInsensitiveIndex::new();
        index.set_complete(true);
        let options = ResolutionOptions {
            aliases: false,
            site_prefix: None,
        };
        assert_eq!(
            resolve(
                &index,
                "a.md",
                LinkKind::Wikilink,
                "sub/missing.png",
                options
            ),
            Resolution::Attachment(None)
        );
    }

    #[test]
    fn ambiguity_outranks_alias_and_survives_catalog_generations() {
        let mut index = CaseInsensitiveIndex::new();
        index.insert("one/b.md");
        index.insert("two/b.md");
        index.insert_aliases("one/b.md", ["b", "Nick"]);
        index.insert_aliases("two/b.md", ["Nick"]);
        let options = ResolutionOptions {
            aliases: true,
            site_prefix: None,
        };
        for target in ["b", "Nick"] {
            assert!(
                matches!(resolve(&index, "a.md", LinkKind::Wikilink, target, options), Resolution::Ambiguous(paths) if paths.len() == 2)
            );
        }
    }
}
