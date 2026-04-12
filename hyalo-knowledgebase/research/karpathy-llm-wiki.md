---
title: "Karpathy's LLM Wiki Pattern — Research & Ideas for hyalo"
type: research
date: 2026-04-09
status: active
tags: [research, llm-wiki, knowledge-management, search, fts, architecture]
source: "https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f"
---

# Karpathy's LLM Wiki Pattern — Research & Ideas for hyalo

## The Core Idea

Andrej Karpathy (2026-04-04) proposes replacing stateless RAG with a **persistent, LLM-maintained wiki** — structured, interlinked markdown that compounds with every source ingested and question asked. The LLM reads raw sources, extracts key information, integrates it into existing pages, updates entity pages, flags contradictions, and strengthens synthesis.

> "The wiki is a persistent, compounding artifact."

The human curates sources, directs analysis, asks questions, thinks about meaning. The LLM does everything else: summarizing, cross-referencing, filing, bookkeeping.

## Three-Layer Architecture

1. **Raw sources** — immutable curated documents. LLM reads but never modifies. Source of truth.
2. **The wiki** — LLM-generated markdown. Summaries, entity pages, concept pages, comparisons, synthesis. LLM owns this entirely.
3. **The schema** — document (e.g. CLAUDE.md) specifying wiki structure, conventions, workflows. Co-evolved with the LLM.

## Core Operations

| Operation | Description |
|-----------|-------------|
| **Ingest** | Drop a source, LLM reads it, writes summary, updates index, updates entity/concept pages, appends log. One source may touch 10-15 pages. |
| **Query** | Ask questions against the wiki. LLM searches relevant pages, synthesizes answer with citations. **Good answers file back into the wiki.** |
| **Lint** | Health-check: contradictions, stale claims, orphan pages, missing cross-references, data gaps. Suggests questions to investigate. |

## Navigation: index.md + log.md

- **index.md** — content-oriented catalog. Each page listed with link, one-line summary, metadata. LLM reads index first, drills deeper. Works at moderate scale (~100 sources, hundreds of pages).
- **log.md** — append-only chronological record of ingests, queries, lint passes. Parseable with unix tools.

## Scaling: Search Beyond index.md

