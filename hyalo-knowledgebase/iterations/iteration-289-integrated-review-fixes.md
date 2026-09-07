---
type: iteration
title: Iteration 289 — Integrated review fixes before API dogfooding
date: 2026-09-07
status: completed
tags:
  - iteration
  - review
  - security
  - npm
  - api
branch: iter-289/integrated-review-fixes
priority: 1
---

# Iteration 289 — Integrated review fixes before API dogfooding

## Goal

Resolve the four verified findings from the independent review of iterations
284, 282, 283, 285, 286, and 287. Preserve vault containment during ranked search,
make indexed scoring and snippets agree on language, expose successful API
diagnostics, and keep non-publishing release validation independent of registry
publication eligibility. Deliver one reviewed Hyalo PR with meaningful regressions.

## Baseline and scope

Reviewed baseline: `5f3a91289843d6a97ab822f9dca563abf4ec85d0` through merged main
`a2654eee130875dfe4c252e006d52bb6debff826`. PRs #332–#342 contain the work; #342 is
the typed API merge. Reconcile against actual main when starting this iteration.

The prerequisite is the merged Hyalo implementation from
[[iterations/iteration-283-ranked-search-snippets]],
[[iterations/iteration-285-typed-output-structs]],
[[iterations/iteration-286-npm-distribution]], and
[[iterations/iteration-287-typed-typescript-api]]. Iteration 287 remains in progress
only because its external consumer task is deferred; that task does not block 289.

Homefinder inspection and local-package dogfooding belong to another agent in
that project. Do not edit that repository or claim its acceptance here. Leave
[[iterations/iteration-288-codex-integration]] desktop verification deferred.
Do not publish packages, bump versions, dispatch release/bootstrap workflows,
change credentials/trusted publishers, or modify the shared release-workflows
repository. Public npm 0.22.0 remains immutable and CLI-only; the typed API is
merged source awaiting a separately authorized release.

Local review evidence is under `.git/reviews/integrated-284-287-20260907/`:
`review.md`, `review.json`, the three reviewer logs, `reproduce-core.py`,
`core-probe-results.json`, and `api-pi-verification.json`. These are optional
supporting artifacts, not committed dependencies. All reproduction requirements
are restated below so a fresh checkout can implement this plan.

## Tasks

- [x] TASK-1 / P1: enforce vault containment before ranked-search live reads.
  - Entry points: `crates/hyalo-cli/src/commands/find/mod.rs` snippet pass near
    line 1492 and `crates/hyalo-core/src/bm25.rs::SnippetQuery::snippets`.
    The former joins a cached relative path; the latter directly opens it.
  - Reproduction: create a temporary vault containing `notes/note.md` with body
    `pineapple safe fixture`; create its index. Replace the note with a symlink
    to a synthetic file outside the vault containing
    `pineapple EXTERNAL_FIXTURE_MARKER`. Whole-vault `find pineapple --index`
    currently returns the external marker with exit 0. Replacing the `notes`
    directory with a symlink reproduces the same defect.
  - Reuse the canonical containment/error conventions used by
    `resolve_file_user` and `read`; verify the current resolved destination
    before opening content, including selected results from a persisted index.
    A staleness warning or lexical path check is insufficient. Fail closed with
    a useful path diagnostic; never read the external fixture to decide whether
    to exclude it. Preserve unrelated I/O failures and existing vault policy.
  - Cover file and parent-directory substitutions, valid in-vault destinations,
    and missing/unreadable targets. Exercise both persisted and affected fallback
    live-read paths. Keep the final-result filter/limit optimization, including
    zero-limit behavior. Do not broaden this into a filesystem redesign or claim
    elimination of every concurrent filesystem race.

- [x] TASK-2 / P2: align scoring and snippet tokenization under language overrides.
  - Entry points: the persisted BM25 fast path and token-reuse fallback in
    `commands/find/mod.rs` near lines 511–590, plus the snippet language choice
    near line 1482. `IndexEntry::bm25_language` records cached document language.
  - Reproduction: index an English-default note with body `running` and no
    frontmatter language. `find run --index --language english` returns a scored
    hit with a `running` snippet. With `--language german`, the same cached hit
    has an empty `matches` array because scoring uses English document tokens
    while snippet extraction uses German document tokens.
  - Preserve the documented frontmatter/CLI/config/default language precedence
    and the distinction between query language and document language. Prefer
    bypassing the persisted fast path when cached document languages disagree
    with effective languages, matching the existing per-entry token-cache check.
    Use compatible cached tokens without additional body reads when possible.
    Never make scoring and snippets agree by silently ignoring an override.
  - Add persisted-index versus live-scan regressions for CLI and config overrides,
    frontmatter language precedence, and section-filter fallback. In the example,
    indexed German behavior must agree with a fresh German scan. Preserve scores
    and order when languages are compatible; retain legitimate empty snippets
    for title-only and cross-line phrase matches. Any added fallback reads must
    obey TASK-1's containment policy.

