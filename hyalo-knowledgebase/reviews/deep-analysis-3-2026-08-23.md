---
title: Deep analysis #3 2026-08-23 — deferred areas (auto-link, BM25 math, schema, managed regions, jaq, error quality)
type: review
date: 2026-08-23
tags:
  - review
  - architecture
  - testing
status: active
related:
  - "[[reviews/adversarial-review-2026-08-23]]"
  - "[[reviews/deep-analysis-2-2026-08-23]]"
---

# Deep analysis #3 — the "Not reviewed" list from report #2

Covers exactly the six items deferred in `deep-analysis-2-2026-08-23.md`:
`auto_link.rs` scoring internals, `bm25.rs` ranking math, `schema.rs` validation
semantics, `init`/`deinit` managed-region editing, jaq evaluation semantics, and
error-message quality. Labels: **VERIFIED** = reproduced; **SUSPECTED** = static only.

---

## F3-1. `--jq` allows unbounded CPU and unbounded intermediate memory (HIGH)

**Location:** `crates/hyalo-cli/src/output.rs:790` — the only guard is an output cap:

```rust
/// Maximum total output size for a jq filter to prevent pathological filters
/// from causing unbounded memory growth (e.g. exponential-expansion patterns).
const JQ_OUTPUT_CAP: usize = 10 * 1024 * 1024; // 10 MiB
```

Two escapes, both VERIFIED on the release binary:

1. **Infinite CPU spin, no output** — a recursive filter that never emits:

```console
$ hyalo find --jq 'def f: f; f' --no-hints
→ hangs indefinitely (killed after 15 s, exit 137; ~1.6 MB RSS, pure CPU spin)
```

The output cap only fires when a value is produced; `f` recurses forever inside
the evaluator before emitting anything.

2. **Unbounded intermediate allocation** — the cap never sees intermediates:

```console
$ /usr/bin/time hyalo find --jq '[range(3e8)] | length' --no-hints
→ 300000000    (correct output, 8.7 s, maximum resident set size 4,810,866,688 bytes)
```

That's **4.8 GB RSS** to evaluate a one-liner. The output (a single number) is well
under the cap; the intermediate array is not counted anywhere.

**Impact:** `--jq` is user-supplied input evaluated with no time or memory limit. In
the project's own agent-driven workflow (CLAUDE.md instructs agents to build `--jq`
programs), an agent writing a wrong-but-plausible filter (`[range(...)]`, accidental
infinite recursion) wedges or OOMs the machine — and any wrapper that pipes vault
content into jq programs makes this a cheap DoS. Note this is *not* untrusted-vault
content (the filter comes from the user/agent, not the vault), so severity rests on
the DoS-yourself agent-loop scenario rather than an attacker.

**Fix:** (a) run the jaq iterator on a thread with a wall-clock deadline
(`filter.id.run(...)` is already a lazy iterator — check a `Instant` every N items and
bail); (b) track approximate allocation as well as output (jaq `Val`s expose sizes;
at minimum cap array-building by counting elements in a `Vec`-valued fold — or simpler:
cap total *emitted value count* and per-value serialized size, plus a global step
counter); (c) document the limit in `--jq --help`.

## F3-2. `init --claude` managed-section upsert anchors the END marker globally, corrupting user files (MEDIUM)

**Location:** `crates/hyalo-cli/src/commands/init.rs:926-927`:

```rust
let start_idx = lines.iter().position(|l| l.contains(SECTION_START));
let end_idx = lines.iter().position(|l| l.contains(SECTION_END));
```

`upsert_managed_section` finds the **first** `<!-- hyalo:end -->` anywhere in the file
— even one that appears *before* the start marker, e.g. a stray marker mention in user
prose. When `end_idx < start_idx` (or the first end marker belongs to something else),
the `s < e` guard fails and the function **appends a second managed section** instead
of replacing the existing one. Then `deinit`'s `strip_managed_section` — which
correctly searches for END only *after* START (init.rs:684) — strips the **original**
region and leaves the appended one orphaned.

**VERIFIED** end-to-end:

```console
$ cat .claude/CLAUDE.md
# T
stray <!-- hyalo:end --> here
<!-- hyalo:start -->
OLD
<!-- hyalo:end -->
tail
$ hyalo init --claude
→ updated  .claude/CLAUDE.md (appended managed section)      # should have replaced
$ hyalo deinit
→ updated  .claude/CLAUDE.md (stripped managed section)      # strips the OLD one
$ cat .claude/CLAUDE.md   # user's "tail" prose now followed by an orphaned full
                          # hyalo section that deinit claims was removed
```

