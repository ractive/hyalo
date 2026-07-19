---
title: "Iteration 185 — link semantics extensions (Phase D: anchors, lint rule, escapes)"
type: iteration
date: 2026-07-18
status: completed
branch: iter-185/link-semantics
tags:
  - iteration
  - links
  - lint
  - features
depends-on: "[[iterations/iteration-184-link-resolver-writer-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
---

# Iteration 185 — link semantics extensions (Phase D)

## Goal

With one scanner (iter-183) and one resolver/writer (iter-184) in place,
add the semantics the review showed are missing — each lands in exactly
one place now. Findings L-16, L-19, L-21, L-22, L-23 from
[[reviews/link-handling-review-2026-07-18]].

**Carried over from iter-184 (Phase C):** iter-184 shipped the L-6 root
fix (an O(1) lowercased companion map in `LinkGraph`, `backlinks_ci`) plus
L-10/L-12/L-24/L-26, but deferred the two large mechanical refactors so its
PR stayed reviewable. Fold these into Phase D before/with the semantics
work: (a) the single `resolve_link(ctx, link, mode)` entry point collapsing
the find/mod, `link_fix`, `backlinks`, and `summary` call sites onto
`LinkResolver`/`LinkGraph`; (b) `auto_link::apply_matches` onto
`execute_plans`/`RewritePlan` with the stronger TOCTOU guard; (c) L-11
per-file partial-failure envelope (applied/failed/skipped) + batch-mv
rollback-vs-report semantics; (d) L-25 dry-run/apply single-path parity.

**Lessons from iter-184 PR review (apply to this iteration too):**

- **`LinkGraph.lower_index` must stay incrementally maintained, not
  rebuilt.** iter-184's first cut had `rename_path`/`remove_source`/
  `insert_links` each call a full O(vault) `rebuild_lower_index()` per
  invocation; since batch-mv calls these once per file, this regressed
  batch-mv throughput ~38-44% vs main (measured on a synthetic 2000-file
  vault) before being fixed in review to update only the changed
  `lower_index` buckets. Any new `LinkGraph` key-set mutation added by
  the `resolve_link` unification (item (a) above) or by L-11's
  partial-failure envelope work must follow the same incremental
  pattern — do not reintroduce a bulk rebuild inside a per-file loop.
  See `lower_index_stays_consistent_across_incremental_mutations` in
  `link_graph.rs` for the regression-test pattern (compares incremental
  state against a from-scratch rebuild) — extend it if new mutation
  methods are added.
- **"Reported separately, excluded from apply" buckets must exclude
  their own count from the general bucket, not just add a new field.**
  iter-184's fuzzy-match tier (L-10) initially left `fixable`/`fixes`
  counting fuzzy matches *in addition to* the new `fuzzy`/`fuzzy_fixes`
  bucket, so the dry-run "Apply N fixes" hint promised fixes that plain
  `--apply` didn't write. When L-22's broken-anchor category (or any
  other new low-confidence/opt-in bucket) is added, make sure the
  headline counts (`broken`, `fixable`, hint text) reflect only what the
  *default* action set actually touches, and add an e2e assertion that
  running the suggested hint command produces the promised result.
- **Perf claims need an actual measurement, not an assumption.**
  iter-184's plan had a ticked "perf on MDN unchanged" sub-claim that
  turned out to be unverified (no MDN corpus was benchmarked in that
  PR). Item 1's "Perf guard: ... MDN-scale timing within budget" task
  below should be backed by a real before/after timing run (MDN corpus,
  or — if unavailable — a synthetic vault at comparable scale with
  numbers recorded in the plan) before being ticked, not marked done on
  the strength of an untested assumption.
