---
type: iteration
title: Iteration 294 — CLI, agent and documentation contracts
date: 2026-09-10
status: in-progress
tags:
  - iteration
  - rust
  - architecture
  - astra-review
branch: iter-294/cli-agent-and-documentation-contracts
priority: 1
depends-on: "[[iterations/iteration-293-mutation-migration-and-data-safety]]"
---

# Iteration 294 — CLI, agent and documentation contracts

## Outcome and dependency

Make hints preserve the complete resolved operation, unify Pi diagnostics and guardrails, and
execute corrected help/README/skill/CI recipes.

[[iterations/iteration-293-mutation-migration-and-data-safety]]

Architecture rationale and complete finding map: [[research/rust-architecture-review-2026-09-10]].
Original findings: [[reviews/astra-code-review-2026-09-09]].

Closure IDs: `C02`, `C03`, `C04`, `C06`, `C07`, `A02`, `A03`, `C08`, `C09`, `C10`, `C11`, `C12`, `C14`, `C15`, `A06`.

## Execution and validation budget

This is one iteration, one branch and one PR: iter-294/cli-agent-and-documentation-contracts.
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

### Stage 1: Resolved scope drives execution and hints

- [x] Introduce command-family resolved specs for find, lint, link fix and auto-link. Merge
  saved views/config/defaults once; retain provenance where needed for effective counter-flags
  and explicit empty inputs.

- [x] Carry resolved target selection, query pattern/regex, sections, filters, sort,
  profile/rules, site-prefix, language and index context in the relevant spec. Preserve only
  supported fields per family, not a universal bag of optional booleans.

- [x] Make continuation transforms explicit: with_limit(0), with_apply(true), or a specific
  drilldown. Scope-preserving continuations must not rebuild their operation from partial
  HintContext fields.

- [x] Rework HintBuilder to store raw OsString/String arguments, not shell-quoted tokens. Shell
  quoting is a renderer; --file=value and -- terminators protect option-looking paths. Test raw
  argv without executing through a shell.

- [x] For files-from stdin or long target lists, never emit a broader apply hint. Re-emit
  bounded resolved targets or omit the executable hint with a clear explanation when safe replay
  cannot fit; do not add implicit manifest writes during a preview.

- [x] Thread hint demand into execution so discarded hints do not trigger body probes. Migrate
  find show-all and lint apply first, then both link families, and delete duplicated view
  resolution.

Acceptance for this stage (focused checks):

- [x] Executing show-all retains the exact query and changes only limit; executing apply retains
  preview files, rules and effective exclusions.

- [x] Tests cover config counter-flags, named views, stdin selections, leading hyphens, spaces,
  quotes and empty targets.

- [x] Hint argv round-trips through Clap and resolves to the same semantic request; tests
  compare effects/result sets rather than only string snapshots.

- [x] --no-hints, --jq and typed API calls perform no body I/O solely for a discarded
  suggestion.

Scope: No promise that a later command sees an unchanged filesystem. Replay preserves operation
scope/options; source fingerprint validation remains necessary at apply time. Do not expose an
unsupported universal SDK command abstraction.

### Stage 2: Close hint scope, empty-output and cardinality gaps

- [x] C02: lint --files-from changed.txt --fix --dry-run on only a.md must produce an apply
  command that never changes eligible b.md. Repeat with --type, --profile, rule/fix-rule and
  stdin input.

