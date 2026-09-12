---
type: iteration
title: "Iteration 296 — Review follow-ups: document safety, Pi ownership, and indexed search"
date: 2026-09-12
status: in-progress
tags:
  - iteration
  - review
  - safety
  - architecture
branch: iter-296/review-followups
priority: 1
---

# Iteration 296 — Review follow-ups

## Objective and handoff

Fix the five reproduced findings from [[reviews/codebase-review-2026-09-12]]
and verify them through realistic CLI workflows. Also close the closely related
`new` preview/apply mismatch recorded in
[[dogfood-results/dogfood-v0230-2026-09-12]]. This plan is self-contained for a new
session after `/clear`; temporary reproduction files are optional evidence.

Reviewed baseline: `36745a4604d8c398303cee61027c7151c778996a`, v0.23.0.
Iteration 295 is completed. This plan and its two linked reports were the
uncommitted implementation handoff. Their original bytes are preserved in the
run baseline on `iter-296/review-followups`.

Read the repository instructions and linked reports before implementation.
Re-find symbols rather than relying on the baseline line numbers below.
The prior review passed formatting, strict Clippy, 5,153 Rust tests and 38 npm/Pi
tests despite these bugs; completion requires assertions about actual file
contents, resolved links, ownership and execution paths.

## Scope and architecture constraints

Keep the existing crates and checked boundaries: prepared invocation/output
preflight, captured source bytes and identity, rooted publication, explicit
write sessions, effect reports, and complete index reconciliation. The current
architecture makes sense for the CLI. Correct consumers of those boundaries;
do not restart the architecture project.

Use normal authored Markdown, small targeted fixtures and an ordinary large
corpus. Preserve useful diagnostics and safe refusal for incomplete frontmatter
and malformed managed markers. No fuzz campaign, exhaustion experiment, race
harness, generic framework, broad API migration or unrelated cleanup belongs
in this iteration. Do not remove resource limits to pass a performance check.
Iteration 287's deferred external consumer work remains out of scope.

## Finding ownership and execution order

| Order | Finding | Main implementation surface | Required result |
| --- | --- | --- | --- |
| 1 | F1: MADR deletes prose after a literal marker | `commands/managed_region.rs`, `commands/madr.rs` | Only the actual managed region changes |
| 2 | F2: Pi setup/removal destroys an unrelated manifest | `commands/init.rs`, Pi installation ownership support | Shared user configuration survives init and deinit |
| 3 | F3: moved filenames produce broken links | Core `link_write.rs`, resolution/parsing helpers, move preflight | Every supported emitted link resolves to the planned target |
| 4 | F4: scaffold defaults become wrong types or invalid YAML | `commands/new.rs`, shared frontmatter emitter | Generated values round-trip exactly; preview agrees with apply |
| 5 | F5: a fresh MDN index rejects its own BM25 section | Core `index.rs`, `bm25.rs`, ranked-query callers | Direct scoring works without unbounded token reconstruction |

The source paths above are relative to `crates/hyalo-cli/src/` for commands
and `crates/hyalo-core/src/` for core modules. Keep changes scoped to these
responsibilities and the affected callers, tests and shipped documentation.

## Tasks and acceptance criteria

### F1 — Parse real managed markers before planning a splice

Baseline: `managed_region.rs:62`, called by `madr.rs:127`.
The existing code finds the first opening-marker substring anywhere in a file.
It recognizes an inline-code example as authority to replace surrounding prose.

Reproduce with `dir = "kb"`, an ADR at `kb/docs/decisions/0001-choice.md`, and
this `kb/docs/decisions/README.md`:

```markdown
# Decisions

Example marker: `<!-- madr:toc:begin -->`

KEEP THIS USER PARAGRAPH

<!-- madr:toc:begin -->

Old generated table

<!-- madr:toc:end -->

KEEP THIS FOOTER
```

`hyalo madr toc --apply` currently exits 0 and deletes the paragraph, the real
opening marker and the example's closing backtick.

- [x] Use shared document/body syntax facts to identify standalone markers outside literal examples; do not add another independent Markdown scanner.
- [x] Have presence detection and splice planning consume one validated marker classification. Return a clear refusal for duplicate, reversed or incomplete real marker pairs.
- [x] Preserve the existing adopt/replace contract: marker-less files retain their prose by default; explicit replacement is a separate operation. Malformed markers must not silently authorize deletion.
- [x] Assert exact bytes before and after the actual managed region survive the example above, plus compact fenced/indented example and CRLF controls.
- [x] Assert malformed marker refusals and previews leave files unchanged, healthy generation updates only the intended block, and a second apply is unchanged.
- [x] Check the affected generator callers for policy consistency without rewriting unrelated OKF behavior or exit-code contracts.

