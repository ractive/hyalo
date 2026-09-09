---
type: iteration
title: Iteration 292 — Query, resolution and index coherence
date: 2026-09-10
status: planned
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-292/query-resolution-and-index-coherence
priority: 2
depends-on: "[[iterations/iteration-291-document-parsing-and-lint-consistency]]"
---

# Iteration 292 — Query, resolution and index coherence

## Outcome and dependency

Unify link identity and owned index refresh, bound snapshot expansion, and fix search
eligibility and disk/index parity in the same branch.

[[iterations/iteration-291-document-parsing-and-lint-consistency]]

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: `R02`, `R03`, `C13`, `R04`, `R06`, `R07`, `R08`, `R15`, `S10`.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-292/query-resolution-and-index-coherence. The
stages below are implementation order within that branch, not separate Ralph iterations,
checkpoints or CI cycles. Use one active implementation writer; divide independent read-only
analysis/review work when useful.

Run focused tests while implementing each stage. Do not run the full workspace/asset/CI suite or
create a commit/PR after every stage. Once all stages are integrated, obtain independent review,
repair findings, and run the complete required gate sequence on the final candidate before its
commit/PR. Reuse results for an unchanged tree. New repairs require affected checks and the
repository-mandated final sequence before a new commit; do not promise exactly one run despite
failures.

Keep each original regression and acceptance condition below. If a foundation already fixes an
assigned defect, verify that implementation and extend its missing cases rather than
reimplementing it or opening a second fix PR. All work is planned and unchecked; the
consolidation changes execution granularity, not scope.

## Implementation stages

### Stage 1: One catalog and target-resolution policy

- [ ] Introduce an immutable per-generation VaultCatalog containing canonical paths,
  directory/stem lookup and optional alias declarations. Keep catalog keys independent from
  filesystem entry identity used for writes.

- [ ] Expose Resolution::Resolved/External/Attachment/Missing/Ambiguous (or equivalent existing
  types) from a shared source-aware resolver. Preserve authored target text, target kind,
  fragment and source location separately. Pass alias/case/site-prefix options explicitly;
  remove process-global alias mode reads from migrated resolution paths. Represent catalog
  completeness separately from selected-file coverage.

- [ ] Centralize relative/root/site-prefix resolution, case policy, padded wikilinks, alias
  opt-in and ambiguity precedence. Migrate graph key construction and rewrite planning to the
  same resolver; semantic link case-folding must not determine filesystem rename collisions.

- [ ] Keep raw outbound occurrences available so a catalog change can re-resolve links from
  unchanged sources, including previously missing/ambiguous references.

- [ ] Unify frontmatter/body link occurrence handoff without making a second YAML parser in
  graph code. YAML lexical source-span repair belongs to iteration 291 after framing migration.

- [ ] Keep old public helper wrappers where needed for compatibility; remove private duplicate
  normalization and paths-only alias caches after callers migrate.

Acceptance for this stage (focused checks):

- [ ] With root b.md and sub/b.md, sub/a.md `[b](b.md)` resolves identically in find, backlinks,
  broken-link checks and move planning.

- [ ] Alias enable/disable, alias collision, new destination, renamed destination and previously
  unresolved references use one precedence policy.

- [ ] Read-side ambiguity is retained rather than arbitrarily selecting a candidate; writers
  preserve documented refusal/skip behavior.

- [ ] Resolution uses retained catalog data without a filesystem stat per link on the normal
  indexed path.

Scope: No global mutable resolver cache, new graph database or change to the default alias
setting. Core public facade remains compatible through wrappers/deprecation unless a separately
documented version boundary is chosen.

### Stage 2: Bound snapshot expansion before reconstruction

- [ ] S10: validate checked aggregate expanded-token bytes before accepting/reconstructing BM25
  data, in addition to file/posting/doc counts. Prefer compact term IDs or borrowed postings
  while preserving phrase positions; no attacker-controlled product of term length and
  occurrence count may escape the bound.

- [ ] Construct small crafted snapshots with oversized term/position expansion and a low test
  budget. Reject before allocation and before any indexed mutation writes. Test initial load and
  effect-triggered refresh; do not attempt a 100-GiB allocation.

Acceptance for this stage (focused checks):

- [ ] Crafted snapshot expansion is refused within a small measured allocation budget before
  content changes; existing valid snapshots remain usable or clearly rebuildable.

Scope: S10 is fully closed in this block before introducing broader index reconstruction. Use
compact adversarial fixtures and low test budgets, never deliberate host exhaustion.

### Stage 3: One owner for complete snapshot updates

- [ ] Before any new reconstruction path is used, enforce checked aggregate expanded-token
  budgets on loaded BM25 data. This prerequisite guard prevents S10 amplification during
  migration; the preceding snapshot-budget stage in this block provides adversarial coverage.

- [ ] Create a complete ScannedDocument replacement value and private owned live index state
  with explicit generation/freshness for catalog, graph and search data. Keep serialized
  snapshot DTOs separate from mutable runtime state.

