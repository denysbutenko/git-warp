# Cleanup Candidate Trust Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make cleanup candidates trustworthy and consistent across CLI and TUI flows.

**Architecture:** Resolve cleanup base branch inside `GitRepository`, use it during branch analysis, and pass filtered `BranchStatus` values into the cleanup TUI. Keep reason rendering as small helper functions so CLI and TUI wording stays aligned.

**Tech Stack:** Rust, clap CLI, ratatui TUI, git CLI integration, cargo tests.

---

### Task 1: Default-Branch Cleanup Analysis

**Files:**
- Modify: `src/git.rs`
- Modify: `tests/unit/git_tests.rs`

- [x] Add tests for a repo initialized with `trunk` where a merged feature worktree is detected against `trunk`.
- [x] Add `cleanup_base_branch()` to prefer `origin/HEAD`, then primary worktree branch, then configured default.
- [x] Use that base for `is_merged` and `is_identical`.

### Task 2: Shared Candidate Filtering

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/tui.rs`
- Modify: `tests/unit/tui_tests.rs`

- [x] Add helper text for cleanup row reasons.
- [x] Filter candidates once in CLI before optional interactive mode.
- [x] Make `CleanupTui` accept the filtered candidates so interactive and non-interactive flows share the same candidate set.

### Task 3: Verification and Publish

**Files:**
- Modify: implementation and test files above
- Create: spec and plan docs for issue #4

- [x] Run targeted failing tests before implementation.
- [x] Run targeted passing tests after implementation.
- [x] Run formatting and whitespace checks.
- [x] Commit, push, and open a ready PR closing issue #4.
