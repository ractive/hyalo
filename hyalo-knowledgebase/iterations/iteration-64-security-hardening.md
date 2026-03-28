---
title: Security hardening for open-source release
type: iteration
date: 2026-03-28
tags:
  - iteration
  - security
status: completed
branch: iter-64/security-hardening
---

# Security hardening for open-source release

Addresses findings from the security audit (2026-03-28). Hardens vault boundary enforcement on CLI commands and file discovery to close the remaining attack surface before going public.

## Vault boundary checks on index commands

`create-index --output` and `drop-index --path` accept arbitrary filesystem paths with no vault boundary validation. In automated pipelines where these args could come from untrusted input, an attacker could write or delete arbitrary files.

- [x] Add `ensure_within_vault` check to `create-index --output` resolved path
- [x] Add `ensure_within_vault` check to `drop-index --path` resolved path
- [x] Add `--allow-outside-vault` escape hatch for intentional use outside the vault
- [x] Add e2e tests: reject paths outside vault (both commands)
- [x] Add e2e tests: `--allow-outside-vault` allows paths outside vault
- [x] Add e2e tests: normal in-vault paths still work

## Symlink boundary check in discover_files

`discover_files` (used by `--glob` and bulk scan) collects symlinked `.md` files without checking if their target is inside the vault. An attacker with write access to the vault dir could plant `evil.md -> /etc/shadow` and cause hyalo to read arbitrary files.

- [x] After `discover_files` collects paths, canonicalize and check against vault boundary
- [x] Skip files that resolve outside the vault (with a warning)
- [x] Add e2e test: symlinked `.md` file pointing outside vault is skipped
- [x] Add e2e test: symlinked `.md` file pointing inside vault is included

## Harden unsafe blocks (maintenance safety)

- [x] Add top-of-function comment in `strip_inline_code` noting single-byte-ASCII constraint for the `unsafe` block
- [x] Add top-of-function comment in `strip_inline_comments` noting single-byte-ASCII constraint
- [x] Replace `.expect("already validated")` with `?` in `append.rs`, `set.rs`, `remove.rs`

## Verify

- [x] `cargo fmt`
- [x] `cargo clippy --workspace --all-targets -- -D warnings`
- [x] `cargo test --workspace`
