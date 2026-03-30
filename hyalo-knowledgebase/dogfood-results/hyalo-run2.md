# Hyalo Dogfood Run 2 — MDN Maintenance Tasks

Date: 2026-03-30
Repo: /Users/james/devel/mdn (14,245 files)
Dir config: `dir = "files/en-us"` in .hyalo.toml

---

## Use Case 1: Title Cleanup — Find "The " prefix titles

**Goal:** Find Web API overview pages with titles starting with "The ".

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find --property 'title~=/^The /' --format text` | 1.489s |
| 2 | `hyalo find --property 'title~=/^The /' --fields properties --no-hints --jq '..group_by page-type..'` | 1.790s |
| 3 | `hyalo find --property 'title~=/^The /' --fields properties --no-hints --jq '..first 20 slugs..'` | 1.061s |

**Total hyalo commands:** 3
**Wall-clock:** ~4.3s

### Results

**12 pages** have titles starting with "The ":

| page-type | count |
|-----------|-------|
| guide | 4 |
| learn-module-chapter | 4 |
| glossary-definition | 1 |
| javascript-language-feature | 1 |
| mdn-writing-guide | 1 |
| tutorial-chapter | 1 |

**Sections:** Spread across Games, Glossary, Learn, MDN, Mozilla/Firefox, Web/API, and Web/JavaScript.

**All 12 pages:**

| Title | Slug |
|-------|------|
| The score | Games/Tutorials/2D_breakout_game_Phaser/The_score |
| The Khronos Group | Glossary/Khronos |
| The box model | Learn_web_development/Core/Styling_basics/Box_model |
| The HTML5 input types | Learn_web_development/Extensions/Forms/HTML5_input_types |
| The business case for web performance | Learn_web_development/Extensions/Performance/business_case_for_performance |
| The "why" of web performance | Learn_web_development/Extensions/Performance/why_web_performance |
| The web standards model | Learn_web_development/Getting_started/Web_standards/The_web_standards_model |
| The MDN Content Kitchensink | MDN/Kitchensink |
| The page-type front matter key | MDN/Writing_guidelines/Page_structures/Page_types/Page_type_key |
| The add-on bar | Mozilla/Firefox/Releases/4/The_add-on_bar |
| The structured clone algorithm | Web/API/Web_Workers_API/Structured_clone_algorithm |
| The arguments object | Web/JavaScript/Reference/Functions/arguments |

**Observation:** None are Web API overview pages (page-type `web-api-overview`). These are mostly guides/tutorials — the PR #43578 may have already cleaned the API ones. The remaining "The " titles in guides/learn may be intentional.

---

## Use Case 2: Deprecation Sweep

**Goal:** Audit deprecated pages and find which ones still have `browser-compat` data.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find --property 'status=deprecated' --format text` | (large output, see persisted file) |
| 2 | `hyalo find --property 'status=deprecated' --no-hints --jq '.total'` | 1.519s |
| 3 | `hyalo find --property 'status=deprecated' --property 'browser-compat' --fields properties --no-hints --jq '..10 examples..'` | 1.065s |

**Total hyalo commands:** 3
**Wall-clock:** ~2.6s (commands 2+3; command 1 ran in parallel with UC1)

### Results

- **591 deprecated pages** total
- **579 of those (98%) still have `browser-compat` data** — nearly all deprecated pages still reference compat tables

**10 examples of deprecated + browser-compat overlap:**

| Title | Slug | browser-compat |
|-------|------|----------------|
| extension.getURL() | Mozilla/Add-ons/WebExtensions/API/extension/getURL | webextensions.api.extension.getURL |
| extension.sendRequest() | Mozilla/Add-ons/WebExtensions/API/extension/sendRequest | webextensions.api.extension.sendRequest |
| runtime.onBrowserUpdateAvailable | Mozilla/Add-ons/WebExtensions/API/runtime/onBrowserUpdateAvailable | webextensions.api.runtime.onBrowserUpdateAvailable |
| tabs.getAllInWindow() | Mozilla/Add-ons/WebExtensions/API/tabs/getAllInWindow | webextensions.api.tabs.getAllInWindow |
| tabs.getSelected() | Mozilla/Add-ons/WebExtensions/API/tabs/getSelected | webextensions.api.tabs.getSelected |
| tabs.onActiveChanged | Mozilla/Add-ons/WebExtensions/API/tabs/onActiveChanged | webextensions.api.tabs.onActiveChanged |
| tabs.onHighlightChanged | Mozilla/Add-ons/WebExtensions/API/tabs/onHighlightChanged | webextensions.api.tabs.onHighlightChanged |
| tabs.onSelectionChanged | Mozilla/Add-ons/WebExtensions/API/tabs/onSelectionChanged | webextensions.api.tabs.onSelectionChanged |
| tabs.sendRequest() | Mozilla/Add-ons/WebExtensions/API/tabs/sendRequest | webextensions.api.tabs.sendRequest |
| offline_enabled | Mozilla/Add-ons/WebExtensions/manifest.json/offline_enabled | webextensions.manifest.offline_enabled |

