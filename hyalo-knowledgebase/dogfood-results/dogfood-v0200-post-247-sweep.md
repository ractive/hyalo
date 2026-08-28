---
title: "Dogfood v0.20.0 — post-247 sweep: stale-index, dot-path arrays, module splits, surface pruning"
type: research
date: 2026-08-28
status: active
tags: [dogfooding, index, dot-path, lint, links, performance, release-readiness]
related:
  - "[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]"
  - "[[iterations/iteration-241-stale-index-detection-and-ux-fixes]]"
  - "[[iterations/iteration-242-remove-iteration-flag]]"
  - "[[iterations/iteration-243-index-parity-bugfixes]]"
  - "[[iterations/iteration-244-index-remaining-deferrals]]"
  - "[[iterations/iteration-245-deferral-carryovers]]"
  - "[[iterations/iteration-246-help-coherence-review-followups]]"
  - "[[iterations/iteration-247-carry-over-sweep]]"
  - "[[iterations/iteration-248-remove-strict-index]]"
  - "[[reviews/deep-review-2026-08-27]]"
---

# Dogfood v0.20.0 — post-247 sweep

Binary: `hyalo 0.20.0 (f297f22760ec 2026-08-28)`, built from `main` after
PRs #277–#286 (iterations 241–248). Baseline for regressions:
[[dogfood-results/dogfood-v0200-arch-refactors-and-agent-cli-followups]]
(2026-08-27).

KBs: own KB (425 files; every mutation ran on a scratch copy with its own
`.hyalo.toml`), GitHub Docs (`../docs/content`, 3,710 files, nested YAML —
two scratch copies for `links fix --apply`), MDN (`../mdn/files/en-us`,
14,375 files, fresh snapshot index). `../vscode` is not on disk — skipped.

Headline: every owner priority checks out. Stale snapshots warn and still
serve, `--strict-index` / `--iteration` / `--changed-since` are gone, dot-path
filters return correct counts on real GitHub Docs frontmatter (verified
against `grep`), `summary` stdout is clean data, and the module split is
behaviour-neutral across hints, lint and find. Two things worth a fix before
a release: `task toggle --index` reintroduces BM25 score drift (BUG-1, LOW,
partial regression of the iter-244 parity AC), and the stale-index probe is
blind to any vault whose notes live two or more directories deep — which is
every file in MDN and GitHub Docs (UX-1, MEDIUM, documented but easy to miss).

## New Feature Verification

### Stale-index warn-but-serve, `--strict-index` removed (iter-241/247/248) — WORKING

- Scratch own KB: `create-index`, wait 2 s, create `research/new-note-dogfood.md`
  with an editor, then `find --file research/new-note-dogfood.md --index
  --format json` → stderr `warning: index older than vault; results may be
  stale — re-run create-index`, stdout is the (stale, empty) result, exit 0.
  Same warning from `find --format text`, `summary --index`, `backlinks
  --index`. `--quiet` suppresses it; JSON stdout stays pure.
- `hyalo --help` and every subcommand help: zero matches for `strict-index`.
  `hyalo find --strict-index` → clap "unexpected argument".
