---
title: "Dogfood v0.16.0 — iter-150 link refactor + crazy edges"
type: research
date: 2026-06-01
status: active
tags: [dogfooding, iter-150]
related:
  - "[[dogfood-results/dogfood-v0160-iter-149-creative]]"
  - "[[iterations/done/iteration-150-link-handling-refactor]]"
---

# Dogfood v0.16.0 — iter-150 link refactor + crazy edges

Binary: `hyalo 0.16.0 (499fa0642370 2026-06-01)`. Built clean.

KBs exercised: throw-away `/tmp` vaults for link-shape edge cases, own
`hyalo-knowledgebase/` (~250 files), MDN `files/en-us/` (~14000 files) for
perf spot-checks.

The session targeted iter-150 (the link handling refactor that was meant
to close BUG-1 and BUG-2 from the iter-149 dogfood), re-verified the
still-open BUG-3 and BUG-4 from the prior round, and then ran a creative
sweep around mv/link semantics looking for the *next* shape of bug.

## iter-150 fixes — verified

### BUG-1 from iter-149 (path-form stripped on mv) — FIXED

```
[[bulk/file-1]]          → [[bulk/moved-1]]              ✓
[[bulk/file-1|aliased]]  → [[bulk/moved-1|aliased]]      ✓
[[bulk/file-1#sec|al2]]  → [[bulk/moved-1#sec|al2]]      ✓ (frag + alias preserved)
```

Done via `mv bulk/file-1.md --to bulk/moved-1.md` against a vault holding
exactly one `file-1.md`. The directory prefix survives; the writer no
longer consults vault-uniqueness to decide emission shape. Matches the
plan's `WrittenForm::PathRelative` preservation guarantee.

### BUG-2 from iter-149 (silent retargeting after mv chain) — FIXED

Reproduced the exact iter-149 scenario:

```
linker.md →  [[bulk/file-1]]
mv bulk/file-1.md  → bulk/moved-1.md   # path form preserved
create-index
mv bulk/moved-1.md → bulk/super-moved.md
```

linker.md now contains `[[bulk/super-moved]]` (correct) rather than
collapsing to a same-basename mismatch in `other/`. Because the path
prefix is preserved through the chain, the second mv never has to consult
ambiguity at all — it just retargets `bulk/moved-1 → bulk/super-moved`.

### Ambiguity surfaces correctly via `links`

In a vault with `a/target.md` and `b/target.md` plus `linker.md →
[[target]]`, `hyalo links` reports the link as `ambiguous` with the
source file and line. `hyalo find --file linker.md` shows
`"target" (unresolved)`. No silent stem-collision retargeting.

## Bug regression testing

### iter-149 BUG-3 (10KB property value silently loses the file) — STILL OPEN (HIGH)

`hyalo set long.md --property "huge=<10000 x's>"` writes successfully
(exit 0, JSON envelope confirms the write). Next `find --file long.md`:

```
warning: skipping long.md: frontmatter too large (no closing `---` found within 200 lines / 8192 bytes)
No results
```

The write path and read path still disagree on what fits. Same severity
and impact as documented in
[[dogfood-results/dogfood-v0160-iter-149-creative]] — file effectively
orphaned. Not in iter-150 scope, so this carrying over was expected, but
worth tracking as an open item.

### iter-149 BUG-4 (unicode tag write/query asymmetry) — STILL OPEN (MEDIUM)

```
hyalo tags                       → shows "日本語  1 file"
hyalo find --tag "日本語"        → invalid character '日' in tag name
```

Same as before. Not iter-150 scope.

### Stale `.hyalo-index` after `mv` — DEFERRED (KNOWN)

