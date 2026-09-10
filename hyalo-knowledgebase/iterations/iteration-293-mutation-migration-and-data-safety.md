---
type: iteration
title: Iteration 293 — Mutation migration and data safety
date: 2026-09-10
status: planned
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-293/mutation-migration-and-data-safety
priority: 1
depends-on: "[[iterations/iteration-292-query-resolution-and-index-coherence]]"
---

# Iteration 293 — Mutation migration and data safety

## Outcome and dependency

Migrate every remaining writer onto the shared execution boundary and close destructive move,
confinement, configuration, partial-effect and concurrency findings.

[[iterations/iteration-292-query-resolution-and-index-coherence]]

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: `S01`, `S02`, `S03`, `S04`, `S06`, `S08`, `C05`, `C01`, `S07`, `S12`, `A01`, `A05`.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-293/mutation-migration-and-data-safety. The
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

### Stage 1: Migrate remaining note writers

- [ ] Inventory remaining note-write entrypoints, including command aliases, lint profiles and
  direct link-rewrite helpers; migrate selection, exact transformations, source versions and
  schema/budget validation to prepared operations.

- [ ] Preserve exclusive creation for new, idempotence/no-op behavior for property/tag
  operations and byte-preserving lint/link edits. Full-map tag/property fallback requires
  content-sensitive captured-input conflict detection.

- [ ] Make execute_plans_partial (or its compatible consolidated replacement) the single link
  executor. Parallel jobs may already have committed; retain every outcome, including successes
  after another job failed, instead of collecting with early result?.

- [ ] Route every committed/created note to mandatory index reconciliation and report
  refresh/invalidation failures. Delete direct note writes and command-owned journal flushes for
  migrated consumers.

- [ ] Replace misleading all-or-nothing and abort-first-failure comments with actual
  planning/commit guarantees; keep only compatibility wrappers that delegate safely.

Acceptance for this stage (focused checks):

- [ ] Each migrated note family has deterministic-preflight byte-identity tests and injected
  partial-failure effect tests.

- [ ] A newly occupied create destination is preserved; content conflicts in tag/property rename
  do not overwrite unrelated edits.

- [ ] Parallel link failures retain all committed paths and truthful index disposition.

- [ ] No migrated note entrypoint bypasses the coordinator; ordinary output/no-op behavior
  retains parity.

Scope: Use the established prepared-operation/report APIs; no new execution framework or product
features. Preserve unaffected output and public Rust/npm compatibility. Keep all operations
inside their declared root and report actual effects rather than intended work.

### Stage 2: Unify single and batch move execution

- [ ] Represent source entries, destination reservations, captured backlinks/self-links and
  exact rewrites in a prepared move plan. Preflight every deterministic collision/transformation
  before changing a note.

- [ ] Use explicit no-replace move operations and distinguish entry versus referent identity;
  preserve tested regular-file case-only behavior and reject unsafe symlink/hard-link
  collisions.

- [ ] Unify single/batch execution with per-operation committed/not-attempted/failure reporting.
  Include created directories, renamed entries, rewritten backlinks and finalization failures.

- [ ] Compensate only owned operations whose entry identity and content version still match,
  using no-replace renames in reverse dependency order. An intervening source/destination entry
  must never be overwritten during rollback.

- [ ] Retain restored/restore-failed/kept effects and reconcile the index to the actual final
  state on every exit. Remove separate single-file bare Result<()> and batch stringified-error
  paths.

Acceptance for this stage (focused checks):

- [ ] A backlink-write failure after rename returns complete move/rewrite/restore effects and
  accurate index state.

- [ ] An injected competing source entry survives compensation; restore failure is explicit.

- [ ] Single and batch paths share tested collision/no-clobber semantics, including case-only
  and alias/referent cases.

- [ ] Dry-run creates no destination directory and describes the same prepared operations; no
  all-or-nothing claim exceeds implemented recovery.

Scope: Use the established prepared-operation/report APIs; no new execution framework or product
features. Preserve unaffected output and public Rust/npm compatibility. Keep all operations
inside their declared root and report actual effects rather than intended work.

### Stage 3: Prepare config and integration artifact writes

- [ ] Plan all types/views/lint-rules configuration changes as TOML-preserving replacements
  through ConfigRoot, retaining unrelated settings and comments with captured-source conflict
  checks.

- [ ] Build one complete init/deinit artifact manifest for every selected integration before the
  first effect; preflight every artifact and parent, required directory and deterministic
  mode/config conflict.

- [ ] Use installation-root confined exclusive creation/replacement/removal rather than
  vault-relative note paths. Preserve existing tracked/untracked artifact and user-content
  policies; no external symlink target may be touched.

- [ ] Replace truncating fs::write call sites with the rooted session and report all
  creation/removal/replacement/finalization effects. Configuration failure must not be
  mislabeled as a note index update.

