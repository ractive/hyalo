---
title: Hyalo Dogfood Run 3 — MDN Maintenance Use Cases
type: research
date: 2026-03-30
status: archived
tags:
  - dogfooding
---
# Hyalo Dogfood Run 3 — MDN Maintenance Use Cases

**Date:** 2026-03-30
**Repo:** ~/devel/mdn (14,245 indexed files)
**Index creation:** 2.346s

---

## UC1: Title Cleanup — "The " prefix

**Commands:**
1. `hyalo find --index ... --title '~=^The ' --format text --fields title` → 0.087s
2. `hyalo find --index ... --title '~=^The ' --glob 'web/api/**' ...` → 0.086s

**Results:**
- 12 pages total have titles starting with "The "
- Sections: games (1), glossary (1), learn_web_development (4), mdn (2), mozilla (1), web/api (1), web/javascript (1)
- Only 1 in web/api (`The structured clone algorithm`) — PR #43578 likely already cleaned up most API pages
- First 12 (all of them):
  1. games/tutorials/2d_breakout_game_phaser/the_score — "The score"
  2. glossary/khronos — "The Khronos Group"
  3. learn_web_development/core/styling_basics/box_model — "The box model"
  4. learn_web_development/extensions/forms/html5_input_types — "The HTML5 input types"
  5. learn_web_development/extensions/performance/business_case_for_performance — "The business case for web performance"
  6. learn_web_development/extensions/performance/why_web_performance — 'The "why" of web performance'
  7. learn_web_development/getting_started/web_standards/the_web_standards_model — "The web standards model"
  8. mdn/kitchensink — "The MDN Content Kitchensink"
  9. mdn/writing_guidelines/page_structures/page_types/page_type_key — "The page-type front matter key"
  10. mozilla/firefox/releases/4/the_add-on_bar — "The add-on bar"
  11. web/api/web_workers_api/structured_clone_algorithm — "The structured clone algorithm"
  12. web/javascript/reference/functions/arguments — "The arguments object"

**Hyalo commands:** 2 | **Wall-clock:** 0.173s

---

## UC2: Deprecation Sweep

**Commands:**
1. `hyalo find --index ... --property 'status=deprecated' --format json --jq 'length'` → 0.106s (returned 2 — only exact match on array)
2. `hyalo find --index ... --property 'status' --fields properties --format json --jq '...|group_by|...'` → 0.098s (status distribution)
3. `hyalo find --index ... --property 'status' --fields properties --format json --jq '...|select(deprecated)|length'` → 0.114s (591 deprecated)
4. `hyalo find --index ... --property 'status' --fields properties --format json --jq '...|select(deprecated and browser-compat)|length'` → 0.108s (579 with compat)
5. `hyalo find --index ... --property 'status' --fields properties --format json --jq '...|[:10]'` → 0.107s (10 examples)

**Results:**
- **591 deprecated pages** total
- Status value distribution: experimental (1338), deprecated (591), non-standard (387)
- **579 of 591 deprecated pages** (98%) also have `browser-compat` data → high cleanup overlap
- Only 12 deprecated pages lack browser-compat data
- 10 example deprecated pages with browser-compat:
  1. extension.getURL() — webextensions.api.extension.getURL
  2. extension.sendRequest() — webextensions.api.extension.sendRequest
  3. runtime.onBrowserUpdateAvailable — webextensions.api.runtime.onBrowserUpdateAvailable
  4. tabs.getAllInWindow() — webextensions.api.tabs.getAllInWindow
  5. tabs.getSelected() — webextensions.api.tabs.getSelected
  6. tabs.onActiveChanged — webextensions.api.tabs.onActiveChanged
  7. tabs.onHighlightChanged — webextensions.api.tabs.onHighlightChanged
  8. tabs.onSelectionChanged — webextensions.api.tabs.onSelectionChanged
  9. tabs.sendRequest() — webextensions.api.tabs.sendRequest
  10. offline_enabled — webextensions.manifest.offline_enabled

