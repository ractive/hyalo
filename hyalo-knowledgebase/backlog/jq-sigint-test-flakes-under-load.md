---
type: backlog
title: The jq SIGINT/SIGTERM cancellation test flakes under load
date: 2026-11-06
status: planned
priority: low
origin: "iter-324 gate runs, 2026-11-05"
---

## Problem

`jq_worker_sigint_and_npm_sigterm_cancellation_preserve_append_effects` failed in loaded local
gate runs during iterations 324 and 325 ("cancelled" missing from stderr) and passed when run
alone. The failing stderr was not captured, so the cause is not established.

Two leads, from reading the code and from a reviewer's run on `target/release/hyalo`:

- "cancelled" went missing when the signal arrived after the 3 s limit.
- "cancelled" went missing when SIGINT was inherited as ignored (exit 2, SIGXCPU cause). A
  background or `nohup` gate run inherits an ignored SIGINT, and DEC-369 runs the filter without
  its handler in that case, so the trigger may be how the gates were launched rather than load.

Both are untested against the real failures.

## Plan

- [ ] Capture the failing stderr first
- [ ] Check whether the failures came from runs with SIGINT ignored
- [ ] Make the test reset an inherited ignored SIGINT and keep the signal inside the 3 s limit, as the evidence supports
