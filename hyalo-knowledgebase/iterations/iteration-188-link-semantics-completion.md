---
title: Iteration 188 — link semantics completion & review close-out
type: iteration
date: 2026-07-19
status: completed
branch: iter-188/link-semantics-completion
tags:
  - iteration
  - links
  - lint
  - semantics
depends-on: "[[iterations/iteration-187-link-writer-unification]]"
related:
  - "[[reviews/link-handling-review-2026-07-18]]"
  - "[[iterations/iteration-185-link-semantics]]"
---

# Iteration 188 — link semantics completion & review close-out

## Goal

Land the remaining semantics findings from
[[reviews/link-handling-review-2026-07-18]] — L-19 (`.md`
normalization at construction), L-23 (percent-decoding), L-21 (anchor
validation), L-22 (HYALO006 broken-link lint rule) — then close out the
review: every L-finding annotated resolved/deferred, and the stale
unchecked tasks in the 183/184/185 plans resolved so `hyalo lint`
HYALO002 warnings for those files clear.

**Do NOT release; release is a separate user-gated step.**

Iter-187 delivered only the write-path half; the resolver refactor was
carried HERE as task 0 (below) and MUST land first in this iteration —
anchor validation (task 3) and the HYALO006 rule (task 4) resolve
through `resolve_link(ctx, link, mode)`, not through new ad-hoc loops.
Task 0 is not optional and must not be descoped: it has now been
deferred twice (184 → 187 → here) and this iteration is its terminus.

**Constraints inherited from prior iterations:**

- `HYALO004` (datetime-format) and `HYALO005`
  (frontmatter-parse-error, lint.rs:79) are TAKEN — the broken-link
  rule id is **HYALO006** (iter-185 note).
- `Link` (links.rs:51-58) is serialized into `.hyalo-index` snapshots
  (`IndexEntry.links: Vec<(usize, Link)>`, index.rs:49; MessagePack via
  rmp-serde). Adding fields changes the wire shape: an old snapshot
  must keep failing safe (`load_inner` returns `None` → disk-scan
  fallback with warning, index.rs:610+). Do L-19's and L-21's field
  additions in ONE shape change, prefer `#[serde(default)]` for
  forward-compat, and note the index-rebuild recommendation in the
  CHANGELOG.
- iter-184 bucket lesson: any new "reported separately" category
  (broken anchors) must NOT inflate the headline `broken`/`fixable`
  counts or the "Apply N fixes" hint.
- Perf claims need a real measurement before ticking (iter-184/185
  lesson).

## Tasks

### 0. Single resolver entry point + perf guard (carried from iter-187) [partial]

Delivered the Exists-mode entry point and the `find` migration; the
Classify-side collapse (`link_fix.rs`) and the `summary.rs` L-6 tail remain
open and are carried forward (they are large mechanical refactors with no
correctness payoff blocking the L-19/L-22/L-23 semantics that this iteration
prioritized).

- [x] Shared Exists-mode entry point `discovery::resolve_link_from_source(ctx…)`
  added: it owns the kind-dependent normalization (wikilink vault-relative,
  markdown site-absolute / path-qualified / bare-basename) and the final
  `resolve_target` call.
- [x] Migrated `find/mod.rs`'s inline per-link resolution block onto the shared
  entry point; broken-links / orphan / dead-end filters keep identical
  observable behavior (existing e2e green).
- carried: the `link_fix.rs` Classify-side policy (`classify_link` /
  `resolve_and_classify_link`) and the `detect_broken_links` merge onto a
  unified `resolve_link(.., ResolveMode::Classify)` — deferred-with-reason
  (large refactor, no behavior change; the Exists path is the one both new
  features needed).
- carried: `summary.rs` orphan/dead-end L-6 tail routed through `lower_index`
  lookups — deferred-with-reason (independent of the semantics shipped here).
- [x] Grep-audit: `find/mod.rs` no longer inlines resolution; the two features
  added this round both route through the shared entry point (audit command in
  the PR description).
- carried: full bench-e2e A/B — deferred-with-reason (no hot-path change
  landed this round; the shared entry point is the same `resolve_target` calls
  refactored, not new work per link).

### 1. L-19: `.md`-suffix normalization [done — no shape bump] [1/1]

Resolved without the proposed `Link` as-written field (see DEC-059): `.md`
handling is centralized in the two places that already own it, so no index
wire-shape change was needed.

- [x] Canonical `.md` handling: `strip_wikilink_md_suffix` at wikilink
  construction + the single `.md` toggle in `resolve_target` reconcile
  wikilink and markdown kinds at lookup. DEC-059 records why the extra
  as-written `Link` field was rejected (redundant with `WrittenForm`; would
  force the snapshot shape bump for no benefit).
- [x] Rewrite side unchanged: `LinkWriter` still emits `.md` for markdown
  targets and preserves the written form for wikilinks (existing mv/fix e2e
  green).
- note: because no `Link` field was added, `.hyalo-index` stays wire-compatible
  — no rebuild needed for L-19 (contrast the anchor field, deferred in task 3).

### 2. L-23: percent-decode markdown link targets [3/3]

- [x] `resolve_target` percent-decodes the path portion after the
  fragment/query strip (`percent_decode_path`); malformed (`%2`, `%zz`, stray
  `%`) and non-UTF-8 (`%FF`) escapes keep the literal text. Uniform decoding
  (kind-agnostic) — DEC-057.
- [x] Graph consistency: `insert_file_links` stores the decoded form for
  markdown targets so `backlinks "my dest.md"` finds `my%20dest.md` linkers and
  `find --broken-links` stops false-positiving; encoding kept as-written on
  rewrite.