iter-150 plan listed "Persistent index gets incremental updates after
`mv`" in scope, but commit `5013e37` documents `mv-side index
incremental patching` as deferred. Confirmed: after `mv b.md --to c.md`,
`hyalo --index-file .hyalo-index find` still lists `b.md`. Normal
commands work because the fs walk re-discovers the renamed file, but the
snapshot is stale until `create-index` is re-run. Tracking, not refiling.

## Bugs Found

### NEW-1: Self-referencing links not rewritten when the file is `mv`'d (HIGH)

Most surprising find this round. A file that links to itself does not
get its own links rewritten on `mv`.

Repro:

```bash
printf -- '---\ntitle: x\ntype: note\ndate: 2026-06-01\n---\nself: [[x]] and [[./x|me]].\n' > x.md
hyalo create-index
hyalo mv x.md --to y.md
cat y.md
# ---
# title: x
# type: note
# date: 2026-06-01
# ---
# self: [[x]] and [[./x|me]].          ← still says [[x]]
```

`hyalo links` then reports two `unfixable` broken links from `y.md` to
`x` — there is no `x.md` left to repair to. The mv envelope reports
`total_files_updated: 0`.

Why HIGH: this is the same family as iter-149 BUG-1/2 (silent breakage
after mv), produces broken links with no signal at mv-time, and survives
the iter-150 unification because the new resolver runs over *inbound*
rewrite plans — and `y.md`'s links to its own old basename are not
classified as inbound by anything that watches the moving file. A user
following the iteration-naming convention (link plans by their iteration
file, then rename when status changes) will hit this regularly.

Fix sketch: `mv` should add the source file to its own
"inbound rewrites" pass after the rename (i.e. treat self-links the same
as cross-file links). The `LinkWriter` itself is fine; the issue is the
caller's iteration over rewrite plans.

### NEW-2: `WrittenForm` collapses `DotRelative` and `MdSuffixed` to bare path on cross-dir source (MEDIUM)

The form-preservation guarantee from the iter-150 plan holds for the
*sibling* case but degrades for the *cross-dir* case.

Sibling case (linker and target in the same directory) — all forms
preserved:

```
[[b]]        → [[c]]      ✓
[[./b]]      → [[./c]]    ✓
[[b.md]]     → [[c.md]]   ✓
[[./b#s|a]]  → [[./c#s|a]] ✓
```

Cross-dir case (linker at vault root, target under `bulk/`, only the
target is renamed within `bulk/`):

```
[[bulk/file-1]]      → [[bulk/moved-1]]    ✓
[[./bulk/file-1]]    → [[bulk/moved-1]]    ✗   (lost `./`)
[[bulk/file-1.md]]   → [[bulk/moved-1]]    ✗   (lost `.md`)
```

Severity MEDIUM: the link still resolves correctly and content is not
lost, but the user's chosen syntax is silently normalised away. This is
exactly the family the iter-150 plan claimed to close: "preserve the
user's written form." A user who consistently writes `./` to mark
descend-into-subdir or `.md` for IDE-style autocomplete sees their
stylistic intent erased the first time they mv anything.

Likely cause: when the source is a different directory than the linker,
the path-rewriter computes a fresh `target_raw` and the
`form == DotRelative | MdSuffixed` branch in the writer is not
re-applied. Sibling case happens to round-trip because the writer
preserves the original token verbatim when the *segment count* doesn't
change.

### NEW-3: `mv` emits no diagnostic when it skips an ambiguous inbound link (MEDIUM)

When `mv` would rewrite a link that resolves to multiple candidates, the
new resolver correctly *refuses* to retarget. But the user gets no
signal at mv-time — the JSON envelope just shows
`total_links_updated: 0`. Discovery requires the user to know to run
`hyalo links` separately.

The iter-150 plan explicitly called this out as a "hard diagnostic" goal
("surfacing BUG-2 ... as a hard diagnostic instead of a silent
rewrite"). Today the *rewrite* is suppressed (good — that's the fix),
but the *diagnostic* is missing. Either:

- emit a `warnings` array in the mv envelope listing skipped ambiguous
  inbound links, or
- print a stderr line `note: 1 ambiguous inbound link was not rewritten
  (run hyalo links to inspect)`.

Also: `--allow-ambiguous` accepted as a flag but had no visible effect
in my repro (same `total_links_updated: 0`). Either the flag wasn't
exercised by my test (the link was ambiguous *before* the mv started,
not as a *result* of the mv) or the flag is wired up for a narrower
case than the name implies. Worth a help-text clarification.

## UX Issues

### UX-1: `--index-file` flag name vs. `--index` from prior hints (LOW)

I reached for `hyalo --index find ...` and got
`unexpected argument '--index' found ... tip: a similar argument
exists: '--index-file'`. The clap suggestion is great. But: I had
internalised `--index` from earlier sessions, and `--index-file` reads
slightly wrong because the value is a *path to* the file, not a file
itself, and the auto-discovered case has no file argument. Minor — the
clap hint absorbs the friction in practice.

### UX-2: Path-form preservation tests are silent successes (LOW)

There is no command that says "what would mv do to this link?" without
actually doing the mv. `hyalo mv --dry-run` reports the *files updated*
plan but not the *rewritten text* per link. A `--show-rewrites` (or
JSON `rewrites: [{file, line, before, after}]`) on dry-run would have
let me verify form preservation in one command per shape instead of
running each repro.

## What Worked Well

- **iter-150 closes the family.** This is the seventh attempt at the
  link writer/resolver split and the first one where every shape I
  threw at the sibling case round-trips losslessly, and the cross-dir
  case preserves *at minimum* the path prefix. Path-form preservation
  is the dominant case in practice (it's what Obsidian generates), so
  this is the right wedge.
- **Ambiguity is now visible.** `hyalo links` reports `ambiguous: N`
  with file+line+target. Previously silent stem collisions are now
  inspectable. The diagnostic just needs to surface at mv-time too
  (NEW-3).
- **`mv` creates intermediate directories.** `mv a.md --to
  new/dir/a.md` works without `mkdir -p` first. Small thing, but
  pleasant.
- **`did you mean c.md?` after mv.** Trying to `find --file b.md`
  after the rename gives `file not found ... hint: did you mean c.md?`
  Lovely Levenshtein touch.
- **MDN scale stable.** `create-index` 2.5s, `summary` 1.3s, BM25
  search 4s on 14k files — within noise of the iter-149 baselines, no
  regression from the link refactor.

## Performance

| KB | Files | Command | Time |
|---|---|---|---|
| MDN en-us | ~14000 | `create-index` | 2.5 s |
| MDN en-us | ~14000 | `summary --format json` | 1.3 s |
| MDN en-us | ~14000 | `find "javascript closures" --limit 3` | 4.0 s |
| MDN en-us | ~14000 | `find --property "page-type=javascript-function"` | 1.0 s |
| /tmp test | 4 | `mv` single (cold) | <50 ms |

No regression vs the iter-149 baselines (`create-index` 2.6 s, BM25
~4 s). The link-resolver unification did not show up as a scan-path
cost.

## Verdict

iter-150 delivers on the headline promise: BUG-1 and BUG-2 are closed
and the new ambiguity diagnostic prevents silent stem-collision
retargeting. The refactor is worth its weight — having one resolver and
one writer makes the remaining gaps (NEW-1, NEW-2) feel like *finite,
nameable* edges rather than the open-ended "iteration eight of the same
family" the iter-150 plan was trying to escape.

Recommended next iteration: NEW-1 (self-link mv) and NEW-3 (mv-time
ambiguity diagnostic). Both are in the writer caller, not the writer
itself, and both close the same UX gap: "mv reports success but
silently leaves broken/skipped links." NEW-2 (cross-dir form
preservation) is a smaller, scoped follow-up that completes the
iter-150 promise.
