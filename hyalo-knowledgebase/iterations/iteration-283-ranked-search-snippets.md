---
type: iteration
title: "Iteration 283 — Snippets for BM25 ranked search results"
date: 2026-09-06
status: completed
tags: [iteration, search, find]
branch: iter-283/ranked-search-snippets
priority: 2
related:
  - "[[research/npm-package-and-typed-typescript-api]]"
  - "[[decision-log]]"
---

# Iteration 283 — Snippets for BM25 ranked search results

## Goal

A ranked `hyalo find PATTERN` answers "which files" but not "where in them". `FileObject` has
both `score` and `matches` (`crates/hyalo-core/src/types.rs:556-558`, `ContentMatch { line,
section, text }`), but `find/mod.rs:899` fills `matches` only on the regex path
(`has_regex_search`); BM25 mode leaves it `None`. A consumer that wants to show a hit — the pi
tools, an MCP server, the planned TypeScript API in
[[research/npm-package-and-typed-typescript-api]] — must run a second `find -e` or `read` per
result to get a line to display, which turns one call into N+1.

This iteration fills `matches` in ranked mode with the lines that carry the query terms, using
the **same field and the same shape** regex mode already emits. No new CLI flag: the field
exists, `--section` scoping already applies to it, and text mode already knows how to print it.

## Tasks

- [x] TASK-1: read the BM25 path end to end (`bm25_tokenize`, stemming, `OR`, quoted phrases,
      the CJK bigram tokenizer, `score_map`) and write down what "a line that matches" means for
      each query form: a line contains at least one stemmed query token; for a quoted phrase,
      the consecutive stemmed sequence; for `OR`, any side. Put the definition in the module doc
      before writing code.
- [x] TASK-2: implement snippet extraction for the top results: for each ranked file, select
      the body lines that match per TASK-1, rank them by the number of *distinct* query tokens
      they carry (ties in document order), keep at most 3 per file, and emit them as
      `ContentMatch { line, section, text }` exactly as regex mode does. Honour `--section`
      scoping the way `find/mod.rs:920-923` does for regex. Lines inside frontmatter never
      qualify.
- [x] TASK-3: `--index` path: confirm whether the snapshot carries enough to produce snippets
      without touching disk. If not, read only the files that made the result set (after
      `--limit`, if any), never the whole vault — the point of the index is that the scan stays
      cheap. Measure MDN `find <term> --index` before and after; the delta must stay well below
      the disk-scan time (DEC-322/323 numbers are the baseline).
- [x] TASK-4: text mode prints ranked matches the same way it prints regex matches
      (`line N [section]: text`). JSON is unchanged in shape — `matches` simply stops being
      `null` in ranked mode. Update `find --help` (the "per-line 'matches' instead of 'score'"
      sentence is now wrong), `skill-hyalo.md`, `rule-knowledgebase.md`, the command reference,
      and the pi extension's `hyalo_find` description if it mentions the gap.
- [x] TASK-5: e2e tests for a single term, a stemmed term (`running` finds a line with `run`),
      an `OR` query, a quoted phrase, `--section` scoping, the 3-per-file cap, a CJK query, and
      `--index` parity (same `matches` as the disk scan). One test asserts a match's `line`
      points at the line that actually contains the token.
