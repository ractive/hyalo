---
title: "Dogfood v0.16.0 — Firefox source tree (zero-frontmatter corpus)"
type: research
date: 2026-06-05
status: active
tags: [dogfooding, iter-152, iter-153, iter-154, iter-155, iter-156]
related:
  - "[[dogfood-results/dogfood-v0160-iter-150-crazy]]"
  - "[[iterations/done/iteration-152-frontmatter-size-budget]]"
  - "[[iterations/done/iteration-153-unicode-tag-symmetry]]"
  - "[[iterations/done/iteration-154-mv-index-patch]]"
  - "[[iterations/done/iteration-155-datetime-type]]"
  - "[[iterations/done/iteration-156-drop-no-tags-warning]]"
---

# Dogfood v0.16.0 — Firefox source tree

Binary: `hyalo 0.16.0 (0a771391c406 2026-06-05)`.

**Primary target:** `/Users/james/devel/firefox` — 2621 `.md` files (1239
non-vendored, the rest `third_party/*`). This is the first dogfood target with
**zero YAML frontmatter** and **zero `[[wikilinks]]`** anywhere — Firefox source
docs are plain markdown with `#` titles and standard `[text](path)` links.
Stresses hyalo's behaviour on a corpus where every frontmatter-driven feature
has nothing to work on.

**Secondary target:** synthetic minimal repros for iter-155 + iter-156 features.

## Prior-report bugs — both fixed

### BUG-3 (10 KiB property value silently loses the file) — FIXED via iter-152

The previous report flagged that a single property value > 10 KiB caused the
file to vanish silently from query results.  Reproducing the same setup against
0.16.0:

```
warning: skipping huge.md: failed to parse YAML frontmatter:
  error: line 2 column 7: budget breached: ScalarBytes { total_scalar_bytes: 11013 }
   --> <input>:2:7
    |
  1 | title: huge
  2 | huge: "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx…
    |       ^ budget breached: ScalarBytes { total_scalar_bytes: 11013 }
```

The file is still skipped (by design — it exceeds the symmetric write-budget
introduced in iter-152), but the user now gets a loud, line/column-annotated
warning naming the offending property and the exact byte count. The "silent
loss" failure mode is gone.

### BUG-4 (unicode/emoji tag write→query asymmetry) — FIXED via iter-153

Wrote a file with `tags: [español, 日本語, "🚀", ableitung]` and queried each:

```
$ hyalo tags
4 unique tags
ableitung    1 file
español      1 file
日本語       1 file
🚀          1 file

$ hyalo find --tag '🚀'      → finds note.md  ✓
$ hyalo find --tag 'español' → finds note.md  ✓
$ hyalo find --tag '日本語'  → finds note.md  ✓
```

All three round-trip. iter-153 (Unicode tag symmetry) landed cleanly.

## New feature verification

### iter-155 — `datetime` property type — WORKING

Schema:

```toml
[schema.types.event]
required = ["title", "type", "when"]

[schema.types.event.properties.when]
type = "datetime"
```

- Valid value (`when: 2026-06-04T15:30:00`) → lint clean.
- Date-only string (`when: 2026-06-04`) → lint error.

### iter-156 — `required` empty-value gate — WORKING

Schema with `required = ["title", "type", "tags"]` and `tags` declared
`type = "list"`. Tested all three inputs:

| Input            | Result                                                       |
|------------------|--------------------------------------------------------------|
| `tags: [item]`   | clean                                                        |
| `tags: []`       | `error: required property "tags" must not be empty (type: note)` |
| `tags: ~` (null) | same error                                                   |

Both empty-array and null fail the required gate, exactly as designed. Atomic-
typed required properties (`title: ""`) still pass (regression guarded by unit
test).

### iter-154 — `mv` snapshot-index patching

Indirectly exercised throughout the dogfood (every `hyalo mv` in this session
patched the index in-place). No regressions. iter-154 itself shipped with new
e2e coverage in PR #180 (since merged), so it was already locked down.

## Findings on firefox

### F-1: Index load dominates short-query latency (MEDIUM, perf)

The firefox snapshot index is **24 MB** for 2621 files. Loading it costs
~2.7 s of wall time, and that cost is paid on every CLI invocation regardless
of how trivial the query is:

| Command (indexed)                       | Wall time |
|-----------------------------------------|-----------|
| `find --limit 1`                        | 2.98 s    |
| `find webextension --limit 5`           | 2.93 s    |
| `summary`                               | 2.88 s    |
| `find --property title`                 | 2.84 s    |
| `find --orphan --limit 5`               | 2.71 s    |
| `lint`                                  | 3.45 s    |

Body-search comparison:

| Command          | Indexed | Disk scan |
|------------------|---------|-----------|
| `find "wptrunner"` | 2.4 s | 5.8 s     |

Index gives a real **2.4× speedup on the work**, but the fixed ~2.7 s load
cost makes that invisible for short queries. For batch/pipeline use, perfectly
fine; for interactive use on a tree this size, the floor is noticeable.

Possible mitigations to consider: mmap-style lazy load (was tried and reverted
on macOS per [[project_perf_architecture]]), smaller index variant for the
"just need file list" case, or just documenting "index helps below ~1 s only
for vaults under a few hundred files."

### F-2: Broken-link reporting on mdbook/sphinx trees is noisy (LOW, by-design but worth a knob)

`find --broken-links` reports **1041 broken-link entries** on firefox out of
2600 total links — 40 %. Breakdown:

| Category                              | Count |
|---------------------------------------|-------|
| Absolute paths (`/toolkit/...`)       | 144   |
| `.html` targets (mdbook-rendered)     | 59    |
| `.rst` targets (sphinx siblings)      | 30    |
| `../` paths (often leave the subtree) | 69    |
| Plain relative `.md` (real rot)       | 89    |
| Other                                 | 650   |

