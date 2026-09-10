---
type: iteration
title: Iteration 291 — Document parsing and lint consistency
date: 2026-09-10
status: completed
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-291/document-parsing-and-lint-consistency
priority: 1
depends-on: "[[iterations/iteration-290-execution-and-filesystem-foundations]]"
---

# Iteration 291 — Document parsing and lint consistency

## Outcome and dependency

Unify document framing, budgets and Markdown visibility, then close the related
parser/lint/template regressions in the same branch.

[[iterations/iteration-290-execution-and-filesystem-foundations]]

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: `S05`, `S09`, `S11`, `S13`, `R05`, `R11`, `R01`, `R09`, `R10`, `R12`, `R13`, `R14`.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-291/document-parsing-and-lint-consistency.
The stages below are implementation order within that branch, not separate Ralph iterations,
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

### Stage 1: One bounded document and frontmatter frame

- [x] Introduce DocumentFrame/FrontmatterFrame carrying exact byte spans, body offset, newline
  style, BOM policy and framing diagnostics; keep parsed property values separate from source
  spans.

- [x] Use bounded incremental BufRead filling instead of allocating full lines before checking
  limits. Share byte/line/node/depth budgets with the normal YAML parser and define boundary
  equality behavior.

- [x] Replace the scanner fixed 16-KiB metadata prefix, lint substring delimiter and duplicated
  frontmatter-reader framing with this component. Metadata-only requests stop after the complete
  allowed frame and never require reading a large body.

- [x] Validate the final serialized/spliced frontmatter using the same reader budget before
  committing. Remove higher-budget verification that permits files the normal reader rejects.

- [x] Migrate read, metadata scans and lint body slicing first, then all frontmatter writer
  entrypoints. Keep source offsets compatible with existing line numbering and byte-preserving
  splices.

- [x] Add generated boundary fixtures and a counted reader that demonstrates allocation/read
  caps before adversarial long lines are materialized.

Acceptance for this stage (focused checks):

- [x] Empty, missing and malformed delimiters, ---note keys, CRLF, BOM, trailing delimiter
  whitespace and EOF are interpreted identically across consumers.

- [x] Valid 20-KiB frontmatter is accepted by both metadata-only and full scans; exact limit and
  limit-plus-one cases have consistent diagnostics.

- [x] Every successful writer output is readable under the normal parser budgets; a
  near-node-limit list refuses an overflowing edit before mutation.

- [x] Metadata-only scans do not read the body; memory is bounded independently of first-line
  length.

Scope: No full YAML concrete-syntax-tree rewrite or requirement to load complete documents.
Extend existing splicing and parser budget code; retain accepted YAML and newline compatibility.

### Stage 2: Shared Markdown visibility and source spans

- [x] Extract or extend a core BodySyntax/SpanCursor API with source offsets and classifications
  for visible text, fenced/indented code, HTML comments, inline code and relevant container
  context. Reuse the best existing behavior after a comparison corpus; do not blindly choose one
  current parser as correct.

- [x] Make structural visitors inspect classified visible syntax and use authored raw bytes only
  to retain the content/location of an actual heading/task. Keep full-text search able to search
  literal code/comments according to its documented policy.

- [x] Move the shared classification below hyalo-mdlint so dependency direction remains mdlint
  -> core. Keep rule configuration/severity/profile behavior in mdlint; do not move third-party
  lint dependencies into core.

- [x] Migrate native HYALO rules and directive recognition onto spans. Recognize disable
  directives only in syntactic HTML comments; preserve existing fence/container and
  disable-next-line semantics.

- [x] Use shared ATX heading parsing and a section-filter type that accepts its documented
  Unicode input without debug-only assumptions. Retain source offset/newline mappings for fixes.

- [x] Delete or adapt the superseded scanner BodyState/LineScanner/BodySpans implementations;
  document the one remaining policy authority and consumers.

Acceptance for this stage (focused checks):

- [x] Code blocks, inline code and HTML comment examples remain byte-identical under lint --fix
  unless a rule explicitly targets those regions.

- [x] Hidden comment headings/tasks do not appear structurally; full-text queries still find
  literal content when requested.

- [x] ATX closing hashes, C#, escaped hashes, CRLF and Unicode section selectors behave
  consistently in debug/release and index/disk paths.

- [x] Existing stock-rule and native-rule suppression cases pass through the shared
  classification, including list-indented fences.

Scope: Do not introduce a full Markdown AST solely to share visibility; a streaming/span service
is sufficient if it meets the corpus. Do not silently adopt a different Markdown dialect.

