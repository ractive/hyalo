---
title: Hyalo Dogfood Run — MDN Docs Maintenance
type: research
date: 2026-03-30
status: archived
tags:
  - dogfooding
---
# Hyalo Dogfood Run — MDN Docs Maintenance

**Date:** 2026-03-30
**Repo:** ~/devel/mdn (files/en-us via .hyalo.toml)

---

## Use Case 1: Title Cleanup — "The " Prefix Detection

**Goal:** Find Web API overview pages with titles starting with "The ".

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find --property 'title~=/^The /' --glob 'web/api/**/index.md' --format text --no-hints -n 20` | 1.291s |
| 2 | `hyalo find --property 'title~=/^The /' --no-hints --jq '.total'` | 1.596s |
| 3 | `hyalo find --property 'title~=/^The /' --no-hints --jq '[.results[] \| {file, slug, section}]'` | 1.442s |

### Results

**Total pages with "The " prefix in title: 12**

Only **1** is in Web/API (the structured clone algorithm guide). The rest are spread across:

| Section | Count | Examples |
|---------|-------|----------|
| learn_web_development | 4 | Box model, HTML5 input types, Web performance, Web standards model |
| mdn | 2 | Kitchensink, page-type key guide |
| web | 2 | Structured clone algorithm (API), arguments object (JS) |
| games | 1 | 2D breakout game — The score |
| glossary | 1 | Khronos Group |
| mozilla | 1 | Firefox 4 add-on bar |

**First 20 (all 12) with slugs:**

1. `Games/Tutorials/2D_breakout_game_Phaser/The_score`
2. `Glossary/Khronos`
3. `Learn_web_development/Core/Styling_basics/Box_model`
4. `Learn_web_development/Extensions/Forms/HTML5_input_types`
5. `Learn_web_development/Extensions/Performance/business_case_for_performance`
6. `Learn_web_development/Extensions/Performance/why_web_performance`
7. `Learn_web_development/Getting_started/Web_standards/The_web_standards_model`
8. `MDN/Kitchensink`
9. `MDN/Writing_guidelines/Page_structures/Page_types/Page_type_key`
10. `Mozilla/Firefox/Releases/4/The_add-on_bar`
11. `Web/API/Web_Workers_API/Structured_clone_algorithm`
12. `Web/JavaScript/Reference/Functions/arguments`

**Wall-clock: ~4.3s | Commands: 3**

---

## Use Case 2: Deprecation Sweep

**Goal:** Find all deprecated pages and identify which still have `browser-compat` data.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find --property 'status=deprecated' --no-hints --jq '.total'` | 1.451s |
| 2 | `hyalo find --property 'status=deprecated' --fields properties --no-hints --jq '[...select(.properties["browser-compat"])] \| length'` | 1.010s |
| 3 | `hyalo find --property 'status=deprecated' --fields properties --no-hints --jq '[...] \| .[0:10]'` | 1.065s |

### Results

- **Total deprecated pages: 591**
- **Deprecated pages with `browser-compat`: 579** (98% overlap!)

Almost all deprecated pages still carry compat data. Only 12 deprecated pages lack it.

**10 examples of deprecated + browser-compat:**

