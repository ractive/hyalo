---
title: "Iteration 148 — Dogfood v0.16.0 (iter-144..147) fixes"
type: iteration
date: 2026-05-29
status: planned
branch: iter-148/dogfood-v0160-iter147-fixes
tags:
  - iteration
  - dogfood-fixes
  - files-from
  - hints
  - help-text
related:
  - "[[dogfood-results/dogfood-v0160-iter-144-147]]"
  - "[[dogfood-results/dogfood-v0160-deep]]"
  - "[[iterations/done/iteration-144-index-suggestion-hint]]"
  - "[[iterations/done/iteration-145-unified-input-resolution]]"
  - "[[iterations/iteration-147-task-files-from]]"
---

## Goal

Address the findings from
[[dogfood-results/dogfood-v0160-iter-144-147]]. Five items: two MEDIUM
(NEW-1 hint crowding, NEW-3 `--dir` prefix in `--files-from`), two LOW
(NEW-4 help-text drift, NEW-5 envelope dup), one friction
(`create-index` outside-vault). All small, all defensive — no new
features, just close the gaps the dogfood opened.

## Scope decisions

### NEW-3 — multi-segment `--dir` prefix strip in `--files-from`

The marquee recipe `git diff --name-only | hyalo --files-from -` fails
on any repo whose vault dir is more than one segment from the repo
root. Repro:

```text

$ cd /Users/james/devel/mdn
$ echo "files/en-us/games/index.md" \
    | hyalo --dir files/en-us find --files-from - --no-hints
note: all --files-from entries were missing; …
{ "files_missing": 1, ... }
```text

The resolver currently strips only the *last* segment of `--dir`
(`en-us`), not the full multi-segment prefix (`files/en-us`). Fix the
resolver to strip the full normalized vault dir prefix (joined repo
root + `--dir`) when an entry starts with it.

**Edge cases to cover:**

- Vault dir is `.` (repo root) — strip no prefix; identity.
- Vault dir is one segment (`hyalo-knowledgebase`) — current behavior;
  must not regress.
- Vault dir is multi-segment (`files/en-us`, `content/docs`) — strip
  full normalized prefix.
- Entry already vault-relative (`games/index.md`) — pass through
  unchanged.
- Entry uses Windows-style separators on Windows — normalize before
  comparison.
- Trailing slash on `--dir` — normalize.

This is the prior-dogfood NEW-2 from
[[dogfood-results/dogfood-v0160-deep]] still open.

### NEW-1 — iter-144 large-vault index hint invisible on MDN

`hints_for_summary` (`crates/hyalo-cli/src/hints.rs:711`) gates the
large-vault hint on `hints.len() < MAX_HINTS` where `MAX_HINTS = 5`.
On MDN the slots are exhausted by orphan / broken-link / `links fix`
hints before the index hint is pushed.

**Decision:** prepend the create-index hint instead of appending. The
index hint has the largest user-visible payoff (~6× BM25 on MDN) so
it deserves priority over health hints. The health hints stay in the
list when there's room (small/clean vaults still see all 5); on
crowded large vaults the index hint replaces the *last* lower-priority
hint, not all of them.

Alternative considered: raise `MAX_HINTS` to 6 in the summary path. We
went with priority instead because raising the cap globally is the
kind of "incremental relaxation" that ratchets up over time. Keep the
cap, fix the ordering.

### NEW-4 — `set` / `remove` / `append --file` help text omits `--files-from`

```text
$ hyalo set --help | grep -A1 "Target file"
        Target file(s) (repeatable). Mutually exclusive with --glob
```text

vs `find` / `task toggle`:

```text
        Mutually exclusive with --glob and --files-from
```text

The flag itself works (mutual exclusion is enforced at clap parse
time), only the help text is stale on the three subcommands. Per
[[feedback_keep_docs_in_sync]] this should be fixed in the same PR
that adds the behavior — slipped through in iter-145. Now: sync the
three `--file` doc strings.

### NEW-5 — `summary` envelope `dir` key duplicated

