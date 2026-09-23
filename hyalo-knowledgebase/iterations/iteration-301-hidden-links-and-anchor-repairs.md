---
type: iteration
title: "Iteration 301: hidden-file links and heading-anchor repairs"
date: 2026-09-20
status: completed
tags:
  - iteration
  - links
  - lint
branch: iter-301/hidden-links-and-anchor-repairs
---

# Hidden-file links and heading-anchor repairs

## Problem and evidence

The 2026-09-20 Jev-assisted tidy of mapl-memory exposed three deterministic
link-tooling gaps while exercising [[iterations/iteration-300-jev-tidy-integration]].
These findings are independent of Jev classification and require no provider calls.

- Seven links to existing hidden repository files were reported as broken targets:
  `.gitignore`, `.github/workflows/lint-kb.yml`,
  `.github/workflows/no-binaries.yml`, and `.github/workflows/likec4-pages.yml`.
  Source-relative links from nested documents failed as well as root-level links.
  All targets were independently verified to exist; strict lint reported seven
  errors across four documents. No link or configuration workaround was applied.
- An initiative brief linked to `solution-space/prd.md#success-metrics`, but the
  target heading was `## 6. Success metrics`, producing `#6-success-metrics`.
  `find --broken-links` marked the fragment broken; `links fix --dry-run` counted
  one broken anchor but offered zero repairs. A user-approved one-line correction
  cleared the anchor diagnostic without changing headings or metadata.
- Strict lint did not report that broken anchor. This is a documented coverage
  limitation, not a newly established regression: HYALO006 checks file targets;
  the existing CI workaround is `find --broken-links --strict`.

The source repository had 495 Markdown files, zero schema violations, zero
generated-index drift, and 24 intentional template orphans. Preserve those
distinctions: an excluded target is not necessarily missing, and an orphan is not
automatically a defect. Reproduce with synthetic content; do not copy private
engineering documents into tests or depend on the temporary session artifacts.

Related work: [[iterations/iteration-297-markdown-file-links]],
[[iterations/iteration-190-link-anchors]],
[[iterations/iteration-211-links-resolution-correctness]], and
[[iterations/iteration-215-anchor-and-broken-links-followups]].

## Intended behavior

An explicit Markdown path to an existing, in-vault hidden file resolves without
opting that file or its directory into document discovery. Resolve `.gitignore`
as written even though it has no conventional extension. Keep source-relative
precedence and the existing vault-root fallback. Do not enumerate hidden trees,
read target contents merely to establish existence, or admit hidden files as
bare-name, alias, or fuzzy candidates as a side effect.

Add a separately configurable broken-heading-anchor lint rule, enabled as a
warning by default and promoted by strict mode. Allocate an unused HYALO rule ID
during implementation. Keep HYALO006's target-only identity; reuse the shared
anchor matcher so find and lint agree on same-file and cross-file fragments,
duplicate-heading slugs, percent encoding, block references, and templates.

`links fix --dry-run` should show conservative anchor-only proposals, including
source line, old/new fragment, and matching heading. For this iteration, support
the concrete numbered-heading case: a broken slug matches exactly one heading
after removing its leading decimal section number, and the emitted replacement
is that heading's actual generated slug. Broader fuzzy guesses remain advisory.
Ordinary `links fix --apply` writes these safe anchor repairs together with safe
file-target repairs. `--apply-fuzzy` additionally enables eligible fuzzy file-target
repairs; broader fuzzy anchor guesses remain advisory.

The user's implementation follow-up supersedes the original separate
`--apply-anchors` opt-in: safe anchor repairs belong in ordinary `--apply` so the
repair command remains discoverable. The eligibility checks, preview and stale
source/heading revalidation still apply.

## Scope and implementation tasks

### Existing hidden targets

