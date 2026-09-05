---
type: iteration
title: "Iteration 273 — Index and named-file honesty: empty successes, stale probe, per-file anchors, mv paths"
date: 2026-09-05
status: planned
tags:
  - iteration
  - index
  - find
  - mv
  - dogfooding
branch: iter-273/index-and-named-file-honesty
priority: 3
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-265-scan-exclude-and-skipped-files]]"
  - "[[backlog/mv-destination-path-resolved-vault-relative]]"
  - "[[backlog/mv-batch-frontmatter-link-scan-gap]]"
  - "[[decision-log]]"
---

# Iteration 273 — Index and named-file honesty

## Goal

Every case from [[dogfood-results/dogfood-v0220-post-batch-261-270]] where hyalo answers a
question about a *named* file, or from the *snapshot index*, with a clean exit 0 and a wrong
or empty answer. Group 4 of the report's recommendations, plus the two open `mv` backlog items
because both are "the path you named is not the path hyalo used". No design questions:
DEC-277/278/280 already state the intended behaviour and these are the gaps between them and
the code.

Constraint: **no new CLI flags**.

## Part A — named files are never an empty success

### NAMED-1: `find --file <unparsable>` exits 1 (BUG-10)

```text
printf -- '---\ntitle: Dup\ntitle: Dup2\n---\n' > dup.md
hyalo find --file dup.md          # results: [], total 0, exit 0 + one skip warning
hyalo set dup.md --property x=1   # error … unparseable frontmatter; nothing was modified, exit 1
```

- [ ] A file named with `--file` or positionally that fails to parse is an error (exit 1, the
      YAML diagnostic, the `lint --rule HYALO005` hint), as `set` already does and as DEC-277
      cites from iteration 204. `--files-from` keeps batch semantics (counted, DEC-284); state
      that asymmetry in `find --help` in one sentence.
- [ ] e2e for `--file`, positional, `--files-from` (counted, not fatal); JSON error envelope.

### NAMED-2: `find --index --file <not in snapshot>` upserts or reports (BUG-11)

