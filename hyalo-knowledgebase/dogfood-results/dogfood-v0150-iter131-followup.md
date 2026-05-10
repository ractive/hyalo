---
title: Dogfood v0.15.0 — iter-131 follow-up (regression checks + fresh issues)
type: research
date: 2026-05-10
status: active
tags: [dogfooding, lint, ux, mv, links, performance]
related:
  - "[[dogfood-results/dogfood-v0150-iter127-130]]"
  - "[[iterations/iteration-131-dogfood-v0150-fixes]]"
  - "[[iterations/iteration-126-markdown-linter]]"
---

# Dogfood v0.15.0 — iter-131 follow-up

Tested binary: `hyalo 0.15.0 (kb dir: hyalo-knowledgebase)` built fresh from `main` at d51c725.

KBs exercised:
- own (`hyalo-knowledgebase/`, 264 files)
- MDN (`/Users/james/devel/mdn/files/en-us/`, 14375 files, index built into `/tmp/mdn.idx` ~114 MB)
- GitHub Docs (`/Users/james/devel/docs/content/`, 3521+ files)
- two synthetic stress KBs (`/tmp/lint-stress`, `/tmp/mv-test`, `/tmp/mv-test2`) for targeted edge cases
- VS Code KB not present locally — skipped

## New Feature Verification (iter-131 fixes)

All six iter-131 acceptance criteria reproduce as fixed:

### BUG-1: `find --file <abs-path-inside-vault>` (FIXED)

`hyalo find --file /Users/james/devel/hyalo/hyalo-knowledgebase/iterations/iteration-131-dogfood-v0150-fixes.md`
now resolves and emits the LLM-misuse `warning:` first, then the result. An abs path *outside* the
vault still errors with `file resolves outside vault boundary` (good).

### BUG-2: `lint-rules set --severity <default>` no-op (FIXED)

`.hyalo.toml` is no longer touched when the desired severity already equals the default — verified
by setting/un-setting and diffing.

### BUG-3: `summary --format json` top-level `dir` field (FIXED)

```
$ hyalo summary --format json | jq '.dir'
"hyalo-knowledgebase"
```
Confirmed against own KB and external KBs.

### UX-1: Redundant `--dir` warning prefix (FIXED)

Now emits `note: --dir is redundant; .hyalo.toml already sets dir = "hyalo-knowledgebase"` (single
prefix). Previously was `warning: note:`.

### UX-2: `find --file <abs-path>` no longer silently empty (FIXED)

The new misuse warning and the resolution path together remove the silent-zero failure mode.

### UX-3: Banner emojis on piped output (FIXED)

`hyalo --help | head` shows the plain-text banner (`hyalo runs against hyalo-knowledgebase ...`) on
non-TTY stdout. Earlier `ℹ️ ` / `⚠️` are gone from piped output.

## Bug Regression Testing (prior reports)

Re-ran repros from `dogfood-v0140-iter126-markdown-linter` and earlier:

| Prior bug | Status |
| --- | --- |
| dogfood-v0140 BUG-1: `lint-rules set --severity` panics on scalar→table | STILL FIXED |
| dogfood-v0140 BUG-2: `--fix` JSON envelope diverges from spec | STILL FIXED |
| dogfood-v0140 BUG-3: `--fix` text output indistinguishable from read-only | STILL FIXED — `would fix`/`fixed`/`remain` markers are clear |
| dogfood-v0140 BUG-4: HYALO003 singular grammar | n/a — HYALO003 not present in current rule list |
| dogfood-v0140 BUG-5: invalid `mode` silently falls back | not re-tested (low) |
| dogfood-v0150 BUG-1/2/3 + UX-1/2/3 | FIXED (see above) |

## Bugs Found

### BUG-A: `hyalo mv` does not rewrite `[[wikilinks]]` to the moved file (HIGH)

`hyalo mv` advertises in `--help`: *"Rewrites all [[wikilinks]] and [markdown](links) in other files
that pointed to the old path."* In practice only the markdown-link form is rewritten — wikilinks
become broken silently.

Repro:

```
$ mkdir /tmp/mv-test2 && cd /tmp/mv-test2
$ printf -- '---\ntitle: A\n---\nSee [B](b.md) and [[b]] and [[b|alias]].\n' > a.md
$ printf -- '---\ntitle: B\n---\nhi\n' > b.md
$ hyalo --dir /tmp/mv-test2 mv b.md --to sub/b.md --format text
Moved b.md → sub/b.md
  a.md: [B](b.md) → [B](sub/b.md)

$ cat a.md
---
title: A
---
See [B](sub/b.md) and [[b]] and [[b|alias]].
```

