---
title: Codebase review — 3 October 2026
type: review
date: 2026-10-03
status: active
tags:
  - review
  - architecture
  - javascript
  - documentation
related:
  - "[[reviews/codebase-review-2026-09-12]]"
  - "[[iterations/iteration-301-hidden-links-and-anchor-repairs]]"
  - "[[iterations/iteration-300-jev-tidy-integration]]"
  - "[[iterations/iteration-287-typed-typescript-api]]"
---

# Codebase review — 3 October 2026

Reviewed `9d105bc1` (main, v0.24.1 in tree) on macOS with a fresh release
build. Six parallel read-only review passes: the npm typed API and Pi runtime,
the Jev helper and tidy skill, the hyalo-core link subsystem (iterations
297–301 diff), the remaining hyalo-core and hyalo-cli code, docs/help
coherence against the binary, and a behavioural dogfood run against the
repository vault plus a synthetic vault. Iterations 296–301 were authored by
a different agent (Codex); this review concentrates on that work and on the
JavaScript deliverables added since [[reviews/codebase-review-2026-09-12]].

Headline: the Codex-authored link work is sound (no correctness bug found in
the iteration 297/301 diff, one performance should-fix), the JavaScript
deliverables are well gated and byte-identical across their copies, and the
test suite passes. The problems are elsewhere: a toolchain break that fails
CI for every new PR, two reproducible CLI bugs (a panic and a link-resolution
disagreement between commands), one JS API misclassification, and a long tail
of documentation that states things the binary does not do.

## P1 — fix before the next PR or release

### F1 — Rust 1.99 fails `clippy -D warnings` on main

Rust 1.99 (stable 2026-09-28, after the last green CI run on 2026-09-23)
adds a lint that fires 84 times in test code: `assert!(x.is_empty())` (76)
and `assert!(!x.is_empty())` (8) across hyalo-core (37), hyalo-cli (33) and
hyalo-mdlint (14), plus two warnings in the hyalo-cli bin target. CI installs
latest stable, so the clippy job fails for every new PR. Tests pass. Being
fixed on `chore/deps-and-rust-1.99` together with a dependency refresh.

### F2 — batch `mv` panics on a filename containing a 4-byte emoji

```text
$ hyalo mv --glob 'notes/*.md' --to archive/ --format json
thread 'main' panicked at core/src/str/mod.rs:862:21:
end byte index 15 is not a char boundary; it is inside '🚀' (bytes 12..16)
[exit=134]
```

Reproduced with `notes/Emoji 🚀 Note.md` in a vault with `dir = "."`.
Single-file `mv` of the same file works; umlaut and CJK names in batch mode
work. The stem's byte length is sliced off the full path. Violates DEC-307
(exit codes 0/1/2 only). Location: `crates/hyalo-cli/src/commands/mv.rs`
batch planning.

### F3 — path-form wikilink with a dotted stem is unresolved, and commands disagree

`[[sub/rel-1.2]]` reports `path: null` although `sub/rel-1.2.md` exists;
bare `[[rel-1.2]]` resolves. `.2` is taken as a non-`.md` extension once a
directory segment is present. Consequences on the same vault:
`find --broken-links` and HYALO006 say broken, `summary.links.broken` says 0,
`links fix` says 0, `backlinks sub/rel-1.2.md` sees nothing. Contradicts
DEC-318 (one shared edge predicate). Real occurrence:
`research/dogfooding-v0.4.1-consolidated.md` line 16, four links to existing
files. Location: `crates/hyalo-core/src/link_resolve.rs` extension check.

### F4 — JS API renders a lint *refusal* as "found issues"

`npm/hyalo/src/api.ts:518-522` and `pi-package/extensions/hyalo.ts:239-245`.
A lint refusal (exit 1, empty stdout, error envelope on stderr; e.g.
`lint -- missing.md`, or any lint under a malformed `.hyalo.toml`, DEC-290)
is returned as a successful lint with an empty finding list, stderr dropped.
Verified against the binary: findings always reach stdout, so "exit 1 and
empty stdout" is a reliable refusal marker. Fix: `lint()` throws
`HyaloError` with the parsed envelope in that case; `lintVaultFile` returns
`unavailable` with the stderr text. Add tests for both.

