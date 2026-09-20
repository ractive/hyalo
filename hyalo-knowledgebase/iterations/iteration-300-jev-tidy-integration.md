---
type: iteration
title: "Iteration 300: opt-in Jev assistance for hyalo-tidy"
date: 2026-09-20
status: in-progress
tags:
  - iteration
  - jev
branch: iter-300/jev-tidy-integration
depends-on: iteration-299-jev-helper
---

# Opt-in Jev assistance for hyalo-tidy

## Problem and intended behavior

Expose the optional helper from [[iterations/iteration-299-jev-helper]] through
`hyalo-tidy` across Codex, Claude and pi. A normal tidy behaves as before and never
starts a JavaScript runtime or sends documents to Jev. An explicit Jev request
enables scoped suggestions for missing types, approved tags, and existing folders.
Audit/repair authorization remains the user's original request.

The design and Bun bootstrap evidence are in
[[research/jev-skill-runtime-options-2026-09-20]]. This iteration depends on the
tested helper contract and portable artifact from iteration 299.

## Scope and ownership

Own optional guidance at
`plugins/hyalo/skills/hyalo-tidy/references/jev.md`, the Codex entrypoint change,
the Claude tidy template and pi tidy entrypoint changes, plus generated reference
and script assets. Update the relevant Rust initializer, ownership manifests,
package synchronization/gates, and integration tests. Keep provider/client logic
in the iteration-299 source package rather than duplicating it in each platform.

Concrete affected areas:

- `plugins/hyalo/skills/hyalo-tidy/` and embedded Codex copies.
- `crates/hyalo-cli/templates/skill-hyalo-tidy.md` plus auxiliary Claude resources.
- `pi-package/skills/hyalo-tidy/` and embedded pi copies.
- `crates/hyalo-cli/src/commands/init.rs`, `init/codex.rs`, and `init/pi_manifest.rs`.
- Relevant `crates/xtask/src/` package/sync checks and `justfile` synchronization commands.

## Tasks

- [x] Add a short optional Jev branch to each tidy entrypoint, linking the shared detailed reference only when explicitly selected.
- [x] Preserve platform frontmatter and existing invocation policies; no second classification skill and no automatic pi hook.
- [x] Establish explicit document/field scope and described local categories before preparing requests; neither API-key presence nor repo config enables the mode.
- [x] Prefer Bun with `--no-install --no-env-file`, support the same bundle through Node, and report unavailable prerequisites without installing them.
- [x] Keep local preparation before main-agent full-document inspection so the helper can actually reduce context loads.
- [x] Normalize audit behavior across the optional paths: no index/cache/config writes and no repairs for a read-only request.
- [x] Document helper exits and partial results; report all deferrals/unavailable batches while continuing independent deterministic findings.
- [x] Require current-evidence review before using a suggestion; preserve valid values and defer candidates missing required fields/sections under the proposed effective schema.
- [x] Preview approved metadata/move commands with Hyalo, inspect destination bindings and link rewrite effects, then apply only within existing repair authorization.
- [x] Add shared auxiliary-asset declarations for reference and helper installation, including correct JavaScript ownership markers or exact content receipts.
- [x] Extend Codex install/preflight/remove beyond SKILL.md and openai.yaml while preserving conflicts, symlink guards, and user-owned files.
- [x] Extend pi's exact artifact allowlist/receipts and sync checks for both resources, including upgrade, modified-file preservation, and removal.
- [x] Add Claude auxiliary-resource preflight/install/removal without an unrelated installer refactor.
- [x] Add reproducible bundle/reference drift checks and keep every embedded Cargo asset inside the CLI crate.
- [x] Exercise fresh init, repeated init, upgrade and deinit for all platform layouts, including installation conflicts and edited helper files.
- [x] Run package-isolated helper smoke tests and realistic skill scenarios on scratch vaults; keep live API evaluation explicitly invoked.
- [x] Run Rust gates in repository order and all applicable package/skill/sync checks; document actual outcomes and unresolved limits.

## Acceptance criteria

- [x] Plain tidy with TYPESAFE_API_KEY present makes zero Jev requests and never invokes Bun/Node.
- [x] An explicitly enabled audit can return suggestions while leaving vault/index/config bytes unchanged.
- [x] Ordinary tidy remains usable with no JavaScript runtime or API key; optional failure is visible rather than empty success.
- [x] Explicit Jev repair preserves unrelated metadata and files, uses Hyalo's mutation commands, and explains the evidence for changes.
- [x] User opt-in is scoped and reused for the session; no per-request approval loop and no automatic persistent enablement.
- [x] A local type convention such as dogfooding-as-research is represented in policy and cannot be silently overridden by confidence alone.
- [x] Source/policy changes, malformed evidence, missing required fields, destination collisions and ambiguous links prevent the corresponding proposed repair.
- [x] Generated resources work after plugin/local/CLI installation with Bun or Node and no node_modules, independent of the source checkout or caller CWD.
- [x] New helper/reference resources obey exact-content ownership on update/removal; edited or unowned resources are preserved. Existing SKILL.md ownership semantics remain unchanged.
- [x] Every distribution contains identical helper bytes and intended reference content; Cargo packaging has no out-of-crate include dependency.
- [x] Final reports separate local findings, externally classified files, accepted suggestions, deferrals, changes and unavailability, with actual usage where known.

