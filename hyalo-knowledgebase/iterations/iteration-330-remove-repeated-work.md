---
title: "Iteration 330: remove repeated work"
type: iteration
date: 2026-11-07
tags: [iteration, performance, index, lint]
status: in-progress
branch: iter-330/remove-repeated-work
priority: 1
related: ["[[decision-log]]", "[[research/rust-performance-2026-11]]"]
---

# Iteration 330: remove repeated work

An audit found that most of hyalo's cost on a large vault is the same work done twice.
This iteration removes it without changing one answer. Every audit estimate was
reproduced and measured first; the ones that did not hold are reported and left
alone. Decisions: DEC-406 (lazy graph, named refresh that compares), DEC-407 (one
comrak parse, MD034 line pruning). No snapshot format bump (stays 12): no entry's
contents changed. Numbers and the audit's ranking are in
[[research/rust-performance-2026-11]].

## Tasks

### Items, in the audit's order

- [x] 1. A named refresh keeps the entry, the graph and the postings when the rescan equals the entry; a ranking `find` and every other caller still refresh (DEC-406)
- [x] 2. The graph is resolved on first read: no rebuild at load or at `configure`; older-format and refused snapshots never build one (DEC-406)
- [x] 3. Lint parses comrak once per document and passes it to every rule; no rule mutates the tree (DEC-407)
- [x] 4. MD034 is handed the document with scheme-free lines emptied; proven exact (DEC-407)
- [x] 5. `AliasVisitor::needs_body` is `false`: body reads on a disk scan halve (28,750 to 14,375); allocations did not move, the audit's -3.4M is not reproduced
- [x] 6. `heading_slugs` per fragment: not reproduced (`--broken-links` adds at most 0.14M allocations on GitHub Docs, 0.53M on MDN); no code change
- [x] 7a. BM25 `push_doc` looks the term up before allocating its key (-2.33M allocations in `create-index` and disk ranked `find`)
- [x] 7b. A shared arena for posting positions: left, `Posting` is serialized and read in many places and the arena is not contained
- [x] Report-only: the double `stat` per note and the `rooted.rs` temp copy are described in the research note; neither changed

### Proof

- [x] Baseline binary built from `5e2512ed` before any change
- [x] Byte-identical JSON, baseline against new, hints off, disk and `--index`, on MDN, GitHub Docs and the Obsidian Hub: `find` with `--fields links`, `--fields properties`, `--broken-links`, `--orphan`, `--dead-end`, `summary`, `tags`, `properties`, `lint`, `backlinks` on 25 notes, ranked `find` on 20 queries: 324 comparisons, 0 differences
- [x] Staleness matrix (an edit, an edit keeping size, an edit keeping size and mtime, a body-only edit keeping both, a delete, a deleted target, an add; `backlinks`, `read`, `find --file`, `lint FILE`): new and baseline give the same 112 results; the 4 that differ from disk are an unnamed referrer edited with size and mtime restored, invisible to any stat probe in both binaries
- [x] Table-driven tests: every default stock rule on 13 bodies with the shared tree against a fresh parse; MD034 on the full against the emptied document; `lint_body` with every rule against one rule at a time; refresh after each kind of edit with and without `tokens_matter`; lazy graph resolves once; `read_aliases` over 10 frontmatter shapes (also run against the old behaviour)
- [x] Allocation counts before and after for every changed command (research note)

### Gates

- [x] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -q` (5,987 passed, 0 failed suites)
- [x] Every xtask gate, `generate-npm-packages --check`, `cargo deny check`
- [x] npm gate (34 tests); case-sensitive volume gate (2,699 passed, only `wikilink_resolution_folds_unicode_normalization` failed, as expected); `hyalo lint --strict` on the changed files
- [ ] PR opened against `main`
