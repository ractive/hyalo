---
type: research
title: Stability retrospective — dogfood findings and semantic surface
date: 2026-09-06
status: completed
---

# Stability retrospective — dogfood findings and semantic surface

Dogfooding keeps finding defects partly because the testbeds and input shapes keep widening,
and partly because fixes change semantics behind familiar commands. It is not evidence that
every prior fix regressed: both post-batch reports re-ran their original cases and reported
zero regressions in those cases. Their new probes then found untested layouts, incomplete
contracts and new mistakes. Passing the original reproducer establishes only that case.

This analysis records **232 observations: 138 real-defect observations, 18 edge/data/process
observations and 76 preferences or extensions**. These are report rows, including explicit
carry-overs and repeated wishes, not 232 independent bugs. Of the 138 defect observations,
36 are attributed to recent feature/fix batches, 77 are latent/carry-over, and 25 have
unestablished age. Exact introduction remains unknown for 94; do not substitute a fix's
test-addition date for its origin. Age attribution is historical inference, not a replay
of old binaries.

The largest descriptive families are output/help/contracts/recipes (74), boundary/mutation/
grammar (32), and semantic parity across paths (26); six others concern cost, scoring or a
false ecosystem premise. The narrower six-area audit below directly supports alternate-path
divergence in 23 observations. It does **not** establish that duplicated computation caused
most bugs, or that typed structs alone would prevent the 74 output/help observations.

## Scope, sources and method

Baseline `origin/main` was `5f3a91289843d6a97ab822f9dca563abf4ec85d0`, after the merged
iteration-288 CLI verification/defer note. Three independent research tracks covered triage,
surface and defect classes. All KB discovery/reads used Hyalo; historical source and tests
used read-only `git show`, `git log` and `git blame`. No historical binaries, external vaults
or source files were changed. Existing test evidence was inspected, not executed.

| Prefix | Report | Defects | Edge/data/process | Preference/extension | Total |
|---|---|---:|---:|---:|---:|
| H | [[dogfood-results/dogfood-v0220-help-efficiency-and-find-shape]] | 23 | 4 | 10 | 37 |
| O | [[dogfood-results/dogfood-v0220-obsidian-vaults]] | 31 | 7 | 26 | 64 |
| A | [[dogfood-results/dogfood-v0220-post-batch-261-270]] | 39 | 5 | 20 | 64 |
| B | [[dogfood-results/dogfood-v0220-post-batch-271-274]] | 45 | 2 | 20 | 67 |
| Total | Four report observation ledgers | 138 | 18 | 76 | 232 |

A reports 29 BUG + 25 UX items; B reports 47 BUG + 14 UX items. This ledger additionally
includes feature-gap bullets, unnumbered wishes, explicit still-open carry-overs and O's
data/process/coverage notes. Compound heading aliases occupy one row unless they describe
separate findings (COH-12/13 and HELP-14/15 are split). Already-FIXED confirmations, positive
verification and performance tables repeating a named finding are not additional rows.
Repeated observations remain separate so every source finding is traceable; these counts
must not be interpreted as deduplicated defect identities or as a severity ranking.

**D** means a defect in an existing behavioral, executable or documented contract.
**E** means an edge/data/process/coverage issue. **P** means a preference or extension,
including behavior explicitly permitted by the old contract. A report's BUG label is not
the classification. For example, A-BUG-6 requested bare-alias resolution on an incorrect
Obsidian premise; implementing that preference created B-BUG-1. A-BUG-1 remains outside
the agreed use case under DEC-292.

**R** means newly introduced defective behavior/regression attributable to the immediately
preceding batch (H: 251–252; O: 254–260; A: 261–270; B: 271–274), including an incomplete
new feature. It does not mean every row has a formerly passing test. **L** means latent or
carry-over according to the report or pre-batch feature history; **U** means unestablished.
For P/E rows this is feature/observation age, not a regression claim. Numeric introduction
entries are historical attributions; `unknown` is explicit uncertainty and `n/a` means
there is no product-defect introduction to identify. The fix column identifies follow-up
work or a decision, not a claim that every item was implemented.

## Surface growth is chiefly semantic

| Reproducible measure | v0.18.0 | v0.21.0 | Baseline main |
|---|---:|---:|---:|
| Top-level Commands enum variants | 26 | 26 | 27 |
| All Subcommand enum variants in args.rs | 52 | 52 | 53 |
| Literal arg attributes in args.rs | 186 | 202 | 206 |
| Repository .hyalo.toml leaf assignments | 61 | 68 | 68 |
| Repository .hyalo.toml table headers | 47 | 55 | 55 |
| Named config.rs deserializer members, qualified subset | 27 | 36 | 40 |
| Canonical fields selectors, including all | 9 | 9 | 13 |
| Fields spellings, including aliases | 10 | 10 | 15 |
| Semantic property-filter operations | 9 | 9 | 13 |
| Property-filter operator spellings | 10 | 10 | 9 |

Tags resolve to `e23ee4b6d330e26b54f8536588680a72ccceb446` and
`593f8237ac2b64844c6abba51140c2a339a5dd17`. The only new top-level variant is Help.
The total includes group containers and child actions, excludes clap-generated help and
aliases, and is not a count of executable leaves. Arg attributes include repeated and
positional declarations: +20 since v0.18.0 (+10.8%), only +4 since v0.21.0 (+2.0%).

Repository config assignments include this vault's schemas; they are not a census of
supported product settings. The supplemental config.rs subset includes named deserializer
members and table containers, excludes arbitrary schemas/views/rules and runtime defaults.
Its four additions after v0.21.0 are links.frontmatter, links.aliases, scan.exclude and
scan.verbose_skips. Stable checked-in config therefore does not imply stable config API.

