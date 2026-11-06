---
title: "Iteration 328: the jq signal test owns its setup"
type: iteration
date: 2026-11-06
tags: [iteration, tests]
status: completed
branch: iter-328/jq-signal-test-owns-its-setup
priority: 3
related: ["[[backlog/done/jq-sigint-test-flakes-under-load]]"]
---

# The jq signal test owns its setup

## Problem

A test filed as flaky under load depended on how it was launched: a background job of a non-interactive shell inherits an ignored SIGINT, and `ctrlc` then installs no handler, so the one the test exercises never ran. The backlog item has the evidence and what stays unproven.

## Tasks

- [x] Reproduce the failure with SIGINT ignored (8 of 8 failed) and as a real background `cargo test` (4 of 4 failed, 3 of 3 passed in the foreground)
- [x] Reset SIGINT, SIGTERM and SIGHUP in the spawned child
- [x] Confirm the test passes with each of the three ignored, with all three ignored, with none, and as a background `cargo test`
- [x] Correct DEC-369's account of `ctrlc` and `nohup` (DEC-401)
- [x] Close the backlog item, stating what was shown and what was not
