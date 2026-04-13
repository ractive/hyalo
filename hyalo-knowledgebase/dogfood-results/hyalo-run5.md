---
title: Hyalo Dogfood Run 5 — MDN Maintenance Tasks
type: research
date: 2026-03-31
status: archived
tags:
  - dogfooding
---
# Hyalo Dogfood Run 5 — MDN Maintenance Tasks
Date: 2026-03-31
Vault: 14,245 files (MDN en-us docs)

## Setup

```
$ time hyalo create-index --format text --no-hints
files_indexed: 14245 | path: files/en-us/.hyalo-index
```
**Time: 2.534s** | 1 command

All subsequent queries use `--index files/en-us/.hyalo-index`.

---

## Use Case 1: Title Cleanup — "The " prefix in Web API overview pages

**Goal**: Find pages with titles starting with "The " (based on real PR #43578).

### Commands & timings

| # | Command | Time | Result |
|---|---------|------|--------|
| 1 | `hyalo find --index $IDX --property 'title~=/^The /' --glob 'web/api/**' --jq '.total'` | 0.081s | 1 |
| 2 | `hyalo find --index $IDX --property 'title~=/^The /' --jq '.total'` | 0.080s | 12 |
| 3 | `hyalo find --index $IDX --property 'title~=/^The /' --fields properties,title --jq '[.results[] | {file, title, type: .properties."page-type", slug: .properties.slug}] | .[:20]'` | 0.081s | 12 results |
| 4 | `hyalo find --index $IDX --property 'title~=/^The /' --fields properties --jq '..group_by sections..'` | 0.086s | 10 sections |

**4 commands, ~0.33s total wall-clock**

### Results

**In `web/api/`**: Only **1 page** still has a "The " title prefix:
- `web/api/web_workers_api/structured_clone_algorithm/index.md` — "The structured clone algorithm" (guide)

**Across all MDN**: **12 pages** total with "The " title prefix:

| # | Slug | Title | Page-type |
|---|------|-------|-----------|
| 1 | Games/Tutorials/2D_breakout_game_Phaser/The_score | The score | guide |
| 2 | Glossary/Khronos | The Khronos Group | glossary-definition |
| 3 | Learn_web_development/Core/Styling_basics/Box_model | The box model | learn-module-chapter |
| 4 | Learn_web_development/Extensions/Forms/HTML5_input_types | The HTML5 input types | learn-module-chapter |
| 5 | Learn_web_development/Extensions/Performance/business_case_for_performance | The business case for web performance | learn-module-chapter |
| 6 | Learn_web_development/Extensions/Performance/why_web_performance | The "why" of web performance | learn-module-chapter |
| 7 | Learn_web_development/Getting_started/Web_standards/The_web_standards_model | The web standards model | tutorial-chapter |
| 8 | MDN/Kitchensink | The MDN Content Kitchensink | guide |
| 9 | MDN/Writing_guidelines/Page_structures/Page_types/Page_type_key | The page-type front matter key | mdn-writing-guide |
| 10 | Mozilla/Firefox/Releases/4/The_add-on_bar | The add-on bar | guide |
| 11 | Web/API/Web_Workers_API/Structured_clone_algorithm | The structured clone algorithm | guide |
| 12 | Web/JavaScript/Reference/Functions/arguments | The arguments object | javascript-language-feature |

**Section breakdown**: learn_web_development (5), web/api (1), web/javascript (1), mozilla/firefox (1), mdn (2), glossary (1), games (1).

**Insight**: The PR #43578 cleanup was very effective — only 1 remains in web/api. The remaining 12 are scattered across learn, glossary, MDN meta, and firefox release notes.

---

## Use Case 2: Deprecation Sweep

**Goal**: Audit all deprecated pages and find which still reference browser-compat data.

### Commands & timings

| # | Command | Time | Result |
|---|---------|------|--------|
| 1 | `hyalo find --index $IDX --property 'status=deprecated' --jq '.total'` | 0.101s | 591 |
| 2 | `hyalo find --index $IDX --property 'status=deprecated' --property 'browser-compat' --jq '.total'` | 0.084s | 579 |
| 3 | `hyalo find --index $IDX --property 'status=deprecated' --property 'browser-compat' --fields properties,title -n 10 --format text` | 0.078s | 10 examples |
| 4 | `hyalo find --index $IDX --property 'status=deprecated' --fields properties --jq '..group_by sections..'` | 0.089s | top 10 sections |

**4 commands, ~0.35s total wall-clock**

### Results

- **Total deprecated pages**: 591
- **Deprecated pages with `browser-compat` data**: 579 (98%)
- **Deprecated pages WITHOUT `browser-compat`**: 12 (2%)

**By section** (top 10):
| Section | Count |
|---------|-------|
| web/api | 448 |
| web/javascript | 34 |
| web/css | 31 |
| web/http | 23 |
| web/html | 19 |
| web/svg | 15 |
| mozilla/add-ons | 10 |
| web/mathml | 5 |
| web/accessibility | 3 |
| glossary | 1 |

**10 examples of deprecated + browser-compat**:
1. `extension.getURL()` — webextensions.api.extension.getURL
2. `extension.sendRequest()` — webextensions.api.extension.sendRequest
3. `runtime.onBrowserUpdateAvailable` — webextensions.api.runtime.onBrowserUpdateAvailable
4. `tabs.getAllInWindow()` — webextensions.api.tabs.getAllInWindow
5. `tabs.getSelected()` — webextensions.api.tabs.getSelected
6. `tabs.onActiveChanged` — webextensions.api.tabs.onActiveChanged
7. `tabs.onHighlightChanged` — webextensions.api.tabs.onHighlightChanged
8. `tabs.onSelectionChanged` — webextensions.api.tabs.onSelectionChanged
9. `tabs.sendRequest()` — webextensions.api.tabs.sendRequest
10. `offline_enabled` — webextensions.manifest.offline_enabled

**Insight**: 98% of deprecated pages still carry browser-compat data. The 12 without it (e.g., guides, glossary entries) are the ones that might need a different cleanup approach. The vast majority (448/591) are in web/api.

---

## Use Case 3: CSS Reorg Planning

**Goal**: Understand CSS section structure — subsections, page-types, missing sidebars.

### Commands & timings

| # | Command | Time | Result |
|---|---------|------|--------|
| 1 | `hyalo find --index $IDX --glob 'web/css/**' --fields properties --jq '..group_by top-level..'` | 0.094s | 5 top dirs |
| 2 | `hyalo find --index $IDX --glob 'web/css/reference/**' --fields properties --jq '..group_by page-type..'` | 0.084s | 14 page-types |
| 3 | `hyalo find --index $IDX --glob 'web/css/reference/**' --fields properties --jq '..group_by subdir..'` | 0.092s | 7 subdirs |
| 4 | `hyalo find --index $IDX --glob 'web/css/guides/**' --fields properties --jq '..group_by page-type..'` | 0.078s | 3 page-types |
| 5 | `hyalo find --index $IDX --glob 'web/css/**' --property '!sidebar' --format text` | 0.078s | 0 |

**5 commands, ~0.43s total wall-clock**

### Results

**Top-level structure under `web/css/`**: 1,226 total pages
| Directory | Count |
|-----------|-------|
| reference/ | 1,003 |
| guides/ | 207 |
| how_to/ | 14 |
| tutorials/ | 1 |
| index.md | 1 |

**Reference subsections** (`web/css/reference/`):
| Subdirectory | Count |
|-------------|-------|
| properties/ | 550 |
| values/ | 184 |
| selectors/ | 166 |
| at-rules/ | 100 |
| webkit_extensions/ | 1 |
| mozilla_extensions/ | 1 |
| index.md | 1 |

**Page-types in reference** (14 types):
| Page-type | Count |
|-----------|-------|
| css-property | 469 |
| css-function | 113 |
| css-pseudo-class | 95 |
| css-shorthand-property | 77 |
| css-type | 63 |
| css-pseudo-element | 53 |
| css-media-feature | 42 |
| css-at-rule-descriptor | 33 |
| css-at-rule | 22 |
| css-selector | 9 |
| css-keyword | 9 |
| landing-page | 7 |
| listing-page | 6 |
| css-combinator | 5 |

**Page-types in guides** (3 types):
| Page-type | Count |
|-----------|-------|
| guide | 141 |
| css-module | 65 |
| listing-page | 1 |

**Pages missing `sidebar`**: **0** — all CSS pages have sidebar values set.

**Insight**: The CSS section is well-organized post-reorg. `properties/` is the dominant subsection (55% of reference). All pages have sidebar values. The 65 `css-module` pages in guides represent the module-based organization introduced in the reorg PRs.

---

## Use Case 4: Stale Content Hunt — XMLHttpRequest

**Goal**: Find all pages mentioning XMLHttpRequest, classify them, identify deprecated ones.

### Commands & timings

| # | Command | Time | Result |
|---|---------|------|--------|
| 1 | `hyalo find --index $IDX "XMLHttpRequest" --jq '.total'` | 1.323s | 173 |
| 2 | `hyalo find --index $IDX "XMLHttpRequest" --fields properties --jq '..group_by page-type..'` | 1.349s | 26 page-types |
| 3 | `hyalo find --index $IDX "XMLHttpRequest" --fields properties,title --jq '[.results[] | {file, type, status, title}]'` | 1.339s | full list |
| 4 | `hyalo find --index $IDX "XMLHttpRequest" --property 'status=deprecated' --fields properties,title --format text` | 0.097s | 6 pages |

**4 commands, ~4.11s total wall-clock**

### Results

**Total pages mentioning XMLHttpRequest**: 173

**Classification**:
| Category | Page-types | Count |
|----------|-----------|-------|
| Guides & tutorials | guide, learn-module-chapter, learn-module, tutorial-chapter, mdn-writing-guide | 40 |
| API reference | web-api-interface, web-api-instance-method, web-api-instance-property, web-api-event, web-api-constructor, web-api-overview | 74 |
| Firefox release notes | firefox-release-notes | 32 |
| HTTP reference | http-header, http-cors-error, http-csp-directive, http-permissions-policy-directive | 11 |
| Other (glossary, JS, HTML, webextensions, landing) | various | 16 |

**Deprecated pages mentioning XMLHttpRequest**: **6 pages** (all related to Attribution Reporting API, which was deprecated):
1. `Web/API/Attribution_Reporting_API` — web-api-overview
2. `Web/API/Attribution_Reporting_API/Registering_sources` — guide
3. `Web/API/Attribution_Reporting_API/Registering_triggers` — guide
4. `Web/API/HTMLScriptElement/attributionSrc` — web-api-instance-property
5. `Web/API/XMLHttpRequest/setAttributionReporting` — web-api-instance-method
6. `Web/HTTP/Reference/Headers/Permissions-Policy/attribution-reporting` — http-permissions-policy-directive

**Insight**: The 40 guide/tutorial pages are the prime candidates for modernization (replacing XMLHttpRequest references with Fetch API). The 74 API reference pages are mostly the XMLHttpRequest API's own docs — expected. The 32 Firefox release notes are historical and don't need updating. The 6 deprecated pages are all from the Attribution Reporting API (recently deprecated) and mention XHR incidentally.

---

## Final Summary

| Use Case | Wall-clock Time | Hyalo Commands | Key Finding |
|----------|----------------|----------------|-------------|
| 1. Title Cleanup ("The " prefix) | 0.33s | 4 | 12 pages across MDN, only 1 in web/api |
| 2. Deprecation Sweep | 0.35s | 4 | 591 deprecated, 98% have browser-compat |
| 3. CSS Reorg Planning | 0.43s | 5 | 1,226 pages, well-organized, 0 missing sidebars |
| 4. Stale Content Hunt (XHR) | 4.11s | 4 | 173 mentions, 40 guides to modernize, 6 deprecated |
| **Setup (index creation)** | **2.53s** | **1** | **14,245 files indexed** |
| **TOTAL** | **~7.75s** | **18** | |

**Notes**:
- UC4 is the slowest because it requires full-text body search across 14K files (~1.3s per query). The other use cases use property/title filters which are metadata-only and resolve in <0.1s from the index.
- The index pays for itself immediately: 18 queries would have required 18 full disk scans (~2.5s each = ~45s) without the index. With the index, total query time is ~5.2s (plus 2.5s index build = 7.7s total). **~6x faster than without index.**
- All queries returned results in under 1.4s. Property/metadata queries complete in 0.08–0.10s.
