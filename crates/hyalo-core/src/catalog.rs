//! Immutable semantic link identities. Catalog paths are never filesystem write authority.
use crate::case_index::CaseInsensitiveIndex;
use crate::link_target::TargetParts;
use crate::links::LinkKind;
use std::path::Path;

/// Resolution policy is invocation data, independent from filesystem collision policy.
#[derive(Debug, Clone, Copy)]
pub struct ResolutionOptions<'a> {
    /// Permit declared aliases after all filename candidates fail.
    pub aliases: bool,
    /// Permit a site-absolute target to resolve to the note declaring its URL
    /// path under `[links] redirect_property`, after everything else fails
    /// (DEC-415). Readers set it from the index; a writer (`mv`, `links fix`,
    /// `links auto`) leaves it off, so a link that names a redirect value is
    /// never attributed to a note it would be rewritten for.
    pub redirects: bool,
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

/// Whether `target` resolves to a note only because that note declares its URL
/// path under `[links] redirect_property` (DEC-415): the redirect step is on,
/// exactly one note claims the path, and the same link resolves to no file with
/// the step off. Cheap for the common link: it returns after one hash lookup
/// unless the path is claimed.
#[must_use]
pub fn resolves_via_redirect(
    index: &CaseInsensitiveIndex,
    source: &str,
    kind: LinkKind,
    target: &str,
    options: ResolutionOptions<'_>,
) -> bool {
    if !options.redirects || !index.redirects_enabled() {
        return false;
    }
    let raw = TargetParts::split(target).resolution_path();
    if index.lookup_redirect(&raw, options.site_prefix).len() != 1 {
        return false;
    }
    matches!(
        resolve(
            index,
            source,
            kind,
            target,
            ResolutionOptions {
                redirects: false,
                ..options
            },
        ),
        Resolution::Missing | Resolution::Attachment(None)
    )
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
    // M1 (iter-322): a note may be called `What is this?.md`. A wikilink's
    // literal path, `?` included, wins over the `?query` split, so a reader
    // and `backlinks` agree with the file system; `[[plain?x=1]]` still falls
    // through to `plain.md` when no note carries the `?` in its name.
    if kind == LinkKind::Wikilink {
        let parts = TargetParts::split(target);
        if !parts.query.is_empty() {
            let literal = &target[..parts.path.len() + parts.query.len()];
            let found = resolve_core(index, source, kind, literal, false, options, &explicit_path);
            if matches!(
                found,
                Resolution::Resolved(_)
                    | Resolution::Attachment(Some(_))
                    | Resolution::Ambiguous(_)
            ) {
                return found;
            }
        }
    }
    resolve_core(index, source, kind, target, true, options, &explicit_path)
}

