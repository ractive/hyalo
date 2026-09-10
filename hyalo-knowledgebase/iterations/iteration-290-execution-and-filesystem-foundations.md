---
type: iteration
title: Iteration 290 — Execution and filesystem foundations
date: 2026-09-10
status: completed
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-290/execution-and-filesystem-foundations
priority: 1
---

# Iteration 290 — Execution and filesystem foundations

## Outcome and dependency

Establish validated invocation, confined filesystem operations, prepared mutation/effect
accounting and one typed output boundary. Migrate representative commands so the foundations
control real execution.

Reviewed main `857517a2c9be9a1c4a8b1c0b451082c81e6ecb09`; no unfinished in-repository prerequisite.

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: foundation block; representative fixes are verified by the later owning block.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-290/execution-and-filesystem-foundations. The
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

### Stage 1: Validate one prepared invocation before execution

- [x] Inventory every executable leaf: target cardinality, output/count/projection capabilities,
  writes, schema/profile needs, index destination and empty-input behavior. Derive command
  identity from the actual Clap hierarchy; an explicit exhaustive capability match is
  acceptable, a second hand-maintained command-name list is not.

- [x] Introduce private-field PreparedInvocation, OutputPlan and distinct
  SingleTargetRequest/BatchTargetRequest values in CLI application modules. Keep Clap DTOs and
  generated TypeScript argument compatibility; normalize defaults and config once without
  pulling Clap into hyalo-core.

- [x] Reject unsupported count/format/projection combinations and compile jq syntax before any
  effectful dispatch. A valid filter can still fail at evaluation; preserve that later
  distinction for the output stage in this block and isolation block 295. Bound jq preflight itself; use a compile-only child seam if
  compilation cannot be safely bounded in-process. Do not move an untrusted compiler into the
  main process merely to fail earlier.

- [x] Normalize IndexIntent::Read/Create/Drop and resolve global/local index destination aliases
  once before index I/O. Preserve explicit CWD-relative index paths and default vault-relative
  paths; deletion does not need snapshot loading.

- [x] Resolve files-from once and enforce single-target cardinality before opening content.
  Preserve explicit-selection versus unrestricted-selection provenance and define an explicit
  empty selection; do not turn zero files into whole-vault selection.

- [x] Migrate read, backlinks, task read and mutation output preflight. Remove their old
  first()-selection and repeated validation paths. Record remaining legacy entrypoints and their
  migration owner in the architecture report.

- [x] Capture observable CLI contracts on deterministic fixtures: stdout bytes, stderr, exit
  status, selected paths and filesystem effects. Separate intentional fixes from preserved
  formatting; do not snapshot a known defect as the desired contract.

- [x] Produce a test-consumable descriptor from Clap plus the exhaustive capability metadata,
  with canonical long/short spellings, value forms, nested command identity and supported output
  modes. This is the producer used by the final gate and npm/Pi option-parity tests; do not
  create a second generated public schema system.

- [x] Define exact single-input outcomes: absent required target becomes a Clap usage error
  (exit 2), an intentional normalization from the current read exit-1 behavior; a missing
  explicit literal target or multiple resolved targets is a structured user error (exit 1 when
  JSON selected); single-file commands given an explicit empty or all-skipped files-from list
  return the same structured cardinality error (exit 1), preserving selection counters and
  diagnostics. This deliberately corrects the contradictory historical exit-0 help while keeping
  typed single-result APIs honest. Batch/query commands keep exit-0 empty outcomes; find
  filename projections emit zero bytes. Do not confuse an empty list with a missing literal
  file.

- [x] Produce effective query inputs (filters, section/language policy and selection provenance)
  and HintDemand::None/Requested with OutputPlan now. Derive hint demand from --no-hints, jq and
  typed-call output intent. Query evaluation in iteration 292 must not depend on the later
  hint/replay refactor.

Acceptance for this stage (focused checks):

- [x] Invalid --count and malformed --jq on set/append/task leave all fixture bytes unchanged;
  no index/config write occurs.

- [x] Repeated named files for single-file commands produce a structured cardinality error even
  when the second path is missing; zero/one/many selections are tested.

- [x] CLI/config/default precedence, -- terminators, --flag=value, TTY/default formats and
  existing TS argument declarations remain compatible except documented corrections.

- [x] Actual runtime dispatch accepts only the prepared output/input path for migrated commands;
  a unit test of an unused constructor is insufficient.

Scope: No new public CLI flags, command registry framework, MCP server, crate split or broad
parser rewrite. Do not touch Homefinder. Existing iteration 287 remains in progress only for its
deferred external consumer.

### Stage 2: Rooted file identities and explicit write sessions

- [x] Add a small rooted-filesystem module inside hyalo-core. Distinguish VaultRoot,
  InstallationRoot and ConfigRoot at the application boundary, plus lexical relative names,
  existing read/write targets and new-entry destinations. Constructors are private or checked; a
  path newtype is not a permanent authorization token.

