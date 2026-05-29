---
title: "Dogfood v0.16.0 — iter-144 through iter-147"
type: research
date: 2026-05-28
status: active
tags: [dogfooding, iter-144, iter-145, iter-146, iter-147]
related:
  - "[[dogfood-results/dogfood-v0160-deep]]"
  - "[[iterations/done/iteration-144-index-suggestion-hint]]"
  - "[[iterations/done/iteration-145-unified-input-resolution]]"
  - "[[iterations/iteration-147-task-files-from]]"
---

# Dogfood v0.16.0 — iter-144 through iter-147

Binary: `hyalo 0.16.0 (670decd6db1c 2026-05-28)`. Tested against:

- **Own KB** — `hyalo-knowledgebase/` (~280 files)
- **MDN Web Docs** — `/Users/james/devel/mdn/files/en-us/` (14,375 files)
- **Synthetic scratch vaults** under `/tmp/hyalo-it147/` and
  `/tmp/hyalo-large/` for the iter-147 acceptance criteria and the
  iter-144 large-vault threshold (600-file vault).

(GitHub Docs lives at `/Users/james/devel/docs/content/` but iter-144
slow-query and iter-147 task plumbing don't need a third vault; VS
Code docs not present.)

## New feature verification

### iter-146 — git provenance in `--version` — WORKING

```
$ /Users/james/devel/hyalo/target/release/hyalo --version
hyalo 0.16.0 (670decd6db1c 2026-05-28) (kb dir: hyalo-knowledgebase)
$ git rev-parse --short=12 HEAD
670decd6db1c
$ git show -s --format=%cs HEAD
2026-05-28
```

Embedded SHA and commit date match the actual repo HEAD. `-V` (short
flag) prints the same banner. When invoked outside the repo (cwd
`/tmp`) the binary correctly omits the `(kb dir: ...)` suffix but keeps
the SHA + date — good degradation.

### iter-145 — unified input resolution + `--files-from` everywhere — MOSTLY WORKING

Single-file commands all accept `--files-from` with a single-entry
list:

- `task read` — OK (errors cleanly with `--files-from resolved to
  multiple files but this command accepts only one` on multi-entry)
- `backlinks` — OK
- `read` — OK

Multi-file commands (`set`, `remove`, `append`, `find`, `lint`) all
accept `--files-from` and surface the standard counters.

Inconsistencies:

- **Help text drift**: `set --help`, `remove --help`, `append --help`
  still document `--file` / `--glob` with the wording *"Mutually
  exclusive with --glob"* and don't mention `--files-from` in that
  conflict sentence. Compare `find --help` and `task toggle --help`
  which say *"Mutually exclusive with --glob and --files-from"*. The
  flag itself works on all three; only the help-text conflict list is
  stale. See [[NEW-4]] below.
- **Single-file error rendering**: `read --files-from -` with a
  multi-entry list emits `Error: ...` plain text on a TTY, while
  `task read` and `backlinks` emit the JSON `{error, hint}` envelope.
  With `--format json` all three render the JSON envelope, so this is
  cosmetic, not a contract break. Worth noting because `read` is the
  one most likely to be hand-piped.

### iter-144 — slow-query / large-vault index hint — PARTIAL

Slow-query hint fires on MDN, suppressed by `--index`, `--no-hints`,
`--quiet`:

```
$ hyalo --dir files/en-us find --property type=note --limit 1
note: --dir is redundant; .hyalo.toml already sets dir = "files/en-us"
{
  "hints": [
    { "cmd": "hyalo create-index",
      "description": "Command took 1189 ms. Create an index for faster queries:" }
  ],
  "results": [],
  "total": 0
}
$ hyalo --dir files/en-us find --index --property type=note --limit 1
…
"hints": []
$ hyalo --dir files/en-us --quiet find --property type=note --limit 1
…
"hints": []
```

Fast queries on own KB (~280 files, all under 100 ms) do NOT trigger
the hint — correct.

Large-vault summary hint **partially WORKING**: it fires on a 600-file
synthetic vault, but is missing on MDN where it would be most useful.
See [[NEW-1]] — it gets crowded out by `MAX_HINTS = 5` because the
five health hints (`properties`, `tags`, `orphan`, `broken-links`,
`links fix`) come first.

