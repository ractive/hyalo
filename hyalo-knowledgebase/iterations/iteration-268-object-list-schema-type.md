---
type: iteration
title: "Iteration 268 — Schema: object-list property type"
date: 2026-09-03
status: planned
tags:
  - iteration
  - schema
  - lint
branch: iter-268/object-list-schema-type
priority: 8
related:
  - "[[backlog/done/object-list-schema-type]]"
---

# Iteration 268 — Schema: object-list property type

## Goal

The schema language knows `list` (any items) and `string-list` (string items with an optional
`item_pattern`), but nothing can describe a **list of maps**. [[backlog/done/object-list-schema-type]]
records the motivating case: mapl-memory migrated `sources:` from plain strings to objects
(`- ref: github:comparis/neon` / `commit: 3c9e0f2`, `- ref: https://example.org/post` /
`read: 2026-09-01`). Since iteration 244 `find --property sources.ref=…` walks dot-paths into
list items, so the data is queryable, but lint cannot enforce the shape: a leftover plain-string
entry, a `rev:` typo for `commit:`, or an unknown pin key all pass `hyalo lint --strict`. The
dangerous half is the string entry: `resolve_path` in
`crates/hyalo-core/src/filter/match_props.rs` returns `None` for a scalar with a remaining path,
so that item silently drops out of every `sources.ref=` query and nothing reports it. The vault
currently documents the contract in a TOML comment next to `type = "list"`.

Proposed configuration, exactly as in the backlog item:

```toml
[schema.types.memory.properties.sources]
type = "object-list"
required-keys = ["ref"]
allowed-keys = ["ref", "commit", "version", "updated", "read"]

[schema.types.memory.properties.sources.key-patterns]
ref = "^(github|confluence|jira|slack|person|runtime|decision):|^https?://"
commit = "^[0-9a-f]{7,40}$"
read = "^\\d{4}-\\d{2}-\\d{2}$"
```

Semantics (DEC-286, tentative — DEC-266..285 are pre-assigned to iterations 261–267):

- Every list item must be a YAML map. A plain-string item is an error whose message carries the
  fix-it text `- ref: <value>`; a number, bool, null or nested list item is an error without it.
- `required-keys` must all be present in every item. Keys outside `allowed-keys` are errors;
  omitting `allowed-keys` allows any extra key. Every name in `required-keys` and `key-patterns`
  must also appear in `allowed-keys` when that list is given, otherwise the config is rejected.
- `key-patterns` maps key → regex, applied to the item's value for that key when it is a scalar
  (strings, numbers, bools and dates are matched against their YAML text). A non-scalar value
  under a `key-patterns` key is an error. Keys in `key-patterns` are optional unless also listed
  in `required-keys`.
- Items are validated independently and every violation is reported (no first-error cut-off,
  consistent with `item_pattern`). Each message names the property, the 0-based item index and,
  where applicable, the key.
- An empty list is valid (vacuous, like `item_pattern` on an empty list). A non-list value is one
  error, as for `list` / `string-list`.

Scope: deliberately flat — string keys, scalar values, regex. No nested maps or lists under a
key, no per-key types (`date`, `enum`), no cross-item uniqueness; this is not JSON Schema.
Config-only: the constraint is written in `.hyalo.toml` by hand. **No new CLI flags** (project
rule); `types set` gains no `--required-keys`/`--allowed-keys`/`--key-pattern`, and it does not
even accept `--property-type k=object-list` in this iteration — `parse_property_type_str` in
`crates/hyalo-cli/src/commands/types.rs` today rejects `string-list` for the same reason, and
adding `types set` support for constraint-bearing types is a separate future DEC. `hyalo set` /
`hyalo append` for writing object items are out of scope; write-time *validation* of an existing
object-list value is in scope only because it falls out of the shared validator (SCHEMA-4).

## Tasks

### SCHEMA-1: config parsing (hyalo-core)

- [ ] `crates/hyalo-core/src/schema.rs`: add `PropertyConstraint::ObjectList { required_keys:
      Vec<String>, allowed_keys: Option<Vec<String>>, key_patterns: IndexMap<String, String> }`
      (line ~437). Keep patterns as `String` to match `pattern`/`item_pattern`; the variant is
      `Clone` like its siblings.
