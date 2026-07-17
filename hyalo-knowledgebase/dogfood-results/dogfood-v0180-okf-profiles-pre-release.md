---
title: "Dogfood v0.18.0-pre — OKF + profiles fleet (7 agents, 6 real KBs + 2 at scale)"
type: research
date: 2026-07-17
status: active
tags:
  - dogfooding
  - okf
  - profiles
related:
  - "[[iterations/iteration-163-okf-frontmatter-foundations]]"
  - "[[iterations/iteration-166-okf-conformance-lint]]"
  - "[[iterations/iteration-167-madr-profile]]"
  - "[[iterations/iteration-169-changelog-profile]]"
  - "[[iterations/iteration-170-lint-github-annotations]]"
  - "[[iterations/iteration-171-setup-hyalo-action]]"
---

# Dogfood v0.18.0-pre — OKF + profiles fleet

Pre-release validation of the iter-163–171 features on binary
`hyalo 0.18.0 (49c670bc)`, run as seven parallel agents: five real OKF
conversions (own KB, hoppy 230 files, ff-rdp 329, mapl-memory 298,
user-service + user-event-service), one bundled-skills audit, one read-only
scale pass (MDN 14,375 files, GitHub Docs 3,710). Raw per-agent reports in the
session scratchpad; conversion work sits on local `okf-conversion` branches
(not pushed) in each repo for human review.

**Verdict: hold the v0.18.0 release.** Every feature works in the happy path
and performance is excellent everywhere, but the fleet found five
release-blocker bugs — four of them independently confirmed by multiple
agents — plus two UX gaps that undermine the two headline use cases
(profiles that compose, and CI linting).

## Release blockers

### RB-1: Profile composition clobbers config (HIGH, 5/7 agents)

`merge_value` (`profiles.rs`) recurses into tables only; **arrays are
replaced, scalars last-write-wins**. Consequences, each observed on real
vaults: `init --profile madr` after okf replaces `[schema] exempt` (generated
index.md/log.md immediately fail `--strict`; hoppy went 4→17 errors);
`[lint] profile` is a single scalar so the previous profile's rules silently
stop running (hoppy: 44 OKF citation warnings → 0, no trace);
`schema.default.required = ["title","type"]` silently became `["type"]`
(own KB + hoppy — validation weakened without warning); hand-written TOML
comments are stripped and the file reordered. `init --help` promises
"multiple profiles compose in one vault". Fix: union arrays, make
`[lint] profile` a list, print `conflict:` lines for scalar overwrites,
and add a stacked-profile integration test. Note one discrepancy to settle
while fixing: ff-rdp observed `[[schema.bind]]` arrays composing correctly,
the skills audit saw only the last profile's binds survive a 4-profile stack.

### RB-2: First `okf index --apply` destroys unmarked hand-written index files (HIGH, data loss)

On an `index.md` without okf markers, the first apply keeps the leading H1
and **discards all other hand-written content**; preservation only holds once
markers exist. Dry-run reports a plain "update" with no loss warning. On
macOS/Windows this targets `INDEX.md` too (same file, case-insensitive FS) —
mapl-memory's 36 KB curated index would have been destroyed had the agent not
tested on a copy first. Fix: insert markers non-destructively (preserve
existing body outside the region) or refuse with a clear message; dry-run
must warn about non-managed content.

### RB-3: Lint silently excludes files with unparseable frontmatter (HIGH, CI trust)

A file with e.g. a duplicate YAML key vanishes from lint: `0 files checked,
no issues`, exit 0, no stderr — a CI gate passes corrupt files. Related
inconsistency: `okf index`/`okf log` hard-abort (exit 2) on the first such
file anywhere in the vault, even when scoped to a subtree, while
find/summary/lint skip silently. Fix: an error-severity parse violation in
lint; skip-and-warn in the generators.

### RB-4: `changelog add` places entries outside `[Unreleased]` (HIGH)

It appends the new `### Category` after the bottom-of-file link-reference
definitions — and KaC files always end with link refs, so **every conformant
changelog** gets non-conformant output that then fails its own lint.
`changelog release` places its footer ref correctly; the defect is specific
to `add`. Minimal repro in the user-event-service agent report.