The first three categories (~233 links) aren't bugs in hyalo — they're how
mdbook and sphinx encode cross-references. But on a 1239-file `find --broken-
links` result it's hard to surface the ~89 *actual* broken `.md` references
(which include real firefox doc rot like `gfx/harfbuzz/README.md → CONFIG.md`
that doesn't exist — vendor drop didn't include all upstream files).

**Suggestion:** a `[links]` config knob, e.g.:

```toml
[links]
ignore_extensions = ["rst", "html"]
ignore_pattern = "^/"      # treat absolute paths as out-of-scope
```

…would let users analyzing mdbook/sphinx trees focus on real rot.

### F-3: Same error reported twice when type-check and HYALO00x both fire (MEDIUM, UX)

A file with `type = "event"` and `when: 2026-06-04` (date instead of datetime)
produces two lint errors for the same problem:

```
event-bad.md:
  - property "when" expected datetime (YYYY-MM-DDThh:mm:ss), got "2026-06-04"
  - property `when` has value "2026-06-04" which is not a valid ISO 8601 datetime
    (YYYY-MM-DDThh:mm:ss)
```

The first is from the schema-validator's type check; the second is HYALO004
(the dedicated datetime-format rule from iter-155). Same offending property,
same diagnosis, two error rows. Either:

- HYALO004 should suppress itself when the schema-validator already typed-
  checked the field, or
- the schema-validator should defer to HYALO004 for typed-format reporting.

Either way, double-reporting is confusing — the user might think they have two
distinct problems to fix.

### F-4: Top-level command `task` is singular; `summary` output says "Tasks" (LOW, UX)

`hyalo summary` reports `Tasks: 254/450` and a follow-up user types
`hyalo tasks` (plural) — which clap rejects:

```
error: unrecognized subcommand 'tasks'
  tip: some similar subcommands exist: 'tags', 'task'
```

The tip is good. But the *summary* output uses the plural label "Tasks", which
naturally invites the plural command. Either rename the command to `tasks`
(breaking) or label the summary row "Task" (silly). Most LLM-friendly fix:
accept `tasks` as an alias for `task`.

### F-5: `hyalo links --file X` rejects `--file` (LOW, UX)

```
$ hyalo links --file dir/feature.md
error: unexpected argument '--file' found
```

Other commands (`read`, `backlinks`, `find`) accept `--file`. The `links`
top-level command takes a positional or a subcommand. Inconsistent with peer
commands. Probably trivial to add.

### F-6: First-time `lint` on a stock corpus is a wall of MD-warnings (LOW, UX)

Running `hyalo lint` on firefox produces **7399 warnings across 1650 files**
(0 errors — no schema). The dominant rules are:

| Rule    | Count |
|---------|------:|
| MD022   | 3139  |
| MD012   | 2532  |
| MD034   | 792   |
| MD001   | 329   |
| MD031   | 299   |
| MD040   | 199   |

…all stock mdbook-lint rules. New users running `hyalo lint` on a
"non-prepared" tree get an overwhelming dump. There IS an escape hatch
(`--rule-prefix HYALO`, `lint-rules set MDxxx --enabled false`), and the help
text mentions it — but the first-encounter experience is noisy. A
`--summary` or `--by-rule` mode that prints the table above (one line per
rule, sorted by count) would help users decide what to silence before drilling
in.

## What worked well

- **Parallel scan.** `create-index` runs at ~580% CPU on the 8-core box;
  2621 files indexed in 3.0 s. Cold disk-scan body search (`find "wptrunner"`)
  at 5.8 s on the same set is also fully parallel.
- **Graceful zero-frontmatter handling.** Every property/tag query against
  firefox returns empty results without crashing or printing scary errors.
  `summary` shows `Tags: 0 — (none)` and the rest of the report is still
  useful. This is exactly what should happen.
- **Graceful "no schema configured".** `hyalo lint` runs against firefox
  (which has no `.hyalo.toml`) and produces only body-pass MD warnings — the
  frontmatter pass correctly no-ops.
- **Sharp error messages.** The frontmatter-size warning (F-3 above
  notwithstanding) includes a YAML-style line/column pointer and a byte
  count. Very different from the "silently lost" failure mode of the prior
  release.
- **iter-156 design lands cleanly.** `required = ["tags"]` + `type = "list"`
  doing the right thing without a separate `min_items` knob is exactly the
  promised UX, and round-trips through both empty-array and null inputs.

## Performance

| Tree                      | Files | Index build | Index load + summary |
|---------------------------|------:|------------:|----------------------:|
| own KB                    |   ~300 |       <1 s | <1 s                  |
| firefox                   |  2621 |        3.0 s | 2.9 s                 |

The 24 MB index is on the bigger end of what makes sense to keep in MessagePack;
for the next-larger target (MDN, ~14 k files) it would be worth re-evaluating
whether a streamable / partial-load format pays off.

## Verdict

Two HIGH/MEDIUM open bugs from the prior report are gone. iter-155 and iter-156
both work end-to-end on synthetic schemas. The new findings from firefox are
all UX-flavored — none of them are correctness bugs, and the dominant ones
(F-1 index load floor, F-2 mdbook noise) are real but cleanly addressable with
configuration knobs rather than refactors.

The most actionable items, in order: **F-3** (double error on datetime
mismatch) is the only one that risks misleading a user. **F-2** (link-noise
knob) would meaningfully improve the experience of pointing hyalo at any
mdbook/sphinx tree. **F-1** (index load floor) is a perf ceiling, not a bug,
but worth keeping in mind before adding more index-resident features.
