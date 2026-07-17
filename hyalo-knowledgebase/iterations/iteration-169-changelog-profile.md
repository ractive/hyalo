---
title: "Iteration 169 — changelog profile (Keep a Changelog 1.1.0)"
type: iteration
date: 2026-07-16
status: planned
branch: iter-169/changelog-profile
tags: [iteration, profiles, changelog, keep-a-changelog, lint, generators]
related: [research/profile-candidates-beyond-okf.md]
priority: 7
depends-on: iteration-168-skills-profile
---

# Iteration 169 — `changelog` profile

Fourth profile (see [[profile-candidates-beyond-okf]]). Keep a Changelog 1.1.0 (<https://keepachangelog.com/en/1.1.0/>) is frontmatter-less — this iteration generalizes the **heading-grammar lint mode** first cut in iter-166's reserved-file checks into a reusable capability (gap #1 from the survey), which later also unlocks Nygard ADRs and Standard Readme.

## Goal

`hyalo lint --profile changelog` validates `CHANGELOG.md` against the 1.1.0 grammar, and `hyalo changelog release <version>` rotates `## [Unreleased]` into a dated version section.

## Steps / Tasks

### 1. Heading-grammar lint mode (generic capability)

- [ ] Extract/generalize iter-166's `index.md`/`log.md` structure checks into a declarative heading-grammar mechanism usable by profiles (sequence, level, pattern, ordering constraints on headings)
- [ ] Grammar for changelog: `# Changelog` → optional `## [Unreleased]` → `## [X.Y.Z] - YYYY-MM-DD` (semver, strictly descending; dates monotonically non-increasing; optional `[YANKED]` marker) → `###` subsections limited to `Added|Changed|Deprecated|Removed|Fixed|Security`
- [ ] Link-ref footer cross-check: every `[X.Y.Z]` heading has a matching link reference definition and vice versa
- [ ] Lints: unknown `###` category, empty section, out-of-order versions, malformed dates
- [ ] Single-file scope via iter-167's `[schema.bind]`: `"CHANGELOG.md" = "changelog"` — a frontmatter-less type bound purely by path (+ frontmatter exemption), coexisting with the vault's other profiles/config (see [[path-bound-schemas]])

### 2. Release generator

- [ ] `hyalo changelog release <X.Y.Z> [--date YYYY-MM-DD]`: rotate `## [Unreleased]` content into `## [X.Y.Z] - <date>`, re-create empty Unreleased, update footer link refs — dry-run default + `--apply`, idempotency guard (refuse if version exists)
- [ ] Reuse the dated-entry machinery from `okf log` (iter-165) where it fits. **iter-165 note:** `okf log`'s date-section insertion (`prepend_log_entry` + `find_heading`/`line_end_after` in `crates/hyalo-cli/src/commands/okf.rs`) inserts under an existing `## <date>` heading or creates a fresh one above older sections — the same shape a changelog `## [X.Y.Z] - <date>` rotation needs, but it is date-ordered (descending) whereas changelog wants semver-ordered sections with an `[Unreleased]` pinned at the top. Reuse the "insert-a-new-section-in-order, preserve-the-rest" splice idea; the exact ordering predicate and footer-link-ref rewrite are changelog-specific. `today_iso8601()`/`now_timestamp_tz()` (hyalo-core / okf module) supply the date default.
- [ ] `hyalo changelog add --category Added --message "..."` (append an entry under Unreleased) — evaluate whether this earns its keep or is scope creep; decide and record

### 3. Tests

- [ ] e2e: the keepachangelog.com reference example lints clean; each grammar violation class produces exactly its lint
- [ ] e2e: `release` rotation round-trip (add entries → release → lint clean → release same version refused)
- [ ] Dogfood: lint hyalo's own CHANGELOG if present; adopt the profile in this repo if it fits
- [ ] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 4. Docs sync (same PR)

- [ ] `--profile` help lists `changelog`; README profiles section extended (now 4 profiles — consider a dedicated README "Profiles" table)
- [ ] Update [[profile-candidates-beyond-okf]] status for changelog

### 5. Retrospective (learnings-propagation — do this LAST, always)

- [ ] Sequence complete — write a consolidated profiles retrospective in the KB (what the profile machinery got right/wrong across okf/madr/skills/changelog) and update [[profile-candidates-beyond-okf]] with revised effort estimates for the deferred candidates (nygard, standard-readme, SSG wave, importers)

## Acceptance Criteria

- [ ] `hyalo lint --profile changelog` fully validates the 1.1.0 grammar incl. footer link refs; reference example passes clean
- [ ] `hyalo changelog release` rotation is correct, idempotent-guarded, dry-run by default
- [ ] Heading-grammar mode is generic (declarative), not changelog-hardcoded
- [ ] Quality gates pass; docs synced; consolidated profiles retrospective written
