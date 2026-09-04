---
type: iteration
title: "Iteration 273 — decide whether writes should gate on a broken [schema]"
date: 2026-09-04
status: planned
tags:
  - iteration
  - schema
  - cli
  - dogfooding
branch: iter-273/validate-write-gate-on-broken-schema
priority: 4
related:
  - "[[iterations/iteration-268-object-list-schema-type]]"
  - "[[decision-log]]"
---

# Iteration 273 — decide whether writes should gate on a broken [schema]

## Goal

Carry-over from [[iterations/iteration-268-object-list-schema-type]] (DEC-287,
"Known gap, deliberately not closed here"): a `.hyalo.toml` whose `[schema]` section
parses as TOML but fails `SchemaConfig::TryFrom` (e.g. an invalid `key-patterns`
regex, or any other schema-level rejection) is reported by `lint`, `find --strict`
and `views run` as a hard failure (DEC-279, iteration 265), but `set --validate` /
`append --validate` do not gate on it: they print a `-q`-proof
`invalid [schema] in .hyalo.toml: …` warning to stderr, fall back to an *empty*
schema, and write anyway — making `--validate` silently vacuous against a broken
schema, exactly when a user most expects it to refuse. Iteration 268 found and pinned
this as consequence of adding `object-list` (a bad `key-patterns` regex is a schema
`TryFrom` failure), confirmed it is pre-existing behavior scoped by DEC-279 rather
than something 268 introduced, and declared — without deciding — that extending the
write gate "belongs to its own decision" because it would change behaviour for every
kind of schema error, not just this one.

This iteration is that decision. Two honest outcomes, both acceptable:

1. Extend the DEC-279 gate to writes: `set --validate` / `append --validate` (and
   arguably `set --validate` implied by `validate_on_write`) refuse when `[schema]`
   fails `TryFrom`, matching `lint --strict`'s stance.
2. Keep the asymmetry, but make it discoverable rather than silent: e.g. `--validate`
   still writes but the JSON result surfaces a `schema_invalid: true` field (not a new
   flag — an existing-result-shape addition) alongside the stderr warning, so a
   scripted caller can detect the vacuity without reading stderr text.

Do not default to option 1 without checking real impact: DEC-279 scoped the original
gate deliberately narrower than "every command", so widening it needs the same rigor
— what breaks for a vault whose `[schema]` briefly goes invalid mid-edit if writes
suddenly start refusing.

## Tasks

- [ ] Re-read DEC-279 (iteration 265, `decision-log.md`) in full for why the gate was
      scoped to `lint`/`find --strict`/`views run` and not writes originally — this is
      not a new problem, so understand what was already weighed.
- [ ] Enumerate every `SchemaConfig::TryFrom` rejection path (`crates/hyalo-core/src/schema.rs`)
      to know the full blast radius of gating writes on it — this is not `object-list`-specific.
- [ ] Decide (1) or (2) above, or a third option if research surfaces one; record the
      choice and reasoning as a new DEC in [[decision-log]], explicitly resolving the
      "belongs to its own decision" note left by DEC-287.
- [ ] Implement the chosen behavior in `crates/hyalo-cli/src/commands/set.rs` and
      `append.rs`; add or update tests (`lint_reports_schema_malformed_for_an_invalid_key_pattern_regex`
      in `crates/hyalo-cli/tests/e2e/lint.rs` currently pins the vacuous case and will need
      updating either way).
- [ ] Update `set --help` / `append --help` if behavior changes; update
      `docs/configuration.md` and [[docs/schema-and-lint]] if they describe `--validate`'s
      guarantees.
- [ ] Gates: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `cargo xtask check-help-drift`.

## Acceptance criteria

- [ ] A DEC is recorded resolving the DEC-287 "belongs to its own decision" note —
      either writes now gate on a broken `[schema]`, or the asymmetry is kept but made
      discoverable in the result JSON, with reasoning either way.
- [ ] `lint_reports_schema_malformed_for_an_invalid_key_pattern_regex` (or its
      replacement) reflects the new behavior, not the old vacuous-write pin.
- [ ] Gates green.

## Links

- [[iterations/iteration-268-object-list-schema-type]] — found and pinned the gap
- [[decision-log]] — DEC-279 (original write-gate scope), DEC-287 (found the gap)
