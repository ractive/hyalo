# Hyalo Dogfood Run 4 — MDN Maintenance Tasks

Date: 2026-03-30

---

## Use Case 1: Title Cleanup — "The " prefix

**Goal:** Find all pages with titles starting with "The " — which sections, how many, list first 20 with slugs.

### Commands

```
# 1. Count all pages with "The " title prefix
time hyalo find --property 'title~=/^The /' --no-hints --jq '.total'
→ 12
⏱ 1.819s

# 2. Group by section
time hyalo find --property 'title~=/^The /' --no-hints \
  --jq '[.results[].properties.slug | split("/")[0:3] | join("/")] | group_by(.) | map({section: .[0], count: length}) | sort_by(-.count)'
→ (see results)
⏱ 1.445s

# 3. List all 12 with slugs and titles
time hyalo find --property 'title~=/^The /' --no-hints --fields properties \
  --jq '.results[] | "\(.properties.slug)\t\(.properties.title)"'
→ (12 results listed below)
⏱ 1.100s

# 4. Check if API overview pages still have "The " prefix
time hyalo find --property 'page-type=web-api-overview' --property 'title~=/^The /' --no-hints --jq '.total'
→ 0
⏱ 1.439s

# 5. Check API interface pages
time hyalo find --property 'page-type=web-api-interface' --property 'title~=/^The /' --no-hints --jq '.total'
→ 0
⏱ 1.422s
```

### Results

**Total pages with "The " prefix:** 12

**Sections breakdown:**
| Section | Count |
|---------|-------|
| Learn_web_development | 4 |
| MDN | 2 |
| Games | 1 |
| Glossary | 1 |
| Mozilla | 1 |
| Web/API | 1 |
| Web/JavaScript | 1 |

**All 12 pages:**
| Slug | Title |
|------|-------|
| Games/Tutorials/2D_breakout_game_Phaser/The_score | The score |
| Glossary/Khronos | The Khronos Group |
| Learn_web_development/Core/Styling_basics/Box_model | The box model |
| Learn_web_development/Extensions/Forms/HTML5_input_types | The HTML5 input types |
| Learn_web_development/Extensions/Performance/business_case_for_performance | The business case for web performance |
| Learn_web_development/Extensions/Performance/why_web_performance | The "why" of web performance |
| Learn_web_development/Getting_started/Web_standards/The_web_standards_model | The web standards model |
| MDN/Kitchensink | The MDN Content Kitchensink |
| MDN/Writing_guidelines/Page_structures/Page_types/Page_type_key | The page-type front matter key |
| Mozilla/Firefox/Releases/4/The_add-on_bar | The add-on bar |
| Web/API/Web_Workers_API/Structured_clone_algorithm | The structured clone algorithm |
| Web/JavaScript/Reference/Functions/arguments | The arguments object |