- [ ] `RawPropertyConstraint` (line ~499, `deny_unknown_fields`): add `#[serde(rename =
      "required-keys")] required_keys: Option<Vec<String>>`, `#[serde(rename = "allowed-keys")]
      allowed_keys: Option<Vec<String>>`, `#[serde(rename = "key-patterns")] key_patterns:
      Option<IndexMap<String, String>>` (kebab-case, matching `min-length`; `IndexMap` so `types
      show` prints keys in file order). A `key-patterns` value that is not a table of strings
      is a TOML deserialisation error; an unknown key inside `[…properties.sources]` stays
      rejected by `deny_unknown_fields`.
- [ ] `TryFrom<RawPropertyConstraint>` (line ~515): add the `"object-list"` arm; reject
      `pattern`, `item_pattern`, `values`, length and number bounds on it; reject the three new
      keys on every other type with the same `property 'x': …` prefix the existing cross-field
      checks use; reject a `required-keys`/`key-patterns` name absent from a given
      `allowed-keys`; reject an empty key name. Add `object-list` to the unknown-type error list
      (line ~687).
- [ ] Compile every `key-patterns` regex once at config load and fail the config with
      `property 'sources': key-patterns.commit: invalid regex: <error>`. This is stricter than
      `item_pattern` (which surfaces an invalid regex per file at lint time); record the
      asymmetry in DEC-286 and leave `item_pattern` as is. Implementation: validate with
      `Regex::new` inside `TryFrom` and keep the `String`; the lint cache below still compiles
      per file run (see SCHEMA-2) so nothing new is stored in the schema type.
- [ ] `merged_schema_for_type` (line ~258) and `from_raw_lossy` need no change; add a test that a
      profile default plus a type override of the same object-list property merges by
      replacement, not by key union.

### SCHEMA-2: validation (hyalo-mdlint)

- [ ] `crates/hyalo-mdlint/src/schema.rs` `validate_constraint` (line ~868): add the
      `ObjectList` arm. Non-array value → one error (`property "sources" must be a list of
      maps`). Per item: non-map string → `property "sources": item 2 must be a map, not a
      string; did you mean \`- ref: https://example.org/post\`?`; other non-map → `item 2 must be
      a map`; missing required key → `item 0: missing required key "ref"`; unknown key →
      `item 1: unknown key "rev" (allowed: ref, commit, version, updated, read)`; pattern
      mismatch → `item 1: key "commit" value "3c9e0f2x" does not match ^[0-9a-f]{7,40}$`;
      non-scalar under a pattern key → `item 1: key "ref" must be a scalar`. Reuse the
      `regex_cache` parameter for the per-key patterns; keep the message prefix format
      identical to the `StringList` arm so `lint --format github` annotations stay uniform.
- [ ] Severity is `Error`, like every other constraint violation in the `SCHEMA` group
      (`crates/hyalo-cli/src/commands/lint/file.rs` line ~256); `--strict` changes nothing for
      it. No new `kind` constant unless the CLI needs to filter on it.
- [ ] `--fix`: no fixer. `apply_fixes` (line ~423) is untouched. Fix the existing misreport
      while here: `file.rs` line ~256 marks every SCHEMA violation `autofixable: true` except
      `missing-required-no-default`; make object-list violations (and, if cheap, `item_pattern`
      / `pattern` mismatches) report `autofixable: false`. If that needs a new `kind`, add
      `schema/constraint-violation`. Record the choice in DEC-286.
- [ ] `validate_constraint_simple` (line ~1174) needs no change; confirm it returns the first
      object-list message.
- [ ] `crates/hyalo-cli/src/commands/new.rs` line ~275: the exhaustive match on
      `PropertyConstraint` must scaffold an object-list property as an empty list `[]`, same as
      `List | StringList`.

### SCHEMA-3: `types show` rendering

- [ ] `crates/hyalo-cli/src/commands/types.rs` `constraint_to_json` (line ~87): emit
      `{"type": "object-list", "required-keys": [...], "allowed-keys": [...] (omit when None),
      "key-patterns": {"ref": "…", …} (omit when empty)}`. Key names mirror the TOML spelling.
- [ ] `crates/hyalo-cli/src/output/text_types.rs` `format_type_show_text` (line ~26): today it
      joins arrays with `, ` and has no branch for a nested object. Add one: a map value prints
      as an indented block (`key-patterns:` then `      ref: ^…` lines), keeping the generic
      key/value dump for everything else. Add a unit test next to `show_type_with_enum_constraint`
      (`types.rs` line ~1072) and an e2e text-mode assertion.