| Slug | Compat Key |
|------|-----------|
| `Mozilla/Add-ons/WebExtensions/API/extension/getURL` | `webextensions.api.extension.getURL` |
| `Mozilla/Add-ons/WebExtensions/API/extension/sendRequest` | `webextensions.api.extension.sendRequest` |
| `Mozilla/Add-ons/WebExtensions/API/runtime/onBrowserUpdateAvailable` | `webextensions.api.runtime.onBrowserUpdateAvailable` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/getAllInWindow` | `webextensions.api.tabs.getAllInWindow` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/getSelected` | `webextensions.api.tabs.getSelected` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/onActiveChanged` | `webextensions.api.tabs.onActiveChanged` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/onHighlightChanged` | `webextensions.api.tabs.onHighlightChanged` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/onSelectionChanged` | `webextensions.api.tabs.onSelectionChanged` |
| `Mozilla/Add-ons/WebExtensions/API/tabs/sendRequest` | `webextensions.api.tabs.sendRequest` |
| `Mozilla/Add-ons/WebExtensions/manifest.json/offline_enabled` | `webextensions.manifest.offline_enabled` |

**Wall-clock: ~3.5s | Commands: 3**

---

## Use Case 3: CSS Reorg Planning

**Goal:** Understand CSS section structure, page-types, and sidebar coverage.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo summary --glob 'web/css/**/*.md' --no-hints --format text` | (large output, ~1s) |
| 2 | `hyalo find --glob 'web/css/**/*.md' --fields properties --jq '[page-type distribution]'` | 0.659s |
| 3 | `hyalo find --glob 'web/css/**/*.md' --property '!sidebar' --jq 'count'` | 0.650s |
| 4 | `hyalo find --glob 'web/css/**/*.md' --jq '[top-level dirs by count]'` | 0.829s |

### Results

**Total CSS pages: 1226**

**Largest subsections under web/css/:**

| Subsection | Pages |
|-----------|-------|
| reference/ | 1003 |
| guides/ | 207 |
| how_to/ | 14 |
| tutorials/ | 1 |
| index.md (root) | 1 |

**Page-type distribution (top 10):**

| Page Type | Count |
|-----------|-------|
| css-property | 469 |
| guide | 143 |
| css-function | 113 |
| css-pseudo-class | 95 |
| css-shorthand-property | 77 |
| css-module | 65 |
| css-type | 63 |
| css-pseudo-element | 53 |
| css-media-feature | 42 |
| css-at-rule-descriptor | 33 |

Other types: css-at-rule (22), how-to (10), landing-page (10), css-keyword (9), css-selector (9), listing-page (8), css-combinator (5).

**Pages missing `sidebar`: 0** — All CSS pages have a sidebar value set.

**Wall-clock: ~2.1s | Commands: 4**

---

## Use Case 4: Stale Content Hunt — XMLHttpRequest

**Goal:** Find pages mentioning XMLHttpRequest, categorize by type, check deprecation status.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find -e 'XMLHttpRequest' --no-hints --jq '.total'` | 1.938s |
| 2 | `hyalo find -e 'XMLHttpRequest' --fields properties --jq '{guides, api_ref, deprecated, total}'` | 1.988s |
| 3 | `hyalo find -e 'XMLHttpRequest' --fields properties --jq '[page-type group counts]'` | 2.006s |

### Results

**Total pages mentioning XMLHttpRequest: 173**

**Breakdown by page-type:**

| Page Type | Count |
|-----------|-------|
| guide | 33 |
| firefox-release-notes | 32 |
| web-api-instance-method | 20 |
| web-api-instance-property | 19 |
| web-api-interface | 14 |
| web-api-overview | 10 |
| web-api-event | 8 |
| http-header | 6 |
| learn-module-chapter | 5 |
| Other (16 types) | 26 |

**Guides vs API reference:**
- Guides: 33
- API reference pages (web-api-*): 74
- Remaining 66 are Firefox release notes, HTTP, glossary, learn, etc.

**Deprecated: 0** — None of the 173 pages mentioning XMLHttpRequest are marked deprecated, despite XMLHttpRequest being largely superseded by Fetch API. This suggests these pages may need review for modernization notices.

**Wall-clock: ~5.9s | Commands: 3**

---

## Final Summary

| Use Case | Description | Wall-clock | hyalo Commands |
|----------|-------------|-----------|---------------|
| 1 | Title cleanup ("The " prefix) | ~4.3s | 3 |
| 2 | Deprecation sweep | ~3.5s | 3 |
| 3 | CSS reorg planning | ~2.1s | 4 |
| 4 | Stale content hunt (XMLHttpRequest) | ~5.9s | 3 |
| **Total** | | **~15.8s** | **13** |

All queries ran against ~13,000+ markdown files in the MDN content repo without an index. Average query time: ~1.2s.
