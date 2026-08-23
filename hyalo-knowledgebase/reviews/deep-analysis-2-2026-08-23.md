---
title: Deep analysis #2 2026-08-23 — architecture, code flaws, and test quality
type: review
date: 2026-08-23
tags:
  - review
  - architecture
  - testing
status: active
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/codebase-review-2026-08-06]]"
---

# Deep analysis #2 — architecture, code flaws, and test quality (2026-08-23)

Second-pass review, complementary to `adversarial-review-2026-08-23.md` (whose H-1, M-1,
M-2, L-1 remain open and are cross-referenced, not repeated). Labels: **VERIFIED** =
reproduced; **SUSPECTED** = static reasoning. All file:line cites from current tree.

---

## Architecture

### ARCH-1. `dispatch::dispatch` is a 2,100-line god function — orchestration and business logic live in the router

**Location:** `crates/hyalo-cli/src/dispatch.rs:494` — one `match` with 26 `Commands::`
arms spanning lines 494–2660. The `Find` arm alone is ~240 lines and contains real
business logic, not routing:

```rust
// dispatch.rs:516-543 (inside the Find arm)
if !file_positional.is_empty() {
    if !filters_raw.glob.is_empty() {
        crate::warn::warn(
            "positional file arguments override the view's --glob; \
             glob filter has been ignored",
        );
    }
    filters_raw.file = file_positional;
    filters_raw.glob.clear(); // file overrides view's glob
}
...
if orphan && dead_end {
    crate::warn::warn(
        "--orphan and --dead-end are mutually exclusive ...",
    );
}
```

The `commands/` module tree exists and holds per-command logic (`commands/set.rs`,
`commands/mv.rs`, …), but the dispatcher re-implements per-command pre/post logic inline:
filter merging, warning policy, view adaptation (`adapt_view_result_to_ext`,
`dispatch.rs:256`), snapshot patching (`patch_index_for_modified_files`,
`dispatch.rs:427`). The Lint arm runs from line 1740 to 2128 (~390 lines).

**Why it matters:** this is the file every new command touches; merge conflicts and
missed cross-cutting steps (see ARCH-3) concentrate here; and command behavior can't be
unit-tested without going through the whole dispatch path (hence 1,594 e2e tests driving
a full process each).

**Fix:** extract one handler per command (e.g. `commands::find::run(FindArgs) ->
CommandOutcome`), with `dispatch` reduced to parsing `Commands` into args structs and
calling it. The warnings above move into the handler where they're testable in-process.

### ARCH-2. Lint lives in hyalo-cli (4,375 lines) while hyalo-mdlint is only the rule engine — the crate split doesn't match the domain

**Location:** `crates/hyalo-cli/src/commands/lint.rs` (4,375 lines) vs
`crates/hyalo-mdlint/src/` (engine + 5 rules, ~small).

Schema validation, the profile system (`okf`, `madr`, `changelog`, `skills`, `github`),
and all the HYALO native rules' orchestration live in the CLI crate. hyalo-mdlint
contains only the mdbook-lint engine wrapper and native rules HYALO001–004. Consequences:
lint logic cannot be reused by the planned library consumers of hyalo-core; profile
lints (`changelog_lint.rs`, `madr_lint.rs`, `okf_lint.rs`, `skills_lint.rs`,
`lint_github.rs` — five more CLI modules) form a hidden subsystem spread across the CLI.

**Fix:** move schema validation + profile linting into hyalo-mdlint (it already depends
on hyalo-core for `util::is_iso8601_*`). CLI keeps flag parsing and output formatting
only. This also gives the lint subsystem an in-process API that the e2e suite can drive
directly instead of spawning processes.

### ARCH-3. Snapshot-index maintenance is scattered across three mechanisms — every mutating command must opt in correctly

**VERIFIED (by grep):** three different index-refresh mechanisms coexist:

- `mutation.rs` — `save_index_if_dirty` (called from `mv.rs`, `set.rs`, `remove.rs`,
  `new.rs`, `lint.rs`, `append.rs`, `dispatch.rs`: 8 call sites)
