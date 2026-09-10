---
type: iteration
title: Iteration 295 — Resource safety and final verification
date: 2026-09-10
status: planned
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-295/resource-safety-and-final-verification
priority: 2
depends-on: "[[iterations/iteration-294-cli-agent-and-documentation-contracts]]"
---

# Iteration 295 — Resource safety and final verification

## Outcome and dependency

Finish user-jq process isolation and dependency-health work, strengthen platform/behavioral
gates and record the final agent-reliability evaluation.

[[iterations/iteration-294-cli-agent-and-documentation-contracts]]

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: `A04`, `R16`.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-295/resource-safety-and-final-verification.
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

### Stage 1: Bound snapshot expansion and isolate user jq

- [ ] A04: run user-supplied jq compilation/evaluation in a bounded child worker with a narrow
  versioned pipe protocol. Reuse the preflight seam from 290; compile-ready precedes mutation,
  evaluation consumes the final envelope. Trusted built-in renderer filters may remain
  in-process.

- [ ] Parent owns timeout, bounded protocol input/output, termination and reaping; map recursion
  abort, runtime failure and timeout to structured errors retaining committed effects. Specify
  tested OS memory limits where available and residual host-OOM risk; a child alone is not a
  universal memory cap.

- [ ] Execute def f: [f]; f in a disposable child and confirm the CLI parent survives. Cover
  finite runtime errors after append, timeout, oversized output, cancellation, worker startup
  failure and Windows/Linux/macOS process behavior.

- [ ] Trace bincode 1.3.3 and yaml-rust 0.4.5 to their owning dependency. Prefer compatible
  maintained upstream releases and lockfile updates; if no safe replacement exists, record a
  dated bounded maintenance disposition with owner/revisit trigger. Do not label unmaintained as
  an exploitable advisory, suppress warnings silently, or weaken audits.

- [ ] After isolation passes its platform tests, update the active CLI/help/skill resource
  guarantees from iteration 294 to the exact supported behavior. This is part of this block, not
  another docs PR.

Acceptance for this stage (focused checks):

- [ ] Recursive jq fails in the child while the CLI returns a structured error; every
  timeout/abort path reaps its worker.

- [ ] Preflight jq failure leaves files unchanged; runtime jq failure preserves effect/index
  metadata from prior commits.

- [ ] Help states only implemented resource guarantees. Refreshed Rust/npm audits record actual
  results and explicit remaining maintenance dispositions.

Scope: Complete jq isolation and dependency maintenance after the snapshot expansion finding has
been fully closed in iteration 292. Do not claim universal memory safety from child-process
isolation. No general job service or silent audit suppression.

### Stage 2: Close platform gates and evaluate agent reliability

- [ ] R16: locate release artifacts using Cargo-reported paths or platform EXE_SUFFIX,
  respecting CARGO_TARGET_DIR and explicit targets. Verify the scale gate on Windows as well as
  Linux/macOS; do not infer runtime success from cross-compilation.

- [ ] Replace misleading success-returning dead-primitives/TODO stubs with implemented checks or
  explicit nonpassing unsupported status and remove them from any completed-gate claims.
  Existing quality-gates CI does not invoke these stubs; preserve its real checks.

- [ ] Upgrade the feature capability gate from help-token presence to Clap descriptor and
  runtime capability assertions, including nested leaves, count/output modes, empty selection
  and mutation preflight. Keep narrow lexical lints labelled as such.

- [ ] Wire scoped behavioral suites for prepared writes/faults, preview/apply equivalence,
  reader/writer budgets, graph/index parity and executable recipes. Demonstrate each gate
  rejects a controlled negative fixture or deliberately broken implementation in test-only
  conditions.

- [ ] Run scale measurements with exact file/edge counts, cold/warm state, allocation/read
  counters and noise controls. Cover metadata-only queries, bulk property edits and graph
  refresh, not only find/link-fix wall time.

- [ ] Create a held-out agent task set comparing Hyalo-assisted workflows to ordinary file
  tools/scripts on identical disposable vaults. Record model/version, prompts, allowed tools,
  success rubric, exact unintended edits, correctness/completeness, retries, calls/context and
  time. Use blinded or deterministic grading where possible; report failures and uncertainty, no
  promotional score from self-assessment.

- [ ] Reconcile every review ID and architecture invariant against final implementation/tests.
  All bug closure IDs require passing evidence; static risks need a real fix or explicit
  evidence-backed non-applicability, not an untested completed checkbox. Preserve iteration 287
  external consumer deferral.

Acceptance for this stage (focused checks):

- [ ] The scale runner finds the actual binary on all supported platforms and reports
  unavailable runtime checks honestly.

- [ ] No stub or string-presence check is counted as proof of a behavioral invariant.

