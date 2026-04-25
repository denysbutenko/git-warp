# Runtime Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make terminal launch config values produce visible `warp switch` behavior.

**Architecture:** Thread terminal launch options from loaded config through the existing CLI handoff into terminal backends. Keep mode precedence unchanged: CLI `--terminal` chooses the mode, config supplies app, auto-activation, and init commands.

**Tech Stack:** Rust, clap CLI, integration tests under `tests/integration/terminal_switch_tests.rs`.

---

### Task 1: Prove Missing Init Command Behavior

**Files:**
- Modify: `tests/integration/terminal_switch_tests.rs`

- [x] **Step 1: Add a config helper that can write terminal mode, activation, and init commands**

```rust
fn write_config_with_terminal_options(
    home_dir: &Path,
    app: &str,
    terminal_mode: &str,
    auto_activate: bool,
    init_commands: &[&str],
)
```

- [x] **Step 2: Add a failing integration test**

```rust
#[test]
fn test_warp_switch_echo_includes_configured_init_commands()
```

- [x] **Step 3: Run the focused red check**

Run: `cargo test --test mod terminal_switch_tests::test_warp_switch_echo_includes_configured_init_commands -- --nocapture`

Expected before implementation: FAIL because stdout has `cd ...` but not `corepack enable` or `pnpm install`.

### Task 2: Thread Terminal Launch Options

**Files:**
- Modify: `src/terminal.rs`
- Modify: `src/cli.rs`

- [x] **Step 1: Add a `TerminalLaunchOptions` type**

```rust
#[derive(Debug, Clone)]
pub struct TerminalLaunchOptions {
    pub auto_activate: bool,
    pub init_commands: Vec<String>,
}
```

- [x] **Step 2: Add command construction helpers**

Generate one shell command per line: `cd '<path>'`, followed by configured init commands.

- [x] **Step 3: Pass options from CLI config into terminal handoff**

`record_terminal_handoff` should pass `config.terminal.auto_activate` and `config.terminal.init_commands.clone()`.

### Task 3: Apply Options in Terminal Backends

**Files:**
- Modify: `src/terminal.rs`

- [x] **Step 1: Update echo and inplace modes**

Print `cd '<worktree>'` and then each init command.

- [x] **Step 2: Update iTerm2 and Terminal.app AppleScript**

When `auto_activate` is true, include `activate`; then write the `cd` command and each init command.

- [x] **Step 3: Keep current mode shell handoff compatible**

For `current`, start the shell in the worktree as before. If init commands are configured, run them in a shell command before dropping into the interactive shell only when that can be done without breaking existing fake-shell tests.

### Task 4: Align Config Display and Docs

**Files:**
- Modify: `src/cli.rs`
- Modify: `README.md`
- Modify: `docs/user-guide.md`

- [x] **Step 1: Print init commands in `warp config --show`**

Display `Init commands: []` when empty, or the configured list when present.

- [x] **Step 2: Document terminal launch behavior**

README and user guide should state that init commands run after switching into the worktree and `auto_activate` affects macOS terminal activation.

### Task 5: Verify and Publish

**Files:**
- All changed files

- [x] **Step 1: Run focused tests**

Run:

```bash
cargo test --test mod terminal_switch_tests -- --nocapture
cargo test --test mod config_tests -- --nocapture
```

- [x] **Step 2: Run formatting and whitespace checks**

Run:

```bash
cargo fmt --check
git diff --check
```

- [ ] **Step 3: Commit, push, and open a ready PR**

Use a concise PR body with summary and only the verification commands that actually ran.