**Observation:** The 98% overlap is expected — deprecated doesn't mean the compat data should be removed. The compat tables show *when* features were deprecated. Cleanup priority should focus on deprecated pages with outdated prose, not just the presence of compat data.

---

## Use Case 3: CSS Reorg Planning

**Goal:** Understand CSS section structure, page types, and sidebar coverage.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find --glob 'web/css/**' --fields properties --no-hints --jq '..page_types..'` | 0.654s |
| 2 | `hyalo find --glob 'web/css/**' --fields properties --no-hints --jq '..subsections..'` | 0.657s |
| 3 | `hyalo find --glob 'web/css/**' --property '!sidebar' --no-hints --jq '..count..'` | 0.649s |

**Total hyalo commands:** 3
**Wall-clock:** ~2.0s

### Results

**1,226 total CSS pages.**

**Subsections by size (under Web/CSS/):**

| Section | Pages |
|---------|-------|
| Reference | 1,003 |
| Guides | 207 |
| How_to | 14 |
| Tutorials | 1 |
| root (index) | 1 |

**Page types:**

| page-type | count |
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
| how-to | 10 |
| landing-page | 10 |
| css-keyword | 9 |
| css-selector | 9 |
| listing-page | 8 |
| css-combinator | 5 |

**Sidebar coverage:** **0 pages missing `sidebar`** — all CSS pages have a sidebar value set.

---

## Use Case 4: Stale Content Hunt (XMLHttpRequest)

**Goal:** Find pages mentioning XMLHttpRequest, categorize by type, check for deprecated ones.

### Commands & Timings

| # | Command | Time |
|---|---------|------|
| 1 | `hyalo find "XMLHttpRequest" --format text` | (large output, persisted) |
| 2 | `hyalo find "XMLHttpRequest" --no-hints --jq '.total'` | 2.034s |
| 3 | `hyalo find "XMLHttpRequest" --fields properties --no-hints --jq '..page-type groups..'` | 1.880s |
| 4 | `hyalo find "XMLHttpRequest" --property 'status=deprecated' --fields properties --no-hints --jq '..pages..'` | 1.424s |

**Total hyalo commands:** 4
**Wall-clock:** ~5.3s

### Results

**173 pages** mention XMLHttpRequest in their body text.

**By page-type (guides vs API ref):**

| Type | Count | Category |
|------|-------|----------|
| guide | 33 | Guide |
| firefox-release-notes | 32 | Historical |
| web-api-instance-method | 20 | API ref |
| web-api-instance-property | 19 | API ref |
| web-api-interface | 14 | API ref |
| web-api-overview | 10 | API ref |
| web-api-event | 8 | API ref |
| http-header | 6 | HTTP ref |
| learn-module-chapter | 5 | Guide |
| Other (16 types) | 26 | Mixed |

**Summary:** ~38 guides, ~71 API reference pages, ~32 release notes, ~32 other.

**Deprecated pages mentioning XMLHttpRequest: 6**

| Title | Slug |
|-------|------|
| Attribution Reporting API | Web/API/Attribution_Reporting_API |
| Registering attribution sources | Web/API/Attribution_Reporting_API/Registering_sources |
| Registering attribution triggers | Web/API/Attribution_Reporting_API/Registering_triggers |
| HTMLScriptElement: attributionSrc property | Web/API/HTMLScriptElement/attributionSrc |
| XMLHttpRequest: setAttributionReporting() method | Web/API/XMLHttpRequest/setAttributionReporting |
| Permissions-Policy: attribution-reporting directive | Web/HTTP/Reference/Headers/Permissions-Policy/attribution-reporting |

**Observation:** The 6 deprecated pages are all Attribution Reporting API related, not XMLHttpRequest itself. The 33 guides mentioning XHR are the top priority for modernization (migrating to Fetch API references). The 32 Firefox release notes are historical and can be left as-is.

---

## Final Summary

| Use Case | Description | hyalo commands | Wall-clock |
|----------|-------------|----------------|------------|
| 1 | Title Cleanup ("The " prefix) | 3 | ~4.3s |
| 2 | Deprecation Sweep | 3 | ~2.6s |
| 3 | CSS Reorg Planning | 3 | ~2.0s |
| 4 | Stale Content Hunt (XMLHttpRequest) | 4 | ~5.3s |
| **Total** | | **13** | **~14.2s** |

### Notes

- **Index path bug:** Initial CSS queries used `files/en-us/web/css/**` as glob but hyalo's `dir` is already `files/en-us`, so the correct glob is `web/css/**`. Got 0 results until corrected. The error was silent (no warning about zero matches from a broad glob).
- **JSON structure:** `--jq` operates on `{results: [], total: N}` (with `--no-hints`). With hints enabled, it wraps in `{data: {...}, hints: [...]}`. This tripped up the first jq attempts.
- **Performance:** All queries on the 14,245-file vault completed in 0.6–2.0s each. Full-text search (`"XMLHttpRequest"`) was slowest at ~2s. Property-only queries were fastest at ~0.6s (CSS glob) to ~1.5s (full vault).
- **Hints:** The suggested follow-up commands from `--format text` output were useful for exploration but the `--jq` path was more efficient for structured analysis.
