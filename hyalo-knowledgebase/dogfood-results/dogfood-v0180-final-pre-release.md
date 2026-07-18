---
title: "Dogfood v0.18.0 — final pre-release: LB verification + OKF deep-dive + scale"
type: research
date: 2026-07-18
status: active
tags: [dogfooding, release-readiness, okf, profiles]
related:
  - "[[dogfood-results/dogfood-v0180-redogfood-fix-wave]]"
  - "[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]]"
  - "[[iterations/iteration-173-generator-safety]]"
  - "[[iterations/iteration-174-lint-ci-trust]]"
  - "[[iterations/iteration-175-profile-polish]]"
---

# Dogfood v0.18.0 — final pre-release

Binary `hyalo 0.18.0 (9e7329a 2026-07-18)`, built from HEAD. Four parallel
agents: LB regression verification, OKF tools deep-dive + docs audit, scale
tests (MDN 14,375 files + GitHub Docs 3,710 files), own-KB exploration
(338 files). All mutations in scratch copies; both external repos and the
own KB verified clean afterwards (`git status` empty).

## Bug Regression Testing (yesterday's leftovers)

All five verified on macOS (case-insensitive FS) with the original repro
scenarios:

### LB-1: exempt globs case-sensitive — STILL FIXED

`INDEX.md` (uppercase, no frontmatter) in an okf-profile vault: `hyalo lint`
produces zero SCHEMA errors; `**/index.md` exempt glob matches.

### LB-4: OKF reserved-file predicates case-sensitive — STILL FIXED

`upper/INDEX.md` vs `lower/index.md` with identical content produce the
identical single `OKF-INDEX-STRUCTURE` finding; no concept-doc rules fire on
the uppercase variant.

### LB-5: `changelog add` splits multi-line bullets — STILL FIXED

Wrapped last bullet with hanging indent, last bullet with nested sub-list,
empty category, and missing category all insert correctly with link refs
intact. One cosmetic leftover: inserting into an existing *empty* category
omits the blank line after the `### Heading` (section-creation path does add
it) — LOW.

### LB-2: skip-summary pluralization — STILL FIXED (core)

`--files-from` with 1 missing path → `note: 1 input path missing`. Two
adjacent misses remain (LOW): the hint line on the next row still says
`1 input path(s) did not exist` (hints.rs:330), and the lint text summary
never agrees: `(1 errors, 0 warnings)`.

### LB-3: yaml-library internals leak — STILL FIXED

Duplicate mapping key via `lint` (HYALO005) and `okf index` both emit clean
messages; no `DuplicateKeyPolicy`/`Options` advice anywhere. Confirmed at
scale too: injected broken YAML on GitHub Docs copies produced clean
human-readable messages.

### Adjacent notes from yesterday

- `[changelog] path` outside the vault: `hyalo lint --profile changelog`
  (the hinted route) now reaches and lints it — effectively resolved.
  Direct file args still refuse (`file resolves outside vault boundary`),
  arguably by-design sandboxing.
- `changelog add` long-message wrapping: unchanged, still one long line
  (LOW).

## Bugs Found

### BUG-1: `mv` misses frontmatter `related` links carrying an anchor (HIGH)

`hyalo mv decision-log.md --to docs/decision-log-archive.md` rewrites body
links incl. anchors (`[[decision-log#DEC-013]]` ✓) and anchor-less
frontmatter entries (✓), but leaves
`related: - "[[decision-log#DEC-041]]"` untouched
(iteration-138-schema-extensions-and-new-command.md line 17) → new broken
link. The miss is specifically *frontmatter + anchor*.

### BUG-2: `links fix --apply` drops the anchor when repairing a frontmatter link (MEDIUM)

Continuing BUG-1: the repair rewrites the entry to
`"[[decision-log-archive]]"` — `#DEC-041` silently lost. Body-link repairs
keep anchors. Together with BUG-1 this is a two-step data-loss path.

### BUG-3: `okf index --apply` not idempotent with dangling/reversed markers; 2nd apply deletes hand prose (MEDIUM-HIGH, data loss)

`index.md` with prose + a lone `<!-- okf:index:begin -->` (no end): first
apply appends a fresh region (file now has two begin markers), dry-run still
drifts, second apply rewrites from the *first* begin to the first end and
silently deletes the hand-written text after the dangling marker. Violates
the documented "apply twice is a no-op". Dangling/reversed/duplicate markers
should be detected and warned, not half-adopted.

### BUG-4: bundled okf skill teaches a `types set` invocation the CLI rejects (MEDIUM-HIGH, embarrassment class)

