---
title: "Iteration 164 — hyalo init --profile okf + bundled OKF skill"
type: iteration
date: 2026-07-16
status: planned
branch: iter-164/okf-init-profile-and-skill
tags: [iteration, okf, init, scaffolding, skill]
related: [research/okf-open-knowledge-format.md]
priority: 2
depends-on: iteration-163-okf-frontmatter-foundations
---

# Iteration 164 — `hyalo init --profile okf` + bundled OKF skill

Makes OKF a first-class, discoverable target. Depends on [[iteration-163-okf-frontmatter-foundations]] (needs `datetime-tz` + `exempt`). See [[okf-open-knowledge-format]].

**Flag naming (2026-07-16 review):** the originally proposed `init --format=okf` is **impossible** — `--format` is hyalo's global output flag (`json|text`) and propagates into every subcommand (verified: `hyalo init --format=okf` → `error: invalid value 'okf'`). Use **`--profile okf`** instead, unifying with `lint --profile okf` (iter-166): one "profile" concept across init and lint.

## Goal

`hyalo init --profile okf` scaffolds an OKF-ready vault config, and `--claude` installs an `okf` skill that turns Claude into a vendor-neutral OKF producer/maintainer using hyalo for the deterministic layer.

## Steps / Tasks

### 1. `--profile` flag on `init`

- [ ] Add `--profile <PROFILE>` (initially: `okf`) to `hyalo init` in `crates/hyalo-cli/src/commands/init.rs`; omitted = today's behavior
- [ ] **Data-driven design**: implement profiles as embedded declarative TOML fragments (schema + lint config + exemptions + template refs), not per-profile Rust code paths — `skills`/`madr`/`changelog` are queued behind okf and must be additive data (see [[profile-candidates-beyond-okf]])
- [ ] **Composable profiles**: multiple `init --profile <p>` runs must coexist in one vault (madr in `adrs/**` + changelog on `CHANGELOG.md`, [[path-bound-schemas]]) — each run *upserts* its own fragment without clobbering others; fragment shape must accommodate future `[schema.bind]` entries (lands in iter-167)
- [ ] Reject unknown profile values with a helpful error listing available profiles

### 2. OKF `.hyalo.toml` profile

- [ ] `[schema.default] required = ["type"]`
- [ ] Declare recommended props: `title:string`, `description:string`, `resource:string` (URL pattern), `tags:list`, `timestamp:datetime-tz`
- [ ] `[schema] exempt = ["**/index.md", "**/log.md"]`
- [ ] Pin `site_prefix` so bundle-absolute links (`/tables/x.md`, the spec-**recommended** §5 form) always resolve from the bundle root — guards against the auto-derived-prefix collision (bundle dir named like a top-level subdir); exact semantics decided in iter-163
- [ ] Broken-link lint rule severity = `warn` (spec forbids rejecting on broken links)
- [ ] Optionally seed common OKF `[schema.types.*]` (e.g. `"BigQuery Table"`, `"BigQuery Dataset"`, `Reference`) with recommended `required-sections` like `# Schema`, `# Citations` — verify TOML quoted keys with spaces work end-to-end
- [ ] `validate_on_write = true` so authoring stays conformant

### 3. Bundled `okf` skill

- [ ] Add `templates/skill-hyalo-okf.md` (+ pi variant if parity is expected) embedded in the binary
- [ ] Skill teaches: OKF concept model, reserved-file rules, link forms (bundle-absolute recommended for concept cross-links), the deterministic-vs-LLM split, and the exact hyalo commands (`find --property type=`, `okf index`, `okf log`, `lint --profile okf`, `new --type`)
- [ ] `hyalo init --profile okf --claude` installs the skill + `.claude` hints referencing OKF workflow

### 4. Tests

- [ ] e2e: `init --profile okf` in a temp dir writes expected `.hyalo.toml`; a scaffolded concept validates; `init --profile okf --claude` installs the skill
- [ ] e2e: default `init` output byte-identical to pre-change (no regression); unknown `--profile` value errors cleanly
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR)

- [ ] `hyalo init --help` documents `--profile`
- [ ] README.md: an "OKF (Open Knowledge Format)" section with the quickstart (`hyalo init --profile okf --claude`)
- [ ] Update [[okf-open-knowledge-format]] gap #3/#6 status
- [ ] Keep the new skill in sync with README/help (house rule)

### 6. Retrospective (learnings-propagation — do this LAST, always)

- [ ] Review the remaining profile iterations ([[iteration-165-okf-index-and-log-generators]] through [[iteration-169-changelog-profile]]) against implementation learnings — especially whether the data-driven profile machinery is generic enough for madr/skills/changelog — update their scope/design/tasks before starting the next iteration

## Acceptance Criteria

- [ ] `hyalo init --profile okf` produces a config under which a real OKF sample bundle lints clean
- [ ] The `okf` skill is installed by `--claude` and references only real, current hyalo commands
- [ ] Default `init` behavior unchanged; global `--format json|text` still works on `init`
- [ ] Quality gates pass; docs + skill updated in the same PR