- [x] TASK-3 / P2: expose successful typed-call diagnostics without changing the
      Rust JSON envelope.
  - Entry points: `npm/hyalo/src/api.ts::parseEnvelope`, `jsonCall`, and
    `ExecutionOptions`; Pi typed-tool rendering in `pi-package/extensions/hyalo.ts`.
  - Reproduction: a transport returns code 0, stdout
    `{"results":[],"total":0,"hints":[]}`, and a stale-index warning on stderr.
    `find({ index: true, quiet: false, transport })` currently drops the warning;
    `raw()` preserves it. The CLI's real stale-index path warns but serves results.
  - Add a documented optional diagnostics callback to the call options. Deliver
    the original successful stderr once through that callback; when absent,
    forward it to the caller's stderr so the default does not silently suppress
    operational warnings. Empty stderr produces no notification. Do not add
    diagnostics fields to generated Rust wire types or change typed result shapes.
  - Keep CLI `quiet` semantics: suppress only what the CLI suppresses, including
    its existing exceptions. Preserve error objects/raw streams and avoid
    double-reporting failed-command diagnostics. Specify callback exception
    behavior explicitly; do not silently swallow consumer failures.
  - Pass a callback through Pi's typed calls and include collected warnings in
    their visible tool response without corrupting structured results. Cover
    native/default output, callback delivery, quiet behavior, and Pi transport.
    Include a real stale-index or malformed-config success case in addition to
    deterministic injected-transport tests; avoid timing-dependent sleeps by
    controlling fixture timestamps if needed.
  - Update API documentation and public option exports. Regenerate Pi JS and
    declarations through the existing build, sync the embedded mirrors, and
    verify both source-package and offline-init layouts. Do not hand-edit bundles.

- [x] TASK-4 / P2: separate offline packaging validation from registry planning.
  - Entry points: `.github/workflows/release.yml`, currently the
    `Pack and dry-run all publications` step near line 232, and
    `crates/xtask/src/npm_package.rs` publication planner tests.
  - Reproduction by control flow: default `workflow_dispatch`, with all three
    publish/bootstrap flags false, still calls `plan-npm-publication`. That
    planner queries the registry and rejects an existing version whose tarball
    integrity differs. Current source 0.22.0 has API contents that public
    CLI-only 0.22.0 lacks, so a non-publishing dry run can fail on immutability.
  - Move registry planning into a distinct step gated on an actual release event
    or explicit `publish_npm`. Keep pack/content checks and offline dry-run
    validation active independently. Preserve explicit bootstrap packing/signing
    behavior and ensure every real publish path obtains a valid plan first.
  - Retain immutable-version rejection, exact package order, identical-version
    skip behavior, main-package-last publication, and version/source safeguards.
    Do not change planner semantics to ignore an integrity mismatch.
  - Validate the actual workflow conditions for default dispatch, publication,
    both bootstrap modes, and release events using the repository's tooling and
    a small meaningful condition/fixture check. Do not add a test that merely
    searches for an exact YAML line. Run actionlint if available and retain the
    planner's existing simulated-registry tests. No live publish, auth, or
    all-platform release rebuild is needed to verify the gating correction.

- [x] TASK-5: complete integrated verification, documentation, fresh review, and
      plan reconciliation; record the final PR and actual check results.
  - Add focused regressions to the existing Rust BM25/e2e and npm/Pi suites.
    Symlink tests must run on capable platforms; retain any justified Windows
    capability skip explicitly and keep all other Windows tests active.
  - Before an implementation commit, run in order: `cargo fmt`,
    `cargo clippy --workspace --all-targets -- -D warnings`, then
    `cargo test --workspace -q`. Discover xtask commands from its current help
    and run every implemented `check-*` gate with the actual base/arguments.
    Report existing stubs or unavailable checks honestly.
  - Run npm typecheck, build, and tests in `npm/hyalo`, including packed ESM/CJS
    consumer coverage. Run npm metadata freshness and Pi sync/runtime freshness.
    Build the release CLI after the final embedded assets are generated, then
    run `./pi-extension-e2e.sh` against that binary. Serialize builds and tests
    that replace/use the CLI; never overlap them. Reuse unaffected passing gates
    after narrow repairs, rerunning affected gates and mandated sequences.
  - Use Hyalo for knowledgebase operations and lint changed knowledgebase files
    with `--strict`; run `git diff --check`. Documentation-only changes require
    only those checks, with no Rust suite or release rebuild.
  - Obtain a fresh independent read-only review of the full iteration diff.
    Give every finding a verified fix or evidence-backed disposition; re-review
    repaired behavior. Reconcile all upcoming plans without executing them.
    Keep 287's consumer task and 288's desktop task unfinished.
  - In authorized PR/remote mode, use scoped commits/pushes, the installed
    create-pr/review-pr/merge-pr workflows, exact-final-head CI on Linux/macOS/
    Windows, and a GitHub merge commit. No force push, squash, protection bypass,
    publication, or unrelated repository changes. Verify merge ancestry/tested
    tree and clean up only the verified merged feature branch.