Both tags' canonical fields are all, properties, properties-typed, tags, sections, tasks,
links, backlinks and title; outline aliases sections. Main adds file/modified/size/lines,
plus the properties_typed alias. Companion output keys such as title_source are excluded.
Both tags support presence, absence, equality/inequality, four ordering operations and
regex. Main adds null/not-null and empty-list/not-empty-list semantics through existing
syntax. It removes the =~ regex spelling, so semantic operations grow while spellings
shrink. Boolean composition and every find flag are outside this specific metric.

### Surface reproduction

Run from the repository root. These exact read-only commands were executed; no checkout
is required. The config/member counts are deliberately limited as defined above.

```sh
for rev in v0.18.0 v0.21.0 5f3a91289843d6a97ab822f9dca563abf4ec85d0; do
  git rev-parse "${rev}^{commit}"
  git show "${rev}:crates/hyalo-cli/src/cli/args.rs" |
    awk '/^#\[derive\(Subcommand\)\]/{subcmd=1}
      subcmd && /^pub.*enum / {name=$3; count=0}
      subcmd && /^    [A-Z][A-Za-z0-9_]*[[:space:]]*[({,]/ {count++}
      subcmd && /^}/ {print name,count; total+=count; subcmd=0}
      END {print "TOTAL",total}'
  git show "${rev}:crates/hyalo-cli/src/cli/args.rs" |
    rg -F -o '#[arg(' | wc -l
  git show "${rev}:.hyalo.toml" |
    awk '/^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=/{n++}
      END {print "CONFIG_LEAF_ASSIGNMENTS",n}'
  git show "${rev}:.hyalo.toml" |
    awk '/^\[/{n++} END {print "CONFIG_TABLES",n}'
  git show "${rev}:crates/hyalo-cli/src/config.rs" |
    awk '/^struct [A-Za-z]+Config|^struct ConfigFile|^struct CaseInsensitiveTable/ {name=$2; count=0; a=1}
      a && /^    [a-z_]+:/{count++}
      a && /^}/{print name,count; total+=count; a=0}
      END {print "CONFIG_MEMBERS_SUBSET",total}'
  git show "${rev}:crates/hyalo-core/src/filter/fields.rs" |
    sed -n '/match part {/,/unknown =>/p' | rg '^ *"' |
    cut -d= -f1 | rg -o '"[^"]+"' | sort -u
  git show "${rev}:crates/hyalo-core/src/filter/parse.rs" |
    awk '/^pub enum FilterOp/{a=1;next}
      a && /^}/{a=0}
      a && /^    [A-Z][A-Za-z]+,/{print; n++}
      END {print "FILTER_OP_VARIANTS",n}'
done
```

The fields pipeline lists accepted spellings; exclude outline and properties_typed when
counting canonical selectors. FilterOp has 7/7/11 variants; add PropertyFilter's Absent
and RegexMatch to obtain 9/9/13 concepts. Inspect the respective parse.rs files to confirm
those two variants and the accepted regex spellings. Both tags explicitly implemented =~
as RegexMatch. DEC-276's explanation that it was never implemented conflicts with source;
its removal is a compatibility contraction, not merely rejection of an accidental input.

### Per-batch default and contract changes

These lists are reconstructed from individual [[decision-log]] entries; the log did not
already contain literal per-batch lists. They describe effects on unchanged invocations,
not additional regressions.

- **251–260:** DEC-252 shrinks find's default shape and promotes title; DEC-254 makes explicit
  fields an exact projection and replaces saved-view pins; DEC-255 promotes scalar titles
  and adds HYALO007; DEC-257 adds mutation dry_run/bulk counters; DEC-258 makes help choose
  the short page. DEC-256 retains raw read text and is not a shipped default change.
- **261–270:** DEC-266 resolves non-md attachments and bars cross-extension fuzzy repairs;
  DEC-267 folds link case by default; DEC-269 broadens frontmatter links; DEC-271/272 narrow
  MD018/MD001 autofixes; DEC-273 changes link-count sort direction; DEC-274 adds typed
  null/list/comparison semantics; DEC-276 removes =~ and rejects empty regex/fields;
  DEC-278 collapses skip diagnostics; DEC-279 makes malformed config fatal for gates;
  DEC-282 expands parent-tag rename; DEC-283 adds filename-title fallback; DEC-284 lets
  named lint files bypass ignore; DEC-286 adds the auto-link stop-list; DEC-290 refuses
  explicit validation without a usable schema.
- **271–274:** DEC-293 makes indented frontmatter closers content; DEC-294 adds suppression
  directive grammar; DEC-295 suppresses site-prefix cosmetic rewrites and preserves emitted
  form; DEC-296 enables aliases; DEC-297 inventories images; DEC-301 changes named-file
  failures/upserts and rule-filtered parse diagnostics; DEC-307 regularizes user-error
  output and OKF dry-run exits. Amendments reject empty comparison operands and honor
  an explicitly empty stop-list.
- **275–281, with 282 queued:** DEC-308 disables aliases again and proposes deterministic
  repairs; DEC-309–311 expand anchor/target normalization; DEC-312 stops required from
  implying string; DEC-317 changes bulk-write durability scheduling; DEC-318 unifies
  note-edge predicates. DEC-319/324–326 repeatedly change fuzzy admission/confidence under
  stable flags and the same 0.8 apply floor. Iteration 282 is not yet a shipped change.

Read the cited decisions with `hyalo read decision-log.md --section '/^DEC-NNN:/'`.
The alias on/off reversal, null-title to stem fallback and scorer chain explain why
counting new commands alone understates the cost of the fix batches.

## Defect classes and prevention

