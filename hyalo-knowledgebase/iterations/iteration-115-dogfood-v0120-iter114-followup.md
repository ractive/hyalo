---
title: Iteration 115 — Dogfood v0.12.0 iter-114 Follow-up Fixes
type: iteration
date: 2026-04-15
status: completed
branch: iter-115/dogfood-v0120-iter114-followup
tags:
  - dogfooding
  - bug-fix
  - index
  - frontmatter
  - links
  - schema
related:
  - "[[dogfood-results/dogfood-v0120-iter114-followup]]"
  - "[[iterations/iteration-114-dogfood-v0120-followup-fixes]]"
---

# Iteration 115 — Dogfood v0.12.0 iter-114 Follow-up Fixes

## Goal

Address the bugs and UX issues surfaced in
[[dogfood-results/dogfood-v0120-iter114-followup]]. Focus on correctness of the link graph
(frontmatter wikilinks), write-path schema validation, and the `--index` / `--property`
value-parsing ergonomics that trip up users.

## Bugs

### BUG-A: `--index` value parsing trap (MEDIUM)

Two sub-issues:

1. `hyalo find --index PATH query` silently swallows PATH as the positional FTS query because
   `--index` is an optional-value flag. Result looks plausible (a small number of matches)
   and the user doesn't realize the index wasn't used.
2. `--index=PATH` with a *relative* path is joined to the vault dir, not CWD. This is
   counter-intuitive and the error message ("failed to read index file: <vault>/<relative>")
   confuses more than it clarifies.

Fixes:
- Warn when the positional query string is a pre-existing file path or ends in `.hyalo-index`
  and `--index` (valueless) was also passed — likely user mistake.
- Resolve `--index=PATH` relative to CWD (matching POSIX convention), and update help text.
- Alternatively: make `--index` take a required value (breaking) or add `-i` short form that
  requires a value.

### BUG-B: Frontmatter wikilinks missing from link graph (MEDIUM)

Wikilinks in list-valued frontmatter properties (`related:`, `depends-on:`, `supersedes:`,
`superseded-by:`) are preserved as strings but never extracted into the `links` field, so
`backlinks` and `--orphan`/`--dead-end` miss them. This breaks the `related`-first workflow
across `iterations/`, `research/`, and `backlog/`.

Reproduced: `hyalo backlinks iterations/iteration-113-dogfood-v0120-fixes.md` returns
"No backlinks found" despite two reports referencing it via frontmatter `related:`.

Fix: when extracting links, also scan string values in list-valued frontmatter properties for
`[[wikilink]]` patterns and add them to the `links` array. Consider restricting to a
configurable set of property names (default: `related`, `depends-on`, `supersedes`,
`superseded-by`) to avoid false positives in code snippets stored as properties.

### BUG-C: `[[wikilink]]` in `--property` value parsed as nested YAML list (MEDIUM)

```
hyalo set file.md --property 'related=[[foo/bar]]'
# stored as YAML flow list-of-list: [[foo/bar]]
hyalo append file.md --property 'related=[[baz/qux]]'
# result: [["[foo/bar]"], ["[baz/qux]"]]
```

Fix: in the `--property` value parser, when the value starts with `[[` and ends with `]]`,
contains no top-level commas, and the inner text doesn't look like YAML flow (no `:` or
nested brackets), treat it as a literal wikilink string rather than flow YAML.

Alternative: add explicit escape guidance to the `set`/`append` error/help, or accept
`'@wikilink:foo/bar'` as a shorthand.

### BUG-D: `set` / `append` skip schema validation (MEDIUM)

Write commands accept any value, including enum violations and regex-pattern misses. Only
`lint` catches these, often long after the file has been committed.

Fix options (not mutually exclusive):
- Add a `--validate` / `--strict` flag to `set` and `append` that runs the lint schema rules
  against the new value and rejects violations.
- Add a `validate-on-write` option to `[schema]` in `.hyalo.toml` (default: off) to enable
  validation globally.
- Keep the current permissive default to avoid breaking scripts, but print a `warn` line
  when the mutation produces a schema violation the user could silently miss.

### BUG-E: `set --where-property` requires explicit `--file` / `--glob` (LOW)

```
hyalo set --where-property "status=planned" --tag test
# Error: set requires --file or --glob
```

Fix: when `--where-property` or `--where-tag` is provided without `--file`/`--glob`, default
the target set to `**/*.md` (all vault files), then apply the where-filters. Update help to
reflect the new semantics.

