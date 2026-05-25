---
title: Dogfood v0.16.0 — deep run after iter-133..140
type: research
date: 2026-05-24
status: active
tags:
  - dogfooding
  - lint
  - schema
  - files-from
  - new-command
  - help-text
  - performance
  - llm-ergonomics
related:
  - "[[dogfood-results/dogfood-v0150-iter132-followup]]"
  - "[[iteration-138-schema-extensions-and-new-command]]"
  - "[[iteration-139-files-from-flag]]"
  - "[[iteration-140-dogfood-138-139-fixes]]"
---

# Dogfood v0.16.0 — deep run after iter-133..140

Binary: `hyalo 0.16.0`. Tested against:

- **Own KB** — `hyalo-knowledgebase/` (279 files)
- **MDN Web Docs** — `../mdn/files/en-us/` (14,375 files)
- **Fresh scratch vaults** in `/tmp/` for schema and `--files-from` edge cases

Ten iterations have merged since the last dogfood report
([[dogfood-v0150-iter132-followup]] at v0.15.0): iter-131..140. The
biggest new surface area is the iter-138 schema extensions
(`item_pattern`, `required_sections`, `hyalo new`), iter-139
(`--files-from`), iter-140 (their bugfix iteration), and iter-137
(cross-platform link resolution).

## Bug regression testing (prior dogfood)

All three open bugs from [[dogfood-v0150-iter132-followup]] are now
fixed.

### prior BUG-1 — `[[./relative]]` wikilinks survive `hyalo mv` — FIXED

```sh
$ hyalo mv b.md --to sub/b.md --apply
# a.md before: "See [[./b]] and [[b]]"
# a.md after:  "See [[b]] and [[b]]"
```

The relative form is now normalized to short form on mv. Backlinks
resolve correctly.

### prior BUG-2 — HYALO003 date-format is shape-only — FIXED

```sh
$ hyalo set bad.md --property modified=2026-13-50
{
  "error": "value \"2026-13-50\" is not a valid ISO 8601 date (YYYY-MM-DD) for property 'modified'",
  "hint": "check month (01–12) and day ranges for the given month"
}
exit=1
```

Same for `2026-02-30`, etc. Clear error message with a useful hint.

### prior BUG-3 — `hyalo lint` always exits 0 — FIXED

```sh
$ hyalo lint   # errors present
exit=1
```

## New feature verification (iter-138/139/140)

iter-140 already shipped fixes for the seven bugs caught the first time
these features were dogfooded. Re-verified all seven in this run; all
pass against a fresh vault. See [[iteration-140-dogfood-138-139-fixes]]
for the original bug list. Highlights:

- **`required_sections`** lint enforcement fires correctly. Missing
  `## Details` surfaces as a `SCHEMA` error with the exact message
  `missing required section: expected "## Details" at or after position 2 in the outline`.
- **Git pipeline** (`git diff --name-only | hyalo lint --files-from -`)
  works against this repo's own subdir vault (`hyalo-knowledgebase/`).
  Counters live under `.results`; `find` returns a `{files, files_missing, …}`
  object.
- **`hyalo new`** auto-creates parent dirs, scaffolded file lints clean.
- **`required_sections` snake_case** key accepted.

## iter-137 — cross-platform link resolution

```sh
$ cat win.md
See [[sub\b]] (backslash-style).

$ hyalo find --file win.md --fields links
# resolves to sub/b.md
```

Windows-style backslash wikilinks resolve to the same target as the
forward-slash form. Good.

## New bugs found

### NEW-1: `item_pattern` reports only the first violation per list — MEDIUM

A `string-list` property with multiple invalid items reports only item 0.
The remaining bad items are silently ignored, so the user has no idea
they're there until they fix item 0 and re-run.

