---
title: "Iteration 188 — link semantics completion & review close-out"
type: iteration
date: 2026-07-19
status: planned
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

### 0. Single resolver entry point + perf guard (carried from iter-187, terminus here) [0/6]

Line references below are against main at `045f6cb`; re-derive after
iter-187's PR #221 merges (it touched link_fix.rs/link_rewrite.rs).

- [ ] Extend `link_resolve.rs` (currently only the mv-oriented
  `LinkResolver`) with a public `resolve_link(ctx, link, mode)` entry
  point: `ResolveMode::Exists` (find --broken-links, backlinks,
  summary, orphan/dead-end) and `ResolveMode::Classify` (links fix's
  Broken/CaseMismatch/Ambiguous/ExactHit). A `ResolveCtx` bundles
  `canonical_dir`, `site_prefix`, `Option<&CaseInsensitiveIndex>`, and
  the stem index. Move (or delegate) the policy in
  `link_fix.rs::classify_link` and
  `link_fix.rs::resolve_and_classify_link` so `link_fix.rs` no longer
  owns resolution order
- [ ] Migrate `find/mod.rs`'s inline per-link resolution block
  (kind-dependent source-relative normalization + direct
  `discovery::resolve_target` calls) onto
  `resolve_link(.., ResolveMode::Exists)`; broken-links and
  orphan/dead-end filters keep identical observable behavior (lock
  with e2e before migrating)
- [ ] Merge the near-duplicates `detect_broken_links` (test-only) and
  `detect_broken_links_from_index` into one implementation over
  `resolve_link`; port the ~10 unit tests onto the surviving entry
  point
- [ ] Finish the L-6 tail in `summary.rs`: orphan/dead-end counting
  does manual case-SENSITIVE membership checks against
  `all_targets()`/`all_sources()` — route through `LinkGraph`'s
  `lower_index`-aware lookups so orphan/dead-end counts agree with
  `backlinks_ci` on case-insensitive vaults; e2e proving `[[foo]]` →
  `Foo.md` is not counted as orphan
- [ ] Grep-audit AC (from iter-184): no independent stem-matching or
  direct `discovery::resolve_target` calls in
  `hyalo-cli/src/commands/` outside the shared entry point; document
  the audit command + result in the PR description
- [ ] Perf A/B (carried iter-187 task 5): benchmark main vs branch
  with `bench-e2e.sh` — `links fix` dry-run scan path,
  `find --broken-links`, synthetic 2000-file batch `mv --apply`.
  Record before/after numbers here before ticking; within noise on
  scan paths, no regression >5% on apply paths

### 1. L-19: `.md`-suffix normalization at Link construction [0/4]

- [ ] Today `parse_wikilink` strips `.md` (links.rs:531 via
  `strip_wikilink_md_suffix`, :545) while `parse_markdown_link` keeps
  it (links.rs:607-634), and consumers compensate per-call-site:
  `auto_link.rs:530` re-strips, `link_graph.rs::strip_md_extension`
  (:496) strips for graph keys, `resolve_target` (discovery.rs:811)
  appends `.md` for stems, `LinkResolver` special-cases `.md` twice
  (link_resolve.rs:42-48, :117-124). Decide the canonical
  representation — proposal: `Link.target` is always the extension-less
  canonical form for `.md` targets regardless of kind, plus an
  as-written field (or reuse `WrittenForm`) preserving what the user
  typed — and record a DEC entry in [[decision-log]]
- [ ] Implement at construction (`parse_wikilink` /
  `parse_markdown_link` only); grep-audit and migrate every consumer
  that compares targets across link kinds; delete the
  `auto_link.rs:530` re-strip and the `strip_md_extension`
  duplication in link_graph.rs
- [ ] Rewrite side unchanged for users: `LinkWriter`
  (link_write.rs:34+) still emits `.md` for markdown targets and
  preserves the written form for wikilinks; mv/fix e2e over
  `[x](note.md)`, `[[note.md]]`, `[[note]]` fixtures byte-identical to
  pre-change output
- [ ] Snapshot-compat check: old `.hyalo-index` deserialization falls
  back to disk scan with the existing warning (no panic, no silent
  misread); new snapshot round-trips; CHANGELOG notes the rebuild

### 2. L-23: percent-decode markdown link targets [0/3]