```text
$ hyalo summary --format json | jq '{top: .dir, inner: .results.dir}'
{ "top": "hyalo-knowledgebase", "inner": "hyalo-knowledgebase" }
```text

`dir` is envelope-level metadata on every other command. Drop it from
`results` and keep it at the top of the envelope. This is a JSON
shape change — guard with a check for downstream consumers, but the
inner field has no documented contract (no schema entry, no examples
in README) so the risk is low.

### Friction — `--allow-outside-vault` on `create-index`

```text
$ hyalo --dir files/en-us create-index -o /tmp/mdn.idx
error: output path is outside the vault boundary
hint: pass --allow-outside-vault to override
$ hyalo --dir files/en-us create-index --allow-outside-vault -o /tmp/mdn.idx
error: unexpected argument '--allow-outside-vault'
```text

The hint suggests a flag the subcommand doesn't accept. Two ways out:
add the flag (consistent with the hint), or drop the hint. The flag
is the right answer — external read-only KBs (MDN, GitHub Docs) want
the index in `/tmp/` or `~/.cache/hyalo/`, not polluting the docs
repo. Reuse the existing `--allow-outside-vault` plumbing from the
other commands that already accept it.

## Steps

### NEW-3 — `--files-from` multi-segment prefix strip

- [ ] Locate `--dir` prefix-strip logic in the `--files-from`
      resolver (`crates/hyalo-cli/src/files_from.rs` or the unified
      resolver in `commands/run.rs`).
- [ ] Replace last-segment strip with full normalized vault-dir
      prefix strip. Reuse the existing path canonicalization helper
      so trailing-slash and Windows-separator handling is consistent.
- [ ] Add unit tests in the resolver module for the six edge cases
      above (`.`, single-segment, multi-segment, vault-relative entry,
      trailing slash, Windows separators).
- [ ] E2E test: in `tests/e2e/files_from.rs` add a `--dir files/en-us`
      scenario that pipes `files/en-us/foo.md` and asserts
      `files_missing == 0`.
- [ ] Polish the hint emitted when entries miss: the current text
      mentions `notes/foo.md` — update so the example also works for
      multi-segment dirs.

### NEW-1 — re-order summary hints

- [ ] Find `hints_for_summary` in `crates/hyalo-cli/src/hints.rs`.
- [ ] Push the create-index hint before the orphan / broken-link /
      links-fix hints, keeping the MAX_HINTS = 5 cap.
- [ ] Update the unit test in `hints.rs` that asserts hint order for
      large vaults.
- [ ] Add an e2e regression test: a 600-file synthetic vault with
      broken links + orphans must include the `create-index` hint in
      its summary output.

### NEW-4 — help-text sync

- [ ] Update `--file` doc string on `set`, `remove`, `append` to
      include `--files-from` in the mutual-exclusion list.
- [ ] Verify with `hyalo set --help | grep -A1 'Target file'` etc.
- [ ] If the wording is duplicated across subcommands, extract to a
      shared `const` so future drift is harder.
- [ ] Help-drift xtask: confirm `check-help-drift` (if it exists)
      catches this kind of asymmetry; otherwise file a follow-up.

### NEW-5 — drop duplicated `dir` from summary envelope

- [ ] Remove `dir` from the `SummaryResults` struct (`commands/summary.rs`
      or equivalent).
- [ ] Verify the envelope-level `dir` is still emitted by the shared
      `OutputEnvelope` wrapper — top-level `dir` is what callers
      should consume.
- [ ] Update any unit / e2e tests that read `.results.dir`.
- [ ] CHANGELOG: note the inner field removal under a `Changed` or
      `Removed` entry; risk is low (no schema, no README usage) but
      mention it for grep-ability.

### Friction — `--allow-outside-vault` on `create-index`

- [ ] Add `--allow-outside-vault` flag to `create-index` subcommand.
- [ ] Plumb through to the same vault-boundary check the other
      commands use.
- [ ] E2E test: `--dir files/en-us create-index --allow-outside-vault
      -o /tmp/mdn-test.idx` succeeds and produces a usable index.
- [ ] Update the existing error hint text only if the flag name
      changes; otherwise the hint is now accurate as-is.