**Note:** Web API overview and interface pages (PR #43578 target) show 0 remaining — cleanup already landed. Remaining 12 are in tutorials, glossary, and meta pages.

**UC1 totals:** 5 hyalo commands, ~7.2s wall-clock

---

## Use Case 2: Deprecation Sweep

**Goal:** Audit all deprecated pages, find which still reference browser-compat data, report overlap with examples.

### Commands

```
# 1. Count all deprecated pages
time hyalo find --property 'status=deprecated' --no-hints --jq '.total'
→ 591
⏱ 2.173s

# 2. Count deprecated pages WITH browser-compat data
time hyalo find --property 'status=deprecated' --property 'browser-compat' --no-hints --jq '.total'
→ 579
⏱ 1.443s

# 3. Count deprecated pages WITHOUT browser-compat (already clean)
time hyalo find --property 'status=deprecated' --property '!browser-compat' --no-hints --jq '.total'
→ 12
⏱ 1.452s

# 4. Group deprecated+compat pages by page-type
time hyalo find --property 'status=deprecated' --property 'browser-compat' --no-hints \
  --jq '[.results[].properties["page-type"]] | group_by(.) | map({type: .[0], count: length}) | sort_by(-.count)'
→ (see results)
⏱ 1.509s

# 5. List 10 Web API deprecated examples
time hyalo find --property 'status=deprecated' --property 'browser-compat' --glob 'web/api/**/*.md' --no-hints --fields properties \
  --jq '.results[:10][] | "\(.properties.slug) | \(.properties["page-type"]) | \(.properties["browser-compat"])"'
→ (see results)
⏱ 0.873s
```

### Results

**Total deprecated pages:** 591
**With browser-compat data (need cleanup review):** 579 (97.9%)
**Without browser-compat (already clean):** 12

**Deprecated+compat breakdown by page-type (top 10):**
| Page Type | Count |
|-----------|-------|
| web-api-instance-property | 232 |
| web-api-instance-method | 121 |
| web-api-interface | 54 |
| web-api-event | 22 |
| javascript-instance-method | 21 |
| css-property | 18 |
| html-element | 18 |
| http-header | 17 |
| svg-attribute | 15 |
| web-api-constructor | 10 |

**10 Web API examples (deprecated with browser-compat):**
| Slug | Type | Browser-Compat Key |
|------|------|--------------------|
| Web/API/Attr/specified | web-api-instance-property | api.Attr.specified |
| Web/API/Attribution_Reporting_API | web-api-overview | html.elements.a.attributionsrc |
| Web/API/AudioListener/setOrientation | web-api-instance-method | api.AudioListener.setOrientation |
| Web/API/AudioListener/setPosition | web-api-instance-method | api.AudioListener.setPosition |
| Web/API/AudioProcessingEvent/AudioProcessingEvent | web-api-constructor | api.AudioProcessingEvent.AudioProcessingEvent |
| Web/API/AudioProcessingEvent | web-api-interface | api.AudioProcessingEvent |
| Web/API/AudioProcessingEvent/inputBuffer | web-api-instance-property | api.AudioProcessingEvent.inputBuffer |
| Web/API/AudioProcessingEvent/outputBuffer | web-api-instance-property | api.AudioProcessingEvent.outputBuffer |
| Web/API/AudioProcessingEvent/playbackTime | web-api-instance-property | api.AudioProcessingEvent.playbackTime |
| Web/API/BaseAudioContext/createScriptProcessor | web-api-instance-method | api.BaseAudioContext.createScriptProcessor |

**UC2 totals:** 5 hyalo commands, ~7.5s wall-clock

---

## Use Case 3: CSS Reorg Planning

**Goal:** Understand CSS section structure — largest subsections, page-types, missing sidebars.

### Commands

```
# 1. Total CSS pages
time hyalo find --glob 'web/css/**/*.md' --no-hints --jq '.total'
→ 1226
⏱ 0.776s

# 2. Top-level CSS subsections
time hyalo find --glob 'web/css/**/*.md' --no-hints \
  --jq '[.results[].properties.slug | split("/") | .[2] // "root"] | group_by(.) | map({subsection: .[0], count: length}) | sort_by(-.count)'
→ Reference: 1003, Guides: 207, How_to: 14, Tutorials: 1, root: 1
⏱ 0.781s

# 3. Reference sub-subsections
time hyalo find --glob 'web/css/reference/**/*.md' --no-hints \
  --jq '[.results[].properties.slug | split("/") | .[3] // "root"] | group_by(.) | map({subsection: .[0], count: length}) | sort_by(-.count)'
→ Properties: 550, Values: 184, Selectors: 166, At-rules: 100, Mozilla_extensions: 1, Webkit_extensions: 1, root: 1
⏱ 0.737s

# 4. Page-types in CSS
time hyalo find --glob 'web/css/**/*.md' --no-hints \
  --jq '[.results[].properties["page-type"]] | group_by(.) | map({type: .[0], count: length}) | sort_by(-.count)'
→ (see results)
⏱ 0.776s

# 5. CSS pages missing sidebar
time hyalo find --glob 'web/css/**/*.md' --property '!sidebar' --no-hints --jq '.total'
→ 0
⏱ 0.700s

# 6. Sidebar values used
time hyalo find --glob 'web/css/**/*.md' --no-hints \
  --jq '[.results[].properties.sidebar] | group_by(.) | map({sidebar: .[0], count: length}) | sort_by(-.count)'
→ cssref: 1226 (100%)
⏱ 0.730s
```

### Results

**Total CSS pages:** 1226

**Top-level subsections:**
| Subsection | Count |
|------------|-------|
| Reference | 1003 |
| Guides | 207 |
| How_to | 14 |
| Tutorials | 1 |
| root | 1 |

**Reference sub-subsections:**
| Subsection | Count |
|------------|-------|
| Properties | 550 |
| Values | 184 |
| Selectors | 166 |
| At-rules | 100 |
| Mozilla_extensions | 1 |
| Webkit_extensions | 1 |

**Page-types (all 17):**
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
| how-to | 10 |
| landing-page | 10 |
| css-keyword | 9 |
| css-selector | 9 |
| listing-page | 8 |
| css-combinator | 5 |

**Sidebar coverage:** 100% — all 1226 pages have `sidebar: cssref`. No missing sidebars.

**UC3 totals:** 6 hyalo commands, ~4.5s wall-clock

---

## Use Case 4: Stale Content Hunt — XMLHttpRequest

**Goal:** Find pages mentioning "XMLHttpRequest" in body, classify as guide vs API reference, check deprecation status.

### Commands

```
# 1. Count all pages mentioning XMLHttpRequest
time hyalo find "XMLHttpRequest" --no-hints --jq '.total'
→ 173
⏱ 2.605s

# 2. Group by page-type
time hyalo find "XMLHttpRequest" --no-hints \
  --jq '[.results[].properties["page-type"]] | group_by(.) | map({type: .[0], count: length}) | sort_by(-.count)'
→ (see results)
⏱ 1.898s

# 3. Count guide pages
time hyalo find "XMLHttpRequest" --property 'page-type=guide' --no-hints --jq '.total'
→ 33
⏱ 1.534s

# 4. Count Web API reference pages
time hyalo find "XMLHttpRequest" --property 'page-type~=/^web-api/' --no-hints --jq '.total'
→ 74
⏱ 1.668s

# 5. Count deprecated pages mentioning XHR
time hyalo find "XMLHttpRequest" --property 'status=deprecated' --no-hints --jq '.total'
→ 6
⏱ 1.479s

# 6. List the deprecated XHR pages
time hyalo find "XMLHttpRequest" --property 'status=deprecated' --no-hints --fields properties \
  --jq '.results[] | "\(.properties.slug) | \(.properties["page-type"])"'
→ (see results)
⏱ 1.425s

# 7. List 10 guide pages
time hyalo find "XMLHttpRequest" --property 'page-type=guide' --no-hints --fields properties \
  --jq '.results[:10][] | .properties.slug'
→ (see results)
⏱ 1.494s
```

### Results

**Total pages mentioning XMLHttpRequest:** 173

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
| glossary-definition | 3 |
| http-cors-error | 3 |
| web-api-constructor | 3 |
| (14 others) | 17 |

**Guide vs API reference:**
- **Guides:** 33 pages
- **Web API reference (all web-api-* types):** 74 pages
- **Other (release notes, HTTP, glossary, etc.):** 66 pages

**Deprecated pages mentioning XHR:** 6 (all Attribution Reporting API pages, not XHR-specific)
| Slug | Type |
|------|------|
| Web/API/Attribution_Reporting_API | web-api-overview |
| Web/API/Attribution_Reporting_API/Registering_sources | guide |
| Web/API/Attribution_Reporting_API/Registering_triggers | guide |
| Web/API/HTMLScriptElement/attributionSrc | web-api-instance-property |
| Web/API/XMLHttpRequest/setAttributionReporting | web-api-instance-method |
| Web/HTTP/Reference/Headers/Permissions-Policy/attribution-reporting | http-permissions-policy-directive |

**Modernization priority:** The 33 guide pages are the best candidates for modernization (replacing XHR references with fetch API). The 74 Web API reference pages are mostly the XHR API's own docs and are expected to mention it. The 6 deprecated pages are Attribution Reporting (not XHR itself).

**UC4 totals:** 7 hyalo commands, ~12.1s wall-clock

---

## Re-run with `--index`

Index creation: `hyalo create-index` → 14,245 files indexed in **2.549s**

All 23 queries re-run identically but with `--index files/en-us/.hyalo-index`. Results unchanged; only timings differ.

### Per-command timing comparison (no-index → indexed)

#### UC1: Title Cleanup (5 commands)
| # | Query | No Index | Indexed | Speedup |
|---|-------|----------|---------|---------|
| 1 | Count "The " titles | 1.819s | 0.084s | **21.7x** |
| 2 | Group by section | 1.445s | 0.080s | **18.1x** |
| 3 | List slug+title | 1.100s | 0.078s | **14.1x** |
| 4 | API overview check | 1.439s | 0.079s | **18.2x** |
| 5 | API interface check | 1.422s | 0.079s | **18.0x** |
| | **UC1 total** | **7.225s** | **0.400s** | **18.1x** |

#### UC2: Deprecation Sweep (5 commands)
| # | Query | No Index | Indexed | Speedup |
|---|-------|----------|---------|---------|
| 1 | Count deprecated | 2.173s | 0.102s | **21.3x** |
| 2 | With browser-compat | 1.443s | 0.093s | **15.5x** |
| 3 | Without browser-compat | 1.452s | 0.078s | **18.6x** |
| 4 | Group by page-type | 1.509s | 0.100s | **15.1x** |
| 5 | 10 Web API examples | 0.873s | 0.085s | **10.3x** |
| | **UC2 total** | **7.450s** | **0.458s** | **16.3x** |

#### UC3: CSS Reorg Planning (6 commands)
| # | Query | No Index | Indexed | Speedup |
|---|-------|----------|---------|---------|
| 1 | Total CSS pages | 0.776s | 0.167s | **4.6x** |
| 2 | Top-level subsections | 0.781s | 0.168s | **4.6x** |
| 3 | Reference sub-subsections | 0.737s | 0.143s | **5.2x** |
| 4 | Page-types | 0.776s | 0.168s | **4.6x** |
| 5 | Missing sidebar | 0.700s | 0.080s | **8.8x** |
| 6 | Sidebar values | 0.730s | 0.165s | **4.4x** |
| | **UC3 total** | **4.500s** | **0.891s** | **5.1x** |

#### UC4: Stale Content Hunt (7 commands)
| # | Query | No Index | Indexed | Speedup |
|---|-------|----------|---------|---------|
| 1 | Count XHR mentions | 2.605s | 1.365s | **1.9x** |
| 2 | Group by page-type | 1.898s | 0.621s | **3.1x** |
| 3 | Guide pages | 1.534s | 0.138s | **11.1x** |
| 4 | Web API reference pages | 1.668s | 0.310s | **5.4x** |
| 5 | Deprecated XHR pages | 1.479s | 0.097s | **15.2x** |
| 6 | List deprecated XHR | 1.425s | 0.093s | **15.3x** |
| 7 | List 10 guide pages | 1.494s | 0.135s | **11.1x** |
| | **UC4 total** | **12.103s** | **2.759s** | **4.4x** |

---

## Final Summary

| Use Case | Commands | No Index | Indexed | Speedup |
|----------|----------|----------|---------|---------|
| UC1: Title cleanup | 5 | 7.2s | 0.4s | **18.1x** |
| UC2: Deprecation sweep | 5 | 7.5s | 0.5s | **16.3x** |
| UC3: CSS reorg planning | 6 | 4.5s | 0.9s | **5.1x** |
| UC4: Stale content (XHR) | 7 | 12.1s | 2.8s | **4.4x** |
| **Total** | **23** | **31.3s** | **4.5s** | **6.9x** |
| +index creation | +1 | — | +2.5s | — |
| **Grand total** | **24** | **31.3s** | **7.0s** | **4.5x** |

### Key Findings

1. **Title cleanup:** Only 12 pages remain with "The " prefix — API overview/interface pages (PR #43578 target) are already clean.
2. **Deprecation sweep:** 591 deprecated pages; 579 (97.9%) still have browser-compat data. Web API properties and methods dominate (353 of 579).
3. **CSS structure:** 1226 pages, well-organized: Reference (1003), Guides (207), How_to (14). 17 distinct page-types. 100% sidebar coverage.
4. **XHR stale content:** 173 pages mention XMLHttpRequest. 33 guides are prime modernization targets. Only 6 deprecated (all Attribution Reporting, not XHR itself).

### Performance observations

- **Index creation:** 2.5s to index 14,245 files — pays for itself after ~2 queries
- **Property-only queries (UC1, UC2):** 15–22x faster with index (~80ms vs ~1.5s). Frontmatter is fully cached in the index, so no disk reads needed.
- **Glob-scoped queries (UC3):** 4.4–8.8x faster. Smaller speedup because glob filtering on 1,226 CSS files was already fast without index.
- **Body text search (UC4):** 1.9–15x faster. The unfiltered full-corpus text search (cmd #1, 1.365s) is the slowest indexed query — the index stores body text but scanning 14k bodies still takes time. Queries that combine text search with property filters (cmds #3–7) benefit enormously because the property filter narrows candidates before body scanning.
- **Overall:** 31.3s → 4.5s (indexed) or 7.0s (including index build). The index is a clear win for any session with more than 2 queries.
