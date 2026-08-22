---
title: "Dogfood v0.21.0-pre2 — integrity wave verified (iters 196, 200–205)"
type: research
date: 2026-08-23
status: active
tags:
  - dogfooding
  - links
  - config
  - security
related:
  - "[[dogfood-results/dogfood-v0210-pre-release-iters-191-198]]"
  - "[[iterations/iteration-200-links-apply-integrity]]"
  - "[[iterations/iteration-201-config-trust]]"
  - "[[iterations/iteration-202-boundary-completion]]"
  - "[[iterations/iteration-203-index-md-resolution]]"
  - "[[iterations/iteration-204-dogfood-low-batch]]"
  - "[[iterations/iteration-205-common-title-frequency-trigger]]"
  - "[[iterations/iteration-196-mdlint-workaround-strip]]"
  - "[[iterations/iteration-206-links-perf-profiling]]"
---

# Dogfood v0.21.0-pre2 — integrity wave verified (iters 196, 200–205)

Binary: `target/release/hyalo` from main `0580e05` (self-reports `0.20.0
(0580e0516a1c 2026-08-23)` — workspace version not yet bumped, expected
pre-release state). Four parallel tester agents. Corpora: own KB (377
files), GitHub Docs `~/devel/docs/content` (3,710 files; 960-file subset
for mutation tests), vscode-docs (760), MDN `~/devel/mdn/files/en-us`
(14,375). All mutation testing on scratchpad copies; all originals and
the repo verified `git status`-clean afterwards.

**Verdict: the integrity wave delivers.** All seven iterations verified
WORKING with exact-number confirmation of every headline claim. All
prior fixes (iters 191–198, v0.20.0 findings) STILL FIXED; the executed-
hint contract is clean (39 hints run verbatim, zero broken). Exactly one
new regression was introduced by the wave (BUG-7, MEDIUM). However the
session found **one HIGH and three MEDIUM corruption paths in the
`links --apply` family that pre-date the wave** (present in released
0.20.0) — the same class iter-200 was opened for, reachable through the
same hint-recommended commands. Recommendation at the end.

## New Feature Verification

### iter-200 — links apply-path integrity — WORKING

- **H-1 regression check PASS**: plain `links fix --apply` on the GitHub
  Docs copy modifies **0 files** (`diff -rq` clean), broken count 3,328
  unchanged — never increases. Same on vscode-docs and an own-KB copy.
  Contrast build: hyalo 0.20.0 on a minimal vault still rewrites
  `[c](/deep/page?x=1)` → `[c](deep/Page)` (slash stripped); this build
  writes `[c](/deep/Page)`.
- **Spelling preservation PASS**: across all 2,331 fuzzy destination
  rewrites — 0 leading slashes gained/lost, 0 `.md` injected. Anchors,
  angle-bracket destinations, CRLF, and missing-trailing-newline all
  survive rewrites.
- **Round-trip guard PASS, arithmetic exact**: 3,328 − 2,331 applied =
  997 remaining; every written fix resolves afterwards.
- **`--apply-fuzzy` PASS on the stated numbers**: 506 files modified,
  broken 3,328 → 997 (plan said ~507 / 3341 → 1008 on a 1-file-larger
  copy). Byte-level check of all 506 files: after masking `](…)`
  destinations, prose is byte-identical.
- **H-2 inert zones PASS**: 11,141 insertions on GH Docs, 11,032 on
  vscode-docs — **0** inside markdown destinations, bare URLs,
  autolinks, link labels, image destinations, or fenced code blocks.
  (Gaps in *other* zone types → BUG-1/2/3 below.)
- **M-1 PASS**: site-absolute basename guesses report `[fuzzy 0.6]`,
  gated behind `--apply-fuzzy`; 1,284 of 2,331 GH Docs fuzzy candidates
  sit exactly at 0.6 — the whole fallback class is behind the gate.

### iter-201 — config trust — WORKING

- **H-4 PASS** with conclusive A/B: repo root `lint --strict --dir
  hyalo-knowledgebase` → 60 files / 4 warnings (config honored); the
  config-less 377 / 694 figure reproduces only when no config applies.
  Foreign `--dir` loads the target tree's config and announces it on
  stderr; no-config case says so explicitly. `hyalo config` reports
  `config_path` / `dir_overridden` consistently.