- `commands/tasks.rs:238,344` — a *local* `patch_index` helper (task toggle/set status)
- `index.rs:481-558` — `refresh_entry` / `rename_entry` for graph-aware updates

Each mutating command picks its own mechanism; nothing enforces that a new mutating
command refreshes the persisted index at all. The mtime-fallback in
`patch_index_for_modified_files` (dispatch.rs:427) catches stale *entries* on the next
read, but the *link graph* staleness is only patched by whoever remembered to call the
graph-aware variant — `index.rs:439`'s own doc comment admits this class of bug existed
("`--index` left the persisted link graph stale").

**Fix:** make index refresh a property of the write path, not the caller: one
`MutationJournal` (or equivalent) that every frontmatter/link write goes through, which
records dirty rel_paths + whether links changed, and is flushed once at the end of
dispatch. `tasks.rs`'s local `patch_index` then disappears.

### ARCH-4. The hints layer is a hand-maintained parallel copy of the CLI surface — and the codebase knows it drifts

**Location:** `crates/hyalo-cli/src/hints.rs` (4,522 lines, 168 functions, 29
hand-assembled `"hyalo ..."` command strings).

Hints are built by string concatenation that must mirror clap's argument definitions by
hand. The project's own test docs state the consequence
(`tests/e2e/hint_execution.rs:1-9`):

> Every other hint test asserts on *substrings* of a hint's command … That is exactly
> the assertion the broken `hyalo tags --limit 0` hint satisfied while failing to run:
> `tags` takes no `--limit`, only `tags summary` does.

`hint_execution.rs` (execution-based sweep over a fixture vault) is an excellent
bandage, but it's an admission: the design guarantees drift, and the meta-test catches
it only after the fact, only for commands the fixture happens to exercise.

**Fix:** derive hint commands from a typed command registry shared with `cli/args.rs`
(e.g. a `CommandSpec` table that both clap definition and hint building consume), or
build hints as argv vectors serialized through the existing `shell_quote` — never as
hand-written strings. Incrementally: new hints must go through a `HintBuilder::cmd()`
API that takes argv, which is already trivially available.

### ARCH-5. hyalo-core exposes everything — the "library" boundary is 26 flat `pub` modules, including dead code

**Location:** `crates/hyalo-core/src/lib.rs:1-26` — every module `pub`, including
plumbing (`fs_util`, `util`, `warn`) and internals (`case_index`, `math`,
`common_words` — a 962-line embedded word list).

**VERIFIED dead code:** `crates/hyalo-core/src/math.rs` exports exactly one function,
`pub fn add(a: i64, b: i64) -> anyhow::Result<i64>`, with zero callers anywhere in the
workspace (`grep -rn "hyalo_core::math" crates` → only math.rs itself). It is public
API that exists only to be public.

**Why it matters:** every internal refactor is a semver-breaking change; invariants
(e.g. "callers must re-check the boundary after symlink resolution" in fs_util) are
enforced only by doc comments; `keywords = ["yaml", "frontmatter", ...]` on the crate
suggests it's meant for external consumption, but nothing marks the supported surface.

**Fix:** curate a root `pub use` façade (parse/find/mutate/link-graph/index types), make
internal modules `pub(crate)`, delete `math.rs`. Cheap now, expensive after external
consumers appear.

### ARCH-6. Presentation-layer mass rivals the core: hints (4.5k) + output (3.2k) + run (1.9k) + output_pipeline + suggest ≈ 11k lines

Not a bug, but the shape says the CLI's hardest problem is *deciding what to say*, not
doing the work. Combined with ARCH-1/ARCH-4 this means behavior changes require edits in
up to four layers (args.rs → dispatch arm → command module → hints.rs), with no
compile-time coupling between them. The fixes above (typed hints, thin dispatch) also
address this; noting it so the layer count itself is treated as a cost.

---

## Code flaws (new; not in report #1)

### F-1. `task toggle --section` silently applies to every section with a matching name

**Location:** `crates/hyalo-cli/src/commands/tasks.rs:33-55` — the section filter
matches *all* tasks whose heading matches:

```rust
let matched: Vec<usize> = tasks
    .iter()
    .filter(|t| { ... filter.matches(level, text) ... })
    .map(|t| t.line)
    .collect();
```

