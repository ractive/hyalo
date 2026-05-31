---
title: "Iteration 150 — Link handling refactor (unify resolver + writer)"
type: iteration
date: 2026-05-31
status: planned
branch: iter-150/link-handling-refactor
tags:
  - iteration
  - refactor
  - links
  - wikilinks
  - mv
  - architecture
related:
  - "[[dogfood-results/dogfood-v0160-iter-149-creative]]"
  - "[[iterations/done/iteration-117-case-insensitive-link-resolution]]"
  - "[[iterations/done/iteration-132-mv-wikilinks]]"
  - "[[iterations/done/iteration-134-links-fix-short-form-wikilinks]]"
  - "[[iterations/done/iteration-136-wikilink-md-suffix-and-short-form-mv]]"
  - "[[iterations/done/iteration-137-cross-platform-link-resolution]]"
  - "[[decision-log]]"
---

## Goal

Stop the dogfood-iteration-dogfood treadmill on link handling. Replace
the current writer/resolver asymmetry with a single canonical
representation of a link (preserving the user's written form) flowing
through one resolver and one writer shared by every mutator.

Concretely this iteration closes the three HIGH bugs from the
[[dogfood-results/dogfood-v0160-iter-149-creative]] session:

- **BUG-1** — `hyalo mv` strips link directory prefix
  (`[[sub/target]]` → `[[renamed]]`).
- **BUG-2** — silent retargeting after `mv` chains; a renamed link
  ends up pointing to an unrelated same-basename file with no warning.
- (Related) `.hyalo-index` is not patched after `mv`, so subsequent
  operations work against stale data — same family.

And makes the *next* link feature additive rather than a per-shape patch.

## Why now — what the history tells us

Three independent passes (KB history, code architecture, git log) all
land on the same diagnosis:

1. The writer/resolver split has been refactored six times
   (iter-39b → 91 → 132 → 133 → 136 → 137) and the same family of
   bug keeps resurfacing. Each iteration adds the *next* link shape the
   resolver knows but the writer doesn't (`[[b]]`, `[[b|alias]]`,
   `[[b#sec]]`, `[[./b]]`, short-form preserve, `.md` suffix tolerance).
   `dogfood-v0160-iter-149-creative` BUG-1/2 is iteration seven of the
   same pattern.
2. The code has **three independent resolvers** (`StemIndex` in
   `link_fix.rs:160`, `CaseInsensitiveIndex` in `link_graph.rs:108`,
   ad-hoc canonicalization in `plan_inbound_rewrites` at
   `link_rewrite.rs:517-554`), each with subtly different
   `.md`/case/ambiguity handling. They drift.
3. `choose_wikilink_form` (`link_rewrite.rs:377-411`) decides emission
   form from *post-move* vault uniqueness, ignoring what the user
   originally wrote. This is the root cause of BUG-1: `[[sub/target]]`
   gets collapsed to `[[renamed]]` because the new basename happens to
   be unique vault-wide. BUG-2 follows from BUG-1: the same basename
   matches the wrong target on a later resolution.
4. `commands/mv.rs` does **not** call `LinkGraph::rename_path` against
   the persisted snapshot index — the index is stale after every `mv`.
   (`link_graph.rs:269` defines the helper but nothing wires it.)

## Scope decisions

### IN — restructure resolution + rewrite

1. **Single canonical link type.** Add `ResolvedLink` to
   `crates/hyalo-core/src/links.rs` carrying *all* of:
   - `written: String` — exactly what the user wrote between the
     delimiters, fragment + alias included.
   - `kind: LinkKind` (Wikilink | Markdown — unchanged).
   - `target_raw: String` — the target as parsed (no `.md` strip, no
     fragment strip, no normalization).
   - `target_norm: String` — vault-relative normalized form used as
     the index key.
   - `alias: Option<String>`.
   - `fragment: Option<String>`.
   - `form: WrittenForm` — `Bare` / `PathRelative` / `VaultAbsolute` /
     `DotRelative` / `MdSuffixed`. This is the key new field: the
     writer reads it to know how to emit a *new* target without
     consulting vault uniqueness. Preserves the user's syntax under
     `mv`.

   `Link` (the lossy type) keeps existing for back-compat at parse
   boundary but is constructed from `ResolvedLink`, not the reverse.

2. **Single resolver.** Introduce `crates/hyalo-core/src/link_resolve.rs`
   with one `LinkResolver { index: Arc<VaultIndex> }` exposing:

   ```rust
   pub fn resolve(&self, link: &ResolvedLink, source: &Path)
       -> Resolution // Hit { vault_path } | Broken(Reason) | Ambiguous(Vec<Path>)
   ```

   - Folds `StemIndex`, `CaseInsensitiveIndex`, and the ad-hoc
     canonicalization in `link_rewrite.rs:517-554` into one place.
   - Owns the precedence rules between match strategies (exact path,
     case-insensitive path, `.md` suffix tolerance, stem lookup) —
     making the iter-137 `ShortFormStemMismatch` vs `LinkCaseMismatch`
     question a single ordered enum, not three implementations.
   - Returns `Ambiguous` (with the candidate list) when stem lookup
     finds >1 match — surfacing BUG-2 (silent retargeting) as a hard
     diagnostic instead of a silent rewrite.

