---
title: Iteration 167 — madr profile (Markdown Architecture Decision Records)
type: iteration
date: 2026-07-16
status: completed
branch: iter-167/madr-profile
tags:
  - iteration
  - profiles
  - madr
  - adr
  - schema
  - lint
related:
  - research/profile-candidates-beyond-okf.md
priority: 5
depends-on: iteration-166-okf-conformance-lint
---

# Iteration 167 — `madr` profile

Second profile after okf (see [[profile-candidates-beyond-okf]]). MADR 4.0.0 (<https://adr.github.io/madr/>) is typed docs with a status lifecycle — exactly hyalo's sweet spot — and the first real test that iter-164's data-driven profile machinery holds: this profile MUST be additive data + at most small new rule kinds, no new per-profile code paths.

This iteration also ships **`[schema.bind]` — path-bound schemas** (design: [[path-bound-schemas]]), with madr as first consumer: ADRs usually live in a subdirectory (`docs/decisions/**`) of a larger vault, so the profile must apply to *those files only*. Iters 168/169 consume the same mechanism.

## Goal

`hyalo init --profile madr` + `hyalo lint` make an ADR directory inside a larger vault schema-valid, scaffoldable via `hyalo new --type adr --file <NNNN-slug.md>` (caller supplies the number; auto-numbering deferred), and lintable — with a generated TOC/status dashboard (parity with `adr generate toc`).

## Steps / Tasks

### 0. `[schema.bind]` — path-bound schemas (generic mechanism)

- [x] `[schema.bind]` ordered path-glob → type map in `.hyalo.toml`; compiled to `GlobSet` at schema-load; first match wins; unknown target type → config warning (design + citations in [[path-bound-schemas]])
- [x] Binding assigns the **effective schema** during lint/`validate_on_write` when `type:` frontmatter is absent (extends the existing `infer_type_from_path` seam, `crates/hyalo-cli/src/commands/lint.rs:776`) — not just `--fix`-time inference
- [x] Precedence: explicit `type:` frontmatter wins; frontmatter↔binding mismatch → new warn-level lint rule
- [x] `lint --fix` type insertion consults bindings after `filename-template`
- [x] Cross-platform glob matching (reuse centralized `globset` infra), tests for ordering/first-match/ambiguity

### 1. Profile fragment (data-driven)

- [x] `[schema.types.adr]`: `status` enum `proposed|rejected|accepted|deprecated` + superseded pattern (`superseded by ADR-\d{4}`), `date`, `decision-makers`/`consulted`/`informed` string-lists — all optional-but-typed per MADR 4
- [x] Profile writes a bind entry for the ADR directory (`init --profile madr` derives/asks the path; default `docs/decisions/**`) so the schema applies to those files only, inside any larger vault
- [x] Required sections: `## Context and Problem Statement`, `## Considered Options`, `## Decision Outcome`
- [x] `filename-template` producing `NNNN-slug.md` — check whether `{n}` zero-pads; if not, add padding syntax (e.g. `{n:04}`) to `crates/hyalo-core` template expansion (generally useful, not madr-specific)
- [x] MADR 3.x variant: accept `deciders` (3.x) alongside `decision-makers` (4.x) — decide mechanism (alias in schema vs second profile `madr3`); document the choice
- [x] Nygard/adr-tools variant (headings-only, no frontmatter): **defer to after iter-169** — needs the heading-grammar lint mode; record in [[profile-candidates-beyond-okf]]

### 2. Lints

- [x] Supersede cross-check: `status: superseded by ADR-0123` → warn if `0123-*.md` doesn't exist in the ADR dir
- [x] Filename↔content coupling: warn when the `NNNN` prefix duplicates an existing ADR number (capability gap #2 from the survey — first minimal cut)

**iter-166 note (2026-07-17) — reusable lint-wiring pattern:** iter-166's `OKF-*` rules established the shape new advisory rules should follow; copy it rather than re-deriving: (1) register each rule as a `RuleCatalogEntry` in `hyalo-mdlint/src/engine.rs` with `default_enabled = true` so `lint-rules list`/`show`/`set --enabled false` work and round-trip a real override even when the rule is "on by default"; (2) gate actual execution on the active profile at the CLI layer (`lint.rs`'s `okf_profile: bool` flag threaded from `dispatch.rs`), not inside the rule itself; (3) respect `[lint.rules.<ID>]` enable/severity overrides *and* `--rule`/`--rule-prefix` filters via a single `is_enabled` predicate closure passed into the rule-running function, so profile rules compose with the generic lint surface for free. Also reuse `overlay_profile()` (`config.rs`) verbatim for the `--profile madr` overlay — don't re-derive the merge-then-reparse flow, and note the OR-vs-overwrite bug iter-166 fixed there (the overlay's `lint_strict` must be taken as-is post-merge, not ORed with the pre-overlay value, or a future fragment that means to leave `strict` unset would incorrectly inherit `true` from an unrelated existing config).

