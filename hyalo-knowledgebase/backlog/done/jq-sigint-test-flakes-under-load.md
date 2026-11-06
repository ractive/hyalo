---
type: backlog
title: The jq SIGINT/SIGTERM cancellation test depended on how it was launched
date: 2026-11-06
status: completed
priority: low
origin: "iter-324 gate runs, 2026-11-05"
---

## Problem

`jq_worker_sigint_and_npm_sigterm_cancellation_preserve_append_effects` failed in local gate runs during iterations 324, 325 and 327 and passed when rerun, so it was filed as a flake under load. It was not load. A job started in the background inherits an ignored SIGINT, and DEC-369 then runs the `--jq` filter without its handler, so the SIGINT half of the test waited for the 3 s limit and "cancelled" never appeared on stderr. CI does not launch that way, which is why it never failed there.

## Evidence

Run through a launcher that sets SIGINT to ignored before exec, the test failed 8 of 8 times at `jq.rs:222`, each in about 3 s. A shell `trap '' INT` around a function did not reach the test binary and passed, so a probe has to confirm the signal state in the process itself.

## Fix

The test resets SIGINT to the default in the spawned child, since it is about the handler and must not depend on its launcher. Product behaviour is unchanged: an ignored SIGINT still runs the filter, as DEC-369 says. With the fix it passes 8 of 8 with SIGINT ignored and 8 of 8 without.
