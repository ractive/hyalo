//! Static jq filter strings, one per output shape, and the shape-signature lookup that picks one.
//!
//! Split out of the single 3,744-line `output.rs` in iteration 247
//! (deep-review hotspot). A file split only: every item keeps the visibility it
//! had in the one module, so `output::...` paths and behaviour are unchanged.

// ---------------------------------------------------------------------------
// jq filter constants — one per output type
// ---------------------------------------------------------------------------

/// jq expression that renders one frontmatter value for text output, applied to
/// the value itself (`.value | PROPERTY_VALUE_EXPR`).
///
/// A scalar prints bare. A list prints `[a, b]` — except when any item contains
/// a `[[wikilink]]`, where the items are JSON-quoted instead:
/// `["[[Futurism]]", "[[Nonfiction]]"]`.
///
/// iter-262 (UX-11): the unquoted form collided with the wikilink brackets and
/// produced `genre: [[[Futurism]], [[Nonfiction]]]`, where the list's own
/// brackets and the first link's are indistinguishable and the value reads like
/// a nested list. Quoting only kicks in for wikilink-bearing lists so ordinary
/// tag lists keep their compact rendering.
pub(super) const PROPERTY_VALUE_EXPR: &str = r#"if type == "array" then (if any(.[] | tostring; contains("[[")) then "[" + (map(tostring | tojson) | join(", ")) + "]" else "[" + (map(tostring) | join(", ")) + "]" end) else . end"#;

/// `PropertyInfo` (used by `--fields properties-typed`): `{name, type, value}`
/// When value is an array (list type), join elements with ", " for readability
/// — or as quoted items when they carry wikilinks (see [`PROPERTY_VALUE_EXPR`]).
pub(super) const PROPERTY_INFO_FILTER: &str = r#""\(.name) (\(.type)): \(.value | if type == "array" then (if any(.[] | tostring; contains("[[")) then "[" + (map(tostring | tojson) | join(", ")) + "]" else "[" + (map(tostring) | join(", ")) + "]" end) else . end)""#;

/// `PropertySummaryEntry`: `{count, name, type, mixed_types?}`
///
/// When `mixed_types` is present the type column shows `mixed (N text, M number…)`
/// so the user can see at a glance that the property is not uniformly typed.
pub(super) const PROPERTY_SUMMARY_ENTRY_FILTER: &str = r#""\(.name)\t\(if .mixed_types then "mixed (" + (.mixed_types | map("\(.count) \(.type)") | join(", ")) + ")" else .type end)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

/// `TagSummary`: `{tags, total}`
pub(super) const TAG_SUMMARY_FILTER: &str = r#""\(.total) unique \(if .total == 1 then "tag" else "tags" end)\n\(.tags | map("  \(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)") | join("\n"))""#;

/// `TagSummaryEntry`: `{count, name}`
pub(super) const TAG_SUMMARY_ENTRY_FILTER: &str =
    r#""\(.name)\t\(.count) \(if .count == 1 then "file" else "files" end)""#;

// Every `LinkInfo` text rendering opens with its source line (iter-215,
// dogfood UX-6): `  line 12: …`. `.line` is always present on a `LinkInfo`,
// but JSON written by a pre-215 hyalo (or a hand-built fixture) may omit it,
// so the `// 0` fallbacks in each filter keep those renderable instead of
// failing. `line 0` is the "unknown" rendering — no real source line is 0,
// they are 1-based.

/// Every `LinkInfo` shape from iter-261 on — one filter, not a key-signature
/// family.
///
/// `kind` (UX-6) is always present, and `suggested_fragment` (DEC-268) is a
/// sixth optional key, so enumerating signatures would have meant 48 arms that
/// all render the same line. Missing optional keys read as `null` in jq, so a
/// single filter covers them all:
/// - `"target"`, plus `#fragment` when the link carried one;
/// - `→ "path"` when it resolved;
/// - ` (kind)` for anything that is not a plain `wikilink`;
/// - `(broken anchor)` / `(unresolved)` / `(out of vault)` verdicts — an
///   `external` link gets none of them, because it names nothing to resolve;
/// - the DEC-268 heading suggestion, then a `[label]` suffix.
pub(super) const LINK_INFO_FILTER: &str = r##""  line \(.line // 0): \"\(.target)\(if .fragment then "#\(.fragment)" else "" end)\"\(if .path then " → \"\(.path)\"" else "" end)\(if .kind and .kind != "wikilink" then " (\(.kind))" else "" end)\(if .path then (if .broken_anchor then " (broken anchor)" else "" end) elif .out_of_vault then " (out of vault)" elif .kind == "external" then "" else " (unresolved)" end)\(if .suggested_fragment then " — did you mean \"#\(.suggested_fragment)\"?" else "" end)\(if .label then " [\(.label)]" else "" end)""##;

