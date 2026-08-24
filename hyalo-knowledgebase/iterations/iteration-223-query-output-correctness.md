---
title: "Iteration 223 — query & output correctness (CJK search, section ambiguity, jq/sort output, schema strictness, error hints)"
type: iteration
date: 2026-08-23
status: planned
branch: iter-223/query-output-correctness
tags: [iteration, search, output, ux]
related:
  - "[[reviews/deep-analysis-2-2026-08-23]]"
  - "[[reviews/deep-analysis-3-2026-08-23]]"
---

# Iteration 223 — query & output correctness

## Goal

Fix the four non-security code flaws from
[[reviews/deep-analysis-2-2026-08-23]] (F-1..F-4): silent multi-section
toggle, CJK-unsearchable BM25, jq errors that dump the whole result set, and
mixed-type sort that looks like nonsense. Plus the three correctness/UX
findings from [[reviews/deep-analysis-3-2026-08-23]] (F3-3, F3-4, F3-5):
schema silently ignoring unknown keys, a misleading boundary error, and
hintless error envelopes.

## Context

- **F-1 (VERIFIED)** — `crates/hyalo-cli/src/commands/tasks.rs:33-55`:
  `task toggle --section "Sec"` toggles tasks under EVERY heading matching
  the name, silently, with no ambiguity signal — while `links` reports
  `ambiguous: N` for the analogous multi-match case. `task toggle` writes
  immediately with no dry-run, so nothing catches over-toggling in a vault
  with repeated headings (`## Tasks` per ADR, `## Notes` per month).
- **F-2 (VERIFIED)** — `crates/hyalo-core/src/bm25.rs:148-149`:
  `text.split(|c| !c.is_alphanumeric())` turns a whitespace-free CJK run
  into one giant token, so `hyalo find 日本語` returns 0 results on a file
  containing the query verbatim. The module claims "Unicode-aware" — it is
  Unicode-*safe*, not CJK-*aware*. Silent correctness hole for any
  JA/ZH/KO vault.
- **F-3 (VERIFIED)** — `crates/hyalo-cli/src/output.rs:710`
  (`apply_jq_filter_result`): a jaq runtime error is stringified with its
  entire input, so one mistyped `--jq` filter dumps megabytes of vault
  content into the error envelope — a content-disclosure vector for
  consumers that log errors.
- **F-4 (VERIFIED)** — `crates/hyalo-core/src/filter/sort.rs:92-95`: the
  mixed-type fallback compares JSON string representations, so
  `priority: "10"` (string) sorts before `priority: 9` (number) because
  `"` < any digit. The total order is deliberate but the result is
  user-visible nonsense with no signal that types were mixed.
- **F3-3 (MEDIUM, VERIFIED)** — `crates/hyalo-core/src/schema.rs:447-459`
  (`RawPropertyConstraint`): serde captures only `type`, `pattern`,
  `item_pattern`, `values`, `min-length`, `max-length`; every other key is
  silently dropped. `type = "number"` with `minimum = 1` / `maximum = 5`
  lints a `priority: 99` file CLEAN. Same for any typo (`patterns =`,
  `value =`). The module is otherwise exemplary at surfacing misconfiguration
  (its `TryFrom` rejects mismatched constraints with specific errors), which
  makes the silent-drop inconsistent with its own philosophy.
- **F3-4 (LOW, VERIFIED)** — `crates/hyalo-core/src/discovery.rs:357-367`:
  `resolve_file` lexically rejects ANY `..` component before resolution, so
  from a vault subdir `hyalo read ../broken.md` — pointing at a file squarely
  inside the vault — returns "file resolves outside vault boundary", which is
  false. The same string is used for genuine escapes, so users learn to
  distrust it.
- **F3-5 (LOW, VERIFIED)** — error envelopes across `commands/*` lack a
  `hint` field for the most common failures: `hyalo set nosuch.md
  --property x=1` → `{"error":"file not found","path":"nosuch.md"}` no hint;
  `hyalo read ''` → identical message for what is almost certainly a shell
  quoting accident; `cd sub && hyalo set a.md ...` fails "file not found"
  when `a.md` exists at the root with no hint that paths are vault-relative.
  For a CLI whose whole UX is drill-down hints (DEC-031/040), the error path
  — where guidance matters most — is hintless, and agents burn retries.

## Tasks

