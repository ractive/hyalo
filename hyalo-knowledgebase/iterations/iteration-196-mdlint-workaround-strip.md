---
title: Iteration 196 — strip mdbook-lint convert_fix workarounds (upstream-gated)
type: iteration
date: 2026-08-17
status: planned
branch: iter-196/mdlint-workaround-strip
tags:
  - iteration
  - dependencies
  - upstream
related:
  - "[[docs/upstream-mdbook-lint-reports]]"
  - "[[iterations/iteration-193-vault-side-effects-and-dep-diet]]"
---

# Iteration 196 — strip mdbook-lint convert_fix workarounds (upstream-gated)

## Goal

Remove the downstream workarounds in `crates/hyalo-mdlint/src/engine.rs`
(`convert_fix`, roughly lines 694-890) that exist only to compensate for
upstream mdbook-lint autofix-coordinate bugs, once upstream ships them
fixed. Reserved as its own iteration by iter-193's non-goals ("Do not
attempt to fix upstream #456 here").

## Trigger — do NOT start until this holds

A published `mdbook-lint-core` / `mdbook-lint-rulesets` release **newer
than 0.15.2** whose history contains upstream PR
[#486](https://github.com/joshrotenberg/mdbook-lint/pull/486)
("fix(rules): correct MD011, MD034, and MD047 autofix corruption",
merged to their `main` 2026-08-05). Check with
`cargo info mdbook-lint-core` / the upstream CHANGELOG. Given the
maintainer's own agent-driven release cadence (five issues to release in
about two weeks last cycle), poll when convenient rather than watching.

Secondary, independent trigger: upstream fixes
[#491](https://github.com/joshrotenberg/mdbook-lint/issues/491) (MD018
continuation-line false positive, filed by us 2026-08-17) — see the MD018
task below; it can ride along or wait for the next gated pass.

## Context

- The workarounds and their upstream counterparts are documented, with
  code excerpts, in [[docs/upstream-mdbook-lint-reports]] §1 and in the
  posted comment on upstream #456
  (issuecomment-5319878913). Summary of what #486 fixed on their `main`:
  MD011 inclusive end column, MD034 Liquid-swallowing + char/byte length
  mix, MD047 no-op range.
- Two workarounds are **NOT** covered by #486 and must survive this
  iteration unless upstream's release notes say otherwise: the per-rule
  `rule_uses_byte_columns` allowlist (`engine.rs:702`), and the
  `line_len + 1` replace-vs-insert disambiguation heuristic. Those are the
  "contract half" of #456, still open upstream.
- The MD011 guard was deliberately written to self-neutralize
  (`content[end..].starts_with(']')`), so it is *safe* under a fixed
  upstream — this iteration removes it for clarity, not correctness.
- iter-193's audit notes two RUSTSEC ignores (`bincode` 1.x, `yaml-rust`)
  arriving via `comrak -> syntect`, and a `toml` 0.5 duplicate tracked as
  upstream #459 — both worth re-checking on any bump.

## Tasks

- [ ] Bump `mdbook-lint-core` / `mdbook-lint-rulesets` in
      `crates/hyalo-mdlint/Cargo.toml` to the triggering release. Read the
      release notes for anything beyond #486 (rule-behaviour changes can
      shift KB lint counts); diff `md018.rs` and any rule in the
      `rule_uses_byte_columns` allowlist for silent column-unit changes,
      as done in iter-193's audit.
- [ ] Remove the MD011 `end += 1` guard, the MD034 Liquid pull-back, and
      the MD047 recompute path from `convert_fix`. For each, first write
      (or re-enable) a fixture test that fails under 0.15.2 semantics and
      passes under the new release — proving the upstream fix is actually
      in the shipped crate, not just on their `main`.
- [ ] Re-verify the `rule_uses_byte_columns` allowlist against the new
      release's source (it "silently rots on every upstream release" — our
      own words on #456). Keep it unless the release implements #456's
      coordinate contract; if it does, plan its removal as a follow-up
      iteration instead of scope-creeping here.
- [ ] Keep the `line_len + 1` disambiguation heuristic and its CRLF
      branch unless the release notes say the encoding changed.
- [ ] MD018 (#491): if the triggering release also fixes it, add a
      regression fixture (continuation line `#472` not flagged; standalone
      `#foo` still flagged) and delete the latent-bug note in iter-193's
      Part B audit trail. If not fixed, leave everything alone.
- [ ] Re-check `deny.toml`: do the two RUSTSEC ignores still resolve
      through `comrak -> syntect`? Does `toml` 0.5 dedupe (upstream #459)?
      Record before/after unique-crate count if the tree changed.
- [ ] Run the full KB + fixture lint corpus before/after and diff violation
      counts; investigate any delta beyond the expected MD011/MD034/MD047
      fix-output changes.
- [ ] Post a short follow-up comment on upstream #456 reporting the
      embedder result (which workarounds we could delete, what remains),
      ONLY with explicit user authorization per session — third-party repo
      writes are user-gated; see
      [[iterations/iteration-194-post-upstream-mdbook-lint-reports]] for
      the pattern.

## Acceptance criteria

- [ ] Each removed workaround has a fixture test proving the upstream fix
      is present in the *published* crate hyalo now pins
- [ ] `convert_fix` contains no compensation for MD011/MD034/MD047 range
      bugs; the byte-column allowlist and `line_len + 1` heuristic remain
      (or their removal is planned in a follow-up, not improvised here)
- [ ] `cargo deny check` clean; RUSTSEC ignore list re-verified and
      annotated with the re-check date
- [ ] KB lint counts unchanged except deltas explained by the release notes
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q` all clean

## Non-goals

- Implementing #456's coordinate contract downstream, or removing the
  byte-column allowlist while upstream columns remain per-rule.
- Working around MD018 downstream — upstream has the report (#491) and a
  strong record of accepting exactly this class of fix.
- Any release; release stays user-gated.