Each D row has exactly one descriptive primary family in the ledger: **C** output/help/
contracts/recipes (74; 53.6%), **G** boundary/mutation/grammar (32; 23.2%), **S** semantic
parity (26; 18.8%), or **X** other (6; 4.3%). A compound row is assigned once to its most
concrete failure; these are observation categories, not experimentally established causes.
The top three cover 132/138 observations. Each has a cheap guard:

| Primary family | Defect observations | Prevention |
|---|---:|---|
| C: output/help/contracts/recipes | 74 | Execute literal shipped examples; compare output-family keys and meaningful text/JSON facts, including one complete JSON error stream. |
| G: boundary/mutation/grammar | 32 | Extend exact-byte round-trip and protected-region fixtures with the trigger plus a neighboring valid syntax case. |
| S: semantic parity | 26 | Exercise the same small fixture through relevant selectors, disk/index and reader/rewrite paths, including nested and changed-snapshot cases. |
| X: cost, scoring, ecosystem premise | 6 | Check the claimed external semantics first; retain a small positive/negative scorer set and the existing scale measurements. |

The six areas explicitly requested by iteration 284 are narrower and overlap the broad
families above. Their 30 rows are disjoint within this table; **do not add these counts to
the primary-family totals**. Exactly 23 demonstrate alternate-path divergence; the other
seven concern a wrong shared predicate, byte preservation, versioning, documentation or
error routing.

| Requested area | D rows | IDs | One prevention |
|---|---:|---|---|
| Disk scan / index | 8 | O-BUG-8, O-BUG-14, O-UX-7; A-BUG-11/12/18; B-BUG-12/24 | Compare fresh and changed snapshots, skip/exclude counts and named-file refusal/warning behavior. |
| Summary / find note-edge predicate | 1 | B-BUG-16 | Assert orphan/dead-end count equality with real/missing attachments and dotted note stems. |
| Reader / move resolution and path normalization | 8 | O-BUG-1; A-BUG-7/14; B-BUG-2/3/8/9/10 | Root/nested moved/twin/source matrix: every inbound link is rewritten or explicitly skipped. |
| Text / JSON factual rendering | 4 | O-UX-4; A-UX-10; B-BUG-32; B-UX-2 | Render one result fixture both ways and compare counts, alias provenance and property context. |
| Frontmatter fences / emitter | 3 | A-BUG-2; B-BUG-33/34 | Parse → unrelated mutation → parse preserves values, body and unaddressed fence bytes. |
| Output contract / error pipeline | 6 | O-BUG-22; H-COH-9; A-BUG-25; B-BUG-17/25/35 | Required/omitted-key and single/batch contract fixtures; exactly one JSON error envelope. |

The direct-divergence count excludes A-BUG-12, B-BUG-12, A-BUG-2, B-BUG-33, H-COH-9,
A-BUG-25 and B-BUG-35. For example, fresh-index equality cannot catch an old-version
snapshot or a stale-probe tolerance gap. A clean root-only fixture cannot catch skipped
subdirectories or B-BUG-3's nested ambiguity guard.

Three counterexamples constrain the recommendation. DEC-293 already had a shared delimiter
helper from iteration 183; A-BUG-2 arose because the common trim-based rule was wrong.
At B's revision, mv's single and batch results were already serializable named structs,
yet B-BUG-25 exposed their incompatible totals. FixPlan already had optional emitted_target;
B-BUG-17 omitted planning for below-floor fuzzy candidates. Serialization faithfully
preserved incomplete state. Typed final contracts help, but merely deriving Serialize
does not decide correct semantics or prevent a direct stderr write outside the pipeline.

DEC-318 is strong evidence for sharing a concept, with a qualification: graph/find had
different attachment information, and the first shared edge predicate needed a dotted-note
fixture during review. The current is_note_graph_edge accepts attachment context. Code
sharing therefore needs correct inputs and boundary cases, not a promise of automatic parity.

### Existing evidence, not newly executed tests

All paths below are under `crates/hyalo-cli/tests/e2e/`; line numbers refer to the fixed
baseline. These examples show that small guard families already exist.

| Finding / mechanism | Test evidence | Blamed test commit |
|---|---|---|
| A-BUG-2 emitter/reader round-trip | iteration271_write_rewrite_safety.rs:145 | ff12511d8 |
| B-BUG-3 nested ambiguity matrix | iteration275_alias_and_mv_guards.rs:67 | 2c9be05c4 |
| B-BUG-33/34 fence bytes | iteration276_autofix_config_index_honesty.rs:433 | a8a243779 |
| O-BUG-22 selector results shape | iteration264_find_consistency.rs:444 | df1e25e3c |
| A-BUG-18 excluded-count parity | iteration273_named_file_honesty.rs:361 | 8ee92159a |
| B-BUG-16 graph parity | iteration277_graph_parity_and_write_perf.rs:57 | 93dbcbbae |

For any numeric fix track, inspect the corresponding iteration E2E suite's report-ID
comments and named test, then run `git blame -L START,END BASE -- PATH`. The research
captured function-declaration blame for all 16 suites covering 254/255, 261–267 and 271–277.
A test's commit establishes fix evidence only. For example, H-FIND-2 traces to compact
defaults in 9c3f1102 (252), then fix 5efda5a7 (254); H-FIND-3 additionally traces to title
stripping 4029ea0e. The test suite's 49c19afc commit did not introduce either bug.

Other explicit feature-to-fix chains: list binding bdeb36fe (266) → d9cf6337 (272);
skip collector 73d612b3 (265) → named-error correction 8ff64e14 (273);
alias feature 3caa5167 (272) → 1ee6acf1 / 80b8a181 (275);
suppression parser 7716bcb5 (271) → eec4d17e (276);
image inventory f1568716 (272) → graph correction fa275015 (277).
For A-BUG-2, `git show bb4c2851^:crates/hyalo-core/src/frontmatter/parse.rs` shows the
wrong shared closer; `git log -S 'fn is_closing_delimiter' -- crates/hyalo-core/src/frontmatter/parse.rs`
finds cd3cad66 (183), which consolidated an already-lenient behavior. Its original
introduction is still unknown.

