---
title: "Dogfood v0.20.0 — arch refactors (225/226), agent-CLI follow-ups (238), links perf (206) verified; index upsert gap"
type: research
date: 2026-08-27
status: active
tags: [dogfooding, index, hints, links, lint, performance]
related:
  - "[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]]"
  - "[[iterations/iteration-206-links-perf-profiling]]"
  - "[[iterations/iteration-225-arch-thin-dispatch-typed-hints-core-facade]]"
  - "[[iterations/iteration-226-arch-lint-crate-index-journal]]"
  - "[[iterations/iteration-234-lint-dead-output-cleanup]]"
  - "[[iterations/iteration-237-pi-package-distribution]]"
  - "[[iterations/iteration-238-agent-cli-followups]]"
  - "[[iterations/iteration-240-review-followups-bugfixes]]"
---

# Dogfood v0.20.0 — arch refactors, agent-CLI follow-ups, links perf

Binary: `hyalo 0.20.0 (56e0fee7a231 2026-08-27)`, built from `main` after
PRs #269–#274. Scope: the six iterations merged since
[[dogfood-results/dogfood-v0210-pre3-fix-waves-207-214]] plus regression of
that report's bug list.

KBs: own KB (414 files, scratch copies for every mutation), GitHub Docs
(`../docs/content`, 3,710 files), MDN (`../mdn/files/en-us`, 14,375 files).
VS Code docs were not present on disk.

Headline: the two large refactors (225/226) are behaviour-neutral as claimed —
output, hints, exit codes and the lint envelope are unchanged, and every
mutation path keeps a snapshot index consistent **for files the index already
knows**. The one real gap is that no mutating command except `new` inserts an
entry for a file the index has never seen (BUG-1). Iter-206 delivers a 3×
`links` speed-up on GitHub Docs and 4× on MDN without a prefix.

## New Feature Verification

### iter-226 — lint crate boundary + MutationJournal — WORKING (with one gap)

Method: fresh scratch copy, `create-index`, then mutate through every write
path with `--index`, and after each step diff `--index` vs disk-scan JSON for
ten queries (`find --tag`, `find --property`, `find --broken-links`,
`backlinks` of an affected target, `find --orphan`, `summary`, `find --file
--fields tasks`, BM25).

| Mutation (`--index`) | entries current | link graph current |
|---|---|---|
| `tags rename` (17 and 79 files) | yes | yes (backlinks 15 = 15) |
| `set --property`, `set --tag` | yes | yes |
| `task toggle --all` | yes | yes |
| `mv` with 4 inbound links | yes | yes (4 = 4, see BUG-5 for ordering) |
| `links fix --apply --apply-fuzzy` (3 broken links) | yes | yes (target backlinks 16 = 16) |
| `lint --fix` | yes | yes |
| `append`/`remove --property` | yes | yes |
| `new --type iteration` | yes (upserted) | yes |
| any of the above on a file **absent** from the index | **no** — see BUG-1 | **no** |

Lint relocation: `lint`, `lint --fix`, `lint --fix --dry-run`, `lint --rule`,
`lint --rule-prefix`, `lint --strict`, `lint --detailed`, single-file lint,
`lint-rules list/show` all produce the same text and the same JSON keys as in
the pre3 report. `lint --rule-prefix ZZZ` now errors (`no rule matches
prefix: ZZZ`) instead of silently running every rule (pre2 BUG-5 fixed).

### iter-225 — thin dispatch, typed hints, core façade — WORKING

- Hints collected in text and JSON for 19 commands. Every `=>` text hint
  carries `[writes]` and its JSON twin has `"writes": true`; every `->` hint
  has `"writes": false`. Format flags propagate (`--format text` in text
  hints, `--format json` in JSON hints). Verified pairs: `lint-rules remove`,
  `views remove`, `views set`, `new --type`, `task toggle`.
- Executed **every read-only hint** emitted by 15 commands verbatim
  (23 distinct commands): all exit 0. No drift found.
