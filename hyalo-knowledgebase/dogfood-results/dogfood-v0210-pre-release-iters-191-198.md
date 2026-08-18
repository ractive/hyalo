---
title: "Dogfood v0.21.0-pre — iters 191-198 all verified; links apply-path corruption found"
type: research
date: 2026-08-18
status: active
tags:
  - dogfooding
  - links
  - write-path
  - cli-surface
  - performance
related:
  - "[[dogfood-results/dogfood-v0200-opus5-review-round]]"
  - "[[iterations/iteration-191-write-path-integrity]]"
  - "[[iterations/iteration-192-cli-surface-truth]]"
  - "[[iterations/iteration-193-vault-side-effects-and-dep-diet]]"
  - "[[iterations/iteration-195a-auto-link-config-exclusions]]"
  - "[[iterations/iteration-197-auto-link-stopword-heuristic]]"
---

# Dogfood v0.21.0-pre — iters 191-198 all verified; links apply-path corruption found

Binary: `hyalo 0.20.0 (3c0e5c2ee542 2026-08-18)` built from main (carries
iterations 191, 192, 193, 195, 195a, 197, 198 — PRs #227-#233).
Four-agent fan-out (write-path, CLI surface, links.auto family, scale):
own KB (366 files), MDN (14,375 files), GitHub Docs (3,710 files), plus
purpose-built scratch vaults. VS Code docs checkout unavailable this session.
External KBs and the repo KB were left unmodified (verified by snapshot
diffs); all mutation ran on scratch copies.

**Verdict up front: every feature shipped since v0.20.0 works as specified
and all five v0.20.0 findings stay fixed — but the session found two
pre-existing HIGH data-corruption bugs in the `links fix/auto --apply`
paths, both reachable through hyalo's own recommended hints. Recommendation:
hold v0.21.0 for a links-apply integrity fix wave (precedent: v0.18.0 was
held for the okf data-loss class).**

## New feature verification

### Write-path integrity (iter-191) — WORKING, with coverage gaps

Boundary-checked writes hold for `set`, `remove`, `append`, `task toggle`,
`lint --fix`, `mv`, `new` (lexical), `okf index`, `okf log`: every traversal
and symlink escape tried was refused at exit 1 with nothing written outside
the vault. A symlinked vault root works for all writers (the common
`~/notes -> /Volumes/x` setup). Three writers were left out of the sweep —
see H-3, M-3, M-4 below.

### CLI surface truth (iter-192) — WORKING

- All ten verb-alias spellings across five groups produce byte-identical
  output (`tags|properties|types|views|lint-rules` × `list|summary`).
- Config envelope: payload under `.results`, `--jq` works,
  `.results.hints_enabled`, `dir` hoisted for compat, `links.auto` shown in
  both formats.
- Hint execution on live data: 35 hints from 14 diverse commands, 35 exit 0.
  (Two return misleading output — see H-4 — and one mutates — see M-7.)
- Single-file `mv --apply` errors per DEC-064 with a good hint.
- COMMAND REFERENCE contains all 26 subcommands; the `mv` synopsis is
  materially stale (M-9).

### Vault side effects (iter-193) — WORKING

- Full read-only surface plus every `--dry-run` writer: vault tree snapshot
  identical to the nanosecond, zero `.hyalo-case-probe-*` anywhere. Verified
  again at MDN scale (~40 commands over 18k files).
- `chmod 500` read-only vault: reads succeed AND case-insensitive wikilink
  resolution still works.
- `create-index` sweeps stale root probes, leaves fresh ones.
- `out_of_vault` bucketing consistent across `summary`, `links`,
  `find --broken-links`; site-absolute stays `broken` per spec. Note: on
  GitHub Docs the reclassification is only 3 links (the corpus has just 9
  `../` links); the old "6,568 broken" headline is site-absolute links,
  which are the domain of the missing-feature F-1 below, not this bucket.

### Review-drain aliases (iter-195) — WORKING

`create-index --path` / `drop-index --output` identical to originals on
success and failure paths; conflict guard with `--index-file` fires for both
spellings; `summary --help` carries the `-n` divergence FLAG NOTE.

### [links.auto] persistent exclusions (iter-195a) — WORKING

Union semantics exactly right across plain scan, `--index`, `--file`, `-g`
negation, and `--dir` from outside; all eight `first_only` combinations
correct; `config_excluded` present when non-zero, omitted when zero;
`hyalo config` reports all four keys; `--apply` honours everything together
and is idempotent. Case-insensitive matching for titles AND target globs
(the latter undocumented — L-13).

### Common-title note (iter-197) — WORKING as built; heuristic under-fires

One self-extinguishing stderr note; stdout byte-identical with/without
(diffed, both formats); `-q`, `--no-warn-common-titles`,
`warn_common_titles = false` all silence; pasting the suggestion
extinguishes; recomputes against config exclusions; fires on the `--index`
path. Plural stemming works ("policies" flagged via "policy"). The
shell-quoting fix from #232 is correct but CLI-unreachable — the wordlist
only contains single ASCII tokens, so no flagged title can need quoting
(defensive code, unit-tested, fine). The heuristic itself has a
high-value gap — see UX-1.

### --no-first-only (iter-198) — WORKING

Overrides config `first_only = true` for one run (15 → 22 matches); clap
conflict with `--first-only` enforced (exit 2).

## Bug regression testing (v0.20.0 report)

| v0.20.0 finding | verdict |
|---|---|
| BUG-1 `lint --fix` through symlink destroys link (HIGH) | **STILL FIXED** — symlink survives, target rewritten; also verified for `task toggle`, `set`, `append`, `mv` |
| BUG-2 read-only commands bump vault mtime (MEDIUM) | **STILL FIXED** — nanosecond-identical mtime, no probe files, incl. at MDN scale |
| BUG-3 `tags summary` emits broken hint (MEDIUM) | **STILL FIXED** — all emitted hints run; residue: bare `hyalo tags --limit 0` still exits 2 (M-8) |
| BUG-4 `config` breaks JSON envelope (MEDIUM) | **FIXED** — all six sub-checks pass |
| UX-1 no out_of_vault bucket (MEDIUM) | **IMPLEMENTED** — consistent across three surfaces |

## Bugs found

All are **pre-existing** relative to v0.20.0 (none are regressions from
iterations 191-198); they are newly *discovered*, mostly because this is the
first session to point the links apply-paths at a site-absolute corpus.

### HIGH

- **H-1: `links fix --apply` strips the leading `/` from site-absolute
  targets, converting working links into permanently broken ones.** The
  writer emits vault-root-relative targets; the resolver reads bare targets
  as file-relative, so nothing it writes ever resolves — and it proposes
  the identical rewrite forever. On a GitHub Docs copy, plain
  `links fix --apply` (no fuzzy) modified **1,097 files** and the broken
  count went **up** (6,565 → 6,582). This is the exact command hyalo's own
  hint recommends. Minimal repro: `[AUTOTITLE](/how-tos/old-home/moved-page)`
  with the target at `how-tos/new-home/moved-page.md` → rewritten to
  `how-tos/new-home/moved-page` (unresolvable), still "fixable".
- **H-2: `links auto --apply` injects wikilinks inside URL destinations,
  bare URLs, and existing link text.** A page titled `net` turns
  `[x](https://pkg.go.dev/x/actions.summerwind.net/v1)` into
  `…summerwind.[[net]]/v1…` — two working URLs destroyed per line. Inline
  code and whole-text link matches are already excluded; URL contexts and
  substring-inside-label are not. Found organically on GitHub Docs
  `actions/` (title `net`).
- **H-3: `madr toc --apply` has no vault boundary check at all.** Plain
  `../` traversal (no symlink needed) writes/modifies `README.md` outside
  the vault at exit 0; symlinked-dir vector works too. `--dry-run`
  unaffected. Expected the refusal `set`/`okf index` give.
- **H-4: an explicit `--dir` silently discards the entire `.hyalo.toml`**
  (schema types, views, `[lint]` ignores, severity overrides, site_prefix,
  changelog path) while printing a note implying the flag is merely
  redundant. Measured: `lint --strict` 50 files/4 warnings vs
  `lint --dir hyalo-knowledgebase --strict` 366 files/no issues — both
  exit 0, so a CI gate goes vacuously green. Live trap: `hyalo config`
  itself emits `--dir`-bearing hints (e.g. `types list --dir …` → No
  results). Long known as a footgun (our CLAUDE.md warns against `--dir`);
  now measured. Either load the config or say loudly that it is dropped.

### MEDIUM

- **M-1 (medium-high): `LinkCaseMismatch` grabs a same-basename file
  anywhere in the vault at confidence 1.0, in the default apply bucket.**
  Downstream of F-1: `/actions` fails to resolve to `actions/index.md`, so
  the fallback rewrites `[GitHub Actions](/actions)` to
  `graphql/reference/actions.md`. 17 such rewrites applied on GitHub Docs.
  Wrong label, unjustified confidence.
- **M-2: one bad key in `[links.auto]` (or any type error) makes the whole
  `.hyalo.toml` fall back to defaults — including `dir`.** The parse error
  itself is excellent (line/column/caret/valid keys), but the recovery is
  all-defaults, the warning is suppressed by `-q`, and it appears the file
  is parsed twice (dedup notice every run, L-14). Same "config silently
  discarded" family as H-4. `links auto --apply -q` can then rewrite a
  different (usually larger) tree than configured.
- **M-3: `changelog add` / `changelog release --apply` write through a
  `CHANGELOG.md` symlink that resolves outside the vault.** The structurally
  identical `okf log` refuses correctly.
- **M-4: `hyalo new --file` escapes through a symlinked directory** —
  validation is lexical only (`..`/absolute rejected) with no
  canonicalization. Creation-only blast radius.
- **M-5: `links fix --apply` false-fails ("modified by another process",
  exit 1) when a note is reachable via an in-vault symlink** — the walker
  enumerates symlink and target as two files and the second write sees the
  first's mtime. The fix lands; the exit code lies. Hard-fails CI/agent
  loops. Same root: symlink+target double-counting inflates `find --count`,
  `summary`, glob-write counters; out-of-vault symlink warning prints twice.
- **M-6: `find --index` has no staleness signal.** External edits (or hyalo
  writes without `--index`) are silently invisible / stale at exit 0.
  Possibly an accepted snapshot tradeoff (iter-47 design) — but an
  index-mtime vs vault-mtime check would catch most cases cheaply. At
  minimum document the contract.
- **M-7: `find` with combined filters emits a *mutating* hint**
  (`views set …` writes `.hyalo.toml`) rendered identically to read-only
  drill-downs. Executed during verbatim hint sampling; reverted, tree
  clean. Mutating hints need a marker or a separate channel — iter-192's
  own contract is "every hint is safe to run".
- **M-8: `--help`'s limit-contract paragraph is false for 3 of the 8
  commands it names** (`types list`, `views list`, `lint-rules list` reject
  `--limit`; `lint-rules list` is uncapped), and bare `hyalo tags` /
  `properties` reject the flags COMMAND REFERENCE documents for them
  (BUG-3 residue: `hyalo tags --limit 0` still exits 2).
- **M-9: `mv`'s COMMAND REFERENCE synopsis is stale** — omits the
  positional form, all of batch mode, `--allow-ambiguous`. The
  check-command-reference gate verifies presence, not accuracy.
- **M-10: `lint --rule <typo>` returns 0 findings at exit 0** (also
  case-sensitive silently). `lint-rules show` validates; `lint --rule`
  does not. A typo'd CI gate passes green forever.

### Missing feature (high leverage)

- **F-1: no `<target>/index.md` resolution.** `/foo`, `foo`, and even
  `/foo/` are broken when `foo/index.md` exists. Makes MDN read as
  **49,703 of 49,705 links broken (99.996%)** and `backlinks` return 0 for
  MDN's most-linked pages; also the root cause that M-1 exploits.
  `--site-prefix` does not help. Single highest-leverage fix for
  directory-index corpora (MDN, most static-site docs).

### LOW

- L-1: `backlinks` double-counts case-mismatched wikilinks (`[[NOTE]]` → 2
  for one link); `find --fields links` and `summary` count correctly.
- L-2: write commands exit 0 when the single explicitly named file is
  skipped as unparseable ("0/0 modified (1 scanned)").
- L-3: `chmod 444` file silently rewritten (mode preserved); atomic writes
  also break hard links (new inode).
- L-4: `mv` onto a *dangling* symlink silently clobbers the symlink (the
  exists-guard follows symlinks, so dangling reads as absent).
- L-5: `read` errors ignore the piped-JSON default (plain text envelope);
  same inconsistency for its `--count` rejection.
- L-6: bad `find -e` regex bypasses the JSON envelope even with
  `--format json` and leaks the internal `(?i)` flag at a wrong column;
  `--property 'title~=/…/'` handles the same failure correctly.
- L-7: `drop-index` misdiagnoses a missing file as a boundary-check failure
  with an irrelevant `--allow-outside-vault` hint.
- L-8: index files are not byte-reproducible (map iteration order); results
  identical, defeats cache-keying.
- L-9: `create-index -o <custom>` emits a `drop-index` hint *without* the
  path — following it targets the default index in the (read-only) vault
  instead of the custom file.
- L-10: `create-index` text output drops `files_indexed` and the
  "replaced existing index" note that JSON carries (and `--help` promises).
- L-11: `mv` rewrites site-absolute links with the auto-derived site prefix
  injected and appends `.md` to extensionless relative links — inconsistent
  with how they were written (contrast: at least it preserves the `/` that
  H-1 strips).
- L-12: common-title note with >5 offenders truncates the flag list too, so
  one paste-back doesn't fully extinguish (two rounds needed); wording
  should admit "showing the 5 noisiest of 6".
- L-13: note displays lowercased titles (`"readme"` for a page titled
  `README`) — grep-hostile; `exclude_target_globs` case-insensitivity is
  undocumented.
- L-14: malformed-config warning emitted twice + "1 identical warning(s)
  suppressed" every run (config parsed twice?).
- L-15: JSON match positions: `line` is 1-based, `col` 0-based.
- L-16: `okf log`'s boundary refusal exits 2 ("internal error" class)
  where siblings exit 1; `mv` uses two different phrasings for the same
  refusal class.

## UX issues

- **UX-1 (the big one): the common-title heuristic is wordlist-only, so it
  misses the titles that actually dominate a run.** On GitHub Docs
  `actions/`+`repositories/`+`get-started/`, a page titled `Workflows`
  produced **531 of 1,324 proposed links (40%)** — never mentioned; the
  note named `limits` (47×, 3.5%). `metrics` (59), `runner groups` (44),
  `concurrency` (40) also silent. And the ASCII gate means non-English
  vaults never see the note at all. A frequency/share-based trigger
  (any title above N matches or X% of the run) alongside the wordlist would
  catch both and is language-independent. The note *plumbing* (wording,
  self-extinguishing, opt-outs) is exactly right — it's the trigger that
  under-fires.
- UX-2: `read --section` not-found error dumps every heading on one line
  (~4 KB on decision-log.md) — punitive in a terminal, expensive for
  agents. Truncate to closest matches + count.
- UX-3: `links` text output buries the dangerous stuff: bucket summary
  first, then thousands of unlabeled fix lines; `out_of_vault_links` /
  `unfixable_links` JSON-only; "Case mismatches: 17" scrolls away before
  the rewrites it announces.
- UX-4: `hyalo config` prints `site_prefix: (none)` rather than the
  auto-derived effective value — the single field whose derivation breaks
  MDN resolution (F-1 context) is the one you can't inspect.
- UX-5: hints repeat a long `--index-file` path verbatim in every hint
  (4-5×); `config_excluded`'s "Excluded … : 1 titles" reads like a failure
  when the match count doesn't move (it counts candidate titles, not
  links); the `--dir` redundancy note actively misleads (see H-4);
  `[types.note]` config error suggests `schema` but not `hyalo types set`.

