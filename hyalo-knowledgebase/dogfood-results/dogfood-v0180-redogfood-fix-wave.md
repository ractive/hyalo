---
title: "Dogfood v0.18.0 re-check — fix-wave 172-175 verification (slim)"
type: research
date: 2026-07-17
status: active
tags: [dogfooding, release-readiness, profiles, okf]
related:
  - "[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]]"
  - "[[iterations/iteration-172-profile-composition-semantics]]"
  - "[[iterations/iteration-173-generator-safety]]"
  - "[[iterations/iteration-174-lint-ci-trust]]"
  - "[[iterations/iteration-175-profile-polish]]"
---

# Dogfood v0.18.0 re-check — fix-wave 172-175 verification (slim)

Single-session slim pass on binary `hyalo 0.18.0 (fe42578e6a6f)` (main after
PRs #201–#204). Scope: re-run the original repro scenarios for the five
release blockers + two UX gaps from
[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]], spot-check the
fixed mediums, and a quick own-KB regression + perf pass. Scratch vaults
(okf+madr composed, changelog, skills) + own KB (337 files, 25 in the lint
gate per the iteration-170 ignore policy).

**Verdict: release v0.18.0.** All five blockers and both UX gaps are fixed
and behave well under the original repros. Four leftovers found (2 MEDIUM,
2 LOW), all fixed in the follow-up PR from this session — none blocks the
release on its own.

## Release-blocker regression testing

### RB-1: profile composition clobbers config — FIXED

`init --profile okf` then `init --profile madr` in one vault:
`[lint] profiles = ["okf", "madr"]` (list — both rule sets fire),
`[schema] exempt` is the union of both profiles' globs,
`[schema.default] required` survives, `[[schema.bind]]` entries persist,
hand-written comments and okf's own comments survive the second merge
(toml_edit). Only cosmetic: merged tables interleave (`schema.default.
properties.*` between `schema.types.adr.*` blocks) — valid TOML, slightly
harder to read.

### RB-2: first `okf index --apply` destroys unmarked index files — FIXED

Hand-written marker-less `INDEX.md` (curated intro, links, extra section) on
case-insensitive macOS FS: dry-run reports `action: "adopt"` with
`preserved_lines: 9`; apply keeps the entire hand-written body and appends
the managed region with blank lines around markers (MD022-clean). Second
apply: `0 files wrote` (idempotent). Case-insensitive targeting picks up
`INDEX.md` as *the* reserved file — no sibling `index.md` created.

### RB-3: lint silently excludes unparseable-frontmatter files — FIXED

Duplicate-YAML-key file now yields
`error HYALO005 line 1 could not parse frontmatter: …` and lint exits 1.
Generators no longer hard-abort: `okf index` prints a stderr warning with a
code frame and reports `1 skipped (malformed frontmatter)` in the summary,
exit 0. (LOW leftover: the message leaks yaml-library advice —
"set DuplicateKeyPolicy in Options if acceptable" — meaningless to users.)

### RB-4: `changelog add` places entries outside `[Unreleased]` — FIXED

On a conformant KaC file ending in link-reference definitions,
`changelog add --category Fixed --message … --apply` inserts
`### Fixed` inside `[Unreleased]`, above `[1.0.0]`, footer refs untouched,
file lints clean (incl. MD047). Note: `add` defaults to `--dry-run`
(documented in `--help`, `"apply": false` in the JSON) — mildly surprising
for an "add" verb but consistent with the generators.

### RB-5: bundled `skills` skill fails its own profile — FIXED

`cargo run -p xtask -- check-bundled-skills` → "8 bundled skill(s) pass the
skills profile", exit 0; wired into quality-gates.yml.

## UX-gap regression testing

### UX-A: walker can't reach `.claude/skills/` — FIXED

The skills profile writes `[scan] include = [".claude/skills/**"]`;
`find` and `lint` both see `.claude/skills/my-skill/SKILL.md`. Bonus
(iter-172 bind=typing): the SKILL.md carries no `type:` frontmatter and
still lints clean under `required = ["type"]`.

### UX-B: skip counters JSON-only — FIXED

`lint --files-from` with a missing path and a non-md path:
text prints `note: 1 input paths missing, 1 non-markdown skipped`;
github prints the same as `::notice::`. (LOW leftover: pluralization —
"1 input paths".)

## Fixed mediums spot-checked

- Bind-typed frontmatter-less files satisfy `required = ["type"]` — WORKING
  (SKILL.md above; ADR below).