- [ ] `resolve_target` (discovery.rs:811-900) never percent-decodes:
  `[x](my%20dest.md)` is unresolvable. Percent-decode the path portion
  after the existing fragment/query strip (:826-831); invalid or
  non-UTF-8 escape sequences keep the literal text; decide whether
  decoding applies uniformly or only to markdown-kind targets
  (resolve_target is kind-agnostic today) and record in the DEC entry
- [ ] Graph consistency: `insert_file_links` (link_graph.rs:505) /
  `normalize_target` (:703) store the decoded form so
  `backlinks "my dest.md"` finds linkers that wrote `my%20dest.md`,
  and `find --broken-links` stops false-positiving on them; encoding
  is kept as-written on rewrite (mv of `my dest.md` preserves the
  `%20` form — parity with PR #220's angle-bracket handling, where
  `[x](<my dest.md>)` already resolves)
- [ ] e2e: `%20` fixture across `find --broken-links`, `backlinks`,
  `mv --apply` (link preserved re-encoded), `links fix`; plus a
  mixed fixture proving `[a](<my dest.md>)` and `[b](my%20dest.md)`
  resolve to the same file

### 3. L-21: anchors carried through resolution [0/5]

- [ ] Stop discarding anchors at parse: fragments are stripped at
  links.rs:519-520 (wikilink) and :616-617 (markdown) via
  `strip_fragment` (:652). Add `fragment: Option<String>` to `Link`
  (same shape-bump as task 1). `LinkSpan.target_end` (links.rs:67-87)
  must keep stopping before `#` so rewrite spans are unaffected;
  fragment-only links (`[[#h]]`, `[t](#h)`) stay non-links for file
  resolution (pinned behavior, links.rs:522-525, :620-623)
- [ ] Heading→anchor matching: decide the wiki/Obsidian convention
  (case/whitespace normalization; `#^block-id` refs are skipped —
  not validatable against headings) consistent with what
  `read --section` users expect (note: `SectionSelector::matches`,
  heading.rs:147-162, is ASCII-case-insensitive substring — anchors
  need exact-heading semantics, so a NEW shared helper, not a reuse);
  record a DEC entry
- [ ] Surface in `find --broken-links`: broken-anchor is a DISTINCT
  category from broken-target (`LinkInfo` in find/mod.rs gains anchor
  fields; broken filter :871-879 extended to include broken-anchor
  hits); JSON and text output document the difference
- [ ] Perf guard: with `--index`, validate anchors against
  `IndexEntry.sections` (`OutlineSection.heading`, types.rs:102-110) —
  zero file reads; without an index, collect the set of anchored
  targets first and read ONLY those files; A/B timing on the bench
  vault recorded before ticking
- [ ] e2e: `[[Foo#real-heading]]` ok, `[[Foo#nope]]` broken-anchor,
  `[[Nope#x]]` broken-target (not double-reported), `[[Foo#^block]]`
  skipped; `links fix` headline counts unchanged by broken anchors

### 4. L-22: HYALO006 broken-link lint rule [0/6]

- [ ] Register HYALO006 ("broken-link") in the hyalo-mdlint catalog:
  `SEVERITY_TABLE` (engine.rs:45-50), `DEFAULT_ON` (engine.rs:55-58),
  and `hyalo_entries` (engine.rs:119-166); decide default severity —
  proposal: enabled, `warn` by default, promoted to error under
  `--strict` (the existing strict-promotion pattern in
  `lint_one_file_extended`) — and whether broken anchors are included
  (severity-configurable) or deferred; record a DEC entry
- [ ] Implement CLI-side following the HYALO005 pattern (rule lives in
  the catalog, logic in hyalo-cli where vault context exists):
  `lint_one_file_extended` (lint.rs:2188) takes an
  `Option<&LinkLintContext>`; per-file findings resolve each of the
  file's links through iter-187's `resolve_link`
- [ ] Vault-level link-graph cache: build the context ONCE per
  invocation in the lint dispatch arm (dispatch.rs:1914-1946, where
  `ExtLintOptions` at lint.rs:1550-1598 already carries
  `snapshot_index`/`index_path`/`vault_dir`/`case_insensitive`) — from
  `snapshot_index.link_graph()` + entries when `--index` is active,
  else one `LinkGraph::build` + `CaseInsensitiveIndex`; shared by
  reference across the rayon workers in `lint_files_extended`
  (lint.rs:1601) — lint must NOT rebuild the graph per file
