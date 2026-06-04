---
title: "Iteration 155: Introduce datetime property type"
type: iteration
date: 2026-06-04
tags: [iteration, schema, frontmatter, linting, obsidian-parity]
status: planned
branch: iter-155/datetime-type
---

# Iteration 155: Introduce datetime property type

## Motivation

Obsidian distinguishes between two distinct property types:

- **date** — calendar day only, `YYYY-MM-DD`
- **datetime** — instant in time, ISO-8601 with time component (e.g. `2026-06-04T14:30:00`)

Hyalo currently exposes only `date` in its schema constraint system. The
parsing infrastructure for datetime values already exists internally
(`frontmatter/types.rs` has `is_datetime()` and a `forced_type="datetime"`
branch in `parse_value()`), but datetime is not selectable as a property
type via `hyalo types set`, not surfaced in `hyalo types show`, not
covered by a linting rule, and not documented. This iteration closes that
gap so users can declare `datetime` properties for Obsidian parity.

## Scope

- Expose `datetime` as a first-class `PropertyConstraint` variant.
- Accept `datetime` in `hyalo types set --property-type k=datetime`.
- Validate `datetime`-typed fields in lint (new HYALO004 rule) and in
  the schema-driven frontmatter parser.
- Render `datetime` in `hyalo types show` (text + JSON).
- Update README + knowledgebase docs + CLI help text.
- Preserve existing `date` behaviour exactly — no migration of existing
  `date`-typed fields to `datetime`.

## Non-goals

- No timezone handling beyond what `is_datetime()` already accepts
  (naive local datetime, no `Z` suffix, no offset, no fractional seconds).
  Extending the accepted grammar is out of scope and can be a follow-up.
- No automatic inference change: type inference already detects datetimes
  separately from dates; this iteration only adds the schema surface.
- No `$now` token analogous to `$today` — file a follow-up if desired.

## Tasks

- [ ] Add `DateTime` variant to `PropertyConstraint` in
      `crates/hyalo-core/src/schema.rs` (around line 209-227), mirroring
      `Date`. Update `RawPropertyConstraint::try_from` (line 255-370) so
      the TOML form `type = "datetime"` round-trips.
- [ ] Wire `datetime` through the schema-driven frontmatter validator so
      that a property declared `datetime` is parsed via the existing
      `parse_value(..., forced_type="datetime")` branch in
      `crates/hyalo-core/src/frontmatter/types.rs` (lines 87-92).
- [ ] Extend `parse_property_type_str()` in
      `crates/hyalo-cli/src/commands/types.rs` (~line 157) to accept
      `datetime`, and update the CLI help text listing valid types
      (~line 166).
- [ ] Update `constraint_to_json()` in
      `crates/hyalo-cli/src/commands/types.rs` (lines 87-111) to emit
      `{"type": "datetime"}` and update the text renderer accordingly.
- [ ] Add lint rule **HYALO004 — datetime-format** in
      `crates/hyalo-mdlint/src/rules/`. Fires when a property declared
      as `datetime` (or with a conventional datetime key — keep the key
      list small and explicit to avoid false positives) does not match
      `is_datetime()`. Use `hyalo_core::util` for the validator
      (add `is_iso8601_datetime()` next to `is_iso8601_date()` in
      `crates/hyalo-core/src/util.rs` if not already exposed).
- [ ] Register HYALO004 in the lint engine, enabled by default, promoted
      to error under `--strict` — match the wiring used for HYALO003 in
      `crates/hyalo-mdlint/src/engine.rs` (lines 131-139).
- [ ] Update `crates/hyalo-cli/tests/e2e/types.rs` with end-to-end
      coverage: `types set foo --property-type when=datetime`, then
      `types show foo` (text + JSON), then a frontmatter sample that
      fails lint with HYALO004 and another that passes.
- [ ] Add unit tests:
      - `schema.rs`: TOML round-trip of `type = "datetime"` constraint.
      - `frontmatter/types.rs`: `parse_value` with `forced_type=datetime`
        accepts valid and rejects malformed inputs.
      - `util.rs`: `is_iso8601_datetime()` boundary cases (invalid
        month, day, hour, minute, second; leap year already covered for
        date).
      - `hyalo003.rs` / new `hyalo004.rs`: lint fires/clears as expected.
- [ ] Update docs:
      - `README.md` schema section (~line 195-201) to list `datetime`
        alongside `date` with a short example.
      - `hyalo-knowledgebase/` reference pages that enumerate property
        types (search with `hyalo find "property type"`).
      - `.claude/rules/knowledgebase.md` if it lists supported types.
- [ ] Run quality gates in order: `cargo fmt`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`.
- [ ] Dogfood: build release, declare a `datetime` field in this
      iteration's frontmatter (or a scratch doc), and verify
      `hyalo lint`, `hyalo types show`, and `hyalo set` all behave.

## Acceptance criteria

- [ ] `hyalo types set foo --property-type when=datetime` succeeds and
      persists; `hyalo types show foo` (text + JSON) reports `datetime`.
- [ ] A `.md` file whose frontmatter declares a `datetime` property with
      a valid `YYYY-MM-DDThh:mm:ss` value passes `hyalo lint`.
- [ ] Same file with a `YYYY-MM-DD` value (no time) fails HYALO004 with a
      clear error message naming the offending property.
- [ ] Existing `date`-typed fields and HYALO003 behaviour are unchanged
      (regression-tested).
- [ ] README and knowledgebase reference docs document `datetime` next
      to `date`, including the accepted grammar.
- [ ] `cargo fmt`, `clippy -D warnings`, and `cargo test --workspace -q`
      all pass.

## Open questions

- Should HYALO004 trigger purely on schema-declared `datetime` fields,
  or also on conventional keys like `created_at`/`updated_at`? Default:
  schema-only, to avoid the false-positive risk that conventional-key
  matching has caused in HYALO003 historically. Revisit if dogfooding
  surfaces a clear need.
- Do we want a permissive mode that also accepts `Z` suffix or `+HH:MM`
  offsets? Out of scope here; track as a follow-up iteration if users
  ask.
