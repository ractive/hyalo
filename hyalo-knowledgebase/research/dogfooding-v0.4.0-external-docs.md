---
date: 2026-03-26
status: superseded
tags:
- research
- cli
- dogfooding
- backlinks
- mv
- performance
title: 'Dogfooding Report: v0.4.0 on External Documentation Repos'
type: research
---

# Dogfooding Report: v0.4.0 on External Documentation Repos

Tested hyalo 0.4.0 against two large, real-world documentation repositories:

- **GitHub Docs** (`docs/content`): 3520 markdown files, rich YAML frontmatter (36 property types), no tags
- **VS Code Docs** (`vscode-docs/docs`): 339 markdown files, 13 property types, no tags

Both repos use absolute-path markdown links (e.g., `/docs/configure/settings.md`,
`/organizations/.../roles-in-an-organization`), which exercises the site_prefix
resolution logic heavily.

## What Works Well

- **Performance is solid**: Summary on 3520 files in 0.25s, find-all in 0.47s. The `--fields` flag cuts time 2-3x when you only need specific data (e.g., `--fields properties` = 0.20s vs all fields = 0.58s).
- **jq integration is a killer feature**: Complex aggregations like grouping 3520 files by contentType in 0.23s. The built-in jq makes hyalo a one-liner analytics tool.
- **Error handling is clean**: Invalid regex, nonexistent files, broken jq expressions, malformed frontmatter — all give clear, structured error messages with helpful hints.
- **Broken frontmatter tolerance**: `code-security/concepts/index.md` has unclosed frontmatter; hyalo warns and skips it gracefully without crashing.
- **`read --section` works perfectly**: Extracts exactly the right section content.
- **`--hints` provides useful drill-down commands** for progressive exploration.
- **`find` combined filtering** (multiple `--property` flags, `-e` regex, `--section`) works as expected — powerful AND semantics.
- **Files with no/empty frontmatter** handled gracefully.
- **Properties on list types**: `--property 'category~=Actions'` correctly regex-matches list items.

## Bugs Found

### BUG-1: `mv` doesn't update absolute-path inbound links (CRITICAL)

`backlinks` correctly finds 116 files linking to `configure/settings.md`, but
`mv --dry-run` reports "No links to update" for the same file.

**Root cause**: `plan_inbound_rewrites()` in `link_rewrite.rs` normalizes link targets
using `normalize_target()` which only handles relative paths. Absolute links like
`/docs/configure/settings.md` in the raw file content don't match `configure/settings.md`
(the vault-relative old_rel). The link graph correctly strips site_prefix when *indexing*
(so `backlinks` works), but the rewrite planner doesn't apply site_prefix stripping when
*matching* links in file content.

**Reproduction**:
```bash
cd vscode-docs
hyalo backlinks --dir docs --file configure/settings.md  # → 116 backlinks
hyalo mv --dir docs --file configure/settings.md --to configure/user-settings.md --dry-run
# → "No links to update"
```

### BUG-2: Negation globs don't work

The help/cookbook shows `--glob '!**/draft-*'` but it errors:
```
{"error":"no files match pattern","path":"\\!**/index.md"}
```
The `!` is being backslash-escaped before reaching the glob matcher.

### BUG-3: `--property 'versions~=ghes'` fails on map-type properties

`versions` exists in 3518 GitHub docs files with values like `{"fpt":"*","ghec":"*","ghes":"*"}`.
The regex filter returns 0 results because it can't match against map/object-type values.
This fails silently — the user gets an empty result set with no indication the property
type is incompatible with regex matching.

## Issues / Design Observations

### ISSUE-1: site_prefix derivation from --dir is fragile

The site_prefix is derived from the `--dir` path: `--dir ../vscode-docs/docs` gives
prefix `vscode-docs/docs`, but links use `/docs/...` so the correct prefix is just `docs`.
You must `cd` into the right parent directory and use `--dir docs` for it to work.

**Suggestion**: Add an explicit `site_prefix` option to `.hyalo.toml` and/or a `--site-prefix`
CLI flag. The current derivation is too dependent on the user's working directory.

### ISSUE-2: mv converts absolute links to relative paths

When `mv` rewrites links in GitHub docs, it changes absolute-path links
(e.g., `/organizations/.../roles`) to relative paths (e.g., `../../organizations/roles.md`).
This changes the link style, which may break site-specific link resolvers that expect
absolute paths. A `--preserve-style` flag or auto-detection of absolute-path conventions
would be safer.

### ISSUE-3: --limit doesn't short-circuit the file scan

`--limit 10` on 3520 files takes 0.38s vs 0.47s for all files — only ~20% faster.
The scan reads and parses all files, then truncates. For large vaults, early termination
after N matches would be a meaningful optimization.

### ISSUE-4: --sort modified is useless on git-cloned repos

All files in a git clone share the same mtime (clone timestamp). `--sort modified` returns
alphabetical order, which is misleading. Consider documenting this limitation or supporting
git-aware sorting (using `git log --format=%ai` for last-modified).

### ISSUE-5: --dir pointing to a file doesn't error

`hyalo summary --dir ../docs/content/README.md` treats a single file as a 1-file vault
with an empty path string. Should probably error with "expected directory, got file".

### ISSUE-6: Duplicate warning on --fields links,backlinks

The broken-frontmatter warning for `code-security/concepts/index.md` appears twice —
once from the main scan and once from building the backlink index. The vault is scanned
twice.

### ISSUE-7: --glob cannot be repeated

`--glob 'actions/**/*.md' --glob 'copilot/**/*.md'` errors with "cannot be used multiple
times". A brace-expansion alternative (`--glob '{actions,copilot}/**/*.md'`) might work
but isn't documented.

## Performance Benchmarks

### hyalo find on GitHub Docs (3520 files)

| Scenario | Time |
|----------|------|
| `--fields properties` only | 0.20s |
| `--fields tasks` only | 0.26s |
| `--fields sections` only | 0.28s |
| `--fields links` only | 0.39s |
| All fields (default) | 0.58s |
| `--fields links,backlinks` (index) | 0.56s |
| `summary` | 0.25s |
| `backlinks` on popular file | 0.18s |

### hyalo vs rg (3520 files)

| Task | rg | hyalo | Ratio |
|------|-----|-------|-------|
| Find files with `title:` | 0.10s | 0.49s | 5x |
| Text search `codespace` | 0.08s | 0.30s | 4x |

hyalo is 4-5x slower than rg for raw text search, which is expected given YAML parsing overhead. The absolute times (0.3-0.5s) are very usable.

### VS Code Docs (339 files)

All operations complete in 0.05-0.10s. Excellent for interactive use.

## Overall Assessment

v0.4.0 is a substantial release. The `backlinks` command and jq integration make hyalo
genuinely useful for documentation analysis at scale. The `mv` command's design (dry-run
preview, link rewriting) is excellent — but BUG-1 (absolute-path links not rewritten)
significantly limits its utility in real-world doc repos where absolute paths are the norm.

The site_prefix mechanism is the right idea but needs to be more robust — making it an
explicit config option rather than derived from the directory path would fix most of the
link resolution issues discovered during this testing.

Priority fixes:
1. **BUG-1**: `mv` must apply site_prefix normalization when matching links for rewriting
2. **BUG-2**: Negation globs should work as documented
3. **ISSUE-1**: Explicit `site_prefix` in `.hyalo.toml`
