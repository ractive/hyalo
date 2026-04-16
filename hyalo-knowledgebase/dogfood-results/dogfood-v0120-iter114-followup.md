---
title: Dogfood v0.12.0 — Post Iteration 114 Follow-up
type: research
date: 2026-04-15
status: completed
tags:
  - dogfooding
  - verification
  - index
  - frontmatter
  - links
  - schema
related:
  - "[[dogfood-results/dogfood-v0120-followup-iter113b]]"
  - "[[dogfood-results/dogfood-v0120-post-iter113]]"
  - "[[dogfood-results/dogfood-v0120-multi-kb]]"
  - "[[iterations/iteration-114-dogfood-v0120-followup-fixes]]"
---

# Dogfood v0.12.0 — Post Iteration 114 Follow-up

Additional dogfood pass after iter-114 merged, focused on areas not exercised in earlier reports:
snapshot-index semantics, schema validation coverage, frontmatter-vs-body link extraction, and
property value parsing.

Binary: `hyalo 0.12.0` — `target/release/hyalo` built from `main` (post 191e23d).

KBs exercised:
- **Own KB** (239 files)
- **MDN Web Docs** (14,245 files) via `--dir ../mdn/files/en-us`
- **GitHub Docs** (3,520 files) via `--dir ../docs/content`

## Iter-113b Regression Re-verification

Re-ran the iter-113b scenario. Root `.hyalo.toml` has `dir = "hyalo-knowledgebase"`.

- `types set testtype --required title --property-type "priority=number"` → writes to the *root*
  `.hyalo.toml`, not `hyalo-knowledgebase/.hyalo.toml`. Correct.
- `views set test-view --property "status=planned"` → round-trips cleanly; `views list` sees it; `views remove` cleans up with an empty diff.
- External KB with its own `.hyalo.toml` (`--dir /tmp/.../ext1`) → `types set` and `views set` write to that KB's config, not the enclosing project's. Correct.
- External KB with no `.hyalo.toml` yet → `types set` creates one at the `--dir` root. Correct.

iter-113b's `config_dir` vs `dir` distinction is holding up.

## New Bugs

### BUG-A: `--index` value parsing is a UX trap (MEDIUM)

`--index[=<PATH>]` is optional-value. Observed behaviour:

| Form | Behaviour |
|------|-----------|
| `--index` (no value) | uses default `<vault>/.hyalo-index` — works |
| `--index=PATH` | uses PATH — works if absolute |
| `--index=./foo` (relative) | **joined to vault dir** — fails with misleading path |
| `--index PATH` (space-separated) | `--index` is valueless (default); PATH becomes the FTS body query |

Two traps:

1. **Space-separated is silently wrong**: `hyalo find --index hyalo-knowledgebase/.hyalo-index --count` returns `21` because `hyalo-knowledgebase/.hyalo-index` is consumed as the positional FTS query (matching the literal string in 21 files). No warning that the index arg looks like a path.
2. **Relative path with `=` is joined to vault**: `--index=hyalo-knowledgebase/.hyalo-index` from repo root emits `warning: failed to read index file: hyalo-knowledgebase/hyalo-knowledgebase/.hyalo-index; falling back to disk scan`. Relative paths should resolve from CWD (POSIX convention), or the help text should call out the vault-relative semantics.

**Suggestion**: warn when a positional query string ends in `.hyalo-index` or is an existing file path, and/or document that `--index=PATH` is vault-relative.

### BUG-B: Wikilinks in frontmatter list properties are not extracted as links (MEDIUM)

Files that reference other files only through `related:` / `depends-on:` / `supersedes:` list
properties do not show up as `links` or contribute to `backlinks`.

Example: `iterations/iteration-113-dogfood-v0120-fixes.md` is referenced by two files through
frontmatter `related:`:
- `dogfood-results/dogfood-v0120-followup-iter113b.md`
- `dogfood-results/dogfood-v0120-post-iter113.md`

But `hyalo backlinks iterations/iteration-113-dogfood-v0120-fixes.md` returns "No backlinks found".

Inspecting `followup-iter113b.md`:
```
properties.related: ["[[…/post-iter113]]", "[[…/multi-kb]]", "[[iterations/iteration-113-…]]"]
links count: 2  (only the two that also appear in the body)
```

Only wikilinks appearing in body prose are captured. Frontmatter wikilinks are preserved as
strings but ignored for the link graph. This breaks the `related`-first workflow that
`iterations/`, `research/`, and `backlog/` conventions rely on.

**Impact**: `backlinks`, `find --orphan`, `find --dead-end`, and `mv` link-rewriting all miss
these references.