### F5 — a gitignored named file vanishes once backlinks are requested

`MyNotes.md` is gitignored. `find --file MyNotes.md` returns it;
`find --file MyNotes.md --fields backlinks` (the hint find itself prints)
returns "No results", exit 0. Contradicts DEC-301 (a named path is a
promise). Gitignore honouring is not mentioned in any help page and is not
counted under `summary.skipped`/`excluded`.

## P2 — material correctness or supported-workflow problems

### Rust

- **anchor matching recomputes heading slugs per link** —
  `crates/hyalo-core/src/anchor.rs:384-405` rebuilds `heading_slugs(sections)`
  (a HashMap plus a `Vec<String>`) on every `fragment_matches_headings_inner`
  call. `find --broken-links` and `links fix` call it once per
  fragment-bearing link, so a reference page with N headings and M inbound
  anchors costs O(M×N) allocation. `AnchorSectionsCache` in `find/mod.rs`
  memoizes the section read but not the slugs. Memoize the slug vector per
  target path.
- **`extract_fence_language` is `pub` and panics on a mismatched
  precondition** — `crates/hyalo-core/src/scanner/fence.rs:74-78` slices
  `trimmed[fence_count * fence_char.len_utf8()..]` unchecked; every in-crate
  caller derives the arguments from the same line, but hyalo-mdlint consumes
  the function. Use `.get()` or narrow to `pub(crate)`.
- **`enum` constraint without `values` is silently unsatisfiable** —
  `crates/hyalo-core/src/schema.rs:764-778` defaults to an empty list, so
  every value fails lint instead of the config being refused at load like
  other shape errors. Reject missing or empty `values` in
  `TryFrom<RawPropertyConstraint>`.
- **`summary --index --format text` prints a raw key dump** (all tags,
  `dead_ends:`, `dir:` …) instead of the compact layout; JSON parity is fine.
- **batch `mv` dry run drops a source already inside the destination** —
  `mv --tag beta --to other/` omits `other/UPPERCASE.md` from `moves` and
  `collisions` is null; the documented contract lists collisions.
- **`lint --strict` promotes HYALO006/HYALO008 to errors** — the repository
  vault gives 19 HYALO008 errors under `--strict`. The short `--strict` help
  and `.claude/CLAUDE.md` say only schema warnings are promoted; the long
  help says otherwise. Pick one and fix the other.
- **`.expect()` outside tests** in four infallible-by-construction places
  (`scanner/body_state.rs:173`, `scanner/strip.rs:68,264,327`,
  `body_syntax.rs:231`); convention violation only, each traced as safe.
- **`numbered_heading_repair`** (`anchor.rs:451-459`) zips two independently
  filtered iterators that must apply the same predicate; add a shared filter
  or a `debug_assert_eq!` on lengths.
- **Two user errors bypass the JSON envelope and exit 2** —
  `crates/hyalo-cli/src/commands/apply.rs:183` (the 8 GiB staging-budget
  `bail!`, reachable from every bulk mutator) and
  `commands/drop_index.rs:134-137` (a non-NotFound delete failure) are plain
  anyhow errors, unlike their siblings that use `hyalo_core::user_error`.
  DEC-307 says every hyalo-own user error exits 1 through the envelope.
- **`lint --fix` lost DEC-317's batched fsync** — `lint/file.rs:45-56`
  always opens `WriteSession::new(Durability::PerFile)` and
  `lint/engine.rs:101` drives the pass with `par_iter()` and no shared
  session; the batched variant is used only by a unit test. Every other bulk
  mutator goes through `PreparedChangeSet::new(dir, count)`. Silent perf
  regression of the 48.1 s → 2.35 s result. `types set --default`
  (`types.rs:440`) has the same problem via a hard-coded count of 0.
- **`hyalo init` rewrites a CRLF `CLAUDE.md` as LF** —
  `init.rs:1398/1668` rebuild the whole file with `lines()` and push `\n`
  for every line, including lines outside the managed region. No CRLF test
  exists in init.rs; okf.rs has one for the same situation.
- **`is_pid_alive` is always `true` off Unix** (`index.rs:2043-2045`), so
  `find_stale_indexes` can never flag an orphaned index file on Windows. No
  comment marks the gap; implement via `windows-sys` or document it.