- [x] Add a minimal fixture with a root README, a nested note, `.gitignore`, and hidden workflow files; reproduce current find, lint, summary, and repair-preview results before changing code.
- [x] Trace the failure through discovery and shared resolution. Inspect `CaseInsensitiveIndex::complete`: its current miss-as-absence optimization must not equate a filtered scan with all possible explicit targets.
- [x] Implement bounded exact-path existence resolution for the omitted hidden targets while retaining canonical vault containment, symlink rules, path precedence, and case-mode behavior.
- [x] Specify and test how explicit `[scan] exclude`, gitignored targets, hidden Markdown targets, and snapshots interact with existence checks; do not silently turn excluded files into scanned documents or weaken traversal boundaries.
- [x] Keep `find`, HYALO006, summary, backlinks/graph classification, and `links fix` on the shared resolution path. Preserve the distinction between document graph edges and non-document attachments.
- [x] Bound/cache repeated existence probes within an invocation and check that the fix does not reintroduce a whole-tree walk or per-link filesystem cost for ordinary indexed hits.

### Anchor lint coverage

- [x] Add the rule catalog entry, configuration/severity handling, and shared anchor context, reusing existing parsed heading data and bounded per-target caching rather than rescanning the vault for each linted file.
- [x] Report a file-absolute source line and fragment for each definitely broken anchor, in text, JSON, and GitHub annotation output; never report an anchor failure in addition to a missing-target failure on the same link.
- [x] Include same-file fragments, cross-file links, and configured frontmatter wikilinks. Preserve skips for block references, external URLs, templated/unknowable anchors, and existing scanner exclusions.
- [x] Verify vault-wide target context under `--file`, `--files-from`, and glob-scoped lint; disabling or filtering out the new rule must avoid its unnecessary heading work.

### Reviewed fragment repairs

- [x] Add typed anchor repair plans and include eligible anchor repairs in ordinary `links fix --apply`; define JSON counts separately from file-target repairs and document whether broken counts describe pre- or post-apply state.
- [x] Generate a candidate for `#success-metrics` to `#6-success-metrics` only when the numbered-heading normalization has one eligible match. Ambiguous, duplicate-heading, templated, or merely similar candidates remain unapplied with a visible reason.
- [x] Reuse shared parsing and exact destination spans for writes; preserve link path, query, label, title, reference definitions, aliases, frontmatter quoting, line endings, and unrelated bytes. Explicitly defer unsupported forms rather than rewriting by global text replacement.
- [x] Revalidate current source text and target headings immediately before applying; a changed or missing heading, stale source span, or conflicting target/fragment repair must be reported without an unsafe write.
- [x] Recheck repaired anchors and prove a second preview/application is a no-op. No heading renames, metadata updates, or document moves are part of anchor repair.

## Acceptance criteria and regression matrix

- [x] Existing hidden-file links clear from broken-target findings across find, lint, summary, and repair preview; missing hidden-file controls remain broken. Markdown document totals and discovery exclusions stay unchanged.
- [x] Exact `.gitignore` paths resolve without extension inference. Nested relative paths, root fallback, percent-encoded paths, configured case modes, out-of-vault paths, and escaping symlinks retain their documented behavior.
- [x] Disk and supported snapshot command paths agree for the fixture; test omitted hidden targets and deletion between preview and apply without creating snapshots in a real user vault.
- [x] Strict lint fails on the original numbered-heading anchor and passes after correction; the default warning, disabled rule, rule-specific filtering, and scoped-lint cases work predictably.
- [x] Same-file/cross-file anchors, duplicate slugs, Unicode/encoded fragments, HTML anchors where currently supported, templates, block references, and frontmatter options agree with existing anchor semantics.
- [x] Dry runs write nothing. Plain `links fix --apply` includes safe anchor proposals and changes only the intended fragment; `--apply-fuzzy` additionally enables eligible fuzzy file-target repairs. Ambiguous candidates and stale evidence cause visible deferral, with no mutation.
- [x] A mixed fixture with hidden-file references, a truly missing target, and a broken anchor retains both genuine findings without double counting or dropping either category.
- [x] Repeated links share target work; record a before/after timing on a representative large fixture and retain ordinary indexed-query performance within measured noise or explain any accepted regression.

## Code and documentation areas

Primary code: `crates/hyalo-core/src/discovery.rs`, `case_index.rs`, `anchor.rs`,
and `link_fix.rs`; `crates/hyalo-mdlint/src/profiles/link.rs` and the rule catalog;
CLI lint/link commands, arguments, typed output, and output filters. Regression
coverage belongs alongside existing link-resolution and anchor e2e tests.