### 3. TOC / dashboard generator

- [x] Decide command shape: per-profile group `hyalo madr toc` (consistent with `hyalo okf index`) vs a generic profile-keyed generator — leaning per-profile group; record decision in [[okf-open-knowledge-format]] CLI-design section
- [x] Generate a TOC/index of ADRs (number, title, status, date), dry-run default + `--apply`, idempotent, managed-region markers (reuse iter-165 machinery). **iter-165 note:** the managed-region splice (`<!-- okf:index:begin/end -->` markers, prose-preserving `splice_managed_region`) and the plan/diff → dry-run-exits-nonzero-on-drift pattern live in `crates/hyalo-cli/src/commands/okf.rs` but are currently module-private and OKF-marker-specific. Extract a small shared helper (parametrize the marker prefix, e.g. `<!-- madr:toc:begin -->`) rather than copy-pasting; the `IndexPlan { rel_path, new_content, old_content }` + `changed()`/`is_new()` shape and the `atomic_write` apply loop carry over directly. **iter-166 note (2026-07-17):** when extracting that shared helper, carry over `okf_lint.rs`'s marker-anchoring fix too — `check_index_structure` finds the *first* `BEGIN` then requires `END` to appear strictly after it (`content[begin + BEGIN.len()..].contains(END)`), not a naive first-occurrence `str::find` on either marker independently, so a marker string merely mentioned in prose above the real managed region can't be mistaken for it. Same class of bug iter-165 hit in `splice_managed_region`; keep both anchored on structural position.

### 4. Tests

- [x] e2e: `init --profile madr` in a temp dir; `hyalo new --type adr --file docs/decisions/NNNN-slug.md` scaffolds the ADR skeleton at a caller-supplied `NNNN-slug.md` path (auto-computing "the next number" is **not** implemented — `--file` requires the number explicitly; deferred, see [[iteration-168-skills-profile]] retrospective note); lint passes on the MADR 4 template examples from the spec repo
- [x] e2e: supersede cross-check fires on a dangling reference; 3.x `deciders` accepted per the chosen mechanism
- [x] e2e: CRLF-terminated ADR files (frontmatter + TOC managed region) lint/generate identically to LF. **iter-166 note (2026-07-17):** CRLF has now bitten new `okf` code twice (iter-165's `find_heading`/`prepend_log_entry`, iter-166's date-heading/citation scanners) — treat it as a standing requirement for any new line-oriented parsing here, not an edge case to remember later.
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [x] `--profile` help lists `madr`; README profiles section extended
- [x] Bundled skill(s) updated if they enumerate profiles
- [x] Update [[profile-candidates-beyond-okf]] status for madr

### 6. Retrospective (learnings-propagation — do this LAST, always)

- [x] Review [[iteration-168-skills-profile]] and [[iteration-169-changelog-profile]] against what implementation taught us (profile-machinery fit, new-rule-kind cost, generator patterns) — update their scope/design/tasks before starting the next iteration

## Acceptance Criteria

- [x] `hyalo init --profile madr` + `new --type adr` + `lint --profile madr` work end-to-end on a fresh ADR directory
- [x] The profile is pure data over iter-164 machinery (any new rule kinds are generic, not madr-only code) — `MADR-SUPERSEDE-RESOLVE`/`MADR-DUPLICATE-NUMBER` are ordinary `RuleCatalogEntry`s exposed via `lint-rules list` and suppressible via the generic `[lint.rules.*]` override, verified by `madr_rules_are_generic_catalog_entries_not_hardcoded`
- [x] Official MADR 4 template lints clean; dangling supersede reference warns — `official_madr4_short_template_lints_clean` uses the real MADR 4.0.0 short-template structure; `dangling_supersede_warns` covers the supersede check
- [x] Quality gates pass; docs synced; retrospective applied to iters 168–169 — `cargo fmt`/`clippy -D warnings`/`cargo test --workspace -q` all green; README `madr` section + `skill-hyalo-madr.md` updated; iter-168/iter-169 plans carry an "iter-167 retrospective" section
