---
title: >-
  Iteration 138 — Schema extensions (item_pattern, required_sections) + `hyalo
  new` scaffolder
type: iteration
date: 2026-05-24
status: completed
branch: iter-138/schema-extensions-and-new-command
tags:
  - iteration
  - schema
  - lint
  - new-command
  - consumer-tooling
related:
  - "[[research/ff-rdp-discipline-consumer-notes]]"
  - "[[decision-log#DEC-041]]"
---

## Goal

Three small, related schema-side extensions that let consumer projects
(ff-rdp, future similar repos) delete in-house discipline tooling and
treat hyalo as the single source of truth for "what does a valid
markdown file of type X look like?":

1. **`item_pattern`** on `string-list` properties — per-item regex
   validation, caught at `hyalo lint` time. Closes the
   `first_call_sites: ["pub_fn => crates/foo/bar.rs", ...]` shape
   without resorting to nested-object frontmatter.

2. **`required_sections`** on type schemas — declares the body outline a
   document of this type must contain. Hash-prefix syntax encodes the
   heading level (`"## Tasks"`, `"### Subtasks"`). Order-significant,
   extras allowed.

3. **`hyalo new --type=<name> --file=<path>`** — schema-driven
   scaffolder that emits a placeholder skeleton (required frontmatter +
   required sections, all values `TBD` / type-appropriate empties).
   Zero templating engine — schema declarations are the only source of
   truth. The intentionally-invalid file is then validated by lint;
   that's the agent feedback loop.

Companion to [[research/ff-rdp-discipline-consumer-notes]].

## Steps

### `item_pattern` — per-item regex on string-list properties [7/7]

- [x] Extend `[schema.types.X.properties.Y]` to accept `item_pattern`
      when `type = "string-list"`.
- [x] Schema-load error if `pattern` and `item_pattern` are both set on
      the same property, or if `item_pattern` appears on a non-list
      property.
- [x] Compile regex at schema load (cache per pattern, mirror the
      existing scalar `pattern` machinery).
- [x] Validation at lint time: iterate each list item, regex-match
      against `item_pattern`. Empty list = vacuous pass.
- [x] Non-string items in the list → hard error
      ("`item N`: expected string, found <kind>").
- [x] Error report shape: file, property name, 0-based item index, the
      value, the pattern. Point at the property's frontmatter line range
      (parser already exposes this).
- [x] `hyalo find --property X~=regex` is already supported on lists
      (verified in `backlog/done/property-regex-yaml-lists.md`) — no CLI
      changes needed.

### `required_sections` — declared body outline [7/7]

- [x] Extend `[schema.types.X]` to accept `required_sections`: an
      ordered list of strings, each `"<hashes> <text>"`, e.g.
      `["# Title", "## Themes", "### Subtasks"]`.
- [x] Validation: walk the document body's normalized ATX heading
      sequence (setext is already normalized in `heading.rs`), match
      each `required_sections` entry against the next-or-later heading
      with the same level and trimmed text. Order-significant: an entry
      cannot match a heading earlier than the previous entry matched.
- [x] Extras allowed — headings not in `required_sections` are silently
      tolerated.
- [x] Trim trailing whitespace after the hashes when comparing
      (`"##  Tasks"` matches `"## Tasks"`).
- [x] Exact-text match. No case-insensitivity, no regex, no emoji
      stripping.
- [x] No level-hierarchy validation (no "`###` must be under `##`") —
      defer to markdownlint MD001.
- [x] Error report per missing section: file, expected level + text,
      where in the outline it should have appeared.

### `hyalo new --type=<name> --file=<path>` [9/9]

- [x] New subcommand under `cli::args::Command::New`.
- [x] Mandatory args:
      `--type <name>` (must match a `[schema.types.X]`),
      `--file <vault-relative-path>`.
