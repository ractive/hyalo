---
title: "Iteration 167 — madr profile (Markdown Architecture Decision Records)"
type: iteration
date: 2026-07-16
status: planned
branch: iter-167/madr-profile
tags: [iteration, profiles, madr, adr, schema, lint]
related: [research/profile-candidates-beyond-okf.md]
priority: 5
depends-on: iteration-166-okf-conformance-lint
---

# Iteration 167 — `madr` profile

Second profile after okf (see [[profile-candidates-beyond-okf]]). MADR 4.0.0 (<https://adr.github.io/madr/>) is typed docs with a status lifecycle — exactly hyalo's sweet spot — and the first real test that iter-164's data-driven profile machinery holds: this profile MUST be additive data + at most small new rule kinds, no new per-profile code paths.

This iteration also ships **`[schema.bind]` — path-bound schemas** (design: [[path-bound-schemas]]), with madr as first consumer: ADRs usually live in a subdirectory (`docs/decisions/**`) of a larger vault, so the profile must apply to *those files only*. Iters 168/169 consume the same mechanism.

## Goal

`hyalo init --profile madr` + `hyalo lint` make an ADR directory inside a larger vault schema-valid, scaffoldable with auto-numbered filenames, and lintable — with a generated TOC/status dashboard (parity with `adr generate toc`).

## Steps / Tasks

### 0. `[schema.bind]` — path-bound schemas (generic mechanism)

- [ ] `[schema.bind]` ordered path-glob → type map in `.hyalo.toml`; compiled to `GlobSet` at schema-load; first match wins; unknown target type → config warning (design + citations in [[path-bound-schemas]])
- [ ] Binding assigns the **effective schema** during lint/`validate_on_write` when `type:` frontmatter is absent (extends the existing `infer_type_from_path` seam, `crates/hyalo-cli/src/commands/lint.rs:776`) — not just `--fix`-time inference
- [ ] Precedence: explicit `type:` frontmatter wins; frontmatter↔binding mismatch → new warn-level lint rule
- [ ] `lint --fix` type insertion consults bindings after `filename-template`
- [ ] Cross-platform glob matching (reuse centralized `globset` infra), tests for ordering/first-match/ambiguity

### 1. Profile fragment (data-driven)

- [ ] `[schema.types.adr]`: `status` enum `proposed|rejected|accepted|deprecated` + superseded pattern (`superseded by ADR-\d{4}`), `date`, `decision-makers`/`consulted`/`informed` string-lists — all optional-but-typed per MADR 4
- [ ] Profile writes a bind entry for the ADR directory (`init --profile madr` derives/asks the path; default `docs/decisions/**`) so the schema applies to those files only, inside any larger vault
- [ ] Required sections: `## Context and Problem Statement`, `## Considered Options`, `## Decision Outcome`
- [ ] `filename-template` producing `NNNN-slug.md` — check whether `{n}` zero-pads; if not, add padding syntax (e.g. `{n:04}`) to `crates/hyalo-core` template expansion (generally useful, not madr-specific)
- [ ] MADR 3.x variant: accept `deciders` (3.x) alongside `decision-makers` (4.x) — decide mechanism (alias in schema vs second profile `madr3`); document the choice
- [ ] Nygard/adr-tools variant (headings-only, no frontmatter): **defer to after iter-169** — needs the heading-grammar lint mode; record in [[profile-candidates-beyond-okf]]

### 2. Lints

- [ ] Supersede cross-check: `status: superseded by ADR-0123` → warn if `0123-*.md` doesn't exist in the ADR dir
- [ ] Filename↔content coupling: warn when the `NNNN` prefix duplicates an existing ADR number (capability gap #2 from the survey — first minimal cut)

### 3. TOC / dashboard generator

- [ ] Decide command shape: per-profile group `hyalo madr toc` (consistent with `hyalo okf index`) vs a generic profile-keyed generator — leaning per-profile group; record decision in [[okf-open-knowledge-format]] CLI-design section
- [ ] Generate a TOC/index of ADRs (number, title, status, date), dry-run default + `--apply`, idempotent, managed-region markers (reuse iter-165 machinery). **iter-165 note:** the managed-region splice (`<!-- okf:index:begin/end -->` markers, prose-preserving `splice_managed_region`) and the plan/diff → dry-run-exits-nonzero-on-drift pattern live in `crates/hyalo-cli/src/commands/okf.rs` but are currently module-private and OKF-marker-specific. Extract a small shared helper (parametrize the marker prefix, e.g. `<!-- madr:toc:begin -->`) rather than copy-pasting; the `IndexPlan { rel_path, new_content, old_content }` + `changed()`/`is_new()` shape and the `atomic_write` apply loop carry over directly. **iter-166 note (2026-07-17):** when extracting that shared helper, carry over `okf_lint.rs`'s marker-anchoring fix too — `check_index_structure` finds the *first* `BEGIN` then requires `END` to appear strictly after it (`content[begin + BEGIN.len()..].contains(END)`), not a naive first-occurrence `str::find` on either marker independently, so a marker string merely mentioned in prose above the real managed region can't be mistaken for it. Same class of bug iter-165 hit in `splice_managed_region`; keep both anchored on structural position.

### 4. Tests

- [ ] e2e: `init --profile madr` in a temp dir; `hyalo new --type adr` produces `NNNN-slug.md` with the next number; lint passes on the MADR 4 template examples from the spec repo
- [ ] e2e: supersede cross-check fires on a dangling reference; 3.x `deciders` accepted per the chosen mechanism
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [ ] `--profile` help lists `madr`; README profiles section extended
- [ ] Bundled skill(s) updated if they enumerate profiles
- [ ] Update [[profile-candidates-beyond-okf]] status for madr

### 6. Retrospective (learnings-propagation — do this LAST, always)

- [ ] Review [[iteration-168-skills-profile]] and [[iteration-169-changelog-profile]] against what implementation taught us (profile-machinery fit, new-rule-kind cost, generator patterns) — update their scope/design/tasks before starting the next iteration

## Acceptance Criteria

- [ ] `hyalo init --profile madr` + `new --type adr` + `lint --profile madr` work end-to-end on a fresh ADR directory
- [ ] The profile is pure data over iter-164 machinery (any new rule kinds are generic, not madr-only code)
- [ ] Official MADR 4 template lints clean; dangling supersede reference warns
- [ ] Quality gates pass; docs synced; retrospective applied to iters 168–169
