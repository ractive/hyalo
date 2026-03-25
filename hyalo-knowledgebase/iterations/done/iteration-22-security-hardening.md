---
branch: iter-22/security-hardening
date: 2026-03-23
status: completed
tags:
- security
- hardening
title: Security Hardening
type: iteration
---

# Iteration 22: Security Hardening

Pre-release security audit found one confirmed exploit and several defense-in-depth improvements.

## Tasks

- [ ] P0: Fix symlink escape in resolve_file and resolve_target (confirmed exploit: write outside vault via --file through symlink)
- [ ] P2: Atomic file writes via tempfile+persist (crash-safe mutations)
- [ ] P2: Add regex size limit in content search (defense-in-depth)
- [ ] P1: Add cargo-audit and cargo-deny to release workflow
- [ ] P3: Document serde_yaml_ng / unsafe-libyaml awareness