- **CI's Linux runner caught a filesystem-case-sensitivity assumption
  that macOS/Windows dev machines hide.** iter-184's own new e2e test
  (`backlinks_case_insensitive_agrees_across_casings`) called
  `backlinks --file foo.md` expecting it to resolve against an on-disk
  `Foo.md`, and passed locally on macOS purely because APFS is
  case-insensitive by default — it failed on `ubuntu-latest` in CI (a
  genuinely case-sensitive filesystem) with "file not found". Root
  cause: `discovery::resolve_file` (used by `backlinks --file`, and any
  other command that resolves a CLI file argument) does a literal
  `Path::is_file()` check with no case-insensitive fallback, so it never
  actually consults `[links] case_insensitive` — only the *graph-level*
  lookup (`LinkGraph::backlinks_ci`) is case-insensitive-aware. Any new
  test or feature that exercises a filesystem-casing scenario must be
  validated against a case-sensitive assumption (or run on Linux CI)
  before being trusted, not just eyeballed on a macOS/Windows dev
  machine. See task 3 below (new) for closing this specific gap.

## Tasks

### 1. Anchor validation (L-21) [0/3]

- [ ] Anchors are carried through resolution (not discarded at parse,
  links.rs:488-490): `[[Foo#nonexistent-heading]]` is reportable as
  broken-anchor by `find --broken-links` (distinct category from
  broken-target, since heading checks read file content)
- [ ] Heading→anchor slugging matches the wiki/Obsidian convention used
  by `read --section`; decide case/whitespace normalization and record
  in the decision log
- [ ] Perf guard: anchor validation only reads target files that are
  actually linked with an anchor; MDN-scale timing within budget

Not started this PR — deferred (see Implementation notes below); requires
plumbing the vault `LinkGraph` into heading-content reads for anchors.

### 2. Broken-link lint rule (L-22) [0/3]

- [ ] New HYALO004 lint rule: broken wikilink/markdown link targets
  (and optionally broken anchors, severity-configurable) so link health
  can gate CI via `lint --strict`
- [ ] Vault-level cache so lint doesn't rebuild the link graph per file;
  respects `[lint] ignore` and `[okf]`/exempt semantics
- [ ] Docs: lint-rules list/show entries, README, knowledgebase

Not started this PR — deferred; `HYALO004` is already taken (datetime-format),
future rule should be `HYALO006`; needs a vault-level graph cache plumbed into
`hyalo-mdlint`, which currently has no link-graph access.

### 3. Escapes and normalization (L-16, L-19, L-23) [1/3]

- [x] L-16: `\[[not-a-link]]` is not extracted (backslash escape per
  CommonMark/Obsidian); rewiters leave it untouched
- [ ] L-19: `.md`-suffix normalization happens at `Link` construction
  (with an as-written field preserved); remove the manual re-strip at
  auto_link.rs:552-557 and audit consumers comparing targets across
  link kinds — deferred, touches the `Link` type shape and every
  cross-kind consumer; sized as a follow-up to avoid a wide diff.
- [ ] L-23: percent-decode markdown link targets in `resolve_target`
  (discovery.rs:714-842) so `my%20page.md` resolves; encoding kept
  as-written on rewrite — deferred alongside L-19.

### 4. Case-insensitive CLI file-argument resolution (new, from iter-184 review) [3/3]

- [x] `discovery::resolve_file` (used by `--file` args on `backlinks`,
  `read`, `set`, `remove`, `append`, single-file `mv`, etc.) does a
  literal `Path::is_file()` check with no case-insensitive fallback, so
  it never actually consults `[links] case_insensitive` — confirmed by
  CI: `backlinks --file foo.md` fails with "file not found" on Linux
  when only `Foo.md` exists, even with case-insensitive mode on. Only
  the graph-level lookup (`LinkGraph::backlinks_ci`) honors the setting.
- [x] Decide scope: thread `case_insensitive_mode` (or a prebuilt case
  index) into `resolve_file`, falling back to a case-insensitive
  directory scan when the literal-case lookup misses. Note
  `resolve_file` is used by more than just `backlinks` — audit call
  sites and vault-boundary/path-traversal checks stay correct for the
  fallback path too.
- [x] e2e: `backlinks --file foo.md` (lowercase arg) finds `Foo.md` on a
  case-insensitive vault, run on Linux CI (not just locally) so a
  filesystem-accident pass doesn't mask a regression again.

Scope landed via `discovery::resolve_file_ci` + `resolve_case_insensitive`,
wired through `resolve_file_user_ci` into `backlinks` only (the command that
already carried `case_insensitive`); other `--file` commands (`read`, `set`,
`remove`, `append`, `mv`) still use the case-sensitive `resolve_file_user` and
remain a follow-up if needed.