- **`MutationJournal::rename_entry`** (`journal.rs:216-249`) has no
  production caller and discards the result of an update-only refresh,
  re-introducing the fixed "unindexed entry dropped" bug class. Delete it.
- **Advisories that bypass `-q`** — the DEC-270 list-collapse note in
  `set.rs:686-693` and skip notes in `lint/file.rs` and `find/mod.rs:253`
  use raw `eprintln!` instead of `crate::warn::note`.
- `views.rs::load_views` swallows a malformed `.hyalo.toml` into an empty
  map and then reports "unknown view" instead of the parse diagnostic
  (single-reviewer finding, not re-verified).
- `okf.rs:16-19` module doc claims dry-run exits non-zero on drift; the
  implementation (iteration 274) deliberately always exits 0 and reports
  drift in `results.changed`.
- `scan_one_file` (`index.rs:2529-2537`) clones `links` and `self_anchors`
  into the entry and also returns the original `FileLinks`, doubling the
  allocation per file on the `create-index` hot path.
- `.expect()` outside tests also in `hyalo-mdlint/src/schema.rs:323` and
  `hyalo-cli/src/output.rs:206,227,410,437`; each infallible today.

### JavaScript deliverables

- **Version floor claim is wrong** — `pi-package/README.md:47-50` says typed
  tools need hyalo ≥ 0.21, but `hyalo_set`/`hyalo_task` inject
  `--internal-mutation-report` (`api.ts:438`), first shipped in v0.24.0; a
  0.21–0.23 binary answers with a clap error, exit 2. Document ≥ 0.24.
- **`pi-runtime.ts:22` throws `HyaloError` without parsing the envelope**, so
  `.envelope`, `.effects` and `.category` are undefined on that path only.
- **Timeout handling** (`api.ts:281-288`) sends SIGTERM and rejects at once:
  no SIGKILL escalation, no wait for `close`, data listeners stay attached.
  A retry can overlap a still-running mutation.
- `hyalo.ts:603-606` lints the absolute `rawPath` before computing the
  relative one; the CLI's "do not pass absolute paths" warning is dropped.
- `pi-package/lib/hyalo-api.d.ts` exports only `PiConfigInfo` as a type;
  `Envelope`, `FindResult`, `ExecutionOptions` are unreachable, hence the
  `Awaited<ReturnType<…>>` workarounds in the extension.

### Jev helper and tidy skill

- **Undeclared policy type is reported as "unavailable"** —
  `npm/jev/src/prepare.ts:54`: a type missing from `[schema.types]` makes
  `types show` exit 1, which `io.ts:43` maps to `Unavailable("Hyalo read
  failed")`, so the run exits 1 with the code `jev.md:137` documents as
  "unavailable credentials/service". Validate types up front and exit 2
  (`Invalid`) naming the type.
- **Windows npm shim cannot be spawned** — `io.ts:33` spawns without a shell,
  which cannot run `hyalo.cmd`; `references/jev.md:15` should say "native
  executable, not the npm shim", or the helper should reject `.cmd`/`.bat`.
- **Section size limit defers with reason `invalid string`** —
  `prepare.ts:74`: a section over 18 000 bytes or empty is deferred with a
  generic reason, contradicting the "explicit outcomes" promise.
- **Strict key whitelists on the provider response** (`decisions.ts:10,13,25`)
  turn any additive provider field into exit 2 "invalid provider response".
  Ignore unknown keys while validating known ones, or document that an API
  envelope change needs a helper release.
- **Dead embedded copies** — only `crates/hyalo-cli/templates/jev/jev.{md,mjs}`
  are `include_str!`'d; the copies under `templates/pi/skills/hyalo-tidy/` and
  `templates/codex/…` are embedded by nothing and exist only to satisfy the
  mirror gates (≈118 KB in the crate tarball).
- No DEC records the Jev integration (decision log ends at DEC-332);
  `docs/codex-integration.md` does not mention the skill assets or receipt.

### Repository hygiene and CI

- **`deny.toml` ignores are stale** — RUSTSEC-2025-0141 (bincode) and
  RUSTSEC-2024-0320 (yaml-rust) cite a comrak→syntect chain that is gone;
  `cargo tree -i` matches nothing. The "136 unique crates" comment is now 164.