## What worked well

- **Zero regressions across five prior findings and eight iterations of new
  surface** — first session ever to verify this much new work with nothing
  broken by it.
- **Encoding fidelity**: BOM, full-CRLF (including CRLF on newly inserted
  lines), and no-trailing-newline files round-trip byte-perfectly through
  every writer; MD047 adds the newline only where wanted.
- **Concurrency**: 20 parallel `set` + 15 parallel `task toggle` — valid
  file every time, last-writer-wins, no temp debris.
- **Unicode/adversarial inputs**: NFC/NFD `café.md`, emoji filenames,
  `weird#name[1].md`, CJK/9,600-char queries — all clean.
- **The snapshot index**: 10× on BM25 (4.10 s → 0.41 s), 2-2.5× elsewhere,
  identical results; 2.4 s build for 14,375 files.
- **Graceful index degradation**: corrupt/missing index → clear warning,
  disk-scan fallback, correct results.
- **Error messages** remain a strength: 14 malformed-input probes on the
  CLI surface all produced correct exit codes and actionable hints; clap
  conflicts are declared, not silently resolved.
- **Empty-vault behavior** clean across ten commands.

## Performance

MDN 14,375 files (scan → indexed): `find --limit 1` 0.96 s → 0.39 s ·
property filter 1.04 s → 0.43 s · `summary` 1.17 s → 0.61 s · BM25 4.10 s →
0.41 s · `lint --count` 3.18 s → 2.63 s · index build 2.41 s (120 MB).
GitHub Docs 3,710 files: `summary` 0.36 s · `lint --count` 0.77 s ·
`lint --fix --dry-run` 0.83 s · **`links` 12.66 s — now the slowest command
measured by 4×, on a KB a quarter MDN's size; worth profiling.**
No command regressed >2× vs the v0.20.0 baselines; BM25 +5%, `summary`
slightly faster. Caveat recorded: `lint --count` counts files-with-
violations, not violations (prior reports conflated this).

