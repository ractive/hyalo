---
date: 2026-03-26
status: completed
tags:
- dogfooding
- v0.4.1
- external-docs
title: Dogfooding v0.4.1 — Edge Cases & Known Issue Recheck
type: research
---

## Known v0.4.0 Issue Status

### ISSUE-1: site_prefix derivation fragile — STILL PRESENT

Running `hyalo --dir ../vscode-docs/docs find --fields links` from **outside** the vscode-docs repo resolves **0 of 2772** links. Running the same command from **inside** (`cd ../vscode-docs && hyalo --dir docs ...`) resolves **2514 of 2772** (91%). The 258 still-unresolved from inside are links to paths that don't exist on disk (redirects, deleted pages, etc.).

The root cause: site_prefix derivation depends on CWD. When CWD is outside the docs tree, `/docs/...` absolute links cannot be mapped to the `--dir` target. There is no `site_prefix` CLI option or `.hyalo.toml` key to override this.

**Severity**: Medium — workaround is to `cd` into the repo first.

### ISSUE-3: --limit doesn't short-circuit — STILL PRESENT

- `find` (all 3520 files): **0.38s**
- `find --limit 10`: **0.34s**

No meaningful speedup. The entire vault is scanned regardless of `--limit`. On this corpus the absolute time is fast enough not to matter, but on very large vaults the lack of short-circuiting could be noticeable.

**Severity**: Low — only matters at scale.

### ISSUE-5: --dir pointing to a file doesn't error — STILL PRESENT

`hyalo --dir ../docs/content/index.md summary` silently treats the file as a vault containing just that one file. It succeeds and returns a valid summary for a 1-file "vault". No error or warning is emitted.

**Severity**: Low — arguably acceptable behavior, but surprising.

### ISSUE-6: Duplicate warning on --fields links,backlinks — STILL PRESENT

Running `find --fields links,backlinks` on a vault with broken frontmatter produces the skipping warning **twice** (once for the links pass, once for the backlinks pass):

```
warning: skipping code-security/concepts/index.md: unclosed frontmatter (no closing `---` found)
warning: skipping code-security/concepts/index.md: unclosed frontmatter (no closing `---` found)
```

**Severity**: Low — cosmetic.

### ISSUE-7: --glob cannot be repeated — STILL PRESENT

`find --glob "actions/**/*.md" --glob "copilot/**/*.md"` fails with:
```
error: the argument '--glob <GLOB>' cannot be used multiple times
```

The brace workaround `--glob "{actions,copilot}/**/*.md"` works correctly.

**Severity**: Medium — repeated `--glob` is a natural expectation from CLI users. The help text does not mention this limitation.

## Additional Edge Case Results

### Broken frontmatter (Test 6)

Files with unclosed YAML frontmatter (e.g., `code-security/concepts/index.md`) are **gracefully skipped** with a warning on stderr. The rest of the vault processes normally. Good behavior.

### Empty files (Test 8)

Empty `.md` files are handled correctly — they appear in results with empty properties, tags, sections, tasks, and links. `summary` counts them. No crash or error.

### Large files (Test 9)

The largest file tested was `contributing/style-guide-and-content-model/style-guide.md` at 107 KB. Hyalo processed it correctly, extracting 61 links. No performance issues.

### Property type conflicts (Test 10)

In VS Code Docs, `Order` appears as both `number` (2 files with numeric values) and `text` (3 files where `Order:` has **no value** — bare YAML key). Hyalo correctly reports these as separate type entries in `properties summary`. The null-value keys being typed as "text" rather than "null" is debatable but reasonable.

### Negation filters (Test 11)

`--property '!title'` works correctly — returns files without a `title` property (READMEs, etc.). There is no `--content` flag; body text search uses positional `PATTERN` or `--regexp/-e`.

### Regex filters (Test 12)

- `--property 'title~=Copilot'` — works correctly (simple pattern match)
- `--property 'title~=/Copilot/'` — works correctly (regex with slashes)
- `--property 'title~=/copilot/i'` — works correctly (case-insensitive regex)

**NEW BUG found**: `--property 'title=~/Copilot/'` (note: `=~` not `~=`) silently returns empty results instead of erroring. This is because `=` is parsed as an equality check with literal value `~/Copilot/`, which matches nothing. The `~=` operator is documented but the common mistake of writing `=~` (Perl/Ruby style) is a footgun. Should either: (a) error on `=~` as unrecognized operator, or (b) treat `=~` as alias for `~=`.

## Summary of New Findings

| ID | Issue | Status |
|----|-------|--------|
| ISSUE-1 | site_prefix fragile | Still present |
| ISSUE-3 | --limit no short-circuit | Still present |
| ISSUE-5 | --dir file no error | Still present |
| ISSUE-6 | Duplicate warnings | Still present |
| ISSUE-7 | --glob not repeatable | Still present |
| NEW-1 | `=~` vs `~=` silent mismatch | New bug |
| NEW-2 | YAML bare keys typed as "text" not "null" | Minor, debatable |