- Error paths remain typed and helpful: unknown sort field lists valid values,
  `set` without changes shows an example, out-of-vault path names the vault,
  `lint --rule NOPE` / `types show nope` point at the list command, `--limit
  -1` gets clap's `-- -1` tip, bad `--jq` reports the parse error, `links fix
  --apply --dry-run` is a clap conflict.

### iter-206 — links fix fuzzy-pass perf — WORKING

| KB | `links` / `links fix --dry-run` | pre3 |
|---|---|---|
| GitHub Docs (6,099 broken) | 4.50 s / 4.27 s | 12.4–14.7 s |
| MDN, `--site-prefix en-US/docs` (510 broken) | 5.78 s | 7.8 s |
| MDN, derived prefix (49,772 broken) | 28.4 s | 110 s |
| own KB | 0.07 s | 0.05 s |

Suggestions are sensible where they are confident (`iteration-206-links-perf-profilin`
→ `…profiling.md` 0.996; MDN `Global_attributes/tabindex` → `global_attributes/index.md`
0.912) and correctly flagged below floor where they are garbage (`/contributing/redirects.md`
→ `rest/pulls/reviews.md` 0.0). The GH Docs `basename-fallback 0.86` relocations
for moved how-tos are exactly the fixes a maintainer would want.

### iter-238 — `--filenames0`, `--iteration` on read/task — WORKING

- `find … --filenames0 | tr '\0' '\n'`: NUL bytes confirmed with `od -c`;
  no trailing newline; no hints leak into the stream; `--limit 0` and zero
  matches work; `| xargs -0 stat` works on GH Docs.
- `--filenames0 --format json` → typed error with a hint; `--filenames0
  --filenames-only` → clap conflict. Both correct.
- `read --iteration 238`, `--section`, `--format json`, `task toggle
  --iteration 238 --all --dry-run`, `backlinks --iteration 238`, `set
  --iteration` all resolve. Passing both `[FILE]` and `--iteration` is a clap
  conflict.
- Nonexistent (`9999`) → `no file found for iteration 9999 (resolved globs:
  …)` exit 1, with a `find --iteration` hint. Vault without an `{n}` template
  (GH Docs) → clear error naming `filename_template` and how to set it.
- `--iteration abc` → wrong message (BUG-3); `--iteration 2` cannot reach
  `iterations/done/iteration-02-links.md` (UX-2).

### iter-234 — dead LintOutput removal — WORKING

`lint` JSON envelope: `{hints, results, total}`; `results` keys
`dry_run, errors, files, files_checked, files_ignored, files_truncated,
files_with_violations, rules_fired, violations, warnings`; `--fix` adds
`remaining_*`, `total_fixed`, `total_remaining`, `total_conflicts`. Text
summary line unchanged. `summary.results.schema` now uses
`files_with_violations` (D-5 rename landed via iter-238).

### iter-237 — pi package distribution — skimmed

User-visible: README gains an "Install the pi integration" section
(`pi install git:github.com/ractive/hyalo`, `pi update --extensions`),
`pi-package/README.md` documents the ≥0.21 requirement for typed tools, and
`hyalo init --pi` is positioned as the vendored fallback. Nothing in the CLI
surface changed; `hyalo init --help` reads correctly. Not exercised against a
live pi install (see [[iterations/iteration-239-pi-install-verification]]).

## Bug Regression Testing (vs pre3 / pre2 reports)

| Bug | Result | Evidence |
|---|---|---|
| pre3 NEW-5 `append`/`remove` re-serialize lists | **FIXED** | append `related=[[foo]]` then remove → file byte-identical to original |
| pre3 NEW-6 `lint --fix` counters over truncated listing | **FIXED** | GH Docs `total_fixed 1618 / remaining 6109 / conflicts 12` identical with default (50 files) and `--limit 0` (2,050 files) |
| pre3 NEW-8 frontmatter ~8 KiB scalar limit | **FIXED** | 20,000-char scalar parses and lints |
| pre3 NEW-9 derived `site_prefix` misfires on MDN | **PARTIAL** | still derives `en-us` and finds 49,772 false broken links, but now emits `warning: site_prefix 'en-us' stripped 0 of 49753 site-absolute link(s)…` and `hyalo config` shows `site_prefix: en-us (derived)` with a note; runtime 110 s → 28 s |
| pre3 NEW-15 `summary` 0 broken vs `find --broken-links` 3 | **FIXED** | summary now reports `0 broken, 5 broken anchors`; the 3 files are anchor-only |
| pre2 BUG-5 unmatched `--rule-prefix` runs everything | **FIXED** | `Error: no rule matches prefix: ZZZ` |
| pre3 NEW-1/2/3/4/7/10–14, UX-1–4 | not re-tested | out of this session's scope (links auto / zone scan / MD010 columns) |

## Bugs Found

### BUG-1: `--index` mutations never insert an entry for a file the index does not know (MEDIUM-HIGH) — **FIXED in iter-243**

Repro (scratch copy of own KB):

```bash
hyalo create-index
printf -- '---\ntitle: ext\ntype: research\ndate: 2026-08-27\nstatus: active\n---\n\n# ext\n\n- [ ] t\n\nSee [[decision-log]].\n' > hyalo-knowledgebase/ext-set.md
hyalo set ext-set.md --property status=done --index          # → modified: ["ext-set.md"]
hyalo find --file ext-set.md --index --format json | jq .total  # → 0   (disk scan: 1)
hyalo backlinks decision-log.md --index                        # ext-set.md absent
```

Same for `lint --fix <file> --index` (reports `total_fixed: 2`), `task
toggle <file> --all --index` (toggles 1), `append <file> --property … --index`
(reports modified). Only `new --type … --index` upserts. Expected per `--index`
help: "Mutation commands patch the index in-place after each write — keeping
the index current." Actual: the journal refreshes existing entries only; a
file created by an editor/Obsidian and then touched with hyalo stays invisible
to every indexed read (`find`, `backlinks`, `summary`, `--broken-links`) and
its outgoing links never enter the graph. Workaround: `create-index` again.
This is the iter-226 ARCH-3 territory — the journal knows the rel_path and
that the write succeeded; an upsert on miss closes it.

### BUG-2: `links fix --apply --index` trusts a stale index silently (MEDIUM) — **FIXED in iter-243**

Append a broken `[[…]]` to an indexed file with an editor, then
`hyalo links fix --apply --apply-fuzzy --index` → `broken: 0`, `applied:
true`, file untouched, no warning. Without `--index` the same command fixes 3
links. The mtime fallback documented for mutation commands does not cover the
broken-link discovery pass. A cheap mitigation: mtime-check the entries before
the discovery pass, or at least print the "index may be stale" warning that
read-only commands already have for incompatible snapshots. Related cosmetic:
`applied: true` / `Applied: yes` is emitted even when `fixes: 0` — it means
"apply mode", not "something was applied", which is exactly what an agent
will misread.

### BUG-3: `--iteration abc` says the ID is empty (LOW) — **CLOSED BY REMOVAL** (`--iteration` deleted in iter-242 / DEC-242)

`hyalo read --iteration abc` → `iteration ID is empty (expected digits
optionally followed by letters, e.g. 206, 01, 16b)`. The ID is not empty; the
validator strips non-digits and reports on the remainder. Same message from
`find --iteration abc` and `set --iteration abc`. Say `invalid iteration ID
"abc"`.

### BUG-4: BM25 scores differ between `--index` and disk scan (LOW) — **PARTIALLY FIXED in iter-243** (fresh-index parity fixed via `on_raw_body_line` + TOKENIZER_VERSION 3; post-mutation drift timeboxed-out: the persisted inverted index cannot be incrementally updated because per-entry tokens are stripped once it exists — rebuild with `create-index` for exact parity)

`hyalo find dogfood --limit 5 --index` vs without: same ranking, scores differ
in the 4th decimal (`1.33928` vs `1.33902`) on a fresh index, drifting further
after mutations (`1.34366`). Some corpus statistic (avg doc length or token
count) is computed differently on the two paths. Harmless today; can flip
ties, and makes indexed-vs-disk output non-diffable.

### BUG-5: `backlinks` order differs between `--index` and disk after a refresh (LOW) — **FIXED in iter-243** (sorted by `(source, line)` on both paths)

After `tags rename --index` touches a linking file, that file's entries move to
a different position in `backlinks <target> --index` than in the disk scan
(counts equal). Sort backlinks by `(source, line)` on both paths so the
outputs are diffable and stable.

## UX Issues

### UX-1: `--file` does not glob, but error hints say it does (MEDIUM-LOW)

`hyalo set nonexistent.md …` → `hint: paths are vault-relative; run "hyalo
find --file <glob>" to locate it`. But `hyalo find --file 'iterations/iteration-23*'`
→ `file not found`; `--glob` is the globbing flag (6 matches). Either make
`--file` accept globs or fix every hint that says `--file <glob>`.