- [x] C03: auto-link preview with --first-only and --exclude-target-glob templates/* must not
  apply repeated mentions or excluded templates; include config and counter-flags.

- [x] C04: fuzzy links-fix preview with --ignore-target draft must leave draft-noet.md untouched
  while repairing other-noet.md. Include case-insensitive behavior and ordinary versus fuzzy
  apply hints.

- [x] C06: find timeout --section Wanted with limit 2 and three matches in a five-file vault
  must show exactly those three under its show-all hint. Cover regex, language, sort,
  site-prefix and index context.

- [x] C07: empty files-from with --filenames0 or --filenames-only returns zero bytes. Execute a
  downstream filename consumer to prove no JSON envelope or [] becomes a path.

- [x] A02/A03: repeated files for single-file read/backlinks/task-read fail consistently; empty
  input messaging agrees with exit status. Every hint for --file=-odd.md must parse and target
  that same file, including spaces/quotes/Unicode variants.

Acceptance for this stage (focused checks):

- [x] Execute emitted raw argv and advertised shell commands on disposable fixtures; compare
  exact selected sets, rules and resulting bytes.

- [x] Unselected sentinel files remain unchanged after every apply hint; unreplayable
  oversized/stdin selections yield advice, never broadened executable commands.

- [x] Empty filename outputs are byte-exact; cardinality errors and missing targets honor
  selected error format.

- [x] Capabilities and writes labels derive from normalized command intent rather than
  shell-string reparsing.

Scope: Do not add an implicit persistent replay file or silently replay stdin. No new shell
dialect guarantee without tests.

### Stage 3: Unify Pi diagnostics, argv and post-write checks

- [x] C08: use one result-rendering helper for generic/typed successful calls so original stderr
  diagnostics are visible exactly once, without corrupting structured stdout. Test actual
  malformed-note/stale-index warnings with a real binary.

- [x] C09: normalize raw --format=value, --format value, --jq and --index-file forms before
  injecting defaults. -f means file, not format; honor -- and reject conflicting duplicates
  clearly. Prefer shared option parsing helpers over new string heuristics.

- [x] C10: make the post-write guardrail run for hyalo_set (and audit other exposed mutation
  tools) using committed effect paths, not only host write/edit events. Avoid double linting and
  recursive hooks; no writes means no guardrail work. Use the internal JSON report accessor from
  typed outcomes; preserve existing public npm set()/task() text ProcessResult return contracts.

- [x] Configure an iteration status enum including completed and a note with an unchecked task.
  Invoke hyalo_set status=completed and assert visible HYALO002 output while preserving
  successful-write status; distinguish lint unavailable/error from a clean result.

- [x] Regenerate Pi runtime/declarations and embedded copies using existing tools. Verify
  source-package, offline init installation and live Pi loading; keep TypeBox parameters and npm
  public result types compatible. Validate real TypeBox schemas and distinguish binary-backed
  host simulation from actual live-Pi loading. If live Pi is unavailable, record that acceptance
  pending; do not count a proxy TypeBox/mock host as live verification.

- [x] Add cancellation/timeout/partial-error tests through the shared outcome path and document
  exactly which mutations receive checks.

Acceptance for this stage (focused checks):

- [x] Generic and typed successful diagnostics are observable once; quiet mode preserves CLI
  semantics.

- [x] Valid equals flags work without duplicate injections; positional option-looking values
  remain literal.

- [x] The configured hyalo_set fixture visibly reports the real lint violation; no unsupported
  automatic-guardrail claim remains.

- [x] npm typecheck/build/tests, TS/Pi freshness and source/offline integration tests pass
  against the same final binary.

Scope: Do not rebuild the transport stack or add a server. No package publication or external
account changes are part of this iteration.

### Stage 4: Make docs, help and skills executable contracts

- [x] C11: replace JSON-array-to-xargs bulk recipes with native filtered mutation or correctly
  encoded files-from input. Execute nonempty/empty fixtures and paths with whitespace; assert
  only selected notes change. Update canonical skills and generated/embedded copies.

- [x] C12: keep OKF dry-run exit semantics and make CI explicitly check results.changed and
  results.skipped_markers. Execute clean, drift, marker-skipped and actual-failure scenarios;
  help and README must describe the same gate.

- [x] C14: remove unconditional views set from tidy orientation. Use inline diagnostic queries
  or inspect/reuse existing views; a tidy run must preserve all existing saved definitions
  byte-for-byte unless the user explicitly requested a replacement.

- [x] C15: document aliases=false by default and condition alias resolution/no-rewrite promises
  on aliases=true. Test config inspection and a repair example under both modes.

- [x] A06: replace all-write dry-run/all-links-preserved/universal-JSON claims with exact
  command-supported guarantees. Correct read whole-file measurements, ambiguity exceptions, jq
  limits and automatic lint coverage in help, README, skills, templates and relevant comments.

- [x] Extend executable recipe gates to assert selected files, bytes changed, result fields and
  failure status, including shell consumers. Provide controlled fixture data for every case;
  syntax-only jq execution is not proof a pipeline works.

- [x] Lead agent discovery with short help plus focused examples; retain comprehensive long help
  for reference. Update docs from verified behavior, removing historical/promotional claims
  unsupported by tests.

- [x] Document read --frontmatter as frontmatter-only unless a section or line selection also
  requests the body. Execute separate body-only, frontmatter-only and combined --frontmatter
  --lines 1: examples; do not describe the standalone flag as simply including YAML in a
  full-file read.

- [x] Describe jq guarantees that actually exist when this block lands. Do not advertise the
  future isolation work from iteration 295; that block must update the claims after its own
  tests pass.

Acceptance for this stage (focused checks):

- [x] Each reviewed recipe executes against a disposable fixture and achieves its stated
  outcome, including empty input and intentional drift/failure.

- [x] Existing saved views survive the prescribed tidy workflow; no setup writes are disguised
  as observation.

- [x] CLI help, README, configuration docs, npm/Pi/Codex instructions and embedded copies agree
  on defaults, limits, exits and supported guarantees.

- [x] Recipe tests fail when a filter is removed, [] is piped as a filename, or a CI drift check
  stops inspecting its fields.

Scope: Preserve historical completed iteration records as history; correct active docs and
misleading current source comments. Do not expand current product scope merely to make old
marketing language true.

## Implementation entrypoints

- `README.md`
- `crates/hyalo-cli/src/cli/args.rs`
- `crates/hyalo-cli/src/commands/find/run.rs`
- `crates/hyalo-cli/src/commands/inputs.rs`
- `crates/hyalo-cli/src/dispatch.rs`
- `crates/hyalo-cli/src/hints.rs`
- `crates/hyalo-cli/src/hints/`
- `crates/hyalo-cli/src/hints/command.rs`
- `crates/hyalo-cli/src/hints/links.rs`
- `crates/hyalo-cli/src/output_pipeline.rs`
- `crates/hyalo-cli/src/run.rs`
- `crates/hyalo-cli/templates/`
- `crates/hyalo-cli/templates/pi/`
- `crates/xtask/src/jq_recipes.rs`
- `docs/ci.md`
- `docs/configuration.md`
- `npm/hyalo/src/api.ts`
- `npm/hyalo/src/pi-runtime.ts`
- `npm/hyalo/test/pi.test.js`
- `pi-package/extensions/hyalo.ts`
- `pi-package/lib/`
- `pi-package/skills/hyalo-tidy/SKILL.md`
- `pi-package/skills/hyalo/SKILL.md`

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

- [ ] In an authorized remote run, publish one PR and wait for required CI on its final head;
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
294, 310, 311, 312. Those drafts were uncommitted planning files and are replaced by the
six-block queue 290–295; their internal sections do not retain separate branch or gate
requirements.

## Foundation reconciliation — 2026-09-10

This note reconciles the iteration 290 candidate before independent review. The supervisor
will verify these interfaces against the landed checkpoint before this block starts.

Iteration 290 supplies `hyalo_cli::describe_invocation`, derived from the actual
Clap hierarchy and exhaustive capabilities, plus `EffectiveQuery`, selection provenance,
`HintDemand` and prepared single/batch requests. Extend these for lint/link-family resolved
specs and raw-argv continuation transforms; do not create a second command-name registry.
Views already normalize before output/index I/O. Empty filename projections and single-input
cardinality corrections have foundation tests; retain them and execute downstream consumers
and emitted hints across the complete acceptance matrix.

The Pi runtime's internal `mutationReport()` uses hidden
`--internal-mutation-report` with JSON/no-hints to receive generated
`MutationReportEnvelope` and actual committed/unchanged/partial effects. Public npm
`set()`/`task()` text `ProcessResult` contracts are unchanged. Adopt that accessor for
guardrails and use `HyaloError.effects`/`category` on failure; do not scrape success text
or infer changed files from requested targets. Hidden transport metadata is excluded from
public descriptors/types. Preserve broken-pipe exit 141 and renderer/system exit 2.
Read the current jq implementation before documenting guarantees; portable isolation remains
295's responsibility. Actual live Pi loading and model-backed guardrail evidence remain
mandatory, distinct from host simulations.

The first independent review found that trailing warning summaries could hide structured
effects from npm. The repaired output boundary drains summaries before the final envelope;
retain the real malformed-note plus post-write failure regression when unifying diagnostics.
Sanitize the completed text effect diagnostic while keeping exact filenames in JSON.


## Iteration 291 reconciliation — 2026-09-10

Iteration 291 supplies the exact contract for active docs and adapters: standalone read
--frontmatter is frontmatter-only; combining it with a body selector requests both; bounded
framing and body capture share DocumentFrame/read_frame_for_body. BodySyntax supplies
structural visibility for code/comments without claiming a full Markdown AST.

C08 diagnostics must surface malformed authored frontmatter and safe refusals once without
corrupting structured stdout. A06/help/README/skills should state that broken frontmatter,
missing delimiters and incomplete edits receive useful diagnostics and no mutation when
processing cannot continue; do not promise compatibility with arbitrary fuzzed/gibberish
syntax.


## Iteration 292 reconciliation — 2026-09-10

Iteration 292 connects `HintDemand` to the actual injected body-read boundary: no-hints, jq and
typed-API argv shapes perform zero hint-only reads, while enabled hints use bounded rooted
reads and the shared 291 syntax. Preserve this measured contract when migrating resolved specs
and raw-argv continuations; the fixture begins after validated jq preflight and does not
establish new worker guarantees. Reuse `CompiledQuery::new(query, language)` through
`score_compiled` and `SnippetQuery::from_compiled`, retaining flat-OR eligibility, language
precedence and valid empty snippets for title-only/cross-line matches. Keep exact authored
`written_target` separate from canonical `target`.

C08/C15 and active docs must preserve explicit alias/case policy on disk and snapshots, skipped
filename identity without claiming metadata coverage or write authority, and diagnostics
cleared only for successfully repaired notes while other skips remain current. Keep the
independent language/Boolean and section/projection/cap regressions alongside actual-read
checks. All original acceptance criteria, ownership and live-Pi requirements remain unchanged.
Normal authored Markdown includes broken frontmatter, missing delimiters and incomplete edits:
provide useful diagnostics and safe refusal without corruption where processing cannot
continue; retain crash, security, confinement, resource and partial-write protections without
exhaustive random-gibberish/fuzz exploration.

## Iteration 293 reconciliation — 2026-09-10

Consume the final 293 observed report through internal `mutationReport()` and
`HyaloError.effects`/`category`. Public npm `set()`/`task()` retain text
`ProcessResult` results; the hidden transport remains outside public descriptors
and argument types. C08/C10 must account for `EffectState::Restored`,
`RestoreFailed` and `Kept`, partial publication and finalization errors. Follow-up
checks use actual surviving note effects and paths, never requested targets or
success-text parsing; retain index outcomes and distinguish installation-directory
effects from notes. Do not introduce generic retries for toggle/append or partial
failures.

`MutationJournal::finalize_observed` reconciles actual final paths: missing notes
are removed before an `Updated` snapshot; unsafe/unreadable paths invalidate, and
failed invalidation reports `UpdateFailed`. Lint worker errors retain earlier
publication effects. `run_init_observed`/`run_deinit_observed` retain artifact and
directory effects through `ReportError`, including created directories before a
later asset failure. Runtime generator dispatch honors global index inputs;
source-compatible direct generator wrappers use an explicit no-index journal.

Active documentation must reflect captured transformations and explicit
`WriteSession` finalization. Public frontmatter/task transformation wrappers keep
their signatures while sharing one source capture through publication; their
legacy Result errors state when publication already committed. The low-level
`atomic_write_within` contract is confined atomic publication of caller-supplied
final bytes, not stale-transform protection. Preserve ordinary Markdown diagnostics
for broken frontmatter, missing delimiters and incomplete edits, with safe refusal
without corruption when processing cannot continue.

All 49 original checkbox texts, finding owners and the 293 dependency remain
unchanged. Actual live Pi loading and model-backed guardrail evidence remain
required; host simulations and static checks cannot fulfill them. Use the final
293 review and native CI evidence recorded by the supervisor, without inferring
platform execution from cross-compilation or an earlier local run.


## Verified implementation — 2026-09-10

The integrated candidate passed independent Astra review after two repair passes.
Resolved continuation specs preserve the selected operation; raw arguments remain
bounded and round-trip through Clap. Lower-confidence link review also retains
the original candidate threshold. Pi surfaces diagnostics once, checks actual
surviving typed-mutation effects and reports unavailable lint when configuration
lookup fails, retrying lookup on a later call without replaying a mutation.
Public npm text result contracts remain unchanged.

The final release is bound to SHA-256
`5c00c61d65688c20fae1e925a50e4b895b679e585eda3ca7b3ab40d971ed1f73`.
Local verification records 5,135 Rust passes, strict Clippy, npm typecheck/build
and 38 tests, generated-asset checks, and actual live Pi loading and guardrails
using Pi 0.84.4 with Sol/high. Executable recipe validation includes selected and
unselected file bytes, empty input, and the actual documented OKF Bash checks
for clean, drift, skipped-marker and command-failure states. Its executed
ignored-check control detects suppressed drift failure.

Two ignored Rust tests, the two existing stub gates and the unavailable MADR
recipe are excluded from coverage. Feature-fanout help checks are not behavioral
capability proof. Iteration 295 retains the stronger gates and resource work;
current jq, filesystem and partial-write limitations remain in force.
Native PR checks and the final remote checkpoint are still pending.