- [ ] Offer one apply_changes/refresh_documents boundary that consumes actual effect records or
  named-file refresh requests. Replace every scan-derived field together, including
  self_anchors, token language and validity metadata.

- [ ] When catalog inputs change (paths, aliases, stems or resolution config), rebuild the
  catalog and graph once from retained occurrences for all sources. This includes unchanged
  sources and missing/ambiguous edges; updating only modified files is insufficient.

- [ ] On token-affecting changes, rebuild or explicitly invalidate BM25 once per batch. Do not
  leave old postings marked current after a named-file refresh. Prefer coherent bounded
  reconstruction before a complex fully incremental scorer.

- [ ] Make snapshot persistence consume only coherent state; failures leave the file effects
  intact but report invalidated/unavailable index disposition. Preserve snapshot wire
  compatibility where possible; version semantic changes deliberately and test old-snapshot
  fallback. Invalidation is an observed persistence operation, not an in-memory label: if
  invalidation fails, report stale state and its error rather than success.

- [ ] Migrate journal and find refresh callers to the owner and remove their selective field
  assignments/direct graph mutation. Public legacy APIs get compatibility adapters rather than
  silent semver breaks.

Acceptance for this stage (focused checks):

- [ ] After set/append/task/move, disk and indexed normalized results agree for fields, anchors,
  aliases, link resolution and ranked eligibility.

- [ ] Adding/removing an alias or target updates backlinks from unchanged source notes,
  including prior missing/ambiguous targets.

- [ ] Named-file refresh finds new terms and no longer matches removed terms; stale data is
  never silently considered fresh.

- [ ] Instrumented bulk updates rebuild global graph/search structures at most once per batch
  when required, not once per modified file.

Scope: Do not combine this migration with a new snapshot format, new persistence database and
fully incremental BM25 algorithm. Keep field projection/lazy loading and resource limits
explicit; performance optimization follows parity.

### Stage 4: Typed query atoms and eligibility before scoring

- [ ] Represent Term and Phrase atoms distinctly with their required/optional/forbidden
  occurrence semantics. Preserve the currently documented flat OR behavior; do not silently
  introduce a new operator-precedence grammar.

- [ ] Compile query normalization/stemming once with explicit effective language. Evaluate
  term/phrase document sets, including positional adjacency for phrases, before ranking eligible
  documents.

- [ ] Compute scores from satisfied positive clauses only. A failed optional phrase must neither
  veto another satisfied OR clause nor admit documents with only part of the phrase.

- [ ] Reuse compiled term/phrase semantics in disk fallback, indexed evaluation and snippet
  selection where appropriate; retain legitimate empty snippets for title-only/cross-line
  matches.

- [ ] Add a small independent straightforward evaluator for test corpora, then compare optimized
  results against it across generated term/phrase/negation combinations.

- [ ] Remove intertwined clause-specific eligibility mutations from the scoring loop after all
  consumers use the new evaluator.

Acceptance for this stage (focused checks):

- [ ] "red blue" OR green includes green red x blue, excludes red alone, and includes an
  adjacent red blue phrase.

- [ ] rust -"memory safety" includes rust memory allocation and excludes the actual adjacent
  phrase.

- [ ] Positive/negative combinations, repeated terms, stopwords, language overrides and existing
  flat OR grammar agree between indexed and disk paths.

- [ ] Scores/order for unaffected queries retain baseline behavior; intentional corrections are
  documented separately from ranking policy changes.

Scope: No new search language, new ranking algorithm or learned relevance tuning. Structural
extraction and representative fixes belong here; this block closes the complete search finding
matrix.

### Stage 5: Close phrase, Boolean and unnecessary-probe defects

- [ ] R02: build documents with green red x blue, red alone, red blue and neither. Query "red
  blue" OR green must include the first and third and exclude the partial phrase; failed phrase
  clauses contribute no score.

- [ ] R03: rust -"memory safety" keeps rust memory allocation and excludes adjacent memory
  safety. Cover ordering, repeated terms, cross-line token adjacency and negative single terms.

- [ ] Expand the independent small-corpus evaluator to the supported flat OR/AND syntax,
  required/optional clauses, stemming/stopwords and language precedence. Compare exact result
  sets before score/order assertions.

- [ ] C13: zero-result property-regex queries with --no-hints, --jq or the typed API must not
  read bodies solely to prepare suggestions. Use an injected read counter or filesystem access
  observer; do not infer absence of reads from absent output.

- [ ] Keep result completeness and diagnostics under capped/skipped content explicit. Run
  indexed, refreshed-index and disk fallback variants with sections and projections.

- [ ] Consume HintDemand from iteration 290 and enforce zero hint-only body reads here. Hint
  serialization and preview/apply replay remain in iteration 294 and are not prerequisites.

Acceptance for this stage (focused checks):

- [ ] The concrete Boolean/phrase examples return the expected sets through every supported
  search path.

