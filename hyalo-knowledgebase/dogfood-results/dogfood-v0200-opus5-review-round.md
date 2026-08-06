---
title: Dogfood v0.20.0 — symlink write-path bug, vault side effects, CLI surface drift
type: research
date: 2026-08-06
status: active
tags: [dogfooding, write-path, cli-surface, links]
related:
  - "[[dogfood-results/dogfood-v0200-slim-pre-release]]"
  - "[[reviews/codebase-review-2026-08-06]]"
---

# Dogfood v0.20.0 — Opus 5 review round

Binary: `target/release/hyalo` at `c42fa6f` (v0.20.0 released).
KBs exercised: own KB (347 files), MDN `../mdn/files/en-us` (14,375 md),
GitHub Docs `../docs/content` (3,710 md), plus purpose-built scratch vaults.
Run alongside [[reviews/codebase-review-2026-08-06]] — this session was aimed
at confirming review findings against real vaults, not re-walking iter-190.

## Bugs found

### BUG-1: `lint --fix` through a symlink destroys the link, fixes nothing, reports success (HIGH)

Extends the review's R1-1 from `task toggle` to the whole `--fix` path.

```bash
vault/alias.md -> real/note.md          # symlink
vault/real/note.md                       # "trailing spaces here   "

$ hyalo lint --fix --file alias.md
  {"errors": 0, "files": [{"file": "alias.md",
    "fixed_groups": [{"count": 1, "rule": "MD009", ...}]}]}   # reports fixed

$ ls -l vault/alias.md    ->  -rw-r--r--    # WAS a symlink, now a regular file
$ grep -c ' $' vault/real/note.md  ->  1    # real file STILL has trailing spaces
```

Three failures compounding:

1. the symlink is silently converted to a regular file
2. the real content is never fixed
3. it is reported as fixed, exit 0

Consequence beyond the review's framing: **`lint --fix` is non-idempotent
in the presence of symlinks.** Re-running keeps reporting MD009 on the real
file forever while the alias copy diverges further on every pass. A vault
with symlinked notes silently accumulates divergent duplicates.

Root cause is shared by every mutation path — `fs_util::atomic_write`'s
`persist` is a `rename(2)` onto the link itself. See R1-1.

### BUG-2: read-only commands bump the vault directory mtime (MEDIUM)

Confirmed on a fresh scratch vault, not just the own KB:

```bash
$ stat -f %m /tmp/dfprobe            ->  1786048122
$ hyalo --dir /tmp/dfprobe find --count   # read-only
$ stat -f %m /tmp/dfprobe            ->  1786048123   # changed
```

`CaseInsensitiveMode::Auto` (the default) probes by creating and deleting a
file in the vault root on every invocation. See R1-4 for the full analysis
including the orphan-litter and read-only-mount cases.

### BUG-3: `hyalo tags summary` emits a hint that does not run (MEDIUM)

```bash
$ hyalo tags summary --format text
  -> hyalo tags --limit 0 ...   # "Show all 203 tags (no limit)"

$ hyalo tags --limit 0
  error: unexpected argument '--limit' found
```

`hints.rs:1439` builds `["tags", "--limit", "0"]`, missing the `summary`
subcommand. `hyalo tags summary --limit 0` is the working form (returns 203).

Harvested and executed all 29 distinct hints the CLI emits across `find`,
`summary`, `tags`, `properties`, `types`, `views`, `lint`, `backlinks`,
`read`, `links fix`, `lint-rules` — **28/29 ran clean; this was the only
failure.** Hint quality is otherwise genuinely good.

The gate that should have caught it asserts on substrings
(`hints.rs:4029`: `show_all.cmd.contains("--limit 0")`) — which passes on
the broken command. There is no execution-based hint gate anywhere in
`tests/e2e/hints.rs`.

### BUG-4: `hyalo config` breaks the JSON envelope contract three ways (MEDIUM)

