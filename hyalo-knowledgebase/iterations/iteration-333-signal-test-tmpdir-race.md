---
type: iteration
title: "Iteration 333: the signal test and the TMPDIR copy race"
date: 2026-11-08
status: completed
tags: [iteration, write-path, signals]
branch: iter-333/signal-test-tmpdir-race
---

# Iteration 333: the signal test and the TMPDIR copy race

`a_signal_mid_phase_cleans_up_and_dies_by_that_signal` failed once on Linux CI with a note copy left in `$TMPDIR` after SIGTERM (DEC-409), with the "interrupted" message printed, so the handler ran. Cause: a receipt (`OwnedPublication`) kept the prepared or captured copy alive but dropped its `Scratch`, so the copy was no longer registered and the handler could not remove it. A second, smaller gap was closed as well: the signals were released while another worker was creating its copy.

## Tasks

- [x] `OwnedPublication` owns the `Scratch` of its copy, in `commit_with_receipt` and `move_no_replace_with_receipt`; `a_receipt_keeps_its_tmpdir_copy_registered` fails 10 of 10 without either
- [x] `flight()` raises the in-flight count before it hooks; the hooks are released only when idle (`is_idle`, pinned by a unit test)
- [x] The signal test asserts 0 copies in `$TMPDIR` again; DEC-409 says the cleanup is best effort and what the failure was
- [x] Signals module run 5 times, fmt, clippy, `cargo test --workspace -q`