- [ ] Unchanged ranking contracts retain score/order; corrections are isolated from new
  relevance tuning.

- [ ] Hint-disabled metadata queries incur zero hint-only body reads, while enabled hints still
  produce valid suggestions.

- [ ] No grammar-precedence change is silently introduced by replacing the evaluator.

Scope: This closes and expands this block regressions; if already fixed there, reuse its
implementation and prove the larger matrix.

### Stage 6: Close stale index and link-graph inconsistencies

- [ ] R04: build oldtoken snapshot, externally add newtoken, then find newtoken --file a.md
  --index. It must agree with disk; removed terms must disappear too, with honest freshness
  diagnostics.

- [ ] R06: with b.md and sub/b.md present, sub/a.md `[b](b.md)` belongs to sub/b.md backlinks.
  Add padded wikilinks and ambiguous/root/site-prefix alternatives to resolver parity tests.

- [ ] R07: aliases=true, target alias Nickname and source `[[Nickname]]`; an unrelated indexed
  property mutation on source must preserve the backlink. Also change an alias on the target and
  verify links from untouched sources.

- [ ] R08: insert frontmatter before a self-anchor and verify indexed line numbers match disk.
  Cover every scan-derived field with full-value replacement assertions.

- [ ] R15: instrument bulk indexed edits across growing sparse and dense graphs. Remove
  whole-graph traversal per changed source; rebuild once or maintain a tested reverse adjacency
  structure after correctness is established.

- [ ] Test initial/load/refresh/mutate/reload/disk parity, malformed or stale snapshots,
  resolver config changes, unparseable notes, skipped counters and index persistence failure.

Acceptance for this stage (focused checks):

- [ ] All four correctness reproductions have passing disk/index differential tests, including
  unchanged sources affected by catalog changes.

- [ ] Bulk graph work scales with one batch rebuild or touched adjacency, rather than N complete
  edge traversals.

- [ ] Performance evidence includes fixture size/edge count, cold/warm conditions and operation
  counters; no unmeasured speedup claim.

- [ ] Snapshot fallback/version behavior remains compatible and does not falsely mark stale
  derived state fresh.

Scope: Do not optimize away source occurrences needed to re-resolve previously missing or
ambiguous links. Do not conflate graph identity with filesystem move identity.

## Implementation entrypoints

- `crates/hyalo-cli/src/commands/find/mod.rs`
- `crates/hyalo-cli/src/commands/find/run.rs`
- `crates/hyalo-cli/src/commands/journal.rs`
- `crates/hyalo-core/src/bm25.rs`
- `crates/hyalo-core/src/case_index.rs`
- `crates/hyalo-core/src/content_search.rs`
- `crates/hyalo-core/src/discovery.rs`
- `crates/hyalo-core/src/frontmatter_links.rs`
- `crates/hyalo-core/src/index.rs`
- `crates/hyalo-core/src/link_graph.rs`
- `crates/hyalo-core/src/link_resolve.rs`
- `crates/hyalo-core/src/link_rewrite.rs`

## One block completion gate

- [ ] Finish all stage acceptance checks and record per-finding evidence. Preserve unrelated
  edits; public API/format changes must be explicit and tested. No whole-vault crash-transaction
  or concurrent-adversary guarantee is implied by the new types.

- [ ] Obtain one fresh independent read-only review of the integrated block. Resolve actionable
  findings; re-review repaired behavior where necessary. Review stages as one final change
  rather than automatically launching a reviewer for every checklist item.

- [ ] Before the final implementation commit/PR run, in order: cargo fmt; cargo clippy
  --workspace --all-targets -- -D warnings; cargo test --workspace -q. Run affected implemented
  xtask and package gates once for the integrated candidate, using actual dependencies and final
  generated assets.

- [ ] For npm/Pi/assets changes run their typecheck/build/tests and TS/Pi/Codex freshness checks
  against the same final binary. Build cargo build --release once after final asset generation,
  then use target/release/hyalo for changed-document inspection/strict lint and run git diff
  --check. Do not rebuild an unchanged binary between documentation-only internal stages.

- [ ] In an authorized remote run, publish one PR and wait for required CI on its final head;
  preserve required multi-platform jobs and GitHub merge checks. Avoid draft pushes merely to
  checkpoint internal stages. This planning request itself authorizes no implementation or
  publication.

- [ ] Reconcile the remaining five-or-fewer block plans once after final review/verification,
  preserving scope and dependencies. Record exact revision, commands, review/CI evidence and
  limits; mark only fulfilled tasks complete. Keep iteration 287 external consumer work
  deferred.

## Consolidation record — 2026-09-10

The user requested larger execution blocks to reduce repeated Clippy/test/review/CI waits. This
block incorporates the substantive tasks and acceptance criteria of original draft iterations
297, 298, 303, 308, 309 and the S10 portion of 313. Those drafts were uncommitted planning files
and are replaced by the six-block queue 290–295; their internal sections do not retain separate
branch or gate requirements.