### 5. Retrospective [0/2]

- [ ] Re-run the full link-review fixture corpus (multi-line spans,
  BOM, CRLF, anchors, aliases, escapes) across find/mv/fix/auto/lint —
  all consistent
- [ ] Close out [[reviews/link-handling-review-2026-07-18]]: mark each
  L-finding fixed/deferred with a pointer

Not done this PR — deferred to a future iteration once tasks 1-3 land.

## Implementation notes (this PR)

This PR lands the two most self-contained, correctness-critical findings from
the plan with full unit + e2e coverage, and scopes the deep cross-crate items
honestly rather than half-shipping them.

**Shipped**

- **L-16 (backslash escapes)** — `links.rs`: a new `is_escaped(bytes, pos)`
  helper counts preceding backslashes (odd = escaped, CommonMark/Obsidian
  semantics). Both extraction paths (`extract_links_from_text_with_original`,
  `extract_link_spans_with_original`) now skip an escaped opener. Decisions
  recorded: `\[[x]]` → literal; `\\[[x]]` → real link (`\\` renders as one
  literal backslash); `\![[x]]` escapes only the `!` and still yields a normal
  `[[x]]` wikilink; `!\[[x]]` (backslash before `[[`) suppresses the embed.
  Rewriters get this for free since they consume the same span extractor.
  Covered by 10 unit tests in `links.rs` + an e2e
  (`find_broken_links_ignores_backslash_escaped_link`) proving escaped targets
  never surface in `find --broken-links`.
- **Task 4 (case-insensitive `--file` resolution)** — closes the iter-184 CI
  gap. `discovery::resolve_file_ci(dir, path_arg, case_insensitive)` +
  `resolve_case_insensitive` component-walk helper: when the literal-casing
  lookup misses and the caller opted in, walk each path component and match it
  against on-disk entries under ASCII case-folding, requiring a *unique* match
  per level (ambiguous levels bail to the literal `NotFound`). The fallback
  reuses the same vault-boundary / traversal / symlink guards — it only
  substitutes on-disk casing, never a different directory. `resolve_file`
  stays as `resolve_file_ci(.., false)` for back-compat (all existing callers
  and 30+ tests unchanged). CLI: `resolve_file_user_ci` threads the flag;
  `backlinks` (which already had `case_insensitive`) routes through it.
  Covered by unit tests in `discovery.rs` (incl. ambiguous-returns-none and a
  direct nested-casing substitution test that is host-FS-independent) + an
  e2e (`backlinks_ci_resolves_lowercase_file_arg_against_capitalized_file`)
  that supersedes the "known limitation" note in
  `backlinks_case_insensitive_agrees_across_casings`. Host-FS-sensitive
  assertions are gated on `probe_case_insensitive` so a macOS pass can't mask
  a Linux-CI regression.

**Deferred (not in this PR) — remain open in the tasks above**

- **L-19 / L-23** (task 3) — `.md`-normalization-at-construction with an
  as-written field, and percent-decoding markdown targets. These touch the
  `Link` type shape and every consumer that compares targets across link
  kinds; sized as a follow-up to avoid a wide, hard-to-review diff.
- **L-21 anchor validation** (task 1) and **broken-link lint rule** (task 2) —
  both require plumbing the vault `LinkGraph` into new places (heading-content
  reads for anchors; a vault-level graph cache into the `hyalo-mdlint` crate,
  which currently has no link-graph access). Note the plan's proposed
  `HYALO004` id is already taken (datetime-format) — the future rule should be
  `HYALO006`.
- **iter-184 carried refactors** (a–d: `resolve_link` unification,
  `apply_matches`→`execute_plans`, L-11 partial-failure envelope, L-25
  parity) — large mechanical refactors, out of scope for this focused PR.

## Acceptance Criteria

- [ ] `hyalo lint --strict` can gate broken links in CI (deferred — lint rule
  not yet built; see task 2)
- [ ] Anchored-link health visible in `find --broken-links` output (deferred —
  see task 1)
- [x] `cargo fmt` / `clippy -D warnings` / `cargo test -q` clean