Expected: `[[b]]` → `[[sub/b]]`, `[[b|alias]]` → `[[sub/b|alias]]`, and the report should list
those rewrites.
Actual: only the markdown link is rewritten. The wikilinks are now broken (`hyalo find
--broken-links` confirms `"b" (unresolved)` in `a.md`). `hyalo backlinks sub/b.md` reports
"No backlinks found" because the inbound wikilink edge was lost.

`hyalo mv ... --dry-run` exhibits the same blind spot — the preview only mentions the markdown
link.

### BUG-B: `hyalo set --property date=<garbage>` accepts any string when no schema applies (LOW)

```
$ hyalo --dir /tmp/task-test set x.md --property "date=not-a-date" --format text
date=not-a-date: 1/1 modified
  "x.md"
```
No warning; later `hyalo find --sort date` would silently misorder this file. Schema-validated KBs
(own KB) presumably catch this via `lint --strict`, but on a schema-less KB the set just succeeds.

Expected: when the property name is `date`, do at minimum a heuristic ISO-8601 validity check, or
emit a `note: 'not-a-date' does not look like an ISO date — store as string?` confirmation. At a
minimum, `lint` should flag a non-date stored under `date`.

### BUG-C: `find --tag <typo>` returns empty with no suggestion (LOW)

```
$ hyalo find --tag iteraton --format text
(empty)
```
By contrast, subcommand typos (`hyalo finnd`) suggest `find`. Tag/property typos are a much more
common LLM mistake and would benefit from the same fuzzy hint.

## UX Issues

### UX-A: `create-index` with `-o /tmp/...` errors with `--allow-outside-vault` hint that's easy to miss (MEDIUM)

Building an index for a vault you don't own (MDN, docs) is the most natural use case. The first
attempt fails with:

```
{
  "hint": "use --allow-outside-vault to override",
  "path": "/tmp/mdn.idx"
}
```