## Acceptance criteria

- [x] Neither file nor directory symlink replacement can disclose outside-vault
      content through ranked snippets; failures identify the affected path.
- [x] Indexed and live language semantics agree under CLI/config overrides and
      frontmatter precedence; valid empty-snippet cases remain supported.
- [x] Successful typed API warnings are observable by default and through a
      callback, and Pi displays them without altering the typed envelope.
- [x] Non-publishing packaging validation never invokes registry publication
      planning; actual publishing retains all immutable-version safeguards.
- [x] All four review findings have regression evidence, required gates and
      final-head CI pass, independent review is resolved, and plans remain honest.

## Fresh-session handoff

Read this plan and repository guidance first. This is a new single-iteration
run, not a resumption of the completed 284–287 batch or its released lock.
Treat this planning file as intentional work if still uncommitted; preserve it
and include it in the scoped iteration branch. Do not rerun npm authentication
or bootstrap. Verify main and dependencies, then implement tasks in the order
above on `iter-289/integrated-review-fixes`.

For Ralph execution, use a fresh implementer and independent read-only reviewer,
with run evidence outside product files. The user previously preferred a cheaper
model for mechanical implementation; use that preference where supported while
preserving the configured review model. Start only 289 when explicitly invoked.
The original planning request did not execute this iteration; the later Ralph
invocation authorized the remote run recorded below.

## Local verification and reconciliation — 2026-09-07

The authorized remote run implements TASK-1 through TASK-4. Canonical containment
checks cover both snippet and fallback body reads. Persisted scoring checks the
actual stored corpus's document languages; new snapshots retain language metadata,
while older snapshots safely fall back until rebuilt. Mixed-language fallback
recovers only compatible cached documents and consumes the recovered token vectors.

Successful typed diagnostics reach `onDiagnostics` or stderr by default, with
callback failures propagated unchanged and Pi warnings displayed separately.
Release packaging stays offline until an actual publication path requires its
immutable registry plan. No package was published and no version changed.

Final local verification passed ordered `cargo fmt`, strict workspace Clippy, and
4,951 Rust tests, with two existing ignored doctests. All 11 implemented xtask
checks passed; dead-primitives and TODO-annotations remain unimplemented stubs.
The jq gate exercised 42 recipes and retained its existing unavailable MADR TOC
fixture because this vault has no `docs/decisions` directory. Actionlint, workflow
condition fixtures, simulated-registry planner tests, metadata and Pi freshness
checks, and strict changed-document lint passed.

The final release CLI passed npm typechecking, nine launcher tests, packed ESM/CJS
consumer coverage, source/offline Pi warning-layout tests, and all 21 API tests.
Live Pi 0.84.4 passed source/offline loading, installed-type compatibility, forced
generic and typed tool calls, and the post-write lint guardrail.

Fresh independent read-only review found unnecessary full-corpus token recovery.
Selective reconstruction and its core regressions resolved that finding; a second
fresh full review returned no actionable findings. Separate attempt reports and
frozen inputs remain under `.git/ralph-loop/run-20260907T205007Z-289/`.

All 282 iteration plans were inventoried after the final implementation. There
are no upcoming plans after 289. The remaining plans 287 and 288 were reread and
need no changes: their external consumer and desktop tasks remain unfinished,
and their package identity, wire shapes, and companion-asset paths remain valid.
The subsequent PR verification below completes the implementation and acceptance
evidence; the remote checkpoint remains subject to final-head checks.

## PR verification — 2026-09-07

[PR #343](https://github.com/ractive/hyalo/pull/343) contains the reviewed source at
`3b220a7a8c66927681f7675f4afc3d42fb33c4a7`. A fresh restricted read-only review of
that exact head found no actionable findings. All 11 applicable CI checks passed:
formatting, Clippy, Rust tests on Linux/macOS/Windows, quality gates, knowledgebase
lint, npm/API tests on Linux/macOS/Windows, and npm packaging. The full-vault lint
job is push-only and was correctly skipped on the PR.

This final plan-status update changes documentation only. Its strict lint and
diff checks must pass, and the supervisor must verify CI and review coverage at
the resulting final head before merging through GitHub. The merge SHA and final
checks are retained in the run ledger rather than predicted here. Iterations
287 and 288 retain their unfinished external tasks.
