---
type: iteration
title: "Iteration 333: the signal test and the TMPDIR copy race"
date: 2026-11-08
status: completed
tags: [iteration, write-path, signals]
branch: iter-333/signal-test-tmpdir-race
---

# Iteration 333: the signal test and the TMPDIR copy race

`a_signal_mid_phase_cleans_up_and_dies_by_that_signal` failed once on Linux CI: one note copy was left in `$TMPDIR` after SIGTERM (DEC-409). Read from the code, not reproduced: the signal hooks were released whenever the registry was empty, so a signal arriving while another worker was between creating its copy and registering it took the default action and killed the process with the copy on disk.

## Tasks

- [x] `interrupt::flight()` hooks the signals before the file is created and keeps them hooked until it is registered; the hooks are released only when nothing is registered and nothing is in flight
- [x] The signal test keeps the vault guarantees as they were (no `.hyalo-tmp-*` file, every note whole) and allows at most one `$TMPDIR` copy per worker thread; DEC-409 says `$TMPDIR` cleanup is best effort
- [x] Signals module run 5 times locally, fmt, clippy, `cargo test --workspace -q`