Skill says "quoted names with spaces are fine end-to-end":
`hyalo types set "Data Table" --required type,title` →
`invalid type name 'Data Table': must contain only alphanumeric characters,
hyphens, or underscores`. Hand-declaring `[schema.types."Data Table"]` in
TOML works end-to-end, so it's a `types set`-only restriction; the skill's
own example concept uses `type: BigQuery Table`. Either relax `types set`
validation or fix the skill (README line 168 is correct).

### BUG-5: HYALO001 fires inside fenced code blocks (MEDIUM)

All 11 findings on full MDN are false positives — `[]` in JS/regex code
fences (`glossary/truthy`, `array/reduce`, `regular_expressions/
character_class`). Rule must skip fenced code.

### BUG-6: HYALO001 reports body-relative line numbers (MEDIUM)

Reported line is off by the frontmatter length (truthy: reported 30, actual
17/36; character_class: reported 7, actual 14). The message text embeds the
wrong number too.

### BUG-7: `create-index` hint omits `--dir` (MEDIUM)

After a slow command on a `--dir` vault the hint is bare
`-> hyalo create-index  # Command took 1062 ms. …:` — run verbatim it would
index the wrong vault. Every other hint carries `--dir`. Description also
ends with a dangling colon.

### BUG-8: `find --orphan` hints drop the active filter (MEDIUM)

Hint "Show all 79 results" proposes `hyalo find --limit 0` → returns all
338 files; sibling "Narrow by tag: iteration (27 files)" proposes a command
returning 146 (actual `--orphan --tag iteration` is 37). Hints are
copy-paste contracts for agents; derived hints must preserve all active
filters and compute counts on the filtered set.

### BUG-9: `summary` "Lint" hint contradicts `lint` (MEDIUM)

Summary says "Lint: 5 errors, 12 warnings"; the hinted `hyalo lint` reports
0 errors, 660 warnings (exit 0 even `--strict`). Summary's schema counters
ignore `[lint].ignore` globs and exclude MD rules yet are labeled "Lint".
Apply ignore globs or relabel.

### BUG-10: `okf index` emits CommonMark-invalid links (MEDIUM)

Spaces in destinations (`* [Blöcke Übersicht 🎉](blocks table.md)`, subdir
`(spaced dir/index.md)`) render as literal text on GitHub; `]` in titles
breaks the link entirely; embedded newlines in `description` emit multi-line
bullets. Angle-bracket/percent-encode destinations, escape `[`/`]` in text,
collapse newlines. (Also seen on MDN copy: unescaped `[test]` in a title.)

### BUG-11: one unwritable `index.md` target aborts `okf index --apply` mid-run (MEDIUM)

`index.md` as a *directory*: apply errors with a raw temp-file message after
already writing alphabetically-earlier files — partial state; dry-run
happily reported `create`. Should warn-and-continue per the
malformed-frontmatter precedent (same abort for `okf log` into a `log.md`
directory).

### BUG-12: `-q` doesn't suppress `okf index` skip warnings (MEDIUM)

Help promises "suppress all warnings printed to stderr"; the
malformed-frontmatter skip warning prints in both flag positions.

### BUG-13: `okf index <dir>` with nonexistent scope silently succeeds (LOW-MEDIUM)

`okf index no-such-dir --dry-run` → `0 files would change (0 scanned)`,
exit 0 — a typo'd scope makes a CI freshness check pass vacuously. (Path
*escape* is properly rejected.)

### BUG-14: `okf log` multiline `--message` corrupts log structure (LOW-MEDIUM)

Raw newlines produce an unindented paragraph + literal `## fake heading`
inside the list; `OKF-LOG-STRUCTURE` doesn't flag the result. Indent
continuation lines or reject newlines.

### BUG-15: `okf log <new-dir>` dry-run/apply disagree (LOW)

Dry-run: `would log entry … (created)`, exit 0; apply: raw
`failed to create temp file` error, exit 2.

### BUG-16: code-span wikilink suppression is line-based (LOW-MEDIUM)

Found while dogfooding this very report. A code span that wraps across a
line break flips the scanner state, so a properly backticked wikilink later
on the continuation line is counted as a link. Repro:
`` Multi line: `[[missing-\ntwo]]` and then `[[missing-three]]` after. ``
→ `find --broken-links` reports `missing-three` broken even though it sits
inside its own single-line code span (CommonMark code spans may contain
newlines, and pair backticks across them). Single-line spans suppress
correctly.

### BUG-17: lint severity display vs counts disagree (LOW)

Typeless concept: both findings render `error SCHEMA` in text mode but the
summary counts `1 errors, 2 warnings`.