## What to stop, what to retain, and scheduling

Stop converting a corpus wish directly into a default before checking its semantics;
A-BUG-6 → B-BUG-1 is the concrete warning. Stop treating one original repro, one shared
helper or one named struct as coverage of a whole concept. Stop making recipe gates
silently add dry-run: B-BUG-6's gate concealed the unsafe literal example.

Retain the four cheap guard families already represented in tests: semantic parity;
mutation round-trip/protected bytes; output-family contracts; literal recipe execution
and safety. Extend the relevant existing fixture with each fix, including its nearest
positive and negative neighbor. Record intentional before/after defaults with the
consumer that must migrate. This is a small test discipline, not new acceptance-criteria
machinery; DEC-328 records it.

| Queued work | Release recommendation and reason |
|---|---|
| [[iterations/iteration-282-directory-token-dominance-rule]] | Before 0.22.0: closes a known 281 regression, with positive relocation and negative basename cases retained. |
| [[iterations/iteration-283-ranked-search-snippets]] | After 0.22.0 for stability alone: useful new result behavior; not required to close the audited defects. If run now, pin regex/BM25 field and section parity. |
| [[iterations/iteration-285-typed-output-structs]] | Before 287; preferably before release if its existing compatibility scope stays bounded. It prepares a public contract, not a demonstrated cure for most defects or an independently proven release blocker. |
| [[iterations/iteration-286-npm-distribution]] | After 0.22.0 stabilization: adds packaging/platform combinations; no audited defect requires npm delivery. Independent of 285. |
| [[iterations/iteration-287-typed-typescript-api]] | After 285 and 286 and stabilized CLI contracts; generated types plus same-commit binary contract tests reduce a second schema-maintenance path. |
| [[iterations/iteration-288-codex-integration]] | Keep desktop verification deferred and its task unchecked; merged CLI smoke evidence does not establish desktop behavior. |

These are recommendations, not changes to the owner's authorized execution order
284 → 282 → 283 → 285 → 286 → 287, and not release authorization. The user may complete
the queued features before deciding when to release.

## Full observation ledger

Every row cites its source through the prefix mapping above and original ID/heading.
G1… labels follow feature bullets in source order; A-W1…4 are the final unnumbered wishes;
O-DATA, O-PARTIAL, O-PROCESS and O-COVERAGE retain the additional observations.
H-R rows cover still-open prior BUG-2, UX-3, UX-5, UX-4 and UX-6 respectively;
O-R rows cover prior UX-4, HELP-14, COH-12, COH-13 and COH-17 respectively.
Family is C/G/S/X for D rows and a dash otherwise.

### H observations

| ID | Kind | Age | Introduction | Fix track | Family | Finding |
|---|---|---|---|---|---|---|
| H-FIND-2/COH-14 | D | R | 252 | 254 | C | Compact default removes tasks used by shipped tidy recipe |
| H-COH-6/HELP-1 | D | R | 251 | 254 | C | Physical-line help split creates sentence fragments |
| H-COH-3/COH-4 | D | R | 252 | 254 | C | Find headline and root cookbook retain previous shape |
| H-HELP-3 | D | U | unknown | 254 | C | Views help advertises flags not forwarded |
| H-COH-10 | D | L | unknown | 254 | C | Shipped lint fix-rule example lacks required fix flag |
| H-FIND-3 | D | R | 252 | 254 | C | Non-string title lost from default projection |
| H-FIND-1 | D | R | 252 | 254 | C | Fields footer names rejected always-on fields |
| H-COH-9 | D | L | unknown | 256 | C | Universal mutation envelope promise false |
| H-COH-15 | D | L | unknown | 254 | C | NBSP help examples cannot paste into shell |
| H-COH-7 | D | L | unknown | 254 | C | Help embeds NUL/newline instead of escape text |
| H-FIND-9 | E | L | unknown | 255 | — | Stale-probe tolerance undocumented at one-second boundary |
| H-FIND-8 | E | L | unknown | 256 | — | All-fields indexed projection costs 20 percent extra |
| H-COH-1/HELP-4/COH-2 | D | R | 251 | 254 | C | Three global-option inventories disagree |
| H-HELP-2 | P | R | 251 | 254 | — | Shared input help paragraphs defeat short-page efficiency |
| H-HELP-5/HELP-13 | P | L | n/a | 256 | — | Want help command short forwarding and typo parity |
| H-HELP-7 | D | L | unknown | 254 | C | Task line coordinate convention undocumented |
| H-HELP-6/COH-11 | D | L | unknown | 254 | C | mv filter operators stale relative to parser |
| H-COH-5 | D | R | 252 | 254 | C | Read help omits newly added size/lines and large-body hint |
| H-FIND-7 | P | R | 252 | decision | — | Prefer large-body warning before read cost |
| H-FIND-5 | P | R | 252 | 254 | — | All-fields hint wording vague on limit-one results |
| H-HELP-8 | P | R | 251 | 254 | — | Long value placeholders trigger larger two-line layout |
| H-COH-16/FIND-6 | D | R | 252 | 254 | C | Same-page orphan/dead-end field promises disagree |
| H-COH-12 | D | L | unknown | 264 | C | Score sort undocumented |
| H-COH-13 | D | L | unknown | 264 | C | Help rejects supported =~ regex alias |
| H-COH-17 | D | R | 251 | 267/274 | C | Zero-result stderr/stdout ordering inverted |
| H-COH-8 | D | L | unknown | 254 | C | Rustdoc internal link leaks into help |
| H-HELP-12 | D | L | unknown | 254 | C | Placeholder drift plus unquoted zsh and wrapped jq recipes |
| H-HELP-14 | P | L | n/a | 267 | — | Want summary JSON keys in short help |
| H-HELP-15 | P | R | 251 | decision | — | Prefer filters before index flags in short help |
| H-FIND-4 | D | L | unknown | 254 | C | Blank title prevents useful H1 fallback |
| H-UX-7 | E | L | unknown | 254 | — | Flags after double dash become positionals as designed |
| H-ROOT1 | P | R | 251 | 256 | — | Root grouping misplaces index writers; examples lack read/task |
| H-R1 | D | L | unknown | 255 | S | No-op indexed set leaves changed disk entry stale |
| H-R2 | D | L | 246 | 255 | C | UTF-8 placeholder falsely says search is lossy |
| H-R3 | P | L | n/a | 255 | — | Want new property flag or actionable follow-up |
| H-R4 | P | L | n/a | 267 | — | Want explicit lint-ignore override |
| H-R5 | E | L | unknown | 254 | — | Leading-dash search needs double dash; repeated by UX-7 |

