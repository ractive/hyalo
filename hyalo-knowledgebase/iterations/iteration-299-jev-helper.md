---
type: iteration
title: "Iteration 299: optional Jev helper and request preparation"
date: 2026-09-20
status: in-progress
tags:
  - iteration
  - jev
branch: iter-299/jev-helper
---

# Optional Jev helper and request preparation

## Problem and intended behavior

Jev classification needs a reliable reusable client before the tidy agent can use
it. The first research used a personal Python helper, which the user wants to
avoid. Build a Bun-based TypeScript runtime that prepares bounded evidence through
Hyalo, makes explicitly authorized Jev requests, and returns typed suggestions.
Ship a portable JavaScript bundle that also runs with Node.

Design: [[research/jev-skill-runtime-options-2026-09-20]]. Classification findings:
[[research/jev-opt-in-classification-2026-09-20]]. Follow-up integration:
planned as `iterations/iteration-300-jev-tidy-integration.md` in the next branch.

## Scope and ownership

Own the new `npm/jev/` private source package: `package.json`, committed `bun.lock`,
TypeScript configuration and package scripts, `src/cli.ts`, `src/prepare.ts`, `src/protocol.ts`,
`src/provider.ts`, `src/decisions.ts`, tests, and build script. Use the user's
requested [Bun quickstart](https://bun.sh/docs/quickstart) to bootstrap the real
package; the completed `/tmp/.../bun-bootstrap` experiment is only a reference.
Do not replace existing repository instructions with generated template guidance.

Keep the SDK and all non-builtin runtime imports inside `dist/jev.mjs`. Pin
`@typesafe-ai/sdk@0.6.0`; pin Bun 1.4.2 for reproducibility initially. Preserve the
existing `npm/hyalo` package-manager contract. No skill activation, new native CLI
command, mutation engine, persistent consent store, or implicit runtime install.

## Tasks

- [x] Bootstrap the Bun package, narrow its generated files to the actual CLI deliverable, pin dependencies, and commit the lockfile.
- [x] Implement a versioned manifest/policy schema with local metadata separated from the explicit API payload; validate finite sizes/counts and known keys.
- [x] Implement local-only `prepare` with explicit file selection and described type/folder/tag candidates; query the resolved Hyalo binary using read-only argument arrays.
- [x] Bound selection, child output and time; preserve partial-selection and oversized-document outcomes instead of claiming complete inspection.
- [x] Respect schema bindings, valid existing properties and missing-only scope; infer deterministic type-to-folder mappings locally.
- [x] Preserve complete small-document evidence or explicitly selected sections, current values, and evidence/policy fingerprints; do not claim fingerprints provide atomic write protection.
- [x] Implement `check` and bounded stdin/file input; both preparation/check paths make zero network calls and require no key.
- [x] Implement `ask --allow-network` using the pinned SDK with explicit endpoint/model/log options, overall deadline, bounded response stream and redirect refusal.
- [x] Configure one retry for 429/529 only, respecting the overall deadline; no nested retries or fallback external provider.
- [x] Validate returned model, IDs, types, options, probability ranges/sums/winner and usage independently of TypeScript declarations.
- [x] Normalize suggestions and deferrals with conservative thresholds, stable statuses, usage, elapsed time and unknown-usage reporting on ambiguous transport failures.
- [x] Define exit 0 for valid results including deferrals, exit 1 for unavailability, and exit 2 for invalid input/protocol; retain successful independent batches when another fails.
- [x] Bundle portable ESM with Bun's Node target and an embedded MIT notice; keep entrypoint execution compatible with both runtime argument conventions.
- [x] Add meaningful Bun tests for protocol, transport, preparation, scope and no-write behavior; keep all ordinary tests offline.
- [x] Add artifact smoke tests under Bun and Node, including an unrelated CWD and no node_modules, and verify a separate TypeScript check.
- [x] Run required Rust gates before committing, helper typecheck/tests/build, and release-build Hyalo dogfood for the actual read commands.

## Acceptance criteria

- [x] Ordinary construction, prepare/check, and ask without the network flag issue zero requests even with a key present.
- [x] Explicitly configured endpoint/model/logging ignore conflicting TypeSafe environment defaults; credential content never appears in logs, argv, manifests, or emitted errors.
- [x] Invalid/unknown answers cannot become suggestions; unknown and low-confidence results remain visible.
- [x] Oversized and slow responses, cancellation during retry wait, redirects, 401/422, and 429/529 have bounded, tested behavior.
- [x] No operation writes notes, indexes, config, caches, or implicit temporary files; only the caller's chosen stdout redirection can create artifacts.
- [x] Paths, current values, and local diagnostic metadata remain outside the transmitted payload; document text is treated as data.
- [x] Prepared evidence is acquired through bounded Hyalo reads without loading full documents into the main agent's context.
- [ ] The same built helper works under Bun 1.4.2 and supported Node versions; platform-specific process and stdin behavior is checked on Linux, macOS, and Windows.
- [x] No Python, runtime package download, `bunx`/`npx`, or source-checkout dependency is required to execute the artifact.

## Validation approach

Mock transport tests cover real boundary behavior: malformed 200 JSON, missing and
extra IDs, unexpected models, false probability distributions, body limits,
redirects, failure after dispatch, authentication, throttling, and total deadlines.
Scratch vaults cover unusual filenames, outside-scope entries, missing fields,
bound schemas, and oversized output. Assert bytes and request counts, not prose
wording. Use controlled local HTTP fixtures for stream and abort behavior under
both runtimes in addition to SDK fetch mocks.

The initial limits are 25 selected documents, 24,000 request bytes, 48 questions,
1 MiB response bytes and a 12-second total request budget. Validate these as explicit
limits without treating bytes as tokens. Do not tune thresholds to a test failure.

Run `cargo fmt`, strict workspace Clippy, and workspace tests in repository order
before a commit/PR. Run `bun install --frozen-lockfile`, `bun test`, a separate
`bun run typecheck` package script invoking `tsc --noEmit`, and `bun build` for the helper. Live smoke calls are manual and
explicitly scoped; they do not belong in CI.

## Deferred work

Application transactions, source-content compare-and-swap, durable caches,
background classification, schema edits, status inference, new folders, generalized
provider plugins, and semantic search are outside this iteration. The helper
does not expose an apply operation.

## Implementation evidence

The Bun quickstart bootstrap was applied to `npm/jev`, then narrowed to the
portable CLI. The helper uses the pinned SDK for requests, authentication and
retries; Hyalo-specific manifests, policy and runtime validation sit above it.
One request groups the independent questions for one selected document. Files
larger than 18,000 bytes need an explicit section; selected source files over
1 MiB defer even with a section. No document is silently truncated.

Local validation on macOS arm64: Bun 1.4.2 and Node 24.19.0; nine offline tests
with 76 assertions, including bounded real HTTP streams under both runtimes,
standalone bundled execution, path-bound/exempt notes and unchanged vault bytes.
Type checking, bundle rebuild comparison, frozen lockfile install, release build,
formatting, strict workspace Clippy and all workspace tests passed. A CI matrix
now covers Linux, macOS and Windows with Node 22; those remote runs are pending.

The tests caught a large `Retry-After` overflowing JavaScript timers into an
immediate retry. Long delays now remain beyond the total deadline, preventing
early retries. Hyalo's public lint JSON omits its internal missing-type identifier;
the helper requires the exact positive missing-type diagnostic from Hyalo 0.24.
An unfamiliar diagnostic safely defers. A stable public violation kind would
remove that compatibility limitation.

One explicitly authorized live call on a synthetic scratch note returned a
research suggestion and the local research-folder mapping in 657 ms, with 400
input tokens and 41 output tokens (estimated USD 0.0000168). No notes were changed.

Before publication, the helper also received the integration's credential-evidence
backstop, own-property folder lookup, and empty-body Retry-After normalization.
The refreshed standalone suite passes ten tests with 79 assertions. These fixes
belong in the helper's own PR; installation-specific coverage follows in iteration
300. Forward references to its not-yet-present plan remain plain paths so this
branch independently passes strict knowledgebase lint.

## PR review follow-up

PR 356's independent review returned four findings: filesystem-aware exclusions,
existing tag semantics, normalized type mappings and aggregate manifest size.
Copilot review 5260484628 returned three inline findings; its two type-mapping
comments duplicate the independent type finding. Its overview also mentions a
retained-result concern: a failed tag request could discard a known local folder
mapping. A regression confirmed that case and it is included in the repair.

Five verified defects are corrected. Exclusions compare filesystem identities
without lowercasing distinct paths; scalar/ASCII-case-variant tags avoid redundant
requests; type wikilinks and single-item lists map without editing metadata;
oversized manifests defer excess documents; local filing suggestions survive
unrelated provider failure. Fifteen offline tests pass with 110 assertions.

Copilot's remaining inline claim, that optional `init` breaks TypeScript checking,
is not a defect: the `insist` assertion on `init?.method` narrows it before signal
access. The pinned TypeScript check passes locally and in the three-platform CI.
The independent follow-up review also covers that disputed assertion contract.

All 13 active CI checks passed on the initial published head `dc7d079d`, including
Bun and Node 22 on Linux, macOS and Windows. The repair batch requires its own
review and platform results before completion.
