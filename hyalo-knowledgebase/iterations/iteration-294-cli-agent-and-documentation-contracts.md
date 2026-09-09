---
type: iteration
title: Iteration 294 — CLI, agent and documentation contracts
date: 2026-09-10
status: planned
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

- [ ] Introduce command-family resolved specs for find, lint, link fix and auto-link. Merge
  saved views/config/defaults once; retain provenance where needed for effective counter-flags
  and explicit empty inputs.

- [ ] Carry resolved target selection, query pattern/regex, sections, filters, sort,
  profile/rules, site-prefix, language and index context in the relevant spec. Preserve only
  supported fields per family, not a universal bag of optional booleans.

- [ ] Make continuation transforms explicit: with_limit(0), with_apply(true), or a specific
  drilldown. Scope-preserving continuations must not rebuild their operation from partial
  HintContext fields.

- [ ] Rework HintBuilder to store raw OsString/String arguments, not shell-quoted tokens. Shell
  quoting is a renderer; --file=value and -- terminators protect option-looking paths. Test raw
  argv without executing through a shell.

- [ ] For files-from stdin or long target lists, never emit a broader apply hint. Re-emit
  bounded resolved targets or omit the executable hint with a clear explanation when safe replay
  cannot fit; do not add implicit manifest writes during a preview.

- [ ] Thread hint demand into execution so discarded hints do not trigger body probes. Migrate
  find show-all and lint apply first, then both link families, and delete duplicated view
  resolution.

Acceptance for this stage (focused checks):

- [ ] Executing show-all retains the exact query and changes only limit; executing apply retains
  preview files, rules and effective exclusions.

- [ ] Tests cover config counter-flags, named views, stdin selections, leading hyphens, spaces,
  quotes and empty targets.

- [ ] Hint argv round-trips through Clap and resolves to the same semantic request; tests
  compare effects/result sets rather than only string snapshots.

- [ ] --no-hints, --jq and typed API calls perform no body I/O solely for a discarded
  suggestion.

Scope: No promise that a later command sees an unchanged filesystem. Replay preserves operation
scope/options; source fingerprint validation remains necessary at apply time. Do not expose an
unsupported universal SDK command abstraction.

### Stage 2: Close hint scope, empty-output and cardinality gaps

- [ ] C02: lint --files-from changed.txt --fix --dry-run on only a.md must produce an apply
  command that never changes eligible b.md. Repeat with --type, --profile, rule/fix-rule and
  stdin input.