### UX-2: `--iteration` cannot address zero-padded or archived iterations (LOW)

`--iteration 2` expands to `iterations/iteration-2-*.md`; the file is
`iterations/done/iteration-02-links.md`. The template is the contract, so this
is by design, but 60+ historical iterations in this very vault are
unreachable by natural key. Consider matching `{n}` zero-padded and searching
the template's directory recursively, or document the limitation in the flag help.

### UX-3: nested YAML property paths silently return nothing (LOW-MEDIUM)

GH Docs: `versions` is `{fpt: "*", ghes: "*", …}` on 3,707 files.
`find --property 'versions.fpt=*'` → `No results` (also `versions.fpt`,
`versions.ghes>=3.10`). No error, no hint that dotted paths are unsupported.
`versions~=fpt` (regex over the serialized map) works as a workaround (2,461)
but that is not discoverable. Either support dot-paths or reject them with a
hint pointing at `~=`.

### UX-4: default `lint` text output hides the errors behind the truncation (LOW-MEDIUM)

GH Docs: `3710 files checked, 2050 with issues (4 errors, 7716 warnings)` —
but the 50 files shown contain 0 errors; the 4 errors only appear with
`--limit 0`. When errors exist and are truncated away, list them first or add a
`-> hyalo lint --severity error` style hint.

