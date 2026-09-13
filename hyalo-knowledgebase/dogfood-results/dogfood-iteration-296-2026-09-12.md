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

## Local checkpoint artifact and verification

The 12 September manual reproductions used this fixed release artifact:

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

## MDN and own-KB spot checks — 12 September

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

## Local checkpoint limits and workflow

Reconstruction caps remain unchanged. Indexed mutations can still refuse a
corpus whose full reconstruction estimate exceeds the cap; the diagnostic
directs mutation without the index and subsequent snapshot recreation. A refresh
that cannot safely reconstruct postings retains metadata with an explicit disk
scoring fallback. No stronger concurrency, crash-durability or filesystem
guarantee is claimed.

At the 12 September local checkpoint, this was native macOS evidence.
Linux/Windows CI, PR publication and merge awaited authorization, so the plan
remained `in-progress`. The 13 September verification below supersedes that
workflow status while preserving the original evidence.
Iteration 287's external consumer remains deferred. The full 289-plan inventory
has no upcoming pending successor. Unsupported legacy xtask placeholders,
ignored tests, the full opt-in scale benchmark, and unperformed native CI are
not counted as passing coverage. No fuzz, exhaustion or race campaign ran.

Detailed commands, source/binary hashes, immutable attempts, failed checks,
reviews and final checkpoint evidence live under
`/Users/james/devel/hyalo/.git/ralph-loop/run-20260912-296`.

## PR verification — 2026-09-13

PR [353](https://github.com/ractive/hyalo/pull/353) extends the local checkpoint
with nine local-review repairs and six distinct Copilot repairs. Numeric
defaults retain accepted integer spellings such as `+1`, `007` and `-007`.
Existing Pi helpers may retain inherited ESM semantics when no nested package
manifest changes that scope. Batch rename projection now builds one borrowed
lookup instead of repeatedly scanning the rename list. Link identity validation
also reuses forward/reverse mappings, preserving first-match and ambiguous
override ordering. An initial UTF-8 BOM is excluded from shared marker syntax
classification while its original bytes and absolute splice offsets survive;
MADR and OKF LF/CRLF, literal and noninitial BOM controls pass.

Pi receipt version 3 records exact installed content for the five shipped
artifacts, with a 2 MiB aggregate content cap shared by pending and previous
bytes. Init refuses unknown or edited
collisions before publication. Deinit preserves artifacts, registrations and
the receipt when artifact ownership or manifest changes conflict. Unchanged
owned artifacts can upgrade. Manifest-only legacy receipts do not prove
artifact ownership; unknown legacy files require preservation or relocation
before init. Shared user metadata and benign post-install edits remain intact.
Surviving exact, directory, glob or uncertain local registrations retain the
whole Pi group; cleanup becomes possible after removing those user references.
A partial upgrade preserves ownership of mixed old/new files. Ordinary retry
rebases evidence to captured installed bytes and clears previous content only
after publication succeeds. Both a deterministic mixed-version collision and
a real permission-denied artifact write verify recovery. Version 1 manifest-only
and version 2 completed receipts remain recognized under their original limits.

Auto-link retains encoded-looking display aliases. Move and fix recognize the
same case-insensitive site prefix as resolution and retain its authored case.
Link fix validates emitted Markdown destinations against the complete catalog;
a short directory spelling that instead resolves to a competing file refuses
without changing bytes. Preview and apply share this decision.
Batch moves now validate only replacements accepted by canonical application;
discarded overlapping proposals cannot refuse an otherwise valid move or inflate
reported counts. Replay must match planned bytes. Markdown replacement validation
uses the same cleaned/original span parsing as planning, so inline-code link
examples in labels remain literal while real destinations and fragments are
validated. Single/batch LF/CRLF preview/apply controls retain exact label bytes.
Plain and labeled/fragment directory-index links retain exact preview/apply
and disk/index graph parity.

Direct BM25 scoring now requires complete corpus coverage, unique paths and
coherent finite statistics. Admission validates posting order, position bounds,
and term-frequency agreement; phrase offsets use checked addition. Tiny partial
and malformed snapshots fall back with exact disk-result parity.

The final product commit is `c2e3992b0d4620aaec27dc005542144b550401b9`.
Its immutable release artifact is
`/Users/james/devel/hyalo/.git/ralph-loop/run-20260912-296/bin/hyalo-review8-final`,
SHA-256 `fff24e6644efb97e33279bf52f93a1792d888cc98bce7310036169c9d4b76a04`.
It was built after that commit, reporting `c2e3992b0d46`; the frozen source
manifest binds its exact product bytes to the committed revision. A prior link
attempt failed during the owner's Apple Command Line Tools reinstall; after
compiler/SDK availability was verified, the ordinary release build succeeded.

- Ordered formatting, strict workspace Clippy and all 5,194 Rust tests passed;
  two historical ignored doctests remain excluded.
- npm typecheck/build and all 40 tests passed against that release artifact.
- The affected mutation-journal, Pi-runtime and typed-output gates passed again;
  the behavioral-contract gate also passed its six suites and 72 tests during
  the preceding repairs. Earlier supported-gate evidence remains recorded;
  native CI runs the applicable quality gates.
- All 131 release dogfood commands passed, plus three real Node checks of the
  unrelated inherited-ESM helper before installation, after init and after deinit.
- MDN remained read-only. A fresh external snapshot indexed 14,375 files;
  ranked results matched disk exactly (65 matches, five returned), metadata
  results matched all 12 entries, and the direct-scoring counter was observed.
  Single wall-time samples were 2.665 s for creation, 3.885/0.571 s for disk/index
  ranked search, and 0.724/0.291 s for disk/index metadata search. Own-KB
  summary/ranked/lint took 0.069/0.271/0.176 s. These are uncontrolled samples.

The full PR review ledger has nine local findings and ten raw Copilot findings
(six inline plus four in its suppressed-comment summary). Two duplicate pairs
reduce these to 17 distinct findings: fifteen fixed and two specifically answered.
Copilot reviewed the original `dab3a2f6` head; its feedback was reproduced against
later source and repaired, not represented as approval of a newer commit.

The two answered observations preserve explicit boundaries. Reversed OKF markers
already produced the same warning and safe skip at baseline; its policy remains
unchanged. MessagePack decoding still precedes structural/posting guards, as it
did at baseline. The 512 MiB serialized and 50-million-posting limits are not
hard peak-allocation guarantees. Generic bounded-decoder redesign is outside this
consumer repair; no exhaustion experiment was run. Reconstruction caps and
documented filesystem/concurrency limitations remain unchanged.

The final product has full-review coverage through `edd5af8f`, followed by a
fresh read-only repair-delta review of `edd5af8f..c2e3992b` with no verified
defects. All ten applicable CI jobs passed on `c2e3992b`. The completion-only
commit receives separate review and checks before merge; this record does not
claim a merge that has not yet occurred. At
`e045a186` and `483380ec`, a Windows concurrency test observed zero successful
competing writers; each targeted rerun passed without changing or skipping the test. Original
failure evidence remains in the run store. Expected skips (the push-only full KB
lint and opt-in scale artifact) are not counted as successful jobs.

Windows CI additionally caught a real slash-rooted-path classification failure
at `30bd2a9e`. The correction in `96734e7e` checks the normalized leading slash
independently of host absolute-path semantics. The original failing assertion
and four additional rooted/UNC/drive controls remain enabled; that failed head
was not rerun to avoid the failure.