- [ ] Preserve valid explicit external config/index destinations through separate authority
  scopes and define multi-root ordering; never broaden root authority from an untrusted artifact
  path.

Acceptance for this stage (focused checks):

- [ ] A late manifest preflight refusal leaves all earlier integration artifacts unchanged.

- [ ] Claude/Pi/Codex escaping symlink fixtures are rejected before external I/O, while valid
  installation layouts remain compatible.

- [ ] Injected pre-persist TOML failure preserves the original; post-persist failure reports
  committed state.

- [ ] All config/integration writers use explicit sessions and expose complete per-artifact
  effects.

Scope: Use the established prepared-operation/report APIs; no new execution framework or product
features. Preserve unaffected output and public Rust/npm compatibility. Keep all operations
inside their declared root and report actual effects rather than intended work.

### Stage 4: Migrate generators and remove write bypasses

- [ ] Inventory and migrate generator create/update/delete paths, reserved-marker policies and
  explicitly configured changelog locations to prepared operations under the correct root.

- [ ] Keep generator dry-run/drift/status behavior and unmanaged-region preservation explicit.
  Compute all deterministic marker/config errors before publishing any generated file.

- [ ] Route index creation/removal through the normalized IndexIntent and rooted destination
  operations without recursively invoking note index maintenance.

- [ ] Produce a complete write-entrypoint inventory across CLI and public core wrappers; remove
  ambient WritePhase/PHASE and remaining direct application write/flush bypasses once every
  caller has an explicit session.

- [ ] Restrict low-level runtime mutation interfaces and keep safe compatibility adapters for
  published APIs. Replace token-presence ownership claims with runtime failure tests and
  meaningful narrow visibility/architecture checks.

- [ ] Verify no new helper permits direct unreported publication; document the final
  root/effect/index owner for every command leaf.

Acceptance for this stage (focused checks):

- [ ] Generator preflight errors leave files unchanged and partial I/O errors retain every
  observed effect.

- [ ] Reserved/user-authored regions and external explicit destination semantics retain their
  contract.

- [ ] No process-global write phase remains and every runtime writer is assigned an explicit
  owner.

- [ ] Existing journal source lint is retained only as a labelled narrow check or replaced;
  helper-name presence is never counted as transaction proof.

Scope: Use the established prepared-operation/report APIs; no new execution framework or product
features. Preserve unaffected output and public Rust/npm compatibility. Keep all operations
inside their declared root and report actual effects rather than intended work.

### Stage 5: Close confinement, move and index-destination defects

- [ ] S01: build one complete init/deinit artifact manifest for Claude, Pi and Codex before any
  mutation; validate every artifact parent under the installation root. Test .claude and .pi
  file/directory symlink escapes, partial installations and removal failures; no external
  fixture artifact may change.

- [ ] S02/S03: reject alias.md -> real.md moved onto real.md; distinguish hard-link/symlink
  entries from a supported regular-file case-only rename. Exercise a/Foo.md and b/foo.md into
  one directory on an actually case-insensitive volume, plus a case-sensitive counterpart and a
  destination created during execution.

- [ ] S04: index note.md, replace the file or its parent with an external symlink, then indexed
  regex-search an external marker. Refuse before opening external content; also cover ranked
  snippets and fallback reads to prove the shared path stayed intact.

- [ ] S06: inject config replacement prepare/persist/finalize failures for types, views,
  lint-rules and init. Original TOML survives pre-persist failure; post-persist effects are
  accurately reported with unrelated comments/settings preserved.

- [ ] S08: force a backlink replacement failure after a single-file move. Restore only verified
  owned changes where safe; otherwise emit all kept/restored/failed effects and a truthful index
  disposition. Cover batch and parallel backlink cases too.

- [ ] C05: with custom.idx and default .hyalo-index present, drop-index --index-file custom.idx
  removes only custom.idx. Test command-local/global aliases, conflicts, relative/CWD semantics,
  missing paths and no unnecessary snapshot load.

Acceptance for this stage (focused checks):

- [ ] Each listed ID has a regression that failed at the reviewed baseline and now passes or has
  an explicitly explained already-fixed migration test.

- [ ] No external fixture bytes are read or changed; no destination clobber occurs; collisions
  detectable in preflight leave all notes unchanged.

- [ ] I/O failures never discard committed move/config effects; index reads cannot silently
  serve a snapshot known invalid after failure.

- [ ] Platform-specific tests run on appropriate CI workers; an unsupported
  case-insensitive/symlink fixture is reported, not counted as coverage.

Scope: Do not change supported path syntax or replace semantic link case policy with filesystem
case policy. Do not claim arbitrary directory-swap race protection.

### Stage 6: Close partial mutation and lost-update findings

- [ ] C01: task toggle --all --count and append with --count or invalid jq must fail before
  changing bytes. Test unsupported options on all write leaf families, including early
  init/config/index paths. Valid runtime jq errors after a write must retain effects.