/// `TaskCount`: `{done, total}`
pub(super) const TASK_COUNT_FILTER: &str = r#""[\(.done)/\(.total)]""#;

/// `OutlineSection` without tasks: `{code_blocks, heading, level, line, links}`
pub(super) const OUTLINE_SECTION_FILTER: &str = r##""\("#" * .level) \(.heading // "(pre-heading)")\(if (.links | length) > 0 then "\n\(.links | map("  → \"\(.)\"") | join("\n"))" else "" end)""##;

/// `OutlineSection` with tasks: `{code_blocks, heading, level, line, links, tasks}`
///
/// NEW-16 (dogfood pre3): `.heading` is the raw markdown text, which may
/// itself already carry a hand-written `[n/m]` count (`## Tasks [6/6]`) —
/// unconditionally appending the computed one doubled it (`## Tasks [6/6]
/// [1/2]`), with the hand-written half free to go stale the moment a task
/// checkbox changed. Strips any such trailing bracket count from the heading
/// before appending the one hyalo actually computed, so it renders once and
/// is always current.
///
/// PR #251 review N15: this filter only ever runs on a section that DOES
/// have `.tasks` (see the `code_blocks,heading,level,line,links,tasks`
/// dispatch key below), so the strip always fires — there is no "no tasks,
/// keep the text" branch here the way `build_file_object_filter`'s inline
/// sections filter has. A heading that ends in bracket text shaped like
/// `[n/m]` for a reason *other* than a task count (`## Aspect Ratio [16/9]`,
/// and it happens to have a task list under it) loses that text: the strip
/// is a plain regex match on shape, not semantics, and cannot tell "this is
/// a stale task count" from "this looks like one." Accepted trade-off — the
/// doubled/stale-count case this filter exists to fix is far more common
/// than a coincidental `[n/m]`-shaped heading suffix on a section that also
/// contains a task list.
pub(super) const OUTLINE_SECTION_WITH_TASKS_FILTER: &str = r##""\("#" * .level) \((.heading // "(pre-heading)") | sub("\\s*\\[[0-9]+/[0-9]+\\]\\s*$"; "")) [\(.tasks.done)/\(.tasks.total)]\(if (.links | length) > 0 then "\n\(.links | map("  → \"\(.)\"") | join("\n"))" else "" end)""##;

/// `TaskInfo`: `{done, line, status, text}`
pub(super) const TASK_INFO_FILTER: &str =
    r#""line \(.line): [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `TaskReadResult`: `{done, file, line, status, text}`
pub(super) const TASK_READ_RESULT_FILTER: &str =
    r#""\"\(.file)\":\(.line) [\(.status)] \(.text)\(if .done then " (done)" else "" end)""#;

/// `TaskDryRunResult`: `{done, file, line, old_status, status, text}`
/// Format: `"file":line [old] -> [new] text` — makes the direction of change
/// explicit for `task toggle --dry-run`.
pub(super) const TASK_DRY_RUN_RESULT_FILTER: &str =
    r#""\"\(.file)\":\(.line) [\(.old_status)] -> [\(.status)] \(.text)""#;