### iter-147 — `--files-from` on `task toggle` / `task set` — WORKING (6/6 ACs)

Verified ACs against `/tmp/hyalo-it147/` (3 fixture files with `## Tasks`
sections):

| AC | Command | Result |
|---|---|---|
| 1. `--all --files-from list.txt` | `task toggle --all --files-from list.txt` | toggles every task in every listed file; envelope is `results.files: [...]` |
| 2. `--section Tasks --files-from -` | stdin'd `a.md\nb.md` | scopes to the heading, mutates both files |
| 3. `--line N --files-from -` | clap rejects with exit 2 | `error: the argument '--line <LINE>' cannot be used with '--files-from <PATH\|->'` |
| 4. counter-aware hints | mixed `a.md\nmissing.md\n/etc/hosts\nnotmd.txt` | `files_missing=1`, `files_skipped_outside_vault=1`, `files_skipped_non_md=1`; both counter-aware hints surface |
| 5. empty input | `--all --files-from -</dev/null` | exit 0, empty `files: []` |
| 6. `--index --files-from` snapshot membership | created index then ran `--index --section Tasks --files-from -` with one in-index path + one not | only the in-index path mutated; `files_missing=1` counter fires; no disk fallback for the missing path |

Also passing:

- **Runtime guard**: `--files-from -` without `--all` or `--section`
  produces a friendly error: `"--files-from requires --all or
  --section"` with both example invocations in the hint.
- **`task set` mirrors `task toggle`** completely — `--status '-'
  --section Tasks --files-from -` works; `--line --files-from`
  rejects.
- **Mutual exclusion**: `--file a.md --all --files-from -` rejects;
  `--files-from a --files-from -` rejects.

## Bugs found

### NEW-1: large-vault index hint crowded out by MAX_HINTS on real large vaults — MEDIUM

The iter-144 large-vault summary hint fires on a 600-file synthetic
vault but **does NOT** fire on MDN (14,375 files), which is exactly the
case it was designed for.

Repro:

```
$ cd /Users/james/devel/mdn
$ hyalo --dir files/en-us summary --format json \
    | python3 -c "import json,sys; d=json.load(sys.stdin); \
                  print([h['cmd'] for h in d['hints']])"
['hyalo properties --dir files/en-us --format json',
 'hyalo tags --dir files/en-us --format json',
 'hyalo find --orphan --dir files/en-us --format json',
 'hyalo find --broken-links --dir files/en-us --format json',
 'hyalo links fix --dir files/en-us --format json']
```

No `create-index` hint. Compare a 600-file synthetic vault (no broken
links, no orphans):

```
$ hyalo --dir /tmp/hyalo-large summary --format json | jq '.hints[].cmd'
"hyalo properties --format json"
"hyalo tags --format json"
"hyalo find --orphan --format json"
"hyalo create-index"
```

Root cause: `hints_for_summary` (`crates/hyalo-cli/src/hints.rs:711`)
gates the large-vault hint on `hints.len() < MAX_HINTS`, where
`MAX_HINTS = 5`. MDN has 4,245 orphans, 49,933 broken links, and the
`links fix` hint — that's already 5 hints by the time the index hint
is considered. Push-order matters and the index hint loses.

**Impact**: medium. The hint exists, the threshold is correct, but
the hint is invisible on the only realistic vault size where its
~6× speedup matters (see perf table). Real users will keep paying
~1.2 s/query.

**Suggested fix**: either (a) prepend the index hint instead of
appending (it's the action with the largest payoff, so it deserves
priority over orphan/broken-link nags), or (b) raise `MAX_HINTS` to 6
in the summary path, or (c) gate the broken-links / orphan hints
below the create-index hint when both fire.

### NEW-2: `--section` substring matching on write-mode commands — by design (was filed HIGH, retracted)

**Retracted.** Originally filed as a HIGH silent-data-corruption bug:
`hyalo task toggle <file> --section Tasks` toggles tasks under a
heading like `# No Tasks heading` because "Tasks" is a substring of
"No Tasks heading".