## Verdict

**Iterations 191-198: all verified, ship-quality. v0.21.0: recommend HOLD**
until a links-apply integrity wave lands, because H-1/H-2 are data
corruption reachable from hyalo's own hints and the project has held
releases for exactly this class before (v0.18.0/okf). Suggested wave:

1. **links-apply integrity (release blocker):** H-1 (site-absolute
   round-trip), H-2 (URL/label contexts in auto-link), M-1 (retire the
   cross-vault basename fallback or cap its confidence), plus a conformance
   fixture: apply every proposed fix on a site-absolute corpus and assert
   broken-count monotonically decreases.
2. **config trust:** H-4 + M-2 + M-7 (the "config silently discarded" family
   and the mutating hint).
3. **boundary completion:** H-3, M-3, M-4 (+ M-5 symlink dedup in the
   walker).
4. **F-1 `index.md` resolution** as its own feature iteration (biggest
   payoff for external corpora; dissolves M-1's trigger).
5. Low/doc batch at leisure: M-8/M-9/M-10 + Ls.

Repro artifacts (session-temporary, will vanish with the scratchpad):
`df-writepath/`, `df-cli/`, `df-linksauto/` (noisy, quote2, ghdocs …),
`df-scale/` (minrepro=H-1, autorepro=H-2, idxtest=F-1, mvtest=L-11, oov,
ghcopy, mdn.idx) under the session scratchpad. Every repro is also written
out as exact commands above.
