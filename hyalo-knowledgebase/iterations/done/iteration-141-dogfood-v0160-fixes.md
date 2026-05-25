---
title: Iteration 141 — Dogfood v0.16.0 fixes (seven findings)
type: iteration
date: 2026-05-24
status: completed
branch: iter-141/dogfood-v0160-fixes
tags:
  - iteration
  - bugfix
  - lint
  - files-from
  - help-text
  - llm-ergonomics
related:
  - "[[dogfood-results/dogfood-v0160-deep]]"
  - "[[iteration-138-schema-extensions-and-new-command]]"
  - "[[iteration-139-files-from-flag]]"
  - "[[iteration-140-dogfood-138-139-fixes]]"
---

## Goal

Fix the seven findings from [[dogfood-v0160-deep]]. Two are medium-impact
correctness bugs in iter-138/139 features; one is a low-effort docs-sync
fix; one is the highest-leverage LLM-ergonomics improvement available
(adding `EXAMPLES:` to 16 subcommand `--help` blocks); the rest are UX
polish.

## Issues

### Medium

#### NEW-1 — `item_pattern` reports only the first violation per list

`validate_property` returns `Option<Violation>` in
`crates/hyalo-cli/src/commands/lint.rs` (around line 1090–1147). The
inner `for (i, item) …` loop returns `Some(Violation)` on the first
failing item, so a `string-list` with multiple bad items only surfaces
item 0. Users fix one and re-run, fix the next and re-run, etc.

- [x] Change `validate_property` return type to `Vec<Violation>` (or
      have the `string-list` branch collect into a vec and return all
      at once via the caller). Touch the minimum number of call sites
      — the simpler path is for the list branch to flat-map into the
      caller's accumulator rather than reshape the whole function.
- [x] Same treatment for the `expected string, got <kind>` per-item
      branch (currently also short-circuits).
- [x] Update the unit tests in `lint.rs` that exercise `item_pattern`
      to assert multi-violation output.
- [x] E2E test: vault with `item_pattern = "^[a-z][a-z0-9-]*$"`, file
      with `tags: [Foo, "1bad", "Bar"]` → expect 3 SCHEMA violations
      from one file, all with item indices.

#### NEW-2 — `--files-from` auto-strip only handles single-segment `--dir`

iter-140 BUG-2 fix strips the configured vault dir basename from each
repo-relative input path. It only handles a single path component
(`--dir kb` strips `kb/`). With `--dir files/en-us` (MDN-shaped repos),
inputs like `files/en-us/x.md` are reported as missing.

The stderr hint also uses single-segment phrasing
(`kb/notes/foo.md with --dir kb`), which is unhelpful for multi-segment
users diagnosing the issue.

- [x] In the `--files-from` path-resolution helper, strip the **full**
      configured `dir` prefix from each input line (including any
      intermediate path components), not just the last component.
      Normalize to forward slashes first so a Windows-flavoured `--dir
      docs\guide` still matches `docs/guide/page.md`.
- [x] When `dir` is `.` (root vault), no prefix to strip — current
      behaviour is correct.
- [x] Update the all-missing stderr hint to quote the actual
      configured `dir`, e.g. "if paths include the vault dir prefix
      (e.g. files/en-us/x.md with --dir files/en-us), …".
- [x] E2E test: vault `files/en-us`, input `files/en-us/x.md` →
      resolves to `x.md` and lints clean.
- [x] E2E test: vault `kb` (single segment) → existing behaviour
      unchanged.
- [x] E2E test: ambiguity check — vault `notes`, input
      `notes/notes/foo.md` should resolve to `notes/foo.md` relative to
      the vault (existing precedence: vault-relative first, then
      strip-and-retry). Document this precedence in `--files-from`
      help text.

### Low — docs and UX polish

#### NEW-3 — `hyalo new --help` text is stale after iter-140 BUG-4

`crates/hyalo-cli/src/cli/args.rs:1021` still says "Refuses with an
error if the parent directory does not exist"; line 1034's `--file`
help says "must not exist; parent must exist". iter-140 fixed the code
to `create_dir_all(parent)` but didn't sync the docs.

- [x] Delete the "parent directory does not exist" bullet from
      `CONSTRAINTS:` in the `new` long-help.
- [x] Update the `--file` short-help to: "Vault-relative path for the
      new file (must not exist; parent dirs created if missing)".
