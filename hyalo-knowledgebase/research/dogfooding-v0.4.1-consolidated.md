---
title: "Dogfooding v0.4.1 — Consolidated Report"
type: research
date: 2026-03-26
status: completed
tags:
  - dogfooding
  - v0.4.1
  - external-docs
---

# Dogfooding hyalo v0.4.1 on External Docs

**Tested**: 2026-03-26
**Repos**: GitHub Docs (`../docs/content`, 3520 files), VS Code Docs (`../vscode-docs/docs`, 339 files)
**Detailed reports**: [[dogfooding-v0.4.1-edge-cases]], [[perf-benchmarks-v041]], [[dogfooding-v0.4.1-backlinks-mv]], [[dogfooding-v0.4.1-creative-testing]], [[dogfooding-v0.4.1-dir-styles]]

---

## Performance: 12-13% Faster Than v0.4.0

| Command (GitHub Docs, 3520 files) | v0.4.1 | v0.4.0 | Change |
|---|---|---|---|
| `summary` | 0.217s | 0.25s | -13% |
| `find --fields properties` | 0.175s | 0.20s | -12% |
| `find --fields links,backlinks` (index) | 0.494s | 0.56s | -12% |
| `find` (all fields) | 0.512s | 0.58s | -12% |
| `backlinks` (42 inbound) | 0.156s | 0.18s | -13% |

Rayon parallelization (iter-44) is paying off. Sublinear scaling: 10x files = ~4.5x time.

VS Code Docs (339 files): summary 0.046s, find 0.082s, backlinks 0.041s.

---

## Bugs Found

### CRITICAL

**BUG: `mv` leaks `--dir` value into absolute-path link rewrites**
When links use root-absolute paths (`/graphql/reference/objects`) and `--dir` is relative (`../docs/content`), the rewriter produces `/../docs/content/graphql/reference/all-objects`. With absolute `--dir`: `//Users/james/devel/docs/content/...`. Only `--dir docs` from the parent works. **This corrupts files.** Root cause: site_prefix derivation from raw `--dir` string.

### HIGH

**BUG: site_prefix only works with Style 4 (`--dir <subfolder>` from parent)**
This is ISSUE-1 from v0.4.0, still present and now confirmed as root cause of the mv bug above. Testing VS Code Docs (links use `/docs/...` prefix):
- `--dir ../vscode-docs/docs` → **0/2772** links resolved
- `cd` into dir, no `--dir` → **0/2772** resolved
- `--dir /absolute/path/docs` → **0/2772** resolved
- `cd ../vscode-docs && --dir docs` → **2514/2772** resolved (91%)

**Fix needed**: Use `dir.file_name()` (last path component) for site_prefix, AND add explicit `--site-prefix` CLI flag + `.hyalo.toml` key.

### MEDIUM

**BUG: `=~` vs `~=` silent mismatch (footgun)**
`--property 'title=~/Copilot/'` (Perl-style `=~`) silently returns empty results. It parses `=` as equality with literal value `~/Copilot/`. Correct syntax is `title~=/Copilot/`. Should either error on `=~` or accept it as alias.

**BUG: `read --frontmatter` suppresses broken-frontmatter error**
`read --file broken.md` correctly errors on unclosed frontmatter, but `read --frontmatter --file broken.md` succeeds and fabricates a closing `---`. The flag should not suppress validation.

### LOW

**BUG: Self-links in backlinks count** — A file linking to itself appears in its own backlinks.

**BUG: Case-only rename fails on macOS** — `mv --file README.md --to Readme.md` errors "target already exists" on case-insensitive FS. Should do two-step rename.

**BUG: `--dir` pointing to a file silently succeeds** — Treats single file as 1-file vault. Should error.

**BUG: Duplicate warning on `--fields links,backlinks`** — "skipping unclosed frontmatter" warning appears twice (one per scan pass).

---

## Known v0.4.0 Issues Still Open

| Issue | Status | Notes |
|---|---|---|
| ISSUE-1: site_prefix fragile | **Still open** | Root cause of mv bug; needs `--site-prefix` flag |
| ISSUE-3: `--limit` no short-circuit | **Still open** | `--limit 10` same speed as full scan |
| ISSUE-5: `--dir file` no error | **Still open** | |
| ISSUE-6: Duplicate warnings | **Still open** | |
| ISSUE-7: `--glob` not repeatable | **Still open** | Brace workaround `{a,b}/**` works |

---

## UX Issues

- **Non-matching glob returns error (exit 1)** instead of empty result (exit 0). Bad for scripting.
- **No reverse sort** — need `--jq 'reverse'` workaround.
- **No `--sort-by count`** on `tags summary` / `properties summary`.
- **Empty body pattern `""` matches all files** — should error or be treated as no filter.
- **`--file /leading-slash`** gives "file not found" with no hint that the slash is the problem.
- **Backlinks output format inconsistency** — `backlinks` command has `target`/`total` fields; `find --fields backlinks` omits them.

---

## What Works Really Well

- **`--jq` integration** — incredibly powerful for analytics and dashboards
- **Pipe-friendliness** — warnings to stderr, data to stdout, composable with unix tools
- **`read --section`** — killer feature for extracting specific content
- **`--hints`** — genuinely useful for exploration, suggests next commands
- **Filter composition** — AND-ing property + tag + content + glob + section is natural
- **Error messages** — clear, actionable, include offending input
- **Performance** — 3520 files in 0.2-0.5s is excellent for a YAML-aware tool
- **`--format text`** — compact output great for LLM consumption

---

## Optimization Opportunities

1. **Early termination for `--limit`** without sort — stop after N matches
2. **Incremental index caching** — cache link index across CLI invocations
3. **rg gap is 5-7x** — expected due to YAML parsing, but memory-mapped I/O could help

---

## Recommended Priority for Next Iteration

1. **site_prefix** — add `--site-prefix` CLI flag + `.hyalo.toml` key + fix `dir.file_name()` derivation (fixes mv bug too)
2. **`=~` footgun** — error on `=~` syntax or accept as alias
3. **`--limit` short-circuit** — meaningful optimization for large vaults
4. **Self-links in backlinks** — filter out self-references
5. **`read --frontmatter` validation** — don't suppress broken frontmatter errors
