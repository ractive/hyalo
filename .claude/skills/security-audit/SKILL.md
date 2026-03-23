---
name: security-audit
description: >
  REQUIRED skill for any security-related request. Use this skill whenever the user wants to find
  anything dangerous, sensitive, or risky in their code, files, or repository. This includes but
  is not limited to: scanning for secrets/keys/tokens/credentials, checking dependencies for
  vulnerabilities, auditing destructive commands for missing safeguards, reviewing files for PII
  or internal data before open-sourcing, checking .env files or git history for leaked credentials,
  or any request where the concern is "is this safe/secure/exposed?" This skill provides a
  structured audit methodology and checklist you MUST follow — do not attempt security reviews
  without it. Skip this skill ONLY for pure feature work, refactoring, bug fixes, or non-security
  code review.
---

# Security Audit

You are performing a security audit of a Rust codebase. Your goal is to find real, actionable security issues — not to generate a long list of theoretical concerns. Prioritize findings by actual risk: a leaked API key in a fixture is more urgent than a theoretical timing attack on a string comparison.

## Audit Structure

Produce a report with these sections. For each section, only include findings — skip "PASS" items unless they are genuinely surprising or counterintuitive. A lean report focused on what needs fixing is more useful than one padded with confirmations of good practices. However, always check and report on dependencies (section 2) even if they look clean — dependency vulnerabilities are silent and a "no issues found" confirmation is valuable there.

### 1. Secrets & Sensitive Data Scan

This is often the highest-impact finding category because leaked secrets can be exploited immediately, unlike code vulnerabilities that require an attack vector. Scan systematically — open and read every non-binary file, don't just check whether directories are gitignored.

**Where to look (read the actual files, don't just check if they exist):**
- Test fixtures (JSON, YAML, TOML in `tests/` or `fixtures/` dirs) — open each one and check for real-looking values
- Documentation and knowledgebase files — read every `.md` file in any `docs/`, `knowledgebase/`, or similar directory. Look for infrastructure details, account references, "production" mentions, internal project names, and internal URLs. These files are easily overlooked but often contain the most revealing information about the real infrastructure behind the project
- Agent memory and config (`.claude/` directory tree) — check committed agent definitions, skill files, and memory indexes for private paths or personal details
- Config files (`.env`, `.env.example`, `settings.json`, `settings.local.json`)
- OpenAPI specs or API reference files — these often contain example data copied from real responses
- Cargo.toml metadata (author emails, repository URLs)
- CI/CD configs

**Git history scan — this catches things the file tree misses:**
- Run `git log --format='%ae' | sort -u` to check author emails — corporate or personal emails will be exposed when open-sourced
- Run `git log --all --oneline --diff-filter=D -- '*.env' '*.key' '*.pem' '*.json'` to check if sensitive files were ever committed and then deleted (they're still in history)
- Check if any commit messages reference ticket numbers, internal systems, or real account details

**What to look for:**
- API keys, tokens, passwords (even "test" values — `redacted-*` is fine, but `sk-live-*` or realistic UUIDs are not)
- Real account IDs, user IDs, or organization IDs (vs obvious dummy values like `00000000-...`)
- Email addresses, usernames, or personal information
- Internal hostnames, IP addresses, or infrastructure details
- URLs with embedded credentials or tokens
- Private file paths (like `/Users/username/...`) that reveal system layout
- References to "production", "staging", or internal environments that reveal infrastructure context

**Severity guide:**
- **CRITICAL**: Live credentials, real API keys, or PII that could enable account access
- **HIGH**: Real-looking IDs or infrastructure details that could aid reconnaissance
- **MEDIUM**: Personal info (emails in git history, local paths) that would be exposed on open-source
- **LOW**: Placeholder values that technically should be more obviously fake

### 2. Rust Code Security

Focus on patterns that create real vulnerabilities in this specific codebase, not generic Rust safety advice.

**Input validation & injection:**
- CLI arguments passed to shell commands, URLs, or file paths without sanitization
- Path traversal: can user-supplied `--path`, `--file`, `--remote_path` etc. escape intended directories? (e.g., `../../etc/passwd`)
- URL construction: are user inputs interpolated into URLs without encoding? Pay special attention to hostname construction — if a user-supplied value becomes part of a domain name, an attacker can redirect requests (and credentials) to their own server
- Does the CLI accept `--format json` or similar that could be used for output injection?

**Authentication & credential handling:**
- Are API keys logged, included in error messages, or written to disk?
- Could debug output (`--debug` flag) leak credentials in HTTP headers?
- Are credentials held in memory longer than necessary?
- Is there any credential caching that could leave secrets on disk?

**Unsafe code:**
- Any `unsafe` blocks — are they actually necessary? Is the safety invariant documented?

**Error handling:**
- Do error messages leak internal paths, server responses, or credential fragments?
- Are panics possible from user input (unwrap on user-controlled data)?

**Dependency concerns:**
- Check `Cargo.toml` and `Cargo.lock` for known-vulnerable versions
- Are there dependencies with overly broad feature flags?

### 3. CLI Misuse Vectors

Think like an attacker who has access to this CLI tool. What could they do that the developer didn't intend?

**Destructive operations:**
- Are delete operations (purge, rm, delete) protected by confirmation prompts?
- Can `--yes` / `-y` flag be used to bypass all safety checks in scripts?
- Is there rate limiting or batch-size limits on destructive operations?
- Does the delete confirmation distinguish between files and directories (recursive delete)?

**Local file system risks:**
- Can download commands write to arbitrary paths? (overwriting ~/.ssh/authorized_keys, etc.)
- Can upload commands read arbitrary local files? (exfiltrating /etc/passwd to cloud storage)
- Are there symlink-following risks?

**Privilege escalation:**
- Could a low-privilege API key be used to discover or access resources it shouldn't?

**Supply chain:**
- Could malicious shell completions be generated?
- Are there code paths where external data (API responses) influences local file operations?

### 4. Configuration & Infrastructure

- Is `.env` properly gitignored?
- Are there any config files that might be committed with real values?
- Does the project have appropriate `.gitignore` coverage?

## Report Format

Present findings as a prioritized list. For each finding:

```
### [CRITICAL|HIGH|MEDIUM|LOW] Brief title

**File:** `path/to/file.rs:42`
**Issue:** What's wrong, concretely
**Risk:** What an attacker could do with this
**Fix:** Specific remediation (one line if possible)
```

End the report with exactly this format:
**X findings: N critical, N high, N medium, N low**

Follow with a 2-3 sentence assessment of the overall security posture.

## What NOT to do

- Don't list items that pass inspection — only report actual findings
- Don't flag theoretical issues in safe Rust code that the type system already prevents
- Don't recommend adding dependencies (like secret-scanning tools) unless there's a specific gap
- Don't rewrite code — just identify the issues and suggest fixes
- Don't pad the report with generic security advice (e.g., "consider using HTTPS" when the code already does)
- Don't flag things as issues when they're intentional design choices (e.g., reading API key from env var is standard practice, not a vulnerability)