- **M-2 PASS**: three malformation classes × ten mutating commands — all
  exit 1 with the parse diagnostic; whole-vault SHA-256 byte-identical.
  Warning survives `-q`. Read-only commands keep salvaged `dir`.
  `--dry-run`, `init`, `deinit` unaffected. The `set --dir <good vault>`
  escape hatch works. Single-parse confirmed (one warning per run).
- **M-7 PASS**: `->` vs `=> … [writes]` classification correct on every
  sampled hint; adversarial filenames (`views set evil.md`, `set.md`)
  did not fool the classifier and get shell-quoted properly.

### iter-202 — vault boundary completion — WORKING

- **H-3/M-3/M-4 PASS**: every escape vector (symlink, `../`, absolute,
  uncanonicalized) refused at exit 1 with nothing written outside;
  legitimate configured repo-root CHANGELOG.md still allowed; in-vault
  controls succeed and are idempotent.
- **M-5 PASS**: canonical dedup enumerates symlink+target once;
  `links fix --apply` on such a vault exits 0, rewrites once, preserves
  the symlink as a symlink; out-of-vault skips warn once per run.
- **L-16 PASS at runtime** (two-path wording, exit 1) — one config-level
  outlier, BUG-14.
- One regression from the dedup work: BUG-7.

### iter-196 — mdbook-lint 0.16.0, workarounds stripped — WORKING

- MD010 `--fix` now applies on lines with multibyte chars (`✘`, `’`);
  re-lint clean.
- MD018 continuation-line false positive gone: **0 MD018 hits across all
  three external corpora** (18,845 files). The KB rewrap workaround from
  commit 3340358 is obsolete.