Then `find --index-file /tmp/mdn.idx` *also* falls back silently with
`warning: failed to load index: failed to open index file: /tmp/mdn.idx; falling back to disk scan`
because the file was never written. That's correct, but users will scratch their heads — consider
either (a) auto-allowing for read-only KBs, or (b) printing the JSON hint as a `note:` line on
text output too (the JSON hint isn't visible when `--format text`).

### UX-B: `hyalo links` requires a subcommand; help text first paragraph reads as if `hyalo links` runs the dry-run (MEDIUM)

```
$ hyalo links --format text
error: 'hyalo links' requires a subcommand but one was not provided
```
But `hyalo links --help` opens with: *"Detect and repair broken wikilinks ... Default behaviour is
a dry run — no files are modified."* This made me try `hyalo links --dry-run` first, which fails
with `unexpected argument '--dry-run'`. The natural default would be to execute the dry-run when
no subcommand is supplied, mirroring `hyalo views` (which defaults to `list`).

### UX-C: `--index-file` flag must be on the subcommand, not the global (MEDIUM)

```
$ hyalo --dir <X> --index-file /tmp/x.idx lint ...
error: unexpected argument '--index-file' found
  tip: 'lint --index-file' exists
```
The clap tip is helpful, but the inconsistency with `--dir` (which is global) is jarring — every
read-only command takes `--index-file`, so promoting it to a global option (with the current
per-subcommand variant kept for back-compat) would remove a constant friction point.

### UX-D: `views <name>` doesn't work; you must use `find --view <name>` (LOW)

`hyalo views open-tasks` errors `unrecognized subcommand 'open-tasks'`. The natural intuition,
once you've run `hyalo views list`, is to invoke a saved view by name. Either map bare-name to
`find --view`, or explicitly mention `find --view` in `views list` output.

### UX-E: `lint --strict` claims to promote "missing-type / undeclared-property warnings" but those don't appear on schema-less KBs (LOW)

On `/tmp/lint-stress` (no `.hyalo.toml`, two files with bad/no frontmatter), `lint --strict`
showed only MD\* rules, no missing-type promotion. This may be by design (only schema-aware KBs
have a notion of declared types) but the `--help` could clarify that missing-type strictness
requires a `types` schema in `.hyalo.toml`.

### UX-F: `hyalo find --sort path` is rejected; it's spelled `file` (LOW)

The error message is great (`valid values are 'file', 'modified', ...`) but `path` is the more
natural alias and could be accepted as a synonym. Likewise `--reverse` is fine but `--desc` would
be more familiar to most users.

## Performance

All times are wall-clock from `time` on warm cache, macOS arm64.

| Operation | Own KB (264) | Docs (3521) | MDN no-index (14375) | MDN with index |
| --- | --- | --- | --- | --- |
| `find --limit 1` | 0.020 s | 0.160 s | – | 0.99 s |
| `find <bm25>` | 0.082 s | 1.17 s | 4.74 s | 0.76 s |
| `summary` | 0.034 s | 0.38 s | 1.39 s | 1.87 s |
| `find --property status=completed` | 0.018 s | 0.16 s | – | – |
| `lint` (whole KB) | 0.05 s | 1.13 s | – | 3.84 s |
| `lint --fix --dry-run` | – | 1.67 s | – | – |
| `find --broken-links` | – | – | – | 1.10 s |
| `create-index` | – | – | 3.98 s | – |

Index payoff for BM25 search on MDN is ~6×. No regressions observed vs. v0.15.0 numbers in the
prior report. Lint with index on MDN (~3.8 s) is bounded by needing to read every file body — the
index speeds up frontmatter-only checks but not body rules.

## What Worked Well

- All six iter-131 fixes verified — clean, no surprises.
- Banner is crisp and TTY-aware.
- `lint --fix --dry-run` output is genuinely scannable: `would fix` / `fixed` / `remain` markers
  + per-rule grouping on the JSON side make this the most ergonomic CLI lint workflow I've used.
- `hyalo find --view <name>` plus extra flags ergonomics is excellent — `find --view open-tasks
  --tag iteration` "just worked".
- Hint chains at the end of every command continue to be the killer ergonomics feature for an LLM
  user. The drill-down on `lint`/`summary` is perfectly tuned.
- Misuse warning fires on `hyalo --dir hyalo-knowledgebase ...` and `find --file <abs-path>` —
  exactly the cases I'd otherwise have wasted time on.
- Subcommand fuzzy suggestions (`finnd` → `find`) are great.
- Performance is solid; index gives a real and reproducible speedup.

## What Was Awkward / Non-Ergonomic / Missed

(Per the user's explicit request.)

1. **`hyalo mv` silently breaking wikilinks** (BUG-A) was the single biggest surprise — and the
   one that would bite an LLM the hardest, because we lean on wikilinks for the `[[related]]`
   chain. The help text actively promises this works.
2. **`--index-file` is a per-subcommand flag**, not global. After `--dir` was promoted to global
   in iter-130, this is the next inconsistency to clean up. I instinctively typed
   `hyalo --dir X --index-file Y lint` four times before learning.
3. **`hyalo links` requires a subcommand**; bare `hyalo links` should default to `links fix
   --dry-run` (matching the `views` precedent). The help text describes the default behavior as
   the dry-run, which conflicts with the actual error.
4. **`hyalo views <name>` not callable directly** — small papercut but the documentation in
   `views list` could mention the `find --view` invocation form.
5. **No fuzzy suggestion on `--tag <typo>` / `--property <typo>`**. Subcommand typos get
   suggestions; tag/property typos return empty with no guidance. This is a high-leverage fix for
   LLM users.
6. **`create-index -o /tmp/...` requires `--allow-outside-vault`** even for an explicit absolute
   path that the user just typed. The hint exists in JSON but not in the text output. For
   read-only external KBs (MDN, docs), opting out of the safety check is the common case, not the
   edge case.
7. **`set --property date=<garbage>`** accepts any string with no warning when no schema applies.
   Even a one-line note: would help LLMs catch their own typos.
8. **`lint --strict` documentation gap**: the README/CLI mention "missing-type and
   undeclared-property warnings" but those didn't surface on the schemaless stress KB. A line in
   `--help` clarifying that strict's promotions are schema-driven would close the loop.
9. **Banner truncation when `--help | head -10`**: on a narrow terminal the banner wraps mid-word
   in the first line ("Don't `cd` into it; pass paths\n relative ..."), which I'd rather not see
   inside `--help`. Consider keeping the banner above `--help` to a single short line.
10. **No way to ask hyalo "what do I have here?" in one shot** — `summary` is great, but I'd love a
    `hyalo doctor` or `hyalo overview` that runs `summary + lint --strict --limit 5 + links fix
    --dry-run` and prints a one-screen health report. Each piece exists; composing them is on me.

## Recommendations (priority order)

1. Fix `hyalo mv` wikilink rewriting (HIGH; matches advertised behavior).
2. Promote `--index-file` / `--index` to global flags.
3. Make `hyalo links` (no subcommand) run `links fix --dry-run`, mirroring `views`.
4. Add fuzzy-match suggestions for `--tag` / `--property=<key>` typos.
5. Print the `--allow-outside-vault` hint as a `note:` line on text output for `create-index`.
6. Add `lint`-side date validity check (or warning on `set` for `date` property when value isn't
   ISO-8601).
7. Optional: `hyalo doctor` aggregate command.