- The probe is shallow directory mtimes only. An in-place append
  (`printf >> file`) to an indexed note produced **no** warning; a new file at
  `iterations/done/…` (depth 2) or MDN `web/css/zz/index.md` (depth 3)
  produced **no** warning. `create-index --help` documents exactly this
  ("misses in-place edits of existing notes and changes more than one level
  deep"). See UX-1 for why that matters on the external vaults.
- The mutation-side heal is stronger than the read-side probe: `links fix
  --apply --apply-fuzzy --index` on GH Docs after an editor edit three
  directories deep printed `warning: index is stale: 1 file changed on disk
  since create-index (e.g. actions/get-started/understand-github-actions.md);
  refreshing from disk and persisting for this run` and fixed the typo link
  (`quickstartt` → `quickstart`). `links auto --dry-run --index` likewise
  reported `index is missing 1 file created outside hyalo … adding file from
  disk` for the depth-2 file the read probe had missed.
- No index present: `find --index` → `warning: failed to load index …;
  falling back to disk scan`, results served. Good.

### `--iteration` removed (iter-242) — WORKING

`find`, `read`, `set`, `task`, `backlinks`: `--iteration 206` → clap
"unexpected argument" on all five; `--help` of each contains no `--iteration`.
The replacement is taught in `find --help` (SEQUENCE-KEYED FILES block, glob
example `**/iteration-02-*.md`) and works: `find --glob 'iterations/iteration-248-*'
--filenames-only` resolves the file.

### Dot-path filters over nested maps and arrays of maps (iter-244/245) — WORKING

GitHub Docs, disk scan, with `grep` ground truth where it is well-defined:

| Filter | hyalo | ground truth |
|---|---|---|
| `versions.fpt=*` (nested map) | 2461 | 2460 `^  fpt:` + 1 inline form |
| `versions.ghec=*` | 2550 | 2550 |
| `versions.ghes=*` | 2155 | 2155 with literal `'*'` (67 carry `>=3.x`) |
| `versions.ghes~=/^>/` | 61 | — |
| `!versions.ghec` | 1160 | 1157 + 3 files with no `versions` at all |
| `journeyTracks.id=ado_migration` (array of maps, auto-descent) | 1 | 1 |
| `journeyTracks.id=getting_started` | 2 | 2 |
| `journeyTracks.0.id=getting_started` (index segment) | 2 | 2 |
| `journeyTracks.1.id=getting_started` | 0 | 0 (track 1 has another id) |
| `journeyTracks.99.id=…`, `journeyTracks.-1.id=…` | 0 | out of range, silent |
| `journeyTracks.guides.href=/migrations/ado/manage-access` (array → array of maps, two levels) | 1 | 1 |
| `journeyTracks.0.guides.1.href=…` | 1 | 1 |
| `journeyTracks.id` (exists) | 9 | 9 (the 2 other `journeyTracks:` hits are body code samples) |
| `introLinks.overview=/actions/get-started/…`, `sidebarLink.text=Get started`, `carousels.recommended.0=/actions/get-started/quickstart` | 1 each | 1 each |
| `versions.` | warns `no files matched --property "versions."; did you mean: versions?` | good |
| `.fpt=*`, `versions..fpt=*` | 0, silent | acceptable |

Every filter above returns the same count through `--index` on a GH Docs
snapshot, and `set --property reviewed=yes --where-property
'journeyTracks.id=ado_migration' --glob '**/*.md' --dry-run` selects exactly
`migrations/ado/index.md`. MDN: `spec-urls.0~=w3c` (53), `status.0=experimental`
(1321) identical on both paths. The `--property` short help and the FILTERS
block describe the semantics accurately (`contacts.0.email`, "any element").

### `summary` banner on stderr (iter-247, DEC-247) — WORKING

`hyalo summary --format text >out 2>err`: `out` starts with `Files: 421`,
`err` is exactly `note: kb dir: hyalo-knowledgebase`. JSON mode prints
nothing on stderr. `summary --index` on MDN: stdout starts with `Files: 14375`.

### Module split hints.rs / lint.rs / output.rs (iter-247) — WORKING, no drift found

- `lint` JSON envelope `{hints, results, total}`; `results` keys `dry_run,
  errors, files, files_checked, files_ignored, files_truncated,
  files_with_violations, rules_fired, violations, warnings`; `--fix` adds
  `remaining_errors, remaining_warnings, total_conflicts, total_fixed,
  total_remaining` — identical to the baseline list. Text summary line
  unchanged (`101 files checked, no issues (320 ignored by [lint] ignore)`).
  `lint --rule-prefix ZZZ` still errors `no rule matches prefix: ZZZ`.
- Hints: `->`/`=>` markers and `[writes]` tags agree with JSON `writes`
  (`lint-rules show MD013` pair verified). Harvested every read-only hint
  from 16 commands (16 distinct commands after dedup), executed each
  verbatim: 0 failures.
- `find` text/JSON shape unchanged: text lists `properties:`/`tags:`/
  `sections:`/`score:` blocks plus three per-file hints; the `views set`
  drill-down still appears after a filtered find.
- `reviews/` is linted now: `types show review` lists the schema, `lint
  --strict --glob 'reviews/**'` → `8 files checked, no issues`; vault-wide
  `lint --strict` is clean.
- `find --changed-since HEAD` → clap "unexpected argument" (DEC-246).

### Help coherence + `read` UTF-8 diagnostic (iter-246) — WORKING

- COMMAND REFERENCE: `hyalo summary [-g/--glob G] [-n/--recent N] [--depth N]`
  (no `--limit`; `summary --limit 5` is a parse error as the reference now
  implies), `changelog add --category CAT --message TEXT`, `okf log --message
  TEXT [TARGET] [--apply]`, `links fix [--apply] [--apply-fuzzy]
  [--min-confidence F] [--case-insensitive] …`, `--language` present in
  `find --help` (4 mentions).
- File with a `\xff` body byte: `read` prints `<line skipped: invalid UTF-8
  (lossy in search; fix encoding to read)>` for that line only, no "per-line
  limit" text, JSON `content` carries the same placeholder. But see UX-3 —
  the placeholder's claim about search is not true.

### `links fix --case-insensitive` (iter-244, UX-6) — WORKING

MDN with `--site-prefix en-US/docs`: `case_mismatches` 49,262 → 0 with the
flag, `broken` stays 510 / `unfixable` 509 / `fuzzy` 1, and the run is
faster (5.81 s → 4.95 s). Text summary no longer offers a ~50k-line
rewrite plan.

## Bug Regression Testing (vs the 2026-08-27 baseline)

| Bug | Result | Evidence |
|---|---|---|
| BUG-1 `--index` mutations never insert unknown files | **STILL FIXED** | fresh index, then editor-created `research/qqc.md` + `set --index` → `find qqc --index` 1 = disk 1, BM25 identical; same for `append --index` and `mv --index` on unknown files |
| BUG-2 `links fix --apply --index` trusts stale index | **STILL FIXED** | GH Docs scratch: editor-appended broken link 3 dirs deep discovered, warned, fixed; `fixes: 0` prints `Applied: no (no fixes written — nothing to apply)` |
| BUG-3 `--iteration abc` message | closed by removal | flag rejected everywhere |
| BUG-4 BM25 index/disk score drift | **PARTIALLY FIXED** | fresh index: identical for 3 queries × own KB and MDN top-20. After `set`, `set --tag`, `append`, `mv`, `lint --fix` (violation already indexed): identical. After `task toggle --all --index`: scores differ (see BUG-1 below) |
| BUG-5 backlinks order index vs disk | **STILL FIXED** | `backlinks decisions/decision-log.md --index` byte-identical to disk after a 4-step mutation wave (45 entries) |
| UX-1 `--file <glob>` hint lie | **FIXED** | `set nonexistent.md …` → `run "hyalo find --glob <glob>"`, and the glob works |
| UX-2 `--iteration` zero-padding | moot | flag removed; glob form documented |
| UX-3 dot-paths silent | **FIXED** | table above |
| UX-4 lint hides errors behind truncation | **FIXED** | scratch KB with 4 SCHEMA errors in one file: `lint --limit 1` shows that file first |
| UX-5 `read --iteration` body-less | moot | flag removed |
| UX-6 MDN case mismatches | **FIXED** | `--case-insensitive` → 0 |

## Bugs Found

### BUG-1: `task toggle --index` leaves BM25 corpus statistics out of sync (LOW)

Repro (scratch copy of this vault, `F=iterations/iteration-246-help-coherence-review-followups.md`):

```bash
hyalo create-index
hyalo task toggle $F --all --index
hyalo find "stale index" --index --limit 0 --format json | jq '.results[]|select(.file=="'$F'").score'   # 3.8541052083618856
hyalo find "stale index" --limit 0 --format json          | jq '.results[]|select(.file=="'$F'").score'   # 3.8391914279589754
```

Same for `read_line_capped` (5.9733 vs 5.9500). Hit counts and ranking were
identical in every query I tried; only the scores drift, in the 2nd–3rd
decimal. Control: the same file after `set --property … --index`, `set --tag
… --index`, or `lint --fix … --index` on a pre-indexed violation is
byte-identical to the disk scan, so the `update_task` path is the one that
skips (or double-counts in) the incremental corpus-statistic maintenance
from iter-244. Also reproduced with `lint --fix --index` when the file had
been appended to by an editor after `create-index` (the rescan-on-write
path). Impact: `--index` output is not diffable against disk after a task
toggle; ties can flip. The iter-244 AC "scores byte-identical after a
mutating wave" holds only for waves without `task toggle`.

### BUG-2: a no-op `set --index` does not refresh an entry the disk has changed (LOW)

```bash
hyalo create-index
printf '\nzzqqx unique token.\n' >> iterations/iteration-245-deferral-carryovers.md
hyalo set iterations/iteration-245-deferral-carryovers.md --property status=completed --index   # already completed → "0/1 modified"
hyalo find zzqqx --index --count   # 0
hyalo find zzqqx --count           # 1
```

The journal refreshes only after a write, and a no-op is not a write, so the
editor change stays invisible and no warning is printed (in-place edits do
not trip the directory-mtime probe). Documented behaviour by the letter of
`create-index --help`, but an agent that runs `set --index` specifically to
"touch" a file into the index gets nothing. Cheap fix: when `set`/`append`/
`task` resolve a target file and find it unmodified, still mtime-compare it
with the index entry and rescan on drift — the machinery exists for `links
fix` (DEC-241).

## UX Issues

### UX-1: the stale-index probe never fires on nested vaults (MEDIUM)

The probe compares *top-level* directory mtimes. MDN has zero notes at depth
1 (`web/css/flex/index.md`), GitHub Docs has three; this vault keeps its
archive at `iterations/done/`. Adding `web/css/zz-dogfood/index.md` to MDN
or `iterations/done/iteration-02b-deep.md` here after `create-index` gives
no warning from `find`/`summary`/`backlinks --index`. The limitation is in
`create-index --help`, but the warning text (`index older than vault`) makes
users believe the absence of a warning means "fresh". Options: recurse one
more level (cheap — still no file stats), or state the probe's depth in the
`--index` flag help where people actually read it.

### UX-2: `Low-confidence matches (excluded from plain --apply)` counts fixes that were applied (LOW)

GH Docs scratch, `links fix --apply --apply-fuzzy`: text says `Fixable: 0`,
`Low-confidence matches (excluded from plain --apply): 5508`, then `Applied:
yes (2254 fixes)`. JSON is unambiguous (`fixable 0, fuzzy 5506,
fuzzy_below_floor 3253, applied_fixes 2253`), but the text label reads as
"5508 excluded" when 2254 of them were written. Under `--apply-fuzzy` the
line should split into applied / below-floor.

### UX-3: `read`'s UTF-8 placeholder promises "lossy in search"; search skips the file (LOW-MEDIUM)

`read` prints `<line skipped: invalid UTF-8 (lossy in search; fix encoding to
read)>`, but `find "line ok"` on the same file prints `warning: skipping
bad-utf8-root.md: stream did not contain valid UTF-8` and returns 0 hits —
the whole file is dropped from BM25 (`--file` metadata listing still works).
`lint` reports it as a `FILE` error, which is right. Either make the scanner
lossy for body search as the placeholder claims, or change the placeholder to
"file is excluded from body search". `read --help` does not mention the
placeholder at all.

### UX-4: explicit file argument does not override `[lint] ignore` (LOW)

`hyalo lint research/bad-utf8.md` → `0 files checked, no issues (1 ignored by
[lint] ignore)`. Correct by the config, but when a user names one file they
almost always want it linted; the ignore list is a CI-gate concern. This
affects the owner's own instruction to `lint --strict` a dogfood report
under `dogfood-results/**`. A `--no-ignore` flag, or honouring an explicit
positional path, would remove the surprise.

### UX-5: `new` cannot seed properties (LOW)

`hyalo new --type iteration --file … --property title=…` → clap error. The
file lands with `title: TBD`, `tags: []` and a lint hint; a second `set` call
is always needed. `new --help` does not point at `set`. Not new to this
release, noted because the iteration-file rules require title/branch/tags.

### UX-6: `find` starting a query with `--` needs `-- ` (LOW, clap)

`hyalo find '--index && …'` → `unexpected argument '--index && …'` with the
tip to use `-- ` — fine, just noting that the tip works.

## What Worked Well

- **Stale-index contract is honest where it applies.** Warning names the
  remedy, stdout stays pure, `--quiet` silences it, no exit-code change;
  `links fix`/`links auto` go further and heal from disk with a precise
  per-file message. `--strict-index` is gone without a trace.
- **Dot-paths are correct on messy real data.** Nested maps, arrays of maps,
  arrays inside arrays of maps, numeric index segments, `!`, `~=`, `--index`
  parity and `set --where-property` all agree with `grep`, including the
  edge where body code samples contain YAML that looks like frontmatter.
- **Refactor is neutral.** Lint envelope keys, hint markers, text shapes,
  error texts (`--limit` tip, `--jq` + `--format text` conflict, vault
  boundary on `create-index --index-file`, `--count` on non-list commands)
  all match the baseline byte for byte where compared.
- **Index parity for the common paths is real now.** Fresh index, `set`,
  `append`, `mv` (known and unknown files), `lint --fix`, `new` — BM25
  scores, backlinks, summary, broken-links all byte-identical to disk.
- **Schema on write** is advisory with a clear note (`write proceeds — run
  hyalo lint to enforce, or set --validate to reject`) and `--validate`
  rejects with the enum list and a did-you-mean.
- **`mv` of the most-linked note** (`decision-log.md`, 42 inbound) into a
  subdirectory needed 0 rewrites — basename resolution kept every link
  resolving, backlinks 42 → 42, no new broken links.

## Performance

Wall-clock, warm cache, Apple Silicon, `--format json --no-hints`, best of
two runs. Baseline column is the 2026-08-27 report.

| Command | Own KB (425) | GH Docs (3,710) | MDN (14,375) | MDN `--index` | baseline (own/GH/MDN/idx) |
|---|---|---|---|---|---|
| `find --limit 1` | 0.02 s | 0.18 s | 1.15 s | 0.48 s | 0.05 / 0.26 / 1.26 / 0.42 |
| BM25 query | 0.19 s | 1.08 s | 4.23 s | 0.45 s | 0.03 / 1.19 / 4.21 / 0.42 |
| `summary` | 0.08 s | 0.41 s | 1.42 s | 0.76 s | 0.08 / 0.51 / 1.33 / 0.72 |
| `find --property k=v --limit 0` | 0.07 s | 0.21 s | 1.07 s | 0.45 s | 0.07 / 0.21 / 1.07 / 0.45 |
| `find --property versions.ghec=* --limit 0` | — | 0.35 s | — | — | new |
| `lint` | 0.13 s | 0.82 s | 3.26 s | — | 0.12 / 0.93 / 2.93 |
| `lint --fix --dry-run` | — | 0.95 s | 2.89 s | — | — / 1.68 / — |
| `find --broken-links --limit 0` | 0.06 s | 0.38 s | 1.70 s | — | 0.06 / 0.42 / 1.54 |
| `links` | 0.09 s | 4.08 s | — | — | 0.07 / 4.50 |
| `links fix --dry-run` | 0.07 s | 4.14 s | 5.81 s (prefix) / 27.9 s (derived) | — | 0.07 / 4.27 / 5.78 / 28.4 |
| `links fix --dry-run --case-insensitive` (prefix) | — | — | 4.95 s | — | new |
| `create-index` | — | — | 2.95 s, 121 MB | — | 2.84 s, 120 MB |

The only >2× delta is own-KB BM25 (0.19 s vs 0.03 s). It is **not a
regression from iterations 241–248**: the Homebrew-installed
`hyalo 0.20.0 (1af840a 2026-07-19)` takes the same 0.19 s on today's vault,
and `--glob 'iterations/*'` (241 files) takes 0.08 s — the vault grew
(3.9 MB, `decision-log.md` alone 220 KB) and the baseline number was
measured on a smaller corpus. Still worth a look: 0.19 s for 3.9 MB is
disproportionate next to GH Docs (3,710 files in 1.08 s).

## KB writes made during this session

- Created this report only. All mutations (`set`, `mv`, `task toggle`,
  `links fix --apply`, `create-index`, `new`, `lint --fix`) ran on scratch
  copies under the session scratchpad; the MDN snapshot index at
  `../mdn/files/en-us/.hyalo-index` (untracked there) was rebuilt.
- No fixes applied to the live vault: the three anchor-only broken links
  (`decision-log.md`, iterations 198/199) predate this session and are the
  same set the baseline reported.

## Release readiness

**Verdict: yes-with-fixes for v0.21.0.** Nothing HIGH or MEDIUM in the bug
list; the two bugs are LOW and both are index-parity edges (`task toggle
--index` score drift, no-op `set --index` not refreshing). The one MEDIUM is
UX-1: the stale-index probe is silent on every nested vault, which is exactly
the kind of vault `--index` exists for. Recommended before tagging: (1) BUG-1
— route `update_task` through the same rescan/statistics path `set` uses so
the iter-244 parity AC holds for the full mutation set; (2) UX-1 — either
recurse the probe one level or put the depth limit in the `--index` flag
help. UX-2/3 are message-only. If the owner prefers to ship now, release with
the CHANGELOG stating the probe's depth-1 limit explicitly.