/// `VaultSummary`: `{dir, dead_ends, files, links, orphans, properties, recent_files, status, tags, tasks}`
/// Compact single-line-per-section format (~20-30 lines regardless of vault size).
///
/// iter-247: `.dir` is deliberately **not** rendered here. It used to lead the
/// text report as a `kb dir: <path>` banner — the only command that prefixed its
/// text output with the vault it had just resolved — which put a
/// cwd-dependent path on stdout for anyone scripting `--format text`. The vault
/// dir now goes to stderr as a `note:` (see `commands::summary::summary`),
/// matching how `--dir`-switching and other resolution context is already
/// announced, and stays in the JSON payload as `.dir` for machine consumers.
pub(super) const VAULT_SUMMARY_FILTER: &str = r#""Files: \(.files.total)\(if (.files.skipped // 0) > 0 or (.files.excluded // 0) > 0 then " (\(.files.skipped // 0) skipped, \(.files.excluded // 0) excluded)" else "" end)\nDirectories: \(if (.files.directories | length) > 0 then (.files.directories | .[:7] | map("\(.directory)/ (\(.count))") | join(", ")) + (if (.files.directories | length) > 7 then ", ..." else "" end) else "(none)" end)\nProperties: \(.properties | length) — \(if (.properties | length) > 0 then (.properties | sort_by(-.count) | .[:7] | map("\(.name) (\(.count))") | join(", ")) + (if (.properties | length) > 7 then ", ..." else "" end) else "(none)" end)\nTags: \(.tags.total) — \(if (.tags.tags | length) > 0 then (.tags.tags | .[:7] | map("\(.name) (\(.count))") | join(", ")) + (if (.tags.tags | length) > 7 then ", ..." else "" end) else "(none)" end)\nTasks: \(.tasks.done)/\(.tasks.total)\nLinks: \(.links.total) total, \(.links.broken) broken\(if .links.broken_anchors > 0 then ", \(.links.broken_anchors) broken anchor\(if .links.broken_anchors == 1 then "" else "s" end)" else "" end)\(if .links.out_of_vault > 0 then ", \(.links.out_of_vault) out of vault" else "" end)\nOrphans: \(.orphans)\nDead-ends: \(.dead_ends)\nStatus: \(if (.status | length) > 0 then (.status | sort_by(-.count) | map("\(.value) (\(.count))") | join(", ")) else "(none)" end)\nRecent: \(if (.recent_files | length) > 0 then (.recent_files | map(.path) | join(", ")) else "(none)" end)""#;

/// `FindTaskInfo`: `{done, line, section, status, text}`
/// Format: `  [x] text (line N, section)` or `  [ ] text (line N, section)`
pub(super) const FIND_TASK_INFO_FILTER: &str =
    r#""  [\(if .done then "x" else " " end)] \(.text) (line \(.line), \(.section))""#;

/// `ContentMatch`: `{line, section, text}`
/// Format: `  line N (section): text`
pub(super) const CONTENT_MATCH_FILTER: &str = r#""  line \(.line) (\(.section)): \(.text)""#;

/// Mutation result with `property` + `value` fields:
/// covers `SetPropertyResult`, `AppendPropertyResult`, and `RemovePropertyResult` (with value).
/// Key signature: `dry_run,modified,property,scanned,skipped,total,value` (without note)
/// or `dry_run,modified,note,property,scanned,skipped,total,value` (with note, alphabetically sorted).
/// Format: `[dry-run] property=value: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
/// Appends `  note: <msg>` when a `note` field is present.
pub(super) const PROPERTY_VALUE_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.property)=\(.value): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)\(if .note then "\n  note: \(.note)" else "" end)""#;

/// Mutation result with `property` only (no value field):
/// covers `RemovePropertyResult` (without value).
/// Key signature: `dry_run,modified,property,scanned,skipped,total`
/// Format: `[dry-run] property: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
pub(super) const PROPERTY_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.property): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// Mutation result with `tag` field:
/// covers `SetTagResult` and `RemoveTagResult`.
/// Key signature: `dry_run,modified,scanned,skipped,tag,total`
/// Format: `[dry-run] tag: N/T modified (S scanned)` when dry-run; omits prefix otherwise.
/// Appends `(S scanned)` when not all scanned files were processed (e.g. where-filters).
pub(super) const TAG_MUTATION_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.tag): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// `RenamePropertyResult` (`properties rename`).
/// Key signature: `conflicts,dry_run,from,modified,scanned,skipped_count,to,total`
/// Format: `[dry-run] from → to: N/T modified`, the modified files, then any
/// conflicts (files that already carried the target key).
pub(super) const PROPERTY_RENAME_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.from) → \(.to): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)\(if (.conflicts | length) > 0 then "\nconflicts: \(.conflicts | join(", "))" else "" end)""#;