**Status: FIXED (iter-175).** `changelog add` now bounds the `[Unreleased]`
section at the footer link-ref block, so new entries land inside the section;
output stays MD047-clean. Regression tests: unit tests in `changelog.rs`
(`add_new_category_lands_before_footer_link_refs`, `add_output_is_md047_clean`)
and e2e `add_places_new_category_before_footer_link_refs_rb4`.

### RB-5: The bundled `skills` skill fails its own skills profile (HIGH, embarrassment class)

Its `description` contains a literal `<` (from `` `<name>/SKILL.md` ``),
which the profile's `description` pattern `^[^<]*$` forbids. Fix the wording
and add a CI/xtask step linting the bundled skill templates with
`--profile skills` so a self-violation can't ship again.

**Status: FIXED (iter-175).** The `skills` skill description no longer contains
`<`, and a new `cargo run -p xtask -- check-bundled-skills` gate (wired into
`quality-gates.yml`) lints every bundled `skill-*.md` as installed under the
skills profile, failing CI on any error-severity violation.

## High-impact UX gaps (fix with the blockers if possible)

- **UX-A: `**/SKILL.md` can't reach `.claude/skills/`** — the vault walker
  skips dot-directories, so the skills profile cannot see the canonical
  Claude Code skill location (4 of 5 real SKILL.md files in ff-rdp were
  unreachable). Allow-list dot-dirs for bind matching or document the
  limitation prominently.
  **Status: FIXED (iter-175).** New `[scan] include = ["glob", …]` config key
  re-admits specific hidden subtrees to the walker (honored by every command;
  `.git` stays hard-excluded). The skills profile ships
  `[scan] include = [".claude/skills/**"]`, and an ephemeral `--profile skills`
  run installs it too, so `.claude/skills/**/SKILL.md` is discoverable and
  lintable in place. e2e: `scan_include_reaches_claude_skills_dir`.
- **UX-B: skip counters are JSON-only** — the diff-aware CI pipeline
  (`git diff … | hyalo lint --files-from -`) silently dropped 41 of 43 input
  paths with zero indication in `--format text` or `--format github` output.
  One summary line ("N missing, M non-md skipped") in both formats restores
  trust in the CI recipe shipped in the README.

## Medium findings (consolidated, deduped)

- `[schema.default] required` leaks onto `[[schema.bind]]`-typed files:
  okf+skills composed → every spec-valid frontmatter-less SKILL.md/ADR errors
  "missing required property type" (2 agents). Bound types should satisfy or
  drop the base `type` requirement.
- Exempt globs are case-sensitive: the profile's `**/index.md` does not match
  `INDEX.md` on a case-insensitive FS where it *is* the reserved file.
- Flag-vs-file divergence: `[lint] profile="okf"` honors user `exempt`
  additions; the `--profile okf` CLI overlay resets them to profile builtins.
- Generated index.md violates hyalo's own MD022 (heading directly after the
  begin marker, 3 agents), and `lint --fix` + `okf index` ping-pong: the fix
  lands inside the managed region, the next regeneration reverts it — CI
  running both checks can never stabilize. Emit a blank line after the
  marker.
- `hyalo new --type <t>` ignores `[schema.types.<t>.defaults]` (4 agents;
  also the cause of empty Status/Date columns in `madr toc`, and the madr
  skill documents the defaults as if they applied).
- `madr toc <dir>` treats every .md in the dir as an ADR, ignoring the bind —
  breaks the rebind-in-place workflow; needs a type/bind filter.
- `types set default` creates a phantom `[schema.types.default]` distinct
  from `[schema.default]` — silently unused config.
- `lint --format json --detailed` caps `files[]` at 50 with no override;
  `--limit 0` (documented unlimited) returns an empty file list.
- `okf index` generates into template/fixture dirs (`_template/`,
  `test/fixture-vault/`) — the generator needs its own ignore list.
- `okf log` can't match an existing log's line format (mixes conventions);
  `OKF-CITATIONS-PRESENT` accepts only a level-1 `# Citations`, clashing with
  MADR-style single-h1 documents.
- Root `CHANGELOG.md` is unreachable when the vault dir is a subdir — the
  changelog profile binds relative to the vault; the only workaround is a
  global `--dir .`. Needs a `changelog_path` (or bind-outside-vault) story.