### Stage 3: Close frontmatter framing, budget and link gaps

- [x] S05: valid ---note: "a<TAB>b" inside YAML must retain the literal tab after lint --fix
  --rule MD010. Include delimiter-like keys, empty frames and supported closing-line whitespace.

- [x] S09: a flow list with 4,997 scalar items near the 5,000-node budget must not become
  unreadable after set title=x. Check exact normal reader budgets on final output, including
  nested/flow/block alternatives and failed-write byte identity.

- [x] S11: use a counted/generated reader with a huge first line and a huge line after the
  opening delimiter to establish a cap before whole-line allocation. Test command entrypoints
  that previously read frontmatter before later file-size checks.

- [x] S13/R05: compare valid 20-KiB metadata-only find against full-body scans, direct read and
  indexed scans. Test 16-KiB split points inside UTF-8 and around the closing delimiter.

- [x] R11: description: | followed by an indented # `[[Target]]` is scalar content. Track
  literal/folded scalar indentation and chomping with bounded YAML lexical spans, preserving
  authored property/span data. Include ordinary YAML comments and quoted # as counterexamples.

- [x] Run the existing frontmatter-splice/scanner fuzz seeds and add these compact boundary
  cases; keep writer preservation and resource limits in the same test oracle.

Acceptance for this stage (focused checks):

- [x] Metadata-only and full scans agree on accepted frontmatter and diagnostics without
  requiring body reads for metadata-only results.

- [x] Every successful write is readable under standard limits and preserves unrelated body/YAML
  bytes.

- [x] Block-scalar links appear in backlinks, broken-link checks and rewrite source mapping;
  YAML comments do not create false links.

- [x] Bounded-reader tests use small controlled resource budgets rather than deliberately
  exhausting the host.

Scope: No full YAML parser replacement or broad reserialization of untouched frontmatter. R05 is
the duplicate of S13, not another independent fix.

### Stage 4: Close Markdown structure and scoped-lint defects

- [x] R01: native lint fixes must not rewrite indented [] example or checkbox examples inside
  multiline HTML comments. Cover HYALO001 and HYALO002 plus fenced/list-indented/inline literals
  and actual visible tasks.

- [x] R09: multiline HTML comments containing # Hidden and - [ ] example must not create
  sections, promoted titles or tasks in disk/index/read/task commands.

- [x] R10: an inline-code example of <!-- markdownlint-disable MD019 --> must not suppress a
  later real MD019 violation. Cover genuine directives and disable-next-line behavior as
  counterchecks.

- [x] R13: # C# stays C#; only syntactically valid ATX closing hash sequences are removed.
  Include trailing whitespace, escaped hashes and non-ASCII titles across outlines, title
  promotion and section selection.

- [x] R14: read --section Résumé must not panic in debug builds and must have consistent
  supported matching in release. Do not describe the baseline debug-only assertion as a release
  panic.

- [x] R12: filename template decisions/{n}-{slug}.md must include decisions/1-test.md in lint
  --type selection. Make coarse glob selection a superset followed by the exact template
  matcher; test one/two/many digits, padding and nonnumeric false positives.

Acceptance for this stage (focused checks):

- [x] Literal regions remain unchanged under native/stock lint fixes; hidden structure stays
  absent while full-text literal search remains supported.

- [x] Directive parsing honors syntax and existing suppression scope without disabling rules
  from examples.

- [x] Heading/task behavior agrees in standalone and indexed consumers on LF/CRLF and Unicode
  fixtures.

- [x] Type-scoped and explicitly targeted lint agree for valid numbered filenames; zero checked
  files cannot silently pass the valid one-digit fixture.

Scope: Keep current Markdown dialect and section-match policy unless the fix explicitly requires
a documented change. Do not broaden this into fuzzy heading matching.

## Implementation entrypoints

- `crates/hyalo-cli/src/commands/lint/fix.rs`
- `crates/hyalo-cli/src/commands/read.rs`
- `crates/hyalo-core/src/filename_template.rs`
- `crates/hyalo-core/src/frontmatter/parse.rs`
- `crates/hyalo-core/src/frontmatter_links.rs`
- `crates/hyalo-core/src/heading.rs`
- `crates/hyalo-core/src/index.rs`
- `crates/hyalo-core/src/scanner/body_state.rs`
- `crates/hyalo-core/src/scanner/frontmatter.rs`
- `crates/hyalo-core/src/scanner/mod.rs`
- `crates/hyalo-core/src/tasks.rs`
- `crates/hyalo-mdlint/src/engine.rs`
- `crates/hyalo-mdlint/src/rules/spans.rs`

