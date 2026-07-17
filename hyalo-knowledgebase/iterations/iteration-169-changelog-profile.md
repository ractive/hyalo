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

**iter-166 retrospective (2026-07-17):** the seed to generalize lives in `crates/hyalo-cli/src/commands/okf_lint.rs`. It ships three primitives task 1 below should lift into a declarative grammar rather than re-implement: (a) `scan_sections()` — a CRLF-tolerant, fenced-code-aware ATX-heading scanner that returns each section's level, heading text, heading line, and body byte-range (already handles nesting via a level stack); (b) `parse_date_heading()`/`is_iso_date()` for `## YYYY-MM-DD` recognition and the newest-first monotonic ordering check (changelog wants the same shape but semver-descending with `[Unreleased]` pinned on top); (c) `is_link_list_line()` for `*`/`-` bullet-with-link detection. Note the deliberate design choice: OKF structure rules are **warn-only** (permissive model); the changelog grammar is stricter, so task 1's generic mechanism must make severity per-rule configurable (the profile picks). Also carry forward the OKF-rule registration pattern (catalog entries in `hyalo-mdlint/src/engine.rs` with `default_enabled = true` so `lint-rules set … --enabled false` writes a real override, gated at runtime by the active profile) and the ephemeral-overlay + `[lint] profile` idempotence machinery in `config.rs`/`run.rs` — reuse it verbatim, don't re-derive.

## Goal

`hyalo lint --profile changelog` validates `CHANGELOG.md` against the 1.1.0 grammar, and `hyalo changelog release <version>` rotates `## [Unreleased]` into a dated version section.

## Steps / Tasks

**iter-167 retrospective (2026-07-17) — reusable patterns from the madr profile:**
- **Single-file binding**: changelog needs the schema bound to one file (`CHANGELOG.md`), not a subtree. The shipped `[[schema.bind]]` (`hyalo-core/schema.rs`) already supports a literal-path glob (`glob = "CHANGELOG.md"`, first-match-wins) — reuse it; no new mechanism needed. Bind targets that name an undeclared type warn at config load (`unknown_bind_targets`).
- **`changelog release` generator**: model on `hyalo madr toc` (`commands/madr.rs`) + the shared `commands/managed_region.rs` helper — `Markers::new("changelog:...")` for any managed region, `GeneratePlan`/`apply_plan`, dry-run-exits-nonzero-on-drift. The rotate-`## [Unreleased]` splice is a managed-region edit; don't hand-roll marker finding.
- **Heading-grammar lint mode** (task 1 below) is the genuinely new capability here — the `MADR-*` rules were still content-scanning pure fns, not a grammar. Budget for that; the *wiring* (catalog entry + `<name>_profile` bool gated on `ctx.lint_profile`) is copy-paste from `madr_lint`, but the grammar engine is net-new.
- Profile fragment + `Profile` entry + skill file shape (see `commands/profiles.rs`, `templates/profile-madr.toml`) carries over unchanged.

**iter-168 retrospective (2026-07-17) — reusable patterns from the skills profile:**
- **`--profile` gating is NOT fully data-driven yet.** Each profile's advisory rules are turned on by a hardcoded boolean in `dispatch.rs` (`let <name>_profile_active = ctx.lint_profile.as_deref() == Some("<name>");`) threaded into `ExtLintOptions.<name>_profile` → `lint_one_file_extended`'s `<name>_profile: bool` param → a runtime `if <name>_profile { … }` block in `lint.rs`. changelog must add its own `changelog_profile_active` line, `ExtLintOptions.changelog_profile` field, thread it through the two fn signatures (mind the `#[allow(clippy::fn_params_excessive_bools)]` already on `lint_one_file_extended`), and add the runtime block + the `changelog_profile: false` default in the lint.rs test. Copy the skills block (`if skills_profile { … }`) verbatim as the template — it is the newest and cleanest.
- **Per-finding severity**: skills needed one *error* rule (`SKILL-RESERVED-NAME`) among otherwise-warn advisory rules, so `SkillFinding` grew a `default_severity: &'static str` and the runtime unwraps `…severity().unwrap_or(f.default_severity)` (not a hardcoded `"warn"`). changelog's grammar is stricter (many error-level rules) — adopt the same `default_severity`-per-finding shape from the start rather than the madr all-warn assumption, and set matching `default_severity` in the `hyalo-mdlint/src/engine.rs` catalog entries.
- **Rust `regex` has no look-around.** The `regex` crate (used for schema `pattern` validation in `lint.rs::validate_constraint`) rejects `(?!…)`/`(?=…)` at compile time — a pattern using them surfaces as a runtime "invalid pattern" *error*, not a silent skip. skills had to move reserved-word negation into an advisory rule and express "no XML tags" as the lookahead-free `^[^<]*$`. changelog's heading-grammar patterns (semver, date) must stay lookahead-free; if a constraint genuinely needs negation/backref, put it in the grammar engine (Rust code), not a `pattern`.
- **New generic constraint available**: string `min-length`/`max-length` now exist on any `string` property (`PropertyConstraint::String { pattern, min_length, max_length }`, `min-length`/`max-length` TOML keys, validated in chars not bytes, rejected on non-string types). Reuse for any bounded changelog field instead of adding another kind.
- **Scaffolding did not need auto-numbering**: the generic `hyalo new --type <t> --file <path>` was sufficient for skills (`hyalo new --type skill --file my-skill/SKILL.md`); `{n:04}` render_number is still un-wired end-to-end (per iter-167's note). changelog scaffolding is a single fixed file, so no numbering is needed either — don't build it.

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