3. **Single writer.** Add `LinkWriter::rewrite(span, new_target,
   policy: PreserveForm)` in `crates/hyalo-core/src/link_write.rs`.
   - Always re-splices from the original byte offsets in `LinkSpan`,
     so fragment + alias + delimiters survive structurally
     (already true for `mv` and `links fix`; not for `auto_link::
     apply_matches` at `auto_link.rs:647`).
   - `policy = PreserveForm` uses `ResolvedLink.form` to emit the new
     target in the *user's* shape: bare stays bare, path-form stays
     path-form, `./` stays `./`, vault-absolute stays vault-absolute.
   - The vault-uniqueness branch in `choose_wikilink_form`
     (`link_rewrite.rs:377-411`) is deleted — replaced by
     `policy.emit_target(&resolved, new_path)`.

4. **Mutators converge.** Route every link-text mutator through
   `LinkWriter`:

   | Caller | Current site | New |
   |---|---|---|
   | `mv` single | `plan_inbound_rewrites` (`link_rewrite.rs:425`) | `LinkWriter::rewrite(span, new_target, Preserve)` |
   | `mv` batch | `plan_inbound_rewrites_with_fm` (`link_rewrite.rs:1084`) | same |
   | `links fix --apply` | `build_replacements_for_file` (`link_fix.rs:922`) | same, with `Preserve` |
   | `links auto --apply` | `apply_matches` (`auto_link.rs:581-651`) | same, with `Bare` policy (this caller intentionally emits short-form) |
   | frontmatter wikilinks | `plan_frontmatter_wikilink_rewrites` (`link_rewrite.rs:1280`) | same; YAML-string path stays its own concern but writer is shared |

5. **Persistent index gets incremental updates after `mv`.** Wire
   `commands/mv.rs::run` to call `LinkGraph::rename_path`
   (`link_graph.rs:269`) on the snapshot index after a successful
   apply, then `save_index_if_dirty`. Same shape as iter-149's
   `add_index_entry` integration. Stale-index after-mv is closed.

### IN — bug fixes that fall out of the refactor

- **BUG-1 fix** — `choose_wikilink_form` deletion + `PreserveForm`
  writer means `[[sub/target]]` stays `[[sub/renamed]]`.
- **BUG-2 fix** — resolver returns `Ambiguous` on stem collision;
  `mv`'s writer surfaces this as a warning (not a silent rewrite) and
  refuses to rewrite the ambiguous link unless `--allow-ambiguous` is
  passed.
- **Index staleness fix** — `mv` patches the on-disk graph.

### OUT — explicit non-goals (deferred to follow-ups)

- **Reference-style `[label][ref]`** — `decision-log.md` DEC-037 still
  applies. Won't fix here; will not break.
- **Frontmatter-as-link expansion** (`children`, `aliases` etc.) —
  DEC-039 still applies.
- **Image link asymmetry** (`![](path)` dropped vs `![[path]]` kept,
  `links.rs:103-104`) — note in code, separate iter.
- **Adding new link syntax** — this is a refactor, not a feature.
- **Behavioural change for non-buggy cases** — every existing e2e
  test that passes today must still pass; the refactor is invisible
  to the happy path.

## Edge cases that MUST be covered by tests

(All cited from real history — every one of these has bitten before.)

1. `mv sub/target.md sub/renamed.md` with inbound `[[sub/target]]` →
   stays `[[sub/renamed]]` (not `[[renamed]]`). **BUG-1 repro.**
2. `mv a/foo.md b/foo.md` while `c/foo.md` exists; resolver returns
   `Ambiguous`, writer warns + skips the rewrite. **BUG-2 repro.**
3. `[[foo.md]]` → `[[bar.md]]` survives `mv foo bar` with `.md`
   suffix preserved (iter-136).
4. `[[./b]]` survives as `[[./new]]` (iter-133).
5. `[[foo|My Foo]]` → `[[bar|My Foo]]` (alias preserved across
   shapes — iter-132).
6. `[[foo#section]]` → `[[bar#section]]` (fragment preserved).
7. `[../sibling.md](../sibling.md)` markdown link survives an `mv` of
   its target without becoming wikilink (form preserved).
8. Vault-absolute `[link](/site/foo.md)` with `site_prefix` survives
   intact (iter-43).
9. Case-insensitive resolution on MDN-style PascalCase URLs vs
   lowercase disk (iter-117) — unchanged behavior.