### O observations

| ID | Kind | Age | Introduction | Fix track | Family | Finding |
|---|---|---|---|---|---|---|
| O-BUG-1 | D | L | unknown | 262 | S | Frontmatter discovery hard-codes related and mv rewrites none |
| O-BUG-2 | D | L | unknown | 261 | G | Non-http external schemes counted broken |
| O-BUG-3 | D | L | unknown | 263 | G | MD018 corrupts Obsidian tags |
| O-BUG-4 | P | L | n/a | 264 | — | Link-count sort default differs from other keys |
| O-BUG-5 | D | L | unknown | 261 | G | Existing non-markdown targets broken and fuzzy crosses extensions |
| O-BUG-6 | D | L | unknown | 261 | G | Attachment basename and relative resolution missing |
| O-BUG-7 | D | L | unknown | 261 | G | Table-escaped wikilink alias pipe misparsed |
| O-BUG-8 | D | L | unknown | 265 | S | Indexed links auto aborts instead of skipping bad YAML |
| O-BUG-9 | D | L | unknown | 263 | G | Nested image links confuse MD034 and MD042 |
| O-BUG-10 | P | L | n/a | 261 | — | Want case-insensitive graph parity with Obsidian |
| O-BUG-11 | P | L | n/a | 266 | — | Want index flag on properties and tags |
| O-BUG-12 | D | L | unknown | 266 | G | Property rename changes untouched value bytes and position |
| O-BUG-13 | P | L | n/a | 266 | — | Want non-scalar type binding; prior string contract explicit |
| O-BUG-14 | D | U | unknown | 265 | S | Full index and disk disagree on invalid UTF-8 BM25 input |
| O-BUG-15 | D | L | unknown | 266 | S | Tag parent filter and rename disagree on subtree |
| O-BUG-16 | D | L | unknown | 266 | S | Summary counts name-type pairs as property names |
| O-BUG-17 | P | L | n/a | 264 | — | Want null property filter |
| O-BUG-18 | P | L | n/a | 264 | — | Want typed comparisons and null-last sorting |
| O-BUG-19 | E | L | unknown | 265 | — | Documented malformed-config fallback unsafe for lint gates |
| O-BUG-20 | P | L | n/a | 264 | — | CLI hyphen and JSON underscore naming differ |
| O-BUG-21 | D | U | unknown | 264 | C | Filename projection emits extra blank record |
| O-BUG-22 | D | U | unknown | 264 | C | File-input modes return incompatible results shapes |
| O-BUG-23 | P | L | n/a | 264 | — | Want empty regex rejected; matching all is normal regex behavior |
| O-BUG-24 | P | L | n/a | 264 | — | Want empty fields rejected |
| O-BUG-25 | D | L | 252 | 267 | C | Help says promoted-title search is frontmatter-only |
| O-UX-1 | D | L | unknown | 265 | C | Repeated multiline skip warnings swamp results |
| O-UX-2 | D | L | unknown | 265 | C | Summary hides skipped files |
| O-UX-3 | P | L | n/a | 267 | — | Want multiword-search positional diagnostic |
| O-UX-4 | D | L | unknown | 262 | C | mv text omits link rewrite total |
| O-UX-5 | P | L | n/a | 267 | — | Want filename-stem title fallback |
| O-UX-6 | D | U | unknown | 261 | C | Broken-link recipe excludes anchors; link kind not exposed |
| O-UX-7 | D | L | unknown | 265/267 | S | Named find snapshot stale unlike mutation refresh |
| O-UX-8 | D | L | unknown | 261/267/279 | X | Fuzzy proposes unrelated targets and zero confidence |
| O-UX-9 | P | L | n/a | 267 | — | Want auto-link common-title stop-list |
| O-UX-10 | P | L | n/a | 263 | — | MD001 caption hierarchy policy differs from author intent |
| O-UX-11 | D | L | unknown | 262 | C | List-of-wikilinks text ambiguous triple brackets |
| O-UX-12 | P | L | n/a | 262 | — | Want warning for intentional scalar replacement of list |
| O-UX-13 | D | U | unknown | 267 | C | Wrong index typo hint and empty-state/stale warning ordering |
| O-UX-14 | D | L | unknown | 267 | C | Multiline mutation values render as apparent keys |
| O-UX-15 | P | L | n/a | 266 | — | Want raw frontmatter instead of parsed YAML display |
| O-UX-16 | D | L | unknown | 263 | C | Autofix conflicts lack actionable text context |
| O-UX-17 | P | L | n/a | 267 | — | Want new preview and non-plausible scaffold defaults |
| O-UX-18 | D | U | unknown | 267 | C | Help/config/github/error formats drift across channels |
| O-G1 | P | L | n/a | 261/262/263 | — | Obsidian profile wish groups BUG-1/2/3/7/10 |
| O-G2 | P | L | n/a | 261 | — | Non-markdown shortest-path wish repeats BUG-5/6 |
| O-G3 | P | L | n/a | 265 | — | Want vault-wide scan exclude |
| O-G4 | P | L | n/a | 264 | — | Want null-aware filters and property-type selector |
| O-G5 | P | L | n/a | 261 | — | Want link kind filter/report/grouping |
| O-G6 | P | L | n/a | 266 | — | Want flexible schema binding and inference |
| O-G7 | P | L | n/a | 266 | — | Index on readers wish repeats BUG-11 |
| O-G8 | P | L | n/a | 261 | — | Want anchor-prefix matching or suggestions |
| O-DATA1 | E | L | n/a | data | — | KB shortened DEC anchors correctly rejected |
| O-DATA2 | E | L | n/a | data | — | KB iteration date is in future |
| O-DATA3 | E | L | n/a | data | — | Dogfood sentinel already occurs in old reports |
| O-PARTIAL1 | D | R | 254 | 267 | C | Residual NBSP help and init/deinit global pointer mismatch |
| O-PARTIAL2 | D | L | unknown | 267 | C | JSON cookbook still promises universal skipped_count |
| O-PARTIAL3 | E | L | unknown | 256 | — | Stem-dedupe optimization shows no measurable GH Docs benefit |
| O-R1 | P | L | n/a | 267 | — | Named lint-ignore override still absent; H-R4 carry-over |
| O-R2 | P | L | n/a | 267 | — | Summary result keys not in short help; H-HELP-14 carry-over |
| O-R3 | D | L | unknown | 264 | C | Score sort undocumented; H-COH-12 carry-over |
| O-R4 | D | L | unknown | 264 | C | Documented-invalid =~ accepted; H-COH-13 carry-over |
| O-R5 | D | L | 251 | 267/274 | C | Zero-result streams interleave; H-COH-17 carry-over |
| O-PROCESS1 | E | L | n/a | process | — | Earlier external checkout edits discarded before preserve instruction |
| O-COVERAGE1 | E | R | 258 | coverage | — | Title-regex hint beyond-cap late-file case not constructed |