- [ ] `hyalo config` does not echo schema types (`crates/hyalo-cli/src/commands/config.rs` only
      reports `exempt`), so nothing to do there; note it in the plan so nobody looks.
- [ ] `types set --property-type` help (`crates/hyalo-cli/src/cli/args.rs` line ~2495) lists 8
      accepted types; leave the list truthful (it already omits `string-list`) and add one
      sentence: "`string-list` and `object-list` carry constraints and are configured in
      `.hyalo.toml` only; see `hyalo types show`". No flag is added.

### SCHEMA-4: write-time validation

- [ ] `set --validate` / `validate_on_write` (`crates/hyalo-cli/src/commands/set.rs` line ~481)
      and `append` (`crates/hyalo-cli/src/commands/append.rs` line ~267) both go through
      `validate_constraint_simple`, so an object-list property is validated as soon as the arm
      exists. Verify with a test that `hyalo set f.md -p 'sources=[a,b]' --validate` is refused
      (items are strings) and that `hyalo append f.md -p sources=x --validate` is refused. The
      advisory `note` on `set` without `--validate` (line ~157) should also fire.
- [ ] Do not add any `set`/`append` syntax for object items. Document in DEC-286 that authoring
      object items is an editor concern until a `set --property 'sources[]=ref=…'` style is
      designed separately (explicitly out of scope, no backlog item created by this iteration).

### SCHEMA-5: tests

- [ ] Unit, `crates/hyalo-core/src/schema.rs` tests module (next to
      `parse_string_list_with_item_pattern`, line ~1190): full parse of the TOML above; each
      rejection in SCHEMA-1 (`pattern` on object-list, `required-keys` on `string`, key not in
      `allowed-keys`, invalid regex in `key-patterns`, unknown key inside the property table,
      `key-patterns` value not a string); `allowed-keys` omitted → `None`.
- [ ] Unit, `crates/hyalo-cli/src/commands/lint/tests.rs` (next to
      `item_pattern_validates_list_items`, line ~906): valid two-object list passes; string
      item → message contains `did you mean \`- ref: …\``; `rev:` typo → unknown key message
      lists the allowed keys; bad `commit` → pattern message; `ref: [a, b]` → scalar message;
      missing `ref` → required-key message; multiple bad items all reported; empty list passes;
      scalar value
      → one error; `list` and `string-list` fixtures unchanged (run the existing tests).
- [ ] e2e, `crates/hyalo-cli/tests/e2e/lint.rs` (next to
      `lint_item_pattern_reports_all_violations`, line ~424): a vault with the TOML above and
      four files (valid, string item, `rev:` typo, bad commit hash); `hyalo lint --strict
      --format json` exits 1, the valid file has no violations, each bad file has exactly one
      violation whose message names the item index and key; `--format github` emits one
      `::error` per violation; `lint --fix --dry-run` proposes nothing for them and reports
      `autofixable: false`.
- [ ] e2e, `crates/hyalo-cli/tests/e2e/types.rs` (next to `types_show_existing_type`, line
      ~106): `hyalo types show memory --format json` contains `required-keys`, `allowed-keys`,
      `key-patterns.commit`; `--format text` shows the indented `key-patterns:` block.
- [ ] e2e, dot-path interaction (same vault, `lint.rs` or `find.rs`): `hyalo find --property
      sources.ref=github:comparis/neon` returns only the valid file; the string-item file is
      absent from the result and `hyalo lint` on it names item 0 — this is the pairing that
      closes the hazard. Add a unit test in `crates/hyalo-core/src/filter/mod.rs` for a mixed
      string/map list (`dot_path_array_skips_scalar_items`) pinning today's skip behaviour.
- [ ] Config-load e2e: an invalid `key-patterns` regex makes `hyalo lint` report
      `schema/malformed` (error under `--strict`) and `hyalo set --validate` refuse to write,
      via the existing `validate_schema_config` path in `lint/config_checks.rs`.

### SCHEMA-6: docs, decision, backlog

- [ ] Help texts: `lint --help` (`args.rs` lines ~1693–1697, the schema-extensions paragraph),
      `types show --help` (line ~2475), `types set --help` (line ~2495, the sentence from
      SCHEMA-3). Run `cargo xtask check-help-drift`; the `-h` byte ceilings must hold, so put
      the detail in `--help`, not `-h`.