- [ ] C03: auto-link preview with --first-only and --exclude-target-glob templates/* must not
  apply repeated mentions or excluded templates; include config and counter-flags.

- [ ] C04: fuzzy links-fix preview with --ignore-target draft must leave draft-noet.md untouched
  while repairing other-noet.md. Include case-insensitive behavior and ordinary versus fuzzy
  apply hints.

- [ ] C06: find timeout --section Wanted with limit 2 and three matches in a five-file vault
  must show exactly those three under its show-all hint. Cover regex, language, sort,
  site-prefix and index context.

- [ ] C07: empty files-from with --filenames0 or --filenames-only returns zero bytes. Execute a
  downstream filename consumer to prove no JSON envelope or [] becomes a path.

- [ ] A02/A03: repeated files for single-file read/backlinks/task-read fail consistently; empty
  input messaging agrees with exit status. Every hint for --file=-odd.md must parse and target
  that same file, including spaces/quotes/Unicode variants.

Acceptance for this stage (focused checks):

- [ ] Execute emitted raw argv and advertised shell commands on disposable fixtures; compare
  exact selected sets, rules and resulting bytes.

- [ ] Unselected sentinel files remain unchanged after every apply hint; unreplayable
  oversized/stdin selections yield advice, never broadened executable commands.

- [ ] Empty filename outputs are byte-exact; cardinality errors and missing targets honor
  selected error format.

- [ ] Capabilities and writes labels derive from normalized command intent rather than
  shell-string reparsing.

Scope: Do not add an implicit persistent replay file or silently replay stdin. No new shell
dialect guarantee without tests.

### Stage 3: Unify Pi diagnostics, argv and post-write checks

- [ ] C08: use one result-rendering helper for generic/typed successful calls so original stderr
  diagnostics are visible exactly once, without corrupting structured stdout. Test actual
  malformed-note/stale-index warnings with a real binary.

- [ ] C09: normalize raw --format=value, --format value, --jq and --index-file forms before
  injecting defaults. -f means file, not format; honor -- and reject conflicting duplicates
  clearly. Prefer shared option parsing helpers over new string heuristics.

- [ ] C10: make the post-write guardrail run for hyalo_set (and audit other exposed mutation
  tools) using committed effect paths, not only host write/edit events. Avoid double linting and
  recursive hooks; no writes means no guardrail work. Use the internal JSON report accessor from
  typed outcomes; preserve existing public npm set()/task() text ProcessResult return contracts.

- [ ] Configure an iteration status enum including completed and a note with an unchecked task.
  Invoke hyalo_set status=completed and assert visible HYALO002 output while preserving
  successful-write status; distinguish lint unavailable/error from a clean result.

- [ ] Regenerate Pi runtime/declarations and embedded copies using existing tools. Verify
  source-package, offline init installation and live Pi loading; keep TypeBox parameters and npm
  public result types compatible. Validate real TypeBox schemas and distinguish binary-backed
  host simulation from actual live-Pi loading. If live Pi is unavailable, record that acceptance
  pending; do not count a proxy TypeBox/mock host as live verification.

- [ ] Add cancellation/timeout/partial-error tests through the shared outcome path and document
  exactly which mutations receive checks.

Acceptance for this stage (focused checks):

- [ ] Generic and typed successful diagnostics are observable once; quiet mode preserves CLI
  semantics.

- [ ] Valid equals flags work without duplicate injections; positional option-looking values
  remain literal.

- [ ] The configured hyalo_set fixture visibly reports the real lint violation; no unsupported
  automatic-guardrail claim remains.

- [ ] npm typecheck/build/tests, TS/Pi freshness and source/offline integration tests pass
  against the same final binary.

Scope: Do not rebuild the transport stack or add a server. No package publication or external
account changes are part of this iteration.

### Stage 4: Make docs, help and skills executable contracts

- [ ] C11: replace JSON-array-to-xargs bulk recipes with native filtered mutation or correctly
  encoded files-from input. Execute nonempty/empty fixtures and paths with whitespace; assert
  only selected notes change. Update canonical skills and generated/embedded copies.

- [ ] C12: keep OKF dry-run exit semantics and make CI explicitly check results.changed and
  results.skipped_markers. Execute clean, drift, marker-skipped and actual-failure scenarios;
  help and README must describe the same gate.

- [ ] C14: remove unconditional views set from tidy orientation. Use inline diagnostic queries
  or inspect/reuse existing views; a tidy run must preserve all existing saved definitions
  byte-for-byte unless the user explicitly requested a replacement.

- [ ] C15: document aliases=false by default and condition alias resolution/no-rewrite promises
  on aliases=true. Test config inspection and a repair example under both modes.

- [ ] A06: replace all-write dry-run/all-links-preserved/universal-JSON claims with exact
  command-supported guarantees. Correct read whole-file measurements, ambiguity exceptions, jq
  limits and automatic lint coverage in help, README, skills, templates and relevant comments.

- [ ] Extend executable recipe gates to assert selected files, bytes changed, result fields and
  failure status, including shell consumers. Provide controlled fixture data for every case;
  syntax-only jq execution is not proof a pipeline works.

- [ ] Lead agent discovery with short help plus focused examples; retain comprehensive long help
  for reference. Update docs from verified behavior, removing historical/promotional claims
  unsupported by tests.

- [ ] Document read --frontmatter as frontmatter-only unless a section or line selection also
  requests the body. Execute separate body-only, frontmatter-only and combined --frontmatter
  --lines 1: examples; do not describe the standalone flag as simply including YAML in a
  full-file read.

- [ ] Describe jq guarantees that actually exist when this block lands. Do not advertise the
  future isolation work from iteration 295; that block must update the claims after its own
  tests pass.

Acceptance for this stage (focused checks):

- [ ] Each reviewed recipe executes against a disposable fixture and achieves its stated
  outcome, including empty input and intentional drift/failure.

- [ ] Existing saved views survive the prescribed tidy workflow; no setup writes are disguised
  as observation.

- [ ] CLI help, README, configuration docs, npm/Pi/Codex instructions and embedded copies agree
  on defaults, limits, exits and supported guarantees.

- [ ] Recipe tests fail when a filter is removed, [] is piped as a filename, or a CI drift check
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
294, 310, 311, 312. Those drafts were uncommitted planning files and are replaced by the
six-block queue 290–295; their internal sections do not retain separate branch or gate
requirements.