The sibling function `strip_managed_section` (init.rs:680-689) already implements the
correct anchoring ("search for the end marker only after the start marker, so a stray
`<!-- hyalo:end -->` in user content before the managed section doesn't confuse the
match") — and the shared `managed_region.rs` module (`Markers::splice`,
managed_region.rs:70-74) fixed this exact bug class for OKF/MADR ("Find END strictly
after BEGIN so a stray marker mention in prose … can't be mistaken for the real
closer", citing iter-165/166). The CLAUDE.md upsert simply never got the fix.

**Impact:** repeated `init --claude` / `deinit` cycles on a CLAUDE.md containing
marker-like text in prose grow the file with duplicate sections and leave stale
"managed" content after deinit — silent content corruption in the file that steers
agent behavior.

**Fix:** make `upsert_managed_section` use the same anchored search as
`strip_managed_section` (end_idx = first END strictly after start_idx). Better: route
CLAUDE.md section editing through `managed_region::Markers` and delete the two
hand-rolled line-scanners.

## F3-3. Schema validation silently ignores unknown keys like `minimum`/`maximum` (MEDIUM)

**Location:** `crates/hyalo-core/src/schema.rs:447-459` (`RawPropertyConstraint`) —
captured fields are exactly: `type`, `pattern`, `item_pattern`, `values`, `min-length`,
`max-length`. Everything else in a property constraint is silently dropped by serde.

**VERIFIED:** with `type = "number"` plus `minimum = 1` / `maximum = 5` in
`.hyalo.toml`, a file with `priority: 99` lints **clean**:

```console
$ hyalo lint --no-hints   # schema with minimum/maximum on priority
→ bad.md: only the enum violation reported; priority 99 passes with "maximum = 5"
```

The module is otherwise exemplary about surfacing misconfiguration — `TryFrom` rejects
`values` on non-enums, `min-length` on non-strings, `pattern`+`item_pattern` together,
each with a specific error message ("so misconfigured TOML surfaces as an error rather
than silently discarding the configured values", schema.rs:490-493). But a user who
naturally writes JSON-Schema-style `minimum`/`maximum` (the names I reached for, and
the ones the error message for a wrong *type* almost invites: "expected number") gets
zero feedback that the constraint doesn't exist. Same for any typo (`patterns =`,
`value =`).

**Fix:** either implement `minimum`/`maximum` (trivial: two `Option<f64>` fields and
two comparisons in the number arm — the natural fix given they're expected names), or
make `RawPropertyConstraint` deny unknown fields (`#[serde(deny_unknown_fields)]`) so
every unsupported key is a hard config error consistent with the module's stated
philosophy.

## F3-4. `resolve_file` reports `../foo.md` inside the vault as "resolves outside vault boundary" (LOW)

**Location:** `crates/hyalo-core/src/discovery.rs:357-367`:

```rust
// Reject path traversal attempts — use `OutsideVault` so the user
// understands the path was rejected because it escapes the vault, not
// because the file doesn't exist.
if normalized.starts_with('/')
    || has_parent_traversal(&normalized)
    || Path::new(&normalized).is_absolute()
{
    return Err(FileResolveError::OutsideVault { ... });
}
```

The check is lexical: any `..` component is rejected *before* resolution, even when
the path stays inside the vault. **VERIFIED:** from `/tmp/err/sub` (a subdir of the
vault `/tmp/err`), `hyalo read ../broken.md` → `"file resolves outside vault
boundary"` — false: `/tmp/err/broken.md` is squarely inside the vault, and the file
exists. The user is told the path escapes when the actual policy is "no `..`
allowed, spell it vault-relative".

**Impact:** misleading error in a common workflow (agent or user cwd'd in a subdir
passes a relative path with `..`); the message actively misdiagnoses. The same error
string is used for genuine escapes, so users learn to distrust it.

**Fix:** either resolve-then-check (join with dir, canonicalize, compare to root —
the machinery exists in `fs_util::escaping_write_target`) and accept in-vault `..`
paths; or keep the lexical rule but say so: "paths must be vault-relative without `..`
components — use `broken.md`, not `../broken.md`". The comment at discovery.rs:357
shows the intent was the former ("so the user understands the path was rejected
because it escapes the vault") but the implementation doesn't distinguish.

## F3-5. `find` errors carry no hints; near-miss and empty-path cases give no guidance (LOW)

**Location:** across `commands/*` — error envelopes generally lack a `hint` field for
the most common failure (VERIFIED): `hyalo set nosuch.md --property x=1` →
`{"error": "file not found", "path": "nosuch.md"}`, no hint. `hyalo read ''` →
`"file not found", "path": ""` — an empty path arg is almost certainly a shell
quoting accident, yet produces the identical message. Running from a subdir with a
vault-relative path (`cd sub && hyalo set a.md ...`) fails with "file not found" when
`a.md` exists at the root — no hint that paths are vault-relative, not cwd-relative
(though `strip_dir_prefix` handles the opposite direction: cwd-style
`vault/foo.md` from outside the vault works, discovery.rs:350-355).

**Impact:** for an agent-driven CLI whose whole UX philosophy is drill-down hints
(DEC-031/DEC-040), the error path — where guidance matters most — is hintless, and
agents burn retry cycles rediscovering that paths are vault-relative.

**Fix:** add hints to the three canonical errors: file-not-found (→ "paths are
vault-relative; run `hyalo find --file <glob>` to locate the file"), empty path (→
"empty path — check shell quoting"), and the F3-4 message. One helper, three call
sites.

---

## Areas checked and found sound (one sentence each, per instructions)

- **`link_score.rs` fuzzy confidence model:** the basename-weighted 0.7/0.3 split
  with a 0.85 token floor is well-reasoned against a real corpus failure, documented
  with the failure case, and its constants are cross-referenced — no change.
- **`auto_link.rs` core mechanics:** VERIFIED correct on self-link exclusion,
  stem-collision ambiguity (two `dup.md` files → reported ambiguous, no links
  written), alias linking (both `ZedMaster` and `Zed` linked with proper
  `[[zed|alias]]` form), word-boundary handling, code-block/heading/link skipping,
  self-title mentions, frontmatter exclusion, and the TOCTOU content-compare guard
  before apply (auto_link.rs:1032-1038). Path-traversal/`has_root` validation on
  `--file` (auto_link.rs:483-495) is correct including the Windows drive-relative
  case. One design observation: the title inventory is frontmatter-`title`/stem/
  `aliases` only — H1-heading titles are **not** link targets (a file whose only
  identity is its H1 gets no auto-links; `--min-length` note in docs only covers
  common-word noise). If intended, document it; if not, it's a feature gap.
- **BM25 ranking math (bm25.rs:421-429, 555-700):** formula matches the documented
  Okapi variant; IDF is the non-negative `ln(1 + …)` form; AND/OR/phrase/exclusion
  semantics VERIFIED correct via probes (`zebra -lion` excludes correctly,
  non-adjacent `"quick fox"` phrase correctly rejected, duplicate query terms
  idempotent — `zebra zebra` returns the same ranking as `zebra`, no double-counted
  tf). Phrase scoring's `phrase_rejected` set correctly strips bag-of-words credit.
  Deserialize-time `validate_doc_ids` guards the OOB panic path. Math is sound; the
  CJK tokenization hole remains report #2's F-2.
- **`schema.rs` config validation semantics** (minus F3-3): enum-with-did-you-mean,
  strict number-typing (string `"3"` correctly rejected against `type = "number"`),
  ordered `bind` globs, exempt globs — all VERIFIED working; flat raw struct with
  explicit cross-field validation is a genuinely good deserialization pattern.
- **`managed_region.rs` (OKF/MADR splice):** anchored marker search is correct and
  tested; the bug is only in init.rs's parallel copy (F3-2), which is the argument
  for deleting the copy.

---

## Test-quality notes for this pass

- F3-2 exists *because* two parallel implementations of marker splicing diverged —
  the init.rs copy has tests (`upsert_managed_section_appends_when_absent`,
  init.rs:1217+) that test the happy paths only, with no stray-marker adversarial
  case. When report #2's ARCH-consolidation happens (route CLAUDE.md through
  `managed_region.rs`), the missing test class comes free.
- F3-3 would have been caught by a config-lint e2e test asserting that *every* key a
  user plausibly writes is either implemented or rejected — a small "unknown schema
  key" test matrix.
- The jq resource issues (F3-1) need a test that runs filters under a deadline;
  suggest `assert_cmd` with `.timeout()` once a limit exists (the fix makes the test
  trivial: assert the command errors rather than hanging).

## Not reviewed (still open, unchanged)

- jaq crate internals (trusted dependency; only the embedding's limits were audited).
- Cross-platform behavior of everything in this pass on Windows (report #1 M-2).
- `common_words.rs` word-list quality for non-English vaults.
