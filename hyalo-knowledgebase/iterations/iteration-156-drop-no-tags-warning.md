---
title: "Iteration 156: Drop hardcoded 'no tags defined' lint warning"
type: iteration
date: 2026-06-04
tags: [iteration, lint, schema, dx]
status: in-progress
branch: iter-156/drop-no-tags-warning
---

# Iteration 156: Drop hardcoded "no tags defined" lint warning

## Motivation

`hyalo lint` emits a hardcoded `warn: no tags defined` for every file whose
frontmatter lacks a `tags` key, whenever the configured schema has at least
one type defined. The warning has no rule ID, is not toggleable via
`hyalo lint-rules`, has no per-schema opt-out, and was never raised as a
design question in [[iteration-102a-schema-and-lint]] (it survived as a
bullet from [[karpathy-llm-wiki]] research that landed verbatim in the
first lint commit `bad9c90`).

In practice many vault types do not — and should not — carry tags
(research notes pinned by file path, generated docs, dogfood reports).
The warning is consistently noisy with no way to silence it short of
adding `tags: []` to every file or deleting all schema types.

Users who *do* want tags enforced already have a precise tool: add
`tags` to the relevant type's `required` array. That promotes a missing
key to a hard error and is exactly the kind of opt-in the schema system
is designed for.

## Scope

- Remove the hardcoded `!has_tags && !schema.types.is_empty()` warning
  from `crates/hyalo-cli/src/commands/lint.rs::validate_properties`.
- Drop the `has_tags: bool` parameter from `validate_properties` and
  from the public `lint_counts_from_properties` API; update the one
  caller in `crates/hyalo-cli/src/commands/summary.rs`.
- Replace the `strict_mode_leaves_no_tags_as_warn` unit test with a
  test that asserts the opposite: a typed file with no `tags` is clean.
- Update `hyalo lint --help` long_about, the `templates/skill-hyalo.md`
  symlinked skill, and `hyalo-knowledgebase/docs/schema-and-lint.md` so
  none of them advertise the removed warning.
- No change to the comma-joined-tags warning (`cli,ux` inside a list)
  or to `tags` constraint validation when a schema *does* declare one.

## Non-goals

- Not adding a `min_items` / `non_empty` constraint to `List` /
  `StringList`. Out of scope; track as a follow-up iteration if needed.
- Not retrofitting rule IDs onto the remaining built-in schema-pass
  warnings ("no `type`", "undeclared property"). Same story, different
  iteration.
- Not touching the inline `#hashtag` story (covered by DEC-020).

## Tasks

- [x] Delete the warning emission block in `lint.rs::validate_properties`.
- [x] Update the surrounding stale comment that referenced the warning.
- [x] Remove the `has_tags` parameter from `validate_properties`.
- [x] Remove `has_tags` from the `lint_counts_from_properties` public
      signature; update `summary.rs` caller.
- [x] Replace the `strict_mode_leaves_no_tags_as_warn` test with
      `missing_tags_is_not_a_violation_by_default` asserting clean lint.
- [x] Update `crates/hyalo-cli/src/cli/args.rs` long_about: drop
      `no 'tags'` from the warn-list.
- [x] Update `crates/hyalo-cli/templates/skill-hyalo.md` strict-mode
      paragraph: drop the `(no tags, etc.)` aside.
- [x] Update `hyalo-knowledgebase/docs/schema-and-lint.md` example
      output and severity-level list; mention `required = ["tags"]` as
      the opt-in path.
- [ ] Run quality gates: `cargo fmt`,
      `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`.

## Acceptance criteria

- [ ] `hyalo lint` against a file with `type: …` set but no `tags`
      property produces zero violations under the project's default
      schema.
- [ ] A schema that declares `required = ["tags"]` for a type still
      raises a hard error when a file of that type lacks the key
      (regression: `required` enforcement is unaffected).
- [ ] `hyalo lint --strict` no longer produces a "no tags defined" warn
      for any file in this repo's knowledgebase.
- [ ] `cargo fmt`, `clippy -D warnings`, and `cargo test --workspace -q`
      all pass.
- [ ] README, schema-and-lint doc, skill template, and `--help` no
      longer advertise the removed warning.
