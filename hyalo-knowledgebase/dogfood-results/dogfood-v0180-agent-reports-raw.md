---
title: "Dogfood v0.18.0-pre — raw per-agent reports (appendix)"
type: research
date: 2026-07-17
status: active
tags:
  - dogfooding
  - okf
related:
  - "[[dogfood-results/dogfood-v0180-okf-profiles-pre-release]]"
---

# Raw per-agent dogfood reports (v0.18.0-pre fleet)

Verbatim condensed field reports from the seven dogfood agents, preserved for
the fix-wave implementers (iterations 172–175) — exact repro commands and
per-vault detail beyond the consolidated report.

## df-scale report (MDN 14,375 files + GitHub Docs 3,710 files, read-only)

Binary 0.18.0 (49c670bc). No bugs. Vaults verified pristine (no content modified, temp configs only ever in scratchpad).

## PERF (no >2x regressions)
- MDN indexed: find --limit 1 0.45s; summary 0.63s (baseline ~0.6s); BM25 0.41s; property filter 0.42s
- MDN unindexed: find 1.10s; summary 1.23s (FASTER than 2.9s baseline); BM25 4.08s (heaviest)
- docs: find 0.09s idx; summary 0.38/0.35s; BM25 0.11/1.03s
- lint plain vs --profile okf (docs full vault): 0.70–0.77s vs 0.75–0.76s → OKF overlay FREE
- lint --format github full docs vault: 1,620 annotations, 254KB, 0.67–0.83s
- [[schema.bind]] lint full vault: 0.71–0.77s → no bind overhead
- lint --profile changelog full vault (heading grammar): 0.76s
- string min/max over all 14K MDN: ~0.15s; create-index rebuild: MDN 3.3s / docs 0.7s
- Index sizes: MDN 114MB, docs 34MB

## BUGS: none

## UX
- UX-1 (MED): skip counters (files_missing/skipped_non_md/outside_vault) are JSON-only — invisible in text AND github formats. Diff-aware CI repro: 43 diff lines (15 .md) → 13 missing + 28 non-md skipped, only 2 linted; github output just says "in 1 file". Silent file-dropping in a CI gate. Recommend one-line skip summary in text/github.
- UX-2 (MED): [[schema.bind]] shipped syntax (array-of-tables, glob=/type= keys) differs from research/path-bound-schemas.md map form ("glob"="type"). Map form error is opaque: `malformed [schema] in .hyalo.toml: invalid type: map, expected a sequence` — names [schema] not [schema.bind]. Fix doc + error message.
- UX-3 (LOW): config discovery walks up from CWD, silently adopts unrelated .hyalo.toml; --dir does not relocate discovery. Recommend --config <path> and/or "loaded config: <path>" line.
- UX-4 (LOW): --format auto-detect JSON when piped surprises in ad-hoc `| head` use.

## WORKED WELL
- OKF flood navigable: grouped+capped text (3/rule/file, rollups → 525 lines), --max-per-rule, --rule scoping 0.10s, drill-down hints. 3,437 files / 1,324 violations manageable.
- --format github robust under unicode/emoji/CJK and ::%,= content (rule-templated messages, never echo raw content); longest line 356 bytes; no embedded newlines; correct summary line.
- Diff-aware --files-from - pipeline correct (repo-root git paths resolve vs dir=content; exit 0; counters in JSON).
- [[schema.bind]] first-match-wins exact + deterministic on overlapping globs (flip order → flips bound type).
- Heading-grammar rules fired correctly on malformed changelogs; string min/max produced 549 correct title-length violations on MDN; complex nested/versioned frontmatter (versions:{fpt,ghec,ghes}) parses clean.

## RECOMMENDATIONS
1. Fix UX-1 before relying on --format github in CI (highest impact).
2. Reconcile schema.bind doc syntax + improve error to name the bind key.
3. Consider --config flag / loaded-config startup line.
4. Otherwise clean at scale.

## df-own-kb report (hyalo-knowledgebase, worktree branch okf-conversion, 4 commits e0b965c..5a58f0f)

