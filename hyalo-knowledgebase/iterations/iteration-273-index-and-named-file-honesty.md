---
type: iteration
title: "Iteration 273 — Index and named-file honesty: empty successes, stale probe, per-file anchors, mv paths"
date: 2026-09-05
status: completed
tags:
  - iteration
  - index
  - find
  - mv
  - dogfooding
branch: iter-273/index-named-file-honesty
priority: 3
related:
  - "[[dogfood-results/dogfood-v0220-post-batch-261-270]]"
  - "[[iterations/iteration-265-scan-exclude-and-skipped-files]]"
  - "[[backlog/done/mv-destination-path-resolved-vault-relative]]"
  - "[[backlog/done/mv-batch-frontmatter-link-scan-gap]]"
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

- [x] A file named with `--file` or positionally that fails to parse is an error (exit 1, the
      YAML diagnostic, the `lint --rule HYALO005` hint), as `set` already does and as DEC-277
      cites from iteration 204. `--files-from` keeps batch semantics (counted, DEC-284); state
      that asymmetry in `find --help` in one sentence.
- [x] e2e for `--file`, positional, `--files-from` (counted, not fatal); JSON error envelope.

### NAMED-2: `find --index --file <not in snapshot>` upserts or reports (BUG-11)

- [x] A named file that exists on disk but is absent from the snapshot is stat-refreshed like
      an in-snapshot file (one stat, one parse — DEC-280's cost argument holds) or reported
      under `files_missing` with a warning. Never `results: []` with exit 0. Same four
      directories deep.
- [x] e2e on a GitHub Docs subtree copy: `create-index`, add a file, `find --index --file` sees
      it; a file in neither place → `files_missing: 1`.

### NAMED-3: per-file `--broken-links` keeps anchor data (BUG-9)

`find --file Source.md --broken-links` and `--glob … --broken-links` drop `broken_anchor` and
`suggested_fragment`; the positional form and the vault-wide sweep keep them.

- [x] Route the `--file`/`--glob` path through the same link-record builder as the sweep.
- [x] e2e: the DEC-268 fixture through `--file`, `--glob`, positional and sweep — identical
      JSON for the link.

### NAMED-4: `lint --rule X` does not leak HYALO005 (BUG-20)

Hub `lint --rule MD018 --count` → 1, the hit being a frontmatter parse error for an unrelated
file; kepano `--rule HYALO006` lists 28 template parse errors.

- [x] A parse failure is reported under `--rule X` only when X is HYALO005; otherwise the file
      is a counted skip (DEC-278 one-line warning) and excluded from `--count`.
- [x] e2e with one unparsable file: `--rule MD018 --count` → 0 plus the skip line.

## Part B — the snapshot index tells the truth

### INDEX-1: the stale probe sees in-place edits (BUG-12)

```text
hyalo create-index; sleep 1.1; printf … > n2.md    # overwrite an existing file
hyalo find --index --property status=final          # n2.md missing, no warning, exit 0
```

DEC-280's directory-mtime probe cannot see an in-place overwrite on APFS; a new file does warn.
The index already stores per-file mtimes.

- [x] Fold per-file mtime/size from the snapshot into the probe: warn when any indexed file's
      on-disk stat differs. Measure one `stat` per file on MDN (~14k syscalls); if too slow,
      stat the N most recently modified plus the directory probe and document the residual blind
      spot. Record as a DEC-280 amendment in [[decision-log]].
- [x] e2e: overwrite an indexed file, `find --index` warns; same-second case documented.

### INDEX-2: `summary --index` reports the excluded count (BUG-18)

- [x] `[scan] exclude` filtering at snapshot load keeps the dropped count and `summary` reports
      it; parity test disk vs index on a kepano copy with `Templates/**` excluded
      (`{total:51, skipped:0, excluded:52}` both ways).

### INDEX-3: invalid-UTF-8 files stay out of `--index` reads (previous report BUG-14, PARTIAL)

- [x] `find --file bad.md --index` gives the disk scan's answer (the "excluded from full-text
      search" sentence; `find -e` still matches lossily). Parity test.

## Part C — `mv` uses the path you named

### MV-1: destination is vault-prefix-stripped like the source (BUG-14, [[backlog/done/mv-destination-path-resolved-vault-relative]])

With `dir = "kb"` from the parent: `mv kb/a.md kb/sub/a.md`, `--file kb/a.md --to kb/sub/a.md`
and `--glob a.md --to kb/sub/ --apply` all create `kb/kb/sub/a.md`.

- [x] Route the destination (positional, `--to` file, `--to` directory, batch) through the same
      CWD-relative → vault-relative normalisation the source uses; refuse a destination outside
      the vault with the existing "outside vault boundary" error.
- [x] Fix the `--to kb/sub/` hint (`did you mean kb/sub/.md?`): a trailing slash is a directory
      destination and single mode should say so.
- [x] e2e for all four forms from the parent directory and from inside the vault; close the
      backlog item (`status: done`, moved to `backlog/done/`) in this PR.

### MV-2: batch `mv` gets the widened split-link scan ([[backlog/done/mv-batch-frontmatter-link-scan-gap]])

- [x] Reuse the `LinkGraphBuild` candidate marker once per batch (the graph is built once
      already) so the cost is one pass regardless of batch size; report
      `frontmatter_links_skipped` per move.
- [x] e2e; measure batch `mv --glob` on the Hub before/after; close the backlog item.

### MV-3: `--on-conflict` is validated and honoured (BUG-24, BUG-26)

- [x] `--on-conflict` becomes a clap value enum (bogus values are usage errors); single-file
      `mv` honours `skip` (one branch).
- [x] The batch collision message distinguishes "two sources map to one destination" from "one
      source collides with an existing file".
- [x] e2e for both.

## Shared closing tasks

- [x] Changelog entries via `hyalo changelog add` (one per task group that changes behaviour).
- [x] DEC-280 amendment recorded; both `mv` backlog items closed.
- [x] Docs: `find --help` (named-file error vs `--files-from` counting), `mv --help`
      (destination resolution, `--on-conflict` in single mode), `--index` help (stale probe).
- [x] Gates green: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, `hyalo lint --strict` on the KB, all xtask `check-*` gates
      (invoke as `CARGO_MANIFEST_DIR=<repo>/crates/xtask ./target/debug/xtask <gate>`).

## Acceptance criteria

- [x] `find --file dup.md` (unparsable) exits 1 with the YAML diagnostic; `--files-from` with
      the same file counts it and exits 0.
- [x] `find --index --file brand-new.md` returns the file; a file in neither place is
      `files_missing: 1`.
- [x] `--file`, `--glob`, positional and sweep produce identical link JSON for a broken anchor,
      `suggested_fragment` included.
- [x] `lint --rule MD018 --count` on a vault with one unparsable file → 0 plus the skip line.
- [x] Overwriting an indexed file in place makes the next unnamed `--index` read warn; the
      probe's extra cost on MDN is recorded and stays under 0.1 s.
- [x] `summary --index` and `summary` agree on `excluded`; invalid-UTF-8 parity holds.
- [x] All four `mv` destination forms land in the vault, never nested; both backlog items
      closed; `--on-conflict bogus` is a usage error; single-file `--on-conflict skip` skips.
- [x] Gates green; changelog; DEC-280 amendment.

## Links

- [[dogfood-results/dogfood-v0220-post-batch-261-270]] — BUG-9, 10, 11, 12, 14, 18, 20, 24, 26
- [[iterations/iteration-265-scan-exclude-and-skipped-files]] — DEC-277/278/280
- [[backlog/done/mv-destination-path-resolved-vault-relative]]
- [[backlog/done/mv-batch-frontmatter-link-scan-gap]]

## Outcome (2026-09-05)

Shipped on `iter-273/index-named-file-honesty`. Decisions recorded as
[[decision-log]] DEC-301 (named files), DEC-302 (the DEC-280 stale-probe
amendment), DEC-303 (the DEC-277 excluded-count amendment), DEC-304 (`mv`
destinations), DEC-305 (`--on-conflict`) and DEC-306 (batch split-link sweep).
Both `mv` backlog items are closed and moved to `backlog/done/`.

### Deviations from the plan

- **NAMED-2, "a file in neither place is `files_missing: 1`".** Not
  implemented as written. `find` already refuses such a path with `file not
  found` and exit 1 (iteration 210's L-7 / BUG-13), and that is both stronger
  than a counter at exit 0 and identical to what the non-`--index` path does.
  The plan's own prose ("never `results: []` with exit 0") is satisfied. Only
  the upsert half of NAMED-2 was built.
- **INDEX-3 (invalid UTF-8 parity).** Already held on HEAD — BM25, `find -e`
  and `find --file` all answer identically off disk and from the index. No code
  change; a parity test now pins it so the PARTIAL cannot regress.
- **INDEX-1 cost.** The per-file probe was measured, not degraded to a
  most-recently-modified sample: `find --index --limit 1` over MDN's 14,375
  files went 0.12 s → 0.15 s (six runs each, warm), inside the 0.1 s budget. It
  runs only when the cheap directory probe found nothing, and short-circuits at
  the first drifted file.

### Verification

- 24 new e2e tests in `crates/hyalo-cli/tests/e2e/iteration273_named_file_honesty.rs`,
  one per acceptance criterion.
- `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace -q`, `hyalo lint --strict`, and every xtask `check-*`
  gate green.
