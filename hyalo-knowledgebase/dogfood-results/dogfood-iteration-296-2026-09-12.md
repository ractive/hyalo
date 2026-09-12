---
type: research
title: Iteration 296 — review follow-up verification
date: 2026-09-12
status: active
---

# Iteration 296 — review follow-up verification

Local verification of [[iterations/iteration-296-review-followups]] on
12 September 2026. The original baseline and findings remain unchanged in
[[reviews/codebase-review-2026-09-12]] and
[[dogfood-results/dogfood-v0230-2026-09-12]].

## Finding dispositions

| Finding | Local disposition and evidence |
| --- | --- |
| F1 — managed-marker content loss | Fixed. Shared document/body syntax identifies real standalone markers. Malformed pairs refuse safely; inline, fenced, indented and CRLF controls preserve surrounding bytes. Second apply is unchanged. OKF retains its existing malformed-marker policy. |
| F2 — shared Pi configuration loss | Fixed. Init merges exact required registrations and records additions; runtime module metadata is confined to `.pi/lib/package.json`. Deinit preserves pre-existing entries and user edits. Receipt-less legacy manifests remain with a diagnostic. Real Node tests load the runtime and preserve user CommonJS execution. |
| F3 — broken emitted destinations | Fixed. Move, fix and auto-link share syntax-aware encoding. Move preflight parses and resolves emitted destinations against the projected catalog; an unsafe representation refuses before writes. Special names, fragments/labels, subsequent moves, batch moves and disk/index backlinks are covered. |
| F4 — scaffold value/type corruption | Fixed. Ordered properties use the shared YAML emitter; typed defaults round-trip and placeholders remain deliberately incomplete. Complete framing/parser checks and parent-path preflight precede writes. Symlink-parent preview and apply now agree. |
| F5 — unusable fresh ranked index | Fixed. Compact scoring admission is separate from bounded token reconstruction. A fresh 14,375-file MDN index scores directly with exact disk parity and no rebuild warning. Selected reconstruction and mutation/refreshed-index refusal controls retain the resource limit. |

Independent review found two additional ownership defects during implementation:
duplicate JSON keys could disappear during rewriting, and repeated init could
fail to record newly added registrations. Both were repaired. Recursive duplicate
keys now refuse before installation or preserve ambiguous removal inputs and
receipts. Repeated init composes ownership from current user state. A fresh final
review is clean, with 28 independent bounded CLI controls.

## Final artifact and verification

All final manual reproductions used this fixed release artifact:

`/Users/james/devel/hyalo/.git/ralph-loop/run-20260912-296/bin/hyalo-repair2`

SHA-256: `ad0066666025114bc27ac8fdb521dad50d2b23ddc7db96c864f17e36948acbef`.
It was built from baseline `36745a4604d8c398303cee61027c7151c778996a` plus
the reviewed iteration changes. The run's immutable source manifests bind the
tested working tree; this is not a published release.

- Ordered `cargo fmt`, strict workspace/all-target Clippy and workspace tests
  passed: 5,168 tests, zero failures, two historical ignored doctests.
- npm typecheck/build and all 39 tests passed against the final native binary.
- All 12 supported xtask checks and generated npm metadata verification passed.
  The jq gate executed 41 recipes; its own-KB MADR recipe remains unexercisable,
  while this iteration's separate authored MADR CLI fixtures passed.
- All 66 final release dogfood commands passed their assertions, including exact
  preservation, Pi ownership, repeated init, duplicate keys, special filenames,
  typed defaults, parent-path refusal, scoped lint-hint replay, literal task
  examples, invalid output preflight, empty input, indexed set/task/move parity,
  static external symlinks and native case-collision refusal.
- Cargo check passed. The additional pedantic review command completed with
  advisories assessed during review; it was not warning-free. The ordinary strict
  gate was clean.

## MDN and own-KB spot checks

MDN remained read-only at `44a5fa2aace490e0114349d9d683675b2f5cacce`.
The explicit temporary snapshot was written outside that corpus. Creation
indexed 14,375 files with zero warnings. Indexed and disk `AbortController`
queries matched exactly: 65 total matches and the same five selected results.
The original `title~=Abort` metadata query matched all 12 results exactly.

The indexed ranked command recorded `direct_indexed_scoring: 1` and zero logical
source/body reads. These are process-local execution counters, not inferred
timings or filesystem syscall counts.

| Final-artifact operation | Wall time |
| --- | --- |
| MDN create-index | 2.821 s |
| MDN ranked query, disk / indexed | 3.919 / 0.546 s |
| MDN metadata query, disk / indexed | 0.578 / 0.307 s |
| Own-KB summary / ranked query / strict lint | 0.068 / 0.275 / 0.180 s |

These are single local samples with uncontrolled cache/background work, not
speed guarantees or a machine-independent threshold. Own-KB strict lint found
zero errors and the existing 15 advisory warnings across five historical files
(156 checked, 340 ignored at that check).

## Limits and remaining workflow

Reconstruction caps remain unchanged. Indexed mutations can still refuse a
corpus whose full reconstruction estimate exceeds the cap; the diagnostic
directs mutation without the index and subsequent snapshot recreation. A refresh
that cannot safely reconstruct postings retains metadata with an explicit disk
scoring fallback. No stronger concurrency, crash-durability or filesystem
guarantee is claimed.

This is native macOS evidence. Linux/Windows CI, PR publication and merge remain
pending authorization; iteration status therefore remains `in-progress`.
Iteration 287's external consumer remains deferred. The full 289-plan inventory
has no upcoming pending successor. Unsupported legacy xtask placeholders,
ignored tests, the full opt-in scale benchmark, and unperformed native CI are
not counted as passing coverage. No fuzz, exhaustion or race campaign ran.

Detailed commands, source/binary hashes, immutable attempts, failed checks,
reviews and final checkpoint evidence live under
`/Users/james/devel/hyalo/.git/ralph-loop/run-20260912-296`.
