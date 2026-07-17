---
title: >-
  Iteration 172 — profile composition semantics (smart merge, profiles list, bind
  typing)
type: iteration
date: 2026-07-17
tags:
  - iteration
  - profiles
  - schema
  - fix-wave
status: completed
branch: iter-172/profile-composition-semantics
---

# Iteration 172 — profile composition semantics

## Goal

Make profile composition actually keep its promise ("multiple profiles compose
in one vault"): array config keys union instead of clobbering, `[lint]`
supports a *list* of active profiles, scalar overwrites warn, hand-written
TOML comments survive, and the `--profile` CLI overlay composes exactly like
file config. Fixes release blocker **RB-1** (confirmed by 5/7 dogfood agents)
and the bind-typing leak from
[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]].

## Decisions (taken 2026-07-17, do not re-litigate — see DEC-052)

- **Smart merge in the materialized `.hyalo.toml`** (not layered fragments —
  that redesign is recorded as future work in DEC-052).
- **Bind = typing**: a file typed via `[[schema.bind]]` satisfies the base
  schema's `required = ["type"]` — it must not error for a missing explicit
  `type:` key.

## Tasks

### 1. Merge engine (`profiles.rs`) [4/4]

- [x] Array keys **union** on merge instead of replace: `[schema] exempt`,
  `[lint] ignore`, `[schema.default] required`, and `[[schema.bind]]`
  (dedup bind entries by `(glob, type)`); preserve stable order
  (existing entries first, new appended)
- [x] Scalar keys: profile still owns its keys (refresh-on-rerun), but when an
  overwrite *changes* an existing differing value, print a
  `conflict: <key> "<old>" -> "<new>" (profile <name>)` line to stderr
- [x] Preserve hand-written comments and key order: switch the merge to
  `toml_edit` (new dependency; keep `toml` for read-only paths if simpler)
- [x] Reconcile the dogfood discrepancy: write a failing test first for the
  4-profile stack (okf+madr+skills+changelog) asserting ALL binds and the
  unioned exempt survive, then make it pass (ff-rdp saw binds compose,
  skills-audit saw them clobbered — find out which path differs)

### 2. `[lint] profiles` list [3/3]

- [x] `[lint] profiles = ["okf", "madr"]` — all listed profiles' native rules
  are active in plain `hyalo lint`; `profile = "okf"` (singular string) stays
  accepted as a 1-element compat alias, warning that `profiles` is preferred
- [x] `init --profile <p>` appends to `profiles` (no duplicates) instead of
  overwriting a scalar
- [x] `--profile <p>` CLI flag **adds** an ephemeral overlay composed with the
  file config — it must honor user `[schema] exempt` additions exactly like
  the file path does (fixes mapl BUG-6 flag-vs-file divergence)

### 3. Bind = typing [1/1]

- [x] A `[[schema.bind]]` match satisfies `required = ["type"]` for the bound
  file — a spec-valid frontmatter-less SKILL.md / MADR file lints clean under
  composed okf+skills / okf+madr (fixes df-own-kb B5 / ff-rdp B2)

### 4. Tests [5/5]

- [x] Unit: union semantics per key incl. `required` (user
  `["title","type"]` + profile `["type"]` → both survive), bind dedup,
  comment/order preservation round-trip
- [x] e2e: the hoppy regression scenario — `init --profile okf` then
  `init --profile madr` → OKF-CITATIONS rules still fire, reserved files stay
  exempt, lint error count does not regress
- [x] e2e: 4-profile stack → every profile's rules fire on a fixture
  violating each; re-running any `init --profile` is byte-idempotent
- [x] e2e: `--profile okf` flag on a vault with user exempt additions honors
  them (identical results to `[lint] profiles` file activation)
- [x] `cargo fmt` / clippy `-D warnings` / `cargo test --workspace -q` green

### 5. Docs sync (same PR) [3/3]

- [x] `init --help` composition claims match the new reality; README profiles
  section documents `profiles` list + conflict warnings
- [x] Bundled skill templates that mention `[lint] profile` updated
- [x] Retrospective task: adapt iteration-173..175 plans to what landed here

## Acceptance Criteria [4/4]

- [x] The ff-rdp dogfood branch scenario needs no hand-editing of `.hyalo.toml`: three `init --profile` runs produce a config where all reserved-file exemptions and all binds are active simultaneously (`three_profile_inits_keep_all_binds_and_exempt`)
- [x] No silent loss: any changed scalar prints a conflict line (`scalar_conflict_is_reported_not_silent`); arrays never shrink on profile init (`array_change_is_not_reported_as_conflict`)
- [x] Frontmatter-less bound files (SKILL.md, ADR) lint clean under composed
  profiles
- [x] Hand-written TOML comments survive `init --profile`