```sh
# tags: [Foo, "1bad", "Bar"]  — 3 items all violate ^[a-z][a-z0-9-]*$
$ hyalo lint --file multi.md --format json | jq '.results.files[].rule_groups[]'
{
  "rule": "SCHEMA",
  "count": 1,         # ← should be 3
  "shown": 1,
  "violations": [
    { "message": "property \"tags\" item 0: value \"Foo\" does not match pattern ..." }
    # items 1 and 2 are missing
  ]
}
```

Root cause: `validate_property` returns `Option<Violation>` (singular)
in `crates/hyalo-cli/src/commands/lint.rs:~1090–1147`. The
`for (i, item) …` loop returns `Some(Violation)` on first match.

**Impact**: medium. Defeats the value of `item_pattern` as a CI check —
fixing one mistake reveals the next on the next run, instead of seeing
all mistakes at once. Compounds when the bad list is long (e.g. a
`first_call_sites: [...]` property in a consumer project).

**Expected**: collect all violations into a `Vec<Violation>` and return
them all.

### NEW-2: `--files-from` auto-strip only handles single-segment `--dir` — MEDIUM

iter-140 BUG-2 fixed the canonical `git diff --name-only | hyalo lint
--files-from -` pipeline for the common case where `--dir` is a single
path component (e.g. `--dir hyalo-knowledgebase`). It does **not** handle
multi-segment vault dirs.

Repro on MDN:

```sh
# vault is at files/en-us/ — git lives at the repo root above
$ cd ../mdn
$ echo "files/en-us/games/techniques/2d_collision_detection/index.md" \
    | hyalo --dir files/en-us find --files-from -
note: all --files-from entries were missing; if paths include the vault
dir prefix (e.g. kb/notes/foo.md with --dir kb), hyalo strips it
automatically — check that the vault dir name matches
{"results": {"files": [], "files_missing": 1, ...}}
```

The hint message even uses single-segment example phrasing (`kb`), which
won't help an MDN-shaped user diagnose.

**Impact**: medium. MDN (`files/en-us/`), GitHub Docs (`content/`),
VS Code Docs (e.g. `docs/`) and many real-world doc repos use nested
vault dirs. The headline `--files-from` recipe doesn't work for them.

**Expected**: strip the **full** configured `dir` prefix from each input
line, not just the basename. Update the stderr hint to quote the actual
configured `dir` for clearer diagnosis.

### NEW-3: `hyalo new --help` text is stale after iter-140 BUG-4 — LOW

The help text in `crates/hyalo-cli/src/cli/args.rs:1021` still says:

```
CONSTRAINTS:
- Refuses with an error if the target file already exists
- Refuses with an error if the parent directory does not exist  ← stale
```

and `--file` (line 1034):

```
Vault-relative path for the new file (must not exist; parent must exist)
                                                       ^^^^^^^^^^^^^^^
```

iter-140 BUG-4 fixed the code to `create_dir_all(parent)` — but the
docs in two places weren't updated alongside the code.

**Impact**: low (cosmetic, but misleads LLMs and humans). Per
`[[feedback_keep_docs_in_sync]]`, help texts must move with code.

**Expected**: drop the "parent must exist" wording; mention that parent
dirs are created automatically.

### NEW-4: `--files-from` doesn't trim leading whitespace — LOW

```sh
$ printf '  edge.md\n' | hyalo find --files-from -
note: all --files-from entries were missing; …
{"files_missing": 1}
```

The iter-139 plan called out CRLF and BOM handling (both work
correctly — verified), and stripping leading `./` (works), but didn't
specify whitespace trimming. Whitespace-padded paths are common when
piping from `column`-ish formatters or hand-typed lists.

**Impact**: low. Workaround is `sed 's/^[[:space:]]*//'` or
`awk '{print $1}'`. But it's a paper cut.

**Expected**: trim leading and trailing whitespace on each line, same
as `./` stripping.

### NEW-5: `create-index --index-file` silently ignored — LOW (UX trap)

The global `--index-file <PATH>` flag is documented to "pass the index
file to any supported command". For `create-index` the actual write
target is `-o / --output`, but `--index-file` is silently accepted and
ignored, and triggers a misleading warning:

```sh
$ hyalo --dir ../mdn/files/en-us create-index --index-file /tmp/mdn.idx
warning: failed to load index: failed to open index file: /tmp/mdn.idx;
         falling back to disk scan
{"path": "../mdn/files/en-us/.hyalo-index", "files_indexed": 14375}
                              ^^^^^^^^^^^^^^ written to default, not /tmp/mdn.idx
```

The warning is technically correct (`create-index` doesn't write the
file the user wanted) but the message implies a *read* problem, not the
flag mismatch.

**Impact**: low but high-confusion. An LLM following the global help
text ("pass via `--index-file`") will get a 114MB file at the default
location plus a warning that means "you're using the wrong flag".

**Expected**: either accept `--index-file` as a synonym for `-o` on
`create-index`, or error early ("`create-index` writes via `-o`,
not `--index-file`").

Also: `create-index -o <path>` still warns about a stale index at the
default location if one exists, even though `-o` redirected the write.
That's noise — `create-index` shouldn't care about the default location
when explicitly targeted elsewhere.

### NEW-6: `--files-from` doesn't dedupe input — LOW

```sh
$ printf 'edge.md\nedge.md\nedge.md\n' | hyalo find --files-from -
# returns edge.md three times
$ printf 'edge.md\nedge.md\nedge.md\n' | hyalo lint --files-from -
# lints edge.md three times
```

Real-world `git diff` output won't duplicate, but `git diff | sort -u`
isn't a thing every caller does; `git log --name-only | uniq` is more
typical and *can* duplicate. For `lint`, duplicates triple the work and
output.

**Impact**: low. Workaround: `sort -u` or `awk '!seen[$0]++'`.

**Expected**: dedupe internally; mention in the help that the input is
deduplicated.

### NEW-7: Most subcommand `--help` blocks lack an `EXAMPLES:` section — LOW (LLM-ergonomics)

Audit:

| Has examples | Lacks examples |
|---|---|
| `lint`, `mv`, `new` | `find`, `set`, `task`, `summary`, `read`, `links`, `create-index`, `types`, `properties`, `tags`, `backlinks`, `remove`, `append`, `views`, `init`, `lint-rules` |

Only 3 of 19 subcommands have an `EXAMPLES:` block. The top-level
`hyalo help` cookbook is excellent and covers the gap for power users,
but an LLM reaching for `hyalo find --help` to learn syntax gets a flag
catalogue, not a recipe.

**Impact**: low pervasive. The bread-and-butter commands (`find`,
`set`, `task`, `read`) are exactly the ones LLMs invoke most.

**Expected**: a 3–6-line `EXAMPLES:` block in each subcommand's
`--help`, derived from the existing top-level cookbook entries.
Particular wins: `find` (BM25 + property + tag combos), `set` (multi-
property, list values, `--apply`), `task` (toggle by section vs line),
`read` (`--section` and `--lines`).

## UX assessment

### What's genuinely great

- **Error messages with suggestions.** Top-tier across the board:
  - typo in property key → `did you mean: status?`
  - typo in subcommand → `a similar subcommand exists: 'find'`
  - unknown type → `available types: note`
  - invalid date → `check month (01–12) and day ranges for the given month`
  - invalid regex → exact `regex` crate diagnostic surfaced
- **Hints** are short, copy-pasteable, and disable cleanly with
  `--no-hints` (useful for scripting).
- **Envelope shape** is documented up front in `hyalo --help` and is
  consistent (`{results, total, hints}`). `--jq` works against it.
- **`hyalo config`** prints the effective configuration in one shot —
  invaluable for diagnosing "why isn't my schema loading?" issues
  (which iter-130 added).
- **iter-138 schema-load errors** for `pattern` + `item_pattern`
  conflicts on the same property surface as a `warning:` on stderr
  before any command runs. Clear root cause.
- **HYALO002** catches real-world drift: `status: completed` but
  unchecked tasks. Found one such error in this repo's own iter-103
  during the run.
- **iter-137** cross-platform link resolution: `[[sub\b]]` and
  `[[sub/b]]` resolve to the same target. Makes vaults authored on
  Windows portable.

### Friction noted

- **Property vs frontmatter terminology**: external docs and other
  tools call it "frontmatter"; hyalo's JSON envelope and CLI use
  "properties". An LLM piping through `jq '.results[].frontmatter.title'`
  will get `null`. Not wrong, but a moment of confusion.
- **`hints: false` vs `--no-hints`**: TOML key uses `false`, CLI uses
  the `--no-hints` boolean form. The top-level help calls this out
  explicitly, which is good — but it's still an inconsistency.
- **`hyalo summary` returns null for `broken_links`, `untyped`,
  `untagged`** at the JSON top level. Either populate or drop the
  keys; null-valued keys force callers to write defensive jq.

## Performance — v0.16.0

All times are wall clock, release build, warm filesystem cache, no
specific tuning. macOS, M-series. Single run each — not statistically
rigorous, but useful as ballparks.

| Vault | Files | Command | Time |
|---|---|---|---|
| own KB | 279 | `find --limit 1` | 23 ms |
| own KB | 279 | `summary` | 34 ms |
| own KB | 279 | `find "iteration"` (BM25) | 88 ms |
| own KB | 279 | `find --property status=completed` | 21 ms |
| own KB | 279 | `lint` (full vault) | 80 ms |
| own KB | 279 | `links` (broken-link scan) | 29 ms |
| MDN | 14,375 | `find --limit 1` (no index) | 1.24 s |
| MDN | 14,375 | `summary` (no index) | 1.15 s |
| MDN | 14,375 | `find "promise"` BM25 (no index) | 3.96 s |
| MDN | 14,375 | `create-index -o /tmp/mdn.idx` | 2.82 s (114 MB) |
| MDN | 14,375 | `find --limit 1` (with index) | 0.67 s |
| MDN | 14,375 | `find "promise"` BM25 (with index) | 0.69 s |
| MDN | 14,375 | `summary` (with index) | 0.88 s |
| MDN | 14,375 | `lint --rule HYALO001` (with index) | 0.93 s |

Indexed BM25 is **~6× faster** than unindexed on 14K files. No
regressions vs prior dogfood baselines on own KB (find --limit 1: 23 ms
now, was ~25 ms at v0.15.0).

## Own KB health snapshot

- **8 lint errors** in 279 files. All in older
  `iterations/done/*` plans plus `research/unified-find-command.md`.
  Mix of HYALO001 (bare `[]`) and one HYALO002 (status=completed with
  unchecked tasks in iter-103). Auto-fixable via
  `hyalo lint --fix --rule HYALO001`.
- **2,268 warnings.** Mostly stylistic (MD013 line length, etc.).
- **11 broken links**, **79 orphans**, **81 dead-ends**. Stable vs
  prior dogfood; worth a `hyalo links` sweep at some point.

## Suggested follow-ups

In rough priority order:

1. **NEW-1** (medium) — collect all `item_pattern` violations per list,
   not just the first.
2. **NEW-2** (medium) — strip multi-segment `--dir` prefix in
   `--files-from`, and fix the all-missing hint to quote the actual
   configured `dir`.
3. **NEW-3** (low, easy) — sync `new --help` text with iter-140's
   `create_dir_all` behaviour. Trivial; just a docs/code drift.
4. **NEW-7** (low pervasive) — add `EXAMPLES:` to the 16 subcommands
   that lack one. Biggest LLM-ergonomics lever in the codebase right
   now.
5. **NEW-5** (low UX trap) — either accept `--index-file` on
   `create-index` or error early instead of silently ignoring it.
6. **NEW-4** (low) — trim whitespace per `--files-from` line.
7. **NEW-6** (low) — dedupe `--files-from` input.

Everything else (terminology nits, null-keyed summary fields) is
quality-of-life polish, not a fix.
