---
title: Upstream mdbook-lint reports (both posted 2026-08-17)
type: docs
date: 2026-08-17
status: archived
tags:
  - upstream
  - mdbook-lint
  - iteration-193
---

# Upstream mdbook-lint reports

Two upstream submissions prepared during [[iterations/iteration-193-vault-side-effects-and-dep-diet]].

**Status: BOTH POSTED (2026-08-17).** The autonomous agent that drafted these
was blocked from writing to a third-party GitHub repository (`gh issue
comment` / `gh issue create` against `joshrotenberg/mdbook-lint` is denied by
the permission classifier, correctly — an unattended agent should not speak
on the project's behalf in someone else's tracker).

- §1 (comment on #456) was **posted 2026-08-17** on the user's explicit
  instruction, with clear Claude Code attribution:
  <https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>
  The posted text amends this draft: upstream PR
  [#486](https://github.com/joshrotenberg/mdbook-lint/pull/486) (merged
  2026-08-05, unreleased) already fixed items 2, 3 and 5 on `main`, so the
  posted version marks those as independent confirmation rather than live
  bugs; items 1 and 4 remain the open contract half.
- §2 (MD018 false-positive issue) was **filed 2026-08-17** the same way, with
  the three reproduction cases re-verified against 0.15.2 immediately before
  posting: <https://github.com/joshrotenberg/mdbook-lint/issues/491>

Target repository: <https://github.com/joshrotenberg/mdbook-lint>

## 1. Comment on issue #456 — autofix coordinates

Target: <https://github.com/joshrotenberg/mdbook-lint/issues/456>
("Make autofix coordinates unambiguous and safe for library embedders")

**Posted 2026-08-17** (amended for upstream #486, Claude Code attribution):
<https://github.com/joshrotenberg/mdbook-lint/issues/456#issuecomment-5319878913>

---

Embedder data point, in case concrete cases help pin down the semantics.

[hyalo](https://github.com/ractive/hyalo) embeds `mdbook-lint-core` as a
library and applies `Fix` ranges itself: it converts `line`/`column` positions
to byte offsets in the original file, then splices. Everything below is a
workaround that currently lives in our `convert_fix` — pure tax from the
coordinate ambiguity this issue is about. Seen on 0.14, re-verified against
0.15.2.

**1. The column unit is not consistent across rules.**

Some rules compute columns from byte lengths (`line.len() + 1`), others index
a `Vec<char>` and emit char positions. On an ASCII line these agree; on a line
with any multibyte character they do not, and picking the wrong one lands the
splice mid-codepoint or on the wrong character. We ended up with a per-rule
allowlist:

```rust
fn rule_uses_byte_columns(rule_id: &str) -> bool {
    matches!(rule_id, "MD009" | "HYALO001") // HYALO001 is our own rule
}
```

Everything else gets a char-based walk, because the worst case there is a
dropped fix rather than a corrupted file. This list is empirical — derived by
reading each rule's source — and it silently rots on every upstream release.

Incidentally, 0.15.x changed `md018.rs` from `line.len()` to
`line.chars().count()` for the violation column. That was not in the release
notes, and it happens to fix a latent bug on our side (we had already
classified MD018 as char-columns). That is exactly the failure mode this issue
should close off: a correctness-relevant coordinate change that is
indistinguishable from a no-op refactor from the outside.

**2. MD011 end column is inclusive, not exclusive.**

MD011's own source comments say the end position is "+1 because end_pos is
0-based position of `]`" — so the end column points *at* the closing bracket
rather than past it. Applying the range as a half-open `[start, end)` leaves a
stray `]` on every fixed line, ASCII included. We extend by one, guarded so a
future upstream correction cannot make us overshoot:

```rust
if rule_id == "MD011" && content[end..].starts_with(']') {
    end += 1;
}
```

**3. MD034 swallows Liquid template markup into the autolink.**

The bare-URL boundary scan treats `{%` / `{{` (very common in GitHub Docs
sources) as URL characters, so the fix produces `<https://example.com/x{%>`
and breaks the template. We pull the range and the replacement back to just
before the Liquid opener.

**4. `end_column == line_len + 1` is overloaded.**

Two different intents share one encoding. MD009/MD023 mean "replace the whole
line, terminator included" (the replacement re-adds its own `\n`). MD022 means
"insert a line before the terminator" (the replacement is the original line
plus a `\n`). Translated to byte offsets, the end lands *on* the terminator in
both cases. Consuming `[start, end)` literally either duplicates a blank line
or drops the insertion, depending on which rule produced it. We disambiguate
heuristically, by comparing the replacement minus its trailing newline against
the original slice:

```rust
if let Some(without_nl) = replacement.strip_suffix('\n')
    && content.get(start..end).is_some_and(|orig| orig != without_nl)
{
    // "replace the line": also consume the terminator (CRLF-aware)
}
```

CRLF input needs an extra branch here, or a fix flips that line's terminator
to LF.

**5. MD047's range is a no-op after translation.**

For the common "file ends with N trailing newlines" case, MD047's fix
positions do not survive the line/column to byte-offset translation, so we
bypass upstream's range entirely and compute the trailing-newline fix
ourselves.

**What would remove all five for us:** byte offsets into the original document
on the `Fix` itself (or, failing that, a documented and enforced per-`Fix`
unit), half-open ranges everywhere with a test asserting it, and an explicit
"include the line terminator" flag instead of encoding intent in
`line_len + 1`. A conformance test applying every rule's fix to a fixture
containing a multibyte character and CRLF terminators would catch the whole
class.

Happy to test a prerelease against our corpus — we run these rules over
several thousand markdown files (our own knowledgebase plus MDN, GitHub Docs
and VS Code docs) and can report diffs.

---

## 2. New issue — MD018 fires on paragraph continuation lines

Suggested title: **MD018: false positive on paragraph continuation lines
starting with `#`**

**Filed 2026-08-17** (repro re-verified against 0.15.2, Claude Code
attribution): <https://github.com/joshrotenberg/mdbook-lint/issues/491>

---

**Version:** 0.15.2 (also present in 0.14).

MD018 ("No space after hash on atx style heading") fires on a wrapped
paragraph whose *continuation* line happens to begin with `#` — for example a
bare issue reference such as `#472`. That line is paragraph text, not a
heading, and `#472` is not a valid ATX heading under CommonMark anyway (ATX
requires a space after the `#` sequence).

**Reproduction**

```markdown
Upstream fixed this in PR
#472 and shipped it in 0.15.2.
```

MD018 reports a violation on line 2. Expected: no violation — line 2 is a
lazy continuation line of the paragraph started on line 1, so CommonMark never
parses it as a heading.

Contrast:

- `#foo` alone between blank lines — correctly flagged (it *is* a
  malformed heading).
- `PR #472` mid-line — correctly not flagged.
- `#472` as a continuation line — **falsely flagged**.

**Notes**

Verified still present in 0.15.2 by diffing `md018.rs` against 0.14: only the
fix generation and the column unit changed; the detection logic did not. The
rule appears to scan lines rather than consult block structure, so it cannot
see that the line is inside a paragraph.

Precedent for the shape of this bug: #274 ("MD018: false positive on Rust
attributes inside code blocks") was accepted and fixed.

**Why this matters for embedders**

The workaround available downstream is to disable MD018 wholesale, which also
loses the genuine `#Heading` typo detection the rule exists for.

---

## Follow-up

- [x] Post the #456 comment and record its URL in
      [[iterations/iteration-193-vault-side-effects-and-dep-diet]]
- [x] File the MD018 issue and record its number here

## Outcome — mdbook-lint 0.16.0 (2026-08-22)

Upstream shipped every report in this document. Release
[v0.16.0](https://github.com/joshrotenberg/mdbook-lint/releases/tag/v0.16.0)
(published 2026-08-20 from release PR #484) contains:

| Report | Upstream PR | Effect on hyalo |
| --- | --- | --- |
| §1 items 2/3/5 (MD011 inclusive end, MD034 Liquid swallowing, MD047 no-op range) | [#486](https://github.com/joshrotenberg/mdbook-lint/pull/486) | `convert_fix` MD011 `end += 1` guard, `trim_md034_liquid`, and the LF half of `md047_fix` deleted |
| §1 items 1/4 (the coordinate contract) | [#493](https://github.com/joshrotenberg/mdbook-lint/pull/493) | `rule_uses_byte_columns`, `line_col_to_byte` and the `line_len + 1` replace-vs-insert heuristic deleted; `Position::to_byte_offset` used instead |
| §2 ([issue #491](https://github.com/joshrotenberg/mdbook-lint/issues/491), MD018 continuation lines) | [#492](https://github.com/joshrotenberg/mdbook-lint/pull/492) | false positive gone; regression fixture added |

Removal work is [[iterations/iteration-196-mdlint-workaround-strip]].

### The last exception — MD047 on CRLF (closed in 0.16.1)

Shipped 0.16.0 `mdbook-lint-rulesets/src/standard/md047.rs` was still
LF-centric, despite the release note claim that "standard-rule fixes now
preserve CRLF":

1. The missing-trailing-newline branch built
   `Fix::insertion("Add newline at end of file", "\n", …)` with a hard-coded
   LF, so applying it to a CRLF file appended a bare LF.
2. `check_file_ending` counted trailing terminators with
   `content.chars().rev().take_while(|&c| c == '\n')`, which stops at the
   `\r` of the preceding CRLF — so a CRLF file with several trailing blank
   lines counted one terminator and MD047 never fired at all.

**Filed 2026-08-23** (user-authorized) as
[joshrotenberg/mdbook-lint#495](https://github.com/joshrotenberg/mdbook-lint/issues/495).
The follow-up comment on #456 reporting the embedder result was dropped by
user choice.

## Outcome — mdbook-lint 0.16.1 (2026-08-27): all compensation removed

Upstream closed #495 with
[#496](https://github.com/joshrotenberg/mdbook-lint/pull/496), released in
[v0.16.1](https://github.com/joshrotenberg/mdbook-lint/releases/tag/v0.16.1)
(2026-08-27). MD047 now counts trailing terminators via a
`strip_suffix("\r\n").or_else(strip_suffix('\n'))` loop (CRLF is one unit, so
both gaps above close at once) and inserts the file's own terminator — where
endings are mixed, the terminator of the line immediately before EOF wins.
The maintainer's note: MD047 was the last LF-centric rule.

That was hyalo's last piece of upstream compensation code.
[[iterations/iteration-250-mdlint-0161-workaround-strip]] deleted `md047_fix`
and its dispatch branch, so **every** rule — MD047 included — now goes through
the generic `convert_fix` translation with no per-rule special cases.
`grep -rn md047_fix crates/` is empty and `hyalo-mdlint` carries no
rule-specific fix overrides.

What remains in `engine.rs` is not upstream compensation and must not be
confused with it:

- `BYTE_COLUMN_RULE_IDS` (MD010/MD042/MD052) converts *reported diagnostic
  columns* — not `Fix` ranges — from byte to scalar offsets. Deliberately not
  filed upstream: each rule computes the offset a different way, so there is
  no single fix to track. Re-check it on every ruleset bump.
- `is_regex_false_positive` suppresses an MD011 false positive on regex prose
  (dogfood UX-4).

The CRLF and mixed-endings MD047 tests kept from the override era
(`hyalo-mdlint` unit tests plus `lint_fix_md047_crlf_and_mixed_endings_converge_in_one_run`
in the CLI e2e suite) are now the regression check that upstream's fix keeps
surviving hyalo's frontmatter splitting and CRLF-atomic offset translation.
