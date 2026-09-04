---
type: iteration
title: "Iteration 270 — Schema write-side semantics: what set/append --validate promise"
date: 2026-09-04
status: in-progress
tags:
  - iteration
  - schema
  - cli
  - dogfooding
branch: iter-270/schema-write-semantics
priority: 10
related:
  - "[[iterations/iteration-268-object-list-schema-type]]"
  - "[[decision-log]]"
---

# Iteration 270 — Schema write-side semantics: what `set`/`append --validate` promise

## Goal

Two open design questions left by iteration 268 (DEC-287), both about the write side of
the schema language and both landing in `crates/hyalo-cli/src/commands/set.rs` and
`append.rs`. Each ends in a DEC; at most one of them ends in code. They are bundled
because they share the same files, the same `--validate` contract, and the same
no-new-CLI-surface bar — deciding them together keeps `--validate`'s guarantees coherent
instead of being shaped by two separate PRs.

Consolidated on 2026-09-04 from iteration-272 (object-list item authoring syntax) and
iteration-273 (write gate on a broken `[schema]`). The original plan texts are in git
history at commit `99e20289` and its parents.

This is a **design iteration first**. The project rule is no new CLI flags without
justification (standing project rule, reaffirmed after review pressure killed
`--iteration` and `--strict-index`; DEC-287 applies it to the schema language
specifically). Neither question may be answered by adding a flag "because the
plan said so": each must either fit inside existing syntax/result shapes or close as
won't-do with a DEC recording why.

## Part A — decide whether writes should gate on a broken `[schema]`

### Background

A `.hyalo.toml` whose `[schema]` section parses as TOML but fails
`SchemaConfig::TryFrom` (e.g. an invalid `key-patterns` regex) is reported by `lint`,
`find --strict` and `views run` as a hard failure (DEC-279, iteration 265), but
`set --validate` / `append --validate` do not gate on it: they print a `-q`-proof
`invalid [schema] in .hyalo.toml: …` warning to stderr, fall back to an *empty* schema,
and write anyway — making `--validate` silently vacuous against a broken schema, exactly
when a user most expects it to refuse. Iteration 268 pinned this
(`lint_reports_schema_malformed_for_an_invalid_key_pattern_regex` in
`crates/hyalo-cli/tests/e2e/lint.rs`) and declared that extending the write gate
"belongs to its own decision" because it changes behaviour for every kind of schema
error, not just this one.

Two honest outcomes, both acceptable:

1. Extend the DEC-279 gate to writes: `set --validate` / `append --validate` (and
   arguably writes under `validate_on_write`) refuse when `[schema]` fails `TryFrom`,
   matching `lint --strict`'s stance.
2. Keep the asymmetry but make it discoverable: `--validate` still writes, and the JSON
   result surfaces a `schema_invalid: true` field (an existing-result-shape addition, not
   a flag) alongside the stderr warning, so a scripted caller can detect the vacuity
   without parsing stderr.

Do not default to option 1 without checking real impact: DEC-279 scoped the original gate
deliberately narrower than "every command", so widening it needs the same rigour — what
breaks for a vault whose `[schema]` briefly goes invalid mid-edit if writes suddenly
start refusing.

### GATE-1: decide and implement

- [x] Re-read DEC-279 in full for why the gate was scoped to `lint`/`find --strict`/
      `views run` and not writes originally.
- [x] Enumerate every `SchemaConfig::TryFrom` rejection path
      (`crates/hyalo-core/src/schema.rs`) to know the full blast radius of gating writes
      on it — this is not `object-list`-specific.
- [x] Decide (1), (2), or a third option research surfaces; record the choice and
      reasoning as a new DEC in [[decision-log]], explicitly resolving DEC-287's "belongs
      to its own decision" note.
- [x] Implement the chosen behaviour in `set.rs` and `append.rs`; update
      `lint_reports_schema_malformed_for_an_invalid_key_pattern_regex` (it currently pins
      the vacuous case and must change either way).
- [x] Update `set --help` / `append --help` if behaviour changes; update
      `docs/configuration.md` and [[docs/schema-and-lint]] where they describe
      `--validate`'s guarantees.

## Part B — decide whether `set`/`append` need an object-list item syntax

### Background

`hyalo set` / `hyalo append` have no syntax for writing a single map item into an
`object-list` property (e.g. adding `- ref: github:foo/bar\n  commit: abc1234` to a
`sources:` list). Iteration 268 gave `object-list` full read-side support and confirmed
that an *existing* object-list value is validated on write, but authoring a new
well-formed item still requires an editor; `hyalo append sources.ref=github:foo/bar` has
no defined meaning today and either errors or appends a bare string that lint then flags
exactly the way DEC-287 was written to catch.

DEC-287 sketched one candidate shape, `set --property 'sources[]=ref=…'`, untested against
real usage — a starting point, not a commitment. The default answer is "stays an editor
concern" unless evidence of real need turns up.

