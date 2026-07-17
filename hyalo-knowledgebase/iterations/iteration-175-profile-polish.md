---
title: Iteration 175 — profile content & tooling polish (changelog add, skills reach, neutral okf, scaffold defaults)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - profiles
  - fix-wave
status: planned
branch: iter-175/profile-polish
---

# Iteration 175 — profile content & tooling polish

## Goal

Close out the remaining release blockers and the sharp edges that make the
profiles awkward on real repos: `changelog add` produces conformant output,
the bundled skills pass their own profile (with a CI gate so that stays
true), the skills profile can reach `.claude/skills/`, root `CHANGELOG.md`
is addressable, the OKF profile is vendor-neutral, and scaffolding honors
schema defaults. Fixes **RB-4**, **RB-5**, **UX-A** and the remaining
mediums from [[dogfood-results/dogfood-v0180-okf-profiles-pre-release]].

## Decisions (taken 2026-07-17, do not re-litigate — see DEC-052)

- **Dot-dir reach via a general walker include-list** (`[scan] include`
  globs), shipped by the skills profile — not a hard-coded `.claude` case.
- **Root changelog via `[changelog] path`**, resolved relative to the config
  file's directory (may point outside the vault dir; changelog commands
  only).
- **Neutral OKF profile**: no BigQuery/Reference example types in the
  shipped fragment.

## Note from iter-174

No scope overlap — iter-174 was lint/CI-trust only (HYALO005 parse-error
gate, honest caps, skip visibility, fix-mode distinguishability); it didn't
touch profiles, `changelog`, `hyalo new`, or `madr toc`. Two things worth
reusing as precedent when implementing this iteration's tasks:

- **Config-driven walker filtering precedent**: `[lint] ignore` exclusion
  (with a visible notice when it drops an explicitly named `--file`, see
  `crates/hyalo-cli/src/dispatch.rs` around the `lint_ignore` filter) is the
  closest existing pattern to task 3's `[scan] include` — same shape
  (glob-set match against vault-relative paths), opposite direction
  (include instead of exclude). Reuse the glob-set-building helper rather
  than re-deriving it.
- **Don't duplicate root-cause helpers**: iter-174 added `terse_root_cause`
  in `lint.rs` as a near-copy of `commands::okf::root_cause` instead of
  lifting it to a shared location (own plan note said to reuse/lift it, but
  it shipped duplicated under time pressure). If task 5 or 6 touches error
  message rendering, consider consolidating both into one shared helper
  instead of adding a third copy.

No other scope adaptation needed — this iteration's tasks stand as written.

## Tasks

### 1. `changelog add` placement (RB-4)

- [ ] Insert the new `### Category` + entry INSIDE `## [Unreleased]`, before
  the footer link-reference block — never after it (user-event-service
  minimal repro becomes the e2e); no trailing-newline damage (MD047 clean)

### 2. Bundled skills pass their own profile (RB-5)

- [ ] Reword the `skills` skill description to drop the literal `<`
  (e.g. "a name/SKILL.md directory") and re-verify all four profile skills
  + base skills pass `lint --profile skills`
- [ ] New xtask gate `check-bundled-skills`: lints
  `crates/hyalo-cli/templates/skill-*.md` (as installed) with the skills
  profile; wired into `quality-gates.yml` CI so a self-violation can't ship

### 3. Reach `.claude/skills/` (UX-A)

- [ ] New `[scan] include = ["glob", ...]` config key: the vault walker
  descends into otherwise-skipped dot-paths matching the globs (never
  `.git/**` — hard-excluded); honored by all commands
- [ ] The skills profile fragment ships
  `[scan] include = [".claude/skills/**"]`
- [ ] e2e on a fixture repo: SKILL.md files under `.claude/skills/` are
  linted by the skills profile without moving them (ff-rdp U1 scenario)

### 4. Root `CHANGELOG.md` reachability

- [ ] `[changelog] path = "../CHANGELOG.md"` (default `CHANGELOG.md` in the
  vault dir): resolved from the config file's directory; used by
  `lint --profile changelog`, `changelog add`, `changelog release`
- [ ] Path is validated (no traversal above the config dir's repo root —
  reuse the mv vault-escape checks); clear error when the file is absent
- [ ] The changelog profile's `init` writes the key when a root
  `CHANGELOG.md` exists and the vault dir is a subdir (the common case that
  hit user-event-service)

### 5. Neutral OKF profile + scaffold fidelity

- [ ] Remove `BigQuery Dataset` / `BigQuery Table` / `Reference` example
  types from `profile-okf.toml` (4 agents flagged them as junk in real
  vaults); document in the okf skill how to add domain types
- [ ] `hyalo new --type <t>` applies `[schema.types.<t>.defaults]`
  (incl. `$today`) to the scaffold (4 agents; also fixes the empty
  Status/Date columns in `madr toc` and makes the madr skill's claims true)
- [ ] `hyalo new` omits the explicit `type:` key when the target path is
  covered by a `[[schema.bind]]` for that type (relies on iter-172's
  bind-=-typing; fixes the non-spec `type: skill` key in SKILL.md scaffolds)
- [ ] `madr toc` includes only files typed/bound `adr` (user-service repro:
  13 concept files polluted the dashboard)
- [ ] `types set default` rejects the reserved name with an error pointing at
  `[schema.default]` (phantom-type trap, user-service BUG-A3)

### 6. Small fixes bundle

- [ ] `init --help` lists the `changelog` profile (it works but is
  undocumented)
- [ ] `--claude` CLAUDE.md managed section gains one profile-specific pointer
  line per installed profile (e.g. okf → `hyalo okf index` drift check)
- [ ] Suppress the redundant `lint --profile <p>` hint when the config's
  `profiles` already activates `<p>` (hoppy UX-2)
- [ ] `hyalo config` reports the EFFECTIVE dir when `--dir` overrides the
  config (ff-rdp B6)

### 7. Tests

- [ ] e2e per task group above (changelog placement, xtask gate red/green,
  dot-dir include, changelog path, defaults scaffold, toc filter,
  types-set-default rejection)
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 8. Docs sync (same PR)

- [ ] README (profiles at-a-glance, changelog section, skills section),
  affected `--help` texts, and the four profile skill templates all updated
  to the shipped behavior
- [ ] Retrospective: write the fix-wave summary into the KB and update
  [[dogfood-results/dogfood-v0180-okf-profiles-pre-release]] finding
  statuses (fixed / deferred)

## Acceptance criteria

- [ ] `changelog add` on a conformant KaC file yields a conformant file, every
  time
- [ ] CI fails if any bundled skill violates the skills profile
- [ ] A stock Claude Code repo's `.claude/skills/` is lintable via the skills
  profile without relocating files
- [ ] `hyalo --profile changelog` workflows work on a repo-root CHANGELOG.md
  with the vault dir set to a docs subdir, without `--dir .` gymnastics
- [ ] `init --profile okf` no longer injects vendor example types
- [ ] After this iteration, the slim re-dogfood checklist in task #10 passes
  and v0.18.0 is releasable