The top-level help states "All JSON is wrapped in a consistent envelope:
`{"results": ..., "total": N, "hints": [...]}`" and "hints is always
present". `config` is the sole violator:

```bash
$ hyalo config --format json
{"config_path": ..., "dir": ..., "hints": true, "format": null, ...}
#  no "results"      no "total"      hints is a BOOL, not an array
```

1. **No envelope.** Bare flat object.
2. **`hints` is a `bool`** (the config setting) where every other command
   has an array. A script doing `.hints | length` gets a type error only on
   `config`.
3. **`--jq` is silently ignored.** `hyalo config --jq '.results.dir'` and
   `hyalo config --jq '.dir'` both print the full unfiltered object — no
   filtering, no error. The `--jq` help says "any command".

Silent is the bad part; an explicit "not supported for config" would be
fine. Note `--count` on `config` *does* error correctly, so the two global
flags disagree with each other about whether `config` is special.

## UX issues

### UX-1: out-of-vault link targets are counted as broken with no distinct bucket (MEDIUM)

GitHub Docs (`--dir ../docs/content`, a subdirectory of a larger repo)
reports **6,568 broken of 14,167 links — 46%**. Sampling shows most are not
hyalo resolution failures but targets that legitimately live outside the
scanned directory:

```text
target='/src/frame/lib/frontmatter.ts'        path=None   # repo source, outside content/
target='../contributing/redirects.md'         path=None   # sibling dir, outside content/
```

These land in `unfixable` (1,655) alongside genuinely-broken intra-vault
links, and inflate the headline `broken` count that `summary` prints. Any
vault that is a subdirectory of a bigger repo — a very common shape — gets a
broken-link number that is not actionable. A distinct `out_of_vault` bucket
(the same treatment iter-184 gave broken anchors) would make the headline
count mean something again.

Confirmed not a site-prefix misconfiguration: `--site-prefix ''` yields the
identical 6,568.

## Regression testing

Re-verified from [[dogfood-results/dogfood-v0200-slim-pre-release]]:

- **Anchor validation / index parity (iter-190)** — STILL FIXED
- **`links fix` bucket accounting (iter-184 lesson)** — STILL CORRECT, and
  worth calling out as the session's best result. On a controlled vault with
  one fuzzy-fixable, one case-mismatch, and one unfixable link:
  `broken: 2, fixable: 0, fuzzy: 1, case_mismatches: 1, unfixable: 1` —
  fuzzy correctly excluded from `fixable`, `--apply` applied only the
  case-mismatch and left the fuzzy link alone, and the hint correctly
  pointed at `--apply-fuzzy`. The bucket discipline established in iter-184
  is holding.

## What worked well

- **Hint accuracy: 28/29 executable.** For a surface this large that is a
  strong result.
- **Unicode and nested frontmatter.** `title: "日本語 — ünïcode ✓"`,
  `tags: [tëst, 日本]`, two-level nested objects, and an empty value all
  round-tripped correctly (`empty: null`, nested structure preserved).
- **`links fix` restraint** — see regression note above.
- **Performance is healthy** and unchanged from prior baselines.

## Performance

| KB | files | command | wall |
|---|---|---|---|
| MDN | 14,375 | `find --limit 1 --count` | 0.91 s |
| MDN | 14,375 | `summary` | 1.28 s |
| MDN | 14,375 | `find "service worker cache"` (BM25) | 3.90 s |
| GitHub Docs | 3,710 | `summary` | 0.40 s |
| GitHub Docs | 3,710 | `lint` (2,151 violations) | 0.70 s |

No regressions. BM25 at 3.9 s on 14 K files is the slowest path but is
consistent with prior reports.

## Verdict

Ship-blocking: **BUG-1**. It is silent data divergence on a common vault
shape (symlinked notes), it affects every mutation command, and `lint --fix`
makes it recurring rather than one-shot. The rest are quality items.