- CRLF clean: MD047 fix appends `\r\n` (the kept exception; gap filed
  upstream as mdbook-lint#495); multi-rule CRLF fixture fixed with
  CR==LF before and after, zero lone terminators.
- Violation-count parity: no mass swing (GH Docs 7,719 / vscode 1,773 /
  MDN 49,963); vscode MD010=737 independently matches a raw
  `grep -P '\t'` count exactly.

### iter-203 — directory targets → index.md — WORKING

- **F-1 PASS, exact**: MDN broken links 49,703 → **509** with
  `--site-prefix "en-US/docs"`; `backlinks web/api/document/index.md` →
  13 across 9 files, matching an independent grep. `fixable 0` on MDN —
  no bulk-rewrite risk there.
- `hyalo config` site_prefix reporting: all four sources (flag / config /
  derived / disabled) correct in text and JSON.
- `mv foo/index.md bar/index.md`: all ten inbound spellings rewrite
  correctly, 0 broken afterwards; L-11 confirmed (prefix preserved where
  written, never injected). One spelling nit → BUG-12.
- HYALO006 and precedence (`foo.md` beats `foo/index.md` for bare
  targets; `page/` → `page.md` links did not regress) confirmed.
- `--index` parity: byte-identical results vs disk scan; a prefix-
  mismatched index is refused with fallback to scan (correct, but see
  UX-3 for the message).

### iter-204 — dogfood M/L batch — WORKING

All 17 items verified individually: M-10 rule-id validation +
case-insensitivity (unmatched-prefix path excepted → BUG-5), L-2 exit
codes, M-6 staleness warning matching its documented blind spot exactly,
L-5/L-6 JSON error envelopes + caret alignment with `(?i)`, L-15 1-based
`col` (byte-indexed though → BUG-13), L-9/L-7, UX-2 closest-headings
(5 shown + honest remainder), M-8 limit contract (`types/views/
lint-rules list` reject `--limit`; bare `tags`/`properties` accept
`--glob`/`--limit` and genuinely filter), L-1 no backlink double-count,
L-4 dangling-symlink mv refusal, case-insensitive site-prefix stripping
(`/EN-US/`, `/en-us/`, `/En-Us/` all resolve; `/xx-YY/` correctly
broken).

### iter-205 — common-title frequency trigger — WORKING

- GH Docs `content/actions`: note names `"workflows" (453×, 44%)`,
  `"OIDC" (177×, 17%)`, etc.; arithmetic independently verified (total
  1025, threshold max(25, ceil(1025/40))=26, nine flagged titles sum to
  exactly the stated 893).
- L-12/L-13: prose shows 5 of 9 honestly; the `--exclude-title`
  suggestion carries **all nine** offenders and is copy-pasteable +
  case-insensitive; dominant original casing reported (`WIDGET` ×20 over
  `Widget`/`widget`).
- Wordlist vs frequency reasons distinguished in singular, plural, and
  mixed phrasings; non-ASCII titles (`Übersicht` ×30) trigger the
  frequency path with correct shell quoting; wordlist stays ASCII-gated.
- Own-KB `"backlinks" (130×, 67%)` fires via frequency — the documented
  true positive from the amended AC.
- stdout byte-identical with/without the note, 6/6 `cmp` checks across
  three vaults × two formats.

## Bug Regression Testing (prior waves)

| Item | Result |
|---|---|
| iter-193 no case-probe writes from read-only commands | STILL FIXED |
| iter-192 config JSON envelope + `--jq '.results.dir'` | STILL FIXED |
| iter-195a `[links.auto]` persistent exclusions | STILL FIXED |
| iter-198 `--no-first-only` (and conflict rejection) | STILL FIXED |
| iter-197 common-title stderr note (wordlist path) | STILL PRESENT |
| Executed-hint contract (39 hints run verbatim) | CLEAN |

## Bugs Found

Only BUG-7 is a regression from this wave. BUG-1/2/3/4 are pre-existing
in 0.20.0 but belong to the H-1/H-2 corruption class and are reachable
through the same hint-recommended `--apply` commands.

### BUG-1: `links auto --apply` corrupts inline code after any unmatched backtick (HIGH, pre-existing)

One stray backtick (e.g. `` press <kbd>`</kbd> ``) flips inline-code
parity for the rest of the file; every later code span is treated as
prose and gets wikilinks injected. CommonMark treats an unmatched
backtick as literal text — the parity accumulator is the wrong model.
Measured silent corruption: GH Docs 9 insertions inside code spans
(`` `git blame` `` → `` `[[git]] blame` ``, `` `README.md` `` →
`` `[[README]].md` ``), vscode-docs 8 (`` `settings.json` `` →
`` `[[settings]].[[json]]` ``), **own KB 3**
(`dogfood-results/dogfood-v0150-iter127-130.md`). Minimal repro:

```bash
mkdir -p /tmp/tick && cd /tmp/tick
printf -- '---\ntitle: git\n---\n\n# git\n' > git.md
cat > note.md <<'EOF'
---
title: note
---

Before: `git blame` stays code.

Press <kbd>`</kbd> to open a terminal.

After: `git blame` should still be code.
EOF
hyalo links auto --apply --dir /tmp/tick   # injects [[git]] into the last code span
```

### BUG-2: `links auto --apply` inserts wikilinks inside Liquid expressions (MEDIUM, pre-existing)

3,328 of 11,141 insertions on the GH Docs copy (30%) landed inside
`{% … %}` / `{{ … }}` — e.g. `{% data variables.[[copilot]].… %}`,
destroying variable references. Repro: `links auto --apply` on a copy of
`content/code-security`, then `grep -rE '\{%[^%]*\[\['`. A `{%…%}` /
`{{…}}` inert zone (or configurable inert-pattern list) closes it.

### BUG-3: `links auto --apply` inserts wikilinks inside HTML tags/attributes (MEDIUM, pre-existing)

128 occurrences on vscode-docs, 5 on GH Docs. Breaks image paths
(`<img src="[[net]].png" alt="[[actions]]">`), anchor names, class
hooks, and `vscode://` URLs. Raw HTML is valid markdown;
`inert_link_zones` should cover tag spans.

### BUG-4: `links fix --apply` strips Liquid syntax from link targets (MEDIUM-HIGH, pre-existing)

hyalo treats `{% ifversion … %}/path{% endif %}/…` as literal path text,
fuzzy-matches the remainder at 0.95, and rewrites the destination —
silently dropping the version conditional. 25 such fixes offered on the
full GH Docs corpus; the iter-200 round-trip guard cannot catch this
(the rewritten target genuinely resolves — corruption is semantic).
Suggested fix: skip targets containing `{%`, `{{`, or `${`.

### BUG-5: unmatched `--rule-prefix` runs every MD rule while claiming it runs none (MEDIUM)

`lint --rule-prefix nope` warns "matches no rule; nothing will be
linted" then runs all MD rules anyway, exit 0 (`rules_fired: 2`).
Contrast `--rule NOPE999` which correctly exits 1. A typo'd prefix
yields a partial lint that looks like a successful filtered run.

### BUG-6: `lint` JSON truncation counters are wrong/inconsistent (MEDIUM, found independently twice)

Two facets, same envelope:
- `results.total` counts only listed files while `warnings`/`errors`
  count the whole run: MDN `web/api` default limit → `total: 1358` vs
  `errors+warnings = 14248` (10×); GH Docs 1,585 vs 7,719; own strict
  repro 520 vs 694. `rules_fired` likewise computed over the truncated
  set (7 vs 8).
- `files_truncated` is computed from `files_checked > limit`, not from
  actual list truncation: 61-file vault, 1 violating file, all listed →
  `files_truncated: true`. The text renderer gets this right
  ("showing 4 of 4"); only JSON is wrong. Consumers loop or over-fetch.

### BUG-7: in-vault symlink shadows the real file in the fuzzy candidate set (MEDIUM, **iter-202 regression**)

Canonical dedup keeps whichever path the walker sees first
(alphabetical), so `alias-target.md -> target.md` drops `target.md` from
enumeration: a link fixable at `[fuzzy 0.966]` becomes `Unfixable: 1`,
and fixes get reported against the alias name. Exact resolution and
backlinks unaffected. Fix direction: prefer the non-symlink path as the
canonical representative.

```bash
mkdir -p v/notes && printf 'dir = "notes"\n' > v/.hyalo.toml
printf -- '---\ntitle: Source\ntype: note\n---\nSee [[targt]] here.\n' > v/notes/source.md
printf -- '---\ntitle: Target\ntype: note\n---\nx\n' > v/notes/target.md
(cd v && hyalo links fix --dry-run)     # fuzzy 0.966 offered
ln -s target.md v/notes/alias-target.md
(cd v && hyalo links fix --dry-run)     # Unfixable: 1
```

### BUG-8: `find --broken-links` checks anchors against raw heading text, not slugs (MEDIUM, pre-existing)

`#sub-section` against `### Sub Section` → reported broken; `#Sub
Section` / `#sub section` / `#Sub%20Section` → pass. Inverted in
practice: of 7 checkable anchors on the GH Docs copy, 6 were false
positives. HYALO006's description points users at this command for
anchors. Related false negative: same-file fragments (`[b](#nope)`) are
never checked.

### BUG-9: HYALO006 line numbers offset by frontmatter length (MEDIUM, pre-existing)

Frontmatter offset applied twice: 3 FM lines → link on line 5 reported
as line 8; 5 FM lines → 7 reported as 12. Isolated to HYALO006 (MD009/
MD019/HYALO001/backlinks all correct in the same file).

### BUG-10: trailing-slash targets inconsistent between `links` and `backlinks` (MEDIUM)

- Over-count: with both `foo.md` and `foo/index.md`, one relative
  `[b](foo/)` appears as a backlink of **both** (slash normalized away
  before the index is keyed); `links` says `ambiguous: 0`.
- Under-count: `[b](/baz/)` resolving to `baz.md` shows in
  `find --broken-links` resolution but is missing from `backlinks
  baz.md`.
- Root enabler (LOW): trailing-slash targets still fall back to
  `<target>.md`, more permissive than iter-203's documented "skips the
  `.md`-append attempt entirely".

### BUG-11: fuzzy confidence does not track semantic plausibility (MEDIUM)

Normalized string distance over full paths — long GH Docs slugs inflate
it: `/actions/reference/actions-limits` → `graphql/reference/actions.md`
scores 0.9 while the only *correct* proposal in the sample scores 0.6.
`fuzzy_min_confidence` defaults to null, so bare `--apply-fuzzy`
accepts 1,047 rewrites at ≥0.8 to unrelated documents. Aggravators:
JSON exposes only counts (`fixes`/`fuzzy_fixes` arrays always empty even
in dry-run — proposals cannot be audited programmatically), and the text
label is `[fuzzy N]` for every strategy, so M-1's honest
`BasenameFallback` name never reaches the user. Suggest a default
confidence floor + final-segment-weighted scoring + per-fix JSON detail.

### BUG-12 (LOW cluster, links family, pre-existing unless noted)

- Query strings silently dropped on rewrite: `/deep/page?x=1` →
  `/deep/Page` (fragments survive; `?` doesn't).
- CommonMark link titles unparsed: `[a](p.md "Title")` → target
  `p.md "Title"`, reported broken, missing from backlinks.
- `mv` appends `.md` to an extensionless spelling: `[f](foo/index)` →
  `[f](bar/index.md)` (violates iter-203's spelling-preservation AC on
  one of ten forms).
- Relative bare-stem relocation labeled `link-case-mismatch` and applied
  by plain `--apply` at 0.95 while the identical site-absolute guess is
  gated — the M-1 mislabeling fixed for one form, left in the other
  (per iter-200's documented design, but hard to explain to a user).

### BUG-13 (LOW cluster, iter-204 edges)

- `links auto` JSON `col` is 1-based but **byte**-indexed, undocumented
  (`col: 12` where the char column is 9 on a multibyte line).
- `lint sub/` hint emits `--glob 'sub//*'` (double slash) which matches
  nothing at exit 0 — a copy-pasteable hint that reads as "clean";
  hints are documented as copy-pasteable and agents follow them.
- `did you mean X.md?` emitted without checking the candidate exists
  (`nosuchdir/` → `nosuchdir/.md`).
- `find nosuchdir/` exits 0 "No results" while `lint nosuchdir/` exits 1
  not-found — L-7 covers lint/read but not find.

### BUG-14 (LOW/MEDIUM cluster, config/UX family)

- Invalid `[changelog] path` config refusals exit 2 with single-line
  wording, breaking the L-16 exit-1 two-path contract (runtime refusals
  comply).
- `views run` is not `find --view` equivalent despite its help: rejects
  positional BM25 patterns, and its `-e` help references a PATTERN
  argument that doesn't exist there.
- `config_excluded` counts excluded titles, not suppressed candidates
  (excluding one title that removes 130 candidates reports `1`).
- `create-index --output /tmp/my-index` is a verbatim help EXAMPLE that
  exits 1 on the boundary check (needs `--allow-outside-vault`); the
  `--index-file` global help ("absolute paths used as-is") contradicts
  the guard.
- `set`/`append` reformat untouched frontmatter (116 of 198 lines
  changed on a GH Docs `index.md`: `>-` refolding, quote-style flips) —
  semantically lossless but diff churn that makes hyalo hard to adopt on
  version-controlled docs repos.

## UX Issues

- **UX-1: no walk-up config discovery, and it fails silently.**
  `cd hyalo-knowledgebase && hyalo lint` re-roots at cwd on defaults
  with no diagnostic — the un-fixed half of the "config silently
  discarded" class iter-201 targeted (the `--dir` half now warns
  loudly). Walk up to an ancestor config, or warn when cwd sits inside a
  tree containing one.
- **UX-2: `hyalo config` gives no machine-readable malformed signal** —
  on a broken config it returns populated defaults + `raw_contents`,
  exit 0, no `malformed` field; the diagnostic is stderr-only. It is the
  designated debugging command for exactly this case. Also: the
  `raw_contents` blob dominates JSON output; worth an opt-in flag.
- **UX-3: index-mismatch warning** prints the same vault path twice and
  leaks `Some("en-us")` Rust formatting; the actual difference (prefix)
  is buried.
- **UX-4**: `links` text output omits the `fuzzy` bucket so displayed
  counts don't add up (6,098 broken vs 25+1,400 shown; JSON reconciles
  exactly). Also: `links`, `views`, `lint-rules list` emit zero hints.
- **UX-5**: fatal L-2 errors are prefixed `warning:` while exiting 1;
  `--fix` can report the same rule as both fixed and conflicted (display
  only); `tags --limit` lacks the "showing N of M" footer that
  `properties --limit` has; stale malformed-config warning prints before
  the "config doesn't apply" note on the `--dir` escape hatch; hint flag
  placement inconsistent (`--format text` appended vs injected).
- **UX-6**: `.results` JSON shape varies by command (arrays vs keyed
  objects vs counters; `source` vs `file` keys), so the documented
  `--jq '.results[].file'` idiom isn't universal. `find --broken-links`
  lists all links of a matching file with no line numbers.
- **UX-7**: `find ''` returns the whole vault — an empty shell variable
  silently becomes a full scan.
- Wished-for: `--min-confidence` floor for fuzzy applies surfaced in
  text output; `links --exclude-target-regex` (would also mitigate
  BUG-4); a documented non-override path for indexing read-only corpora.

## What Worked Well

- The boundary refusal diagnostics: two-path form + actionable hint made
  every escape-vector diagnosis unambiguous, including distinguishing
  `../` traversal from symlink escape.
- The derived-site-prefix note in `hyalo config` names the exact failure
  mode and both remedies — it is 99× the difference on MDN.
- Unicode end-to-end: Japanese/Cyrillic/emoji/spaced filenames round-
  trip through find, backlinks, BM25, set, task toggle, and mv including
  link rewriting.
- Honest documentation: M-6's staleness warning matches its documented
  blind spot exactly; UX-2's closest-headings suggestion is ranked and
  counts the remainder truthfully.
- The `mutation.rs` writes-classifier resisted adversarial filenames.
- Parallelism: lint runs at 600–850% CPU; MDN full lint in 3.76s.

## Performance

No regressions; two baselines improved. Best-of-3, warm cache, Apple
Silicon.

| Command | Own KB (377) | vscode (760) | GH Docs (3.7k) | MDN (14.4k) |
|---|---|---|---|---|
| `find --limit 1` | 47ms | 87ms | 154ms | 1004ms |
| BM25 query | 161ms | 663ms | 1024ms | 3812ms |
| `summary` | 66ms | 155ms | 413ms | 1343ms |
| property filter | 49ms | 90ms | 157ms | 1089ms |
| `lint` | 85ms | 690ms | 1210ms | 3760ms |
| `links` (read-only) | 73ms | 237ms | 11621ms | see below |

- MDN indexed BM25: 428ms vs 3778ms unindexed = **8.8×**, matching the
  ~10× baseline. Index build 2.7s, 120MB.
- GH Docs `links`: **11.6s vs the 12.7s baseline — slightly improved**;
  the iter-200/203 resolver changes cost nothing.
- **MDN `links` resolves the 206 scoping question**: 84.8s *without* a
  site prefix (49,703 broken links each paying fuzzy-candidate cost) vs
  8.68s *with* the prefix (509 broken) and 5.52s with a matching index.
  Cost tracks broken-count × candidate-set size, not corpus size —
  [[iterations/iteration-206-links-perf-profiling]] should target the
  per-broken-link fuzzy cost, not the 12.7s figure.
- `links fix --apply --apply-fuzzy` (506 files written): 7.5s;
  `links auto --apply` (738 files): 8.3s — both fine.
- iter-201's config resolution costs ~12ms on the lint path (89→101ms).

## Recommendation

The wave held: every claimed fix verified, one contained MEDIUM
regression (BUG-7). Every corruption finding this session is
**pre-existing in released 0.20.0**, so v0.21.0 is strictly safer than
what users run today — releasing now is defensible. But the release's
headline is apply-path integrity, and BUG-1 (HIGH, code-span injection,
3 hits in our own KB) plus BUG-2/3/4 sit in exactly that story.
Suggested: one small pre-release iteration — **inert-zone completion**
(code-span parity model, `{%…%}`/`{{…}}`/HTML-tag zones, BUG-4 target
skip) plus the BUG-7 dedup preference — then cut v0.21.0. The remaining
MEDIUMs (BUG-5/6/8–11) and the UX batch can follow as normal
iterations; BUG-6 and BUG-11's JSON gaps are the natural next wave for
agent-facing trust.
