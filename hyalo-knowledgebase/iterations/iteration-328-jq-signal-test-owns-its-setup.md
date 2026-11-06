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

A test filed as flaky under load really depended on how its gate run was launched: a background job inherits an ignored SIGINT, and DEC-369 then runs the filter without the handler the test exercises. The backlog item has the evidence.

## Tasks

- [x] Reproduce the failure with SIGINT ignored (8 of 8 failed)
- [x] Reset SIGINT in the spawned child
- [x] Confirm 8 of 8 pass with SIGINT ignored and 8 of 8 without
- [x] Close the backlog item with the proven cause
