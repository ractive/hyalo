---
title: Iteration 159 — Add pi integration to hyalo init command
type: iteration
date: 2026-07-04
tags:
- iteration
- cli
- integration
- pi
status: completed
branch: pi-integration
---

## Goal
Add `--pi` flag to `hyalo init` command to install pi skill artifacts, mirroring the existing `--claude` flag functionality.

## Tasks

- [x] Update CLI arguments in `crates/hyalo-cli/src/cli/args.rs`:
  - Add `pi: bool` field to `Init` struct
  - Update command documentation to include pi integration examples
  - Update `Deinit` command documentation to mention pi artifacts

- [x] Update command execution in `crates/hyalo-cli/src/run.rs`:
  - Extract `pi` flag in the `Commands::Init` match
  - Pass `pi` parameter to `init_commands::run_init()`

- [x] Update core implementation in `crates/hyalo-cli/src/commands/init.rs`:
  - Add `pi: bool` parameter to `run_init()` and `run_init_in()`
  - Add embedded constants for pi templates (skill files, extension, package.json)
  - Implement conditional pi installation logic (independent of claude installation)
  - Add pi artifact removal to `run_deinit_in()`

- [x] Create template files in `crates/hyalo-cli/templates/`:
  - `skill-hyalo-pi.md` (hyalo skill for pi)
  - `skill-hyalo-tidy-pi.md` (tidy skill for pi)
  - `extension-hyalo.ts` (TypeScript extension for pi)
  - `package.json` (pi package configuration)

- [x] Update test calls to `run_init_in()` with new `pi` parameter

- [x] Run quality gates:
  - [x] `cargo fmt`
  - [x] `cargo clippy --workspace --all-targets -- -D warnings`
  - [x] `cargo test --workspace -q`

- [x] Create branch `pi-integration` and commit changes
  - [x] Write descriptive commit message
  - [x] Push to remote

- [x] Create PR on GitHub
  - [x] Review diff for any issues
  - [x] Ensure all tests pass

- [ ] Dogfood: Migrate personal pi skills from `~/.claude/skills/` to `~/.pi/skills/`:
  - [x] Create `~/.pi/skills/create-pr/SKILL.md`
  - [x] Create `~/.pi/skills/merge-pr/SKILL.md`
  - [x] Create `~/.pi/skills/review-pr/SKILL.md`

## Changes Made

### 1. **Command-line Arguments (`crates/hyalo-cli/src/cli/args.rs`)**
- Added `pi: bool` field to the `Init` struct with proper documentation
- Updated command help text to include pi integration examples
- Updated `Deinit` command description to mention pi artifact removal

### 2. **Command Execution (`crates/hyalo-cli/src/run.rs`)**
- Modified the `Init` pattern match to extract both `claude` and `pi` flags
- Updated call to `init_commands::run_init()` to pass the `pi` parameter

### 3. **Core Implementation (`crates/hyalo-cli/src/commands/init.rs`)**
#### Function Signatures
- Updated `run_init()` to accept `pi: bool` parameter
- Updated `run_init_in()` to accept `pi: bool` parameter alongside existing `claude` parameter

#### PI Integration Logic
- Added conditional pi installation steps that run independently of claude installation
- PI installation creates:
  - `.pi/skills/hyalo/SKILL.md` (hyalo skill for pi)
  - `.pi/skills/hyalo-tidy/SKILL.md` (tidy skill for pi)
  - `.pi/extensions/hyalo.ts` (TypeScript extension for pi)
  - `.pi/package.json` (pi package configuration)

#### Deinit Updates
- Enhanced `run_deinit_in()` to remove pi artifacts:
  - `.pi/skills/hyalo/SKILL.md`
  - `.pi/skills/hyalo-tidy/SKILL.md`
  - `.pi/extensions/hyalo.ts`
  - `.pi/package.json`
- Properly cleans up empty `.pi/` directory structure

#### Template Constants
- Added embedded template constants for pi artifacts:
  - `PI_SKILL_CONTENT`
  - `PI_TIDY_SKILL_CONTENT`
  - `PI_EXTENSION_CONTENT`
  - `PI_PACKAGE_JSON_CONTENT`

### 4. **Template Files (`crates/hyalo-cli/templates/`)**
Created four new template files:
- **`skill-hyalo-pi.md`**: Direct copy from hoppy project's pi skill
- **`skill-hyalo-tidy-pi.md`**: Modified from hoppy project (replaced "hoppy-knowledgebase" with "hyalo-knowledgebase" sentinel)
- **`extension-hyalo.ts`**: TypeScript extension enabling pi tool integration
- **`package.json`**: Configuration for pi package registration

### 5. **Test Updates**
Updated all test calls to `run_init_in()` to include the new `pi` parameter (set to `false` for existing tests), ensuring test suite passes.

## Key Design Decisions

1. **Independent Installation**: Claude and pi integrations are independent - users can install one, both, or neither
2. **Idempotent Operations**: Both install and deinit are idempotent (safe to run multiple times)
3. **Consistent Patterns**: Followed the exact same patterns as the existing claude integration
4. **Template Parameterization**: Uses the same `hyalo-knowledgebase` sentinel replacement as claude templates
5. **Clean Removal**: Deinit properly removes all pi artifacts and cleans up empty directories

## Verification Steps Completed

1. ✅ All tests pass (`cargo test --package hyalo-cli`)
2. ✅ Code formatting passes (`cargo fmt`)
3. ✅ Linting passes (`cargo clippy --workspace --all-targets -- -D warnings`)
4. ✅ Builds successfully (`cargo build --release`)

## Usage Examples

```bash
# Install only pi integration
hyalo init --pi

# Install both claude and pi integrations
hyalo init --claude --pi

# Remove all integrations (both claude and pi)
hyalo deinit
```