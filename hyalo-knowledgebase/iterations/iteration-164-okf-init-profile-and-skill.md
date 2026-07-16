---
title: "Iteration 164 — hyalo init --format=okf + bundled OKF skill"
type: iteration
date: 2026-07-16
status: planned
branch: iter-164/okf-init-profile-and-skill
tags: [iteration, okf, init, scaffolding, skill]
related: [research/okf-open-knowledge-format.md]
priority: 2
depends-on: iteration-163-okf-frontmatter-foundations
---

# Iteration 164 — `hyalo init --format=okf` + bundled OKF skill

Makes OKF a first-class, discoverable target. Depends on [[iteration-163-okf-frontmatter-foundations]] (needs `datetime-tz` + `exempt`). See [[okf-open-knowledge-format]].

## Goal

`hyalo init --format=okf` scaffolds an OKF-ready vault config, and `--claude` installs an `okf` skill that turns Claude into a vendor-neutral OKF producer/maintainer using hyalo for the deterministic layer.

## Steps / Tasks

### 1. `--format` flag on `init`

- [ ] Add `--format <default|okf>` to `hyalo init` in `crates/hyalo-cli/src/commands/init.rs` (default preserves today's behavior)
- [ ] Refactor init scaffolding so format-specific `.hyalo.toml` fragments are selectable (avoid a hard fork of the whole command)

### 2. OKF `.hyalo.toml` profile

- [ ] `[schema.default] required = ["type"]`
- [ ] Declare recommended props: `title:string`, `description:string`, `resource:string` (URL pattern), `tags:list`, `timestamp:datetime-tz`
- [ ] `[schema] exempt = ["**/index.md", "**/log.md"]`
- [ ] Broken-link lint rule severity = `warn` (spec forbids rejecting on broken links)
- [ ] Optionally seed common OKF `[schema.types.*]` (e.g. `"BigQuery Table"`, `"BigQuery Dataset"`, `Reference`) with recommended `required-sections` like `# Schema`, `# Citations` — verify TOML quoted keys with spaces work end-to-end
- [ ] `validate_on_write = true` so authoring stays conformant

### 3. Bundled `okf` skill

- [ ] Add `templates/skill-hyalo-okf.md` (+ pi variant if parity is expected) embedded in the binary
- [ ] Skill teaches: OKF concept model, reserved-file rules, the deterministic-vs-LLM split, and the exact hyalo commands (`find --property type=`, `okf index`, `okf log`, `lint --profile okf`, `new --type`)
- [ ] `hyalo init --format=okf --claude` installs the skill + `.claude` hints referencing OKF workflow

### 4. Tests

- [ ] e2e: `init --format=okf` in a temp dir writes expected `.hyalo.toml`; a scaffolded concept validates; `init --format=okf --claude` installs the skill
- [ ] e2e: default `init` output byte-identical to pre-change (no regression)
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [ ] `hyalo init --help` documents `--format`
- [ ] README.md: an "OKF (Open Knowledge Format)" section with the quickstart (`hyalo init --format=okf --claude`)
- [ ] Update [[okf-open-knowledge-format]] gap #3/#6 status
- [ ] Keep the new skill in sync with README/help (house rule)

## Acceptance Criteria

- [ ] `hyalo init --format=okf` produces a config under which a real OKF sample bundle lints clean
- [ ] The `okf` skill is installed by `--claude` and references only real, current hyalo commands
- [ ] Default `init` behavior unchanged
- [ ] Quality gates pass; docs + skill updated in the same PR