### Cross-cutting

- [ ] CHANGELOG `Unreleased` entry per item under the right sections.
- [ ] Tick the NEW-1, NEW-3, NEW-4, NEW-5 boxes in
      [[dogfood-results/dogfood-v0160-iter-144-147]] (mark each as
      "fixed in iter-148").
- [ ] Run discipline xtasks before `/create-pr`:
      `cargo run -p xtask -- check-dead-primitives --since origin/main`
      and `cargo run -p xtask -- check-todo-annotations --since
      origin/main`.
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D
      warnings`, `cargo test --workspace -q`.
- [ ] Cross-platform CI green.

## Tasks

- [ ] Fix `--files-from` multi-segment `--dir` prefix strip
- [ ] Add unit + e2e tests for the resolver edge cases
- [ ] Reorder summary hints so `create-index` is not crowded out
- [ ] Sync `set` / `remove` / `append --file` help text
- [ ] Drop duplicate `dir` key from `summary` envelope
- [ ] Add `--allow-outside-vault` to `create-index`
- [ ] CHANGELOG entries (one per fix)
- [ ] Tick the corresponding boxes in the dogfood report
- [ ] xtask discipline checks pass
- [ ] `cargo fmt`, clippy `-D warnings`, full test suite green

## Acceptance criteria

- [ ] `echo files/en-us/foo.md | hyalo --dir files/en-us find
      --files-from -` resolves the file (no `files_missing`) on any
      multi-segment vault dir, including `.` and single-segment as
      regressions
- [ ] `hyalo --dir <large-vault> summary --format json` includes a
      `create-index` hint, even when orphan/broken-link/`links fix`
      hints are also present
- [ ] `hyalo set --help`, `hyalo remove --help`, `hyalo append --help`
      all mention `--files-from` in the `--file` mutual-exclusion
      sentence
- [ ] `hyalo summary --format json | jq '.results.dir'` is `null`
      (field removed); top-level `.dir` still present
- [ ] `hyalo --dir files/en-us create-index --allow-outside-vault -o
      /tmp/test.idx` succeeds and the index loads via `--index-file`
- [ ] All new tests pass on Linux, macOS, Windows CI
- [ ] NEW-3 cannot reproduce against this build on
      `/Users/james/devel/mdn`

## Design notes

- **Don't widen scope.** This is a five-item closeout iteration. No
  refactors of unrelated code, no "while we're here" polish on
  adjacent commands.
- **Prepend, don't expand.** NEW-1 is fixed by re-ordering, not by
  raising `MAX_HINTS`. Keep the cap honest.
- **Reuse existing plumbing.** `--allow-outside-vault` already exists
  on other commands; the path-canonicalization helper for `--dir`
  already exists; the `OutputEnvelope` already emits top-level `dir`.
  This iter is 95% wiring and 5% logic.
- **Substring `--section` is by design** — see [[decision-log]] under
  iter-36 DEC. The dogfood's retracted NEW-2 is not in scope; do not
  add a gate, warning, or flag.

## Out of scope

- Iter-144 slow-query hint threshold tuning (already well-tuned).
- Iter-145 single-file error rendering quirk (`read` plain text vs
  JSON envelope on TTY) — cosmetic, no behavior change.
- `--files-from` whitespace trim (prior NEW-4), dedup (prior NEW-6),
  `--index-file` ignored (prior NEW-5) — separate iter, separate scope.
- Help-drift xtask additions; mention in steps but don't block on it.
- Any `--section` matching changes.

## References

- [[dogfood-results/dogfood-v0160-iter-144-147]] — source of all five
  findings
- [[dogfood-results/dogfood-v0160-deep]] — prior NEW-2 (this iter's
  NEW-3) was first reported here
- [[iterations/done/iteration-144-index-suggestion-hint]] — defines
  the hint NEW-1 is fixing
- [[iterations/done/iteration-145-unified-input-resolution]] — added
  `--files-from` to `set`/`remove`/`append`; NEW-4 is a help-text
  miss from that work
- [[iterations/iteration-147-task-files-from]] — most recent iter; no
  regressions, just adjacent polish
