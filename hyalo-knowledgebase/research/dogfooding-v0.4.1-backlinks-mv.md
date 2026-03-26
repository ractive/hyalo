---
date: 2026-03-26
status: completed
tags:
- dogfooding
- v0.4.1
- external-docs
title: Dogfooding v0.4.1 — Backlinks & mv Commands on External Docs
type: research
---

## Test Environment

- **GitHub Docs** (`../docs/content`): 3520 markdown files
- **VS Code Docs** (`cd ../vscode-docs && hyalo --dir docs ...`): ~600 markdown files
- hyalo v0.4.1 installed at `/Users/james/.cargo/bin/hyalo`

## Backlinks Command

### What Works Well

1. **Correct counting**: `find --fields backlinks` + jq correctly identifies top-linked files. GitHub Docs: `graphql/reference/objects.md` (42 backlinks). VS Code Docs: `configure/settings.md` (116 backlinks).
2. **JSON output**: Clean structure with `file`, `backlinks[]` (source, line, target, label), `total`. All fields populated correctly.
3. **Text output**: Readable format `source:line ("label")` — easy to scan.
4. **jq integration**: Composing jq filters on backlinks works seamlessly (e.g., extracting unique sources, labels, counts).
5. **Orphan files**: Files with zero backlinks return `{"backlinks": [], "total": 0}` — clean, no crash.
6. **Non-existent files**: Returns proper JSON error `{"error": "file not found", "path": "..."}` with exit code 1.
7. **Performance**: Backlinks scan on 3520 files completes in **0.15s** — excellent.
8. **Both link styles detected**: Markdown links (`[text](path)`) and links without `.md` extension are both tracked.

### Issues Found

#### BUG-1: Self-links included in backlinks (Severity: Low)

`backlinks --file graphql/reference/objects.md` includes 1 backlink where `source == "graphql/reference/objects.md"` (the file links to itself). This is technically correct (the file does contain a self-referencing link) but most users expect "backlinks" to mean links from **other** files. Consider filtering self-links or adding a flag to control this.

#### BUG-2: Inconsistent target format — with and without .md extension (Severity: Low)

For `configure/settings.md` in VS Code Docs, the `target` field shows both `"configure/settings"` (no extension) and `"configure/settings.md"` (with extension), depending on how the linking file wrote the link. This is accurate representation of what's in the source files, but makes it harder to filter/group programmatically.

#### ISSUE-6 (still present): Duplicate warning on backlinks scan

When using `find --fields backlinks` on a vault with broken frontmatter, the warning appears twice (once for the regular scan, once for the backlinks scan). Already tracked.

#### UX: No `--sort-by backlinks` on find command

There's no way to sort find results by backlink count directly. You must use jq: `--jq 'sort_by(-(.backlinks | length))'`. A `--sort backlinks` option would be a natural addition.

#### UX: Hints not shown for orphan backlinks result

`backlinks --file README.md --format text --hints` on an orphan file shows no hints. Could suggest: `hyalo find --file README.md --fields links` to see what the orphan links to.

## mv Command (--dry-run only)

### What Works Well

1. **Dry-run mode**: `--dry-run` works perfectly — shows all planned changes without modifying files. Essential for testing on repos you don't own.
2. **JSON output**: Clean structure with `from`, `to`, `dry_run`, `updated_files[]` (file, replacements[]), `total_files_updated`, `total_links_updated`.
3. **Text output**: Excellent readability — `Would move X -> Y`, then per-file diffs showing old/new link text.
4. **Consistent with backlinks**: mv reports same link count (42) as backlinks for the same file — the graph is consistent.
5. **Error handling**: Good error messages for all tested error cases:
   - Non-existent source: `{"error": "file not found"}`
   - Existing destination: `{"error": "target file already exists"}`
   - Same source/dest: `{"error": "source and destination are the same path"}`
   - Missing .md extension: `{"error": "target path must end with .md", "hint": "did you mean new-readme.md?"}`
   - Leading slash in --to: `{"error": "target path must be relative and within the vault"}`
   - Leading slash in --file: `{"error": "file not found"}` (could be more helpful)
6. **Internal link rewriting**: When moving a file to a deeper directory, relative links inside the moved file are correctly adjusted (e.g., `../contributing/` becomes `../../../../../contributing/`).
7. **Fragment preservation**: `#section` fragments in links are preserved through rewrites.
8. **Performance**: mv dry-run on 3520 files completes in **0.16s**.

### Issues Found

#### BUG-3: Absolute-path links rewritten with --dir prefix leaking (Severity: HIGH)

**Reproduction**: GitHub Docs uses root-absolute links like `[text](/graphql/reference/objects#section)`.

```
hyalo --dir ../docs/content mv --file graphql/reference/objects.md \
  --to graphql/reference/all-objects.md --dry-run
```

**Expected**: `[text](/graphql/reference/all-objects#section)`
**Actual**: `[text](/../docs/content/graphql/reference/all-objects#section)`

The `--dir` value (`../docs/content`) leaks into the rewritten link path. With an absolute `--dir` path it's even worse: `[text](//Users/james/devel/docs/content/graphql/reference/all-objects#section)`.

This bug does NOT manifest when the `--dir` value appears as a prefix in the links. VS Code Docs links use `/docs/configure/settings.md` with `--dir docs`, so the `/docs/` prefix matches and rewrites correctly.

**Root cause hypothesis**: The link rewriter resolves absolute links (`/path`) by joining the `--dir` value with the new relative path, but doesn't strip the `--dir` prefix. It should recognize that `/` means "vault root" and rewrite accordingly.

**Impact**: Any vault where links use root-relative paths (`/foo/bar`) without including the `--dir` value as prefix will produce corrupted links. This is common in static site generators (Hugo, Jekyll, Docusaurus) where `/` means "site root."

#### BUG-4: Case-only rename fails on macOS (Severity: Low)

`mv --file README.md --to Readme.md` fails with `target file already exists` on macOS (HFS+/APFS case-insensitive). This is a filesystem limitation, but could be handled by doing a two-step rename (README.md -> temp.md -> Readme.md) or at least with a more specific error message mentioning case-insensitive filesystem.

#### UX: --file with leading slash gives unhelpful error

`backlinks --file /graphql/reference/objects.md` gives `file not found` without any hint that the leading slash is the problem. Could add: `"hint": "paths should be relative to --dir, not start with /"`.

#### UX: ./prefix works but /-prefix doesn't

`--file ./graphql/reference/objects.md` works fine, but `--file /graphql/reference/objects.md` fails. This inconsistency could trip up users.

## Summary of Findings

| ID | Issue | Severity | Command |
|----|-------|----------|---------|
| BUG-1 | Self-links included in backlinks | Low | backlinks |
| BUG-2 | Inconsistent target format (with/without .md) | Low | backlinks |
| BUG-3 | --dir prefix leaks into absolute-path link rewrites | **HIGH** | mv |
| BUG-4 | Case-only rename fails on macOS | Low | mv |
| UX-1 | No --sort backlinks on find | Enhancement | find |
| UX-2 | No hints for orphan backlinks result | Low | backlinks |
| UX-3 | Leading-slash --file gives unhelpful error | Low | both |
| ISSUE-6 | Duplicate warnings on backlinks scan | Low | find |
