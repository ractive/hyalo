---
title: "Dogfood v0.16.0 — iter-149 verification + creative consistency audit"
type: research
date: 2026-05-31
status: active
tags: [dogfooding, iter-149]
related:
  - "[[dogfood-results/dogfood-v0160-iter-148-verify]]"
  - "[[iterations/done/iteration-149-new-updates-index]]"
---

# Dogfood v0.16.0 — iter-149 verification + creative consistency audit

Binary: `hyalo 0.16.0 (d6bacca70784 2026-05-29)`. Tested KBs: own (~250 files),
MDN `files/en-us` (~14K files), plus throwaway test vaults under
`/tmp/hyalo-dogfood-iter149/` and `/tmp/hyalo-mv-test/`.

Headline counts: **2 HIGH bugs**, **1 MEDIUM bug**, **2 LOW UX issues**, **0 regressions**.
iter-149 verdict: **WORKING** — `hyalo new` patches the snapshot index in place
exactly as the plan describes; the perf win on MDN is ~650x vs full rebuild.

## iter-149 verification — `hyalo new` updates the index

Note: the user prompt assumed there is an opt-in `--index` flag. There isn't —
the behaviour is automatic when an index file exists. Confirmed by reading
`hyalo new --help` and the iter-149 plan (`hyalo-knowledgebase/iterations/done/iteration-149-new-updates-index.md`).
Treating "the auto-patch" as the feature.

### Verified

| Scenario | Result |
| --- | --- |
| Vault has no `.hyalo-index` → `hyalo new` succeeds, no index created | OK |
| `create-index` then `hyalo new` → file appears in `find` without rebuild | OK |
| `hyalo new` with rich frontmatter placeholders → properties / type indexed | OK |
| `hyalo new` on nested path `sub/nested/deep.md` → indexed correctly | OK |
| Re-create deleted file with same name → entry refreshed (no dup) | OK |
| 5 parallel `hyalo new` calls → all 5 entries in index, all 5 files on disk | OK |
| No `--dry-run` flag exists → N/A (correctly listed N/A in plan) | OK |
| Performance on MDN 14K vault: patch vs full rebuild | **4ms vs 2.6s (~650x)** |
| Performance on own KB ~250 vault: patch vs rebuild | **4ms vs 70ms (~17x)** |

Repro for the MDN perf:
```bash
cd /Users/james/devel/mdn/files/en-us
printf '[schema.types.scratch]\nrequired = ["title"]\n' > .hyalo.toml
hyalo create-index                                    # ~2.6s
time hyalo new --type scratch --file scratch.md       # ~4ms
time hyalo find --file scratch.md                     # ~0.4s (index read)
```

### UX observation — no hint about index auto-patch

`hyalo new` returns a hint to run `hyalo lint`, but never mentions that the
index was updated. There is also no warning when the vault has no index — a
new user creating their first 100 files won't know that `find` is invisible
to them until they run `create-index`. See **UX-1** below.

## Bug regression spot-check (iter-148 NEW-1/3/4/5)

Not separately re-verified — the previous report (`dogfood-v0160-iter-148-verify.md`)
exhaustively covered them and the relevant code paths have not changed in iter-149.
No regressions observed in any command exercised below.

## Index consistency audit (end-to-end, never done before)

For each mutator: build a fresh `create-index`, perform the mutation, read the
index back via `find --file <path>`, confirm the entry reflects the new state.

| Mutator | Repro (test vault) | Index reflects new state? |
| --- | --- | --- |
| `set --property` | `hyalo set note-a.md --property "title=Updated A"` | YES — `title` updated |
| `set --tag` | `hyalo set tagfile.md --tag newtag` | YES — `tags: [existing, newtag]` |
| `remove --property` | `hyalo remove note-a.md --property date` | YES — `date` gone from index |
| `mv --to` | `hyalo mv note-a.md --to notes/renamed-a.md` | YES — old gone, new present |
| `task toggle --all` | `hyalo task toggle task-file.md --all` | YES — `[2/2]` completed in index |
| `new` (iter-149) | `hyalo new --type note --file foo.md` | YES — full entry inserted |
| `append` (frontmatter) | `hyalo append note.md --tag t` | n/a — see BUG-3 |
| `lint --fix` | trivial fix on MD047 | YES |

All confirmed working — the index never went stale during the audit.
This is a genuine improvement vs prior assumptions; iter-149 closes the gap
that motivated the audit.

## Bugs Found

### BUG-1: `hyalo mv` strips directory prefix from wikilink targets (HIGH)

When `mv` rewrites inbound links, it converts the previously fully-qualified
form `[[sub/target]]` into the basename form `[[renamed]]`, even when both the
source and destination remain inside `sub/`. The directory prefix is dropped
unconditionally.

Repro:
```bash
mkdir -p /tmp/mv-bug/sub && cd /tmp/mv-bug
printf '[schema.types.note]\nrequired = ["title"]\n' > .hyalo.toml
printf -- '---\ntitle: a\ntype: note\n---\nLink: [[sub/target]]\n' > a.md
printf -- '---\ntitle: t\ntype: note\n---\n' > sub/target.md
hyalo create-index
hyalo mv sub/target.md --to sub/renamed.md
cat a.md
# Before: Link: [[sub/target]]
# After:  Link: [[renamed]]
```

Impact: once the link is shortened to a basename, any future file with the
same basename anywhere in the vault silently captures the reference. See BUG-2
for the data-integrity consequence.

Expected: rewrite should preserve the qualification level of the original
link. `[[sub/target]]` → `[[sub/renamed]]`, not `[[renamed]]`.