/// `RenameTagResult` (`tags rename`).
/// Key signature: `dry_run,from,modified,renamed_tags,scanned,skipped_count,to,total`
/// Format: the headline pair, then one line per tag the rename actually
/// touched — iter-266 DEC-282 expands a parent rename over its whole subtree,
/// so `music → audio` alone would hide that `music/genres` moved too.
pub(super) const TAG_RENAME_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)\(.from) → \(.to): \(.modified | length)/\(.total) modified\(if .scanned != .total then " (\(.scanned) scanned)" else "" end)\(if (.renamed_tags | length) > 0 then "\n\(.renamed_tags | map("  \(.from) → \(.to) (\(.files) file\(if .files == 1 then "" else "s" end))") | join("\n"))" else "" end)\(if (.modified | length) > 0 then "\n\(.modified | map("  \"\(.)\"") | join("\n"))" else "" end)""#;

/// `BacklinksResult`: `{file, backlinks: [...]}`
/// Format: `N backlink(s) for "file"` with each backlink listed as `  source.md: line N`.
/// Empty case: `No backlinks found for "file"`.
pub(super) const BACKLINKS_RESULT_FILTER: &str = r#"if (.backlinks | length) == 0 then "No backlinks found for \"\(.file)\"" else "\(.backlinks | length) \(if (.backlinks | length) == 1 then "backlink" else "backlinks" end) for \"\(.file)\"\n\(.backlinks | map("  \(.source): line \(.line)\(if .kind == "frontmatter" then " [frontmatter: \(.property)]" else "" end)") | join("\n"))" end"#;