- **cargo-deny never runs on PRs**; only inside the shared release workflow.
- **Codex plugin manifest pinned at 0.1.0**
  (`plugins/hyalo/.codex-plugin/plugin.json:3`), deliberately excluded from
  the version gate while it ships the jev helper that changes per release.
- **CHANGELOG versus tags** — GitHub has v0.20.0, v0.21.0, v0.24.0, v0.24.1.
  CHANGELOG lists 0.23.0 with an npm link, has no 0.22.0 heading, and the
  0.21.0/0.20.0 footer links read `TBD`; README cites 0.22.0;
  `npm-registry.yml` defaults to 0.22.0.
- **`docs/releasing.md` is stale** — claims seven targets built and tested
  while `release.yml` disables tests on three aarch64 targets; omits the npm
  job, AUR publication and the three `workflow_dispatch` npm modes.
- **Reusable workflows float on mutable tags with `secrets: inherit`** under
  write plus OIDC permissions (`release.yml:38`, `publish-crates.yml:23`,
  `cloudsmith-republish.yml:12`). First-party, bounded; pin or document.
- **`quality-gates` runs only on `pull_request`**; a direct push to main
  skips all eleven xtask gates.
- Local: `.git/ralph-loop/` holds 2.3 GB of stale worktrees and
  `.claude/worktrees/` 1.0 GB with 13 unregistered directories. Prune.

### Documentation that states something false

1. `docs/configuration.md:39-41,408` says hyalo never walks up for
   `.hyalo.toml`; `config.rs:788` adopts the nearest ancestor config.
2. `docs/configuration.md:426` reads `.hints[]` through `--jq`, which is
   always `[]` (DEC-313). Survives because `docs/` is outside
   `check-jq-recipes`.
3. `templates/skill-hyalo.md:29-31` (installed by `init --claude`): "npm
   0.22.0 is CLI-only, the API needs a source build". 0.23.0 shipped the API.
4. `skill-hyalo.md:665` "four rules narrowed" versus five; `:653` says
   MD001..MD059 while the binary has MD060.
5. `.claude/CLAUDE.md` and `rule-knowledgebase.md:359` say `required` means
   present and non-empty; the binary accepts `""` and rejects null. Both
   also say "snapshot format v2"; the binary writes 3.
6. `docs/configuration.md:337` names `links.fuzzy_min_confidence`; the JSON
   key is `results.links_fuzzy_min_confidence`.
7. `hyalo init --help` says `--claude` installs "the hyalo skill"; it installs
   two skills plus the rule.
8. README:285-293 "works once v0.21.0 is tagged" is dead text;
   `pi-package/README.md:3-6` "embeds these same files" (a vendored copy).
   `CLAUDE.md:19` and `schema-and-lint.md:183,247` cite paths that moved to
   `iterations/done/`.

### Undocumented behaviour

- CHANGELOG `[Unreleased]` has nothing from iteration 301: HYALO008,
  hidden-path resolution, `links fix` anchor repairs. `docs/ci.md:48` still
  says `--strict` promotes only HYALO006 while its own example now errors on
  HYALO008.
- `docs/configuration.md` documents none of `[scan]`, `[search] language`,
  `[lint] strict`/`[lint.rules]`, `[views]`, `[changelog]`, `[okf]`,
  `[links] frontmatter = false`.
- Help-only flags: `summary --depth/--recent`, `find --filenames0`,
  `--desc`, `lint-rules list --enabled-only/--disabled-only`,
  `links fix --expand-short-form`, `task set --status`,
  `changelog add --wrap`. HYALO003/HYALO004 are named nowhere.
- Gitignore honouring during discovery (see F5).

## P3 — nits and UX friction

- `new` on an existing file says "mutation did not finish completely …
  task toggle is not safe to retry blindly" with an empty `Effects:` list;
  `init --profile bogus` and `deinit --dir /nonexistent` carry the same empty
  trailer.
- Zero-result hint contradicts itself: `find --property status=planned
  --tag iteration` says "never to that value" while listing `planned (4)`.
- `mv --on-conflict skip` text output is a raw dump and its hint drops
  `--on-conflict`, so the hinted command exits 1.