- [ ] `--files-from` correctness: the graph/resolution context is
  vault-wide even when the linted file set is scoped — a scoped file
  linking to an unscoped-but-existing file must not fire; e2e with
  `--files-from -` piped list
- [ ] Config integration: `[lint.rules.HYALO006]` severity/enabled
  overrides round-trip via `lint-rules set/show`; `[lint] ignore`d
  files are excluded (already handled by the dispatch-level file
  filter); `--rule HYALO006` / `--rule-prefix HYALO` select it
- [ ] Docs: `lint-rules list`/`show` entries (from the catalog),
  README lint section, `templates/rule-knowledgebase.md` lint bullet
  mentions link gating, CHANGELOG; e2e: broken wikilink AND broken
  markdown link each produce a finding; `--strict` exits 1; clean
  vault exits 0

### 5. Review close-out & plan hygiene [0/4]

- [ ] Annotate every finding in
  [[reviews/link-handling-review-2026-07-18]] — L-1 through L-26 plus
  L-A1/L-A2 — with its outcome: resolved-in (iteration/PR pointer) or
  deferred-with-reason; flip the review's `status` from `active` when
  all findings are dispositioned
- [ ] Resolve stale unchecked tasks in
  [[iterations/iteration-183-link-scanner-unification]] (4 boxes:
  FileVisitor `on_frontmatter_line`, behavior-capture baselines, L-18
  sentinel, MDN perf check),
  [[iterations/iteration-184-link-resolver-writer-unification]]
  (resolver/write-path boxes + both deferred ACs), and
  [[iterations/iteration-185-link-semantics]] (tasks 1-3, 5 + both
  deferred ACs): tick a box ONLY if the work verifiably landed (in
  183-186, PR #220, iter-187, or this iteration), otherwise rewrite it
  as an annotated non-checkbox entry "superseded-by
  [[iterations/iteration-187-link-writer-unification]] /
  [[iterations/iteration-188-link-semantics-completion]]" (or
  deferred-with-reason, e.g. L-18 stays deliberately scoped out) —
  never a dishonest tick
- [ ] Verify: `hyalo lint --rule HYALO002` no longer flags
  iteration-183/184/185 (pre-existing findings on iterations
  152/159/173/181 are OUT of scope — do not touch)
- [ ] Use `hyalo` itself for all KB edits (task toggle / set / append)
  per CLAUDE.md; fall back to Edit only for body prose rewrites

### 6. Retrospective [0/3]

- [ ] Re-run the link-review fixture corpus (multi-line spans, BOM,
  CRLF, anchors, aliases, escapes, angle-bracket + percent-encoded
  destinations) across find/mv/fix/auto/lint — all consistent
  (carried from iter-185 task 5)
- [ ] Record all DEC entries made this iteration in [[decision-log]]
  (anchor convention, HYALO006 severity, percent-decode scope, L-19
  representation)
- [ ] README/help/CHANGELOG in sync; no release — release is a
  separate user-gated step

## Acceptance Criteria

- [ ] One resolver entry point: grep-audit shows no independent
  stem-matching or direct `resolve_target` resolution loops in command
  code outside `resolve_link`/`LinkGraph` (closes iter-184 AC 1,
  carried via iter-187); perf A/B recorded
- [ ] `hyalo lint --strict` can gate broken links in CI via HYALO006
  (closes iter-185 AC 1), with the graph built once per invocation
- [ ] Anchored-link health visible in `find --broken-links` output as a
  distinct broken-anchor category (closes iter-185 AC 2)
- [ ] `[x](my%20dest.md)` resolves to `my dest.md` in find/backlinks/
  fix; encoding preserved on rewrite
- [ ] `.md` normalization lives in exactly one place; grep-audit shows
  no per-consumer re-stripping
- [ ] Every L-1..L-26 + L-A1/L-A2 finding dispositioned in the review
  doc; `hyalo lint --rule HYALO002` clean for iterations 183/184/185
- [ ] e2e coverage for tasks 1-4; anchor validation perf measured and
  recorded (index path: no extra file reads)
- [ ] `cargo fmt` / `clippy --workspace --all-targets -- -D warnings` /
  `cargo test --workspace -q` clean