- [ ] All 49 distinct prior finding groups have final dispositions and regression evidence; R05
  remains the single duplicate of S13.

- [ ] Agent comparison results are reproducible and report limits; if model execution is
  unavailable, that acceptance remains pending rather than being replaced by static review.

- [ ] Final gates and independent review apply to the exact finished tree; no
  release/publication or external consumer task is silently included.

Scope: This does not authorize a new release, repository-external migration or shared workflow
changes. Prefer existing Rust xtask and package-native JS/TS tooling; do not add Python as
repository tooling.

## Implementation entrypoints

- `.github/workflows/`
- `Cargo.lock`
- `Cargo.toml`
- `crates/hyalo-cli/src/main.rs`
- `crates/hyalo-cli/src/output/jq.rs`
- `crates/hyalo-cli/src/run.rs`
- `crates/hyalo-core/src/bm25.rs`
- `crates/hyalo-core/src/index.rs`
- `crates/xtask/src/bench_scale.rs`
- `crates/xtask/src/feature_fanout.rs`
- `crates/xtask/src/help_drift.rs`
- `crates/xtask/src/mutation_journal.rs`
- `crates/xtask/src/stubs.rs`
- `npm/hyalo/test/`

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
313, 314. Those drafts were uncommitted planning files and are replaced by the six-block queue
290–295; their internal sections do not retain separate branch or gate requirements.

## Foundation reconciliation — 2026-09-10

This note reconciles the iteration 290 candidate before independent review. The supervisor
will verify these interfaces against the landed checkpoint before this block starts.

Iteration 290 now has compile-only preflight and killable jq evaluation workers in
`output/jq.rs`, preserving the parent-owned `ExecutionReport` effects on output failure.
This pilot is the starting point, not closure of A04. Implement the planned narrow versioned
pipe protocol, cancellation/startup/abort/timeout/reap matrix and measured platform limits.
Current source/input/output/time caps and Unix resource setup are documented in the
architecture handoff; Darwin has no `RLIMIT_AS` guarantee and Windows currently relies on
parent termination and byte limits. Reassess every promise against actual platform tests.

Use `hyalo_cli::describe_invocation` for behavioral capability validation, including nested
leaves and hidden-option exclusion, rather than a second schema. Keep the 290 deterministic
compatibility and prepared-effect fixtures as regression inputs alongside all later blocks.
Astra owns jq/resource design and implementation; after its writer freezes, Sol owns remaining
gates, documentation and the held-out model comparison. The 50-row/49-group ledger and every
original acceptance criterion remain authoritative; no stub, static proxy or unavailable
platform/model run counts as passing evidence.


## Iteration 291 reconciliation — 2026-09-10

Carry DocumentFrame/read_frame_for_body, BodySyntax, heading parsing, exact filename matching
and complete render validation into the final reader/writer behavioral gates. Use deterministic
broken-frontmatter, missing-delimiter, incomplete-edit and common-mistake fixtures to prove
clear diagnostics, safe refusal and unchanged bytes.

Do not reopen exhaustive random-gibberish/fuzz syntax exploration. Controlled negative fixtures
remain required for crashes, data loss, security, confinement, resource bounds and unsafe
partial writes. A04, R16, the 50-row/49-group ledger and all original acceptance rows remain
unchanged.


## Iteration 292 reconciliation — 2026-09-10

Carry the reviewed 292 contracts into final behavioral verification: complete owner-managed
replacements and pending-generation guards; explicit disk/snapshot case and alias policy;
skipped filename identity distinct from metadata coverage; successful repair clearing only
current diagnostics for that path; conservative move ambiguity and alias preservation; typed
Boolean/phrase eligibility shared with snippets; and zero actual hint-only reads for suppressed
demand. Snapshot wire format remains v2, with real disk fallback when untouched postings lack
current tokenizer metadata. Retain the S10 crafted-snapshot expansion refusal and
unchanged-note preflight evidence, plus 8/32-note sparse/dense batch counters and independent
authored query references; their measured fixtures imply neither universal memory bounds nor
speedup guarantees.

A04/R16 ownership, the 50-row/49-group ledger and every original criterion remain unchanged.
Retain `291-ci-resource-observation.json` solely as A04 diagnostic-ordering evidence: the
failing check passed one retry with no product change, and this observation adds no new scope.
The 292 review accepted all four repairs; supplied execution records report 5,083 passing
tests, two ignored tests and a successful release build, not final 295 or remote closure. Use
normal authored Markdown, including broken frontmatter, missing delimiters and incomplete
edits, with useful diagnostics and safe refusal without corruption where processing cannot
continue; preserve crash, security, confinement, resource and partial-write protections without
exhaustive random-gibberish/fuzz exploration.