- `new --dry-run` hints `lint --file <path>` for a file never written.
- `-q` silences the malformed `[schema]` warning but not the TOML one, and
  that lint exits 0 although the docs say it refuses.
- `summary` omits `links.broken_anchors` when 0; `task read --line` returns
  an object where `--all` returns an array; `task toggle --dry-run` lacks a
  `[dry-run]` marker; `properties rename --dry-run` lists all files unlimited;
  `links auto` hint adds `--no-first-only` although `first_only` is false.
- Help style: "Max" versus "Maximum"; `setup-node` pinned with a major-only
  comment; Node 22 in `jev-helper` versus 24 elsewhere.
- Jev: `cli.ts:22` is argument-order-sensitive; root-level notes can never be
  a folder "no-change"; the `TYPESAFE_` env strip is case-sensitive while the
  Windows environment is not; iteration 299/300 test counts are stale.
- README:183,186,190 and `rule-knowledgebase.md:169,213` ship `--apply` and
  `lint --fix` examples without `--dry-run`, and README:190 carries a trailing
  `# comment` that swallows an appended flag. A review agent's example runner
  ran `lint --fix` for real because of this; the six whitespace autofixes
  were reverted.
- Wishes from the dogfood run: a `--no-backlinks` filter (an unreferenced
  note with outgoing links is in neither `--orphan` nor `--dead-end`);
  `--fields links` cannot distinguish ambiguous from missing targets; a
  `task list`; `init`/`deinit --dry-run`.

## Structural observations

- **37 of 113 e2e modules (16 772 lines) are named by iteration number**
  (`iteration238_followups.rs` … `iteration282_directory_token_dominance.rs`).
  A reader looking for the tests of a behaviour has to know which iteration
  introduced it. Rename by behaviour as files are touched; do not bulk-move.
- **Three hand-maintained skill texts** (Claude template 1 011 lines, Pi 343,
  Codex 81) and a hand-maintained DEC claims paragraph in `.claude/CLAUDE.md`,
  `rule-knowledgebase.md` and the skill's pitfalls section. The Pi and Codex
  copies are gated only against their vendored twins, never against the
  Claude template, which alone carries two of the false claims above.
- `check-jq-recipes` skips README, `docs/*.md`, the root `CLAUDE.md` and the
  KB docs; no gate ties documented flags to clap (an ad-hoc inventory diff
  is currently clean).
- xtask carries two dead stubs (`check-dead-primitives`,
  `check-todo-annotations`) and `check-behavioral-contracts`, which re-runs
  test subsets that `cargo test --workspace` already runs and is wired into
  neither CI nor the justfile; its zero-tests-matched guard is therefore
  never exercised. `just check` lacks the eleven xtask gates CI runs.
- `crates/hyalo-cli/src/cli/args.rs` (3 611 lines) repeats the dry-run doc
  comment across 15+ subcommands with wording that has already drifted
  ("any files" versus ".hyalo.toml", "(the default; --apply writes)" present
  or absent), and inlines the same `file_positional`/`file`/`glob`/
  `dry_run`/`index_flags` quintet in `Set`, `Remove`, `Append`, `Mv` and
  `Lint` instead of a flattened shared struct like `FindArgs` already uses.
- The snapshot is written and read as one in-memory `Vec<u8>` (capped at
  512 MiB); the lazy BM25 decode mitigates load cost, but it is a whole-file
  materialisation the streaming guidance would flag. Awareness, not a
  defect.
- `.claude/CLAUDE.md` has grown into one dense paragraph of behavioural
  claims with DEC numbers. It is the surface most likely to drift (two
  contradictions found) and the one every agent session loads.

## Verified sound

- Link subsystem diff of iterations 297–301: percent-decoding bounds, the
  balanced-paren bare-destination parser, CRLF byte offsets in anchor
  repairs, `%2e%2e` handling, the `catalog::resolve` attachment fallback
  (does not mask broken links on the `find --broken-links` path), the
  outbound-rewrite de-duplication in `link_rewrite.rs`, and a new
  `ensure_within_vault` check in `resolve_case_insensitive` that closes a
  real symlink-escape gap. No new `unwrap`/`expect` outside tests.