/// `LinksFix result`: `{ambiguous, ambiguous_links, applied, applied_fixes, broken, broken_anchors, case_mismatch_fixes, case_mismatches, failed, failed_fixes, fixable, fixes, ignored, relocation_fixes, relocations, unapplied, unapplied_fixes, unfixable, unfixable_links}`
/// Format: summary line with fix status. Includes case-mismatch, relocation, and ambiguous counts when non-zero.
/// On `--apply`, the per-fix detail lines show only fixes that were actually
/// written to disk (`applied_fixes`); on dry-run they show the full plan
/// (`fixes`), since nothing has been attempted yet. A non-zero `unapplied`
/// count (fixes whose on-disk text no longer matched the plan) gets its own
/// section so a stale or partially-applied run is never silently reported as
/// fully applied.
///
/// iter-210 (UX-4) changed three things about the layout:
/// 1. the `fuzzy` bucket has a **count** line next to `Fixable`/`Unfixable`, so
///    the summary block accounts for every broken link. Without it a large
///    vault showed `6098 broken` over `25 fixable + 1400 unfixable` and left
///    the reader to guess where the other 4,673 went.
/// 2. `unfixable_links` and `out_of_vault_links` — previously JSON-only — are
///    listed in text too, capped at 20 entries with an "and N more" footer so
///    the actionable buckets stay readable on a vault with thousands.
/// 3. the fuzzy per-fix listing moved to the **end** of the report. It is the
///    longest section by far and used to bury every actionable bucket under it.
///
/// NEW-13 (dogfood pre3) adds a `Relocations` count/section, separate from
/// `Case mismatches`: a bare-stem link resolved to a different directory is a
/// move, not a casing fix, and had been silently folded into the case-mismatch
/// count.
pub(super) const LINKS_FIX_FILTER: &str = r#""Broken links: \(.broken)\(if .broken_anchors > 0 then "\n\(.broken_anchors) broken anchor(s) — see `find --broken-links`" else "" end)\nFixable: \(.fixable)\(if .fuzzy > 0 then "\nLow-confidence matches (\(if .fuzzy_applied then "applied via --apply-fuzzy" else "excluded from plain --apply" end)): \(.fuzzy)" else "" end)\nUnfixable: \(.unfixable)\nIgnored: \(.ignored)\(if .case_mismatches > 0 then "\nCase mismatches: \(.case_mismatches)" else "" end)\(if .relocations > 0 then "\nRelocations: \(.relocations)" else "" end)\(if .ambiguous > 0 then "\nAmbiguous (short-form): \(.ambiguous)" else "" end)\(if .out_of_vault > 0 then "\nOut of vault (target above vault root): \(.out_of_vault)" else "" end)\(if .templated > 0 then "\nTemplated (dynamic destination, never rewritten): \(.templated)" else "" end)\(if .failed > 0 then "\nFailed (write error): \(.failed)\n\(.failed_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\" [\(.error)]") | join("\n"))" else "" end)\nApplied: \(if .applied then "yes (\(.applied_fixes | length) fix\(if (.applied_fixes | length) == 1 then "" else "es" end))" else if .dry_run then "no (dry run)" else "no (no fixes written — nothing to apply)" end end)\(if .applied then "\(if (.applied_fixes | length) > 0 then "\n\(.applied_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\"") | join("\n"))" else "" end)\(if .unapplied > 0 then "\nUnapplied (plan did not match on-disk text): \(.unapplied)\n\(.unapplied_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\"") | join("\n"))" else "" end)" else "\(if (.fixes | length) > 0 then "\n\(.fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\"") | join("\n"))" else "" end)" end)\(if (.unfixable_links | length) > 0 then "\nUnfixable links (no candidate in the vault):\n\(.unfixable_links | .[:20] | map("  \(.source) line \(.line): \"\(.target)\"") | join("\n"))\(if (.unfixable_links | length) > 20 then "\n  … and \((.unfixable_links | length) - 20) more (use --format json for the full list)" else "" end)" else "" end)\(if (.out_of_vault_links | length) > 0 then "\nOut-of-vault links (target above vault root, never rewritten):\n\(.out_of_vault_links | .[:20] | map("  \(.source) line \(.line): \"\(.target)\"") | join("\n"))\(if (.out_of_vault_links | length) > 20 then "\n  … and \((.out_of_vault_links | length) - 20) more (use --format json for the full list)" else "" end)" else "" end)\(if (.case_mismatch_fixes | length) > 0 then "\nCase-mismatch fixes:\n\(.case_mismatch_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\" [\(.rule // "link-case-mismatch")]") | join("\n"))" else "" end)\(if (.relocation_fixes | length) > 0 then "\nRelocation fixes:\n\(.relocation_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\" [\(.rule // "shortest-path")]") | join("\n"))" else "" end)\(if (.ambiguous_links | length) > 0 then "\nAmbiguous links:\n\(.ambiguous_links | map("  \(.source) line \(.line): \"\(.target)\" [ambiguous]") | join("\n"))" else "" end)\(if (.templated_links | length) > 0 then "\nTemplated links (dynamic destination, never rewritten):\n\(.templated_links | map("  \(.source) line \(.line): \"\(.target)\" [templated]") | join("\n"))" else "" end)\(if (.fuzzy_fixes | length) > 0 then "\nLow-confidence matches (\(if .fuzzy_applied then "applied at or above confidence \(.fuzzy_min_confidence)" else "not applied — pass --apply-fuzzy" end)):\n\(.fuzzy_fixes | map("  \(.source) line \(.line): \"\(.old_target)\" → \"\(.new_target)\" [\(.rule // "fuzzy-match") \((.confidence * 1000 | floor) / 1000)]\(if .below_floor then " — below floor" else "" end)") | join("\n"))\(if .fuzzy_below_floor > 0 then "\n  \(.fuzzy_below_floor) of \(.fuzzy_fixes | length) below the confidence floor \(.fuzzy_min_confidence) — raise or lower it with --min-confidence <0.0-1.0>" else "" end)" else "" end)""#;