- [ ] S07: a.md has x: []; b.md has x: {k: v}. Appending x=one to both preflights the mapping
  incompatibility and changes neither. Separately inject unavoidable second-file failure and
  verify first-file effect/index reporting.

- [ ] A01: a.md has a task on line 6; b.md has ordinary text there. Batch toggle --line 6
  refuses before changing a. A failure injected after the first commit must expose that effect
  so a retry is not blind.

- [ ] S12: pause tag rename and properties-rename full-map fallback after captured input; change
  an unrelated field, including same-length content with controlled timestamps. Resume and
  require conflict/preservation rather than stale full-map overwrite. Test two Hyalo writers and
  an editor independently.

- [ ] A05: select a.md and in-vault alias.md -> a.md explicitly, then toggle all. Reject or
  deduplicate before apply so one physical task is not toggled twice. Add duplicate path
  spellings and hard-link identity where supported.

- [ ] Exercise append/task no-op, idempotent set, skipped malformed notes, quiet mode,
  structured error results and index invalidation failure. Document exact safe retry decisions
  from effect status.

Acceptance for this stage (focused checks):

- [ ] All deterministic transformation/selector/output errors leave notes/config/index
  unchanged.

- [ ] Injected partial execution has complete committed/failed/not-attempted accounting,
  including failures during index reconciliation.

- [ ] Unrelated concurrent edits remain intact or produce explicit conflict; no automatic retry
  replays a non-idempotent operation.

- [ ] Tests demonstrate conflict semantics rather than only atomic old-or-new byte visibility.

Scope: Do not promise kernel compare-and-swap against arbitrary external editors. If cooperative
locks are added, define their root scope, ordering and lifecycle separately from optimistic
version checks.

## Implementation entrypoints

- `crates/hyalo-cli/src/commands/append.rs`
- `crates/hyalo-cli/src/commands/changelog.rs`
- `crates/hyalo-cli/src/commands/create_index.rs`
- `crates/hyalo-cli/src/commands/drop_index.rs`
- `crates/hyalo-cli/src/commands/find/mod.rs`
- `crates/hyalo-cli/src/commands/init.rs`
- `crates/hyalo-cli/src/commands/init/codex.rs`
- `crates/hyalo-cli/src/commands/journal.rs`
- `crates/hyalo-cli/src/commands/links.rs`
- `crates/hyalo-cli/src/commands/lint/`
- `crates/hyalo-cli/src/commands/lint_rules.rs`
- `crates/hyalo-cli/src/commands/madr.rs`
- `crates/hyalo-cli/src/commands/mv.rs`
- `crates/hyalo-cli/src/commands/new.rs`
- `crates/hyalo-cli/src/commands/okf.rs`
- `crates/hyalo-cli/src/commands/properties.rs`
- `crates/hyalo-cli/src/commands/remove.rs`
- `crates/hyalo-cli/src/commands/tags.rs`
- `crates/hyalo-cli/src/commands/tasks.rs`
- `crates/hyalo-cli/src/commands/types.rs`
- `crates/hyalo-cli/src/commands/views.rs`
- `crates/hyalo-cli/src/dispatch.rs`
- `crates/hyalo-cli/src/output_pipeline.rs`
- `crates/hyalo-core/src/fs_util.rs`
- `crates/hyalo-core/src/link_rewrite.rs`
- `crates/xtask/src/mutation_journal.rs`

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
299, 300, 301, 302, 304, 305. Those drafts were uncommitted planning files and are replaced by
the six-block queue 290–295; their internal sections do not retain separate branch or gate
requirements.

## Foundation reconciliation — 2026-09-10

This note reconciles the iteration 290 candidate before independent review. The supervisor
will verify these interfaces against the landed checkpoint before this block starts.

Iteration 290 establishes checked `VaultRoot`, `InstallationRoot`, `ConfigRoot`,
`RelativeName`, `CapturedInput::prepare`, `PreparedReplacement::commit`,
`NewEntry::create`, `move_no_replace` and explicit fallible `WriteSession::finish`.
Effects distinguish entries from followed content. Source checks compare identity and exact
captured bytes; static confinement does not eliminate directory-swap races. The no-replace
pilot uses hard-link publication then source removal and rejects symlink moves. Extend its
backend only with tested case-only and guarded-compensation behavior; never fall back to a
clobbering rename. Windows directory sync is explicitly unavailable.

Set/append/task already use `PreparedChangeSet` and typed `ApplyReport`; retain those
pilots and expand their acceptance matrix. Types configuration carries its original capture
through replacement and reports post-persist effects, but its default-note loop and other
config/integration writers still require migration. `IndexIntent` already normalizes
create/drop destinations; filesystem publication ownership still belongs here. Remaining
legacy `WritePhase` callers include remove, property/tag rename and lint. Inventory every
writer again after 291/292. Sol owns implementation; delegate difficult move/rollback design
to Astra with serialized write ownership. Validate all original partial-effect, conflict,
external-symlink and actual-platform cases.