End state: okf+madr merged (2 clobbered keys manually repaired); 12 index.md + root log.md generated, idempotent; DEC-049..051 converted to valid ADRs under docs/decisions/; madr toc works. Lint 0 err/676 warn (baseline 665/21; delta = generated files' own MD013/MD022).

## BUGS
- B1 (HIGH) profile composability broken: init --profile madr after okf flips lint.profile "okf"->"madr" (OKF rules stop running: 21->0 warnings) and REPLACES schema.exempt ["**/index.md","**/log.md"] -> ["docs/decisions/README.md","docs/decisions/index.md"] so generated index.md fail --strict missing-type. Root cause lint_profile: Option<String> (config.rs:158) single-valued. Expected array union / conflict warning.
- B2 (HIGH) init --profile okf clobbered pre-existing schema.default.required ["title","type"]->["type"] silently (help claims upsert w/o clobbering); also strips all hand-written TOML comments + reorders whole file.
- B3 (HIGH) files with unparseable frontmatter (e.g. duplicate YAML key) are SILENTLY EXCLUDED from lint: "0 files checked, no issues", exit 0. CI gate passes corrupt files. (set warns; lint doesn't.) Expected error-severity parse violation.
- B4 (MED) lint --fix <-> okf index ping-pong: begin-marker immediately followed by ## heading fires MD022; --fix inserts blank INSIDE managed region; next okf index reverts (drift exit 1). CI never stable. Fix: blank line after begin marker.
- B5 (MED) [[schema.bind]] can't deliver frontmatter-free MADR: adr.required=[] but merged default schema still requires type -> spec-valid frontmatter-less ADR errors under --profile madr.
- B6 (MED) hyalo new --type adr ignores [schema.types.adr.defaults] (date=$today, status=proposed) — scaffold has only type: adr.
- B7 (LOW) generated artifacts violate hyalo's own default lint (MD022 every index.md, MD013 long titles/log messages): 665->688 warnings right after generation.

## UX
- U1 (MED) no upward .hyalo.toml discovery: from crates/ cwd, config=(none), dir="." -> lint --format github happily linted crates/ with cwd-relative paths. Silent wrong-vault op; dangerous with --fix. Warn when ancestor config exists.
- U2 (MED) OKF-CITATIONS-PRESENT accepts only level-1 "# Citations" (spec-faithful) — clashes with MADR h1-title+h2-sections docs; conformant ADRs permanently warn.
- U3 (MED) okf index generated index.md inside research/setup-hyalo-action/test/fixture-vault/ (test fixture); no exclude mechanism for okf index (lint.ignore doesn't apply).
- U4 (LOW) okf index/log + madr toc text output = raw key:value dump, first row mis-nested ("files: action: create").
- U5 (LOW) dead-end hint (find --task todo --file decision-log.md -> no results).
- U6 (LOW) --format github --fix --dry-run indistinguishable from plain lint.
- U7 (LOW) OKF profile ships BigQuery Table/Dataset types into every vault; okf index sorts byte-order (Ü last).
- U8 (LOW) hyalo new requires --file even when type has filename-template docs/decisions/{n:04}-{slug}.md — could derive next number/slug.

## WORKED WELL
okf index grouped-by-type + relative links exactly right; managed region/idempotency/drift-exit verified; unicode ok. okf log newest-first same-day merge ok. madr toc clean. schema.bind inference works (bind-mismatch rule exists); adr status pattern + required_sections validated correctly. --format github repo-root prefixing + --files-from compose. Overlay lint == materialized config. Perf fine (30-file vault, all <0.06s).

## RAW-TOOL FALLBACKS (feature gaps)
body-section append (no command), ADR body restructure, .hyalo.toml key repair (no hyalo config set), okf log entry removal (no undo), heading-level change.

## RECS
1. Conflict-aware profile merge: union arrays, printed conflict: lines, lint.profile as list.
2. Blank line after okf:index:begin (kills B4+B7 MD022).
3. Lint-error on unparseable frontmatter (B3) — highest CI-trust fix.
4. Bound types drop type requirement (B5); new applies defaults (B6); derive --file from filename-template (U8).
5. Warn on ancestor config (U1).

## df-skills-audit report (bundled profile skills, scratch vaults, repo untouched)

INSTALL MATRIX: all 4 profile skills install to .claude/skills/<name>/SKILL.md; base hyalo + hyalo-tidy coexist; re-init idempotent (toml byte-stable); deinit removes bundled artifacts + managed CLAUDE.md section, preserves user skills/prose, idempotent; changelog profile WORKS but missing from init --help.

## BUGS
- BUG-1 (HIGH) bundled `skills` skill fails its own skills profile: description contains `<` (`<name>/SKILL.md`), profile pattern ^[^<]*$ forbids it -> SCHEMA error. Fix: reword desc; add CI linting bundled skills with --profile skills.
- BUG-2 (HIGH) stacked profiles clobber TOML arrays: merge_value (profiles.rs:138) recurses tables only; arrays replaced. All-4 vault: only changelog's [[schema.bind]] survives; **/SKILL.md and docs/decisions binds GONE; exempt clobbered to ["CHANGELOG.md"]; [lint] profile scalar last-write-wins (changelog). Proven wrong verdicts (broken SKILL.md validated vs okf default schema). == root cause of df-own-kb B1. Fix: array union for schema.bind/exempt; lint.profile semantics for stacks; integration test.
- BUG-3 (MED) hyalo new ignores [schema.types.<t>.defaults] — madr skill documents status/date that never appear (== df-own-kb B6; also causes empty madr toc Status/Date columns).
- BUG-4 (LOW) init --help omits changelog profile.
- BUG-5 (LOW) --claude CLAUDE.md managed section generic; zero profile-specific pointers.
- BUG-6 (LOW) new --type skill emits `type: skill` key non-spec for SKILL.md.

## CONTENT AUDIT: otherwise all taught commands/flags/15 rule IDs real & correct (okf index/log flags, madr toc, changelog add/release --dry-run exit 1, lint-rules set, find variants). madr skill claims scaffold emits status/date (false — BUG-3); madr/skills "no explicit type needed" vs scaffold writing literal type: (LOW).

## BEHAVIORAL EVALS: okf loop clean end-to-end (friction: TBD placeholders for plain-string required props pass lint -> weak drive-loop); changelog add/release loop clean, [X.Y.Z]: TBD footer passes lint silently; madr toc shows dashes (BUG-3 downstream); skills name:TBD errors correctly.

## SELF-LINT: 6 files, 1 error (BUG-1) + 57 style warns + 2 undeclared-property (hyalo-tidy: context, disable-model-invocation — CC extension keys).

## RECS: fix BUG-2 first (composability promise + wrong verdicts); BUG-1 + CI self-lint; new honors defaults; help text changelog; profile-aware CLAUDE.md pointers.

## df-mapl report (comparis/mapl-memory, 298 files, branch okf-conversion 3 commits, clean end state 0 err/50 warn)

Deliberately did NOT run okf index --apply on the real vault (would destroy 36KB curated INDEX.md — BUG-1). Generator assessed on isolated copies.

## BUGS
- BUG-1 (HIGH, data loss): FIRST `okf index --apply` on an index.md WITHOUT okf markers keeps only leading H1 and DISCARDS all hand-written prose/sections/links (help promises preservation — only true once markers exist). dry-run says "update index.md" with NO loss warning. On macOS INDEX.md≡index.md (same inode) so the documented command targets the curated file. Fix: non-destructive marker insertion or refuse+warn; dry-run must warn.
- BUG-2 (MED): exempt-glob matching is case-sensitive: profile default ["**/index.md","**/log.md"] does NOT match INDEX.md/LOG.md on case-insensitive FS -> reserved file linted as concept (SCHEMA missing type). Rename to lowercase -> exempt works.
- BUG-3 (LOW): SCHEMA missing-type reported autofixable:true but --fix is a no-op on it.
- BUG-4 (MED): lint --format json --detailed caps files[] at 50 (files_truncated:true) with no override flag even --max-per-rule 0; per-file detail unreachable programmatically (workaround --glob per subtree).
- BUG-6 (MED): flag-vs-file profile divergence: [lint] profile="okf" in config honors user [schema] exempt additions (0 errors); `--profile okf` CLI flag overlay RESETS exempt to profile builtin, ignoring user additions -> errors return. Flag should merge like the file path does.
(BUG-5 retracted: **/ does match root files; real issue purely case.)

## UX
- UX-1 (MED): profile injects BigQuery Dataset/Table + Reference example types into real vaults (hand-delete needed) — ship neutral skeleton. (== df-own-kb U7)
- UX-2 (LOW): silently sets site_prefix="" (disables auto-derived absolute-link resolution), undocumented.
- UX-3 (MED): okf log appends "## date + - entry" format, can't match vault's existing flat "- date | action | ..." convention; mixes styles in one file.
- UX-4 (LOW): okf index --format text malformed flatten (== df-own-kb U4).
- UX-5 (MED): okf index generates into _template/ scaffold dirs; schema exempt does NOT suppress generation; no generator ignore list (== df-own-kb U3).

## WORKED WELL
- Deep-merge PRESERVED the entire hand-tuned .hyalo.toml (dir=".", all keys) — pure additive. (Non-overlapping keys case; own-kb's B2 was overlapping-key clobber.)
- Marker-based preservation on SECOND+ runs verified; scoped okf log <dir> targets dir-local log.md; dry-run drift exit codes CI-usable; hints copy-pasteable.
- Perf: full lint 298 files 0.045s; okf index dry-run 0.018s.

## PRODUCT VERDICT: generated index.md CANNOT replace curated INDEX.md here — groups by raw type ("## state") vs semantic sections; bare filename links without description unless every concept gains title+description frontmatter (0/293 have them). Generator honors title/description when present. Keep hand-maintained.

## DATA FINDINGS (KB-side): last_verified mixed 207 text vs 86 date; 42 files sources:[]; 2 files type: reference outside declared enum (schema drift doc vs reality).

## FALLBACKS: body edits (MD040/MD018) via Edit; .hyalo.toml via Edit (expected); Python/jq to aggregate lint JSON past the 50-file caps (BUG-4); ls/stat for inode proof.

## df-ffrdp report (ff-rdp/kb, 329 files, branch okf-conversion 5 commits 0bff6b3..99bb59b, clean, not pushed)

End state: okf+skills+madr composed (with manual exempt union); 20 idempotent index.md + log.md; 2 ADRs from decision-log + toc. Remaining 1821 warnings = "not an OKF vault" signals (iteration frontmatter vs concept schema, 326 missing Citations), not defects.

## BUGS
- B1 (HIGH) [schema] exempt CLOBBERED per init --profile (3rd confirmation). NOTE DISCREPANCY: ffrdp says [[schema.bind]] arrays DO compose correctly, df-skills-audit says binds were clobbered in all-4 stack — reconcile during fix (order/profile-dependent?).
- B2 (HIGH) [schema.default] required=["type"] leaks onto [[schema.bind]]-typed files: okf+skills composed -> every real SKILL.md errors missing-type; skills profile in isolation = 0 errors. == df-own-kb B5 (2nd confirmation). Fix: default.required must not apply to bind-typed files.
- B3 (MED) okf index/log hard-abort exit 2 on FIRST unparseable file anywhere in vault, even with subtree scope (okf index rdp dies on iterations/iter-84); find/summary/lint skip-and-warn on the same file. Generators should skip-warn + honor scope in pre-scan.
- B4 (MED) generated index.md fails hyalo's own MD022 (heading after begin-marker, line 4); exempt covers SCHEMA pass but not markdown-body pass. == own-kb B4/B7 (3rd confirmation).
- B5 (MED) `--limit 0` (documented unlimited) returns ZERO file results on lint JSON; --count --limit 0 correctly 330.
- B6 (LOW) hyalo config doesn't reflect --dir override (shows config dir, not effective dir).
- B7 (LOW) adr defaults auto-apply stamped type:adr + defaults onto generated docs/decisions/README.md (bind glob matched it) -> dashboard failed adr lint; regen cleaned.
- B8 (LOW) new --type adr doesn't materialize defaults (3rd confirmation of defaults gap).

## NON-BUGS verified: exit codes correct everywhere (earlier readings were pipeline artifacts); deep-merge preserved dir + all 5 views, idempotent byte-identical; .DS_Store ignored.

## UX
- U1 (HIGH for skills): walker SKIPS dot-dirs -> **/SKILL.md bind can't reach .claude/skills/ (canonical CC location; 4/5 SKILL.md here unreachable). Biggest real-world blocker for skills profile.
- U2 (MED) [lint] profile single scalar, last profile wins (== own-kb B1 aspect).
- U3 (MED) lint takes only ONE positional file; inconsistent with set.
- U4 (LOW) BigQuery example types injected (3rd confirmation).
- U5 (LOW) madr default bind path vs existing kb/decision-log.md convention: creates parallel tree.

## SKILL RULES: all true positives verified in isolation (name pattern, desc max-length + no-<, NAME-DIRNAME, RESERVED-NAME, LINE-BUDGET). Clear messages.

## WORKED WELL: okf index/log/madr toc managed regions + idempotency + drift exits; MADR end-to-end (new->fill->lint 0/0->toc); lint --fix MD022 661->104 MD031 132->10 across 46 files no corruption; --validate actionable.

## PERF (329 files warm): summary 0.04s; lint --profile okf 0.11s; bm25 0.14s; okf index dry 0.01s.

## FALLBACKS: iter-84 oversized frontmatter scalar (that WAS the bug); ADR body prose; hand-union exempt TOML.

## df-hoppy report (hoppy-knowledgebase, 230 files, branch okf-conversion 3 commits 1e1a931..d39378a, clean, not pushed)

End state: valid OKF bundle — 14 index.md + root log.md + docs/decisions with 3 ADRs + toc README. Lint 4 errors (all pre-existing HYALO002) / 183 warnings, baseline was 9 errors. Remaining: 96 undeclared-prop warns, 74 MD040 (fences; --fix does NOT fix MD040), 44 CITATIONS warns currently masked by BUG-1, ~40 more decisions for full MADR adoption.

## BUGS
- BUG-1 (MED, composability): init --profile madr clobbers [lint] profile (okf->madr; OKF-CITATIONS 44->0 silently) AND replaces exempt (un-exempts all 15 reserved files: lint 4->17 errors). == 4th confirmation of clobber family. Docs contradicted. Hand-union workaround.
- BUG-2 (MED): okf merge silently dropped user's schema.default.required ["title","type"]->["type"] — title no longer required, no warning. == df-own-kb B2 (2nd confirmation of required clobber specifically).
- BUG-3 (LOW): new --type adr ignores defaults (4th confirmation). Required sections scaffold correctly.

## UX
- UX-1 (LOW): --section frontmatter natural guess errors; real flag --frontmatter; consider alias (error msg itself good).
- UX-2 (LOW): okf/madr hints suggest lint --profile even when config already sets profile (no-op overlay).
- UX-3: TTY format detection worth confirming (JSON on interactive terminal in harness — likely pipe artifact).

## WORKED WELL
- Deep-merge preserved all 5 type schemas + 8 views byte-for-byte (non-overlapping keys).
- Idempotency byte-level across init/index/toc; drift exit codes good CI shape.
- Generators genuinely usable (63-entry backlog index clean; date-grouped log; toc dashboard).
- schema.bind works (decisions validate as adr, README exempted); 3 real decisions fit adr schema 0 violations; MADR-SUPERSEDE-RESOLVE (dangling ADR-0099) + MADR-DUPLICATE-NUMBER both fire correctly.
- Edge cases: unicode/emoji/日本語 filenames+titles fully worked (find/index/wikilink/backlinks); CRLF == LF lint; undeclared types + frontmatter-free reserved files handled permissively.

## PERF (230 files): lint 0.08s; okf index dry 0.04s; find --property 0.01s.

## RECS: [lint] profile as list / auto-union arrays (headline); warn on required clobber; new defaults; alias --section frontmatter; suppress redundant profile hint.

## FALLBACKS: ADR prose (expected); exempt union in TOML (forced by BUG-1).

## df-userservices report (user-service 3 commits 3bd495d..85a775c; user-event-service 3 commits 6256e83..7c7021e; not pushed, trees clean)

Repo A end state: tracked files 0 okf lint errors (10 remaining errors all on untracked research/); adr-001 fully conformant; okf lint 163->43.
Repo B end state: 9 warns 0 errors; root CHANGELOG.md converted to KaC 1.1.0 (all entries preserved), lints clean; changelog add/release dogfooded (no release applied).

## BUGS
- BUG-A1 (HIGH): profile compose clobbers exempt + [lint] profile (5th confirmation).
- BUG-B1 (HIGH, NEW): `changelog add` appends the new category AFTER the bottom link-reference definitions — outside [Unreleased]; KaC always has refs at bottom so EVERY conformant changelog is hit; output fails own lint (MD047). `release --apply` places its ref correctly — defect specific to add. Minimal repro provided.
- BUG-A2 (MED, NEW): `madr toc <dir>` treats EVERY .md as an ADR (ignores adr bind/type) — listed all 13 concept files with dash numbers. Rebind-to-shared-dir workflow breaks toc; dedicated docs/decisions required.
- BUG-A3 (MED, NEW): `types set default` creates phantom [schema.types.default] — NOT the base [schema.default]; silently unused. Reject reserved name or target base schema.
- BUG-B2 (LOW, NEW): changelog/body linters parse INSIDE HTML comments (## in <!-- --> flagged).

## ROOT-CHANGELOG UX (confirmed): profile binds CHANGELOG.md relative to VAULT dir; with dir=<kb-subdir> the repo-root CHANGELOG.md is unreachable ("file not found"); workaround global `--dir .` pulls whole repo into vault scope. Need bind-outside-vault / changelog_path / documented pattern.

## WORKED WELL
- Deep-merge scalar keys safe (dir survived everywhere).
- MADR approach comparison: bind-in-place (glob adr-*.md) natural for lone ADR but breaks toc (A2); mv to docs/decisions better for full adoption — mv --dry-run planned ALL inbound link rewrites (md links + wikilinks) + fixed the file's own outbound depth — excellent.
- changelog release rotation correct in scratch incl. idempotency refusal; --profile bogus lists all 4 profiles; adr required_sections order enforcement good.

## FALLBACKS (feature gaps): all .hyalo.toml restructuring by hand (no hyalo command for [[schema.bind]] / [schema.default.properties]); `set` rejects string-list values (append only; no way to set full list); body/heading edits; trailing-newline trim after changelog add.