### Other LOWs (collected)

- `summary` false-positive did-you-mean: `hero-6` vs `hero-4` asset paths
  flagged as possible typo; numeric-suffix values shouldn't trigger it.
- `set` validates dates but not enums/patterns — `status=bogus` and a
  wrong `branch` prefix write silently; emit the same advisory note.
- `--jq` + `--format text` → clear error but exit 2 (help defines 2 =
  internal error).
- `set` JSON response echoes coerced YAML-list values as plain strings.
- `set` list coercion keeps literal quotes on quoted elements: found while
  writing the follow-up iterations —
  `--property 'related=["[[a]]", "[[b]]"]'` stores
  `- "\"[[a]]\""` (quotes become content). Unquoted elements
  (`related=[[[a]], [[b]]]`) round-trip correctly. Elements should be
  parsed as YAML flow-sequence scalars, not comma-split raw text.
- `new --type iteration` scaffolds `branch: TBD`, violating the type's own
  `^iter-\d+…/` pattern — fresh scaffold lint-errors immediately.
- `okf index` grammar: `1 file wrote` → "written";
  `preserving 1 existing lines` → "line".
- `okf log --action ""` silently ignored while `--message ""` errors.
- `init --profile okf` re-run prints `updated .hyalo.toml` even when
  byte-identical.
- `hyalo lint a.md b.md` → "unexpected argument"; only one positional FILE
  accepted.

## Documentation Mismatches (okf audit)

- `--site-prefix` help (every subcommand): "pass `--site-prefix \"\"` to
  disable absolute-link resolution entirely" — false; empty prefix means
  *resolve from bundle root* (what the okf profile relies on). Help text is
  stale; README/skill/profile semantics are what's implemented.
- Bundled okf skill: spaced `types set` promise — see BUG-4.
- README conflict-line format documented as `conflict: <key> "<old>" ->
  "<new>"`; actual is `warning: conflict: validate_on_write false -> true
  (profile okf)` — cosmetic drift.
- "apply twice is a no-op" (help + README + skill) — falsified by BUG-3's
  marker edge cases.
- Empty directories are omitted from `Subdirectories` (sensible, avoids
  broken links) but undocumented.
- Spec §"bundle-root index.md may carry a single `okf_version` key": the
  generator preserves any extra keys and no lint rule flags violations —
  docs imply enforcement that doesn't exist.
- `research/okf-open-knowledge-format.md` claims okf commands emit a
  `-> hyalo lint --profile okf` hint; text mode emits no hints at all (a
  non-standard `results.hint` JSON field exists, standard `hints` array
  stays empty).

## UX Issues

- **MDN-style vaults**: 49,933/49,935 links "broken" (site-URL links, not
  file paths) yet hints cheerfully offer `links fix`. Heuristic wanted:
  "~100% of links unresolvable — links appear to be site URLs; check
  `--site-prefix`".
- `links fix` fuzzy matcher confidently proposes wrong targets
  (`[[iterations/done/iteration-132-mv-wikilinks]]` → `iteration-02-links.md`
  at 0.90); `--apply` writes all in one shot. Wants a confidence gate /
  `--min-confidence`.
- okf commands print no drill-down hints in text mode even with `--hints`.
- `[okf] ignore` excludes files from generation but not from okf lint rules
  — split-brain for `_template/**` trees.
- Duplicate marker pairs: only the first region is managed; second region's
  stale content preserved forever with no warning.
- MD034 URL detector swallows trailing Liquid `{%` (GitHub Docs); the fix
  would wrap template syntax into the autolink.
- MD011 fires as *error* on literal regex text in prose.
- Post-`lint --fix` output still prints the pre-fix "Show all N files with
  issues" hint.
- HYALO005 double prefix: `could not parse frontmatter: failed to parse
  YAML frontmatter: …`.
- `--property 'priority>=high'` silently does lexicographic comparison on
  enum values.
- `mv A B` positional form unsupported (`--to` required) — asymmetric with
  other positional-file commands.

## Own-KB Hygiene (fix independently of release)

- 25 broken links in 11 files; genuinely stale:
  `[[iteration-80-musl-targets-winget]]`
  (research/release-pipeline-unification.md),
  `[[iterations/done/iteration-132-mv-wikilinks]]` (iteration-150),
  `[[CLAUDE]]` (iteration-118),
  `[[feedback_keep_docs_in_sync]]` (points outside the KB),
  `[[doc]]` (iteration-168). The rest are illustrative pseudo-links in
  dogfood reports — backtick them.
