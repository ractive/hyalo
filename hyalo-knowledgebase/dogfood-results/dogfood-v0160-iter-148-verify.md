---
title: "Dogfood v0.16.0 — iter-148 fix verification"
type: research
date: 2026-05-31
status: active
tags: [dogfooding, iter-148]
related:
  - "[[dogfood-results/dogfood-v0160-iter-144-147]]"
  - "[[iterations/iteration-148-dogfood-v0160-iter147-fixes]]"
---

# Dogfood v0.16.0 — iter-148 fix verification

Binary: `hyalo 0.16.0 (d6bacca70784 2026-05-29)`.

KBs exercised:
- Own KB — `hyalo-knowledgebase/` (283 files)
- MDN — `../mdn/files/en-us/` (14,375 files)
- GitHub Docs — `../docs/content/` (3,640 files)

Primary goal: re-run the exact repros from
[[dogfood-results/dogfood-v0160-iter-144-147]] to confirm iter-148 closed
NEW-1 through NEW-5 and the `create-index` outside-vault friction. Some
creative exploration and perf spot-checks alongside.

## Bug regression testing

### NEW-1: large-vault index hint crowded out — STILL FIXED

On MDN (`--dir files/en-us`, 14,375 files) the create-index hint is now
the FIRST hint in the summary, ahead of properties / tags / orphan /
broken-link hints:

```
  -> hyalo create-index  # Vault has 14375 files — create an index for faster queries:
  -> hyalo properties --dir files/en-us --format text
  -> hyalo tags --dir files/en-us --format text
  -> hyalo find --orphan --dir files/en-us --format text  # 4245 orphan files
  -> hyalo find --broken-links --dir files/en-us --format text  # 49933 broken links
```

Same on GitHub Docs (3,640 files) — index hint first. The
"prepend instead of append" decision worked exactly as planned: the
hint is now visible on the very vaults where the speedup matters most
(see Performance below — 5.3× on MDN).

### NEW-3: multi-segment `--dir` prefix strip in `--files-from` — STILL FIXED

The marquee `git diff --name-only`-style recipe now works on a
multi-segment vault dir:

```
$ cd ../mdn
$ echo "files/en-us/games/index.md" \
    | hyalo --dir files/en-us find --files-from - --no-hints
... files_missing: 0, total: 1
```

Edge cases verified:

- Mixed prefixed + bare entries (`games/index.md` and
  `files/en-us/games/index.md`) dedup to one result.
- Path that doesn't exist after strip yields a single missing entry and
  the helpful "all --files-from entries were missing" hint pointing at
  the vault dir.
- Empty / whitespace lines are tolerated.
- `#` comment lines are counted as `files_skipped_non_md`.

### NEW-4: `set` / `remove` / `append` help text omits `--files-from` — STILL FIXED

All three subcommands now read:

```
Target file(s) (repeatable). Mutually exclusive with --glob and --files-from
```

Matches `find` / `task toggle`. Docs in sync with behavior.

### NEW-5: `summary` envelope `dir` key duplicated — STILL FIXED

```
$ hyalo summary --format json | jq '{top: .dir, inner: .results.dir}'
{ "top": "hyalo-knowledgebase", "inner": null }
```

`results.dir` is gone; `dir` lives only at the envelope level — matches
every other command's shape.

### Friction: `create-index` outside vault — STILL FIXED

`create-index --allow-outside-vault -o /tmp/mdn.idx` now works:

```
$ rm -f /tmp/mdn.idx
$ hyalo --dir files/en-us create-index -o /tmp/mdn.idx
{ "error": "output path is outside the vault boundary",
  "hint": "use --allow-outside-vault to override", ... }
$ hyalo --dir files/en-us create-index --allow-outside-vault -o /tmp/mdn.idx
{ "results": { "files_indexed": 14375, "path": "/tmp/mdn.idx", "warnings": 0 } }
```

The hint and the flag agree. External read-only KBs (MDN, GitHub Docs)
can now keep the index in `/tmp/` without polluting the upstream repo.
Took 2.7s wall to index 14,375 MDN files into a 114 MB snapshot.

## New feature verification

No new features in iter-148 — it was a defensive-fix iteration. The
features under test (iter-144 hint, iter-145 unified resolution /
`--files-from`, iter-146 git provenance, iter-147 task-files-from) are
all covered by the regression checks above. `--version` continues to
include the git sha + date as iter-146 specified
(`hyalo 0.16.0 (d6bacca70784 2026-05-29) ...`).

## Bugs found

None. All five iter-148 targets verified closed; no regressions on the
own KB, MDN, or GitHub Docs.

## UX issues

### UX-1: `--files-from` comment-line handling is silent — LOW

A line beginning with `#` is treated as a missing file and counted as
`files_skipped_non_md`. That's defensible (it doesn't end in `.md`) but
many tools (e.g. `pip`, `requirements.txt`) treat `#` as a comment.
Hyalo doesn't document either way. If `#`-comment support isn't added,
a one-line note in `--files-from` help would prevent quiet skips when a
user pipes a hand-written list with comments.

Not a regression, not a blocker, just a paper-cut worth a 1-line
docstring fix.

## What worked well

- The "all --files-from entries were missing" hint pointing back at the
  vault dir is exactly the kind of just-in-time guidance that turns a
  silent empty-result into a debuggable one.
- iter-148 changed the JSON envelope (NEW-5) without any visible
  downstream pain — small surface, easy fix.
- The hint priority reorder (NEW-1) is the right call: it makes the
  single highest-payoff hint impossible to miss on the vaults where it
  matters, without raising the `MAX_HINTS` ceiling.
- Indexed MDN search is genuinely fast (~0.7-0.9s end-to-end on 14k
  files, including disk-load of a 114 MB index).

## Performance

All timings are wall-clock from a single `time` invocation; warm FS
cache; `--no-hints --format json > /dev/null`.

| KB | Files | Command | Time |
|---|---|---|---|
| own | 283 | `find --limit 1` | 0.026s |
| own | 283 | `find "iteration"` | 0.101s |
| own | 283 | `summary` | 0.040s |
| MDN | 14,375 | `find "css grid layout" --limit 5` (no index) | 4.97s |
| MDN | 14,375 | `find "css grid layout" --limit 5` (with index) | 0.93s |
| MDN | 14,375 | `summary` | 1.92s |
| MDN | 14,375 | `create-index --allow-outside-vault` | 2.72s |
| MDN | 14,375 | `find --property page-type=css-property` (indexed) | 0.72s |
| GitHub Docs | 3,640 | `summary` | 0.45s |
| GitHub Docs | 3,640 | `find "actions workflow" --limit 3` | 1.33s |

Indexed BM25 vs non-indexed BM25 on MDN: **5.3× speedup**. Consistent
with prior reports; no regressions.

## Verdict

iter-148 lands cleanly. All five targets (NEW-1, NEW-3, NEW-4, NEW-5,
create-index friction) verified fixed against the exact repros from
[[dogfood-results/dogfood-v0160-iter-144-147]]. No new bugs, no
regressions, one LOW-severity UX note about `#` comments in
`--files-from`.