### A observations

| ID | Kind | Age | Introduction | Fix track | Family | Finding |
|---|---|---|---|---|---|---|
| A-BUG-1 | E | L | known by 122 | decision | — | Concurrent same-file writes lose updates; explicitly declined DEC-292 |
| A-BUG-2 | D | L | unknown | 271 | G | Indented scalar fence truncates frontmatter and corrupts writes |
| A-BUG-3 | D | L | unknown | 271 | G | MD031 edits an unterminated code block |
| A-BUG-4 | D | L | unknown | 271 | G | site_prefix case rewrite appends index and preview differs |
| A-BUG-5 | D | R | 266 | 272 | S | New list-type binding rejected by implicit string validation |
| A-BUG-6 | P | L | n/a | 272 then 275 | — | Alias-resolution request rests on false Obsidian premise |
| A-BUG-7 | D | R | 262 | 271 | S | Frontmatter rewrite lacks body ambiguity guard |
| A-BUG-8 | D | U | unknown | 272 | G | Same-page markdown links lose kind and label |
| A-BUG-9 | D | U | unknown | 273 | S | Named/glob broken-link path drops anchor fields |
| A-BUG-10 | D | R | 265 | 273 | S | Named unparsable file becomes empty success |
| A-BUG-11 | D | L | unknown | 273 | S | Named file missing from snapshot becomes empty success |
| A-BUG-12 | D | L | unknown | 273 | S | New stale probe misses in-place writes |
| A-BUG-13 | D | L | unknown | 271 | G | Empty rename target writes empty property key |
| A-BUG-14 | D | L | unknown | 273 | S | Destination normalization nests vault directory |
| A-BUG-15 | E | L | unknown | 272 | — | Malformed triple-bracket flow list capture fails |
| A-BUG-16 | E | L | unknown | 272 | — | Malformed nested wikilink capture consumes prose |
| A-BUG-17 | D | L | 251 | 274 | C | Zero-result hint confuses absent key with absent value |
| A-BUG-18 | D | R | 265 | 273 | S | Snapshot loses new excluded count |
| A-BUG-19 | D | U | unknown | 274 partial;276 | G | False case mode emits noncanonical path and config hides value |
| A-BUG-20 | D | U | unknown | 273 | S | Rule-filtered lint includes unrelated parse errors |
| A-BUG-21 | E | L | unknown | 272 | — | Parenthesized scheme target misclassified internal |
| A-BUG-22 | D | U | unknown | 274 | C | Hint names nonexistent site config key |
| A-BUG-23 | D | R | 267 | 274 | G | Empty configured stop-list does not replace defaults |
| A-BUG-24 | D | L | unknown | 273 | S | mv conflict policy invalid or ignored |
| A-BUG-25 | D | U | unknown | 274 | C | JSON caller errors become plain text and wrong exit class |
| A-BUG-26 | D | L | unknown | 273 | C | Single-source collision says multiple sources |
| A-BUG-27 | D | L | 252 | 274 | C | Round-trip fields footer advertises rejected score |
| A-BUG-28 | D | L | unknown | 271 | G | MD019 rewrites code comments |
| A-BUG-29 | D | U | unknown | 274 | C | Shipped jq IN recipe cannot execute |
| A-UX-1 | P | L | n/a | 274 | — | Exit taxonomy needs explicit clap convention |
| A-UX-2 | D | L | unknown | 274 | C | Indexed hints drop index context |
| A-UX-3 | D | L | unknown | 274 | X | Site diagnostic arrives after wasted fuzzy scoring |
| A-UX-4 | D | R | 264 | 274 | C | Sort warning depends on limit |
| A-UX-5 | P | L | n/a | 274 | — | Schema rule group not selectable |
| A-UX-6 | D | R | 267 | 274 | C | Empty scaffold fields produce redundant errors |
| A-UX-7 | D | L | 251 | 274 | C | Types long-help indentation leaks |
| A-UX-8 | D | U | unknown | 274 | C | Changelog error suggests unsupported property flag |
| A-UX-9 | P | L | n/a | 274 | — | Named-list missing/empty conventions need documentation |
| A-UX-10 | D | L | unknown | 274 | C | Fuzzy text says below-floor candidates applied |
| A-UX-11 | D | R | 267 | 274 | C | Stop-list note noisy and misstates flag composition |
| A-UX-12 | P | L | n/a | decision | — | Config inspection fallback differs intentionally from execution |
| A-UX-13 | P | L | n/a | 274 | — | Want per-rule fix totals and truncation hint |
| A-UX-14 | P | L | n/a | 274 | — | Override null confused with effective enabled value |
| A-UX-15 | P | L | n/a | 274 | — | Want case-insensitive title collation |
| A-UX-16 | D | L | unknown | 274 | C | H1 display retains invisible HTML comments |
| A-UX-17 | D | L | unknown | 274 | G | Task JSON text retains CR from CRLF |
| A-UX-18 | D | L | unknown | 274 | G | Deinit nonexistent directory succeeds |
| A-UX-19 | D | U | unknown | 274 | G | First schema silently enables write validation |
| A-UX-20 | P | L | 176 | 274 | — | OKF dry-run drift exit differs by design |
| A-UX-21 | E | L | unknown | 274 | — | Malformed filter operands silently match nothing |
| A-UX-22 | D | U | unknown | 274 | C | Prefix warning counts already-resolved wikilink |
| A-UX-23 | P | L | n/a | decision | — | YAML numeric display differs from Obsidian |
| A-UX-24 | P | L | n/a | decision | — | Touched nested tag value changes YAML style within contract |
| A-UX-25 | D | U | unknown | 274 | C | Index creation hint recommends invalid combination |
| A-G1 | P | L | n/a | 272 | — | Markdown image inventory requested |
| A-G2 | D | L | unknown | 271 | G | Autofix ignores explicit markdownlint suppression |
| A-G3 | P | L | n/a | 272 | — | Want underscore-folded anchor suggestions |
| A-G4 | P | L | n/a | decision | — | Want redirect_from URL resolution |
| A-G5 | P | L | n/a | decision | — | Want block-reference and slug validation |
| A-G6 | P | L | n/a | decision | — | Want inline body tags |
| A-W1 | P | L | n/a | decision | — | Want type filter normalization like schema binding |
| A-W2 | P | L | n/a | decision | — | Want inline tags rename or clearer scope |
| A-W3 | P | L | n/a | decision | — | Want todo-only task projection |
| A-W4 | P | L | n/a | 273 | — | Want mv conflict value enum; overlaps A-BUG-24 |