/// `LinksAuto result`: `{ambiguous_titles, applied, apply_outcomes, files_applied, files_failed, files_skipped, matches, scanned, total}`
/// plus optional `config_excluded_titles` / `config_excluded_mentions`
/// (iter-195a, renamed and paired in iter-213), present only when
/// `[links.auto]` config exclusions removed candidate titles — hence the
/// `// 0` fallbacks in the filter.
/// Format: summary line + per-match details.
pub(super) const LINKS_AUTO_FILTER: &str = r#""\(.matched) unlinked mention\(if .matched == 1 then "" else "s" end) found in \(.matches | map(.file) | unique | length) file\(if (.matches | map(.file) | unique | length) == 1 then "" else "s" end) (\(.scanned) scanned)\(if (.ambiguous_titles | length) > 0 then " (\(.ambiguous_titles | length) ambiguous title\(if (.ambiguous_titles | length) == 1 then "" else "s" end) skipped)" else "" end)\(if (.config_excluded_titles // 0) > 0 then "\nExcluded by [links.auto] config: \(.config_excluded_titles) title\(if .config_excluded_titles == 1 then "" else "s" end), suppressing \(.config_excluded_mentions // 0) mention\(if (.config_excluded_mentions // 0) == 1 then "" else "s" end)" else "" end)\nApplied: \(if .applied then "yes" else "no" end)\(if (.files_failed + .files_skipped) > 0 then "\nWrites: \(.files_applied) applied, \(.files_skipped) skipped, \(.files_failed) failed" else "" end)\(if (.matches | length) > 0 then "\n\(.matches | map("  \(.file):\(.line)    \"\(.matched_text)\" → [[\(.link_target)\(if .matched_text == .link_target then "" else "|\(.matched_text)" end)]]") | join("\n"))" else "" end)""#;