- `new --type adr` honors `[schema.types.adr.defaults]` (`status: proposed`,
  `date: $today` → 2026-07-17), scaffolds required sections, omits the
  bind-implied `type:` key — WORKING.
- `madr toc` excludes explicitly non-adr-typed files (`type: concept` file
  in `docs/decisions/` dropped from the table; bind-typed files included) —
  WORKING.
- `types set default` rejected with a precise message pointing at
  `[schema.default]`, exit 1 — WORKING.
- `lint --limit 0` = unlimited; counts identical across `--limit` values
  (pre-truncation counting) — WORKING.
- lint/`okf index` ping-pong: generated region is MD022-clean, lint finds
  nothing to fix inside it, second apply is a no-op — WORKING.
- `[changelog] path = "CHANGELOG.md"` reaches the repo-root changelog when
  the vault is a subdir — WORKING.
- Neutral OKF profile: no BigQuery example types in the merged config —
  WORKING.

## Leftovers found (all fixed in this session's follow-up PR)

### LB-1: exempt globs still case-sensitive (MEDIUM)

`[schema] exempt = ["**/index.md"]` does not match `INDEX.md` on a
case-insensitive FS — while `okf index` (iter-173) *does* treat `INDEX.md`
as the reserved file there. Net effect: the generator adopts/maintains
`INDEX.md`, then `lint` flags that same file
`error SCHEMA missing required property "type"` → red CI on macOS/Windows
OKF vaults. Root cause: `ExemptGlobs::is_exempt`
(crates/hyalo-core/src/schema.rs:167) normalizes backslashes but never case;
iter-173's case handling covered only the generator's reserved-file
targeting. Fix: exempt matching honors the same effective
`[links] case_insensitive` setting (auto-detected) the generator uses.

### LB-4: OKF reserved-file predicates also case-sensitive (MEDIUM)

Same bug class as LB-1, second site: `okf_lint.rs`'s own
`is_index_file`/`is_log_file` predicates compare case-sensitively, so an
adopted `INDEX.md` — exempted from SCHEMA after the LB-1 fix — was still
classified as a concept doc and warned `OKF-CITATIONS-PRESENT` under
`--profile okf`; lowercase `index.md` got zero findings. Fixed by threading
the same effective flag and case-folding the predicates; under
case-insensitivity `INDEX.md` now takes the reserved-file path
(`OKF-INDEX-STRUCTURE` applies, concept-doc rules don't).

### LB-5: `changelog add` splits multi-line bullets (MEDIUM-HIGH, found post-LB-fix while dogfooding the release notes)

Adding the PR #205 entries to hyalo's own root CHANGELOG.md: when the last
bullet of the target `### Category` wraps across lines (hanging indent — as
KaC files routinely do), `changelog add --apply` inserts the new entry after
that bullet's *first* line, orphaning its continuation lines below the new
entries and corrupting the existing entry. The RB-4 fix bounded the section
at the footer link refs but the insertion anchor still doesn't skip
continuation lines. Fixed in the follow-up PR from this session.

Two adjacent LOW notes from the same exercise:

- `hyalo lint CHANGELOG.md` cannot reach the `[changelog] path` file when it
  lies outside the vault (`file not found`) — `changelog add` can write a
  file that lint can't check by path.
- `changelog add` emits the message as one long line; files wrapped at 80
  columns end up style-inconsistent (MD013-relevant where enabled).

### LB-2: skip-summary pluralization (LOW)

`note: 1 input paths missing` → "1 input path missing".

### LB-3: HYALO005 / generator warnings leak yaml-library internals (LOW)

`…duplicate mapping key: type, set DuplicateKeyPolicy in Options if
acceptable` — the trailing library advice refers to a Rust API the user
cannot touch. Trim it from user-facing messages.

## What worked well

- The adopt path is genuinely safe: dry-run names the action (`adopt`) and
  the preserved line count before anything is written.
- `changelog add --dry-run` default + drift-nonzero exit makes the CI story
  coherent across generators.
- Error message quality is high (`types set default` explains *where* to do
  it instead; redundant `--dir` gets a note).
- Own KB: 0 errors under `--strict` (660 warnings, all MD013-class in
  frozen-adjacent files).

## Performance (own KB, 337 files)

| command | time |
| --- | --- |
| `summary` | 0.05 s |
| `find --property status=completed` | 0.03 s |
| `find "profile composition"` (BM25) | 0.11 s |

No regressions vs. the pre-release fleet baselines.