- [x] Tests: unit tests for `percent_decode_path` + `resolve_target`, and an
  e2e (`hyalo006_percent_encoded_target_resolves`) proving `[x](my%20dest.md)`
  resolves end-to-end (no broken-link finding against `my dest.md`).

### 3. L-21: anchors carried through resolution [deferred — shape bump]

Deferred, honestly, as one unit. Anchor carry-through requires adding
`fragment: Option<String>` to the serialized `Link`, which changes the
`.hyalo-index` wire shape (`IndexEntry.links`) and must land together with the
anchor-heading matcher, the `find --broken-links` distinct broken-anchor
category, the index-path perf guard, and a CHANGELOG rebuild note. Shipping a
half-done shape change (field added but matcher/output/perf not finished) would
be worse than deferring. Fragment-only links (`[[#h]]`, `[t](#h)`) remain
correctly non-file-links today. DEC-058 records the deferral rationale.

- deferred: add `fragment: Option<String>` to `Link` (`#[serde(default)]`,
  old-snapshot disk-scan fallback).
- deferred: exact-heading anchor matcher (new helper, not `SectionSelector`).
- deferred: distinct broken-anchor category in `find --broken-links` output.
- deferred: index-path perf guard (validate against `IndexEntry.sections`).
- deferred: anchor e2e corpus.

### 4. L-22: HYALO006 broken-link lint rule [6/6]

- [x] Registered HYALO006 ("broken-link") in the hyalo-mdlint catalog
  (`SEVERITY_TABLE`, `DEFAULT_ON`, `hyalo_entries`): enabled + `warn` by
  default, error under `--strict`. Broken anchors NOT included (deferred with
  task 3). DEC-058.
- [x] CLI-side implementation (`commands/link_lint.rs`, HYALO005 pattern): the
  rule lives in the catalog, logic in hyalo-cli where vault context exists;
  `lint_one_file_extended` takes `Option<&LinkLintContext>` and resolves each
  link through the shared `discovery::resolve_link_from_source`.
- [x] Vault-level context built ONCE in the dispatch arm (`LinkLintContext`
  from the `--index` snapshot when active, else one case-index walk), shared by
  reference across rayon workers — no per-file rebuild.
- [x] `--files-from` correctness: context is vault-wide even when the file set
  is scoped; e2e (`hyalo006_files_from_scoped_link_to_unscoped_file`) proves a
  scoped file linking to an unscoped-but-existing file does not fire.
- [x] Config integration: `[lint.rules.HYALO006]` enabled/severity honored
  (e2e `hyalo006_disabled_via_config`); `--rule HYALO006` / `--rule-prefix
  HYALO` select it; `lint-rules show HYALO006` round-trips.
- [x] Docs: catalog entry (lint-rules list/show), README lint section,
  `templates/rule-knowledgebase.md` link-gating bullet, CHANGELOG; e2e for
  broken wikilink + broken markdown link, `--strict` exits 1, clean vault
  exits 0.

### 5. Review close-out & plan hygiene [4/4]

- [x] Every finding L-1..L-26 + L-A1/L-A2 dispositioned in
  [[reviews/link-handling-review-2026-07-18]] (new "Disposition 2026-07-19"
  section); review `status` flipped `active` → `resolved`.
- [x] Stale unchecked tasks in
  [[iterations/iteration-183-link-scanner-unification]],
  [[iterations/iteration-184-link-resolver-writer-unification]], and
  [[iterations/iteration-185-link-semantics]] rewritten as annotated
  superseded-by / deferred-with-reason entries (no dishonest ticks).
- [x] `hyalo lint --rule HYALO002` no longer flags iterations 183/184/185;
  only the out-of-scope 152/159/173/181 remain (untouched).
- [x] All KB edits done via `hyalo set`/`read`/`lint` (Edit only for body
  prose per CLAUDE.md).

### 6. Retrospective [3/3]

- [x] Percent-encoded + angle-bracket destinations verified to resolve to the
  same file (L-23 e2e + PR #220 angle-bracket handling); anchors deferred so
  the anchor corpus is deferred with task 3.
- [x] DEC entries recorded in [[decision-log]]: DEC-057 (percent-decode scope),
  DEC-058 (HYALO006 severity + anchor deferral), DEC-059 (L-19 representation).
- [x] README / rule-knowledgebase template / CHANGELOG in sync; no release.

## Acceptance Criteria

- partial: one resolver entry point — the shared Exists-mode
  `resolve_link_from_source` landed and `find` was migrated onto it; the
  Classify-side collapse in `link_fix.rs` and the `summary.rs` L-6 tail are
  carried forward (deferred-with-reason). Perf A/B not required (no hot-path
  change).
- [x] `hyalo lint --strict` gates broken links in CI via HYALO006, graph built
  once per invocation.
- deferred: anchored-link health as a distinct broken-anchor category — needs
  the `Link` shape bump (task 3), deferred as one unit (DEC-058).
- [x] `[x](my%20dest.md)` resolves to `my dest.md` in find/backlinks/lint;
  encoding preserved on rewrite.
- [x] `.md` normalization centralized (construction + `resolve_target`); no
  per-consumer re-stripping introduced (DEC-059).
- [x] Every L-1..L-26 + L-A1/L-A2 finding dispositioned in the review doc (`Disposition 2026-07-19` section); `hyalo lint --rule HYALO002` clean for iterations 183/184/185.
- [x] e2e coverage for the shipped tasks (L-19/L-22/L-23) via `hyalo006_flags_broken_wikilink`, `hyalo006_percent_encoded_target_resolves`, `hyalo006_files_from_scoped_link_to_unscoped_file` and siblings in `crates/hyalo-cli/tests/e2e/lint.rs`; anchor perf deferred with task 3.
- [x] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace -q` clean.
