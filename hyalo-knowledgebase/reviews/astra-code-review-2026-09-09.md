---
type: review
title: GPT-6 Astra code review - 2026-09-09
date: 2026-09-09
status: active
tags:
  - rust
  - security
  - review
  - astra-review
---

# Preserved GPT-6 Astra code review

This preserves the preceding review at its exact baseline; it is not a new implementation
status report. The original local evidence directory was `/tmp/hyalo-review-20260909`.
Raw fixture outputs are optional supporting artifacts and are not required to execute the
new plans: concrete triggers and acceptance checks are restated in the report and plans.
The baseline no-source-change statement below describes the review turn, before these
planning documents were created. All findings remain open until implemented and verified.

Structural assessment and complete ownership map:
[[research/rust-architecture-review-2026-09-10]].

## Original report

**Reviewed commit:** `857517a2c9be9a1c4a8b1c0b451082c81e6ecb09` (`main`), 9 September 2026.
**Conclusion:** Hyalo has useful functionality, but it is not presently dependable for unattended bulk maintenance. The failures include actual note destruction, reads and writes escaping the intended root, successful mutations reported as failures, scope-changing apply hints, and incorrect search/link results. These are substantive correctness and trust-boundary problems, not cosmetic Rust warnings.

No project source, documentation, commits, or remote state were changed during this review. The repository remained clean at the reviewed commit. Reproductions wrote only to disposable fixtures under this report directory.

### Basis and limits

Three independent **GPT-6 Astra, xhigh** reviews inspected security/I/O, Rust/core semantics, and CLI/documentation/agent contracts. Each ran in a fresh native review process whose startup confirmed `read-only` and `approval=never`. The initially attempted custom reviewer agents stopped because their inherited permissions did not meet the review sandbox requirement; they did not supply the findings. All three replacement review processes completed successfully.

The independent passes produced 44 findings, with one overlap (R05 duplicates S13). The parent review reproduced the relevant behavior for 36 of those 43 distinct findings and traced the remaining seven in source. Reproduction of one variant does not establish every platform, race, or consequence in its review comment. Six additional parent finding groups are recorded below. Every independent finding has a disposition in the ledger; none was silently dropped.

Coverage included the Rust workspace, CLI inputs/output/hints, parser/index/link/mutation paths, README and help, configuration/CI instructions, npm/Pi integration and shipped skills, and selected repository quality gates. The help inventory exercised 53 help pages, including 42 leaf commands. This was a broad whole-project review, not a claim that every source line or every executable path was exhaustively examined.

Runtime checks used the existing current-source **debug** binary on macOS; its path and SHA-256 are in binary.json (`binary.json`, optional local evidence). The earlier baseline in this session passed formatting, Clippy, and 4,951 tests with two ignored; that full suite was not rerun as part of this review. Passing that baseline did not prevent the failures below. No Windows/Linux runtime matrix, destructive resource exhaustion, deterministic race harness, full published-artifact/supply-chain audit, or held-out LLM task benchmark was performed.

### Findings that should block confident automated writes

1. **Two ordinary move cases destroy data and exit successfully (S02, S03).** On the tested case-insensitive macOS filesystem, moving `a/Foo.md` and `b/foo.md` into an empty `archive` overwrites one note. Separately, moving `alias.md -> real.md` onto `real.md` replaces the real note with a self-referencing symlink. Both were reproduced with exit 0. String comparison and canonical-path equality do not establish safe rename identity. Use filesystem-aware collision planning and no-replace execution; reject alias/referent collisions. See [move alias check](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L1195) and [batch collision check](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L559).

