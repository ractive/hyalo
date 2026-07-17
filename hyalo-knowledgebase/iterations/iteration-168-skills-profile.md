---
title: Iteration 168 — skills profile (Agent Skills SKILL.md)
type: iteration
date: 2026-07-16
status: completed
branch: iter-168/skills-profile
tags:
  - iteration
  - profiles
  - skills
  - agent-skills
  - schema
  - lint
related:
  - research/profile-candidates-beyond-okf.md
priority: 6
depends-on: iteration-167-madr-profile
---

# Iteration 168 — `skills` profile

Third profile (see [[profile-candidates-beyond-okf]]). The Agent Skills spec (<https://agentskills.io/specification>) has the hardest machine-checkable constraints of any surveyed standard — hyalo becomes a CI-friendly Rust validator for skill collections. Dogfood target: this repo's own `.claude/skills/` (and the SKILL.md templates hyalo itself ships in `crates/hyalo-cli/templates/`).

## Goal

`hyalo lint --profile skills` validates a directory of `<skill-name>/SKILL.md` dirs against the spec; `hyalo new` scaffolds a compliant skill.

## Steps / Tasks

**iter-167 retrospective (2026-07-17) — reusable patterns from the madr profile:**
- **Profile machinery held with zero new per-profile code** — a profile is a `templates/profile-<name>.toml` fragment + a `Profile` entry in `commands/profiles.rs` + a skill file. Follow that shape for `skills`; no new `--profile` branches.
- **New generic rule kinds cost little**: `MADR-*` advisory rules = one `commands/madr_lint.rs` (pure fns over content/dir) + a `RuleCatalogEntry` block in `hyalo-mdlint/src/engine.rs` (`default_enabled = true`) + a `<name>_profile` bool threaded from `dispatch.rs`→`lint.rs` gated by `ctx.lint_profile.as_deref() == Some("<name>")`. The skills max-length / dirname-coupling / line-budget rules should copy this wiring verbatim.
- **Property↔dirname coupling** (skills `name` == parent dir): the `[[schema.bind]]` path-glob → type map (in `hyalo-core/schema.rs`, `SchemaConfig::bound_type_for`, first-match-wins `GlobSet`) is the natural home to *bind* the skills schema to `.claude/skills/**`, but the dirname-equality check itself is a new advisory rule — model it on `MADR-DUPLICATE-NUMBER` (per-file, reads the parent dir).
- **Generators**: reuse `commands/managed_region.rs` (`Markers::new(prefix).splice(...)` + `GeneratePlan`/`read_old_content`/`apply_plan`, dry-run-exits-nonzero-on-drift) — already extracted and generic. Don't re-derive.
- **Filename tokens**: `{n:04}` zero-padding shipped in `hyalo-core/filename_template.rs` (`Placeholder::N { pad }`, `render_number`); **not actually wired into `hyalo new`** — `render_number` has no caller outside its own module, and `hyalo new --type adr` still requires the caller to supply the numbered filename via `--file`. If skills' `hyalo new --type skill` wants "next available slug" behavior, that auto-numbering logic still needs to be written from scratch (scan the target dir, compute next value, call `render_number`); don't assume iter-167 left a usable end-to-end path.

**iter-167 PR-review retrospective (2026-07-17) — process/correctness lessons from the review pass, not just the implementation:**
- **Directory-scan rules must exclude the file being linted.** iter-167's `MADR-SUPERSEDE-RESOLVE` initially treated a file's own presence in its directory as satisfying its *own* dangling-reference check (a self-referential `status: superseded by ADR-0099` on ADR 0099 incorrectly resolved) — `duplicate_number_sibling` already excluded `self_name` but the sibling-existence helper it shared didn't. Any skills rule that scans siblings/parent-dir contents (the dirname-coupling check, a future cross-skill uniqueness check) must exclude the current file explicitly and get a same-file/self-reference regression test, not just a two-file happy-path test.
- **Any user-supplied path argument needs `.replace('\\', "/")` before use in constructed paths**, even when `Path::join` would handle it transparently — a raw backslash surviving into a *displayed or written* relative path (e.g. this profile's generator output, an error message) produces a mixed-separator string on Windows. Grep the codebase for the existing idiom (`rg "replace\('\\\\\\\\', \"/\"\)"`) and apply it to every `--dir`/path-like CLI arg skills introduces, not just the ones that happen to get exercised by non-Windows CI.
- **Word acceptance-criteria checkboxes with the actual test/symbol name, not just the claim** — `ac-fidelity-check.sh`'s heuristic (and, more importantly, human/agent re-verification later) matches backtick-quoted symbols and `test_`-prefixed fn names against the diff. iter-167's ACs originally said things like "Official MADR 4 template lints clean" with no backing test at all (the existing fixture was a hand-rolled equivalent, not the literal spec template) — write the test *and* name it in the checkbox from the start, e.g. "`hyalo lint --profile skills` catches the description-length violation (`description_over_1024_chars_errors`)".
- **A task that says "record decision in [[doc]]"** is itself a checkable claim — iter-167's TOC-command-shape task said this and initially didn't do it; caught only in PR review. If a task promises a KB cross-reference, do it before ticking the box, not "implicitly satisfied by the code existing."

### 1. New generic rule kinds (capability gaps #2/#4 from the survey) [3/3]

- [x] **String max-length constraint** on properties (`max-length = 1024`) — generic `PropertyConstraint` extension, also future-proofs MyST/Windsurf
- [x] **Property↔dirname coupling rule**: property value must equal parent directory name (generic: `equals = "$parent-dir"` or a dedicated lint rule) — needed for `name`. Exclude the file itself when scanning; add a self-reference/degenerate-case regression test (see iter-167 PR-review retrospective above)
- [x] **Per-file line-budget lint** (warn above N body lines; spec recommends <500)

### 2. Profile fragment [5/5]

- [x] `[schema.types.skill]` — dispatched by path via iter-167's `[schema.bind]`: `"**/SKILL.md" = "skill"` (resolves the filename-dispatch question; see [[path-bound-schemas]])
- [x] `name`: required, pattern `^[a-z0-9]+(-[a-z0-9]+)*$`, 1–64 chars, ≠ reserved words (`anthropic`, `claude`), == parent dirname
- [x] `description`: required, 1–1024 chars, no XML tags (pattern)
- [x] Optional: `license`, `compatibility` (≤500), `metadata` (map — note: hyalo treats objects as text; validate presence only, don't type it), `allowed-tools`
- [x] Line budget: warn >500 body lines

### 3. Scaffolding [2/2]

- [x] `hyalo new --type skill` (or `--profile`-aware equivalent) creates `<name>/SKILL.md` with compliant frontmatter; name validated up front
- [x] Optional companion dirs (`scripts/`, `references/`, `assets/`) documented, not created by default

### 4. Tests [3/3]

- [x] e2e: lint this repo's own `.claude/skills/` directory — fix any violations found (dogfooding!). `crates/hyalo-cli/templates/*.md` are `skill-*.md` scaffolding sources (not literal `SKILL.md` files), so the `**/SKILL.md` glob does not apply to them — nothing to lint there.
- [x] Unit: name regex edge cases (leading/trailing/consecutive hyphens), reserved words, dirname mismatch, description length bounds
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR) [2/2]

- [x] `--profile` help lists `skills`; README profiles section extended
- [x] Update [[profile-candidates-beyond-okf]] status for skills

### 6. Retrospective (learnings-propagation — do this LAST, always) [1/1]

- [x] Review [[iteration-169-changelog-profile]] against implementation learnings (esp. how filename-based dispatch and the new rule kinds landed) — update its scope/design/tasks before starting it

## Acceptance Criteria

- [x] `hyalo lint --profile skills` catches every hard spec violation (name regex/length/dirname/reserved words, description bounds) and warns on the line budget — see `name_pattern_rejects_consecutive_hyphens`, `name_over_64_chars_errors`, `description_over_1024_chars_errors`, `description_with_xml_tag_errors`, `reserved_name_errors`, `dirname_mismatch_warns`, `line_budget_warns_above_500` in `crates/hyalo-cli/tests/e2e/skills_profile.rs`
- [x] This repo's own skills lint clean, verified by a dogfooding regression test (`this_repos_own_skills_lint_clean_under_skills_profile`, `crates/hyalo-cli/tests/e2e/skills_profile.rs`) that copies the live `.claude/skills/**/SKILL.md` files into a temp vault and asserts zero errors under `--profile skills`. Bundled templates under `crates/hyalo-cli/templates/` are named `skill-*.md` (scaffolding source, not live `SKILL.md` files), so the `**/SKILL.md` glob does not apply to them — nothing to lint there.
- [x] New rule kinds are generic, reusable by future profiles: `min-length`/`max-length` are plain `PropertyConstraint` fields on any `string` property (`crates/hyalo-core/src/schema.rs`, not skills-specific), and `SKILL-RESERVED-NAME`/`SKILL-NAME-DIRNAME`/`SKILL-LINE-BUDGET` are ordinary `RuleCatalogEntry` registrations in `hyalo-mdlint/src/engine.rs` (toggleable via `lint-rules set`, listed by `lint-rules list`) — see `skill_rules_are_generic_catalog_entries_not_hardcoded` and `skill_rule_can_be_disabled`
- [x] Quality gates pass (`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q` all green as of this PR); docs synced (`README.md` "Agent Skills profile" section, `--profile` help); retrospective applied to iter 169 (see [[iteration-169-changelog-profile]])