### F2 — Establish ownership of shared Pi configuration

Baseline: `init.rs:929` replaces `.pi/package.json`; `init.rs:1454` removes it
unconditionally. In separate fresh fixtures, seed:

```json
{"private":true,"dependencies":{"custom-extension":"1.0.0"}}
```

`hyalo init --pi` currently removes the dependency; `hyalo deinit` deletes the
manifest even when Hyalo's Pi integration was never installed.

- [x] Define the minimal ownership record needed to distinguish user fields, pre-existing integration entries and additions actually made by Hyalo. Path confinement alone is not ownership.
- [x] Merge only required Hyalo integration entries into valid existing manifests. Preserve dependencies, scripts, unrelated Pi registrations and unknown fields; avoid duplicates on repeated init.
- [x] Preflight manifest parsing and conflicts before installing artifacts. Preserve or clearly refuse an unrecognized/invalid existing manifest rather than replacing it.
- [x] On deinit, remove only proven Hyalo-owned additions. Preserve entries that predated installation and user changes made afterward; retain ambiguous state with a useful diagnostic.
- [x] Define conservative compatibility for older Hyalo-created manifests without an ownership record. Do not infer ownership from a filename, package name or version alone.
- [x] Add CLI tests for both independent reproductions, fresh install, repeated init, init/deinit with shared configuration, post-install user edits, and supported legacy cleanup.
- [x] Retain symlink refusals, captured-source conflict handling and observed partial-effect reporting. Include any new ownership artifact in the same installation preflight and publication/removal boundary.
- [x] Verify the shipped Pi package and embedded installation still load the intended runtime; refresh embedded/generated copies only when their canonical sources change.

### F3 — Emit syntax-safe links and verify destination identity

Baseline: `link_write.rs:124` splices raw computed destination text. Create
`note.md` and `ref.md` containing `[Note](note.md)` and `[[note]]`. In separate
fixtures, move `note.md` to `C#.md` and `Release (final).md`. Both moves currently
succeed, but the next link query reports broken rewritten occurrences.

- [x] Give Markdown destinations and wikilink destinations their appropriate encoding/escaping policy through the shared link writer; preserve fragments, labels and supported written forms.
- [x] Verify parsing and resolution of emitted links identify the planned destination. If a destination cannot be represented safely in an affected syntax, refuse before the move or use the existing explicit unsupported-rewrite contract; never count a broken occurrence as successfully updated.
- [x] Cover the two reproduced names, a space-containing name, a literal percent sequence, and an existing fragment/label with compact authored tests. Avoid an exhaustive filename-character matrix.
- [x] Exercise a normal single move and a small batch, checking disk/index link and backlink agreement after publication. Keep external-link behavior unchanged.
- [x] Exercise the shared writer's affected `links fix`/`links auto` paths so escaping policy does not diverge by entrypoint.
- [x] Preserve no-clobber destination planning, case-equivalence checks, source receipts and partial-effect reporting. Preview and apply must use the same encoded plan/refusal.

### F4 — Use the shared YAML emitter for scaffolds

Baseline: `new.rs:537`, `new.rs:637`; preflight at `new.rs:108` checks size only.
With `dir = "kb"`, use this schema:

```toml
[schema.types.note]
required = ["title"]
[schema.types.note.defaults]
title = "[Draft]"
[schema.types.note.properties.title]
type = "string"
```

`hyalo new --type note --file note.md` currently creates an array-valued title.
A `"{draft}"` default becomes a mapping, and `"first\nsecond"` produces
frontmatter that named `find` refuses to parse.

- [x] Serialize keys and values through the shared YAML emitter while retaining deliberate property ordering and scaffold sections.
- [x] Validate the complete generated document against normal framing/parser/resource budgets before creating files or directories. Exact supplied values must survive a read-back with their intended types.
- [x] Preserve the documented deliberately invalid placeholders for missing values; parser validity is distinct from whether a placeholder satisfies the schema.
- [x] Test bracketed, brace-containing and multiline defaults, a key requiring quotation, ordinary typed defaults, and a missing-value placeholder control.
- [x] Close the associated dogfood observation: `new --dry-run` and apply must perform the same predictable parent-path validation. An in-vault `alias -> real` directory currently previews success but apply exits 2. Preserve the chosen symlink policy and provide the same actionable refusal in both modes.
- [x] Verify unsupported/invalid scaffolds and previews create no file or directory, and existing targets remain untouched.

### F5 — Separate scoring safety from reconstruction safety