### B observations

| ID | Kind | Age | Introduction | Fix track | Family | Finding |
|---|---|---|---|---|---|---|
| B-BUG-1 | D | R | 272 | 275 | X | Bare aliases falsely resolved under incorrect ecosystem premise |
| B-BUG-2 | D | R | 272 | 275 | S | Alias tie-break conflicts with mv ambiguity |
| B-BUG-3 | D | L | 262 | 275 | S | Root-only frontmatter ambiguity fix misses real layouts |
| B-BUG-4 | D | R | 271 | 276 | G | New disable-next-line suppresses wrong line |
| B-BUG-5 | D | L | unknown | 276 | G | List-indented fenced samples corrupted by autofix |
| B-BUG-6 | D | U | unknown | 276 | C | Shipped bulk-write recipe changes vault without preview |
| B-BUG-7 | D | R | 272 | 275 | G | New underscore suggestions omit exact folded equality |
| B-BUG-8 | D | L | unknown | 275 | S | Moved file self-link body/frontmatter ambiguity differs |
| B-BUG-9 | D | R | 272 | 275 | S | New flow-list extraction never reaches rewrite/report path |
| B-BUG-10 | D | R | 273 | 275 | S | New destination normalization rejects absolute/root forms |
| B-BUG-11 | P | L | n/a | 276 | — | Named missing snapshot falls back under old contract |
| B-BUG-12 | D | L | unknown | 276 | S | Old-semantic snapshots silently served after upgrade |
| B-BUG-13 | D | L | unknown | 277 | X | Indexed prefix resolver still stats each link |
| B-BUG-14 | E | L | unknown | 277 | — | Bulk durability fsync cost exposed at scale |
| B-BUG-15 | D | L | unknown | 277 | C | Hints drop site_prefix and change query answer |
| B-BUG-16 | D | R | 272 | 277 | S | New attachment kinds differ in summary/find graph predicate |
| B-BUG-17 | D | R | 272 | 277 | C | New emitted_target promise misses fuzzy bucket |
| B-BUG-18 | D | L | unknown | 277 | X | Fuzzy confidence permits incorrect above-floor proposals |
| B-BUG-19 | D | L | unknown | 276 | C | Zero max-per-rule contradicts unlimited contract |
| B-BUG-20 | D | L | unknown | 276 | G | Schema typos silently disable constraints |
| B-BUG-21 | E | L | unknown | 276 | — | Documented vault-relative path shadows existing CWD file |
| B-BUG-22 | D | L | unknown | 276 | G | Noncanonical macOS false-case path persists |
| B-BUG-23 | D | L | unknown | 275 | G | Wikilink whitespace not trimmed |
| B-BUG-24 | D | L | 265 | 277 | S | Indexed summary omits skipped counts/directories |
| B-BUG-25 | D | U | unknown | 275 | C | Batch dry-run aborts on collision and omits total |
| B-BUG-26 | D | R | 272 | 275 | C | New alias collision lacks candidates and ambiguity label |
| B-BUG-27 | D | R | 274 | 276 | C | New SCHEMA show hints unsupported mutation |
| B-BUG-28 | D | L | unknown | 276 | C | Empty config falsely said to explicitly set dir |
| B-BUG-29 | D | R | 274 | 276 | C | Exit help not synchronized with DEC-307 |
| B-BUG-30 | D | R | 273 | 276 | C | New stale-probe documentation halves blind window |
| B-BUG-31 | D | L | unknown | 275 | G | Split-link skip target retains whitespace |
| B-BUG-32 | D | R | 272 | 275 | C | New alias text marker omitted by renderer |
| B-BUG-33 | D | L | unknown | 276 | G | Closing-fence whitespace lost on property mutation |
| B-BUG-34 | D | R | 271 | 276 | G | New delimiter wording exceeds opener behavior |
| B-BUG-35 | D | L | unknown | 276 | C | Parse diagnostics leak outside JSON error envelope |
| B-BUG-36 | P | L | n/a | 275 | — | Nested heading-path support undecided |
| B-BUG-37 | D | L | unknown | 275 | G | Dot-relative resolved path is noncanonical |
| B-BUG-38 | D | L | unknown | 276 | G | Tag rename loses flow style unlike set |
| B-BUG-39 | D | U | unknown | 275 | C | Batch ambiguity warning duplicated |
| B-BUG-40 | D | L | unknown | 276 | G | Task grammar misses valid forms and accepts missing space |
| B-BUG-41 | P | L | n/a | 276 | — | Scalar coercion policy undocumented |
| B-BUG-42 | P | L | n/a | 276 | — | Required property implies string under prior schema policy |
| B-BUG-43 | D | R | 271 | 276 | G | New suppression parser ignores unknown rule IDs |
| B-BUG-44 | D | U | unknown | 276 | X | Near-duplicate warning reports unrelated values |
| B-BUG-45 | D | U | unknown | 277 | S | Broken anchors key always zero |
| B-BUG-46 | D | R | 274 | 277 | C | New site diagnostic warnings disagree on denominator |
| B-BUG-47 | D | L | unknown | 277 | C | Advertised site-only hint suppression incomplete |
| B-UX-1 | P | L | n/a | 276 | — | Want no-op versus failure skip reasons |
| B-UX-2 | D | R | 271 | 275 | C | Ambiguity text drops property context present in JSON |
| B-UX-3 | P | L | n/a | 276 | — | Want overwrite refusal rationale |
| B-UX-4 | D | U | unknown | 276 | C | Shipped jq hints recipe always empty |
| B-UX-5 | D | U | unknown | 276 | C | Broken-link recipe omits fragment |
| B-UX-6 | P | L | n/a | 277 | — | Want text projection narrowed to broken links |
| B-UX-7 | P | L | n/a | 276 | — | MD010 sample tabs need opt-out policy |
| B-UX-8 | D | R | 273 | 277 | C | New per-file warning witness differs from directory warning |
| B-UX-9 | P | L | n/a | 277 | — | Derived site prefix misleading without provenance |
| B-UX-10 | P | L | n/a | 277 | — | Want autolink inventory |
| B-UX-11 | P | L | n/a | 277 | — | Bare filename positional intentionally remains search |
| B-UX-12 | P | L | n/a | 277 | — | Absent-key inequality semantics undocumented |
| B-UX-13 | P | L | n/a | 277 | — | Prefer missing-file error before stale-index warning |
| B-UX-14 | P | L | n/a | 276 | — | Want malformed opener-specific lint diagnostic |
| B-G1 | P | L | n/a | decision | — | Want MDN slug encoding map |
| B-G2 | P | L | n/a | 279 | — | Want basename fallback for directory-index corpora |
| B-G3 | P | L | n/a | decision | — | Want ambiguity candidates in find link objects |
| B-G4 | P | L | n/a | 276 | — | Want snapshot version stamp; overlaps B-BUG-12 |
| B-G5 | P | L | n/a | 276 | — | Want null writer/coercion table; overlaps B-BUG-41 |
| B-G6 | P | L | n/a | 277 | — | Want link-kind histogram without full dump |

## Reproducing the ledger counts and verification

The following counts the embedded rows after reading this report through Hyalo:

```sh
target/release/hyalo read research/stability-retrospective-2026-09-06.md --format text |
  awk -F ' *[|] *' '/^[|] [AHOB]-/ && $3 ~ /^[DEP]$/ {
    n++; kinds[$3]++; if ($3=="D") {ages[$4]++; families[$7]++}
  } END {
    print "observations",n
    for(k in kinds) print "kind",k,kinds[k]
    for(k in ages) print "defect-age",k,ages[k]
    for(k in families) print "family",k,families[k]
  }'
```

Changed-file `hyalo lint --strict` and `git diff --check` are the authorized gates for
this documentation-only iteration. No Rust builds/tests or external actions are part of
this report. Research evidence is retained locally under
`.git/ralph-loop/run-20260906-284-287/284/`: source captures, triage.tsv, triage-report.md,
surface-report.md with reproduced commands, classes-report.md, class mappings and
test-blame/history inventories. The substantive ledger, metrics, commands and limitations
are embedded here so the report remains usable without ignored run files.
