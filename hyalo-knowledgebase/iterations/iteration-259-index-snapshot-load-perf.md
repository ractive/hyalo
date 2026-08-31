---
type: iteration
title: Iteration 259 — snapshot-index load/deserialize floor
date: 2026-08-31
status: completed
tags: [iteration, performance, carry-over]
branch: iter-259/index-snapshot-load-perf
depends-on: "[[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]"
---

# Iteration 259 — snapshot-index load/deserialize floor

## Goal

Carry-over from [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]'s
FIND-8 investigation, never folded into 257 or 258 (both were bug/UX-shaped,
not perf). Quoting that iteration's Outcomes section verbatim:

> Worth noting for future perf work: the remaining ~0.37 s floor on this
> vault is snapshot load and deserialization of a 123 MB index, which dwarfs
> everything `find` does afterwards. Any further `--fields` micro-optimisation
> on an indexed vault is measuring noise until that is addressed.

256 fixed the *specific* quadratic dedupe bug it found (DEC-259,
`CaseInsensitiveIndex::insert`, 62.4 ms → 4.2 ms) but explicitly did not
touch the much larger fixed cost underneath it: loading and deserializing a
`.hyalo-index` snapshot before any `find` filtering happens at all. On the
14 399-file / 123 MB MDN vault used for that measurement, this floor was
~0.37 s — the majority of total `find` latency on that vault, and the reason
127's author flagged that further per-field timing work would be measuring
noise rather than signal until this is addressed.

This iteration is about locating *where* that floor comes from (MessagePack
deserialization itself, disk I/O, allocation shape, or something else) and
whether any of it is reducible without changing the on-disk snapshot format
in a way that breaks compatibility — not about a specific fix committed in
advance.

## Tasks

### PERF-1: characterize the snapshot-load floor [1/1]

- [x] Reproduce 256's measurement independently: build (or reuse, if still
      available) a large synthetic or real vault (MDN-scale, ~14k files /
      ~120 MB index) and profile `hyalo find --limit 1` end to end,
      isolating snapshot read + deserialize from everything downstream.
      Confirm the ~0.37 s figure still holds against current `main` (perf
      characteristics may have shifted since 256). Use `xtask bench-scale`
      if it fits this vault size, or a dedicated one-off harness if not —
      note which, and why, in the outcome.
- [x] Break the floor down: how much is reading bytes off disk (cold vs warm
      cache), how much is MessagePack decode, how much is post-decode
      reconstruction (e.g., rebuilding in-memory indexes/maps from the
      flat snapshot). Use a profiler or manual instrumentation — whichever
      gives trustworthy numbers fastest.
- [x] Decide, with evidence, whether there is a reducible cost here at all
      (e.g., streaming decode instead of whole-buffer deserialize, avoiding
      an intermediate allocation, lazy-loading index sections that most
      `find` invocations don't touch) or whether the floor is inherent to
      "read N megabytes and materialize M structures" and not worth chasing
      further. Either finding is a valid outcome — this task is
      "characterize and decide", not "must ship a speedup".

## Acceptance criteria

- [x] PERF-1 produces a written, numbers-backed characterization of the
      snapshot-load floor (where the time goes, on what vault size), checked
      into this file's Outcome section or a `research/` note it links to.
- [x] Either a concrete, scoped follow-up fix is identified (and filed as its
      own iteration if it's more than a small patch) or the floor is
      recorded as inherent/not-worth-chasing with a DEC explaining why —
      not left as an open question.
- [x] If any code changes are made in this iteration, gates green: `cargo
      fmt`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace -q`, all `xtask check-*`, `hyalo lint --strict`.

## Non-goals

- Changing the on-disk snapshot format's wire compatibility without a
  clear DEC covering migration — this iteration characterizes the cost
  first; a format change (if warranted) is its own follow-up iteration.
- Re-litigating DEC-259's `CaseInsensitiveIndex::insert` fix — that
  quadratic bug is already fixed; this is the cost *underneath* it.

## Links

- [[iterations/iteration-256-envelope-help-forwarding-and-index-cost]]

## Outcome

Characterized, decided, and the decision is *not* "inherent". One decision
recorded — [[decision-log#DEC-264]] — and one follow-up filed,
[[iterations/iteration-260-lazy-bm25-snapshot-load]]. No production code
changed in this iteration; the instrumentation used to get the numbers was
temporary and was reverted before commit. Full write-up with every measurement:
[[research/snapshot-load-floor-2026-09-01]].

### The floor reproduces, and it is bigger than its label

256's ~0.37 s holds exactly on current `main`: `find --limit 1 --index` against
MDN (14 399 files, 14 375 indexed, 121 614 239-byte index) runs in 0.36 s,
against < 0.01 s on an empty vault. But "snapshot load and deserialization" was
undercounting it — teardown adds another 59 ms at process exit that no
stopwatch around the command's output ever sees.

### Where the 360 ms goes

| stage | ms |
|---|---:|
| read 116 MiB, warm page cache (6.0 GiB/s) | 19 |
| read 116 MiB, cold via `F_NOCACHE` (1.9 GiB/s) | 59 |
| MessagePack decode, whole document | **240** |
| SEC-1 path validation | 2 |
| SEC-3 + MED-1 BM25 validation | 6–12 |
| entries re-sort + `path_index` | 1.3 |
| `rebuild_lower_index` | 0.7 |
| teardown (drop at exit) | 59 |

`xtask bench-scale` was not the right tool and was not used — it generates its
own synthetic vault and times whole commands, with no way to express
sub-command probes inside the load path on a specific real 116 MiB index. A
one-off instrumented release build was faster and more trustworthy.

### Every suspect in the plan was wrong

The plan named disk I/O, MessagePack decode, allocation shape, and post-decode
reconstruction. Three of the four are noise:

- **I/O** is 5 % warm, 17 % cold.
- **Allocation shape** in the entries: all 14 375 `IndexEntry` values decode
  from their own 19 MiB buffer in 41 ms.
- **Post-decode reconstruction** — re-sort, `path_index`, `rebuild_lower_index`
  — is 2 ms combined. DEC-259 already took the quadratic piece.

Decode is the cost, and the *shape* of the decode cost is the finding: the
on-disk split is entries 19 MiB / graph 8 MiB / **bm25_index 87 MiB (76 %)**,
and decoding the whole document while materializing *nothing at all*
(`IgnoredAny` everywhere) still costs 179 ms of the 240 ms. Three quarters of
the decode is serde token-walking BM25 postings. That kills the obvious fix —
marking `bm25_index` as `IgnoredAny` saves only 35 ms — and points at the real
one.

### The fix, measured rather than proposed

`rmp_serde::to_vec_named` writes a map with string keys and emits `bm25_index`
last, so a hand-written `Deserialize` that `break`s on that key skips all 87 MiB
without reading them, and `from_slice` accepts the unconsumed tail. Measured:
**240 ms → 61.5 ms**. With the skipped BM25 validation and the 43 ms of skipped
teardown, `find --limit 1 --index` projects from ~360 ms to ~150 ms — a ~2.4×
win on the most common indexed command, with the snapshot bytes unchanged and
every existing index still readable. The iteration's non-goal on wire
compatibility is untouched.

It is filed as 260 rather than landed here because the load-side change is
small and its blast radius is not: `write_snapshot` would silently drop the
BM25 section on any mutating command's save, the SEC-3/MED-1 "reject the whole
snapshot" contract does not compose with a section that fails mid-query, and
early-stop is load-bearing on a derive field order that no test pins. Three
decisions and a regression test — an iteration, not a patch.