**VERIFIED:**

```console
$ printf '%s\n' '---' 'title: t' '---' '# Sec' '- [ ] one' '' '# Sec' '- [ ] two' > t.md
$ hyalo task toggle t.md --section "Sec"
→ toggles BOTH "one" and "two" (output lists both), no ambiguity warning
```

Contrast: `links` reports `ambiguous: N` when a wikilink target matches multiple files.
The mutation analog of that ambiguity — duplicate headings — is applied silently.

**Impact:** a user with repeated headings ("## Tasks" per ADR, "## Notes" per month)
toggles far more than intended, and because `task toggle` writes immediately, there's no
dry-run default to catch it (the write is atomic, but not previewed).

**Fix:** when a `--section` selector matches multiple *distinct heading instances*,
either refuse with an ambiguity error suggesting `--line`, or require an explicit
`--all-sections`/`--nth` flag. At minimum include the matched section line numbers in
the output the way links reports ambiguity.

### F-2. BM25 tokenization makes CJK content silently unsearchable

**Location:** `crates/hyalo-core/src/bm25.rs:148-149`:

```rust
pub fn tokenize(text: &str, stemmer: &Stemmer) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
```

For CJK text (no whitespace), every alphanumeric run becomes one giant token, so query
terms never match. **VERIFIED:**

```console
$ printf '%s\n' '---' 'title: CJK' '---' '日本語のテキストです' > cjk.md
$ hyalo find 日本語
→ {"results": [], "total": 0}    # the file contains the query verbatim
```

The module header claims "Unicode-aware tokenization" — it is Unicode-*safe*, not
CJK-*aware*. For a knowledgebase tool this is a silent correctness hole for any
Japanese/Chinese/Korean vault.

**Fix (cheapest first):** (a) detect CJK runs and additionally index them as bigrams;
(b) or fall back to substring matching when the query contains CJK and BM25 returns
nothing; (c) at minimum, document the limitation in `find --help` and the README.

### F-3. jq runtime errors embed the entire document serialization in the error message

**Location:** `crates/hyalo-cli/src/output.rs:710` (`apply_jq_filter_result`) — the
jaq runtime error is stringified with its input. **VERIFIED:**

```console
$ hyalo find --jq '.results | .file'
→ "cause": "jq runtime error: cannot index [{\"file\":\"a.md\",...ENTIRE RESULT SET...] with \"file\""
```

On a 14k-file vault a single mistyped filter dumps megabytes of vault content into the
error envelope (stderr/JSON). It's also a content-disclosure vector for scripted
consumers that log error output.

**Fix:** truncate the embedded value to ~200 chars (`…` suffix) and name the failing
filter position instead; keep full detail behind `--debug` if needed.

### F-4. Mixed-type property sort produces type-grouped ordering that looks wrong to users

**Location:** `crates/hyalo-core/src/filter/sort.rs:92-95` — documented fallback:

```rust
// Fallback: compare JSON representations.
let sa = va.to_string();
let sb = vb.to_string();
sa.cmp(&sb)
```

**VERIFIED:** with `priority: 9` (number), `priority: 10` (number), `priority: "10"`
(string), `find --sort property:priority` returns `c.md ("10"), a.md (9), b.md (10)` —
the string `"10"` sorts before the number `9` because `"` (0x22) < any digit. The total
ordering is deliberate (comment says so) but the result is user-visible nonsense with no
signal that types were mixed.

**Fix:** keep the total order, but emit a one-line warning when a sort key has mixed
types in the result set ("property:priority has mixed types; numbers sort after
strings"), or group missing/type-mismatched last consistently (as `Null` already does).

### F-5. Dead public API: `hyalo_core::math::add`

**VERIFIED** (see ARCH-5): zero callers workspace-wide. Delete it. If it predates a
checked-arithmetic policy, that policy is not enforced by having the function around —
clippy's `arithmetic_side_effects` would be the actual mechanism.

### F-6. Carry-overs from report #1 that this analysis re-confirms as the top fixes

- **H-1** unrestricted `dir` in `.hyalo.toml` (`config.rs:706`) — the write-scope escape;
- **M-1** whole-run `lint` abort on one invalid-UTF-8 file (`commands/lint.rs:2226`);
- **L-1** `create-index -o` bypasses the DEC-062 symlink-following write policy
  (`index.rs:925-929`).

---

## Test quality assessment

**What's there (and good):** 3,652 tests total — 1,594 e2e (process-level, TempDir
fixtures), 1,136 core unit, 922 CLI unit. Assertions are overwhelmingly behavioral with
specific expected values, e.g. `tests/e2e/links.rs`:

```rust
assert_eq!(
    broken, 2,
    "expected 2 broken links: [[nonexistent]] from b.md and [[Authnticoton]] ..."
);
```

The standout is `tests/e2e/hint_execution.rs`: it harvests every hint the CLI emits
across a broad command sweep and *executes* each against a fresh vault copy — a genuinely
unusual and valuable meta-test. Matrix tests (`dispatch.rs:2660`
`find_needs_stem_map_matrix`), boundary/TOCTOU tests in `fs_util.rs`, and
permission-preservation tests show the suite tests *consequences*, not snapshots.

**Gaps, ordered by payoff:**

1. **T-1 — No concurrency or crash-recovery tests.** The atomic-write machinery
   (temp + fsync + rename + dir-fsync, `fs_util.rs:204-278`) is the tool's most
   safety-critical code and has zero tests under contention or interruption: no
   two-process mutation race, no SIGKILL-mid-write, no kill between `persist` and
   parent-dir sync. A `#[test]` that spawns N processes running `set` on the same file
   and asserts the file is always one of the valid old/new contents is ~50 lines.
2. **T-2 — Fuzz-shaped inputs are hand-rolled, not systematic.** The adversarial
   hardening (SEC-1/2/3, line caps, anchor budgets) was driven by one-off PoCs;
   there are no `cargo-fuzz` targets for the four parser surfaces (frontmatter splice,
   scanner, link parser, MessagePack loader). Report #1 survived my afternoon of manual
   probing, but a corpus-based fuzzer would cover the input space continuously. (Set up
   targets, not CI, per project preference.)
3. **T-3 — e2e assertions parse into `serde_json::Value` and index by string** —
   `json["results"]["links"]["total"].as_u64()`. Output-shape regressions surface as
   `expect` panics with generic messages rather than typed assertion failures, and
   nothing ties the tests to the typed output structs that already exist
   (DEC-025). Deserializing into the real structs in tests would make schema changes a
   compile error across 1,594 tests instead of a runtime discovery.
4. **T-4 — Windows-only behaviors are untested anywhere** (see report #1 M-2): no
   drive-relative-path test, no ADS test, and the CI matrix — per `Cross.toml` —
   builds for Windows but the case-insensitivity logic (`case_index.rs`) is exercised
   only by `#[cfg]`-gated tests on the platforms that happen to run them. Probe-file
   behavior on real NTFS is unverified.
5. **T-5 — Substring assertions still dominate some suites** (`links.rs`: 104
   `.contains(` vs 5 typed-JSON parses). `hint_execution.rs`'s header explains exactly
   why substring assertions lie. Where they assert on *file content* they're fine; where
   they assert on *CLI output* they should become JSON parses.
6. **T-6 — No scale regression gate.** `bench-e2e.sh` exists but is manual; my probe
   (2,000 files: find 119 ms, links 367 ms cold) shows no problem today, but the known
   perf debt (iter-206 fuzzy candidates) has nothing preventing reintroduction. A
   criterion bench asserting a budget on the 14k-file synthetic vault, run in CI
   nightly, would pin it.

---

## Not reviewed (this pass)

- `auto_link.rs` scoring internals beyond the e2e surface (unchanged from report #1).
- `bm25.rs` ranking math beyond tokenization (IDF/length normalization correctness).
- `schema.rs` validation semantics vs. its Obsidian/OKF analogues.
- `init.rs`/`deinit.rs` CLAUDE.md managed-section editing (managed_region.rs).
- jaq evaluation semantics (trusted dependency).
- Error-message quality across all 40+ commands (spot-checked only).
