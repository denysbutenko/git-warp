# Recovery Guidance Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add concise next-step recovery hints to common Git-Warp failure paths.

**Architecture:** Keep hints in `src/cli.rs` near the existing command flows. Add a small helper for "not in repo" errors, extend switch and cleanup warning text, and reuse `warp doctor` for setup-oriented hooks and CoW guidance.

**Tech Stack:** Rust CLI, `anyhow`, existing integration tests under `tests/integration`.

---

### Task 1: Failing Regression Tests

**Files:**
- Modify: `tests/integration/cli_surface_tests.rs`
- Modify: `tests/integration/terminal_switch_tests.rs`

- [ ] **Step 1: Assert repo-bound commands explain where to run**

In `test_switch_rejects_multiple_target_selectors`'s area, add a new test that runs:

```bash
warp switch feature/demo
```

from a temp directory that is not a Git repo. Expected stderr contains:

```text
Not in a Git repository
Run this command inside a Git repository
```

- [ ] **Step 2: Assert doctor gives setup recovery commands**

Extend `test_doctor_outside_repo_prints_recovery_guidance` so stdout also contains:

```text
warp hooks-install --level user --runtime all
warp switch --no-cow <branch>
```

- [ ] **Step 3: Assert terminal handoff gives a manual-terminal fallback**

Extend `test_warp_switch_reports_terminal_handoff_failure_as_incomplete` so stdout contains:

```text
Retry with `--terminal echo`
```

- [ ] **Step 4: Assert branch/path mismatch gives inspection guidance**

Extend `test_warp_switch_reports_existing_worktree_branch_mismatch_as_incomplete` so stdout contains:

```text
Use a different --path or run `warp ls`
```

- [ ] **Step 5: Verify RED**

Run:

```bash
cargo test --test mod cli_surface_tests::test_switch_outside_repo_prints_recovery_guidance -- --nocapture
cargo test --test mod cli_surface_tests::test_doctor_outside_repo_prints_recovery_guidance -- --nocapture
cargo test --test mod terminal_switch_tests::test_warp_switch_reports_terminal_handoff_failure_as_incomplete -- --nocapture
cargo test --test mod terminal_switch_tests::test_warp_switch_reports_existing_worktree_branch_mismatch_as_incomplete -- --nocapture
```

Expected: the new assertions fail before implementation.

### Task 2: Recovery Hint Implementation

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add not-in-repo helper**

Add:

```rust
fn not_in_git_repo_error() -> anyhow::Error {
    anyhow::anyhow!(
        "Not in a Git repository. Run this command inside a Git repository, or use `cd <repo>` first."
    )
}
```

Use it where `GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a Git repository"))?` appears.

- [ ] **Step 2: Add switch warning hints**

When branch checkout verification warns because an existing path is on a different branch, append:

```text
. Use a different --path or run `warp ls` to inspect worktrees.
```

When terminal handoff fails, append:

```text
. Retry with `--terminal echo` to print manual commands instead.
```

- [ ] **Step 3: Add cleanup process hint**

When cleanup finds processes and skips a worktree, print:

```text
💡 Run `warp cleanup --mode <mode> --kill` to terminate them, use `--force` to ignore them, or stop the process manually.
```

- [ ] **Step 4: Add doctor CoW fallback hint**

When doctor reports CoW unavailable, add a next step:

```text
Run `warp switch --no-cow <branch>` to skip CoW checks for a switch.
```

### Task 3: Verification And Publish

**Files:**
- Verify changed tests and formatting.

- [ ] **Step 1: Run focused tests**

```bash
cargo test --test mod cli_surface_tests -- --nocapture
cargo test --test mod terminal_switch_tests -- --nocapture
```

- [ ] **Step 2: Run formatting and whitespace checks**

```bash
cargo fmt --check
git diff --check
```

- [ ] **Step 3: Commit and open PR**

```bash
git add src/cli.rs tests/integration/cli_surface_tests.rs tests/integration/terminal_switch_tests.rs docs/superpowers/specs/2026-04-26-recovery-guidance-design.md docs/superpowers/plans/2026-04-26-recovery-guidance.md
git commit -m "Add actionable recovery guidance"
git push -u origin 8-recovery-guidance
gh pr create --repo denysbutenko/git-warp --base main --head 8-recovery-guidance --title "Add actionable recovery guidance" --body-file /tmp/git-warp-pr-body.md
```