- [x] TASK-6: `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, every xtask `check-*` gate, `hyalo lint --strict` on the
      knowledgebase. DEC entry for the snippet rule (what qualifies, the cap, the ranking).

## Acceptance criteria

- [x] `hyalo find rust --format json` returns, for every result, a non-null `matches` array
      whose entries have the same `{line, section, text}` shape regex mode emits, each `text`
      containing at least one stemmed query token.
- [x] `matches` is capped at 3 per file, ordered by distinct-token count then line number, and
      respects `--section`.
- [x] `--index` and disk-scan output are byte-identical for the same query.
- [x] No new CLI flag. `find --help` and the bundled skill/rule text describe the new behaviour.
- [x] MDN `find <term> --index` wall time stays within 10% of `main`.
- [x] Gates green.

## Implementation evidence

Baseline release: `4c9310741086ad8bc5f6091bb902b64ad629ba47`, retained with its
SHA-256 under `.git/ralph-loop/run-20260906-284-287/283/`. MDN's 14,375 markdown
files (256 MiB) were copied from `/Users/james/devel/mdn/files/en-us` to
`/tmp/hyalo-iter283-mdn/corpus`; the snapshot lives beside that scratch corpus.
The source repository is read-only throughout this iteration.

The representative benchmark is the common term `javascript`, with JSON
output and no explicit limit flag, `--site-prefix en-US/docs`, `--no-hints`, and the same
scratch snapshot for baseline and candidate. Baseline: 12 runs after 3 warmups,
mean 480.6 ms (standard deviation 39.7 ms). Commands and raw timings are retained
in that iteration's run directory. The plan's DEC-323 link concerns bulk writes,
not indexed search; DEC-322 provides historical index context, while the retained
main binary supplies this iteration's exact before/after baseline. The 10% gate
above is unchanged.

The literal default command emits 50 results with `total: 4055`; JSON has the
same default cap as text. It is the primary performance gate. The additional
`--limit 0` stress case emits all 4,055 results. These two workloads must not be
conflated. The default query's complete disk/index JSON envelopes are identical.

The snapshot contains stemmed tokens and token positions, but no original text
or file-line offsets. Snippets therefore stream only final selected files,
after filtering, sorting and limiting. Qualification is defined in the BM25
module documentation and DEC-330 in [[decision-log]].

Full-envelope parity testing also exposed and corrected an older title-token
mismatch: disk scoring had included the displayed filename fallback, while
snapshots used only frontmatter string titles or H1 headings. Both now share the
existing indexed rule. Disk scores change in corpora containing title/H1-less
notes; indexed scores, display titles and snapshot schema remain unchanged.
The BOM/CRLF German fixture guards this alongside language and snippet line
accuracy. Existing text formatting is `line N (section): text`, so the renderer
is preserved rather than adopting the plan's illustrative bracket spelling.

Pre-review release timings use alternating main/candidate subprocess pairs, five
warmup pairs, no concurrent builds/tests, and unchanged MDN source data:

| Indexed query | Results emitted / total | Main mean | Candidate mean | Change |
| --- | --- | --- | --- | --- |
| `find javascript --index` (30 pairs) | 50 / 4,055 | 448.25 ms | 454.60 ms | +1.42% |
| Same query plus `--limit 0` (15 pairs) | 4,055 / 4,055 | 483.72 ms | 657.28 ms | +35.88% |

The primary default-query 10% gate passes. The unlimited stress case is
supplemental evidence, with a real 174 ms cost for reading/tokenizing all
selected bodies and emitting their snippets. Parallel selected-file extraction
reduced its candidate mean from the initial serial 1,324.44 ms to 657.28 ms.
The main disk-scan query averaged 4.090 s (five runs after one warmup), so both
indexed deltas stay well below disk-scan time. Complete final disk/index JSON
envelopes compare byte-identically for both the 50-result and 4,055-result runs.

Raw evidence: `paired-timing-default-final.json`,
`paired-timing-unlimited-final.json`, the Rust benchmark harnesses,
`mdn-*-final.json`, and per-command logs under the iteration run directory.
The source repository's before/after status files compare identically.

Implementation validation passed ordered fmt, clippy and workspace tests.
All ten discovered xtask commands returned zero; dead-primitives and
todo-annotations explicitly remain unimplemented stubs, and the jq gate cannot
exercise `madr toc` without this vault's absent ADR directory. Fourteen bundled
skills passed conformance. Strict configured-KB lint has zero errors and only
the four pre-existing HYALO002 warnings (270, 271, 277, 281); the two changed KB
files are clean. Changelog-profile lint retains its existing `2022-04` link
warning. Candidate dogfooding confirmed ranked Rust snippets and a quoted
`"ranked snippets"` query scoped to DEC-330 using the existing text renderer.

## First review repair

Both verified boundary findings are addressed in shared helpers: bounded reads
validate complete retained UTF-8 lines rather than individual buffer chunks,
and frontmatter skipping counts normalized LF bytes to match the scanner's
64-KiB budget. Tests cover 2/3/4-byte Unicode at varying buffer boundaries,
incomplete EOF, actual invalid bytes, exact/over-limit lines, and LF/CRLF
frontmatter at and above the budget. CLI regressions reproduce the character at
absolute byte 8191 and the thousand-comment-line CRLF note, checking successful
ranked results, exact snippet lines and complete disk/index parity.

Repair-specific gates and updated timings are retained separately under
`repair-1-*` in the iteration run directory; original implementation evidence
remains intact. See the first-review repair note in DEC-330.

The repaired release passes the same primary timing gate: 30 alternating pairs
measure 446.54 ms main versus 444.42 ms candidate (-0.48%) for the default
50-result query. The supplemental unlimited query measures 454.57 → 608.28 ms
(+33.81%, fifteen pairs). Both use five warmup pairs and no concurrent builds
or tests. Complete repaired disk/index JSON envelopes remain byte-identical
for 50/4,055 and 4,055/4,055 emitted/total results. The parent's exact Unicode
and CRLF repro fixtures now exit zero and report the previously missing lines.

Repair validation reran ordered fmt/clippy/workspace tests, bundled-skill and jq
runtime gates, strict configured-KB lint, changed-KB lint and changelog-profile
lint. Existing help/flag/command-reference/package checks and the static journal
guard are reused because their checked inputs did not change; the two xtask
stubs remain explicitly unimplemented. Lint retains only the previously recorded
four KB warnings and one changelog warning; changed KB files remain clean.

## Final review and plan reconciliation — 2026-09-07

A fresh independent review confirmed both boundary repairs and found no actionable issues.
Final workspace evidence totals 4,878 passing tests and two existing ignored doctests.
All four upcoming plans were inspected: 285 now names this iteration's byte-parity baseline,
and 287 includes ranked snippet contract cases. No changes were needed for 286's packaging
scope or 288's deferred desktop verification. External prerequisites remain unfinished.
Plan hashes, dispositions, review inputs and command evidence are preserved in the run store.

## Related links

- [[research/npm-package-and-typed-typescript-api]] — "Snippets in ranked mode" open question;
  this iteration is the "ranked mode grows a snippet field" option it prefers
- [[decision-log]] — DEC-322 index context and DEC-323 bulk-write context
