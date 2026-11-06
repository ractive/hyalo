---
type: backlog
title: The jq SIGINT/SIGTERM cancellation test depended on how it was launched
date: 2026-11-06
status: completed
priority: low
origin: "iter-324 gate runs, 2026-11-05"
---

## Problem

`jq_worker_sigint_and_npm_sigterm_cancellation_preserve_append_effects` failed in local gate runs during iterations 324, 325 and 327 and passed when rerun, so it was filed as a flake under load. It depended on how it was launched. `ctrlc` installs nothing unless SIGINT, SIGTERM and SIGHUP are all at their default (DEC-401), and a background job of a non-interactive shell inherits an ignored SIGINT, so the handler this test exercises never ran and "cancelled" never reached stderr. CI does not launch that way.

## Evidence

On commit 1475418e, with the load average near 3, the unchanged test passed 3 of 3 in the foreground and failed 4 of 4 as `cargo test` in the background of a non-interactive `sh` or `zsh`, with SIGINT confirmed ignored at the level that started cargo. All failed the `contains("cancelled")` assertion. So a background launch alone causes this failure, through cargo, without load.

What is not established: iteration 327's failure was in a run launched with `(… &)`, but the launch state of the runs in iterations 324 and 325 was not recorded and no failing stderr was captured, so their cause is inferred from the matching signature, not observed. A late signal gives the same signature.

## Fix

The test resets all three signals in the child it spawns, so it no longer depends on its launcher, including `nohup`. Product behaviour is unchanged (DEC-369, DEC-401). It passes in every launch state tried, listed in the iteration note.

## Not fixed

The test still sends its signal 100 ms after it sees the commit, and that signal must land inside the 3 s filter limit. A run starved for that long can still fail the same assertion. If it does, capture stderr and bound the timing.