## UX Improvements

### UX-1: Rename `find --language` to `--stemmer` (or alias)

`--language` currently selects the Snowball stemmer (english, german, …), not the
Markdown-fenced-block language. Users type `--language rust` expecting code-block filtering
and get "unknown stemming language". Add `--stemmer` as an alias; keep `--language` for
backward compat with a deprecation note in help.

### UX-2: `lint --count`

`--count` is rejected by `lint` even though lint is essentially a list command. A
`files-with-issues` count is natural at scale (e.g. MDN lint emits 14k file-with-issues
rows). Support `--count` on lint and have it return the number of files with issues
(mirroring existing `--count` semantics on `find`).

### UX-3: Clearer `task toggle --dry-run` output

Current output shows the post-toggle state as `[ ]` or `[x]`, which reads as the "current"
state at a glance. Switch to `line N: [x] → [ ]` format so the direction of change is
explicit.

### UX-4: `properties <name>` hint misleads toward `--version`

`hyalo properties versions` prints "did you mean `hyalo --version`?" — but the user clearly
typed `properties`. Better hint: "properties has subcommands; try `hyalo properties summary`
or `hyalo properties rename`".

### UX-5: `.hyalo.toml` ignore list for known-malformed files (carried over from UX-6 prior)

GH Docs has one genuinely-malformed frontmatter file that trips every `hyalo` call with a
warning. An `[lint] ignore = ["path/to/file.md"]` (or `[ignore] files = [...]`) entry would
silence the known case without hiding genuine new breakage.

## Tasks

- [x] BUG-A: warn when positional query matches an existing file or ends in `.hyalo-index` and `--index` (valueless) is also set
- [x] BUG-A: resolve `--index=PATH` relative paths against CWD instead of vault dir; update `--help` wording
- [x] BUG-A: add e2e test covering `--index PATH` (space-separated) warning path
- [x] BUG-B: extract `[[wikilink]]` patterns from string values in list-valued frontmatter properties (configurable property list)
- [x] BUG-B: include frontmatter-derived links in `backlinks`, `--orphan`, `--dead-end`, and snapshot-index link graph
- [x] BUG-B: add e2e test asserting `backlinks` surfaces references via `related:` frontmatter
- [x] BUG-C: update `--property` value parser to treat `[[text]]` (no commas, no nested YAML) as literal wikilink string
- [x] BUG-C: add e2e test for `set` / `append` with a `[[wikilink]]` value preserving it as a string, not nested list
- [x] BUG-D: introduce `--validate` flag on `set` and `append` (off by default) that runs schema checks against new values
- [x] BUG-D: add optional `validate-on-write` to `[schema]` config; if true, `--validate` is implicit
- [x] BUG-D: add e2e tests: `--validate` rejects enum + pattern violations; lint still catches them when `--validate` is off
- [x] BUG-E: allow `set --where-property` / `--where-tag` without `--file`/`--glob` (defaults to all vault files)
- [x] BUG-E: add e2e test for `set --where-property ... --tag ...` standalone form
- [x] UX-1: add `--stemmer` alias for `--language` on `find` and keep both flags documented
- [x] UX-2: support `--count` on `lint`, returning files-with-issues count
- [x] UX-3: reformat `task toggle --dry-run` output as `line N: [x] → [ ]`
- [x] UX-4: improve hint for `properties <subcommand-typo>` to point at `summary` / `rename`
- [x] UX-5: add `[lint] ignore = [...]` (or equivalent) config key to skip listed files, surfaced in CLAUDE.md / README
- [x] Run full dogfood pass after fixes; update [[dogfood-results/dogfood-v0120-iter114-followup]] with verification status

## Acceptance Criteria

- [x] `backlinks <file>` finds references made via `related:` / `depends-on:` frontmatter on sibling iteration/research files
- [x] `hyalo set x.md --property 'related=[[foo/bar]]'` stores `related: "[[foo/bar]]"` (string), not a nested list
- [x] `hyalo set --validate` rejects values that `hyalo lint` would flag as errors (enum + pattern)
- [x] `--index=./path` (relative) resolves from CWD; absolute paths and valueless `--index` unchanged
- [x] `hyalo lint --count` returns an integer
- [x] All tests pass: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q`
- [x] README / CLAUDE.md / skill templates updated with any new flags