- [ ] `crates/hyalo-cli/templates/skill-hyalo.md` line ~474 (symlinked as
      `.claude/skills/hyalo/SKILL.md`): add `object-list` to the property-type list with the
      three keys, one example, and the "config-only, no `types set` flag" note.
- [ ] `docs/configuration.md` line ~324: add `object-list` to the type list.
      [[docs/schema-and-lint]] "Property Types" section: add the type, the TOML example and the
      violation messages. The core `schema.rs` module doc block (lines 5–29) gets the example.
- [ ] `hyalo changelog add` under Added: "Schema: `object-list` property type with
      `required-keys`, `allowed-keys`, `key-patterns` for lists of maps; lint reports item
      index and key, string items get a `- ref:` fix-it hint". Under Fixed: SCHEMA-group
      constraint violations no longer report `autofixable: true` when `--fix` has no fixer.
- [ ] DEC-286 in [[decision-log]]: object-list semantics (flat, config-only, no `types set`
      surface, compile-once regex with load-time failure vs `item_pattern`'s lint-time report,
      all violations reported, `autofixable: false`), with the mapl-memory `sources:` case as
      context and the rejected alternatives (JSON Schema import; `list` + per-key `pattern`
      sugar; nested types).
- [ ] Backlog: tick the acceptance boxes in [[backlog/done/object-list-schema-type]] as they land,
      set `status=completed` and `hyalo mv` it to `backlog/done/` at the end. The backlog schema
      has no "scheduled" status and no link property, so until then the item stays `planned`;
      this iteration's `related` property is the link, visible through
      `hyalo backlinks backlog/object-list-schema-type.md`.

## Acceptance criteria

- [ ] Scratch vault with the Goal's TOML (type `memory`, files bound by `type: memory`) and four
      files `valid.md`, `string-item.md`, `typo-key.md` (`rev: 3c9e0f2`), `bad-commit.md`
      (`commit: zzz`): `hyalo lint --strict --format json --jq '.results.files[] |
      {file, msgs: [.violations[].message]}'` shows `valid.md` with `[]`, `string-item.md` with
      one message containing `item 0` and `- ref: `, `typo-key.md` with one containing `item 0`
      and `unknown key "rev"`, `bad-commit.md` with one containing `item 0`, `key "commit"` and
      the pattern; exit code 1. `hyalo lint --fix --dry-run --count` on that vault proposes 0
      fixes.
- [ ] Same vault: `hyalo types show memory --format json --jq
      '.results.properties.sources | keys'` → `["allowed-keys","key-patterns","required-keys",
      "type"]`; `hyalo types show memory --format text` prints a `key-patterns:` block with
      `commit`, `read`, `ref` lines.
- [ ] Same vault: `hyalo find --property sources.ref=github:comparis/neon --format json --jq
      '[.results[].file]'` → `["valid.md"]` (the string-item file never matches, and lint is
      what reports it).
- [ ] Same vault: `hyalo set string-item.md -p 'sources=[x]' --validate` exits 1 with the
      object-list message; without `--validate` it writes and the JSON result carries a `note`.
- [ ] Vault whose `key-patterns.commit` is `[` : `hyalo lint --strict --count` reports a
      `schema/malformed` error naming `property 'sources'`, `key-patterns.commit` and the
      regex error; `hyalo config --jq '.results.malformed'` stays `false` (the TOML parses,
      the schema does not).
- [ ] `cargo test --workspace -q`: every existing `list` / `string-list` / `item_pattern` test
      still passes unchanged.
- [ ] `hyalo types set memory --property-type sources=object-list` is rejected with the same
      unknown-type error as `string-list` today, and `hyalo types set --help` explains why.
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, `cargo xtask
      check-help-drift`.
- [ ] Changelog entries added with `hyalo changelog add`; DEC-286 recorded in [[decision-log]];
      skill file, `docs/configuration.md` and [[docs/schema-and-lint]] updated in the same PR;
      [[backlog/done/object-list-schema-type]] boxes ticked, status `completed`, moved to
      `backlog/done/`.

## Links

- [[backlog/done/object-list-schema-type]]
- [[decision-log]]
- [[docs/schema-and-lint]]
- [[iterations/done/iteration-138-schema-extensions-and-new-command]] — introduced
  `string-list` / `item_pattern`
- [[iterations/iteration-244-index-remaining-deferrals]] — UX-3, dot-path property filters
- [[iterations/iteration-266-properties-tags-schema-mutations]] — `types set` binding fixes,
  same code area, no overlap