- [x] Update command help, the command contract, and installed skill/rule guidance that currently says lint ignores anchors or repairs cannot apply them.
- [x] Update affected generated TypeScript/JSON contracts and embedded distribution copies through existing sync commands; run applicable drift checks.
- [x] Record the final exclusion policy, rule ID, repair eligibility, apply semantics, and measured dogfood results in the knowledgebase; do not mark planned behavior as implemented.

## Validation and completion

- [x] Build with `cargo build --release` before implementation dogfooding; use `target/release/hyalo` for repository knowledgebase interactions.
- [x] Run focused core and CLI regressions, including realistic synthetic reproductions of the seven hidden-link false positives and numbered-heading anchor.
- [x] Run `cargo fmt`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -q`; run applicable help/schema/package-sync checks after output or distribution changes.
- [x] Exercise the synthetic fixtures on macOS and Linux, make symlink-specific checks platform-appropriate, and record the user-approved deferral of Windows execution to the existing CI matrix.
- [x] Rebuild and dogfood against the Hyalo knowledgebase. Any optional mapl-memory follow-up is read-only and must preserve its uncommitted tidy edits; the original broken anchor is already repaired, so do not reintroduce it there for testing.
- [x] Record remaining limitations and review the final diff before proposing a PR. Complete the iteration only after behavior, tests, documentation, and relevant platform checks are verified.

## Non-goals

No Jev model, prompt, or threshold changes; no network or live API calls in tests;
no taxonomy, protected-page, orphan, configuration, or user snapshot changes;
no broad hidden-tree discovery; no speculative fuzzy fragment application; no
generalized external-link checking. This document creates the plan only and does
not authorize implementation, a commit, a push, or a PR in the planning session.

## Implementation record

Implementation was authorized separately on 2026-09-20. Work was kept uncommitted on
`iter-301/hidden-links-and-anchor-repairs`; mapl-memory was not modified or used
as a test fixture.

### Resolution and discovery policy

The complete case index covers the discovered inventory, not every possible
explicit path. A literal hidden target can now use an existence-only lookup at
each normal source-relative/root-fallback candidate. No extension is inferred
for omitted hidden files. Lookups retain containment and case policy, never read
target contents, and inspect only named parent directories when folding case.
An invocation-local cache retains up to 4,096 probe results, including misses;
ordinary indexed hits do not enter that cache or probe disk.

Hidden targets omitted by `[scan] exclude` or gitignore can resolve explicitly.
Nonhidden excluded and gitignored targets retain their existing behavior.
Omitted hidden Markdown is classified as an attachment: no document counts,
graph membership, headings, bare-name, alias or fuzzy candidates are added.
Snapshots retain the discovered inventory and use the same live hidden-path
lookup; a later invocation observes hidden target deletion.

### Anchor lint and repair contract

`HYALO008` is `broken-heading-anchor`, enabled as a warning by default and
promoted by strict mode unless severity is explicitly configured. HYALO006
remains target-only. Both share the vault-wide resolution context; heading data
is cached per target only when HYALO008 is selected. Same-file checks use the
current source bytes; indexed runs reuse snapshot outlines. Scanner exclusions,
frontmatter selection, block references and template semantics remain shared
with find. Summary now counts broken anchors even when targets are also broken.

Repair eligibility is an exact unique match after removing a leading decimal
section number, emitting the actual generated heading slug. Ordinary
`links fix --apply` applies eligible safe anchor repairs together with safe target
repairs. The user's follow-up removed the separate anchor flag; `--apply-fuzzy`
adds eligible fuzzy file-target repairs without permitting fuzzy anchor guesses.
JSON uses `anchor_fixable`/`anchor_fixes`, `anchors_applied`/`applied_anchor_fixes`,
and `anchors_deferred`/`deferred_anchor_fixes` separately from target repair
fields. `broken` and `broken_anchors` describe the pre-apply scan.

Supported single-line body Markdown links and wikilinks use exact parser spans.
Reference definitions, multiline destinations and frontmatter rewrites are
deferred, preserving their bytes. Ambiguous matches, stale source/heading evidence
and sources with conflicting target repairs are also deferred. Fragment repairs
must preserve heading text and slugs, including links appearing inside headings.
No fuzzy anchor application or metadata/heading mutation is introduced.

### Baseline and validation evidence before the apply follow-up

The preimplementation release build passed. The synthetic hidden fixture had
seven false target findings across two discovered documents in find, HYALO006,
summary and repair preview. The numbered-heading fixture reported one broken
anchor and zero repairs before implementation.

Baseline repository dogfood: 505 documents, zero schema errors/warnings, zero
broken file targets, and 23 pre-existing broken anchors across 11 documents.
Those unrelated anchors are evidence for the new lint rule, not authorized
knowledgebase repairs.

Initial validation ran on macOS. Docker's daemon was initially unavailable,
and there was no Windows execution environment. Portable tests are included and
Unix symlink tests are platform-gated. The later local Linux validation is
recorded below; no remote CI run is claimed.

The final hidden fixture reports zero false findings in all four commands and
still discovers two documents. All 505 knowledgebase files explicitly selected
with `lint --files-from` produce 23 HYALO008 warnings, exactly matching find and
repair preview. Ordinary lint honors configured ignores and checks 161 files,
reporting 19 of those warnings. No existing knowledgebase anchor was rewritten.

Performance used 1,500 synthetic documents with 18,000 ordinary links, one warmup
and eight measured interleaved baseline/new release runs per query:

| Indexed query | Before median | After median | Before range | After range |
|---|---|---|---|---|
| Summary | 55.889 ms | 56.237 ms | 54.656–59.246 ms | 54.320–58.900 ms |
| Find with links | 47.274 ms | 46.669 ms | 44.689–49.484 ms | 45.604–48.708 ms |

Changes of +0.62% and -1.28% fall inside measured noise. A 1,000-occurrence
same-line anchor test exercises shared source spans; occurrence queues avoid
rescanning previously assigned proposals. Apply revalidation reads target
headings again after source preparation and before batch publication. The shared
executor checks captured source bytes at each publication; concurrent unrelated
target edits are not a cross-file transaction.

Unknowable template anchors are visible advisory deferrals without incrementing
the broken-anchor count. Setext headings remain outside existing matching
support, but candidate writes that could change them are explicitly deferred.

Before the user changed the apply contract, the ordered `cargo fmt`, workspace
all-target clippy with warnings denied, and `cargo test --workspace -q` passed: 1,068 CLI unit tests, 2,276 CLI end-to-end
tests, 1,568 core unit tests, 256 lint tests, 70 xtask tests, one integration test,
and two executed doctests; two doctests remain ignored by the existing suite.
That release rebuild passed. Its mixed fixture verified the former anchor hint's
separate opt-in, strict lint failing before and passing after the repair,
byte-preserving CRLF output, and a second application doing nothing. Those results
validate the earlier implementation; they do not validate the revised ordinary
`--apply` behavior or its updated help and hints.

One earlier full-suite attempt overlapped the help-drift builder, which replaces
`target/debug/hyalo`; its missing-executable failures were resolved by rerunning
the full gates sequentially. A review-found heading-mutation case was corrected
and is protected by ATX/Setext regression tests. No test failure is waived.

Dogfood improvement proposed: have the help-drift gate build once and inspect an
isolated executable, so its repeated help captures cannot replace the executable
used by another validation process. This tooling follow-up is outside this
iteration's link behavior scope.

Before the apply follow-up, drift checks passed: help text, command reference, feature fanout, typed
output, generated TypeScript (46 declarations), Codex package (14 assets), Pi
package (8 files), bundled skills (14), and mutation journals (10 command
modules). Both package sync commands ran; TypeScript regeneration produced no
contract drift. All 41 exercisable shipped jq recipes passed; the existing MADR
TOC recipe cannot run against this vault because `docs/decisions` is absent.

### Apply follow-up validation

The revised ordinary `--apply` contract passed four updated focused end-to-end
tests, then `cargo fmt`, workspace all-target clippy with warnings denied, and
the full workspace test suite with the same passing counts recorded above.
Tests cover the default/explicit dry run, plain apply, apply with fuzzy target
repairs enabled, ambiguous deferrals, the ordinary apply hint, second-application
convergence and snapshot refresh.

The rebuilt release binary passed a synthetic mixed hidden-target/anchor check:
preview writes nothing; the hint offers ordinary `links fix --apply`; that command
repairs the fragment while preserving query, title and CRLF bytes; strict HYALO008
changes from one error to zero; the second application is a no-op. The removed
flag is absent from help. Repository dogfood remains at 505 documents, zero
broken targets, 23 existing broken anchors and zero schema findings. No existing
knowledgebase anchor was rewritten.

Help drift, command reference, typed output, generated TypeScript, Codex/Pi
distribution sync, and bundled-skill checks all passed again. Implementation and
macOS validation are complete. All changes remain uncommitted; no push or PR was
created, and mapl-memory remains untouched.

### Local platform validation on 2026-09-23

Starting the existing Rancher Desktop backend made Linux execution available.
The uncommitted checkout was mounted read-only into the existing ARM64
`rust:1-bookworm` image, using Rust 1.96.1 and an isolated container build
directory. All 25 focused tests passed:

- Core: `explicit_hidden` (3, including Unix symlink containment),
  `explicit_path_cache` (1), `omitted_hidden_paths` (1), and
  `anchor_fix::tests` (7).
- CLI end-to-end: `links_resolution::hidden` (2), `iteration301` (6), and
  `hyalo008` (5).

These exercise native Linux case behavior, discovery exclusions, disk/snapshot
agreement, hidden target deletion, stale repair evidence, heading preservation,
CRLF/query/title preservation, ordinary apply and idempotence, and scoped lint.
The container was removed after execution. The local output is retained at
`/tmp/hyalo-301-linux-validation.log`.

The installed macOS executable and validated release executable have identical
SHA-256 hashes. Strict lint of this iteration document passes without findings.

Local Windows runtime execution remains unavailable. An attempted workspace/all-target
check for the installed `x86_64-pc-windows-msvc` Rust target stopped in the
existing `alloca` dependency because Windows C headers (`malloc.h`) are absent;
this is not a successful Windows build or test. The existing CI test matrix in
`.github/workflows/ci.yml` includes all 301 tests on Ubuntu, macOS, and Windows,
but could not run the then-uncommitted tree without publishing it. A read-only
portability review found no concrete defect in the new fixtures; it does not
substitute for execution.

On 2026-09-23, the user explicitly approved closing iteration 301 with Windows
validation deferred to the existing CI matrix. The platform task is closed by
that disposition; Windows execution had not passed or run at local closure. Iteration 301 is
completed with implementation, documentation, macOS validation, and focused
Linux validation finished. The installed binary includes the completed changes.
At local closure, nothing had been committed or pushed and no PR had been
created. The user subsequently invoked `create-pr` and `review-pr`, authorizing
scoped commits, publication, and review of iteration 301. Windows validation is
to be checked on the published PR head. This authorization does not include
merging. Mapl-memory's uncommitted changes remain untouched.

### Published validation and review

PR #359 was published against iteration 300 at `c09cb55a5e68`. Its full workspace
tests passed on Linux, macOS, and Windows; the Windows run is
[recorded in CI](https://github.com/ractive/hyalo/actions/runs/35887046211/job/107269741596).
This closes the earlier Windows execution deferral for that revision. Review
repairs require validation on their own final head.

The independent full review covered `4818b193a322..c09cb55a5e68` in an enforced
read-only process and returned two medium findings: hidden-path precedence in
anchor lint/attachment classification, and anchor writes bypassing the shared
bulk executor. Both were verified. Copilot was requested for `c09cb55a5e68` but
returned no review within the five-minute wait; that is incomplete coverage,
not a successful empty review.

Both findings were repaired in one batch. Anchor lint and find now use the
actual resolved target's discovery membership, preserving a source-relative
hidden attachment over a discovered root fallback. Its regression spans
disk/snapshot and full/file/glob scopes while retaining checks for discovered
targets outside the selected source scope.

Anchor repairs now prepare source edits, freshly validate target headings,
and publish through one shared executor batch. More than eight source files
retain the existing bulk durability policy and parallel writes. A nine-file
batch regression checks stale source/heading/missing-target deferrals and a
late source conflict, preserving successful per-file effects and unchanged
bytes in rejected files.

The consolidated repair batch passed `cargo fmt`, workspace/all-target Clippy
with warnings denied, and the full workspace test suite in that order. Focused
validation also passed the new six-mode resolution regression, five HYALO008
end-to-end cases, two link-context unit tests, and eight anchor-repair unit tests.
