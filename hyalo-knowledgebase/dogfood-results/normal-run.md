# Normal Run (built-in tools only)
Date: 2026-03-30

---

## UC1: Title Cleanup — "The " Prefix in Titles

**Task:** Find all pages with titles starting with "The ".

**Commands & Timings:**
- `grep -rn '^title: The ' files/en-us/web/api/ --include='*.md'` — 0.792s (1 result)
- `grep -rn '^title: The ' files/en-us/ --include='*.md'` — 2.428s (12 results)

**Results:** 12 pages total across 10 sections.

| Section | Count |
|---------|-------|
| learn_web_development/extensions | 3 |
| web/javascript | 1 |
| web/api | 1 |
| mozilla/firefox | 1 |
| mdn/writing_guidelines | 1 |
| mdn/kitchensink | 1 |
| learn_web_development/getting_started | 1 |
| learn_web_development/core | 1 |
| glossary/khronos | 1 |
| games/tutorials | 1 |

**All 12 pages with slugs:**
1. Mozilla/Firefox/Releases/4/The_add-on_bar — The add-on bar
2. Web/API/Web_Workers_API/Structured_clone_algorithm — The structured clone algorithm
3. Web/JavaScript/Reference/Functions/arguments — The arguments object
4. Learn_web_development/Core/Styling_basics/Box_model — The box model
5. Learn_web_development/Extensions/Forms/HTML5_input_types — The HTML5 input types
6. Learn_web_development/Extensions/Performance/business_case_for_performance — The business case for web performance
7. Learn_web_development/Extensions/Performance/why_web_performance — The "why" of web performance
8. Learn_web_development/Getting_started/Web_standards/The_web_standards_model — The web standards model
9. MDN/Writing_guidelines/Page_structures/Page_types/Page_type_key — The page-type front matter key
10. MDN/Kitchensink — The MDN Content Kitchensink
11. Glossary/Khronos — The Khronos Group
12. Games/Tutorials/2D_breakout_game_Phaser/The_score — The score

**Tool calls:** 3 (Bash)

---

## UC2: Deprecation Sweep

**Task:** Find all deprecated pages, check which reference browser-compat data.

**Commands & Timings:**
- `grep -rl '^  - deprecated' files/en-us/ --include='*.md'` — 2.063s (592 results)
- `grep -l 'browser-compat:' <deprecated files>` — 0.027s (580 results)

**Results:**
- **592** deprecated pages total
- **580** (98%) also reference browser-compat data
- Only 12 deprecated pages lack browser-compat references

**By section:**
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
| mdn/writing_guidelines | 1 |

**10 example deprecated pages with browser-compat:**
1. Mozilla/Add-ons/WebExtensions/manifest.json/offline_enabled
2. Mozilla/Add-ons/WebExtensions/API/tabs/sendRequest — tabs.sendRequest()
3. Mozilla/Add-ons/WebExtensions/API/tabs/onHighlightChanged
4. Mozilla/Add-ons/WebExtensions/API/tabs/onActiveChanged
5. Mozilla/Add-ons/WebExtensions/API/tabs/onSelectionChanged
6. Mozilla/Add-ons/WebExtensions/API/tabs/getAllInWindow — tabs.getAllInWindow()
7. Mozilla/Add-ons/WebExtensions/API/tabs/getSelected — tabs.getSelected()
8. Mozilla/Add-ons/WebExtensions/API/extension/sendRequest — extension.sendRequest()
9. Mozilla/Add-ons/WebExtensions/API/extension/getURL — extension.getURL()
10. Mozilla/Add-ons/WebExtensions/API/runtime/onBrowserUpdateAvailable

**Tool calls:** 3 (Bash)

---

## UC3: CSS Reorg Planning

**Task:** Understand CSS section structure: subsections, page-types, missing sidebars.

**Commands & Timings:**
- `ls files/en-us/web/css/` — 0.003s
- `grep '^page-type:' ... | sort | uniq -c` — 0.152s
- `grep '^sidebar:' ... | sort | uniq -c` — 0.137s
- `grep -rL '^sidebar:' ...` — 0.148s
- `find files/en-us/web/css/reference -maxdepth 1` — 0.156s

**Top-level structure:** `guides/`, `how_to/`, `index.md`, `reference/`, `tutorials/`

**Largest subsections under web/css/reference/:**
| Subsection | Pages |
|------------|-------|
| properties | 550 |
| values | 184 |
| selectors | 166 |
| at-rules | 100 |
| webkit_extensions | 1 |
| mozilla_extensions | 1 |
| **Total under reference/** | **1,003** |

**Page-types (1,226 total pages):**
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
| css-at-rule | 22 |
| landing-page | 10 |
| how-to | 10 |
| css-selector | 9 |
| css-keyword | 9 |
| listing-page | 8 |
| css-combinator | 5 |

**Sidebar:** All 1,226 pages have `sidebar: cssref`. **No pages missing a sidebar value.**

**Tool calls:** 4 (Bash)

---

## UC4: Stale Content Hunt — XMLHttpRequest

**Task:** Find pages mentioning XMLHttpRequest, categorize by type and deprecation.

**Commands & Timings:**
- `grep -rl 'XMLHttpRequest' files/en-us/ --include='*.md'` — 2.074s (173 results)
- `grep -l '^page-type: web-api'` — 0.020s
- `grep -l '^page-type: guide'` — 0.016s
- `grep -l '^  - deprecated'` — 0.017s
- Page-type breakdown — instant

**Results: 173 pages mention XMLHttpRequest**

**By page-type:**
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
| web-api-constructor | 3 |
| http-cors-error | 3 |
| glossary-definition | 3 |
| Other (13 types) | 17 |

**Summary:**
- **74 API reference pages** (web-api-*)
- **33 guide pages**
- **6 deprecated pages** (all Attribution Reporting API related, not XHR itself)

**Deprecated pages mentioning XMLHttpRequest:**
1. Web/HTTP/Reference/Headers/Permissions-Policy/attribution-reporting
2. Web/API/XMLHttpRequest/setAttributionReporting
3. Web/API/HTMLScriptElement/attributionSrc
4. Web/API/Attribution_Reporting_API/Registering_triggers
5. Web/API/Attribution_Reporting_API
6. Web/API/Attribution_Reporting_API/Registering_sources

**Tool calls:** 4 (Bash)

---

## Final Summary

| Use Case | Description | Wall Clock | Tool Calls |
|----------|-------------|------------|------------|
| UC1 | Title cleanup ("The " prefix) | ~3.5s | 3 |
| UC2 | Deprecation sweep | ~3.5s | 3 |
| UC3 | CSS reorg planning | ~0.6s | 4 |
| UC4 | Stale content (XMLHttpRequest) | ~2.1s | 4 |
| **Total** | | **~9.7s** | **14** |

All timings are grep/find wall-clock times only (exclude tool invocation overhead).
Total Bash tool invocations across the session: ~14.
