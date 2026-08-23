---
title: Iteration 214 — frontmatter format preservation (minimal-diff writes)
type: iteration
date: 2026-08-23
status: in-progress
branch: iter-214/frontmatter-format-preservation
tags:
  - iteration
  - frontmatter
  - ux
related:
  - "[[dogfood-results/dogfood-v0210-pre2-integrity-wave]]"
---

# Iteration 214 — frontmatter format preservation (minimal-diff writes)

## Goal

`set`/`append`/`remove` (and every other frontmatter writer) must touch
only the lines belonging to the keys they change. Untouched frontmatter
lines stay byte-identical, making hyalo usable on version-controlled
docs repos where diff churn is a hard adoption blocker.

## Context

From [[dogfood-results/dogfood-v0210-pre2-integrity-wave]] (BUG-14
cluster, last item): adding one property to a GitHub Docs `index.md`
changed **116 of 198 frontmatter lines** — long list items refolded into
`>-` block scalars, title quote style flipped `'` → `"`. Round-trip
comparison confirmed semantic preservation, so this is churn rather than
loss — but a one-key change producing a 116-line diff is unreviewable
and makes `hyalo set` unusable in any repo where frontmatter is under
code review. The current writer parses the whole frontmatter into
`serde_yaml` values and re-serializes everything.

Likely approach (verify against the actual writer code first): targeted
line splicing — locate the changed key's line span in the raw
frontmatter text, replace only that span with newly serialized YAML for
that key, leave every other byte alone. Falling back to full
re-serialization only when the file's YAML cannot be span-mapped
(anchors/aliases, exotic constructs) — and warning when that fallback
triggers.

## Tasks

- [x] Research the current write path (`write_frontmatter` and callers)
      and the span information available from the YAML parser; record
      the chosen mechanism as a DEC (targeted splice vs format-preserving
      YAML crate vs other).
- [x] Implement minimal-diff writes for `set`, `remove`, `append`,
      `task toggle` (frontmatter portion), `mv` (frontmatter-touching
      paths), and `types`/`lint-rules`/`views set` writes to
      `.hyalo.toml` if they share the problem (verify; scope to what
      actually churns).
- [x] Preservation test corpus: nested objects, block scalars (`>-`,
      `|`), single/double/unquoted strings, flow and block lists,
      comments, unusual indentation, CRLF — assert that changing key X
      leaves every line not belonging to X byte-identical. Use real
      GitHub Docs frontmatter shapes as fixtures.
- [x] e2e: repeat the dogfood measurement — add one property to a copied
      GitHub Docs `index.md` and assert the diff touches only the added
      line(s).
- [x] Define and document the fallback: when span-mapping fails, either
      refuse with a clear error or re-serialize with a loud warning —
      pick one, record as a DEC, never silently churn.
- [x] Docs/CHANGELOG: describe the new guarantee and its fallback
      boundary.

## Acceptance criteria

- [x] Adding one property to the GitHub Docs fixture changes only the
      inserted line (was 116 of 198 lines)
- [x] The preservation corpus passes byte-identity for untouched lines
      across all listed YAML shapes
- [x] Fallback behavior is explicit, tested, and never silent
- [x] Existing frontmatter-write e2e suite still green (semantics
      unchanged)
- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Preserving formatting through `lint --fix` body rewrites (different
  code path, already minimal).
- A general YAML formatting style option — this is about not touching
  what the user did not ask to change.
