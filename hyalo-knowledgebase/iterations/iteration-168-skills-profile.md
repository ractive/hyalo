---
title: "Iteration 168 — skills profile (Agent Skills SKILL.md)"
type: iteration
date: 2026-07-16
status: planned
branch: iter-168/skills-profile
tags: [iteration, profiles, skills, agent-skills, schema, lint]
related: [research/profile-candidates-beyond-okf.md]
priority: 6
depends-on: iteration-167-madr-profile
---

# Iteration 168 — `skills` profile

Third profile (see [[profile-candidates-beyond-okf]]). The Agent Skills spec (<https://agentskills.io/specification>) has the hardest machine-checkable constraints of any surveyed standard — hyalo becomes a CI-friendly Rust validator for skill collections. Dogfood target: this repo's own `.claude/skills/` (and the SKILL.md templates hyalo itself ships in `crates/hyalo-cli/templates/`).

## Goal

`hyalo lint --profile skills` validates a directory of `<skill-name>/SKILL.md` dirs against the spec; `hyalo new` scaffolds a compliant skill.

## Steps / Tasks

### 1. New generic rule kinds (capability gaps #2/#4 from the survey)

- [ ] **String max-length constraint** on properties (`max-length = 1024`) — generic `PropertyConstraint` extension, also future-proofs MyST/Windsurf
- [ ] **Property↔dirname coupling rule**: property value must equal parent directory name (generic: `equals = "$parent-dir"` or a dedicated lint rule) — needed for `name`
- [ ] **Per-file line-budget lint** (warn above N body lines; spec recommends <500)

### 2. Profile fragment

- [ ] `[schema.types.skill]` — dispatched by path via iter-167's `[schema.bind]`: `"**/SKILL.md" = "skill"` (resolves the filename-dispatch question; see [[path-bound-schemas]])
- [ ] `name`: required, pattern `^[a-z0-9]+(-[a-z0-9]+)*$`, 1–64 chars, ≠ reserved words (`anthropic`, `claude`), == parent dirname
- [ ] `description`: required, 1–1024 chars, no XML tags (pattern)
- [ ] Optional: `license`, `compatibility` (≤500), `metadata` (map — note: hyalo treats objects as text; validate presence only, don't type it), `allowed-tools`
- [ ] Line budget: warn >500 body lines

### 3. Scaffolding

- [ ] `hyalo new --type skill` (or `--profile`-aware equivalent) creates `<name>/SKILL.md` with compliant frontmatter; name validated up front
- [ ] Optional companion dirs (`scripts/`, `references/`, `assets/`) documented, not created by default

### 4. Tests

- [ ] e2e: lint this repo's `.claude/skills/` and the bundled templates — fix any violations found (dogfooding!)
- [ ] Unit: name regex edge cases (leading/trailing/consecutive hyphens), reserved words, dirname mismatch, description length bounds
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [ ] `--profile` help lists `skills`; README profiles section extended
- [ ] Update [[profile-candidates-beyond-okf]] status for skills

### 6. Retrospective (learnings-propagation — do this LAST, always)

- [ ] Review [[iteration-169-changelog-profile]] against implementation learnings (esp. how filename-based dispatch and the new rule kinds landed) — update its scope/design/tasks before starting it

## Acceptance Criteria

- [ ] `hyalo lint --profile skills` catches every hard spec violation (name regex/length/dirname/reserved words, description bounds) and warns on the line budget
- [ ] This repo's own skills + bundled templates lint clean (after fixing real findings)
- [ ] New rule kinds (max-length, dirname-coupling, line budget) are generic, reusable by future profiles
- [ ] Quality gates pass; docs synced; retrospective applied to iter 169
