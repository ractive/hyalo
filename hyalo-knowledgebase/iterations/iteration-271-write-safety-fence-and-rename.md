---
type: iteration
title: "Iteration 271 — Write safety: column-0 closing fence, emitter guard, empty rename target"
date: 2026-09-05
status: planned
tags:
  - iteration
  - frontmatter
  - mutations
  - dogfooding
branch: iter-271/write-safety-fence-and-rename
priority: 1
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[decision-log]]"
---

# Iteration 271 — Write safety: column-0 closing fence, emitter guard, empty rename target

## Goal

Two write-path bugs from [[dogfood-results/dogfood-v0220-post-batch-261-270]] that destroy
frontmatter with exit 0. Both live in `hyalo-core` and neither needs a design decision beyond
one DEC on fence strictness, so they ship as one PR. This is group 1 of the report's
recommended iterations; BUG-1 (concurrent writers) was closed won't-fix in DEC-292 and is out
of scope.

Constraint: **no new CLI flags**. Both fixes tighten existing behaviour.

## Part A — an indented `  ---` inside the frontmatter is not a closing fence (BUG-2, HIGH)

### Background

`is_closing_delimiter` in `crates/hyalo-core/src/frontmatter/parse.rs` accepts any line whose
trimmed text is `---` as the closing fence, documented in a comment as deliberate leniency. No
DEC covers it, and both YAML and Obsidian close only at column 0. Consequence:

```text
printf -- '---\ntitle: Ind\nk: |-\n  a\n  ---\n  b\nafter: 1\n---\nREALBODY\n' > ind.md
hyalo read ind.md --frontmatter --jq '.results.frontmatter'   # {"k":"a","title":"Ind"} — after: lost
hyalo set ind.md --property z=1                                # replaces the "  ---" line with "z: 1" + "---"
```

hyalo produces the trigger itself: `hyalo set f.md --property "k=$(printf 'a\n---\nb')"`
emits `k: |-` with an indented `---` line, and `find --file f.md` then reads `k: "a"`. The
next mutation destroys the block. `lint` reports nothing.

### FENCE-1: strict column-0 close

- [ ] Record the decision in [[decision-log]]: the closing fence is a line that is exactly
      `---` (optionally followed by whitespace or `\r`) at column 0, matching the opener. State
      why the leniency existed (the comment's consolidation argument) and why it loses.
- [ ] Census before changing anything: for every testbed (`hyalo-knowledgebase/`,
      `../obsidian-hub`, `../kepano-obsidian`, `../mdn/files/en-us`, `../docs/content`) count
      the files whose parse outcome differs between lenient and strict close. Record the counts
      in the Outcome section. A non-zero count is not a blocker but each such file must be
      looked at: a file that only parsed because of leniency is malformed and HYALO005 should
      say so after the change.
- [ ] Implement strict close in `parse.rs`; keep CRLF handling (`---\r`) and the BOM path
      intact (both are pinned by existing tests in the same module).
- [ ] Unit tests: the BUG-2 fixture parses to `k = "a\n---\nb"`, `after = 1`, body `REALBODY`;
      an indented `  ---` as the *last* frontmatter line is not a close (the file is then
      unterminated → HYALO005, not silently truncated); `---` followed by trailing spaces still
      closes; a body-level `---` (thematic break) after a real close is untouched.
- [ ] e2e: `read --frontmatter`, `find --file --fields properties`, `set`, `append`, `remove`
      on the fixture all see the full map; `set z=1` adds `z` after `after` and leaves
      `REALBODY` as the body; the file diff is exactly one added line.

### FENCE-2: emitter guard

- [ ] The YAML emitter used by `set`/`append` must never write a block scalar containing a line
      that trims to `---` (or `...`) in a way the strict reader would misread. Choose one:
      double-quote the scalar (`"a\n---\nb"`) when any line trims to a fence marker, or refuse
      with an error naming the offending line. Prefer quoting — it round-trips and asks nothing
      of the user. Record the choice in the same DEC.
- [ ] Round-trip test: `set --property "k=$(printf 'a\n---\nb')"` then `find --file` returns
      the three-line string, `read --frontmatter` echoes bytes that re-parse identically, and a
      second `set` on another key changes exactly one line.
- [ ] Check the multi-line *quoted* scalar case from the report (`k: "x\n  ---\n  y"`) parses
      once the fence is strict — it failed before only because the fence was mis-detected.

## Part B — `properties rename --to ''` must be rejected (BUG-13, MEDIUM)

```text
hyalo properties rename --from title --to ''   # exit 0; every file now has "": Note 2
```

Titles then fall back to the filename stem (DEC-283), so the loss is invisible in `find`.

### RENAME-1

- [ ] Reject empty, whitespace-only, and otherwise invalid keys for both `--from` and `--to`
      with exit 1 and the same message shape `types set ''` and `set --property '=v'` use. Reuse
      whatever validator those paths share rather than adding a third.
- [ ] While there: `--from X --to X` (no-op) should say so and exit 0 without touching files;
      confirm `--to <existing key>` still reports `conflicts` and writes nothing (pinned by an
      existing e2e — keep it green).
- [ ] e2e for the empty-target rejection in both text and JSON envelopes, `--dry-run` included.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per part).
- [ ] The fence DEC recorded in [[decision-log]].
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates.
      Run xtask as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`
      (`cargo run -p xtask` deadlocks on the nested build lock).

## Acceptance criteria

- [ ] The BUG-2 repro: `read --frontmatter` returns `after: 1` and the three-line `k`; `set
      --property z=1` leaves `REALBODY` as the body and the diff is one added line.
- [ ] `set --property "k=$(printf 'a\n---\nb')"` round-trips through `find --file` byte-exact
      in meaning (three-line string) and the next mutation does not destroy it.
- [ ] Strict-vs-lenient census recorded for all five testbeds; every file whose outcome changed
      is listed with a one-line verdict (malformed and now reported, or unaffected).
- [ ] `hyalo properties rename --from title --to ''` and `--from ''` exit 1 with nothing
      written; `--to <existing>` still reports conflicts and writes nothing.
- [ ] Existing frontmatter tests (BOM, CRLF, no-EOL, `---\n---\n`, thematic break in body,
      45 hostile scalar values from the report's adversarial pass) stay green.
- [ ] Gates green; two changelog entries; one DEC.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-2, BUG-13
- [[decision-log]] — DEC-292 (concurrent writers won't-fix, out of scope here)