### UX-5: `read --iteration` on a body-less file prints nothing (LOW)

`hyalo new --type iteration --file … --index` then `hyalo read --iteration
900` → empty stdout, exit 0. Correct, but indistinguishable from "nothing
resolved" in a pipeline; `--frontmatter` would help and the hint list already
suggests it for `read <file>`.

### UX-6: MDN with the correct prefix reports 49,262 "case mismatches" (LOW)

`links fix --dry-run --site-prefix en-US/docs` on MDN: every `/en-US/docs/Web/API/…`
link resolves only through case folding because MDN's directories are lowercase.
They are counted as fixable case mismatches, which would rewrite ~50k links in
canonical MDN casing if applied. A `links.case_insensitive = true` (or
`--case-insensitive`) that treats these as resolved, not fixable, would make
MDN usable.

## What Worked Well

- **Refactors are truly neutral.** 2,100-line dispatch → handlers and a 4,375
  -line lint module moving crates, and not a single byte of text output, JSON
  key, hint, or exit code changed across 19 commands and three KBs.
- **Index consistency for known files is solid.** Eight mutation paths, ten
  queries each, entries and link graph current after every step — the
  MutationJournal does what iter-226 promised for the files it tracks.
- **Hints are trustworthy now.** Every read-only hint runs; `=>`/`[writes]`
  and `"writes": true` agree everywhere; the `views set` hint after a filtered
  `find` is the kind of drill-down that saves a lookup.
- **iter-206** is a real win: GH Docs `links` 12.4 s → 4.5 s, MDN misderived
  prefix 110 s → 28 s, with byte-identical suggestions.
- **Error messages** consistently name the fix (`--limit -1` tip, `append
  --tag` → "use hyalo set", vault-boundary message names the vault, missing
  `{n}` template shows the `types set` command).
- `--filenames0` is exactly right: raw bytes, no hints leaking, typed refusal
  with `--format json`.
- NEW-5/6/8/15 from pre3 are all fixed; `config` now says `(derived)` for the
  MDN prefix and explains the single-segment rule.

## Performance

Wall-clock, warm cache, Apple Silicon; `--format json --no-hints`.

| Command | Own KB (414) | GH Docs (3,710) | MDN (14,375) | MDN `--index` | pre3 |
|---|---|---|---|---|---|
| `find --limit 1` | 0.05 s | 0.26 s | 1.26 s | 0.42 s | flat |
| BM25 query | 0.03 s | 1.19 s | 4.21 s | 0.42 s (10×) | flat |
| `summary` | 0.08 s | 0.51 s | 1.33 s | 0.72 s | flat |
| `find --property k=v --limit 0` | 0.07 s | 0.21 s | 1.07 s | 0.45 s | flat |
| `lint` | 0.12 s | 0.93 s | 2.93 s | — | flat |
| `lint --fix --dry-run` | — | 1.68 s | — | — | flat |
| `find --broken-links --limit 0` | 0.06 s | 0.42 s | 1.54 s | — | flat |
| `links` | 0.07 s | **4.50 s** | — | — | 12.4–14.7 s |
| `links fix --dry-run` | 0.07 s | **4.27 s** | **5.78 s** (prefix) / **28.4 s** (derived) | — | 7.8 s / 110 s |
| `create-index` | — | — | 2.84 s, 120 MB | — | 2.8 s / 115 MB |

No regressions > 2×. Index build on the own KB: 6 MB for 414 files.

## Recommendation

Fix BUG-1 (upsert on miss in the journal) and add the stale-index warning to
`links fix --apply --index` (BUG-2) before advertising `--index` for agent
loops that mix editor writes with hyalo writes — that is the ralph-loop
scenario. BUG-3 and UX-1 are one-line message fixes. UX-3 (dot-paths) is the
only item that is a feature.