## One block completion gate

- [x] Finish all stage acceptance checks and record per-finding evidence. Preserve unrelated
  edits; public API/format changes must be explicit and tested. No whole-vault crash-transaction
  or concurrent-adversary guarantee is implied by the new types.

- [x] Obtain one fresh independent read-only review of the integrated block. Resolve actionable
  findings; re-review repaired behavior where necessary. Review stages as one final change
  rather than automatically launching a reviewer for every checklist item.

- [x] Before the final implementation commit/PR run, in order: cargo fmt; cargo clippy
  --workspace --all-targets -- -D warnings; cargo test --workspace -q. Run affected implemented
  xtask and package gates once for the integrated candidate, using actual dependencies and final
  generated assets.

- [x] For npm/Pi/assets changes run their typecheck/build/tests and TS/Pi/Codex freshness checks
  against the same final binary. Build cargo build --release once after final asset generation,
  then use target/release/hyalo for changed-document inspection/strict lint and run git diff
  --check. Do not rebuild an unchanged binary between documentation-only internal stages.

- [x] In an authorized remote run, publish one PR and wait for required CI on its final head;
  preserve required multi-platform jobs and GitHub merge checks. Avoid draft pushes merely to
  checkpoint internal stages. This planning request itself authorizes no implementation or
  publication.

- [x] Reconcile the remaining five-or-fewer block plans once after final review/verification,
  preserving scope and dependencies. Record exact revision, commands, review/CI evidence and
  limits; mark only fulfilled tasks complete. Keep iteration 287 external consumer work
  deferred.

## Consolidation record — 2026-09-10

The user requested larger execution blocks to reduce repeated Clippy/test/review/CI waits. This
block incorporates the substantive tasks and acceptance criteria of original draft iterations
295, 296, 306, 307. Those drafts were uncommitted planning files and are replaced by the
six-block queue 290–295; their internal sections do not retain separate branch or gate
requirements.

## Foundation reconciliation — 2026-09-10

This note reconciles the iteration 290 candidate before independent review. The supervisor
will verify these interfaces against the landed checkpoint before this block starts.

Iteration 290 now supplies `hyalo_core::rooted::CapturedInput`, captured-source
`render_frontmatter` validation and `find_task_lines_in` / `render_tasks`.
The CLI set/append/task pilots prepare complete batches through
`commands::apply::PreparedChangeSet` before publication. Reuse these entrypoints:
replace their current framing/budget/visibility internals with the shared authorities,
preserving source offsets and byte identity. Do not recapture a source after transformation.
The new normal-reader output check is an existing guard, not completion of S09's boundary
matrix. All stage regressions and metadata-only counted-reader checks remain required.

Ownership stays core framing/syntax, mdlint rule policy, CLI apply orchestration. Validate
the existing prepared-mutation and deterministic baseline fixtures alongside new framing,
literal-region and reader/writer boundary tests. The exact candidate interfaces and platform
limits are recorded in [[research/rust-architecture-review-2026-09-10]].


## Verification record — 2026-09-10

The integrated implementation passed 5,063 workspace tests (2 ignored), strict Clippy,
the release build, implemented xtask/package checks, npm binary tests and representative
command regressions. Independent Astra review accepted the final repair. Removing the last
property now refuses before publication if the exposed body would become frontmatter;
ordinary removal succeeds. Framing, syntax visibility and final-output validation share the
core authorities described in the architecture handoff.

Validation targets authored Markdown, including broken frontmatter, missing delimiters and
common incomplete edits. Unsupported input receives a diagnostic and safe refusal. The user
explicitly excluded further random-gibberish exploration and exhaustive syntax campaigns.
Existing counted-reader boundaries, crash/security protections and original acceptance
criteria remain required. Fuzz evidence is limited to recorded seeds and bounded runs;
scanner seeds do not establish the writer-preservation oracle. Stub gates, ignored tests and
unexercised recipes are excluded from coverage. Retained buffers are bounded, but delimiter
classification can consume a long candidate line. No universal race, resource-isolation or
whole-vault crash-transaction guarantee is implied.

The run retains the complete original acceptance and finding ledgers, repair/review reports,
byte snapshots and measured native-review usage. Remote CI and merge are recorded separately;
this local verification does not claim that pending remote checks have passed.

PR #347 passed all eleven required checks at implementation head
0e56f0cd9504f0bed1b28944e754b0e2ed9ae29b, including Windows runtime tests and all npm
launchers. The completed metadata head must pass its own checks before GitHub merge.