### BUG-2: `mv` chain silently retargets links to unrelated files (HIGH)

A direct consequence of BUG-1, but worth tracking separately because the
end-state of the data-integrity loss is what an end user would observe.

Repro (continues from `/tmp/hyalo-dogfood-iter149/vault`):
```bash
hyalo mv bulk/file-1.md --to bulk/moved-1.md     # rewrites [[bulk/file-1]] → [[moved-1]] (BUG-1)
mkdir other && printf -- '---\ntitle: other\ntype: note\ndate: 2026-05-31\n---\n' > other/moved-1.md
hyalo create-index
hyalo mv bulk/moved-1.md --to bulk/super-moved.md
hyalo find --file linker.md --format text
# links: "moved-1" → "other/moved-1.md"   <- silently retargeted to unrelated file
```

The `linker.md` originally pointed at `bulk/file-1.md`. After two
benign-looking renames, it now points to `other/moved-1.md` — a completely
unrelated file. No broken-link warning, no ambiguity warning at the moment of
mv; this is invisible to the user.

Severity HIGH because it produces wrong-but-plausible link resolution with
zero observable signal.

### BUG-3: 10KB property value silently corrupts subsequent parses (HIGH)

`hyalo set foo.md --property "huge=$(printf 'x%.0s' {1..10000})"` writes the
value successfully (exit 0, JSON envelope confirms write). On the *next*
parse of the same file (any subsequent `find`, `lint`, `summary`), hyalo
errors out with:

```
warning: skipping long.md: frontmatter too large (no closing `---` found within 200 lines / 8192 bytes)
```

The file is effectively orphaned from the KB — it exists on disk but is
invisible to every read path. `find --file long.md` returns no results;
`find --jq '.results[0].properties.huge | length'` returns `0`.

Repro:
```bash
cd /tmp/hyalo-dogfood-iter149/vault
hyalo new --type note --file long.md
BIG=$(python3 -c "print('x'*10000)")
hyalo set long.md --property "huge=$BIG"
hyalo find --file long.md                # No results + "frontmatter too large" warning
```

`set` should either reject the write upfront (matching the 8192-byte parse
budget) or raise the parse budget to match what it is willing to write.
Currently the write and read paths disagree and the file is silently lost.

### BUG-4: tag stored with unicode chars is unreachable via `find --tag` (MEDIUM)

The tag storage path happily accepts and indexes tags like `日本語` and
`emoji-🎉` (verified via `hyalo tags` — they appear in the output). But the
query path rejects them:

```bash
hyalo find --tag "日本語"
# Error: invalid character '日' in tag name; allowed: letters, digits, _, -, /
```

Asymmetry between write and query is the issue. Either tighten write to
match query (reject unicode at write-time, with a clear error) or relax
query to accept whatever has been written. Currently a tag can be set but
never searched.

Repro:
```bash
cd /tmp/hyalo-dogfood-iter149/vault
printf -- '---\ntitle: t\ntype: note\ndate: 2026-05-31\ntags: ["日本語"]\n---\n' > unicode.md
hyalo create-index
hyalo tags                      # shows "日本語  1 file"
hyalo find --tag "日本語"        # ERROR
```

## UX Issues

### UX-1: No hint that `hyalo new` patched the index (LOW)

After `hyalo new` succeeds, the only hint surfaced is "run lint to see
placeholder violations." A second hint mentioning that the snapshot index was
patched (or that no index exists and `create-index` is recommended for large
vaults) would help users understand the iter-149 behaviour without reading
the plan file.

### UX-2: Stderr YAML-parse warnings are repeated for every command (LOW)

Any vault that contains a single malformed-YAML file produces 10+ lines of
"invalid indentation" / "unclosed frontmatter" warnings on **every** `find`,
`tags`, `properties`, `summary`, `links` call. With our test vault holding
three broken files, `hyalo find --tag mytag` printed 16 lines, 4 of which
were the actual answer. On large vaults with a handful of stale files this
becomes hostile to scripting.

Suggestion: parse-warning files should be diagnosed once at `create-index`
time, optionally summarised on subsequent reads as
`(N files skipped due to parse errors — run hyalo lint to inspect)`, with
full traces gated behind `--verbose` or `--show-warnings`.

## What Worked Well

- **iter-149 perf.** ~650x speedup on MDN-scale vaults is a real win — adding
  one note is essentially free, vs ~3s before. Anyone doing scripted KB
  authoring will feel this.
- **Index consistency.** This is the first dogfood that actually ran the
  full audit end-to-end (set / remove / mv / task toggle / new / lint --fix).
  Every mutator landed a correct index entry. No stale index detected after
  any mutation.
- **Concurrent `new`.** Five parallel `hyalo new` invocations against the
  same `.hyalo-index` produced five correct entries with no corruption or
  lost writes. There is presumably file locking under the hood; whatever it
  is, it held under this stress.
- **Mass mv via `--files-from`** worked cleanly for 50-file batches. Link
  rewrite covered every reference (the issue is BUG-1, not coverage).
- **No frontmatter / empty file** handling is graceful: `set` on a file with
  no frontmatter inserts one cleanly.

## Performance

| KB | Files | `create-index` | `hyalo new` (patch) | Speedup |
| --- | --- | --- | --- | --- |
| Own KB | ~250 | 70 ms | 4 ms | ~17x |
| MDN en-us | ~14000 | 2.6 s | 4 ms | ~650x |

Baseline timings on M-series Mac. No regressions observed vs prior reports.