- [x] Quick grep `grep -rn "parent must exist\|parent directory does not exist" crates/`
      and clean up any other occurrences.

#### NEW-4 — `--files-from` doesn't trim whitespace

`printf '  edge.md\n' | hyalo find --files-from -` reports the path as
missing because leading whitespace isn't trimmed.

- [x] In the per-line parsing helper, `s.trim()` each line before any
      other processing (after the empty-line skip).
- [x] Unit test: lines with leading spaces, trailing spaces, tabs.
- [x] E2E test: whitespace-padded input lints/finds correctly.

#### NEW-5 — `create-index --index-file` UX trap

The global `--index-file` flag is documented as "pass to any supported
command", but `create-index` writes via `-o / --output`. Passing
`--index-file` to `create-index` is silently ignored; the file goes
to the default location and the user gets a misleading
"failed to load index" warning.

Pick one of two approaches and apply it; do not ship both.

**Option A (preferred)**: accept `--index-file` as a synonym for `-o`
on `create-index` only. Lower surprise: every command that talks
about an index now uses the same flag name. Document in
`create-index --help` that `--index-file` and `-o` are equivalent.

**Option B**: error early if `--index-file` is passed to
`create-index`: "`create-index` writes via `-o/--output`, not
`--index-file`; did you mean `hyalo create-index -o <path>`?" — uses
the project's existing "did you mean" pattern.

- [x] Decision: pick A or B. Recommend A (less friction).
- [x] Apply chosen approach.
- [x] Either way: stop warning about a stale index at the default
      location when `-o` redirected the write target. The "stale
      index" check only makes sense if we're about to use that index.
- [x] E2E test for the chosen approach.

#### NEW-6 — `--files-from` doesn't dedupe input

Pipeline output (e.g. `git log --name-only`) can repeat paths. The
current `--files-from` implementation processes the same path multiple
times — `lint` re-lints, `find` returns duplicates.

- [x] Dedupe after path resolution but before the actual work
      (a `HashSet<String>` keyed on the resolved vault-relative path).
      Preserve **first-seen order** so the output ordering matches the
      input ordering (use an `IndexSet`).
- [x] Document in `--files-from` help text: "Input is deduplicated;
      results follow first-seen order."
- [x] Unit test for the dedupe + order-preservation behaviour.
- [x] E2E test: same path 3× → single entry in output.

#### NEW-7 — Most subcommand `--help` blocks lack EXAMPLES (LLM-ergonomics)

Only `lint`, `mv`, and `new` have an `EXAMPLES:` block. The 16
without: `find`, `set`, `task`, `summary`, `read`, `links`,
`create-index`, `types`, `properties`, `tags`, `backlinks`, `remove`,
`append`, `views`, `init`, `lint-rules`.

LLMs reach for `<cmd> --help` first; they shouldn't have to escalate
to top-level `hyalo help` to find idiomatic patterns.

- [x] Add a 3–6-line `EXAMPLES:` block to each of the 16 subcommands
      listed above. Source material: the top-level `hyalo help`
      cookbook entries (already a good starting set) plus the
      iteration plans that introduced the features.
- [x] Prioritise content quality on the heavy hitters:
  - `find`: BM25 query, `--property` + `--tag` combo, regex
    (`title~=/regex/`), `--section`, `--fields links`, `--files-from`
    with a git-style pipeline.
  - `set`: single scalar, list value, multi-property in one call,
    targeting via `--glob`, write-time date validation.
  - `task`: toggle whole file (`--all`), by section, by line numbers.
  - `read`: `--section "Heading"`, `--lines N:M`, `--fields ...`.
- [x] Update the top-level help cookbook if any new patterns surface
      while doing the per-command examples, so the two stay in sync.
- [x] Lint check: a unit/integration test that every `Command` in the
      clap tree has a non-empty `EXAMPLES:` block (parses
      `long_about`). Future-proofs the contract.

## Tasks

- [x] NEW-1: change `item_pattern` validator to collect all violations
- [x] NEW-1: same for `expected string, got <kind>` branch
- [x] NEW-1: unit + e2e tests for multi-violation output
- [x] NEW-2: strip full `--dir` prefix in `--files-from` resolution
- [x] NEW-2: update all-missing stderr hint to quote actual `dir`
- [x] NEW-2: e2e tests (multi-segment, single-segment unchanged,
  ambiguity precedence)
