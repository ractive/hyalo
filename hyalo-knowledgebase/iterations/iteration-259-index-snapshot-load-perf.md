---
type: iteration
title: Iteration 259 — snapshot-index load/deserialize floor
date: 2026-08-31
status: planned
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

### PERF-1: characterize the snapshot-load floor [0/1]

- [ ] Reproduce 256's measurement independently: build (or reuse, if still
      available) a large synthetic or real vault (MDN-scale, ~14k files /
      ~120 MB index) and profile `hyalo find --limit 1` end to end,
      isolating snapshot read + deserialize from everything downstream.
      Confirm the ~0.37 s figure still holds against current `main` (perf
      characteristics may have shifted since 256). Use `xtask bench-scale`
      if it fits this vault size, or a dedicated one-off harness if not —
      note which, and why, in the outcome.
- [ ] Break the floor down: how much is reading bytes off disk (cold vs warm
      cache), how much is MessagePack decode, how much is post-decode
      reconstruction (e.g., rebuilding in-memory indexes/maps from the
      flat snapshot). Use a profiler or manual instrumentation — whichever
      gives trustworthy numbers fastest.
- [ ] Decide, with evidence, whether there is a reducible cost here at all
      (e.g., streaming decode instead of whole-buffer deserialize, avoiding
      an intermediate allocation, lazy-loading index sections that most
      `find` invocations don't touch) or whether the floor is inherent to
      "read N megabytes and materialize M structures" and not worth chasing
      further. Either finding is a valid outcome — this task is
      "characterize and decide", not "must ship a speedup".

## Acceptance criteria

- [ ] PERF-1 produces a written, numbers-backed characterization of the
      snapshot-load floor (where the time goes, on what vault size), checked
      into this file's Outcome section or a `research/` note it links to.
- [ ] Either a concrete, scoped follow-up fix is identified (and filed as its
      own iteration if it's more than a small patch) or the floor is
      recorded as inherent/not-worth-chasing with a DEC explaining why —
      not left as an open question.
- [ ] If any code changes are made in this iteration, gates green: `cargo
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