### Scope questions to resolve before writing any parser

- [x] Does a second dogfooding cycle, or the mapl-memory `sources:` migration that
      motivated iteration 268, actually hit this gap in practice? Check the mapl-memory PR
      referenced in [[backlog/done/object-list-schema-type]] and any hand edits to
      object-list properties in dogfooded vaults since 268 landed.
- [x] If a syntax is warranted: one flat `key=value,key2=value2` item per invocation is
      the minimum useful shape — an item needs `required-keys` satisfied in one write, not
      accumulated field-by-field.
- [x] Interaction with `--validate` (and with Part A's decision): a partially-specified
      item must refuse exactly like today's scalar-item case.
- [x] Can the existing `-p`/`--property` dotted-path syntax `find` already understands for
      reads be reused for writes (symmetry argument), avoiding any new flag? Investigate
      before proposing a bespoke syntax.

### ITEM-1: decide and (maybe) implement

- [x] Decide: implement a syntax, or close as won't-do. Record the reasoning in the same
      DEC as Part A or its own DEC in [[decision-log]] — there is no "silently dropped"
      outcome.
- [ ] If implementing: design the syntax, justify it against the no-new-CLI-surface bar,
      implement in `set.rs` and `append.rs`, validate against
      `required-keys`/`allowed-keys`/`key-patterns` before writing, update `--help`,
      `docs/configuration.md`, [[docs/schema-and-lint]], the skill file, and CHANGELOG.
- [x] If won't-do: no code lands for Part B; the DEC is the deliverable.

## Shared closing tasks

- [x] DEC(s) recorded in [[decision-log]] covering both decisions.
- [x] Changelog entry via `hyalo changelog add` for every behaviour that changed.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*`
      gates (help drift in particular if any `--help` text changed).

## Acceptance criteria

- [x] A DEC resolves DEC-287's "belongs to its own decision" note: either writes now gate
      on a broken `[schema]`, or the asymmetry is kept but made discoverable in the result
      JSON, with reasoning either way.
- [x] `lint_reports_schema_malformed_for_an_invalid_key_pattern_regex` (or its
      replacement) reflects the new behaviour, not the old vacuous-write pin.
- [x] A decision is recorded for the object-list item syntax either way — a working
      `set`/`append` syntax with tests, or a DEC explaining why it stays an editor concern.
- [ ] If a syntax is implemented: `hyalo set f.md --property '<chosen syntax>' --validate`
      refuses an item missing a `required-keys` key and accepts one that satisfies
      `required-keys`/`allowed-keys`/`key-patterns`.
- [x] No new CLI flag lands without an explicit justification in the DEC.
- [ ] Gates green.

## Outcome

**Part A → DEC-290, implemented.** Neither of the plan's two options as written.
The gate is scoped by *what `--validate` promises*, not by command kind: `set` /
`append` refuse with exit 1 (writing nothing, `--dry-run` included) when
`[schema]` fails `SchemaConfig::try_from` **and** validation was requested —
`--validate` or `[schema] validate_on_write`. A write that made no such claim
still proceeds on the empty fallback with the `-q`-proof warning, and `mv`,
`remove`, `task toggle` and every read are untouched, so a vault whose schema is
half-edited stays usable. Option 1 read literally (gate *every* write) was
rejected because a rejected `[schema]` — unlike an unparsable `.hyalo.toml` —
still leaves `dir`, `[lint] ignore` and the views intact; option 2
(`schema_invalid: true` in the result JSON) was rejected because it leaves the
default case as wrong as before and charges every caller a check for a guarantee
the flag already advertised. All 13 `try_from` rejection paths are config
authoring errors reachable only by editing `.hyalo.toml`, never by editing a
note.

**Part B → DEC-291, won't-do, no code.** No demand: `object-list` appears in zero
`.hyalo.toml` files across this vault and both dogfood testbeds, and the one real
user (mapl-memory `sources:`) migrated in an editor and said in its own request
that `set --property` support was out of scope. The symmetry argument fails —
`--property` on the write side is a literal top-level key by design
(`reject_dotted_property_collision` exists to refuse dotted writes), and a write
needs an item selector the read syntax cannot express. A bespoke `sources[]=k=v`
item syntax would pack a second key/value language, with its own escaping rules,
into a value position that already infers types — against the standing
no-new-CLI-surface bar, for a shape one editor invocation writes correctly.

No new CLI flag landed.

## Links

- [[iterations/iteration-268-object-list-schema-type]] — found and deferred both questions
- [[iterations/iteration-265-scan-exclude-and-skipped-files]] — DEC-279, original gate scope
- [[backlog/done/object-list-schema-type]]
- [[decision-log]] — DEC-279, DEC-287, DEC-290, DEC-291