- [x] NEW-3: scrub stale "parent must exist" wording in args.rs
- [x] NEW-3: grep audit for any other occurrences
- [x] NEW-4: trim whitespace per `--files-from` line
- [x] NEW-4: unit + e2e tests
- [x] NEW-5: decide A or B; apply
- [x] NEW-5: stop warning about default-location stale index when `-o`
  redirected
- [x] NEW-5: e2e test
- [x] NEW-6: dedupe `--files-from` input via `IndexSet`
- [x] NEW-6: document in help text
- [x] NEW-6: unit + e2e tests
- [x] NEW-7: add `EXAMPLES:` blocks to all 16 missing subcommands
- [x] NEW-7: prioritise quality on `find`, `set`, `task`, `read`
- [x] NEW-7: lint/integration test requiring `EXAMPLES:` on every
  command
- [x] CHANGELOG `Unreleased` entries under Fixed and Added
- [x] Cross-platform CI verification (macOS + Ubuntu + Windows)

## Acceptance criteria

- [x] `tags: [Foo, "1bad", "Bar"]` against `item_pattern =
      "^[a-z][a-z0-9-]*$"` produces three SCHEMA violations, one per
      item, each citing its index (NEW-1)
- [x] `git diff --name-only | hyalo lint --files-from -` works against
      a vault with multi-segment `--dir` (e.g. `files/en-us`); inputs
      resolve, `files_missing = 0` (NEW-2)
- [x] The all-missing stderr hint quotes the actual configured `dir`
      (NEW-2)
- [x] `hyalo new --help` no longer mentions "parent must exist"
      (NEW-3)
- [x] `printf '  x.md\n' | hyalo find --files-from -` resolves `x.md`
      after whitespace trim (NEW-4)
- [x] `create-index --index-file <path>` either writes to `<path>`
      (Option A) or returns a clear "did you mean -o?" error
      (Option B); no silent ignore (NEW-5)
- [x] Duplicate input lines to `--files-from` produce a single result
      per resolved path, in first-seen order (NEW-6)
- [x] Every `hyalo <subcommand> --help` includes an `EXAMPLES:` block
      with at least 2 concrete invocations (NEW-7)
- [x] An automated check guards against EXAMPLES regressions on new
      commands (NEW-7)
- [x] CHANGELOG `Unreleased` updated under Fixed and Added
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace` green on all three CI platforms

## Design notes

- **NEW-2 ambiguity precedence.** With vault `notes` and input
  `notes/notes/foo.md`, two interpretations exist: (a) the literal
  vault-relative path `notes/notes/foo.md`, (b) repo-relative with
  prefix stripped → `notes/foo.md`. Preserve iter-140 BUG-2's
  precedence: try vault-relative first, then strip-and-retry only if
  it missed. No info loss either way. Same precedence for
  multi-segment vault dirs.
- **NEW-5 — why prefer Option A?** The project's invariant should be
  "one flag name per concept". `--index-file` is the concept "the
  snapshot's path". `create-index` is the only command that writes
  it; `-o` is a clap convention but `--index-file` would be the more
  self-consistent name. Keeping both as synonyms (with `-o` as the
  documented preference for write, `--index-file` for read) keeps
  scripts working.
- **NEW-6 — order preservation.** `git diff` output is implicitly
  ordered (alphabetical, typically). Dropping order on dedupe would
  surprise callers piping into `xargs` for parallel work. `IndexSet`
  is cheap.
- **NEW-7 — testing the contract.** A unit test that walks
  `Command::get_subcommands()` recursively and asserts each one has a
  non-empty `EXAMPLES:` section in its `long_about` is the right
  mechanism. Don't ship without it; otherwise the next new command
  silently regresses the LLM-ergonomics fix.

## Out of scope

- A general redesign of the `--index-file` / `-o` / `--output` flag
  family. NEW-5 is a targeted fix, not a flag spring-clean.
- Auto-discovering "the user probably meant" repo-relative paths
  beyond a single dir-prefix strip. (NEW-2 keeps strip semantics
  conservative.)
- Cleaning up the 8 lint errors in this repo's own KB (bare
  checkboxes in iter-103/14, HYALO002 in iter-103). Separate
  housekeeping; not blocking iter-141.
- "Property vs frontmatter" terminology rename. The asymmetry was
  noted in dogfood-v0160 but a rename is a much larger surface change
  than the dogfood findings warrant.
- `hyalo summary`'s null-valued top-level keys (`broken_links`,
  `untyped`, `untagged`). Cosmetic; defer.