### BUG-C: `set` / `append --property "key=[[wikilink]]"` parses brackets as YAML flow lists (MEDIUM)

```
$ hyalo set file.md --property 'related=[[foo/bar]]'
# stored as:
related: [[foo/bar]]   # YAML flow: list containing list containing "foo/bar"
```

After a second `append --property 'related=[[baz/qux]]'`:
```
related: [["[foo/bar]"], ["[baz/qux]"]]
```

The value `[[foo/bar]]` is valid YAML flow syntax for a nested list, so the parser takes it
literally. The user's intent (a wikilink string) is almost always what they want.

**Workaround** (requires arcane knowledge): `--property 'related=["[[foo/bar]]"]'`.

**Suggestion**: if the value starts with `[[` and ends with `]]` and contains no commas, treat
it as a single wikilink string. Alternatively, add a `--wikilink-property` variant or a
documented escape.

### BUG-D: `hyalo set` does not validate values against the schema (MEDIUM)

```
# iteration schema: status is enum, branch has a pattern
hyalo set iterations/iteration-NN.md --property "status=not-a-valid-status"
#    → status=not-a-valid-status: 1/1 modified
hyalo set iterations/iteration-NN.md --property "branch=bad-branch"
#    → branch=bad-branch: 1/1 modified

hyalo lint iterations/iteration-NN.md
#    error  property "status" value "not-a-valid-status" not in [planned, in-progress, …]
#    error  property "branch" value "bad-branch" does not match pattern "^iter-\\d+[a-z]*/"
```

Write path skips schema checks entirely; only `lint` (or a subsequent run) catches the
divergence. A `--strict` / `--validate` flag or a config-level default would tighten this
without breaking existing workflows.

### BUG-E: `set --where-property` still requires `--file`/`--glob` (LOW)

```
hyalo set --where-property "status=planned" --tag test
#    Error: set requires --file or --glob
```

`--where-property` is itself a filter over the vault, but `set` refuses to infer that as the
target set. Either accept `--where-property`/`--where-tag` as a standalone target selector
(implicit `--glob "**/*.md"`), or improve the error to mention that the user can add
`--glob "**/*.md"` explicitly.

## Existing Bugs — Status

- **BUG-6 (absolute URL-style link rewriting in `mv`)**: MDN dry-run `mv web/javascript/.../promise/any/index.md` → the only links are absolute URL-style (`/en-US/docs/...`) which `find --fields links` shows as unresolved (`path: null`). `mv --dry-run` now reports `total_files_updated: 0` and no rewrite of those links — this appears resolved.
- **UX-5 (link resolution case-sensitivity)**: still open; not covered by iter-113 or iter-114.
- **UX-6 (repeated unclosed-frontmatter warning on GH Docs)**: still open. `docs/content/code-security/concepts/index.md` is genuinely malformed (file ends mid-list with no closing `---`), but every `hyalo` invocation prints the warning. Would benefit from a `.hyalo.toml` skip list.

## What Continues to Work Well

### Snapshot index — great speedups when used correctly

Once you know to use `--index` (no value) or `--index=<absolute-path>`:

| Operation | No index | With index | Speedup |
|-----------|----------|-----------|---------|
| MDN `summary` | 0.81s | 0.44s | 1.8× |
| MDN `find "getUserMedia"` | 3.47s | 0.33s | 10.5× |
| MDN `lint --limit 0` | (not tested) | 0.66s | — |
| Own KB `summary` | 0.02s | 0.02s | (already fast) |
| Own KB `find "snapshot index"` | 0.06s | 0.01s | 6× |

### Mutation with `--index` updates the index in-place

`set`, `remove`, `append`, `mv`, and `task toggle` all keep `.hyalo-index` current when
`--index` is active; subsequent `find --index` sees the new state without rebuild.

### Structured search + FTS composes cleanly

```
hyalo find "bm25" --property status=completed --sort score --reverse --limit 3 --index
```
works end-to-end and ranks sensibly.

### iter-113 fixes verified still holding

- TOML section-preserving writes (BUG-2): no churn on `types set`/`views set`.
- Bare-boolean warnings: `find "AND"` and `find "OR"` emit the quoting hint; `find "and"` /
  `find "or"` (lowercase) are regular terms (45 body matches each).
- `task toggle --all` handles deep indentation.
- `remove --tag "cli,ux"` removes malformed comma-tags.
- `lint --type iteration` works; `lint --fix --dry-run` proposes `split-comma-tags`.

## UX Observations (not blockers)