## Low / polish

BigQuery example types injected into every vault (4 agents — ship a neutral
profile); generators' `--format text` output is a malformed key:value flatten
(3 agents); `hyalo config` doesn't show the effective `--dir` override;
`init --help` omits the working `changelog` profile; `--claude` CLAUDE.md
section carries no profile-specific pointers; `new --type skill` emits a
non-spec `type: skill` key; changelog/body linters parse inside HTML
comments; `site_prefix=""` set silently; dead-end and redundant hints;
`--section frontmatter` could alias `--frontmatter`; index sorting is
byte-order for non-ASCII.

## Feature gaps surfaced by fallbacks to raw tools

No command to manage `[[schema.bind]]` / `[schema.default.properties]`
(every agent hand-edited TOML — a `hyalo config set`/`hyalo bind` family
would close it); `hyalo set` cannot write a string-list value (`append` only);
no body-section append (Citations sections were added with raw edits); no
`okf log` remove/undo; `new` can't derive `--file` from the type's
`filename-template`.

## What worked well

- **Performance is a non-story in the best way**: the OKF lint overlay and
  `[[schema.bind]]` add zero measurable cost; full-vault lint at 3,710 files
  ≈0.75 s, 298 files 0.045 s; `--format github` produced 1,620 well-formed
  annotations (254 KB) in ~0.7 s surviving unicode/CJK/`::%,=` content;
  unindexed MDN summary is now *faster* (1.23 s) than the old 2.9 s baseline.
- **Managed regions, idempotency, drift exit codes** verified everywhere —
  the CI contract (dry-run exits 1 on drift, 0 clean; byte-stable re-runs)
  held on every vault.
- **Deep-merge preserves non-overlapping config perfectly** (hand-tuned
  mapl config, hoppy's 5 type schemas + 8 views byte-for-byte, dir/views in
  all repos) — the clobber problem is specifically overlapping keys.
- **MADR end-to-end works**: new → fill → lint clean → toc; both advisory
  rules fire as true positives (dangling supersede, duplicate number); `mv`
  into `docs/decisions/` planned every inbound link rewrite correctly.
- **All skills-profile rules are true positives** with clear messages;
  `deinit` is safe (preserves user files, idempotent); every command the four
  bundled skills teach exists and behaves as taught (sole exception: the
  defaults claim, see RB/medium list).
- **Edge cases held**: unicode/emoji/CJK filenames and titles, CRLF files,
  `.DS_Store`, undeclared types, frontmatter-free reserved files — all
  handled; exit codes verified correct across the board.
- `changelog release` rotation + idempotency refusal worked flawlessly
  (dogfooded for real on this repo's own CHANGELOG.md conversion, PR #200).

## Conversion outcomes (branches for review, none pushed)

| Repo | Branch | State |
|------|--------|-------|
| hyalo (worktree) | `okf-conversion` (4 commits) | okf+madr merged, 12 indexes, DEC-049..051 → ADRs, 0 errors |
| hoppy | `okf-conversion` (3 commits) | valid OKF bundle, errors 9→4 (all pre-existing), 3 ADRs + toc |
| ff-rdp | `okf-conversion` (5 commits) | okf+skills+madr composed (hand-unioned exempt), 20 indexes, 2 ADRs |
| mapl-memory | `okf-conversion` (3 commits) | 0 errors/50 warns; INDEX.md kept hand-maintained (generator verdict: not a replacement without title/description migration of 293 files) |
| user-service | `okf-conversion` (3 commits) | tracked files 0 errors; adr-001 conformant in place |
| user-event-service | `okf-conversion` (3 commits) | 0 errors; root CHANGELOG.md now KaC 1.1.0 and lint-clean |

## Recommended path to v0.18.0

Fix wave (one iteration): RB-1..RB-5 + UX-A/UX-B, with regression tests —
then re-run a slim dogfood pass (profile stacking on ff-rdp branch, first
`okf index --apply` on a marker-less copy, `changelog add` on a real KaC
file, skills bind against `.claude/skills/`) before tagging. The medium list
is a solid iteration-173 backlog; the feature-gap list (config-editing
commands, set for string-lists) deserves its own design pass.