Baseline: `index.rs:694` uses `bm25.rs:934`'s aggregate expanded-token estimate
when admitting a BM25 section, even for queries that score directly from postings.
On the ordinary 14,375-file MDN checkout, index creation reports zero warnings,
but the first indexed `AbortController` query rejects that section and asks for
a rebuild. Results remain correct but indexed ranked search falls back to disk.

- [x] Separate structural validity and bounded scoring admission from budgets needed to reconstruct owned token arrays. Retain checks for malformed paths/IDs/postings and allocation amplification.
- [x] Keep direct-postings scoring available for a valid compact index when only its hypothetical full expansion exceeds the reconstruction cap.
- [x] Budget the actual selected reconstruction operation where applicable. Mutation/refresh must retain a safe bounded refusal or documented fallback, honest effects and correct snapshot disposition; do not expose an unchecked reconstruction path through the refactor.
- [x] Make creation/load diagnostics consistent with the usable index capabilities. Do not recommend rebuilding unchanged content to fix a content-based resource limit.
- [x] Use a small focused test with a lowered test-only budget, or an equivalent bounded fixture, to prove scoring can succeed while full reconstruction is refused. Keep the existing compact-snapshot amplification regression passing without performing dangerous allocations.
- [x] Build a fresh MDN snapshot and compare the same indexed/disk ranked query and metadata filter. Assert exact results and prove direct indexed scoring was used through an existing observable counter/test seam; elapsed time alone is insufficient.
- [x] Record modest wall-time spot-checks and resource-policy limitations. Do not impose an invented machine-specific speed threshold or raise/remove caps to satisfy the check.

## Focused verification and completion

Use one freshly built artifact and record its absolute path, revision and hash.
Multiple stale Cargo/Homebrew binaries were present during the review; do not
use an unverified PATH executable for reproductions. Use disposable vaults and
read-only external corpora; write temporary indexes outside them explicitly.

- [x] Add focused regressions asserting actual preservation, resolved identity and ownership for F1–F5 and the related `new` preview mismatch.
- [x] Recheck the previous working controls: scoped lint apply-hint replay, literal task examples, invalid jq/count before writes, empty files-from, ordinary indexed set/task/move parity, static external symlink refusals and case-collision refusal.
- [x] Run the repository's commit/PR gates in order: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -q`; address failures.
- [x] Run the npm typecheck and test suite against the final native binary and the relevant existing embedded/generated-asset drift checks. Exercise the affected Pi installation behavior, not only source-string checks.
- [x] Build the final release binary and dogfood the repaired examples, own KB, and MDN indexed query. Record which checks actually ran and which platform paths remain CI-only.
- [ ] Obtain a fresh review of the final changes through the repository's review workflow; address verified issues and complete applicable native CI before landing under the repository's normal PR process.
- [ ] Write a concise follow-up dogfood report, update the five finding dispositions with evidence, and mark this plan completed only after its acceptance checks pass. Keep the historical review's original baseline and findings intact.

## Explicit dispositions for other review observations

The broad mutable command context, `PreparedCommand::Legacy`, process-global
library configuration and remaining checked-path/raw-open adapters are recorded
architecture debt, not additional refactors in this iteration. Preserve the
existing documented concurrency/crash-durability limits; do not claim stronger
filesystem guarantees from these fixes.

The suggested generator textual diff is deferred UX work; structural marker
safety is required regardless. The own-KB's historical warnings, orphan counts
and broken-link backlog are documentation maintenance outside this fix scope.
The stale PATH observation is handled by binary verification during validation.

Optional local evidence: `/tmp/hyalo-review-20260912`, containing the original
small fixtures, per-command JSON, test logs and fixed reviewed binary. The plan
and linked review contain sufficient reproductions if that directory is gone.

## Local verification — 2026-09-12

F1–F5 and the related scaffold preview mismatch are implemented and locally
verified. See [[dogfood-results/dogfood-iteration-296-2026-09-12]] for the five
dispositions, fixed artifact, exact command evidence, resource limits and
performance spot checks. Two independent review findings about duplicate JSON
keys and repeated-init ownership were repaired; the fresh final review is clean.
All 5,168 Rust tests and 39 npm tests pass, along with the supported xtask gates
and 66 final release dogfood commands.

The full 289-plan inventory has no later pending iteration; iteration 287's
external consumer remains deferred and unchanged. Historical review findings
and their original baseline remain intact. No resource cap or promised
acceptance criterion was weakened.

Native Linux/Windows CI, PR publication and merge remain pending authorization.
The two final workflow/completion rows remain open and status stays
`in-progress`; the report portion of the last row is fulfilled. This local
checkpoint does not claim remote landing or full iteration completion.
