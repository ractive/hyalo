---
title: "Dogfood v0.21.0-pre3 — fix waves 207 + 210–214 verified; document-scoped zone gaps remain"
type: research
date: 2026-08-23
status: active
tags: [dogfooding, links, lint, frontmatter, config, performance]
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
  - "[[iterations/iteration-207-inert-zone-completion]]"
  - "[[iterations/iteration-210-output-truth]]"
  - "[[iterations/iteration-211-links-resolution-correctness]]"
  - "[[iterations/iteration-212-fuzzy-confidence-trust]]"
  - "[[iterations/iteration-213-config-ux-polish]]"
  - "[[iterations/iteration-214-frontmatter-format-preservation]]"
  - "[[iterations/iteration-215-anchor-and-broken-links-followups]]"
  - "[[iterations/iteration-206-links-perf-profiling]]"
---

# Dogfood v0.21.0-pre3 — fix waves 207 + 210–214 verified

Binary: `hyalo 0.20.0 (5cadca0ba623 2026-08-23)` built from main HEAD `5cadca0`
(post PR #246). Session run by 4 parallel agents plus an orchestrator pass.
Corpora: own KB (386 files), GitHub Docs `~/devel/docs/content` (3,710),
MDN `~/devel/mdn/files/en-us` (14,375), vscode-docs (760). All writes ran
against scratchpad copies; both real checkouts and the repo verified untouched
(`git status` clean, pre-existing `.hyalo-index` files predate the session).

Headline: all 14 bug clusters from [[dogfood-results/dogfood-v0210-pre2-integrity-wave]]
are fixed or substantially fixed, and iters 211/212/213 verify clean at scale.
But iter-207's premise — `links auto --apply` never writes into a non-prose
zone — still fails on two zone classes it did not enumerate (CommonMark
reference links, line-wrapped links), one of which corrupted a pristine copy of the own KB
during testing (the checked-in KB is untouched). Both share one root cause: the link/tag zone scan is line-scoped, not
document-scoped.

## New Feature Verification

### iter-207 — inert-zone completion — PARTIAL

All four targeted bugs closed; verified with an independently written
CommonMark scanner, diffing each corpus against a pristine copy.

- Code spans (BUG-1): **0** insertions inside code spans across 68,984
  insertions on three corpora (GH Docs 35,860 / vscode-docs 32,924 / own KB
  200 — the first two match iter-207's recorded numbers exactly). Nested
  fences, tilde fences, unmatched backtick inside a fence, `<kbd>`-backtick
  pairing all handled CommonMark-correctly (hyalo matches a real renderer,
  not a heuristic).
- Liquid (BUG-2): 0 insertions inside `{% %}`/`{{ }}` on GH Docs (was 3,328).
- Raw HTML (BUG-3): 0 on GH Docs for single-line tags; **1 leak remains on
  vscode-docs via a multi-line `<video>` tag** (NEW-10 below).
- Templated bucket: `templated: 63` in text and JSON; bucket arithmetic exact
  (`0 fixable + 5,506 fuzzy + 530 unfixable + 63 templated = 6,099 broken`);
  all 63 templated targets byte-intact after `links fix --apply --apply-fuzzy`.
- Symlink-dedup preference (BUG-7): real file wins in all four alias orderings
  tried (alphabetically first, last, both).
- Apply-path integrity: 2,253 fixes across 910 files, `6,099 → 3,846` broken
  (exactly −2,253), 0 non-destination changes in changed files, idempotent.

But two structural gaps remain — NEW-1 and NEW-2 below, both HIGH, both
silent-corruption class, both reachable through the hint-recommended command.

### iter-210 — output truth — PARTIAL

- Plain `lint` whole-run counters (BUG-6): fixed. GH Docs `total 7720 ==
  4 errors + 7716 warnings`, invariant across `--limit 1/50/100000`;
  `files_truncated` tracks actual truncation; `rule_groups[].count` sums to
  the total. The original MDN repro now reports `14248 == 1 + 14247`.
- Unmatched `--rule-prefix` (BUG-5): exits 1 with `no rule matches prefix`
  + hint, lints nothing.
- `links` text buckets reconcile and print actionable sections before the
  5,506-line fuzzy listing; per-fix JSON (`fuzzy_fixes`, `unfixable_links`,
  `out_of_vault_links`, `templated_links`) fully populated in dry-run.
- DEC-073 columns: `links auto` and MD009/MD011/MD034 report 1-based Unicode
  scalars — but **MD010 is still byte-indexed** (NEW-11).
- Hint gate: 47 read-only hints from 21 invocations ran verbatim, 0 broken.
- **Gap:** the `lint --fix` path still computes totals over the display-
  truncated file list (NEW-6) — BUG-6's defect surviving on the write path,
  found independently by two agents.

### iter-211 — links resolution correctness — WORKING

- GitHub-slug anchors (BUG-8): slug/raw/percent-encoded forms, unicode,
  punctuation stripping, emoji, duplicate-heading `-1` suffixes, headings
  inside code fences (correctly not anchors), same-file fragments — all
  correct. GH Docs: 1,048 broken anchors of 1,822 checkable, matching the
  iteration's own post-fix number; a 6-sample of the remainder is all genuine
  (OpenAPI-generated pages, Liquid headings = documented limitation).
- HYALO006 line numbers (BUG-9): exact for 0/3/5/17-property frontmatter.
- Backlinks parity + trailing slash (BUG-10): 10 target spellings → 10 single
  backlink entries, zero double-counting; 30/30 targets agree at GH Docs
  scale; DEC-077 directory fallback honored.
- DEC-076 gating: bare `links fix --apply` on full GH Docs modified **0
  files**; ShortestPath (0.95) vs BasenameFallback (0.7, below floor) split
  exactly as documented.
- `mv` spelling round-trip: 12 reference spellings (extensionless, absolute,
  trailing-slash, fragment, angle-bracket, title, query) all preserved.

### iter-212 — fuzzy confidence trust — WORKING

- The three BUG-11 poster cases reorder exactly as the plan claims (0.9→0.504,
  0.889→0.533 now below floor; the correct proposal 0.6→0.870 now applied).
- Ground truth reproduces: independent re-index of GH Docs `redirect_from`
  (9,633 URLs) scores the 2,253 applied fixes at **99.3% correct** — identical
  to the plan's evidence table. Unfiltered floor-0: 76.2%. Accuracy is
  monotone by confidence band above 0.7 (`[0.80,0.85)` = 99.8%).
- Floor mechanics: `fuzzy_below_floor 3,253`, applied = 5,506 − 3,253 = 2,253,
  idempotent second pass applies 0. `--min-confidence` works at 0/0.9/0.99,
  implies `--apply-fuzzy`, rejects out-of-range at exit 2; config key
  `[links] fuzzy_min_confidence` honored, flag wins.
- Strategy labels honest in text (`[basename-fallback 0.861]`) and JSON.
- Doc nit: the plan's evidence table claims 100% at the 0.9 floor; re-measure
  says 304/312 (one repeated wrong proposal ×8) — likely a redirect-index
  size difference, worth a footnote not a fix.

### iter-213 — config UX polish — WORKING

- Ancestor `.hyalo.toml` adoption (DEC-079): works from vault subdirs, loud
  stderr note with remedy, nearest config wins, non-governed dirs fall back
  clean, report/note stream separation verified.
- Malformed-config signal: text leads with `malformed: true` + TOML
  diagnostic + "every value below is a built-in default"; JSON `parse_error`;
  `--jq '.results.malformed'` → `true`; `--raw` appends file text. Writers
  still hard-refuse (iter-201 preserved); readers warn and proceed.
- Changelog config-path refusals now exit 1 with the shared error+hint
  envelope (was exit 2 single-line).
- `views run` positional PATTERN: 12/12 view×pattern combos byte-identical
  to `find <pattern> --view <view>`, including BM25 rank order; `PATTERN`+`-e`
  correctly rejected. **Gap:** `views run` emits zero hints where `find
  --view` emits 2 (NEW-18).
- `config_excluded_titles`/`mentions` split, `create-index` help examples
  (4/4 run verbatim), UX-3 index-mismatch wording, UX-5 error prefix /
  conflict dedup / tags footer / trailing hint flags: all verified fixed.

### iter-214 — frontmatter format preservation — PARTIAL

- The headline holds for `set`: on the 198-line poster file the diff is now
  **1 line** (was 116); across all 406 GH Docs copies, `set` changed exactly
  one line per file, zero fallback warnings; `set`+`remove` round-trips to
  byte-identical.
- Preservation dimensions all pass for `set` on existing keys: key order,
  quoting styles, flow vs block lists, nested maps, comments, blank lines,
  CRLF (inserted lines get `\r\n`), unicode, block scalars, tabs, values with
  `: `/`#`/quotes/newlines. `task toggle` touches only checkbox lines; `mv`
  rewrites one flow-list element leaving sibling quote styles intact.
- DEC-081 fallback warns honestly for `? key` syntax and top-level flow
  mappings; identical-value writes are zero-churn; splice unaffected by an
  active snapshot index.
- **Gaps:** `append`/`remove <key>=<value>` re-serialize the whole touched
  list — 361 of 406 GH Docs files churn, worst 118 lines (NEW-5); mixed line
  endings silently rewrite untouched lines with no fallback warning (NEW-7);
  the documented 64 KiB frontmatter limit is really ~8 KiB of scalar content
  with a leaked parser-internal error (NEW-8).

## Bug Regression Testing (vs pre2 report)

| Bug | Verdict | Evidence |
|---|---|---|
| BUG-1 code-span injection (HIGH) | FIXED NOW | 0/68,984 insertions in code spans, 3 corpora |
| BUG-2 Liquid injection | FIXED NOW | 0 on GH Docs (was 3,328) |
| BUG-3 HTML injection | PARTIALLY FIXED | single-line tags 0 (was 5); multi-line tags still leak (1 on vscode-docs, was 128) |
| BUG-4 `links fix` strips Liquid | FIXED NOW | 0 rewrite offers on 63 templated targets, byte-intact after apply |
| BUG-5 `--rule-prefix` silent full run | FIXED NOW | exit 1, envelope + hint, nothing linted |
| BUG-6 lint JSON counters | FIXED (plain lint) / NOT FIXED (`--fix` path, NEW-6) | GH Docs + MDN counters limit-invariant |
| BUG-7 symlink shadows real file | FIXED NOW | real file wins in 4 orderings |
| BUG-8 anchors vs slugs | FIXED NOW | GH Docs 1,048 genuine broken anchors of 1,822; 6-sample all true positives |
| BUG-9 HYALO006 offset | FIXED NOW | exact at 0/3/5/17-property frontmatter |
| BUG-10 trailing-slash parity | FIXED NOW | 10 spellings → 10 entries; 30/30 targets at scale |
| BUG-11 fuzzy confidence | FIXED NOW | poster cases reorder; 99.3% ground-truth accuracy; auditable per-fix JSON |
| BUG-12 links LOW cluster | MOSTLY FIXED | query strings, CommonMark titles, mv spelling round-trip all pass; hint miscount recurs as NEW-14 |
| BUG-13 iter-204 edges | FIXED NOW | glob hint, gated did-you-mean, FILE-target exit parity, scalar cols (except MD010 → NEW-11) |
| BUG-14 config/UX cluster | FIXED NOW | changelog exit 1, views run pattern, config_excluded counts, create-index help |

No regressions in iter-200/201/202/203 behavior (apply-path monotone +
idempotent, malformed-config writer refusal, boundary gates, foreign-`--dir`
announcement all re-verified in passing).

## Bugs Found

### NEW-1: `links auto --apply` destroys CommonMark reference links (HIGH)

`[label][ref]`, `[ref][]`, `[ref]`, `![ref][ref]` and the `[ref]: url`
definition line are not inert; every form gets `[[…]]` injected, including the
definition, so the rendered page loses the link and shows literal brackets.
Inline `[label](dest)` on the same page is correctly skipped, isolating the
gap. Real corpora: 54 corruptions / 35 files on vscode-docs (broken links
330 → 382), 8 / 5 files on GH Docs (6,099 → 6,107, targets like `"[codespaces"`).
It compounds: the hint-recommended `links fix --apply --apply-fuzzy` then
"fixes" `[[[gamma]]]` to `[[gamma]]]` at `[fuzzy-match 1.0]`, leaving stray
brackets — two hint-recommended applies in sequence take valid markdown to
garbage. Repro: vault with `gamma.md` (`title: Gamma`) + a file using the
five reference forms; `links auto --apply` rewrites all five.

### NEW-2: line-wrapped links are not inert — zone scan is line-scoped (HIGH)

A `[[…]]` or `[…](…)` opening on one line and closing on the next is invisible
to the zone scan: hyalo writes into the target, the label, and even the
destination (`(target.md)` alone on a continuation line was corrupted).
**This fired on a pristine copy of the own KB**: 1 of 200 own-KB insertions landed inside
`[[research/release-pipeline-unification|reusable` (wrapped at col ~72) in
`iterations/iteration-161-shared-release-workflow.md:17`, producing
`[[research/[[release-pipeline-unification]]|reusable` and taking that copy from
0 broken links to 1 (the checked-in KB is untouched). The KB hand-wraps at ~72 columns and has 4 wrapped
wikilink lines across 3 files, so this is systemic exposure every time the
`=> links auto --apply [writes]` hint is followed. Same root cause covers
NEW-1 and NEW-10: make the link/tag zone scan document-scoped.

### NEW-3: auto-link silently rewrites rendered prose instead of emitting an alias (MEDIUM-HIGH)

`links auto` replaces matched text with the target stem and never emits
`[[target|matched_text]]`, though the read side supports it. On GH Docs,
7,968 of 35,860 proposals (22.2%) alter what the page says — 5,178 by case,
2,790 outright (`pull requests` → `[[pulls]]`, `revocation` → `[[revoke]]`).
The JSON already carries `matched_text` and `link_target` separately; emit the
alias form whenever they differ. No decision on record documents prose
substitution as intended.

### NEW-4: ambiguity checked in the title namespace, link emitted in the stem namespace (MEDIUM-HIGH)

The "ambiguous titles are skipped" check passes because
`graphql/reference/pulls.md` (`title: Pull requests`) and `rest/pulls/pulls.md`
(`title: REST API endpoints for pull requests`) have distinct titles — but the
emitted link is the shared stem `[[pulls]]`. Result: `links auto --apply` on
GH Docs writes 1,492 links (`ambiguous: 0 → 1,492`) that hyalo's own resolver
then reports as ambiguous. Ambiguity must be checked on what is emitted.

### NEW-5: `append`/`remove <key>=<value>` re-serialize the whole touched list (MEDIUM-HIGH)

Adding one item re-emits every element, refolding entries over ~80 cols into
`>-` block scalars — DEC-080's original defect relocated from "whole
frontmatter" to "whole touched list". Corpus-wide: one `append` per file
churns 361 of 406 GH Docs files >1 line (`set`: 0 of 406); worst
`admin/index.md` 118 lines. Flow lists explode to block style (`tags:
[iteration, demo]` becomes 4 lines when you add a tag — directly hits own-KB
usage). Fix direction: splice within the list's line span.

### NEW-6: `lint --fix` counters computed over the truncated listing while writes cover the whole run (MEDIUM)

Found independently by two agents. GH Docs at default limit: prints
`fixed 646 · remaining 939 · conflicts 0` but modifies 671 files (~1,604
fixes); at `--limit 100000`: `fixed 1618 · remaining 6109 · conflicts 12`.
Confirmed display-only (with/without `--limit 0` produce byte-identical
trees), but a user who dry-runs at the default limit sees `conflicts 0` and
silently hits 12 on apply. Also `errors`/`warnings` silently change meaning
between plain lint (whole-run) and `--fix` (remaining-only) under the same
key names. This is BUG-6 unfixed on the write path.

### NEW-7: mixed line endings silently churn untouched lines (MEDIUM)

DEC-081 and `set`/`remove --help` name mixed line endings as a warned
fallback trigger. No warning fires, and untouched lines lose their `\r`
(CR count 2 → 0 on a file where only `target:` was set). Exactly the
"unexplained diff churn" DEC-081 exists to prevent.

### NEW-8: frontmatter size limit is ~8 KiB of scalar content, not the documented 64 KiB — and the error leaks internals (MEDIUM)

Help says "64 KiB / 2000 lines"; the real ceiling is 8,192 bytes of total
scalar content, failing with `budget breached: ScalarBytes {
total_scalar_bytes: 8205 }`. Not hypothetical: GH Docs `admin/index.md` has
7,961 bytes of frontmatter — ~40 redirect entries from becoming unreadable by
hyalo entirely. Raise the budget or fix the docs; replace the message either
way. (Same leak family: `duplicate mapping key: …, set DuplicateKeyPolicy in
Options if acceptable`, and `budget breached: Anchors { anchors: 1 }`.)

### NEW-9: auto-derived `site_prefix` misfires silently on MDN — 49,772 false broken links and a 110 s run (MEDIUM)

With no configured prefix, hyalo derives `en-us` from the dir name; MDN links
are spelled `/en-US/docs/…`, the prefix strips nothing, every site-absolute
link classifies broken: `links` = 49,772 broken / 110.4 s vs 510 / 7.8 s with
`--site-prefix 'en-US/docs'`. `hyalo config` shows `site_prefix: en-us
(derived)` with no indication it matched nothing. A "prefix stripped 0 of N
site-absolute links" warning turns a 110-second wrong answer into an obvious
misconfiguration. Also the perf shape for [[iterations/iteration-206-links-perf-profiling]]:
`find --broken-links --limit 0` does the same resolution in 0.47 s (GH Docs)
/ 1.60 s (MDN) — the entire `links` gap is fuzzy candidate generation,
O(broken × files).

### NEW-10: multi-line HTML tags leak (MEDIUM)

Residue of BUG-3, same root cause as NEW-2: a `<video …` tag wrapped across
two lines has its continuation-line attributes treated as prose (1 corruption
on vscode-docs: `title="Demo of navigation and [[intellisense]] features"`).

### NEW-11: MD010 columns still byte-indexed, violating DEC-073 (LOW-MEDIUM)

`àéî\tTAB` → MD010 reports `column 7` (bytes), expected 4 (scalars); emoji
line reports 5, expected 2. MD009/MD011/MD034 are scalar-correct. JSON-only
impact (text omits columns).

### NEW-12: valid same-file fragment links missing from the link inventory (LOW-MEDIUM)

`[a](#part-two)` (resolvable) is absent from `find --fields links` while
`[b](#nope)` (broken) is listed — inventory completeness depends on the
verdict, which is backwards. Produced the only 3 apparent parity mismatches
in the GH Docs backlinks sweep (self-links with dead anchors).

### NEW-13: `case_mismatches` bucket now carries path relocations (LOW-MEDIUM)

`[a](target)` → `sub/target.md` (`[shortest-path]`, applied by plain
`--apply`) is counted and listed under "Case mismatches". A user reads that
count as cosmetic; these are relocations. JSON `case_mismatch_fixes` same.

### NEW-14: below-floor `[writes]` hint promises fixes it will not make (LOW)

When all fuzzy candidates are below the 0.8 floor, `links` still emits
`=> hyalo links fix --apply --apply-fuzzy … # Review then apply 3253
lower-confidence fuzzy fixes [writes]`; running it verbatim prints
`Applied: yes` and changes 0 files. Count post-floor candidates or point at
`--min-confidence`. (Recurrence of the BUG-12 hint-truth cluster.)

### NEW-15: `summary` says 0 broken links while `find --broken-links` reports 3 files (LOW)

Own KB: `summary` → `Links: 853 total, 0 broken`; `find --broken-links` → 3
files with broken anchors (e.g. `decision-log#DEC-068`, where the heading is
`## DEC-068: …` so the shorthand fragment doesn't match the full slug —
arguably iteration-215 design territory, but the two commands must agree on
what "broken" counts).

### NEW-16 (LOW cluster, splice/write family)

- Body-less files without trailing newline gain one on `set` (6 of 406 GH
  Docs files; only residue in an otherwise byte-perfect corpus round-trip).
- DEC-081's fallback-trigger list is partly fiction: anchors/aliases and
  `%YAML` directives hard-error (never reach fallback); duplicate keys
  likewise. Doc should list only `? key`, top-level flow mappings, invalid
  UTF-8 (+ mixed EOL once NEW-7 is fixed).
- Dotted property paths silently create literal top-level keys
  (`--property 'versions.fpt=X'` → key named `versions.fpt`, no warning, no
  descent into the existing map).
- `set` type inference silently retypes `'42'` (string) → `42` (number) with
  no advisory note.
- `links fix --dry-run` previews a normalized target (`"/deep/page" →
  "deep/Page.md"`), not the actual replacement written (`/deep/Page?x=1`).

### NEW-17 (LOW cluster, config family)

- `--dir .` at the repo root: `note: --dir . selects a different vault:
  ./.hyalo.toml does not apply, ./.hyalo.toml is in effect` — both halves of
  a contradiction about one path; also the only place `config_path` prints
  relative.
- Malformed-config note says "every value below is a built-in default" then
  prints the salvaged `dir: vault` from the file; salvaging also differs by
  malformation class (unknown-field salvages `dir`, unclosed-table doesn't)
  with identical wording.
- Effective config for `--dir <foreign-tree>` depends on the caller's cwd
  (running from inside the foreign tree finds its ancestor config; from
  elsewhere reports "no .hyalo.toml — built-in defaults"). Harmless today
  only because the observed config was `dir`-only.

### NEW-18 (LOW cluster, envelope/hints)

- `views run` emits zero hints where the equivalent `find --view` emits 2 —
  incomplete parity against iter-213's own AC. `lint-rules show` and
  `task list` are also hint dead-ends.
- `fuzzy_fixes` entries carry `line` but no `col` (iter-210 task text asked
  for it).
- `backlinks` JSON normalizes `target` inconsistently: strips slashes but not
  `.md`, so the authored spelling is unrecoverable.
- Section headings with a stale hand-written count render doubled:
  `## Tasks [6/6] [7/7]`. Feature idea: a lint rule flagging hand-written
  `[n/m]` that disagrees with the computed count.

## UX Issues

### UX-1: `[lint] ignore` is invisible in the summary line (LOW-MEDIUM)

Bare `lint` on the own KB prints `68 files checked` on a 386-file vault with
no mention that 318 files are config-ignored. A named ignored file warns
(`1 named file excluded by [lint] ignore`) — good — but a `--glob` matching
only ignored files prints `0 files checked, no issues` with no warning,
inconsistent with the named form and a vacuous-green trap. One appended
`(318 ignored by [lint] ignore)` fixes the false alarm.

### UX-2: no exit-code path gates broken anchors in CI (MEDIUM)

`lint --strict` exits 1 on broken targets (HYALO006) but a vault whose only
defect is a dead anchor exits 0, and `find --broken-links` always exits 0.
iter-211 just made anchor checking trustworthy; give it a gate. Related:
`links` prints `Broken links: 0` on a vault whose only defect is a dead
anchor — the summary an agent will trust.

### UX-3: ancestor-adoption note fires on every command from a vault subdir

Full sentence + two absolute paths + remedy, repeated on every invocation for
an agent looping inside `hyalo-knowledgebase/iterations/`. Consider `-q`
suppression or once-per-session emission. Also the stderr `warning:`
duplicate of the malformed-config diagnostic is redundant now that the
`config` report leads with it.

### UX-4: `hyalo read --format json` omits frontmatter without `--frontmatter`

`--jq '.results.frontmatter.x'` silently yields `null` instead of erroring;
a hint on the JSON output would save the round trip. Related: `hyalo
properties` types nested maps as `text` (GH Docs `versions`, `featuredLinks`)
— a `map` type would make the output trustworthy.

## What Worked Well

- **`set` splice is exemplary**: 406/406 GH Docs files at exactly one changed
  line, byte-perfect round-trips, CRLF-aware insertion, comment preservation
  per DEC-080. The 198-line poster file went 116-line diff → 1.
- **The fuzzy floor is real trust**: 99.3% ground-truth accuracy reproduced
  independently; monotone accuracy by band; honest strategy labels; every
  proposal auditable in JSON. `links fix --help` documenting the measured
  evidence for its default is a pattern worth keeping.
- **Counter truth for plain `lint`** held under every limit permutation on
  two corpora, and 47/47 read-only hints executed verbatim.
- **Anchor checking at scale**: the remaining 1,048 GH Docs broken anchors
  sample as all-genuine — the false-positive era is over.
- **Config debuggability**: `hyalo config` (+ `--raw`, `malformed`,
  `parse_error`) now answers "why is my config not working" in one call.
- **Boundary/refusal messages** (create-index outside vault, changelog path,
  malformed-config writer refusal) are uniformly clear, actionable, non-silent.

## Performance

No regressions; lint improved. Wall-clock, warm cache, Apple Silicon.

| Command | Own KB (386) | GH Docs (3,710) | MDN (14,375) | prior |
|---|---|---|---|---|
| `find --limit 1` | 0.03–0.06 s | 0.17 s | 1.2–1.7 s | flat |
| BM25 query | 0.13–0.17 s | 1.1 s | 4.1–6.2 s | flat |
| `summary` | 0.05–0.07 s | 0.45 s | 1.4–2.0 s | flat |
| `lint` | 0.09 s | 0.79–1.05 s | 3.1–3.6 s | **improved** (was 1.21 / 3.76) |
| `find --broken-links --limit 0` | — | 0.47 s | 1.60 s | — |
| `links` read-only | 0.05 s | 12.4–14.7 s | 7.8 s (with prefix) / 110 s (misderived prefix) | ~11.6 s / 8.7 s |
| `links auto --apply` | — | 33.0 s (2,637 files) | — | write-bound: ~12.5 ms/file, 9% CPU |
| `lint --fix --dry-run` | — | ~1.6 s | — | — |

MDN snapshot index: build 2.8 s / 115 MB; BM25 4.08 s → 0.46 s (**8.9×**),
`find --limit 1` 2.7×, `summary` 2.0×. GH Docs `links` drift 11.6 → 12.4–14.7 s
is confounded by the broken count nearly doubling (3,328 → 6,099) on the
current corpus — cost still tracks broken × candidates, which is
[[iterations/iteration-206-links-perf-profiling]]'s target. The apply path is
fsync-bound (0.59 s user vs 33 s wall), a better iter-206 target than reads.

## Recommendation

The six-iteration wave did what it claimed: every pre2 bug cluster is closed
or reduced to a residue, three iterations verify clean at scale, and `set` +
the fuzzy floor are genuinely trustworthy now.

But if v0.21.0 is gated on "no `--apply` corruption paths", **NEW-1 and NEW-2
reopen the gate** — same silent-corruption class as BUG-1/2/3, reachable via
the recommended hint, and NEW-2 already corrupted a copy of the own KB during
this session. Both (plus NEW-10) share one fix: make the existing link/tag zone
scan document-scoped instead of line-scoped. NEW-3/NEW-4 (alias emission,
stem-namespace ambiguity) are the natural second half of that same
iteration: together they'd make `links auto --apply` both safe and honest.
NEW-5/NEW-7 (list splice, mixed-EOL churn) are the iter-214 equivalent —
churn, not corruption, so lower stakes.

Suggested order: one document-scoped-zones + alias iteration (NEW-1/2/3/4/10),
then release v0.21.0; fold NEW-6 (lint --fix counters) in as a cheap rider or
into [[iterations/iteration-216-results-shape-consistency]]; NEW-15 and UX-2
belong with [[iterations/iteration-215-anchor-and-broken-links-followups]].
