# TUI Ergonomics Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the agents and cleanup TUIs easier to scan and operate on first use.

**Architecture:** Keep the existing `src/tui.rs` architecture, but move user-visible semantics into small model helpers that can be unit tested without terminal snapshots. Rendering will consume those model fields and keyboard handling will add expected aliases without changing existing commands.

**Tech Stack:** Rust, ratatui, crossterm, existing Cargo unit tests.

---

### Task 1: Agents Dashboard Semantics

**Files:**
- Modify: `src/tui.rs`
- Modify: `tests/unit/tui_tests.rs`

- [ ] **Step 1: Write the failing tests**

Add assertions that `build_dashboard_model` exposes `state_label`, that empty-state copy is action-oriented, and that `session_detail_lines` includes a plain `State:` line.

Run: `cargo test --test mod test_build_dashboard_model_empty_state test_session_detail_lines_include_expected_fields -- --nocapture`

Expected: FAIL because `DashboardRow::state_label` and `State:` do not exist yet.

- [ ] **Step 2: Implement the model fields**

Add `state_label` to `DashboardRow`, add `session_state_label`, populate it in `build_dashboard_model`, and include it in `session_detail_lines`.

- [ ] **Step 3: Update dashboard rendering**

Render agents rows as symbol plus plain state, runtime, location, agent, and relative time. Keep selection styling and details panel unchanged except for the new state line.

- [ ] **Step 4: Verify agents tests**

Run: `cargo test --test mod test_build_dashboard_model_empty_state test_session_detail_lines_include_expected_fields -- --nocapture`

Expected: PASS.

### Task 2: Cleanup Row Model and Bulk Selection

**Files:**
- Modify: `src/tui.rs`
- Modify: `tests/unit/tui_tests.rs`

- [ ] **Step 1: Write the failing tests**

Add tests for a public `build_cleanup_rows` helper and `next_bulk_selection_state` helper. The row test should cover merged, remote, dirty state and assert display text contains plain words instead of relying on emoji-only meaning.

Run: `cargo test --test mod cleanup -- --nocapture`

Expected: FAIL because the cleanup helpers do not exist.

- [ ] **Step 2: Implement cleanup helpers**

Create `CleanupRow` with `branch`, `path_label`, `reason_label`, `remote_label`, `dirty_label`, and `display_line`. Implement `build_cleanup_rows(statuses, selected)` and `next_bulk_selection_state(selected)` in `src/tui.rs`.

- [ ] **Step 3: Wire cleanup rendering**

Use `build_cleanup_rows` inside `CleanupTui::run`, change the header to `Worktree cleanup`, update the list title to explain the screen, and update the footer to include `a: Toggle all` and `Esc: Cancel`.

- [ ] **Step 4: Add keyboard aliases**

Handle `j` as down, `k` as up, `a` as select/clear all, and `Esc` as cancel in cleanup. Add `j/k` aliases to agents and worktree switcher navigation.

- [ ] **Step 5: Verify cleanup tests**

Run: `cargo test --test mod cleanup -- --nocapture`

Expected: PASS.

### Task 3: Final Verification

**Files:**
- Modify: `src/tui.rs`
- Modify: `tests/unit/tui_tests.rs`
- Create: `docs/superpowers/specs/2026-04-26-tui-ergonomics-design.md`
- Create: `docs/superpowers/plans/2026-04-26-tui-ergonomics.md`

- [ ] **Step 1: Format**

Run: `cargo fmt --check`

Expected: PASS.

- [ ] **Step 2: Run focused TUI tests**

Run: `cargo test --test mod tui_tests -- --nocapture`

Expected: PASS.

- [ ] **Step 3: Run adjacent agents tests**

Run: `cargo test --test mod agents_tests -- --nocapture`

Expected: PASS.

- [ ] **Step 4: Check patch hygiene**

Run: `git diff --check`

Expected: PASS.