Karpathy suggests [qmd](https://github.com/tobi/qmd) — hybrid **BM25 + vector search** with LLM re-ranking, all local. Multiple commenters converged on similar stacks:

- **BM25** (Okapi BM25) — probabilistic relevance ranking for keyword search. The standard for full-text search.
- **FTS5** — SQLite's full-text search extension, used by several implementations (browzy, LENS).
- **Hybrid search** — BM25 + vector embeddings, with optional **RRF (Reciprocal Rank Fusion)** to merge ranked results from different retrieval methods.
- **sqlite-vec** — lightweight vector similarity on top of SQLite (used by LENS).

### RRF (Reciprocal Rank Fusion)

A simple, effective method to combine ranked results from multiple retrieval systems (e.g. BM25 keyword search + vector semantic search):

```
RRF_score(d) = Σ 1 / (k + rank_i(d))
```

Where `k` is a constant (typically 60), and `rank_i(d)` is the rank of document `d` in the i-th retrieval system. No normalization needed, no tuning of weights. Just merge by reciprocal rank.

**Why it matters for hyalo:** If we add both BM25 and vector search, RRF is the natural way to combine them without complex score normalization.

## Key Insights from Comments

### Architecture & Design Patterns

1. **Progressive disclosure with token budgets** (bluewater8008): L0 ~200 tokens (project context), L1 ~1-2K (index), L2 ~2-5K (search results), L3 5-20K (full articles). Don't read full articles until index is checked.

2. **One template per entity type** (bluewater8008, dkushnikov/Mnemon): A person page needs different sections than an event or paper summary. Seven types was the sweet spot. Mnemon uses 7 source-specific templates (article, video, podcast, book, paper, idea, conversation).

3. **Every task produces two outputs** (bluewater8008): Output 1 = what the user asked for. Output 2 = updates to the wiki. Without this rule, knowledge evaporates into chat history.

4. **Cross-domain tags from day one** (bluewater8008): Shared entities across domains become the most valuable graph nodes. Retrofitting is painful.

5. **TLDR at top of articles** (YokoPunk): Helps both humans and LLMs. LLM does index scan → TLDR → decide whether to read full article. Saves tokens.

6. **Source provenance with content hashes** (Jwcjwc12/Freelance): Every proposition records which source files produced it + content hashes. On query, check if files still match. Match = valid, mismatch = stale. Git branching works for free.

7. **Reflect step** (bendetro): Not just `ingest → compile → query → lint` but `ingest → compile → reflect → query → lint`. The wiki should know *why* it knows things, tracking decisions and alternatives as first-class pages.

### Epistemic Integrity (the hardest problem)

8. **Source-grounded, citation-first, review-gated wiki** (laphilosophia): The robust version is not "autonomous wiki" but the LLM proposes patches/summaries/links, not silently overwrites. Key constraints:
   - Separate facts, inferences, and open questions explicitly
   - Require source links for important claims (passage-level)
   - Make ingest idempotent
   - Have LLM propose diffs instead of silently overwriting
   - Lint for stale claims, unsupported claims, contradiction tracking

9. **Counter-argument generation** (localwolfpackai): Every concept page should have a `## Counter-Arguments & Data Gaps` section. Sanitizes confirmation bias.

10. **No content invention** (peas): The LLM must be editor, not writer — every sentence traces to what the user said. Gaps get `[TODO: ...]` markers, not hallucinated filler.

### Practical Tips

11. **Obsidian Web Clipper** for getting sources into raw collection quickly.
12. **Download images locally** to let LLM reference them directly.
13. **Wiki as git repo** — version history, branching, collaboration for free.
14. **Dataview / Bases plugin** for dynamic queries over frontmatter.
15. **Marp** for generating slide decks from wiki content.

## Relevance to hyalo

hyalo is already positioned as exactly the kind of tool Karpathy describes for the "index" layer — it helps LLMs navigate, search, and mutate a markdown knowledgebase. Several commenters built tools that overlap with what hyalo already does.

### What hyalo already provides

| Karpathy concept | hyalo equivalent |
|---|---|
| Structured frontmatter schema | `hyalo set`, `hyalo properties`, YAML frontmatter |
| Index / catalog | `hyalo find` with property/tag/content filters |
| Cross-references | `hyalo backlinks`, wikilink resolution, `hyalo links fix` |
| Lint (orphan pages, broken links) | `hyalo links fix` (broken links), `hyalo backlinks` (orphans) |
| Task tracking | `hyalo task read/toggle/set` |
| Vault overview | `hyalo summary` |
| Move without breaking links | `hyalo mv` |
| Snapshot index for performance | `hyalo create-index` / `--index` |

### What's missing — potential features

| Gap | Description | Priority |
|---|---|---|
| **BM25 ranked search** | Current `find` does substring/regex matching, not relevance-ranked. BM25 would rank results by relevance, critical for large vaults. Research done: [[research/fts-and-vector-search]], [[research/fts-lightweight-alternatives]]. Recommended crate: `bm25` with `default-features = false` (3 deps). | High |
| **Lint command** | A dedicated `hyalo lint` that checks: orphan pages (no inbound links), broken wikilinks, stale claims (old `date` + no recent updates), missing frontmatter fields, duplicate titles, pages without tags. Some pieces exist (`links fix`, `backlinks`), but no unified health-check. | High |
| **Ingest workflow / skill** | A `hyalo-tidy`-style skill that reads a raw source, creates a summary page, updates related entity/concept pages, appends to log. This is orchestration, not core CLI — better as a Claude skill. | Medium |
| **RRF hybrid search** | Combine BM25 keyword results with content-match results. Only relevant if we add BM25. | Low (future) |
| **Vector embeddings** | Semantic search via embeddings. Research done, conclusion: premature for hyalo. BM25 covers 90% of use cases. | Low (future) |
| **Token budget hints** | `hyalo find` could output estimated token counts per result, helping LLMs decide what to read in full. | Low |
| **TLDR/summary field** | Convention: `summary` frontmatter property for one-line descriptions. `hyalo find` could show these in results to help LLMs triage without reading full files. Already possible via frontmatter, but could be a first-class convention. | Low |

## Tools & Projects Referenced

| Project | Author | Stack | Notable feature |
|---|---|---|---|
| [qmd](https://github.com/tobi/qmd) | tobi | Local | Hybrid BM25/vector + LLM re-ranking, CLI + MCP |
| [browzy](https://github.com/VihariKanukollu/browzy.ai) | VihariKanukollu | Node.js | FTS5 + BM25, wikilinks, multi-provider |
| [LENS](https://github.com/flyersworder/lens) | flyersworder | Python | SQLite + sqlite-vec, FTS5 + vector, contradiction matrix (TRIZ-inspired) |
| [Binder](https://github.com/mpazik/binder) | mpazik | SQLite | Structured data → markdown rendering, transaction log |
| [Palinode](https://github.com/Paul-Kyle/palinode) | Paul-Kyle | SQLite | Git-versioned, BM25 + vector via SQLite-vec, 18 MCP tools |
| [Mnemon](https://github.com/dkushnikov/mnemon) | dkushnikov | Obsidian | 7 source-type templates, personalization layer, uses qmd |
| [sage-wiki](https://github.com/xoai/sage-wiki) | xoai | Go | Single binary, compile/search/query/lint, MCP server |
| [Freelance](https://github.com/duct-tape-and-markdown/freelance) | Jwcjwc12 | SQLite | Provenance via content hashes, query-time compilation |

## Conclusions

1. **BM25 is the clear next step** for hyalo search. Research is done, crate is chosen (`bm25`), the gap is real. This is the single highest-impact feature for wiki-scale vaults.

2. **A `lint` command** would directly implement one of the three core operations. hyalo already has the building blocks (backlinks, link checking, property queries) — it's a matter of composing them into a unified health-check.

3. **The ingest/compile workflow** is better as a skill (like `hyalo-tidy`) than core CLI. hyalo provides the substrate; the LLM orchestrates the workflow.

4. **hyalo is already well-positioned** in this space — it's the exact kind of local, CLI-based, markdown-native tool that multiple commenters built or wished for. The main gap is relevance-ranked search.