2. **Root confinement is inconsistent (S01, S04).** An installation root containing `.claude` symlinked to an external directory causes `init --claude` to create artifacts outside the root; `deinit` deletes those external artifacts even though the reproduced command eventually exits 2. An indexed note replaced with a symlink to an external text file is read by indexed regex search and its contents returned. These need a hostile or surprising local workspace, not a remote service exploit. All external paths in the tests were disposable fixtures. See [artifact installation](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/init.rs#L591) and [indexed live read](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/find/mod.rs#L958).

3. **A failed command can have already changed files, with no usable effect report (C01, S07, S08, A01).** `task toggle ... --count` changes a checkbox and then returns an unsupported-option error. Retrying the failed command undoes the first change. Invalid jq syntax likewise fails after an append. A batch append or toggle can modify the first note and fail on the second; a move can rename its source and then fail to rewrite a backlink. These failures omit completed effects and can bypass index maintenance. Validate deterministic errors and transformations before applying; return committed paths and index disposition for unavoidable partial failures. A mutation journal that maintains an index is not a rollback transaction. See [late output validation](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/output_pipeline.rs#L82).

4. **Preview/apply hints change the selected work (C02–C04).** `lint --files-from changed.txt --fix --dry-run` offers vault-wide `hyalo lint --fix`; following it changes an unselected note. Auto-link hints drop `--first-only` and excluded target globs. Fuzzy repair hints drop `--ignore-target` and rewrite an explicitly excluded target. This is particularly serious when shipped instructions tell agents to follow hints. Build hints from the same complete resolved operation used by execution, and check that preview and apply cover identical targets and rules. See [lint hint context](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/run.rs#L1960) and [link hint construction](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/links.rs#L208).

5. **Auto-fixes modify literal examples and YAML values (R01, S05).** Native task rules bypass shared code/comment spans: `lint --fix` rewrites an indented `[] example` and checkbox examples in HTML comments. A separate delimiter bug treats a YAML key beginning `---` as the end of frontmatter and changes a tab inside a property value. A formatting-preserving tool must share the same source boundaries across parsing and fixing. See [native rule dispatch](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-mdlint/src/engine.rs#L924) and [body splitting](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/lint/fix.rs#L128).

The high-severity evidence (`high-severity-reproductions.json`, optional local evidence), retry evidence (`retry-and-jq.json`, optional local evidence), and detailed ledger retain commands, outputs, and source locations.

### Read correctness also needs repair

The remaining core findings are not merely inconvenient formatting:

- Boolean/phrase search returns wrong sets: `"red blue" OR green` both loses a valid document and admits a partial phrase; `rust -"memory safety"` incorrectly rejects `rust memory allocation` (R02–R03).
- Refreshing an explicitly named indexed note leaves BM25 postings stale. Indexed property writes lose alias-resolved backlinks and leave self-anchor locations stale (R04, R07–R08).
- Backlinks can attribute `[b](b.md)` in `sub/a.md` to root `b.md` despite a valid `sub/b.md` taking resolver precedence (R06).
- Selecting only metadata skips valid frontmatter beyond 16 KiB although full scans accept it up to the shared 64-KiB budget (S13/R05). A successful metadata write can also produce YAML that the normal reader rejects for exceeding its node budget (S09).
- HTML-comment contents become searchable headings/tasks; an inline-code lint directive suppresses a real violation; YAML block-scalar links are omitted; `# C#` becomes title `C`; single-digit numbered type files are skipped by type-scoped lint (R09–R13).
- Unicode section filters panic in **debug builds** (R14). This reproduced debug assertion is not evidence of a release-binary panic.

A healthy small fixture did show equal disk/index results and successful ordinary link-preserving moves. Those counterchecks establish that useful paths work; they do not cancel the specific semantic failures.

### Rust and security assessment

The crate split, typed structures, explicit error handling, formatting-preserving edit machinery, and extensive tests are useful foundations. Counting `clone`, demanding zero `unwrap` everywhere, or citing a clean Clippy run would miss the important problems.

The recurring design weakness is **duplicated policy with caller-managed consistency**: multiple frontmatter boundaries, Markdown classifications, link resolvers, snapshot refresh paths, argument builders, and output paths disagree. Broad shared command context and serialize/parse output plumbing make invariants harder to enforce. Historical comments describe intended guarantees that the execution paths do not always provide.

The highest-value Rust improvements are to make invalid command combinations and input cardinality unrepresentable or reject them at the boundary; represent a fully validated mutation plan; centralize confined atomic replacement; and make derived-state updates complete by construction. These align with the Rust API Guidelines' emphasis on [validation through types and boundary checks](https://rust-lang.github.io/api-guidelines/dependability.html), and on documenting [errors and panics](https://rust-lang.github.io/api-guidelines/documentation.html). They are targeted design changes, not an argument for a wholesale rewrite.

Additional source-supported risks need controlled follow-up: truncating configuration writes under disk failure (S06), concurrent full-map tag rename losing unrelated edits (S12), snapshot expansion allocating roughly 100 GiB from a compact crafted representation (S10), and line allocation before the frontmatter byte cap (S11). The 100-GiB figure is derived from term length times occurrence count; no such allocation was attempted. A separate bounded jq recursion test **did abort with stack overflow**, contradicting help's clean-error resource-safety promise (A04). This is local availability risk; no remote execution was demonstrated.

The refreshed RustSec audit checked 232 locked dependencies and reported **zero vulnerability entries**, plus two unmaintained-package warnings: [bincode 1.3.3](https://rustsec.org/advisories/RUSTSEC-2025-0141.html) and [yaml-rust 0.4.5](https://rustsec.org/advisories/RUSTSEC-2024-0320.html). The npm lockfile audit reported zero vulnerabilities. “Unmaintained” is not interchangeable with “known exploitable vulnerability,” and clean dependency audits do not establish application safety. Raw Rust audit (`cargo-audit.json`, optional local evidence) and npm audit (`npm-audit.json`, optional local evidence) are included.

There is also avoidable O(N×E) graph work during bulk updates and a Windows executable-path defect in the scale gate (R15–R16). Two named xtask checks are explicitly [stubs returning success](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/xtask/src/stubs.rs#L1); their green status cannot count as completed validation. This is a coverage limitation, not a new claim that they hide an identified production failure.

### Command, documentation, and agent fit

**Useful fit:** structured selections, projected results, typed read tools, metadata-aware edits, links, and lint can save agents substantial bespoke scripting. Actual validated editing preserved CRLF, an inline YAML comment, and unrelated bytes; dry-run edits preserved exact bytes. Typed Pi diagnostics also worked where the generic wrapper did not.

**Current limit:** I would use Hyalo for supervised queries and explicit, reviewed edits. I would not treat it as a dependable unattended vault caretaker, blindly execute its apply hints, or blindly retry failed mutations. Non-idempotent operations, hidden successful-command warnings, incomplete scans, and silent partial writes undermine the agent's ability to know what happened. More enthusiastic agent instructions do not repair those semantics.

The integration defects reinforce that assessment: the generic Pi tool discards successful stderr diagnostics and misdetects raw format flags; `hyalo_set` promises an automatic lint guardrail that does not run. With a configured iteration schema, the typed tool marked a note completed without invoking lint, while explicit lint immediately found HYALO002 (C08–C10). The bulk-update skill pipes a JSON array into `xargs` as a filename, and the tidy skill's prescribed setup overwrites existing named views (C11, C14).

Documentation is not coherent end to end. The OKF CI recipe/help promises drift failure while the command exits 0 and requires inspecting result fields. Alias resolution is documented as enabled by default but defaults to false. “All writes support dry-run” overstates coverage. Read help describes body measurements while results measure the whole file. “All commands default to JSON” ignores context and command exceptions. Leading-hyphen filenames produce unusable follow-up hints, empty filename projections return JSON, and single-file commands silently ignore extra selected files (C07, C12, C15, A02–A03, A06).

The long root and find help measured about **36 KiB each**, while short help measured about 2.5 and 3 KiB. The shipped skill directs agents toward long help. This is measurable context overhead, not proof of model task failure. Prefer short discovery plus focused examples; remove duplicated promotional guarantees and keep executable examples tied to actual behavior.

A credible agent evaluation should measure outcomes on unseen fixtures: exact intended files changed, preservation of other bytes, query result correctness, completeness diagnostics, recovery after partial errors, total tool calls/context, and time. Compare against ordinary file tools and scripts on the same tasks. Include ambiguous links, aliases, Unicode, empty inputs, malformed notes, stale indexes, and retries. Existing model-written praise and counts of passing tests are not evidence of superiority. No such comparative evaluation was run here.

### Recommended repair sequence

1. Prevent destructive moves and escaping I/O; reject output/transform errors before writes; provide explicit partial-effect reporting and safe index disposition.
2. Make preview/apply selection identical and unify literal/frontmatter boundaries before allowing automated fixes.
3. Correct query semantics and unify link resolution and derived-state refresh; add disk/index differential checks.
4. Repair Pi wrappers, executable help/skill/CI examples, empty/cardinality contracts, and misleading guarantees.
5. Address resource budgets, concurrent updates, scale behavior, and real cross-platform gates; then evaluate agent task success against a baseline.

Regression tests should assert **effects and invariants**, not just flag acceptance, hint strings, or successful exit. Especially valuable are preview/apply scope equality, failed-preflight byte identity, alias/case collision handling, partial-I/O effect accounting, parser-reader/writer agreement, disk/index equivalence, and execution of published recipes.

### Complete independent-finding ledger

P1 means address before relying on automated writes or advertised confinement. P2 means a material correctness, reliability, resource, integration, or documented-workflow defect. A reproduced entry validates the trigger exercised in its evidence file; variants and downstream implications can remain source-based. All findings remain open in this review. Detailed original comments and suggested repairs follow the ledger.

| ID | Priority | Finding | Disposition / evidence |
|---|---|---|---|
| S01 | P1 | [Confine all init/deinit artifacts before modifying them](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/init.rs#L591) | Reproduced (`high-severity-reproductions.json`, optional local evidence) |
| S02 | P1 | [Reject moves from a symlink onto its referent](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L1195) | Reproduced (`high-severity-reproductions.json`, optional local evidence) |
| S03 | P1 | [Detect filesystem-equivalent batch destinations](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L559) | Reproduced (`high-severity-reproductions.json`, optional local evidence) |
| S04 | P1 | [Recheck confinement before indexed regex reads](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/find/mod.rs#L958) | Reproduced (`high-severity-reproductions.json`, optional local evidence) |
| S05 | P2 | [Recognize complete frontmatter delimiters before body fixes](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/lint/fix.rs#L128) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| S06 | P2 | [Replace configuration files atomically](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/types.rs#L590) | Source-supported; not runtime-reproduced — No disk-full/interruption injection. |
| S07 | P2 | [Preflight batch transformations and account for partial writes](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/append.rs#L357) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| S08 | P2 | [Recover single-file moves when link rewriting fails](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L1328) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| S09 | P2 | [Enforce reader parser budgets on generated frontmatter](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter/parse.rs#L819) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| S10 | P2 | [Bound expanded BM25 token bytes before reconstruction](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L936) | Source-supported; not runtime-reproduced — Allocation amplification derived from code; deliberately no 100-GiB stress run. |
| S11 | P2 | [Limit frontmatter reads before allocating complete lines](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter/parse.rs#L1134) | Source-supported; not runtime-reproduced — Allocation occurs before the cap; no multi-gigabyte fixture. |
| S12 | P2 | [Detect concurrent frontmatter changes during tag renames](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/tags.rs#L353) | Source-supported; not runtime-reproduced — Stale full-map replacement traced; no deterministic concurrent-writer race. |
| S13 | P2 | [Read the complete permitted frontmatter prefix](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/scanner/mod.rs#L183) | Reproduced (`native-reproductions-2.json`, optional local evidence) |
| R01 | P1 | [Exclude literal regions before running native task rules](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-mdlint/src/engine.rs#L924) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R02 | P2 | [Evaluate optional phrases without globally rejecting documents](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L1100) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R03 | P2 | [Preserve phrase boundaries when evaluating negation](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L1031) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R04 | P2 | [Invalidate BM25 postings after refreshing a named file](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L2099) | Reproduced (`native-reproductions-2.json`, optional local evidence) |
| R05 | P2 | [Read frontmatter through the shared parser budget](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/scanner/mod.rs#L183) | Duplicate of S13 |
| R06 | P2 | [Build graph keys using the shared target-resolution semantics](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/link_graph.rs#L685) | Reproduced (`native-reproductions-2.json`, optional local evidence) |
| R07 | P2 | [Include aliases when rebuilding the snapshot resolver](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L969) | Reproduced (`native-reproductions-2.json`, optional local evidence) |
| R08 | P2 | [Refresh self-anchor metadata with other scanned fields](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L1017) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| R09 | P2 | [Classify heading and task syntax after comment suppression](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L2608) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R10 | P2 | [Ignore directive-shaped text inside inline code](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-mdlint/src/rules/spans.rs#L393) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| R11 | P2 | [Preserve block-scalar content during frontmatter link scanning](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter_links.rs#L178) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| R12 | P2 | [Include single-digit sequence numbers in template globs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/filename_template.rs#L136) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| R13 | P2 | [Strip only valid ATX closing hash sequences](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/heading.rs#L34) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R14 | P2 | [Remove the invalid ASCII precondition on section filters](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/heading.rs#L168) | Reproduced (`rust-reproductions-1.json`, optional local evidence) |
| R15 | P2 | [Avoid full-graph traversals for each journaled file](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/link_graph.rs#L457) | Source-supported; not runtime-reproduced — O(N×E) derived from callers and graph traversals; no scale benchmark. |
| R16 | P2 | [Locate the platform-specific release executable](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/xtask/src/bench_scale.rs#L194) | Source-supported; not runtime-reproduced — Platform path defect traced; no Windows runtime. |
| C01 | P1 | [Reject unsupported --count before executing mutations](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/output_pipeline.rs#L82) | Reproduced (`retry-and-jq.json`, optional local evidence) |
| C02 | P1 | [Preserve resolved lint selection in apply hints](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/run.rs#L1960) | Reproduced (`high-severity-reproductions.json`, optional local evidence) |
| C03 | P1 | [Carry auto-link exclusions into the apply command](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/links.rs#L208) | Reproduced (`native-reproductions-2.json`, optional local evidence) |
| C04 | P1 | [Preserve repair exclusions in links-fix hints](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/links.rs#L36) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| C05 | P2 | [Route drop-index's global index path to the deletion target](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/dispatch.rs#L717) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| C06 | P2 | [Preserve body-search constraints in find follow-up hints](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/command.rs#L334) | Reproduced (`showall.json`, optional local evidence) |
| C07 | P2 | [Preserve filename projections for empty files-from input](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/run.rs#L321) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| C08 | P2 | [Surface successful-command diagnostics in the generic Pi tool](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L62) | Reproduced (`pi-diagnostics.json`, optional local evidence) |
| C09 | P2 | [Recognize raw argv output flags before injecting defaults](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L126) | Reproduced (`pi-diagnostics.json`, optional local evidence) |
| C10 | P2 | [Apply the promised lint guardrail to hyalo_set](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L361) | Reproduced (`pi-guard-configured.json`, optional local evidence) |
| C11 | P2 | [Emit individual paths in the bulk-update recipe](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/skills/hyalo/SKILL.md#L281) | Reproduced (`docs-recipes.json`, optional local evidence) |
| C12 | P2 | [Make the documented OKF CI gate inspect drift explicitly](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/docs/ci.md#L52) | Reproduced (`native-reproductions-3.json`, optional local evidence) |
| C13 | P2 | [Skip diagnostic body probes when hints are disabled](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/find/run.rs#L306) | Source-supported; not runtime-reproduced — Unconditional discarded-hint probe traced; no syscall/latency benchmark. |
| C14 | P2 | [Preserve existing named views during the Pi tidy workflow](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/skills/hyalo-tidy/SKILL.md#L49) | Reproduced (`native-reproductions-4.json`, optional local evidence) |
| C15 | P2 | [Document alias resolution as opt-in](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/docs/configuration.md#L16) | Reproduced (`docs-recipes.json`, optional local evidence) |

### Additional parent findings

These supplement the independent ledger; overlapping parent observations were folded into C01, C06, C08, and C09.

| ID | Priority | Finding and effect | Evidence / source |
|---|---|---|---|
| A01 | P1 | Batch task toggle changes the first file before a later invalid task line fails. Output omits the first effect; retry reverses it. Prevalidate and report committed effects. | Reproduction (`task-batch.json`, optional local evidence); [tasks.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/tasks.rs#L754) |
| A02 | P2 | Single-file read/backlinks/task-read silently accept repeated files and use only the first, even if a later path does not exist. Empty read input has contradictory exit messaging; a missing read input with explicit JSON returns plain text. Validate cardinality and normalize errors. | Inputs (`contracts.json`, optional local evidence), additional cases (`single-input.json`, optional local evidence); [inputs.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/inputs.rs#L188) |
| A03 | P2 | `find --file=-odd.md` works, but all three generated read/find/backlinks hints use `--file -odd.md` and fail argument parsing. Shell quoting alone does not protect option parsing; use `--file=value`. | Reproduction (`contracts.json`, optional local evidence); [hint serializer](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/command.rs#L465) |
| A04 | P2 | `summary --jq 'def f: [f]; f'` aborts with stack overflow. Timeout threads do not bound recursion or allocations; help's never-hang/OOM clean-error promise is unsupported. Correct the claim and use enforceable resource isolation if accepting hostile expressions. | Reproduction (`retry-and-jq.json`, optional local evidence); [jq limits](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/output/jq.rs#L28), [help](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/cli/args.rs#L383) |
| A05 | P2 | Explicitly selecting a file and its in-vault symlink alias toggles the same physical task twice, exits 0, and leaves its original state. Ordinary scans skip the alias; this requires explicit targets. Deduplicate identities before non-idempotent mutation. | Reproduction (`aliases.json`, optional local evidence); input resolution and task dispatch |
| A06 | P2 | README/help overclaim dry-run coverage and universal link preservation; read size/line descriptions and default JSON claims omit real behavior/exceptions. Narrow guarantees and execute representative documentation examples. | Help catalog (`help-catalog.json`, optional local evidence), CLI matrix (`matrix.json`, optional local evidence); [README](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/README.md#L35) |

#### Verification qualifications and counterchecks

- The configured Pi guard reproduction is `pi-guard-configured.json`. The earlier `pi-guard.json` omitted the schema and only established that no lint call occurred; it did not establish a missed HYALO002 violation.
- The valid links-fix exclusion reproduction is in `native-reproductions-3.json`. An earlier ordinary-relocation fixture in `native-reproductions-2.json` was not proof of that exclusion defect; the corrected fixture uses broken fuzzy candidates, where `--ignore-target` applies.
- Ranked and regex queries both found fenced-code and HTML-comment content in the full-text counterchecks. A suspected general “full-text excludes code” defect was rejected; this is distinct from incorrectly classifying hidden structural headings/tasks.
- Healthy workflow evidence (`healthy.json`, optional local evidence) covers byte preservation, dry run, a straightforward move and link check, and small-fixture disk/index agreement.
- The native reviewer reports describe static inspection only. Runtime verification and the qualifications above were added by the parent review; do not attribute those executions to the independent reviewers.

### Original independent comments

The following retain all review comments for implementation detail. Source ranges identify the relevant code at the reviewed commit, not a proposed patch.

#### S: Security review

##### S01 [P1] Confine all init/deinit artifacts before modifying them

[init.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/init.rs#L591) — **Reproduced**

If `.claude` or `.pi` is a directory symlink outside the installation root, `init --claude`/`--pi` follows it and overwrites external files; `deinit` similarly deletes external files through `remove_artifact`. The Codex preflight does not validate these paths. Preflight every integration artifact and its parent components before any mutation, rejecting escaping symlinks for both initialization and removal.

##### S02 [P1] Reject moves from a symlink onto its referent

[mv.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L1195) — **Reproduced**

With `alias.md -> real.md` inside the vault, moving `alias.md` to `real.md` passes this canonical-path equality check. However, `execute_mv` renames the symlink itself: on POSIX this replaces the real note with a self-referencing symlink, destroying its contents. Distinguish a case-only rename of one directory entry from two separate entries resolving to the same target, and reject the latter; the batch collision check needs the same distinction.

##### S03 [P1] Detect filesystem-equivalent batch destinations

[mv.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L559) — **Reproduced**

On a case-insensitive macOS volume, `a/Foo.md` and `b/foo.md` can coexist, but moving both into an initially empty `archive` directory produces one filesystem destination. This string-keyed collision check treats them as distinct, both existence checks pass before execution, and the second `fs::rename` overwrites the first note. Check destination equivalence under the target filesystem and use no-replace execution to prevent clobbering. This affects the required platform support in [CLAUDE.md:50](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L50).

##### S04 [P1] Recheck confinement before indexed regex reads

[mod.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/find/mod.rs#L958) — **Reproduced**

After an index records `note.md`, replacing that note with a symlink to an external readable text file makes `find --index -e '.'` return the external file's matching contents. Snapshot loading validates paths lexically, staleness only produces a warning, and `scan_file_multi` follows this path without a vault-boundary check. Resolve and validate every snapshot-derived live-read target against the canonical vault immediately before reading, as the BM25 document resolver already does.

##### S05 [P2] Recognize complete frontmatter delimiters before body fixes

[fix.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/lint/fix.rs#L128) — **Reproduced**

A valid frontmatter block containing `title: x` followed by a key such as `---note: "a\tb"` (with a literal tab) is split inside that key because the search accepts any line beginning with three dashes. `lint --fix --rule MD010` then treats the remaining YAML as body text and replaces the tab inside the property value. Use the shared complete-line delimiter policy instead of substring matching; it should also handle empty blocks and supported trailing whitespace consistently.

##### S06 [P2] Replace configuration files atomically

[types.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/types.rs#L590) — **Source-supported; not runtime-reproduced**

When updating an existing `.hyalo.toml`, `fs::write` truncates the original before writing the replacement. Disk exhaustion or interruption during a `types` mutation can therefore destroy unrelated configuration and schema data. The `views`, `lint_rules`, and init configuration writers use the same unsafe pattern. Route these replacements through a shared, confined same-directory temporary-file writer that preserves the original until the replacement is complete.

Qualification: No disk-full/interruption injection.

##### S07 [P2] Preflight batch transformations and account for partial writes

[append.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/append.rs#L357) — **Reproduced**

Appending `x=one` to `a.md` containing `x: []` and then `b.md` containing `x: {k: v}` writes the first file before returning this error on the second. Neither prepass rejects the incompatible mapping, and the early return bypasses `journal.flush()`, leaving an active on-disk index stale and omitting the successful mutation from the response. Precompute transformations and output-budget checks before writing; on unavoidable later failures, report committed paths and persist or invalidate their index state.

##### S08 [P2] Recover single-file moves when link rewriting fails

[mv.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/mv.rs#L1328) — **Reproduced**

If a backlink file is readable but its parent directory is not writable, planning succeeds and the source note is renamed before creating the backlink's temporary file fails. The error propagates without restoring the rename or accounting for successful rewrites, and the caller skips index maintenance. The failed command therefore leaves moved files, partially updated links, and a stale snapshot. Apply rollback and partial-result accounting to single-file moves rather than returning directly after the rename.

##### S09 [P2] Enforce reader parser budgets on generated frontmatter

[parse.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter/parse.rs#L819) — **Reproduced**

A valid flow-style `items` list containing 4,997 scalar items consumes the reader's 5,000-node budget including its map, key, and sequence. Adding `title=x` preserves that compact list and writes 5,002 nodes: splice verification permits 100,000 nodes, while this final check enforces only bytes and lines. The successful mutation makes the note unreadable by subsequent Hyalo operations. Validate the final YAML using the normal reader budgets before replacing the file.

##### S10 [P2] Bound expanded BM25 token bytes before reconstruction

[bm25.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L936) — **Source-supported; not runtime-reproduced**

A crafted snapshot can contain one 1-MiB term with 100,000 positions for a valid document. It passes both the snapshot-size limit and `validate_bm25`'s posting-count/document-ID checks, but this reconstruction allocates roughly 100 GiB by cloning the term per occurrence. `MutationJournal::flush` invokes reconstruction after note writes, so an ordinary indexed mutation can exhaust memory after changing data. Validate a checked aggregate expanded-byte budget before accepting or reconstructing the BM25 section.

Qualification: Allocation amplification derived from code; deliberately no 100-GiB stress run.

##### S11 [P2] Limit frontmatter reads before allocating complete lines

[parse.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter/parse.rs#L1134) — **Source-supported; not runtime-reproduced**

A note with a multi-gigabyte first line, or such a line after its opening delimiter, can exhaust memory during `read_frontmatter`: `BufRead::lines()` allocates the entire line before the 64-KiB check runs. Commands such as `set` call this reader before the rewrite helper's file-size guard, so that guard does not prevent the allocation. Use bounded incremental reads for the opener and remaining frontmatter budget, consistent with [CLAUDE.md:48](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L48).

Qualification: Allocation occurs before the cap; no multi-gigabyte fixture.

##### S12 [P2] Detect concurrent frontmatter changes during tag renames

[tags.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/tags.rs#L353) — **Source-supported; not runtime-reproduced**

If an editor or another Hyalo process changes frontmatter after `tags_rename` reads it, this write supplies the old complete property map and can delete newly added properties or revert unrelated values. Atomic replacement does not detect stale inputs, and this command lacks the fingerprint checks used by `set`, `append`, and `remove`. Capture the fingerprint before reading and verify it before replacement; also cover `properties_rename`'s full-map fallback.

Qualification: Stale full-map replacement traced; no deterministic concurrent-writer race.

##### S13 [P2] Read the complete permitted frontmatter prefix

[mod.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/scanner/mod.rs#L183) — **Reproduced**

For a valid frontmatter block between 16 KiB and the supported 64-KiB limit, a frontmatter-only scan reads just this first chunk and passes it to `scan_slice_multi` as though it were the whole file. The missing closing delimiter is then reported as malformed frontmatter. Consequently, a disk query such as `find --fields file,properties` omits notes that full-body scans and `read_frontmatter` accept. Continue bounded reading until the closing delimiter or the shared frontmatter budget is reached.

#### R: Rust review

##### R01 [P1] Exclude literal regions before running native task rules

[engine.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-mdlint/src/engine.rs#L924) — **Reproduced**

With `lint --fix`, an indented code line such as `    [] example` is rewritten into task syntax; checkbox examples inside multiline HTML comments are also affected. Unlike the stock-rule path, these native-rule branches bypass `BodySpans`, while HYALO001/HYALO002 only implement their own simple fence exclusion. Pass shared code/comment classification into both rules before collecting violations, so fixes cannot alter literal examples and HYALO002 cannot count them as unfinished tasks.

##### R02 [P2] Evaluate optional phrases without globally rejecting documents

[bm25.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L1100) — **Reproduced**

For `"red blue" OR green`, a document containing `green red x blue` is incorrectly rejected despite satisfying `green`. Conversely, a document containing only `red` receives a positive score but never enters `all_term_docs`, so it incorrectly survives the pure-OR filter. Track satisfaction and score contributions per clause: a failed optional phrase should contribute nothing, not veto another matching clause or admit partial matches.

##### R03 [P2] Preserve phrase boundaries when evaluating negation

[bm25.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/bm25.rs#L1031) — **Reproduced**

The parser accepts negated phrases, but flattening every MustNot clause into this postings union changes their meaning. For example, `rust -"memory safety"` excludes a document containing `rust memory allocation`, although the excluded phrase never occurs. Evaluate each multi-term exclusion using positional phrase adjacency, retaining the existing postings exclusion for single terms.

##### R04 [P2] Invalidate BM25 postings after refreshing a named file

[index.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L2099) — **Reproduced**

After creating a BM25 snapshot and appending a new term to a note, an indexed search explicitly naming that file refreshes its entry here but still searches the old inverted index. `refresh_entry_and_links_at` replaces entry tokens without updating postings, and the find fast path checks tokenizer version and language rather than freshness. Consequently, newly added terms remain missing and removed terms can still match without a stale-index warning. Rebuild or invalidate the inverted index whenever this refresh changes an entry.

##### R05 [P2] Read frontmatter through the shared parser budget

[mod.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/scanner/mod.rs#L183) — **Duplicate of S13**

A valid frontmatter block with a closing delimiter beyond 16 KiB, such as a 20 KiB description scalar, is truncated by this metadata-only path and reported as unclosed. The shared parser permits 64 KiB, so full scans accept the same file while metadata-only properties/tags scans skip it. Stream through the closing delimiter while enforcing the shared byte and line budgets instead of imposing this smaller fixed prefix.

##### R06 [P2] Build graph keys using the shared target-resolution semantics

[link_graph.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/link_graph.rs#L685) — **Reproduced**

When `sub/a.md` contains `[b](b.md)` and both `b.md` and `sub/b.md` exist, the graph stores the edge under root `b.md`, while `discovery::normalize_link_target` correctly prefers `sub/b.md`. Backlinks therefore attributes the edge to the wrong note. This separate normalization also misses resolver-supported padded wikilinks such as `[[ b ]]`. Resolve graph storage keys through the shared read-side normalization and precedence rules while retaining authored text separately.

##### R07 [P2] Include aliases when rebuilding the snapshot resolver

[index.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L969) — **Reproduced**

With `[links] aliases = true`, an initial snapshot correctly resolves `[[Nickname]]` to a note declaring that alias. A subsequent indexed property mutation on the linking file removes its old edges and reinserts them using this paths-only resolver, losing the alias-resolved backlink even though neither the link nor alias changed. Populate aliases from entry properties as the full-scan builder does, and invalidate the resolver when alias declarations change.

##### R08 [P2] Refresh self-anchor metadata with other scanned fields

[index.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L1017) — **Reproduced**

This selective replacement omits `scanned.self_anchors`. An indexed property mutation that inserts a frontmatter line consequently updates ordinary links and the file fingerprint but leaves `[jump](#Missing)` at its old source line; subsequent broken-anchor results report stale locations, and freshness checks no longer trigger a rescan. Copy `self_anchors` alongside `links`, or centralize replacement of all scan-derived fields.

##### R09 [P2] Classify heading and task syntax after comment suppression

[index.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/index.rs#L2608) — **Reproduced**

For a multiline HTML comment containing `# Hidden` and `- [ ] example`, the scanner supplies empty cleaned content but still invokes visitors with the original line. This collector parses that raw heading and task, and `TaskExtractor` does the same, producing invisible sections, promoted titles, and searchable tasks. Determine whether structural syntax is visible from comment-suppressed content, using raw text only to preserve the content of an actual visible heading or task.

##### R10 [P2] Ignore directive-shaped text inside inline code

[spans.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-mdlint/src/rules/spans.rs#L393) — **Reproduced**

Prose documenting `<!-- markdownlint-disable MD019 -->` inside backticks is treated as a live suppression directive by this raw substring scan. A genuine MD019 violation later in the document is then silently hidden, even though Markdown contains no actual HTML comment at the documented example. Recognize directives only in syntactic HTML-comment spans, excluding inline code as well as fenced and indented blocks.

##### R11 [P2] Preserve block-scalar content during frontmatter link scanning

[frontmatter_links.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/frontmatter_links.rs#L178) — **Reproduced**

For YAML `description: |` followed by an indented `# [[Target]]`, the hash is literal scalar content, not a YAML comment. Calling `strip_comment` without tracking scalar context discards the link, so backlinks and frontmatter broken-link checks omit an authored edge that the YAML parser accepts. Track literal/folded block-scalar boundaries and indentation, or obtain source spans from a YAML-aware representation before applying comment stripping.

##### R12 [P2] Include single-digit sequence numbers in template globs

[filename_template.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/filename_template.rs#L136) — **Reproduced**

In glob syntax, `[0-9][0-9]*` requires two digits; it does not mean one-or-more digits. Consequently, `decisions/{n}-{slug}.md` accepts `decisions/1-test.md` through `FilenameTemplate::matches`, but `lint --type` excludes it because that caller selects files using `to_glob`. Generate a true superset such as `[0-9]*` and use the exact template matcher where numeric or width restrictions must be enforced.

##### R13 [P2] Strip only valid ATX closing hash sequences

[heading.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/heading.rs#L34) — **Reproduced**

A valid heading `# C#` is parsed as `C` because every trailing hash is removed, regardless of the required preceding whitespace. Conversely, trailing whitespace after a legitimate closing sequence prevents its removal. This shared parser feeds indexed titles, outlines, and section matching, so the discrepancy affects more than presentation. Apply ATX closing-sequence rules after handling trailing whitespace, preserving literal or escaped hashes.

##### R14 [P2] Remove the invalid ASCII precondition on section filters

[heading.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/heading.rs#L168) — **Reproduced**

In debug builds, a valid section filter such as `Résumé` panics when a heading is evaluated. `SectionFilter::parse` accepts Unicode and `to_ascii_lowercase` preserves its non-ASCII bytes, so this asserted precondition is not established by the caller. Support those accepted strings without panicking; the existing byte comparison can preserve non-ASCII bytes while folding ASCII, or the implementation can consistently use Unicode case handling.

##### R15 [P2] Avoid full-graph traversals for each journaled file

[link_graph.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-core/src/link_graph.rs#L457) — **Source-supported; not runtime-reproduced**

A bulk indexed property mutation calls this full edge traversal for every changed file, even when its links are unchanged; `insert_links` additionally traverses the complete target-key set. Updating N files in a graph with E edges therefore costs O(N×E), becoming quadratic for ordinary similarly connected vaults despite batching snapshot persistence. Maintain source-to-target adjacency for targeted removal and track inserted keys directly, or rebuild the graph once per mutation batch.

Qualification: O(N×E) derived from callers and graph traversals; no scale benchmark.

##### R16 [P2] Locate the platform-specific release executable

[bench_scale.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/xtask/src/bench_scale.rs#L194) — **Source-supported; not runtime-reproduced**

On Windows, Cargo produces `target/release/hyalo.exe`, but the scale gate checks only this extensionless path both before and after building. It therefore fails with a missing-binary error even after a successful build. Use the platform executable suffix or Cargo's reported artifact path, consistent with the repository's [cross-platform requirement](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L50).

Qualification: Platform path defect traced; no Windows runtime.

#### C: Contract review

##### C01 [P1] Reject unsupported --count before executing mutations

[output_pipeline.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/output_pipeline.rs#L82) — **Reproduced**

For an otherwise valid `hyalo set note.md --property status=done --count`, dispatch writes the property before this branch reports that `--count` is unsupported and exits 1. The mutation result is discarded, so an agent receives a flag error despite a successful write; retrying non-idempotent mutations is particularly problematic. Validate count support for every command before dispatch, rather than discovering it from the completed outcome.

##### C02 [P1] Preserve resolved lint selection in apply hints

[run.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/run.rs#L1960) — **Reproduced**

When `hyalo lint --files-from changed.txt --fix --dry-run` finds fixes, the hint context contains no file targets: it is captured before `--files-from` resolution and never updated afterward. Its “Apply auto-fixes” command therefore becomes vault-wide `hyalo lint --fix`. `--type` and `--profile` are also omitted. Preserve the resolved targets and effective rule selection before offering an apply command, especially because agents are instructed to follow these hints ([CLAUDE.md:30](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L30)).

##### C03 [P1] Carry auto-link exclusions into the apply command

[links.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/links.rs#L208) — **Reproduced**

With default configuration, previewing `links auto --file note.md --first-only --exclude-target-glob 'templates/*'` offers an apply hint that retains the file but drops both restrictions. Following it links repeated mentions and eligible template targets that were absent from the preview. Preserve all effective auto-link flags, including counter-flags, when constructing the apply command; this is part of the prescribed hint-following workflow ([CLAUDE.md:30](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L30)).

##### C04 [P1] Preserve repair exclusions in links-fix hints

[links.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/links.rs#L36) — **Reproduced**

If a preview uses `links fix --ignore-target draft` and finds another applicable repair, this hint drops the exclusion and subsequently repairs matching draft targets too. Dropping `--case-insensitive` likewise reintroduces casing rewrites that the preview deliberately treated as resolved. Capture and replay the effective repair options in both ordinary and fuzzy apply hints, rather than preserving only the glob and globals ([CLAUDE.md:30](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L30)).

##### C05 [P2] Route drop-index's global index path to the deletion target

[dispatch.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/dispatch.rs#L717) — **Reproduced**

When both `custom.idx` and the default snapshot exist, `hyalo drop-index --index-file custom.idx` loads the custom snapshot but dispatches deletion with `path = None`, deleting `.hyalo-index` instead. The documented global alias is merged into the destination for `create-index`, but not for `drop-index`. Resolve the global flag into the deletion path before dispatch, reject conflicting destinations, and avoid loading a snapshot merely to delete it.

##### C06 [P2] Preserve body-search constraints in find follow-up hints

[command.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/hints/command.rs#L334) — **Reproduced**

When `hyalo find 'needle'` exceeds the default limit, “Show all N results” uses this builder, which drops the body pattern and emits `hyalo find --limit 0`. The follow-up returns the entire vault rather than the remaining matches, potentially flooding agent context. Regex and section constraints are similarly absent. Replay the complete resolved query for scope-preserving hints, consistent with the documented hint-following workflow ([CLAUDE.md:30](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/CLAUDE.md#L30)).

##### C07 [P2] Preserve filename projections for empty files-from input

[run.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/run.rs#L321) — **Reproduced**

An empty `--files-from` source short-circuits dispatch through this branch, so `find --files-from - --filenames0` emits a normal result envelope instead of zero bytes. The same problem affects `--filenames-only`; their projections run only inside normal find dispatch. Consequently, downstream filename consumers receive diagnostic/result text as a filename on the no-work path. Apply the requested projection to the empty outcome too.

##### C08 [P2] Surface successful-command diagnostics in the generic Pi tool

[hyalo.ts](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L62) — **Reproduced**

A generic Pi query against a stale snapshot can exit 0 while the CLI warns on stderr that its results may be stale. This success branch discards stderr, hiding that warning—and malformed-config fallback warnings—from the agent. The typed wrapper already preserves successful diagnostics. Include stderr in generic successful results as well, without mixing it into stdout's structured payload.

##### C09 [P2] Recognize raw argv output flags before injecting defaults

[hyalo.ts](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L126) — **Reproduced**

Calling the generic tool with `args: ['--format=json']` injects another `--format text`, producing a clap duplicate-argument error. Supplying `--jq` through `args` also receives the incompatible text default, while `-f` incorrectly suppresses it even though that flag means file, not format. Recognize both value-taking flag spellings before `--`, inspect raw jq arguments, and remove the nonexistent format short alias.

##### C10 [P2] Apply the promised lint guardrail to hyalo_set

[hyalo.ts](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/extensions/hyalo.ts#L361) — **Reproduced**

The `hyalo_set` description promises automatic post-write linting, but execution only calls npm's `set`, and the `tool_result` hook explicitly accepts only Pi's `write` and `edit` tools. Setting an iteration to `completed` while it still has open tasks therefore never runs the promised HYALO002 check, even with the relevant schema configured. Run the guardrail for this mutation path or explicitly remove the guarantee and require a separate lint step.

##### C11 [P2] Emit individual paths in the bulk-update recipe

[SKILL.md](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/skills/hyalo/SKILL.md#L281) — **Reproduced**

This filter produces a JSON array, which Hyalo serializes with brackets and quotes rather than as individual raw paths. `xargs -I` consequently passes the array representation to `hyalo set` as a filename; even an empty result produces a spurious `[]` target. Replace the recipe with native bulk filtering or emit individual path strings into `--files-from`, and update the identical installed Pi skill template.

##### C12 [P2] Make the documented OKF CI gate inspect drift explicitly

[ci.md](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/docs/ci.md#L52) — **Reproduced**

With stale reserved indexes, this workflow step now succeeds: `commands/okf.rs::run_index` deliberately returns exit 0 for dry runs and reports drift through `results.changed` and `results.skipped_markers`. The advertised gate therefore silently stops enforcing freshness. Make the CI recipe assert those fields and update `OkfAction::Index` help, which also still promises a nonzero drift exit.

##### C13 [P2] Skip diagnostic body probes when hints are disabled

[run.rs](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/crates/hyalo-cli/src/commands/find/run.rs#L306) — **Source-supported; not runtime-reproduced**

An empty property-regex query still calls `zero_result_body_search` under `--no-hints`, `--jq`, and the npm typed find API. That helper opens up to 512 files and scans up to its nominal 8 MiB budget solely to construct a suggestion that these callers discard. This adds unnecessary body I/O to indexed metadata queries. Thread effective hint enablement into dispatch and skip both suggestion passes when no hint consumer exists.

Qualification: Unconditional discarded-hint probe traced; no syscall/latency benchmark.

##### C14 [P2] Preserve existing named views during the Pi tidy workflow

[SKILL.md](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/pi-package/skills/hyalo-tidy/SKILL.md#L49) — **Reproduced**

If a vault already defines `orphans`, `stale-in-progress`, or another listed view with user-specific filters, Phase 1 replaces its definition unconditionally. `set_view` inserts over the existing TOML entry rather than merging or refusing, so a hygiene pass permanently destroys saved query configuration before diagnosis begins. Use inline diagnostic queries, or inspect existing views and require explicit authorization before replacing them; update the matching installed template too.

##### C15 [P2] Document alias resolution as opt-in

[configuration.md](https://github.com/ractive/hyalo/blob/857517a2c9be9a1c4a8b1c0b451082c81e6ecb09/docs/configuration.md#L16) — **Reproduced**

When `[links].aliases` is omitted, config loading defaults it to false, contrary to this reference. The later claim that `links fix` never rewrites alias targets is therefore also false for the default configuration: bare aliases become repair candidates. An agent following this reference can expect alias links to remain untouched and then rewrite them during repair. Document the false default and scope the resolution/no-rewrite guarantees explicitly to `aliases = true`.