- [ ] A named file that exists on disk but is absent from the snapshot is stat-refreshed like
      an in-snapshot file (one stat, one parse — DEC-280's cost argument holds) or reported
      under `files_missing` with a warning. Never `results: []` with exit 0. Same four
      directories deep.
- [ ] e2e on a GitHub Docs subtree copy: `create-index`, add a file, `find --index --file` sees
      it; a file in neither place → `files_missing: 1`.

### NAMED-3: per-file `--broken-links` keeps anchor data (BUG-9)

`find --file Source.md --broken-links` and `--glob … --broken-links` drop `broken_anchor` and
`suggested_fragment`; the positional form and the vault-wide sweep keep them.

- [ ] Route the `--file`/`--glob` path through the same link-record builder as the sweep.
- [ ] e2e: the DEC-268 fixture through `--file`, `--glob`, positional and sweep — identical
      JSON for the link.

### NAMED-4: `lint --rule X` does not leak HYALO005 (BUG-20)

Hub `lint --rule MD018 --count` → 1, the hit being a frontmatter parse error for an unrelated
file; kepano `--rule HYALO006` lists 28 template parse errors.

- [ ] A parse failure is reported under `--rule X` only when X is HYALO005; otherwise the file
      is a counted skip (DEC-278 one-line warning) and excluded from `--count`.
- [ ] e2e with one unparsable file: `--rule MD018 --count` → 0 plus the skip line.

## Part B — the snapshot index tells the truth

### INDEX-1: the stale probe sees in-place edits (BUG-12)

```text
hyalo create-index; sleep 1.1; printf … > n2.md    # overwrite an existing file
hyalo find --index --property status=final          # n2.md missing, no warning, exit 0
```

DEC-280's directory-mtime probe cannot see an in-place overwrite on APFS; a new file does warn.
The index already stores per-file mtimes.

- [ ] Fold per-file mtime/size from the snapshot into the probe: warn when any indexed file's
      on-disk stat differs. Measure one `stat` per file on MDN (~14k syscalls); if too slow,
      stat the N most recently modified plus the directory probe and document the residual blind
      spot. Record as a DEC-280 amendment in [[decision-log]].
- [ ] e2e: overwrite an indexed file, `find --index` warns; same-second case documented.

### INDEX-2: `summary --index` reports the excluded count (BUG-18)

- [ ] `[scan] exclude` filtering at snapshot load keeps the dropped count and `summary` reports
      it; parity test disk vs index on a kepano copy with `Templates/**` excluded
      (`{total:51, skipped:0, excluded:52}` both ways).

### INDEX-3: invalid-UTF-8 files stay out of `--index` reads (previous report BUG-14, PARTIAL)

- [ ] `find --file bad.md --index` gives the disk scan's answer (the "excluded from full-text
      search" sentence; `find -e` still matches lossily). Parity test.

## Part C — `mv` uses the path you named

### MV-1: destination is vault-prefix-stripped like the source (BUG-14, [[backlog/mv-destination-path-resolved-vault-relative]])

With `dir = "kb"` from the parent: `mv kb/a.md kb/sub/a.md`, `--file kb/a.md --to kb/sub/a.md`
and `--glob a.md --to kb/sub/ --apply` all create `kb/kb/sub/a.md`.

- [ ] Route the destination (positional, `--to` file, `--to` directory, batch) through the same
      CWD-relative → vault-relative normalisation the source uses; refuse a destination outside
      the vault with the existing "outside vault boundary" error.
- [ ] Fix the `--to kb/sub/` hint (`did you mean kb/sub/.md?`): a trailing slash is a directory
      destination and single mode should say so.
- [ ] e2e for all four forms from the parent directory and from inside the vault; close the
      backlog item (`status: done`, moved to `backlog/done/`) in this PR.

### MV-2: batch `mv` gets the widened split-link scan ([[backlog/mv-batch-frontmatter-link-scan-gap]])

- [ ] Reuse the `LinkGraphBuild` candidate marker once per batch (the graph is built once
      already) so the cost is one pass regardless of batch size; report
      `frontmatter_links_skipped` per move.
- [ ] e2e; measure batch `mv --glob` on the Hub before/after; close the backlog item.

### MV-3: `--on-conflict` is validated and honoured (BUG-24, BUG-26)

- [ ] `--on-conflict` becomes a clap value enum (bogus values are usage errors); single-file
      `mv` honours `skip` (one branch).
- [ ] The batch collision message distinguishes "two sources map to one destination" from "one
      source collides with an existing file".
- [ ] e2e for both.

## Shared closing tasks

- [ ] Changelog entries via `hyalo changelog add` (one per task group that changes behaviour).
- [ ] DEC-280 amendment recorded; both `mv` backlog items closed.
- [ ] Docs: `find --help` (named-file error vs `--files-from` counting), `mv --help`
      (destination resolution, `--on-conflict` in single mode), `--index` help (stale probe).
- [ ] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [ ] `find --file dup.md` (unparsable) exits 1 with the YAML diagnostic; `--files-from` with
      the same file counts it and exits 0.
- [ ] `find --index --file brand-new.md` returns the file; a file in neither place is
      `files_missing: 1`.
- [ ] `--file`, `--glob`, positional and sweep produce identical link JSON for a broken anchor,
      `suggested_fragment` included.
- [ ] `lint --rule MD018 --count` on a vault with one unparsable file → 0 plus the skip line.
- [ ] Overwriting an indexed file in place makes the next unnamed `--index` read warn; the
      probe's extra cost on MDN is recorded and stays under 0.1 s.
- [ ] `summary --index` and `summary` agree on `excluded`; invalid-UTF-8 parity holds.
- [ ] All four `mv` destination forms land in the vault, never nested; both backlog items
      closed; `--on-conflict bogus` is a usage error; single-file `--on-conflict skip` skips.
- [ ] Gates green; changelog; DEC-280 amendment.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-9, 10, 11, 12, 14, 18, 20, 24, 26
- [[iterations/iteration-265-scan-exclude-and-skipped-files]] — DEC-277/278/280
- [[backlog/mv-destination-path-resolved-vault-relative]]
- [[backlog/mv-batch-frontmatter-link-scan-gap]]