## Behavioral evaluation

Use fresh scratch fixtures with realistic requests: audit only; tidy without Jev;
explicit inbox classification; approved missing-metadata repair; a valid but unusual
existing type; a folder collision; a path-bound schema; missing status required by
a suggested type; no runtime/key; SDK failure after one successful batch; injected
instructions; a non-English note; and stale evidence before repair.

Reuse deterministic failure fixtures from iteration 299. Keep real-label ambiguity
and prior rubric-tuning examples separate from held-out semantic evaluation. Record
accepted precision, deferral coverage, request cost, and actual main-agent reads
avoided. Neither SDK typing nor the small research sample proves correctness.

When an independent forward-test is authorized, give the evaluator the installed
skill and realistic request without expected answers or the author's assumptions;
keep all mutations in its scratch vault. Otherwise perform the same concrete
behavioral checks locally and state that limitation.

## Validation and completion

Run formatting, strict Clippy and workspace tests before commit/PR. Check Codex
package sync, pi package sync, bundled skills, helper bundle drift, standalone
Cargo packaging/embedding, helper type checking, Bun tests, and supported-runtime
smoke tests. Live requests stay out of ordinary CI.

The feature is complete only after all three installed skill layouts have the
runtime and reference resources, ordinary tidy is verified network-free, and the
scoped audit/repair scenarios pass. No implementation task is marked complete by
the planning or bootstrap experiments.

## Implementation and local validation

Implemented the optional branch in all three entrypoints. Every distribution
contains the same bundled SDK/helper and shared reference. Codex and Claude use
bounded exact-content receipts for these new resources; pi extends its existing
receipt. Existing entrypoint ownership behavior was deliberately left unchanged.
Old managed entrypoints therefore retain their pre-existing update/removal rules.

Added `sync-jev-assets` / `check-jev-assets` and recursive Pi resource sync.
The Cargo package verified successfully in isolation, including embedded assets.
All 14 bundled skills passed conformance, all Codex/pi assets passed drift checks,
and five new Rust integration tests covered fresh/repeated/legacy installs,
receipt-owned upgrades, user edits, unowned assets, symlinks and deinit for all
three layouts. Eleven Bun tests (94 assertions) include Bun/Node execution of the
actually installed bundles outside the checkout. Type checking remains a separate
`bun run typecheck` step. No Python or runtime package install is required.

The scratch workflow used four copies of a synthetic experiment body with
different local repair conditions. Four API requests returned research suggestions
in 1,484 ms total request time, using 1,672 input tokens and 164 output tokens
(estimated USD 0.000070224). The audit preserved vault/config bytes. During repair,
one candidate was deferred for a missing required status, a destination collision
was refused by `hyalo mv --dry-run`, and a changed body produced a different
evidence fingerprint. Only the ready note received `type: research` and a move
through Hyalo, after previews. Its existing title/status were preserved and the
changed file passed strict lint; the vault had no broken links. A subsequent
ordinary local audit used Hyalo alone.

Follow-up checks verified that ordinary audit commands also work with an empty
PATH (no Bun/Node discovery), both with a key present and absent, leaving vault
and configuration bytes unchanged. An ambiguous basename link appeared in the
move preview's `skipped_ambiguous` list; that move was deferred. Editing the policy
after classification made `check` exit 2 with a fingerprint mismatch. The final
format, strict workspace Clippy and full workspace test sequence passed.

These identical semantic inputs tested workflow boundaries, not independent
classification accuracy. Tests were performed locally, without an independent
agent evaluation. A fresh multilingual/adversarial semantic evaluation and remote
Linux/Windows execution remain unverified; the added CI matrix covers Linux,
macOS and Windows using Bun 1.4.2 and Node 22. No measured agent-context savings
or calibrated precision is claimed. Both iterations remain in progress pending
their remote-platform acceptance checks; the implementation is ready for review.

Dogfood findings: the public lint JSON lacks a stable missing-type identifier,
so the helper requires Hyalo 0.24's exact positive diagnostic and otherwise defers.
`set --validate` still validates assigned properties rather than the entire
resulting document; the skill explicitly reviews complete schema requirements
before applying a type suggestion. These are compatibility limits, not reasons
to infer missing status values or override path bindings.
