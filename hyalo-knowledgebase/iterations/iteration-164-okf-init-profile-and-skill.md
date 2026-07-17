---
title: Iteration 164 — hyalo init --profile okf + bundled OKF skill
type: iteration
date: 2026-07-16
status: completed
branch: iter-164/okf-init-profile-and-skill
tags:
  - iteration
  - okf
  - init
  - scaffolding
  - skill
related:
  - research/okf-open-knowledge-format.md
priority: 2
depends-on: iteration-163-okf-frontmatter-foundations
---

# Iteration 164 — `hyalo init --profile okf` + bundled OKF skill

Makes OKF a first-class, discoverable target. Depends on [[iteration-163-okf-frontmatter-foundations]] (needs `datetime-tz` + `exempt`). See [[okf-open-knowledge-format]].

**Flag naming (2026-07-16 review):** the originally proposed `init --format=okf` is **impossible** — `--format` is hyalo's global output flag (`json|text`) and propagates into every subcommand (verified: `hyalo init --format=okf` → `error: invalid value 'okf'`). Use **`--profile okf`** instead, unifying with `lint --profile okf` (iter-166): one "profile" concept across init and lint.

**iter-163 retrospective (2026-07-17):** all the foundation primitives this iteration depends on landed exactly as anticipated — `PropertyConstraint::DateTimeTz` (`crates/hyalo-core/src/schema.rs`), `SchemaConfig.exempt: ExemptGlobs` with `[schema] exempt = [...]` parsing (also in `schema.rs`, not `hyalo-cli/src/config.rs` — the raw-TOML-to-`SchemaConfig` conversion lives in `hyalo-core`; `hyalo-cli/src/commands/config.rs` only surfaces it for `hyalo config` output), and `site_prefix = ""` confirmed correct for bundle-root link resolution (no new explicit form needed — item 37 already reflects this). No API surprises to account for; item 2's `.hyalo.toml` fragment can be written directly against the shipped schema. One reusable asset: `crates/hyalo-cli/tests/e2e/lint.rs::okf_schema_toml()` is a working `dir = "."` + `site_prefix = ""` + `[schema] exempt` + `datetime-tz` fixture — iter-164's e2e tests (item 4) can crib its shape for the `init --profile okf`-generated config instead of re-deriving the TOML from scratch.

## Goal

`hyalo init --profile okf` scaffolds an OKF-ready vault config, and `--claude` installs an `okf` skill that turns Claude into a vendor-neutral OKF producer/maintainer using hyalo for the deterministic layer.

## Steps / Tasks

### 1. `--profile` flag on `init` [4/4]

- [x] Add `--profile <PROFILE>` (initially: `okf`) to `hyalo init` in `crates/hyalo-cli/src/commands/init.rs`; omitted = today's behavior
- [x] **Data-driven design**: implement profiles as embedded declarative TOML fragments (schema + lint config + exemptions + template refs), not per-profile Rust code paths — `skills`/`madr`/`changelog` are queued behind okf and must be additive data (see [[profile-candidates-beyond-okf]])
- [x] **Composable profiles**: multiple `init --profile <p>` runs must coexist in one vault (madr in `adrs/**` + changelog on `CHANGELOG.md`, [[path-bound-schemas]]) — each run *upserts* its own fragment without clobbering others; fragment shape must accommodate future `[schema.bind]` entries (lands in iter-167)
- [x] Reject unknown profile values with a helpful error listing available profiles

### 2. OKF `.hyalo.toml` profile [7/7]