- [ ] F-1: when `--section` matches more than one distinct heading instance,
      refuse with an ambiguity error naming the matched heading line numbers
      and suggesting `--line`, OR require an explicit opt-in flag
      (`--all-sections`/`--nth`). Decide which (prefer refuse-by-default,
      matching the `links` ambiguity precedent) and record it. At minimum
      the output must surface the matched section lines
- [ ] F-2: make BM25 tokenization CJK-aware. Cheapest sufficient approach:
      detect CJK (and other scriptio-continua) runs and additionally index
      them as character bigrams, tokenizing CJK queries the same way so they
      match. Fall back / document per the review's ladder if full bigram
      indexing is too invasive for one iteration — but "documented
      limitation only" is the floor, not the target. Add a decision-log
      entry for the tokenization approach and note the index-format
      implication (snapshot rebuild)
- [ ] F-3: truncate the embedded value in a jq runtime error to ~200 chars
      with an `…` suffix and name the failing filter/position; keep full
      detail behind an explicit `--debug`/verbose path if wanted. Audit for
      the same whole-input-in-error shape elsewhere in output.rs
- [ ] F-4: keep the total order, but emit a one-line warning when a sort key
      has mixed types across the result set ("property:priority has mixed
      types; numbers sort after strings"), and make missing/type-mismatched
      values group consistently (as `Null` already does)
- [ ] F3-3: either implement `minimum`/`maximum` for number constraints
      (two `Option<f64>` fields + two comparisons — the natural fix given
      they're the expected names), OR add `#[serde(deny_unknown_fields)]` to
      `RawPropertyConstraint` so any unsupported key is a hard config error
      consistent with the module's stated philosophy. Prefer implementing the
      two AND denying unknown fields, so typos still surface. Test matrix:
      every plausible key is either honored or rejected
- [ ] F3-4: make the `..` rejection honest — either resolve-then-check
      (join with dir, canonicalize, compare to root; machinery exists in
      `fs_util`) and ACCEPT in-vault `..` paths, or keep the lexical rule but
      reword to "paths must be vault-relative without `..` — use `broken.md`,
      not `../broken.md`". Do not reuse the genuine-escape wording for the
      no-`..` policy
- [ ] F3-5: add `hint` to the three canonical errors — file-not-found
      ("paths are vault-relative; run `hyalo find --file <glob>` to locate
      it"), empty path ("empty path — check shell quoting"), and the F3-4
      message. One helper, three call sites
- [ ] Docs sync: `find --help` (CJK limitation/behavior + `--sort`
      mixed-type note), `task toggle --help` (section ambiguity behavior),
      schema/config docs for `minimum`/`maximum` if implemented, README
      search section if it claims CJK, CHANGELOG, decision-log
- [ ] Tests: F-1 multi-section repro (refused/flagged); F-2 `find 日本語`
      returns the file, plus a mixed CJK+latin doc; F-3 mistyped `--jq`
      error is bounded in length and names the filter; F-4 mixed-type sort
      emits the warning and orders deterministically; F3-3 `minimum`/`maximum`
      honored (or an unknown key rejected); F3-4 in-vault `../x.md` from a
      subdir resolves or gives the reworded message; F3-5 the three errors
      carry hints

## Acceptance criteria

- [ ] `task toggle --section` on a vault with two same-named headings does
      not silently toggle both — it refuses or requires explicit opt-in, and
      names the matched sections
- [ ] `hyalo find 日本語` returns a file containing 日本語 (and CJK queries
      match in general), or the limitation is documented AND a substring
      fallback returns it — no silent empty result
- [ ] A mistyped `--jq` filter produces a bounded error (≤ ~200 chars of
      embedded value) that names the filter, on a large vault
- [ ] Mixed-type `--sort property:x` warns and is deterministic
- [ ] A number constraint with `minimum`/`maximum` either enforces them or
      the config is rejected; an unknown constraint key is never silently
      dropped
- [ ] An in-vault `../file.md` from a subdir no longer reports a false
      "outside vault boundary" (resolves, or the message names the real
      no-`..` policy)
- [ ] file-not-found, empty-path, and boundary errors carry actionable hints

## Non-goals

- Full CJK morphological segmentation / a tokenizer dependency (bigrams are
  sufficient; note the tradeoff)
- BM25 ranking-math correctness beyond tokenization (IDF/length norm — not
  in scope)
