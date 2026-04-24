# Switch Outcome Reporting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `warp switch` report creation, checkout, terminal handoff, and manual fallback as verified outcomes.

**Architecture:** Keep the change in the switch command path. Add small local outcome structs/helpers in `src/cli.rs`, have `handle_switch` collect step states, and print a compact success or incomplete summary after terminal handoff.

**Tech Stack:** Rust CLI, existing `std::process::Command` Git calls, current integration-test harness with fake executables and temp Git repositories.

---

### Task 1: Regression Tests

**Files:**
- Modify: `tests/integration/terminal_switch_tests.rs`

- [ ] **Step 1: Add fake failing terminal support**

Add a helper that creates an `open` command exiting non-zero so `--terminal tab` plus `terminal.app = "warp"` exercises terminal handoff failure without depending on the local desktop.

- [ ] **Step 2: Add terminal failure regression test**

Create a temp repo and config, run `warp switch --no-cow feature/handoff-fails`, and assert stdout contains:

```text
✅ Worktree creation: created
✅ Branch checkout: feature/handoff-fails
⚠️  Terminal handoff: failed
⚠️  Switch incomplete
💡 Run: cd '
```

- [ ] **Step 3: Add existing worktree branch mismatch regression test**

Create an existing worktree for `feature/existing`, then run `warp --terminal echo switch --no-cow feature/wrong --path <existing-path>`. Assert stdout contains:

```text
↪️  Worktree creation: already existed
⚠️  Branch checkout: expected feature/wrong
⚠️  Switch incomplete
💡 Run: cd '
```

- [ ] **Step 4: Verify tests fail**

Run:

```bash
cargo test --test mod terminal_switch_tests -- --nocapture
```

Expected: the new assertions fail because current output does not include the verified step summary.

### Task 2: Outcome Reporter

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add outcome types**

Add local `SwitchStepStatus`, `SwitchStep`, and `SwitchOutcomeReport` structs near the switch helpers. They need methods for `done`, `skipped`, `warned`, `has_warnings`, and `print`.

- [ ] **Step 2: Track creation and checkout**

In `handle_switch`, replace the early success print with outcome recording. Verify the worktree path exists after creation. For existing paths, verify the branch with:

```bash
git -C <worktree> branch --show-current
```

Traditional creation records checkout as done after `create_worktree_and_branch` succeeds. CoW checkout records warning on non-zero exit.

- [ ] **Step 3: Track terminal handoff and fallback**

Change `switch_to_worktree_path` to return a terminal handoff outcome instead of printing final success/fallback directly. On error, include the terminal error as warning detail.

- [ ] **Step 4: Print final summary**

After terminal handoff, append the handoff step to the report and print:

```text
✅ Switch complete: <path>
```

only when all outcomes are successful or skipped. Otherwise print each step plus:

```text
⚠️  Switch incomplete: <path>
💡 Run: cd '<path>'
```

### Task 3: Verification And Publish

**Files:**
- Modify: `src/cli.rs`
- Modify: `tests/integration/terminal_switch_tests.rs`
- Add: `docs/superpowers/specs/2026-04-24-switch-outcomes-design.md`
- Add: `docs/superpowers/plans/2026-04-24-switch-outcomes.md`

- [ ] **Step 1: Format**

Run:

```bash
cargo fmt
```

- [ ] **Step 2: Check whitespace**

Run:

```bash
git diff --check
```

- [ ] **Step 3: Run focused tests**

Run:

```bash
cargo test --test mod terminal_switch_tests -- --nocapture
cargo test --test mod switch_post_create_tests -- --nocapture
```

- [ ] **Step 4: Commit, push, and open PR**

Inspect `git status -sb`, stage only this issue's files, commit with a terse message, push `5-p1-report-switch-create`, and open a non-draft PR against `main`.