This is documented intentional behavior, not a bug. From
[[decision-log#DEC for iter-36]]:

> Section substring default — `--section` changed from exact
> whole-string to case-insensitive substring (contains) matching. This
> is backwards-compatible in practice: any query that previously
> matched will still match (exact match is a subset of substring
> match). Power users can use `--section '~=/regex/'` for regex.

The motivating use case (headings with date/counter suffixes like
`## DEC-031: ... (2026-03-22)`, `## Tasks [4/4]`) makes substring the
right default for `read`/`find`. The argument that write-mode commands
deserve stricter matching is not made anywhere in iter-22, iter-36, or
the decision log — and the consistent design across read- and
write-mode is itself a feature.

If a user wants strict matching they can pin the level (`--section
'## Tasks'`) or use the regex form (`--section '~=/^Tasks$/'`). The
iter-147 `--files-from` plumbing inherits the same semantics — also
intentional.

Leaving this note in the report so the retraction is traceable. No
action needed.

### NEW-3: prior NEW-2 from `dogfood-v0160-deep.md` still open — MEDIUM

The multi-segment `--dir` prefix-strip in `--files-from` reported in
[[dogfood-v0160-deep#NEW-2]] is unchanged in this build.

```
$ cd /Users/james/devel/mdn
$ echo "files/en-us/games/index.md" \
    | hyalo --dir files/en-us find --files-from - --no-hints --format json
note: --dir is redundant; .hyalo.toml already sets dir = "files/en-us"
note: all --files-from entries were missing; if paths include the vault dir
      prefix (e.g. notes/foo.md), hyalo strips it automatically — check
      that the vault dir matches
{ "results": { "files": [], "files_missing": 1, ... } }
```

The hint message was slightly polished (no more `kb/notes` example —
now `notes/foo.md`) but the underlying behavior is unchanged. The
canonical `git diff --name-only | hyalo --files-from -` recipe still
fails on any repo whose vault is more than one directory deep.

This is the headline `--files-from` pipeline use case for MDN-shaped,
GitHub-Docs-shaped, and VS Code-Docs-shaped repos.

### NEW-4: `set` / `remove` / `append` help text mutual-exclusion list omits `--files-from` — LOW

```
$ hyalo set --help | grep -A1 "Target file"
          Target file(s) (repeatable). Mutually exclusive with --glob
```

vs `find`, `task toggle`, etc.:

```
          Target file(s) (relative to --dir) — flag form, repeatable.
          Mutually exclusive with --glob and --files-from
```

The flag itself works (mutual exclusion is enforced — clap errors on
`set --file a.md --files-from -`), only the help text on `--file` is
stale on the three `set`/`remove`/`append` subcommands. Per the
[[feedback_keep_docs_in_sync]] feedback, help texts should track
behavior.

### NEW-5: `hyalo summary` `dir` key is duplicated in the envelope — LOW

```
$ hyalo summary --format json | jq '{top: .dir, inner: .results.dir}'
{
  "top": "hyalo-knowledgebase",
  "inner": "hyalo-knowledgebase"
}
```

The `dir` field appears both at the top of the envelope and inside
`results.dir`. Other commands keep envelope-level metadata separate
from `results`. Minor, but it's the kind of duplication that bites
schema-checkers and confuses LLM consumers.

## Bug regression testing

Re-verified the seven NEW-* items from
[[dogfood-results/dogfood-v0160-deep]]:

| Prior bug | Status in v0.16.0 (this build) |
|---|---|
| NEW-1 — `item_pattern` reports only first violation | still open |
| NEW-2 — `--files-from` doesn't strip multi-segment `--dir` | **still open** — see [[NEW-3]] above |
| NEW-3 — `hyalo new --help` says "parent must exist" | not re-checked (text drift, not behavioral) |
| NEW-4 — `--files-from` doesn't trim leading whitespace | still open (`printf '  edge.md\n'` → files_missing=1) |
| NEW-5 — `create-index --index-file` silently ignored | still open |
| NEW-6 — `--files-from` doesn't dedupe input | still open |
| NEW-7 — most subcommands lack EXAMPLES in --help | partially addressed — `task toggle`, `task set` now have examples |

Spot-checked the always-zero-exit regression too:

```
$ hyalo lint --rule HYALO001 --format json >/dev/null; echo $?
1   # ← errors present
```

Still fixed.

## UX assessment

### Genuinely great

- **iter-147 runtime guard message**: when the user supplies
  `--files-from` without `--all` or `--section`, the error gives the
  cause, the constraint, and both working invocations in one block.
  Best-in-class error UX.
- **`--quiet` and `--no-hints`** both work end-to-end on the
  slow-query hint, as documented.
- **`--version` banner format** carries SHA + date + kb-dir on one
  line — easy for `support` triage, easy to parse with `awk '{print $3}'`
  (sha) or `grep -oE '\b[0-9a-f]{12}\b'`.
- **iter-147 envelope shape** is the right call — promoting to
  `results.files: [...]` when `--files-from` is used keeps the
  per-task structure flat and `jq`-friendly. The single-file path
  retains the object shape, so existing scripts don't break.

### Friction

- **`--allow-outside-vault` not available on `create-index`**: 
  `hyalo --dir files/en-us create-index -o /tmp/mdn.idx` errors with
  "output path is outside the vault boundary" and suggests
  `--allow-outside-vault`, but that flag isn't accepted on
  `create-index` (`unexpected argument`). The user has to drop the
  index file inside the vault dir, which is awkward for read-only
  external KBs like MDN.
- **MDN summary spam**: 5 hints (orphans 4245, broken-links 49933,
  links fix dry-run) for a docs vault hyalo doesn't author is noise.
  Combined with [[NEW-1]] the most useful hint (`create-index`) is
  invisible.
- **`set/remove/append --files-from` is undocumented in EXAMPLES**:
  none of these three subcommands show a `--files-from` invocation in
  their `EXAMPLES:` block, even though they all accept it now.

## Performance — v0.16.0

Wall clock, release build, warm cache, single run each. macOS, M-series.

| Vault | Files | Command | Time |
|---|---|---|---|
| own KB | ~280 | `find --limit 1` | 20 ms |
| own KB | ~280 | `summary` | 34 ms |
| own KB | ~280 | `find iteration` (BM25) | 97 ms |
| own KB | ~280 | `find --property status=completed` | 22 ms |
| own KB | ~280 | `lint` (full vault) | 84 ms |
| MDN | 14,375 | `find --limit 1` (no index) | 1.09 s |
| MDN | 14,375 | `summary` (no index) | 1.11 s |
| MDN | 14,375 | `find promise` BM25 (no index) | 3.83 s |
| MDN | 14,375 | `find --index --limit 1` | 0.66 s |
| MDN | 14,375 | `find --index promise` BM25 | 0.65 s |
| MDN | 14,375 | `summary --index` | 0.92 s |

No regressions vs the prior dogfood baseline ([[dogfood-v0160-deep]]).
Index speedup on BM25 ≈ 5.9×, consistent with the prior run.

## Suggested follow-ups

In rough priority order:

1. **NEW-3 (MEDIUM)** — strip multi-segment `--dir` prefix in
   `--files-from`. Still the biggest practical blocker for the
   marquee `git diff | hyalo` recipe on real doc repos.
2. **NEW-1 (MEDIUM)** — re-order summary hints so the iter-144
   large-vault index hint can fire on real large vaults, not just
   600-file synthetics. Either prepend or raise MAX_HINTS for summary.
3. **NEW-4 (LOW)** — sync `set` / `remove` / `append --file` help
   text to mention `--files-from` in the mutual-exclusion list.
4. **NEW-5 (LOW)** — drop the duplicated `dir` key from the summary
   envelope.
5. Accept `--allow-outside-vault` (or `-o`-outside-vault) on
   `create-index` so external read-only KBs can store the snapshot in
   `/tmp/` or `~/.cache/hyalo/`.

(NEW-2 retracted — see section above; substring `--section` is by
design.)

## What worked well

- **iter-147** plumbed cleanly through both subcommands. All six ACs
  pass on the first try, including the snapshot-membership path and
  the friendly runtime guard for missing `--all`/`--section`.
- **iter-146** version banner is exactly what was specified — SHA,
  date, kb dir, gracefully degrades outside a git context.
- **iter-145** unified resolver: the `task read` / `backlinks` /
  `read` error message *"--files-from resolved to multiple files but
  this command accepts only one"* is consistent across the three
  newly-extended single-file commands — the unification visibly paid
  off.
- **iter-144** slow-query hint is well-tuned. 500 ms threshold caught
  every realistic MDN query in this run without false positives on
  own-KB sub-100 ms queries.
