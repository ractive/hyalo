---
type: backlog
title: The jq SIGINT/SIGTERM cancellation test flakes under load
date: 2026-11-06
status: planned
priority: low
origin: "iter-324 gate runs, 2026-11-05"
---

## Problem

`jq_worker_sigint_and_npm_sigterm_cancellation_preserve_append_effects` expects
"cancelled" on stderr after a fixed delay between spawning the worker and sending
SIGINT/SIGTERM. Under load the signal can arrive before the handler is installed, or
after the append already finished, so the message never appears and the test fails.

It failed in three loaded local gate runs during iteration 324, and passed when run alone
and on an otherwise idle machine. CI has not shown it.

## Plan

- [ ] Replace the fixed delay with a readiness signal (the handler installed, or the worker blocked on a known point) before the signal is sent
- [ ] Make the test independent of whether the append finished before the signal arrived, or hold the append open until the signal has been delivered
- [ ] Run the test in a loop under CPU load to confirm it no longer flakes