/// `MvResult`: `{dry_run, from, to, total_files_updated, total_links_updated, updated_files}`
/// Format: `[dry-run] Moved <from> → <to>`, the two counters, then the list of
/// updated files and their replacements.
///
/// iter-262 (UX-4): the counters used to be JSON-only, so a `mv` that rewrote
/// nothing looked identical in text mode to one that rewrote everything — which
/// is how a vault full of `categories: ["[[Books]]"]` links could break in
/// silence. They print in dry-run mode too, where they are the whole point.
pub(super) const MV_RESULT_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)Moved \(.from) → \(.to)\nfiles updated: \(.total_files_updated), links updated: \(.total_links_updated)\(.updated_files | if length > 0 then "\n" + (map("  \(.file): " + (.replacements | map(.old_text + " → " + .new_text) | join(", "))) | join("\n")) else "" end)\(if (.frontmatter_links_skipped // []) | length > 0 then "\nfrontmatter wikilinks not rewritten (span a line break):\n" + ((.frontmatter_links_skipped) | map("  \(.source) line \(.line): \"\(.target)\"") | join("\n")) else "" end)""#;

/// `BatchMvResult`: `{applied, dry_run, moves, totals, updated_files}` plus the
/// optional `conflicts` / `skipped` arrays.
///
/// Format: the same headline-then-counters shape as [`MV_RESULT_FILTER`]
/// (iter-262, UX-4). Batch mode had no text filter at all before, so it fell
/// back to the generic key-value dump and its counters were buried inside a
/// `totals:` block.
pub(super) const BATCH_MV_RESULT_FILTER: &str = r#""\(if .dry_run then "[dry-run] " else "" end)Moved \(.totals.moves) file\(if .totals.moves == 1 then "" else "s" end)\(.moves | if length > 0 then "\n" + (map("  \(.from) → \(.to)") | join("\n")) else "" end)\nfiles updated: \(.totals.files_changed), links updated: \(.totals.replacements)\(.updated_files | if length > 0 then "\n" + (map("  \(.file): " + (.replacements | map(.old_text + " → " + .new_text) | join(", "))) | join("\n")) else "" end)\(if ((.conflicts // []) | length) > 0 then "\nconflicts: " + ((.conflicts) | join(", ")) else "" end)\(if ((.skipped // []) | length) > 0 then "\nskipped: " + ((.skipped) | join(", ")) else "" end)""#;

/// `ViewsListEntry`: `{filters, name}`
/// Format: `name  key=value key=value ...` — compact one-line summary of the view and its filters.
pub(super) const VIEWS_LIST_ENTRY_FILTER: &str = r#""\(.name)\t\(.filters | to_entries | map("\(.key)=\(.value | if type == "array" then join(",") else tostring end)") | join(" "))""#;

/// `ViewsMutationResult`: `{action, name}`
/// Format: `action: name`
pub(super) const VIEWS_MUTATION_RESULT_FILTER: &str = r#""\(.action): \(.name)""#;

// ---------------------------------------------------------------------------
// Shape-based filter lookup
// ---------------------------------------------------------------------------

/// Compute a sorted comma-joined key signature from a JSON object's top-level keys.
pub(super) fn key_signature(map: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut keys: Vec<&str> = map.keys().map(String::as_str).collect();
    keys.sort_unstable();
    keys.join(",")
}

/// Whether a sorted key signature describes a [`LinkInfo`].
///
/// `target` is the one key every `LinkInfo` has had since the shape existed;
/// everything else — `kind`, `line`, `path`, `label`, `fragment`,
/// `broken_anchor`, `out_of_vault`, `suggested_fragment` — is either optional
/// or was added later, and a `LinkInfo` read back from an older snapshot may
/// be missing any of them. So the test is "contains `target`, and every key is
/// one this shape declares", which keeps pre-iter-261 fixtures rendering while
/// still refusing any other object that happens to carry a `target`.
fn is_link_info_signature(key_sig: &str) -> bool {
    const LINK_INFO_KEYS: [&str; 9] = [
        "broken_anchor",
        "fragment",
        "kind",
        "label",
        "line",
        "out_of_vault",
        "path",
        "suggested_fragment",
        "target",
    ];
    let mut has_target = false;
    for key in key_sig.split(',') {
        if !LINK_INFO_KEYS.contains(&key) {
            return false;
        }
        has_target |= key == "target";
    }
    has_target
}

/// Look up the jq filter for a given key signature.
///
/// Returns `None` for unknown shapes, which will fall back to generic formatting.
pub(super) fn lookup_filter(key_sig: &str) -> Option<&'static str> {
    // `LinkInfo` is matched by shape rather than by exact signature (iter-261):
    // with `kind` always present and five optional keys around it, the
    // signature family would be 48 arms rendering one identical line.
    if is_link_info_signature(key_sig) {
        return Some(LINK_INFO_FILTER);
    }
    match key_sig {
        // PropertyInfo
        "name,type,value" => Some(PROPERTY_INFO_FILTER),
        // PropertySummaryEntry (mixed_types is skipped when None, so two signatures)
        "count,name,type" | "count,mixed_types,name,type" => Some(PROPERTY_SUMMARY_ENTRY_FILTER),
        // TagSummary
        "tags,total" => Some(TAG_SUMMARY_FILTER),
        // TagSummaryEntry
        "count,name" => Some(TAG_SUMMARY_ENTRY_FILTER),
        // TaskCount
        "done,total" => Some(TASK_COUNT_FILTER),
        // OutlineSection (with and without tasks)
        "code_blocks,heading,level,line,links" => Some(OUTLINE_SECTION_FILTER),
        "code_blocks,heading,level,line,links,tasks" => Some(OUTLINE_SECTION_WITH_TASKS_FILTER),
        // TaskInfo
        "done,line,status,text" => Some(TASK_INFO_FILTER),
        // FindTaskInfo
        "done,line,section,status,text" => Some(FIND_TASK_INFO_FILTER),
        // ContentMatch
        "line,section,text" => Some(CONTENT_MATCH_FILTER),
        // TaskReadResult
        "done,file,line,status,text" => Some(TASK_READ_RESULT_FILTER),
        // TaskDryRunResult
        "done,file,line,old_status,status,text" => Some(TASK_DRY_RUN_RESULT_FILTER),
        // VaultSummary
        "dead_ends,dir,files,links,orphans,properties,recent_files,status,tags,tasks"
        | "dead_ends,dir,files,links,orphans,properties,recent_files,schema,status,tags,tasks" => {
            Some(VAULT_SUMMARY_FILTER)
        }
        // Mutation results with property + value (SetPropertyResult, AppendPropertyResult,
        // RemovePropertyResult with value) — with or without optional `note` field
        // (iter-216 D-1 added `skipped_count` to all three shapes.)
        // iter-262 adds `list_collapsed`, present only when a `set` replaced a
        // list value with a scalar — hence the two extra signatures.
        "dry_run,modified,property,scanned,skipped,skipped_count,total,value"
        | "dry_run,modified,note,property,scanned,skipped,skipped_count,total,value"
        | "dry_run,list_collapsed,modified,property,scanned,skipped,skipped_count,total,value"
        | "dry_run,list_collapsed,modified,note,property,scanned,skipped,skipped_count,total,value" => {
            Some(PROPERTY_VALUE_MUTATION_FILTER)
        }
        // Mutation results with property only (RemovePropertyResult without value)
        "dry_run,modified,property,scanned,skipped,skipped_count,total" => {
            Some(PROPERTY_MUTATION_FILTER)
        }
        // Mutation results with tag (SetTagResult, RemoveTagResult)
        "dry_run,modified,scanned,skipped,skipped_count,tag,total" => Some(TAG_MUTATION_FILTER),
        // RenamePropertyResult (`properties rename`)
        "conflicts,dry_run,from,modified,scanned,skipped_count,to,total" => {
            Some(PROPERTY_RENAME_FILTER)
        }
        // RenameTagResult (`tags rename`, iter-266 adds `renamed_tags`)
        "dry_run,from,modified,renamed_tags,scanned,skipped_count,to,total" => {
            Some(TAG_RENAME_FILTER)
        }
        // BacklinksResult
        "backlinks,file" => Some(BACKLINKS_RESULT_FILTER),
        // LinksFix result (iter-187 adds `failed`/`failed_fixes` for L-11)
        "ambiguous,ambiguous_links,applied,applied_fixes,broken,broken_anchors,case_mismatch_fixes,case_mismatches,dry_run,failed,failed_fixes,fixable,fixes,fuzzy,fuzzy_applied,fuzzy_below_floor,fuzzy_fixes,fuzzy_min_confidence,ignored,out_of_vault,out_of_vault_links,relocation_fixes,relocations,templated,templated_links,unapplied,unapplied_fixes,unfixable,unfixable_links" => {
            Some(LINKS_FIX_FILTER)
        }
        // LinksAuto result (iter-187 adds per-file apply outcome fields for L-11;
        // iter-195a adds the config-exclusion attribution, present only when
        // `[links.auto]` config exclusions removed candidates — hence two
        // signatures. iter-213 split it into a title count and a mention count,
        // which always appear together).
        "ambiguous_titles,applied,apply_outcomes,dry_run,files_applied,files_failed,files_skipped,matched,matches,scanned"
        | "ambiguous_titles,applied,apply_outcomes,config_excluded_mentions,config_excluded_titles,dry_run,files_applied,files_failed,files_skipped,matched,matches,scanned" => {
            Some(LINKS_AUTO_FILTER)
        }
        // MvResult (`skipped_ambiguous` / `frontmatter_links_skipped` are each
        // omitted when empty and vary independently, hence all four
        // signatures — `skipped_ambiguous` itself has no `.skipped_ambiguous`
        // reference in MV_RESULT_FILTER because it is already reported via a
        // separate stderr note in text mode; see the NEW-3 handling in `mv`).
        "dry_run,from,to,total_files_updated,total_links_updated,updated_files"
        | "dry_run,from,frontmatter_links_skipped,to,total_files_updated,total_links_updated,updated_files"
        | "dry_run,from,skipped_ambiguous,to,total_files_updated,total_links_updated,updated_files"
        | "dry_run,from,frontmatter_links_skipped,skipped_ambiguous,to,total_files_updated,total_links_updated,updated_files" => {
            Some(MV_RESULT_FILTER)
        }
        // BatchMvResult (`conflicts` / `skipped` are omitted when empty).
        "applied,dry_run,moves,totals,updated_files"
        | "applied,conflicts,dry_run,moves,totals,updated_files"
        | "applied,dry_run,moves,skipped,totals,updated_files"
        | "applied,conflicts,dry_run,moves,skipped,totals,updated_files" => {
            Some(BATCH_MV_RESULT_FILTER)
        }
        // ViewsListEntry
        "filters,name" => Some(VIEWS_LIST_ENTRY_FILTER),
        // ViewsMutationResult
        "action,name" => Some(VIEWS_MUTATION_RESULT_FILTER),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// jq filter execution engine
// ---------------------------------------------------------------------------