- Duplicate files in `iterations/done/`:
  `iteration-25-release-profile-and-quick-wins.md` **and**
  `iteration-25-release-profile-quick-wins.md`.
- 34 files `status: completed` with open tasks; iteration-171 still
  `in-progress` (4 open "hyalo's own CI uses the action" tasks).
- Vendored `research/setup-hyalo-action/` files pollute summary counters
  (see BUG-9).

## What Worked Well

- **All five LB fixes hold** — the fix-wave + leftover PRs are solid.
- **Performance at scale, no regressions**: MDN full lint 2.5s, GitHub
  Docs lint 0.84s, BM25 on MDN 3.9–4.1s scan vs **0.39s with snapshot
  index (10x)**; `create-index` for 14,375 files in 2.3s; zero HYALO005 on
  both real KBs (GitHub Docs' nested YAML parses cleanly).
- **OKF core semantics rock-solid on well-formed input**: byte-idempotent
  applies, drift exit codes CI-usable (1 drift / 2 error), non-destructive
  adopt verified on MDN's 1,228 real content-bearing index.md files, prose
  outside markers preserved verbatim, `okf_version` kept, scoped runs
  don't warn about out-of-scope bad files, path traversal rejected.
- **Profile composition**: okf+madr+changelog union-merge, byte-identical
  re-runs in any order, comments and unknown tables survive, scalar
  conflicts warned loudly.
- **`init --profile okf --claude`** installs 5 files; the skill's command
  inventory verified working except the spaced-`types set` step (BUG-4).
- **Guardrails**: index-outside-vault refused with `--allow-outside-vault`
  hint; vault-boundary checks everywhere; `set` round-trips unicode/
  multiline/booleans/YAML lists perfectly; `append` scalar→list promotion
  as documented; `mv` skips code-span occurrences.
- **jq integration** stable envelope, actionable errors; dashboards are
  one-liners.

## Performance

| Command | KB | Time |
|---|---|---|
| `find --limit 1` | MDN 14,375 | 0.94–1.09s |
| BM25 `find "flexbox layout"` | MDN | 3.9–4.1s (scan) / **0.39s (index)** |
| `summary` | MDN | 1.14s |
| `lint --format json` | MDN | 2.48s |
| `create-index` | MDN | 2.29s (114 MB) |
| `find --limit 1` | GH Docs 3,710 | 0.13–0.22s |
| BM25 search | GH Docs | 1.05s |
| `lint --format json` | GH Docs | 0.84s |
| `okf index --apply` | MDN css copy, 1,228 files | 0.39s |
| `summary` | own KB 338 | 43ms |

No command exceeded 2x prior baselines. No hangs, no crashes.

## Release Assessment

The v0.18.0 headline features (okf tools, profiles, lint pipeline) are
functionally solid and all previous blockers stay fixed. Recommend fixing
before release: **BUG-3** (marker-edge data loss in the flagship generator),
**BUG-4** (bundled skill teaches a command that fails — same embarrassment
class as RB-5), and ideally **BUG-10** (invalid links in generated output).
BUG-1/BUG-2 (`mv`/`links fix` frontmatter-anchor loss) are serious but
pre-existing, not 0.18.0 regressions — fine as a fast-follow. Everything
else can ride a later wave.

## Re-verify Addendum 2026-07-18 (post iter-176/177)

All three pre-release blockers re-verified FIXED against the original
repros with the release binary built from merged main (PR #207/#208):

- **BUG-3** — dangling/reversed/duplicate/dangling-end markers all
  skip-and-warn; index.md byte-identical across two applies
  (`skipped_markers` counted, `changed: 0`). Data-loss path gone.
- **BUG-4** — `types set "Data Table"` succeeds and round-trips through
  `new` → `lint` → `find --property 'type=Data Table'` per the bundled
  skill's flow.
- **BUG-10** — generated links CommonMark-valid: spaced destinations
  angle-bracketed, `[]` escaped in text, multiline descriptions collapsed;
  second apply is a byte-identical no-op. (BUG-9 grammar also confirmed:
  "2 files written".)

New findings recorded in
[[reviews/link-handling-review-2026-07-18]] addendum (L-A1 angle-bracket
destinations unsupported by the link parser, L-A2 escaped-bracket link
text invisible, dry-run exit-code contract nit) — deferred to the planned
link chain (iterations 178–185), not release blockers. Minor `new`/lint
nits also observed (MD047 on own scaffold, OKF-CITATIONS-PRESENT off-by-one
line cite, post-`new` hint promising placeholder violations that lint
doesn't emit).

**Verdict: cleared for v0.18.0 release.**