- [x] Keep directory-entry identity separate from followed-file identity so alias/referent moves
  cannot masquerade as case-only renames. Capture fingerprints from the same opened source used
  to plan; explicitly handle symlinks, hard-link identity, missing destinations and filesystem
  name equivalence.

- [x] Provide confined open, prepare replacement, commit replacement, no-replace move and
  confined artifact removal operations. Revalidate existing parent components immediately at
  I/O; never require reading external content to establish refusal. Include exclusive create-new
  and explicit creation/removal/directory effects; preserve create_new behavior under a
  destination introduced after planning.

- [x] Encapsulate platform behavior behind concrete small helpers. Verify Unix/macOS and Windows
  no-clobber primitives or use a maintained narrow dependency after evaluation. If a platform
  cannot implement an operation safely, return an explicit unsupported outcome before mutation
  rather than falling back to clobbering rename.

- [x] Introduce for migrated consumers, alongside a temporarily retained legacy WritePhase, an
  explicit WriteSession that owns durability policy and touched directories. finish() returns
  finalization errors; Drop performs only best-effort cleanup and cannot be the
  success-reporting mechanism. Separate progress callbacks from persistence policy. Record which
  legacy callers still use ambient state; complete removal belongs to the final writer-migration
  slice.

- [x] Migrate one real config replacement and indexed regex read; retain safe compatibility
  wrappers for existing published APIs until the planned caller migration. Add narrow
  deterministic fault injection at prepare, persist and finalize boundaries.

Acceptance for this stage (focused checks):

- [x] External file and parent-directory symlinks are rejected before read/write/delete in
  migrated operations; valid in-root aliases retain their documented semantics.

- [x] Existing destination is never overwritten by the no-replace primitive, including a
  destination introduced between planning and execution.

- [x] Failure before persistence preserves original bytes; failure after persistence returns a
  committed effect with failed finalization, never a plain no-effect error.

- [x] Two independent sessions in one process do not share progress, directory-sync lists or
  durability policy. Tests record actual platform capability skips.

Scope: No claim of a vault-wide crash transaction or immunity to an adversary swapping
directories during system calls. Preserve the documented threat-model limit unless a
handle-relative implementation is explicitly designed and tested. Atomic replacement and crash
durability remain separate contracts.

### Stage 3: Prepared changes and effects that survive failure

- [x] Introduce CapturedInput, PreparedChangeSet and ApplyReport with explicit per-path effects:
  unchanged, committed, not attempted, failed-before-commit and
  committed-with-finalization-error. Use actual observed effects, not intended writes or a
  success count inferred after the loop.

- [x] Precompute all deterministic transformations, selector checks, schema checks and output
  budgets before the first note write. Bound aggregate memory by staged prepared data or compact
  plans; do not collect the entire vault into owned strings. Validate exact rendered YAML with
  current normal-reader limits already here; iteration 291 later centralizes that policy. Every
  selected input is validated before the first commit; bounded preparation never permits
  committing an early chunk before validating later inputs.

- [x] Deduplicate physical mutation targets and reject incompatible alias/hard-link selections.
  Record source fingerprints and verify before replacing; distinguish conflict from I/O failure.
  Non-idempotent task operations must never silently execute twice for one identity. Use
  content-sensitive version checks, including equal-size edits with coarse/restored timestamps;
  mtime and size alone do not meet this contract.

- [x] Implement a CLI-owned apply coordinator using this block operations. Start with serial
  commit for transparent ordering; parallel planning is acceptable. Define stop-on-error and
  not-attempted states explicitly.

- [x] On every exit after a committed effect, reconcile the current journal with actual changed
  paths and report updated, invalidated or unavailable index state. Prefer invalidating an
  unsafe snapshot to presenting it as fresh. Index maintenance failure must not erase the
  content effect report. This invocation finalizer is mandatory immediately, before iteration
  292 replaces the journal internals. Report Invalidated only after persistent invalidation
  succeeds. If both refresh and invalidation fail, report Stale/UpdateFailed with that error and
  retain committed effects; never promise the next process will reject an unchanged snapshot.

- [x] Migrate set/append/task handlers and delete their write-then-early-return paths. Use an
  internal report adapter until this block standardizes wire output. Document retry semantics:
  task toggle is not safe to retry without checking effects.

Acceptance for this stage (focused checks):

- [x] Two-file append with [] then mapping and task toggle with valid then invalid task line
  perform no writes when the defect is knowable in preflight.

- [x] Injected second-file I/O failure reports the first committed path and remaining
  unattempted paths; index state is accounted for and a retry cannot be mistaken for a first
  attempt.

- [x] Injected post-persist permission/finalization failure reports committed bytes accurately.

- [x] CRLF, inline YAML comments, unrelated properties/body bytes and no-op behavior remain
  preserved; dry-run and apply derive from the same prepared edits.

Scope: No generic transaction framework, distributed lock service or promise to undo arbitrary
concurrent edits. Reuse existing partial-execution types where their semantics fit; consolidate
them rather than retaining competing reports.

### Stage 4: One outcome, rendering and failure boundary

