---
type: iteration
title: "Iteration 272 — design a set/append syntax for object-list items"
date: 2026-09-04
status: planned
tags:
  - iteration
  - schema
  - cli
  - dogfooding
branch: iter-272/object-list-item-authoring-syntax
priority: 5
related:
  - "[[iterations/iteration-268-object-list-schema-type]]"
  - "[[decision-log]]"
---

# Iteration 272 — design a set/append syntax for object-list items

## Goal

Carry-over from [[iterations/iteration-268-object-list-schema-type]] (DEC-287),
explicitly deferred there: `hyalo set` / `hyalo append` have no syntax for writing a
single map item into an `object-list` property (e.g. adding
`- ref: github:foo/bar\n  commit: abc1234` to a `sources:` list). Iteration 268 gave
`object-list` full read-side support — parsing, validation, `types show`, dot-path
`find` — and confirmed that an *existing* object-list value is validated on write
(`set --validate` / `append --validate` already refuse a scalar or string item via
the shared validator), but authoring a new well-formed item still requires an editor;
`hyalo append sources.ref=github:foo/bar` or similar has no defined meaning today and
either errors or (worse) appends a bare string that lint then flags exactly the way
DEC-287 was written to catch.

This is a **design** iteration first: the project rule is no new CLI flags without
justification (`feedback_no_cli_surface_growth`), so before writing any parser, decide
whether a syntax belongs on `append`/`set` at all, or whether the answer is "stays an
editor concern" (in which case this plan closes as won't-do with a DEC recording why,
rather than shipping a flag nobody asked for twice). DEC-287 sketched one candidate
shape, `set --property 'sources[]=ref=…'`, untested against real usage — treat it as a
starting point, not a commitment.

## Scope questions to resolve before implementing

- [ ] Does a second dogfooding cycle (or the mapl-memory `sources:` migration that
      motivated iteration 268) actually hit this gap in practice, or was the read-side
      support (lint + `find`) sufficient on its own? Check
      `hyalo find --property status=planned --tag dogfood` and the mapl-memory PR
      referenced in [[backlog/done/object-list-schema-type]] for evidence either way.
- [ ] If a syntax is warranted: one flat `key=value,key2=value2` item per invocation,
      or a way to build up multiple keys across calls? Multi-key-per-call is the
      minimum useful shape (an object-list item needs `required-keys` satisfied in one
      write, not accumulated field-by-field against a schema that has no notion of a
      "draft" item).
- [ ] Interaction with `--validate`: a partially-specified item (missing a
      `required-keys` key) should refuse exactly like today's scalar-item case.
- [ ] Whether this needs a new flag at all, or can reuse the existing `-p`/`--property`
      dotted-path assignment syntax `find` already understands for reads (symmetry
      argument) — investigate before proposing a bespoke syntax.

## Tasks

- [ ] Research: grep mapl-memory (or any other dogfooded vault) for `sources:` /
      similar object-list edits made by hand since iteration 268 landed, to see whether
      the gap is actually felt.
- [ ] Decide: implement a syntax, or close as won't-do with a DEC. If won't-do, record
      the reasoning in [[decision-log]] and mark this iteration `superseded` (not
      `completed` — no code lands either way) pointing at the DEC.
- [ ] If implementing: design the flag/syntax, get it past the no-new-CLI-surface bar
      (justify why this is different from a config knob), implement in
      `crates/hyalo-cli/src/commands/set.rs` and `append.rs`, validate against
      `required-keys`/`allowed-keys`/`key-patterns` before writing, update `--help`,
      `docs/configuration.md`, `hyalo-knowledgebase/docs/schema-and-lint.md`, the
      skill file, and CHANGELOG.
- [ ] Either way: run `cargo xtask check-help-drift` and the full gate sequence
      (`cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`) before closing.

## Acceptance criteria

- [ ] A decision is recorded either way — a working `set`/`append` object-list item
      syntax with tests, or a DEC explaining why it stays an editor concern — there is
      no "silently dropped" outcome for this plan.
- [ ] If implemented: `hyalo set f.md --property 'sources[]=...' --validate` (or
      whatever syntax is chosen) refuses an item missing a `required-keys` key, and
      accepts one that satisfies `required-keys`/`allowed-keys`/`key-patterns`.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB.

## Links

- [[iterations/iteration-268-object-list-schema-type]] — introduced `object-list`,
  deferred this
- [[decision-log]] — DEC-287
- [[backlog/done/object-list-schema-type]]