fn resolve_core(
    index: &CaseInsensitiveIndex,
    source: &str,
    kind: LinkKind,
    target: &str,
    split_query: bool,
    options: ResolutionOptions<'_>,
    explicit_path: &impl Fn(&str) -> Option<String>,
) -> Resolution {
    if crate::links::is_external_target(target) {
        return Resolution::External;
    }
    // iter-322 (DEC-384): the one canonical split of an authored target.
    let parts = TargetParts::split(target);
    let raw = if split_query {
        parts.resolution_path()
    } else {
        crate::link_target::resolution_path(target)
    };
    let raw = raw.as_ref();
    if raw.is_empty() {
        return Resolution::Missing;
    }
    let absolute = raw.starts_with('/');
    let trailing = raw.ends_with('/');
    let mut candidates = Vec::with_capacity(2);
    if absolute {
        candidates.push(crate::link_graph::strip_site_prefix(
            raw,
            options.site_prefix,
        ));
    } else if kind == LinkKind::Markdown || raw.starts_with("./") || raw.starts_with("../") {
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
        let resolved = resolve_path(index, candidate, trailing, explicit_path);
        if !matches!(resolved, Resolution::Missing) {
            return resolved;
        }
    }
    // iter-322 (C-1): Obsidian matches a wikilink against any path *suffix*, so
    // `[[x/note]]` names `a/x/note.md` — the spelling it writes to tell two
    // `note.md` apart. One match resolves, several are ambiguous.
    if kind == LinkKind::Wikilink && !absolute && raw.contains('/') && !raw.starts_with('.') {
        let by_suffix = index.lookup_path_suffix(raw.trim_end_matches('/'));
        if !by_suffix.is_empty() {
            return candidates_result(&by_suffix);
        }
    }
    // iter-319 (DEC-374): the shared target normalisation — `?query`, `./`,
    // `%20` — so `backlinks` agrees with `find` and HYALO006 on what is
    // ambiguous; the literal `What?` stem is tried before its `?` is stripped.
    if kind == LinkKind::Wikilink
        && !absolute
        && let Some(paths) = index.target_ambiguity(target)
    {
        return Resolution::Ambiguous(crate::case_index::sorted_candidates(paths));
    }
    let bare = raw.trim_end_matches('/');
    if !absolute && !bare.contains('/') {
        let stem = strip_md(bare);
        // iter-319 (DEC-374): "this bare name is ambiguous" is
        // `CaseInsensitiveIndex::ambiguity`'s one definition — the same call
        // that fills `find`'s `candidates` field, HYALO006's message and
        // `links fix`'s `ambiguous_links`. A filename beats an alias, an
        // ambiguous filename beats it too, and an alias two notes claim is
        // ambiguous in either `[links] aliases` mode (DEC-308), so asking it
        // first leaves the unique-name arms below to the unambiguous cases.
        if let Some(paths) = index.ambiguity(stem) {
            return Resolution::Ambiguous(crate::case_index::sorted_candidates(paths));
        }
        if let [only] = index.lookup_stem_all(stem) {
            return hit(only.to_owned());
        }
        if options.aliases
            && let [only] = index.lookup_alias_all(stem)
        {
            return hit(only.to_owned());
        }
    }
    // DEC-415: the last resort, for a site-absolute target only. A real file
    // at the path, its `.md` and directory-index forms have all been tried
    // above, so a claim on a URL never shadows a file; two claimants are
    // ambiguous, never tie-broken.
    if absolute && options.redirects {
        let claimants: Vec<String> = index
            .lookup_redirect(raw, options.site_prefix)
            .into_iter()
            .map(str::to_owned)
            .collect();
        if !claimants.is_empty() {
            return candidates_result(&claimants);
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
        // BUG-15 (iteration 311): a bare site-absolute `/` (or an absolute
        // target that is exactly the configured `site_prefix`) strips down to
        // an empty `target` here. `format!("{target}{suffix}")` would then
        // build `/index.md` -- a leading slash glued onto nothing -- which
        // the index never contains, even when the vault-root `index.md` the
        // directory-index rule means to find does exist. Drop the suffix's
        // own leading slash for the empty case so `/` reaches the same
        // vault-root `index.md` that `/dir` reaches via `dir/index.md`.
        let candidate = if target.is_empty() {
            suffix.trim_start_matches('/').to_owned()
        } else {
            format!("{target}{suffix}")
        };
        let result = literal(index, &candidate);
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
            redirects: false,
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
            redirects: false,
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
            redirects: false,
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
            redirects: false,
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
            redirects: false,
            site_prefix: None,
        };
        for target in ["b", "Nick"] {
            assert!(
                matches!(resolve(&index, "a.md", LinkKind::Wikilink, target, options), Resolution::Ambiguous(paths) if paths.len() == 2)
            );
        }
    }

    /// iter-319 (DEC-374): the catalog's `Ambiguous` and the case index's
    /// `ambiguity` are one definition, in both `[links] aliases` modes — an
    /// alias two notes claim is ambiguous either way (DEC-308), a unique one
    /// resolves only when the vault opted in.
    #[test]
    fn catalog_ambiguity_is_the_case_index_ambiguity_in_both_alias_modes() {
        let mut index = CaseInsensitiveIndex::new();
        for path in ["b/note.md", "a/note.md", "x/one.md", "x/two.md", "x/own.md"] {
            index.insert(path);
        }
        index.insert_aliases("x/one.md", ["Twin"]);
        index.insert_aliases("x/two.md", ["Twin"]);
        index.insert_aliases("x/own.md", ["Solo"]);
        for aliases in [false, true] {
            let options = ResolutionOptions {
                aliases,
                redirects: false,
                site_prefix: None,
            };
            for target in ["note", "Twin", "Solo", "nowhere"] {
                let resolved = resolve(&index, "src.md", LinkKind::Wikilink, target, options);
                let expected = index.ambiguous_candidates(target);
                match (&resolved, &expected) {
                    (Resolution::Ambiguous(paths), Some(candidates)) => {
                        assert_eq!(paths, candidates, "{target} aliases={aliases}");
                    }
                    (Resolution::Ambiguous(paths), None) => {
                        panic!(
                            "{target} aliases={aliases}: catalog ambiguous {paths:?}, index not"
                        );
                    }
                    (_, Some(_)) => {
                        panic!("{target} aliases={aliases}: index ambiguous, catalog {resolved:?}");
                    }
                    (_, None) => {}
                }
            }
        }
        let on = ResolutionOptions {
            aliases: true,
            redirects: false,
            site_prefix: None,
        };
        let off = ResolutionOptions {
            aliases: false,
            ..on
        };
        assert_eq!(
            resolve(&index, "src.md", LinkKind::Wikilink, "Solo", on).path(),
            Some("x/own.md")
        );
        assert_eq!(
            resolve(&index, "src.md", LinkKind::Wikilink, "Solo", off),
            Resolution::Missing
        );
    }

    /// iter-322 (C-1, C-12): a partial path resolves by suffix; a relative
    /// `../` wikilink is read from its source folder, not called external.
    #[test]
    fn partial_paths_and_parent_relative_wikilinks_resolve() {
        let mut index = CaseInsensitiveIndex::new();
        index.set_case_insensitive_paths(true);
        for path in [
            "a/x/note.md",
            "b/y/note.md",
            "c/x/note.md",
            "t.md",
            "o/o.md",
        ] {
            index.insert(path);
        }
        index.set_complete(true);
        let options = ResolutionOptions {
            aliases: false,
            redirects: false,
            site_prefix: None,
        };
        let wiki = |source: &str, target: &str| {
            resolve(&index, source, LinkKind::Wikilink, target, options)
        };
        assert_eq!(wiki("s.md", "y/note").path(), Some("b/y/note.md"));
        assert_eq!(wiki("s.md", "Y/Note.md").path(), Some("b/y/note.md"));
        assert_eq!(
            wiki("s.md", "x/note"),
            Resolution::Ambiguous(vec!["a/x/note.md".to_owned(), "c/x/note.md".to_owned()])
        );
        assert_eq!(wiki("s.md", "z/note"), Resolution::Missing);
        assert_eq!(wiki("sub/s.md", "../t").path(), Some("t.md"));
        assert_eq!(wiki("sub/s.md", "../o/o").path(), Some("o/o.md"));
        assert_eq!(wiki("s.md", "../t"), Resolution::External);
        // `[links] case_insensitive = "false"` means exact bytes (R3).
        let mut exact = index.clone();
        exact.set_case_insensitive_paths(false);
        let strict = |target: &str| resolve(&exact, "s.md", LinkKind::Wikilink, target, options);
        assert_eq!(strict("y/note").path(), Some("b/y/note.md"));
        assert_eq!(strict("Y/Note"), Resolution::Missing);
    }

    /// DEC-415: the redirect map is the last resort, answers a site-absolute
    /// target only, never shadows a file, and two claimants are ambiguous.
    #[test]
    fn a_redirect_is_the_last_resort_for_a_site_absolute_target() {
        let mut index = CaseInsensitiveIndex::new();
        index.set_case_insensitive_paths(true);
        for path in ["real.md", "dir/index.md", "claim.md", "one.md", "two.md"] {
            index.insert(path);
        }
        index.insert_redirects("claim.md", ["/real", "/dir", "/via"]);
        index.insert_redirects("one.md", ["/dup"]);
        index.insert_redirects("two.md", ["/dup"]);
        let on = ResolutionOptions {
            aliases: false,
            redirects: true,
            site_prefix: None,
        };
        let off = ResolutionOptions {
            redirects: false,
            ..on
        };
        let md =
            |target: &str, options| resolve(&index, "src.md", LinkKind::Markdown, target, options);
        assert_eq!(md("/via", on).path(), Some("claim.md"));
        assert_eq!(md("/VIA/", on).path(), Some("claim.md"));
        assert_eq!(md("/via", off), Resolution::Missing);
        // A real file, or a directory index, outranks a claim on its path.
        assert_eq!(md("/real", on).path(), Some("real.md"));
        assert_eq!(md("/dir", on).path(), Some("dir/index.md"));
        // Two claimants: ambiguous, never tie-broken.
        assert_eq!(
            md("/dup", on),
            Resolution::Ambiguous(vec!["one.md".to_owned(), "two.md".to_owned()])
        );
        assert_eq!(md("/dup", off), Resolution::Missing);
        // A relative or bare target names a file, not a URL.
        assert_eq!(md("via", on), Resolution::Missing);
        let wiki = resolve(&index, "src.md", LinkKind::Wikilink, "via", on);
        assert_eq!(wiki, Resolution::Missing);
        let wiki_abs = resolve(&index, "src.md", LinkKind::Wikilink, "/via", on);
        assert_eq!(wiki_abs.path(), Some("claim.md"));
        // The label `via: "redirect"` is for a link only the map resolved.
        let via = |target: &str, options| {
            resolves_via_redirect(&index, "src.md", LinkKind::Markdown, target, options)
        };
        assert!(via("/via", on));
        assert!(!via("/via", off));
        assert!(!via("/real", on));
        assert!(!via("/dup", on));
        assert!(!via("/nope", on));
    }
}