- [x] `[schema.default] required = ["type"]`
- [x] Declare recommended props: `title:string`, `description:string`, `resource:string` (URL pattern), `tags:list`, `timestamp:datetime-tz`
- [x] `[schema] exempt = ["**/index.md", "**/log.md"]`
- [x] Set `site_prefix = ""` so bundle-absolute links (`/tables/x.md`, the spec-**recommended** §5 form) resolve from the bundle root — iter-163 confirmed `""` (→ `None`) strips only the leading `/`, avoiding the auto-derived-prefix collision (bundle dir named like a top-level subdir); no new explicit form was needed
- [x] Broken-link lint rule severity = `warn` (spec forbids rejecting on broken links) — **satisfied by construction, no config added**: `hyalo lint` never errors on broken cross-file links (that's the advisory `find --broken-links`); no severity override exists or was needed. See retrospective.
- [x] Optionally seed common OKF `[schema.types.*]` (e.g. `"BigQuery Table"`, `"BigQuery Dataset"`, `Reference`) with recommended `required-sections` like `# Schema`, `# Citations` — verify TOML quoted keys with spaces work end-to-end
- [x] `validate_on_write = true` so authoring stays conformant

### 3. Bundled `okf` skill [3/3]

- [x] Add `templates/skill-hyalo-okf.md` (+ pi variant if parity is expected) embedded in the binary
- [x] Skill teaches: OKF concept model, reserved-file rules, link forms (bundle-absolute recommended for concept cross-links), the deterministic-vs-LLM split, and the exact hyalo commands — **scope corrected in-flight**: only shipped commands are referenced (`find --property type=`, `new --type`, `lint --strict`, `set`/`append`, `mv`, `find --broken-links`); `okf index`/`okf log`/`lint --profile okf` don't exist yet (iters 165–166) and are deliberately omitted rather than taught as vaporware. See retrospective.
- [x] `hyalo init --profile okf --claude` installs the skill at `.claude/skills/okf/SKILL.md` — note: the base `.claude/CLAUDE.md` managed-section hint stays generic (not OKF-specific); the skill file itself is the OKF workflow reference

### 4. Tests [3/3]

- [x] e2e: `init --profile okf` in a temp dir writes expected `.hyalo.toml`; a scaffolded concept validates; `init --profile okf --claude` installs the skill
- [x] e2e: default `init` output byte-identical to pre-change (no regression); unknown `--profile` value errors cleanly
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR) [4/4]

- [x] `hyalo init --help` documents `--profile`
- [x] README.md: an "OKF (Open Knowledge Format)" section with the quickstart (`hyalo init --profile okf --claude`)
- [x] Update [[okf-open-knowledge-format]] gap #3/#6 status
- [x] Keep the new skill in sync with README/help (house rule)

### 6. Retrospective (learnings-propagation — do this LAST, always) [1/1]

- [x] Review the remaining profile iterations ([[iteration-165-okf-index-and-log-generators]] through [[iteration-169-changelog-profile]]) against implementation learnings — especially whether the data-driven profile machinery is generic enough for madr/skills/changelog — update their scope/design/tasks before starting the next iteration

## Acceptance Criteria

- [x] `hyalo init --profile okf` produces a config under which a real OKF sample bundle lints clean
- [x] The `okf` skill is installed by `--claude` and references only real, current hyalo commands
- [x] Default `init` behavior unchanged; global `--format json|text` still works on `init`
- [x] Quality gates pass; docs + skill updated in the same PR — verified locally 2026-07-17: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q` (822+53+25 passed) all green; README.md, `init --help`, and `templates/skill-hyalo-okf.md` all touched in this PR's diff

## Retrospective (2026-07-17)

The data-driven profile machinery landed as designed and **is generic enough for the queued profiles**:

- `Profile` (`crates/hyalo-cli/src/commands/profiles.rs`) is `{ name, description, toml_fragment, skills }` — adding `madr`/`skills`/`changelog` is one more `PROFILES` entry plus an embedded `templates/profile-<name>.toml` (+ optional skill), **no new Rust branches**. Item 30 ("additive data") satisfied.
- `merge_into_config` is a recursive **deep-merge/upsert**: scalars and arrays owned by a profile overwrite; tables merge; untouched keys (another profile's `[schema.types.*]`) survive. This is exactly the composability item 31 needs for madr-in-`adrs/**` + changelog-on-`CHANGELOG.md`. **Caveat for iter-167 ([[path-bound-schemas]]):** path-bound profiles will want `[schema.bind]` entries, which the current deep-merge handles structurally (they are just more table keys), but overlapping `bind` arrays from two profiles would *overwrite* rather than *union* — iter-167 should decide bind-array merge semantics (likely append-with-dedupe) rather than rely on last-writer-wins.
- **Broken-link "severity = warn" (item 40) was a non-issue:** `hyalo lint` never errors on broken cross-file links (that is a `find --broken-links` advisory feature, and MD011/MD042 only cover reversed/empty inline links). So the spec's "don't reject on broken links" is satisfied by construction — no lint-rule severity override was needed or added. iters 165–166 should not assume a configurable broken-link lint severity exists.
- The skill deliberately references **only shipped commands**; `okf index`/`okf log`/`lint --profile okf` are name-dropped in the skill's setup prose only as they land (iters 165–166). When those ship, update `templates/skill-hyalo-okf.md` in the same PR (house rule).

No scope changes required for iters 165–169; the one actionable follow-up is the `[schema.bind]` array-merge decision noted above, recorded for iter-167.