- [x] Change internal successful payloads from valid-JSON-by-comment String to
  serde_json::Value, constructed once from the existing named Serialize DTOs. Retain distinct
  raw text/byte variants; do not add a giant enum duplicating all existing result structs.

- [x] Return typed UserDiagnostic and ExecutionReport values with domain status,
  diagnostics/completeness and optional effects. Remove command-local serialization of errors,
  exit_code_override mutation and selection counter injection after dispatch.

- [x] Give OutputPlan and one renderer responsibility for JSON/text/GitHub output, filename
  projections, empty results, hints, sanitization and exit classification. Keep NUL filename
  bytes exact and raw-content behavior explicit.

- [x] Preserve success JSON shape, ordering, optional omission versus null and singular error
  hint. For partial failure, specify additive structured effect/index fields and stable error
  categories; update Rust exports, generated TS, npm error parsing and documentation in the same
  change. Do not silently change a published result type or invent versioned envelopes without a
  migration decision. Keep successful public npm set()/task() ProcessResult text APIs; provide
  an internal JSON mutation-report accessor for Pi so it can inspect committed/no-op/partial
  effects without scraping text.

- [x] Compile jq before dispatch; if evaluation or rendering fails after writes, preserve the
  execution effect report and distinguish output failure from mutation failure. Define
  broken-pipe behavior and error precedence; domain lint findings must not mask a
  renderer/system failure.

- [x] Remove obsolete success-string helpers and migrate all command result constructors
  mechanically, keeping command-specific formatting behavior under parity tests.

Acceptance for this stage (focused checks):

- [x] Every success payload reaches serialization once; existing valid JSON/text/byte contracts
  match the captured baseline except listed fixes.

- [x] Partial apply and post-write jq error preserve effects in structured stderr and npm
  HyaloError; successful diagnostics remain separate from stdout.

- [x] Zero-result filename projections produce zero bytes, and invalid user inputs are
  consistently structured when JSON was selected.

- [x] TS generation and npm contract tests validate old success consumers plus new error
  metadata; no hand-edited generated artifacts.

Scope: No replacement of dynamic frontmatter Values with an artificial static schema. Keep
anyhow for contextual unexpected errors and concrete enums at domain boundaries. Public core
struct privacy changes require compatibility handling.

## Implementation entrypoints

- `crates/hyalo-cli/src/cli/inputs.rs`
- `crates/hyalo-cli/src/commands/append.rs`
- `crates/hyalo-cli/src/commands/find/mod.rs`
- `crates/hyalo-cli/src/commands/init.rs`
- `crates/hyalo-cli/src/commands/inputs.rs`
- `crates/hyalo-cli/src/commands/journal.rs`
- `crates/hyalo-cli/src/commands/mutation.rs`
- `crates/hyalo-cli/src/commands/set.rs`
- `crates/hyalo-cli/src/commands/tasks.rs`
- `crates/hyalo-cli/src/dispatch.rs`
- `crates/hyalo-cli/src/error.rs`
- `crates/hyalo-cli/src/output.rs`
- `crates/hyalo-cli/src/output_pipeline.rs`
- `crates/hyalo-cli/src/run.rs`
- `crates/hyalo-cli/tests/e2e/`
- `crates/hyalo-core/src/discovery.rs`
- `crates/hyalo-core/src/fs_util.rs`
- `crates/hyalo-core/src/link_write.rs`
- `npm/hyalo/src/generated/`

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
290, 291, 292, 293. Those drafts were uncommitted planning files and are replaced by the
six-block queue 290–295; their internal sections do not retain separate branch or gate
requirements.

## Verification record — 2026-09-10

Implementation commit `d6b71fd1755bbaf7cd9005f50a296ec1d5b5e18e` in
[PR 346](https://github.com/ractive/hyalo/pull/346) passed all eleven required CI jobs,
including Linux/macOS/Windows workspace tests and npm checks. Independent Astra reviews
resolved I290-R01 through I290-R05 and the Linux Clippy and Windows replacement findings.
The final local ordered fmt/Clippy/test sequence passed 4,998 tests; Windows passed 4,923.
Two local ignored doctests, four Windows ignores and Unix-only exclusions are not coverage.
All eleven implemented xtask checks, release/npm checks and changed-document strict lint
passed. The success-returning stubs and the unexercised MADR recipe remain excluded.

The bounded FIFO regression retains confinement, normalized identity and opened-handle
validation. Compatibility writers close source readers before Windows replacement.
Static confinement, directory-sync and jq resource limitations remain as documented in
[[research/rust-architecture-review-2026-09-10]]. Successor plans 291–295 were re-read
against the reviewed code with no semantic changes needed; no later pending plan exists.
All 50 original finding rows retain their later closure owners, including the R05/S13
duplicate. Iteration 287's external consumer remains deferred.

The Ralph run `run-20260909T224558Z-290-295` retains exact snapshots, acceptance evidence,
reviews and CI records. This completion-record change is verified separately; the final
PR head must pass CI again before the GitHub merge checkpoint advances the queue.