- [x] **No `--force`.** Refuse with a clear error if the target file
      exists ("file already exists at <path>; remove it first if you
      mean to re-create").
- [x] **No `mkdir -p`.** Refuse with a clear error if the parent dir
      doesn't exist ("parent directory <path> does not exist; create it
      first").
- [x] Synthesise the file from schema:
  - Frontmatter: `type: <name>` (always); each `required` property
    with a type-appropriate placeholder:
    - string → `"TBD"`
    - number → `0`
    - bool → `false`
    - list → `[]`
    - date (declared type=date) → today's ISO (`2026-05-24`)
    - enum (declared `values = [...]`) → first declared variant
  - Body: if `required_sections` is set on the type, emit each heading
    at its declared level with a `TBD` paragraph and a blank-line
    separator. If unset, emit just frontmatter.
- [x] Unknown `--type`: error with the list of available types from
      `[schema.types.*]`.
- [x] Missing `--type` or `--file` with `hyalo new`: error with usage
      hint.
- [x] Output (JSON envelope): `{type, file, created: true}`. Text mode:
      one-line `created kb/iterations/iter-XX-slug.md`.
- [x] Hint after success: "Run `hyalo lint --file <path>` to see
      placeholder violations".

### Docs + UX surfaces [8/8]

- [x] `hyalo new --help` text with examples (the help must show the
      exact flag shape the agent will copy).
- [x] `hyalo lint --help`: mention `item_pattern` and `required_sections`
      as the new schema features it enforces.
- [x] `hyalo types show <name> --help`: ensure the output includes any
      `item_pattern` / `required_sections` declared on the type.
- [x] `README.md`: add `hyalo new` to the command summary table; add a
      one-paragraph example showing the agent loop
      (`new` → edit → `lint`).
- [x] `crates/hyalo-cli/templates/rule-knowledgebase.md`: add the
      `hyalo new` line under the existing skill conventions so agents
      reading the rule pick it up automatically.
- [x] `.claude/CLAUDE.md` and the root `CLAUDE.md` references to the
      rule template (none direct — it's symlinked, so the template
      update covers both).
- [x] CHANGELOG `Unreleased` entry under Added (item_pattern,
      required_sections, `hyalo new`).
- [x] Decision-log: add DEC-043 for the schema-as-source-of-truth +
      no-templating-engine call. Reference
      [[research/ff-rdp-discipline-consumer-notes]].

### Hints [5/5]

> All four follow-up hint integrations landed in iter-143
> ([[iterations/iteration-143-hint-and-files-from-polish]]).
> Per-subcategory SCHEMA hints (`item_pattern`, `required_sections`) were
> consolidated into a single generic `hyalo types show <T>` hint that points
> at the schema declaration — more actionable than paraphrasing the violation
> message.

- [x] `HintSource::New` variant. After a successful `hyalo new`:
      suggest `hyalo lint --file <path>`. After a failed
      `--file <existing-path>`: suggest `rm <path>` (only if the agent
      is in destructive-allowed mode? probably just plain text). After
      missing `--type`: list types from the schema.
- [x] Update existing hints that mention "create a new file" or
      similar to use `hyalo new` instead of suggesting `Write`.
      *Moot — a `grep` confirmed no such "create a new file" hints exist;
      dropped in iter-143 without action.*
- [x] `hyalo types show <name>` hint: when the type has a `template-able`
      shape (i.e. `required` is set), suggest `hyalo new --type <name>
      --file <path>` as a usage example. *Landed in iter-143.*
- [x] Lint hint for missing `required_sections`: suggest editing the
      body to add the missing heading at the right level.
      *iter-143 ships a generic SCHEMA hint linking to `hyalo types show
      <T>` — same actionable target, less paraphrasing.*
- [x] Lint hint for `item_pattern` violation: include the pattern in
      the hint description ("expected pattern: `<pattern>`").
      *Same as above — generic SCHEMA hint covers it.*

## Tasks [12/12]

> All ticked. Per-rule lint hints for `required_sections` and `item_pattern`
> were consolidated into a single generic SCHEMA-→-`types show` hint by
> iter-143; see Hints section above.

- [x] Implement `item_pattern` schema field + validation
- [x] Implement `required_sections` schema field + validation
- [x] Implement `hyalo new` subcommand
- [ ] Wire `HintSource::New` + per-rule lint hints
- [x] Update all help texts (CLI args, subcommand descriptions, examples)
- [x] Update README.md command table and add `hyalo new` example
- [x] Update `templates/rule-knowledgebase.md`
- [x] Add CHANGELOG `Unreleased` entry
- [x] Add decision-log DEC-043
- [x] Unit tests: each schema feature (positive + negative cases)
- [x] E2E tests: `hyalo new` happy path, file-exists, parent-dir-missing,
      unknown-type, missing-args, validate that `hyalo lint` immediately
      surfaces `TBD` violations on the produced file
- [x] Cross-platform CI verification (macOS + Ubuntu + Windows) — must
      stay green after iter-137

## Acceptance criteria [7/7]

- [x] `hyalo lint` flags `item_pattern` violations per item with index
      and pattern in the message
- [x] `hyalo lint` flags missing `required_sections` with level and text
- [x] `hyalo new --type=iteration --file=iterations/iter-99-foo.md`
      creates a skeleton; `hyalo lint --file iterations/iter-99-foo.md`
      reports the `TBD` placeholder violations
- [x] `hyalo new` refuses (with clear error) when the file exists OR
      when the parent dir doesn't exist
- [x] README, help texts, rule template, CHANGELOG, decision-log all
      updated in the same PR
- [x] All three CI platforms green
- [x] `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`
      green

## Design notes

- **No templating engine.** The schema declarations ARE the template.
  Zero `{var}` substitution. The one "smart default" is today's ISO
  for `date`-typed properties — and that's typed-default behaviour,
  not templating.
- **Schema, not directory layout, determines type.** `--file` is fully
  specified by the caller; hyalo does not derive a `dir` from the
  type. (Rejected design alternative: `[schema.types.X] dir =
  "iterations/"` to infer the destination.)
- **Intentionally-invalid output.** `hyalo new` emits placeholders
  designed to fail subsequent `hyalo lint`. The lint loop is how the
  agent learns what to fill in. Pre-validated output would defeat the
  point.
- **`required_sections` defers heading-hierarchy correctness** to
  markdownlint MD001. We check presence + level, not level-skipping.
- **Per-item regex applies to `string-list` only.** Object-typed lists
  and other shapes are out of scope; we explicitly reject nested-object
  frontmatter as a direction (see consumer notes).

## Out of scope

- Templating mini-DSL (literal `{var}`, expressions, conditionals). The
  agreed pattern is "schema declarations as template" — no syntax.
- `dir` field on type schemas (rejected — agent specifies `--file`).
- `--force` flag on `hyalo new` (rejected — agent removes the file if
  they mean to re-create).
- `mkdir -p` on `hyalo new` (rejected — agent creates the dir first).
- Bulk creation (`hyalo new --type=iteration --batch=...`). Single-file
  is the unit.
- `--files-from <path|->` on lint/find — moved to iter-139 to keep this
  iter focused.
- Per-section `non_empty = true` constraint in `required_sections`.
  Defer to a follow-up; first feedback pass first.
- Schema-aware autofix that adds missing required sections — also a
  follow-up; the lint message is enough for v1.

## References

- [[research/ff-rdp-discipline-consumer-notes]] — the consumer wishlist
  and the design dialogue that produced this scope
- [[decision-log#DEC-041]] — markdown linter foundation
- [[backlog/done/property-regex-yaml-lists]] — already-shipped piece
  (regex matching inside YAML list values, via `--property X~=`); this
  iter adds the validation half of the same feature