**Note:** `--property 'status~=deprecated'` regex filter returned only 2 results — appears to not match inside YAML arrays. Workaround: filter with `--property 'status'` (existence) + jq post-filter.

**Hyalo commands:** 5 | **Wall-clock:** 0.533s

---

## UC3: CSS Reorg Planning

**Commands:**
1. `hyalo find --index ... --glob 'web/css/**' --fields properties --format json --jq 'subsections'` → 0.093s
2. `hyalo find --index ... --glob 'web/css/**' --fields properties --format json --jq 'page-types'` → 0.092s
3. `hyalo find --index ... --glob 'web/css/**' --property '!sidebar' --format text` → 0.079s

**Results:**
- **1,226 total CSS pages**
- Largest subsections: reference (1003), guides (207), how_to (14), tutorials (1)
- 17 page-types in CSS:
  - css-property (469), guide (143), css-function (113), css-pseudo-class (95)
  - css-shorthand-property (77), css-module (65), css-type (63), css-pseudo-element (53)
  - css-media-feature (42), css-at-rule-descriptor (33), css-at-rule (22), how-to (10)
  - landing-page (10), css-keyword (9), css-selector (9), listing-page (8), css-combinator (5)
- **0 pages missing a sidebar value** — CSS section is fully sidebared

**Hyalo commands:** 3 | **Wall-clock:** 0.264s

---

## UC4: Stale Content Hunt — XMLHttpRequest

**Commands:**
1. `hyalo find --index ... "XMLHttpRequest" --fields properties --format json --jq '.total'` → 1.525s
2. `hyalo find --index ... "XMLHttpRequest" --fields properties --format json --jq 'page-type groups'` → 1.299s
3. `hyalo find --index ... "XMLHttpRequest" --fields properties --format json --jq 'deprecated count'` → 1.234s

**Results:**
- **173 pages** mention "XMLHttpRequest" in body text
- Breakdown: 33 guides, 32 firefox-release-notes, 20 instance-methods, 19 instance-properties, 14 interfaces, 10 API overviews, 8 events, 6 HTTP headers, 5 learn chapters, plus 26 others
- **6 pages** are both deprecated and mention XMLHttpRequest (all Attribution Reporting API):
  1. Attribution Reporting API (overview)
  2. Registering attribution sources
  3. Registering attribution triggers
  4. HTMLScriptElement: attributionSrc property
  5. XMLHttpRequest: setAttributionReporting() method
  6. Permissions-Policy: attribution-reporting directive
- Modernization priority: 33 guides + 5 learn chapters = **38 educational pages** mentioning XHR that could be updated to use Fetch API

**Hyalo commands:** 3 | **Wall-clock:** 4.058s (body text search is slower than property/index queries)

---

## Final Summary

| Use Case | Wall-clock | Hyalo Commands | Key Finding |
|----------|-----------|----------------|-------------|
| UC1: Title Cleanup | 0.173s | 2 | 12 pages with "The " prefix, only 1 in web/api |
| UC2: Deprecation Sweep | 0.533s | 5 | 591 deprecated pages, 98% have browser-compat |
| UC3: CSS Reorg Planning | 0.264s | 3 | 1,226 CSS pages, 17 page-types, 0 missing sidebars |
| UC4: Stale Content Hunt | 4.058s | 3 | 173 pages mention XHR, 6 deprecated, 38 guides |
| **Total** | **5.028s** | **13** | |

**Index creation:** 2.346s (one-time cost, 14,245 files)
**Grand total (including index):** 7.374s

### Observations
- Index-based property/glob queries are extremely fast (~80-100ms)
- Body text search is ~10-15x slower (~1.2-1.5s) — still fast for 14K files
- `--property 'status~=deprecated'` regex doesn't match inside YAML arrays (only 2 of 591). Workaround: use jq post-filter on `--property 'status'` (existence check). Potential hyalo enhancement.
- `--jq` is powerful for aggregation but requires knowing the JSON structure (`{results: [...], total: N}`)