- Frontmatter frame, splice and parse: column-0 close detection, mandatory
  re-parse verification before any splice result is returned, quoted
  rendering of `---`/`...` scalars, char-boundary-safe slicing.
- Atomic writes: temp-in-directory, `sync_all`, persist, directory fsync per
  DEC-317; `WriteSession` capture, verify, stage, publish with fault
  injection at every point.
- Schema: `deny_unknown_fields` on all three raw structs, and on every
  nested config struct in `config.rs`.
- jq isolation: the worker is a separate re-exec'd process, not a thread
  with an unsafe `Send` wrapper; the five `unsafe` blocks (rlimit, Windows
  job object, test-only waitpid/OpenProcess) carry correct SAFETY comments;
  timeout, kill and reap are enforced and tested.
- Exit-code contract in `run.rs`: all ten `AppError::User` sites go through
  `format_error`; the bare `eprintln!`s in the clap-interception block map to
  exit 2 by design. Hint `writes` flags derive centrally from
  `mutation::command_argv_writes`, cross-checked by a 40-command corpus.
- Snapshot loader: format-version refusal, unsafe-path rejection (null
  bytes, absolute, `..`, UNC, NTFS ADS), entry/edge/posting caps, lossless
  pid cast and ESRCH handling, deterministic tie-break by path in
  `ranked_matches`.
- hyalo-mdlint: suppression comment scoping, multi-fix offset application
  in descending order with re-validation, byte-versus-char column handling
  pinned by tests, fence and comment exemptions.
- JS API: envelope always carries `hints`; exit 0/1/2 mapping; every
  `FindArgs`/`ReadArgs`/`SummaryArgs`/`GlobalArgs` field mapped and every
  long flag exists in clap; `shell: false` everywhere; `--` before
  positionals; EPIPE/EOF on stdin handled; backpressure safe; dual
  ESM/CJS consumers type-check and run; `dist` not tracked; all copies
  byte-identical and gated (`check-ts-types`, `check-pi-runtime`,
  `check-pi-package-sync`, `generate-npm-packages --check`).
- Jev: network only in `ask` behind `--allow-network` plus a key; key never
  in argv, stripped from the child environment, refused in inputs; 1 MiB
  streamed cap; 12 s abort; one retry on 429/529; malformed responses defer;
  the model can only emit approved label ids; no write path; paths and
  exclusions checked by realpath, dev:ino and per-component lstat; all six
  `jev.mjs` and five `jev.md` copies identical and rebuilt by CI on three
  OSes.
- CI: fmt, clippy, three-OS tests, eleven xtask gates on PRs, no
  `continue-on-error`, third-party actions SHA-pinned, lockfiles tracked, no
  secrets in tracked files, launcher does no network fetch and has no install
  scripts.
- Dogfood: all `find` filter forms and invalid-operand exits, link kinds,
  fences and comments ignored, percent-encoded anchors, numbered and nested
  headings, aliases, case mismatch and ambiguity handling, CRLF, block
  scalars, unclosed frontmatter, `set`/`append`/`remove`, `mv` destination
  forms, lint variants, index create/stale/drop, malformed config handling.
  Timings on the 505-file vault: `summary` 0.20 s, `lint` 0.72 s,
  everything else under 0.1 s.

## Proposed follow-ups

1. `chore/deps-and-rust-1.99` (in progress): F1, toolchain, dependency and
   actions refresh, stale deny ignores.
2. Review follow-up iteration: F2, F3, F5, the two envelope bypasses, the
   `lint --fix` and `types set` durability regressions, the CRLF rewrite in
   `init`, the remaining P2 Rust items, the batch `mv` collision gap, the
   `summary --index` text layout, and the P3 hint fixes.
3. JS follow-up: F4, the version floor, `pi-runtime.ts` envelope, timeout
   escalation, type re-exports; Jev exit-code classification, Windows shim
   note, section-size reason, tolerant response validation, a DEC for Jev.
4. Docs follow-up: the eight false claims, the undocumented config sections
   and flags, CHANGELOG 0.22.0/0.23.0 and iteration 301 entries,
   `docs/releasing.md`, `--dry-run` on every mutating example; extend
   `check-jq-recipes` to `docs/` and README.
5. CI: cargo-deny on PRs, `quality-gates` on push, plugin manifest version in
   the gate, `just gates`.