10. Cross-platform: same e2e suite passes on macOS + Linux + Windows
    (iter-137 lesson — keep the case_index unit tests and add cases
    for the new ambiguity branch).
11. Persistent `.hyalo-index` after `mv`: re-running `hyalo find` with
    `--index` sees the new path immediately (no full rebuild).

## Tasks

- [ ] Add `ResolvedLink`, `WrittenForm`, `Resolution`, `PreserveForm`
      types in `hyalo-core/src/links.rs` + new `link_resolve.rs`,
      `link_write.rs` modules
- [ ] Implement `LinkResolver` consolidating `StemIndex` /
      `CaseInsensitiveIndex` / `plan_inbound_rewrites`
      canonicalization
- [ ] Implement `LinkWriter::rewrite` with `PreserveForm` /
      `Bare` policies; re-splice from `LinkSpan` byte offsets
- [ ] Migrate `mv` single (`commands/mv.rs` + `link_rewrite.rs`) to
      `LinkResolver` + `LinkWriter`
- [ ] Migrate `mv` batch (`link_rewrite.rs:860+`) similarly
- [ ] Migrate `links fix --apply` (`link_fix.rs:864+`)
- [ ] Migrate `links auto --apply` (`auto_link.rs:581+`) — with
      `Bare` policy
- [ ] Migrate frontmatter wikilink rewriter
      (`plan_frontmatter_wikilink_rewrites`)
- [ ] Delete `choose_wikilink_form` (`link_rewrite.rs:377-411`) and
      the now-dead per-callsite stem-lookup helpers
- [ ] Wire `commands/mv.rs::run` to call `LinkGraph::rename_path` +
      `save_index_if_dirty` on the snapshot index
- [ ] Add `Ambiguous` surfacing in `mv` (warning + skip; opt-in
      `--allow-ambiguous` flag with explicit caller responsibility)
- [ ] Unit tests for `LinkResolver` (precedence ordering, ambiguity,
      case folding, `.md` suffix, `./` form)
- [ ] Unit tests for `LinkWriter` (each `WrittenForm` round-trips
      under rewrite)
- [ ] E2E tests for all 11 edge cases above; tag the BUG-1 / BUG-2
      ones explicitly in the test name
- [ ] Add `tests/e2e/mv_link_form_preserve.rs` covering shapes 1–8
- [ ] Cross-platform CI green (run on Linux + Windows runners — iter-137
      lesson)
- [ ] Update `commands/mv.rs --help`, README, agent rule template
      (`templates/rule-knowledgebase.md`) if any wording assumes the
      old short-form-on-uniqueness behavior
- [ ] Bench: confirm batch `mv` over 100 files with link rewrites
      doesn't regress (target: within 10% of current)

## Acceptance Criteria

- [ ] BUG-1 fixed: `mv sub/target.md sub/renamed.md` with inbound
      `[[sub/target]]` produces `[[sub/renamed]]`, exact byte-equal
      modulo the basename
- [ ] BUG-2 fixed: ambiguous stem collision produces a warning + skip,
      not a silent retarget; exit code reflects the skip
- [ ] `.hyalo-index` is consistent immediately after `hyalo mv` (no
      `create-index` rebuild needed)
- [ ] One `LinkResolver` in `link_resolve.rs`; `StemIndex` +
      `CaseInsensitiveIndex` + ad-hoc canonicalization in
      `link_rewrite.rs` removed or reduced to thin adapters
- [ ] One `LinkWriter` in `link_write.rs`; every mutator routes through
      it
- [ ] `choose_wikilink_form` is deleted (grep returns 0)
- [ ] All existing e2e tests pass unchanged (no behavior change on the
      happy path)
- [ ] Cross-platform CI green on Linux + macOS + Windows
- [ ] `cargo fmt && cargo clippy --workspace --all-targets -- -D
      warnings && cargo test --workspace -q` clean
- [ ] Code-line delta: net **negative or near-zero** despite the new
      modules — the per-callsite duplication should outweigh the
      shared infrastructure. Document the delta in the PR.

## Notes for the implementing agent

- This is a refactor: behavior on the happy path must not change.
  Diff every existing e2e test result; if anything other than the
  BUG-1/BUG-2 tests changes, that's a regression.
- Do **not** widen scope. The deferred items (reference-style,
  frontmatter-as-link, image asymmetry) each deserve their own plan.
  Add a `## Deferred` section to the PR description listing them.
- The PR will likely be large. Split commits along these lines:
  (1) add new types/modules without callers; (2) migrate mv;
  (3) migrate links fix; (4) migrate links auto; (5) migrate
  frontmatter; (6) wire index incremental update; (7) delete dead
  code. Reviewable per-commit.
- Cross-platform is non-optional. Run the full e2e suite on Linux
  before requesting review — iter-137 shipped four merged PRs that
  silently broke on non-macOS.