- **`find --title "dogfood"` is case-insensitive substring** — good. `--property 'title~=...'` is
  regex. The `--help` COMMON MISTAKES section already covers this; no bug.
- **`find --language rust`** returns `unknown stemming language: "rust"`. The flag is the
  Snowball stemmer (english/german/…), not a code-block language filter. The flag name
  invites confusion with Markdown fenced-block languages. Consider renaming to `--stemmer`
  or adding a code-block filter under a different flag.
- **`lint --count`** is not supported (errors: "--count is only supported for list commands").
  Lint can have thousands of issues; a count shortcut is natural. Current workaround:
  `hyalo lint --jq '.results.files | length'` or text-mode grep.
- **`task toggle --all --dry-run`** output shows target states as `[ ]` (toggled result),
  which could be read as "these are still unchecked". Format suggestion:
  `line 39: [x] → [ ]` would be clearer.
- **`find --view` cross-KB**: `hyalo --dir ../docs/content find --view open-tasks` errors
  because the GH Docs KB has no views defined. Expected, but the error (`unknown view`) could
  mention that no views exist in *that* KB.
- **`--orphan`/`--dead-end` mutual-exclusion warning** is helpful and correctly suppresses
  results.
- **Properties summary hint** (`properties versions` → "did you mean `hyalo --version`?") is
  cute but off-target when the user typed `properties <name>`. Mentioning the correct
  subcommand structure (`properties summary`, `properties rename`) would be more useful.

## Suggested Priorities

| # | Item | Severity | Effort | Status |
|---|------|----------|--------|--------|
| 1 | Extract frontmatter wikilinks into link graph (BUG-B) | MEDIUM | Medium | ✅ iter-115 |
| 2 | Schema validation on write (BUG-D, `set`/`append` behind flag or config) | MEDIUM | Small-Medium | ✅ iter-115 |
| 3 | Fix `--index=PATH` relative-path semantics + warn on space-separated trap (BUG-A) | MEDIUM | Small | ✅ iter-115 |
| 4 | Handle `[[wikilink]]` in `--property` value without nested-list parse (BUG-C) | MEDIUM | Small | ✅ iter-115 |
| 5 | `set --where-property` auto-target without `--glob` (BUG-E) | LOW | Small | ✅ iter-115 |
| 6 | Rename `find --language` → `--stemmer` (or alias) | LOW | Small | ✅ iter-115 |
| 7 | `lint --count` + better `task toggle --dry-run` format | LOW | Small | ✅ iter-115 |
| 8 | `.hyalo.toml` ignore-list for known-malformed files (UX-6) | LOW | Small | ✅ iter-115 |

## Iter-115 Verification (2026-04-15)

All issues resolved in [[iterations/iteration-115-dogfood-v0120-iter114-followup]]. Verified:

- **BUG-A**: `hyalo --index=./rel.hyalo-index find` resolves `./rel.hyalo-index` against CWD
  (not vault). Bare `--index` with a file-path positional emits a warning suggesting `--index=PATH`.
- **BUG-B**: `hyalo backlinks b.md` now surfaces `a.md` when `a.md` references `b` via
  `related: [[b]]` frontmatter. Configurable via `[links] frontmatter_properties = [...]` in
  `.hyalo.toml` (defaults to `related`, `depends-on`, `supersedes`, `superseded-by`).
- **BUG-C**: `hyalo set file.md --property 'related=[[foo/bar]]'` stores the value as a literal
  string `"[[foo/bar]]"`; `append` on the same property produces a flat list of strings.
- **BUG-D**: `hyalo set --validate ...` (or `--strict`) rejects enum/pattern violations. Global
  opt-in via `[schema] validate_on_write = true`.
- **BUG-E**: `hyalo set --where-property status=planned --tag x` (no `--file`/`--glob`) defaults
  to all `**/*.md` under the vault.
- **UX-1**: `hyalo find --stemmer english` works as an alias for `--language`.
- **UX-2**: `hyalo lint --count` prints the number of files with issues as a bare integer.
- **UX-3**: `hyalo task toggle --dry-run` prints `line N: [old] -> [new]` showing direction of change.
- **UX-4**: `hyalo properties <typo>` hints at `properties summary` / `properties rename`.
- **UX-5**: `[lint] ignore = ["path/glob"]` in `.hyalo.toml` skips matched files from lint output
  and suppresses their parse-error warnings in read-only commands.

**Superseded by [[iterations/iteration-118-split-index-flag]]:** `--index=PATH` is now `--index-file=PATH`; bare `--index` is a boolean flag.
