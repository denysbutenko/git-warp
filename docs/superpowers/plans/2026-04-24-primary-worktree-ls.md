# Primary Worktree Ls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix primary/current worktree detection and make `warp ls` show compact status labels for daily worktree decisions.

**Architecture:** Keep structural worktree facts in `src/git.rs` by enriching `WorktreeInfo`. Keep command-specific formatting and volatile dirty/process checks in `src/cli.rs`, so cleanup and switching can reuse accurate primary/current data without inheriting `ls` formatting concerns.

**Tech Stack:** Rust, clap CLI, git porcelain commands, sysinfo process discovery, cargo unit and integration tests.

---

### Task 1: Add regression tests for worktree identity

**Files:**
- Modify: `tests/unit/git_tests.rs`

- [ ] **Step 1: Update `test_list_worktrees_single` expectations**

Add assertions after the existing path assertion:

```rust
assert!(worktrees[0].is_primary);
assert!(worktrees[0].is_current);
assert!(!worktrees[0].is_detached);
```

- [ ] **Step 2: Add linked-worktree primary/current regression test**

Add this test near the other worktree list tests:

```rust
#[test]
fn test_list_worktrees_marks_main_primary_and_linked_current() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let linked_path = repo_path.join("worktrees").join("feature-current");

    std::env::set_current_dir(repo_path).unwrap();
    let main_repo = GitRepository::find().unwrap();
    main_repo
        .create_worktree_and_branch("feature-current", &linked_path, None)
        .unwrap();

    std::env::set_current_dir(&linked_path).unwrap();
    let linked_repo = GitRepository::find().unwrap();
    let worktrees = linked_repo.list_worktrees().unwrap();

    let main = worktrees.iter().find(|w| w.path == repo_path).unwrap();
    let linked = worktrees.iter().find(|w| w.path == linked_path).unwrap();

    assert!(main.is_primary);
    assert!(!main.is_current);
    assert!(!linked.is_primary);
    assert!(linked.is_current);
}
```

- [ ] **Step 3: Run tests and confirm RED**

Run:

```bash
cargo test test_list_worktrees -- --nocapture
```

Expected: compile failure or test failure because `WorktreeInfo` does not yet expose `is_current` and `is_detached`, and primary detection is still wrong.

### Task 2: Add CLI output regression coverage

**Files:**
- Modify: `tests/integration/cli_surface_tests.rs`

- [ ] **Step 1: Add detached worktree helper**

Add below `create_worktree`:

```rust
fn create_detached_worktree(repo_path: &Path, name: &str) -> PathBuf {
    let worktree_path = repo_path.join(".worktrees").join(name);
    fs::create_dir_all(worktree_path.parent().unwrap()).unwrap();

    let output = Command::new("git")
        .args(["worktree", "add", "--detach"])
        .arg(&worktree_path)
        .arg("HEAD")
        .current_dir(repo_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "git worktree add --detach failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    worktree_path
}
```

- [ ] **Step 2: Add `warp ls` status-label test**

Add this test near the other CLI surface tests:

```rust
#[test]
fn test_ls_shows_primary_current_dirty_and_detached_statuses() {
    let temp_dir = setup_test_repo();
    let repo_path = temp_dir.path();
    let feature_path = create_worktree(repo_path, "feature-status");
    let detached_path = create_detached_worktree(repo_path, "detached-status");

    fs::write(feature_path.join("dirty.txt"), "changed\n").unwrap();

    let output = warp_command(&feature_path).args(["ls"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("main [primary]"));
    assert!(stdout.contains("feature-status [current dirty]"));
    assert!(stdout.contains("[detached]"));
    assert!(stdout.contains(&repo_path.display().to_string()));
    assert!(stdout.contains(&detached_path.display().to_string()));
}
```

- [ ] **Step 3: Run integration test and confirm RED**

Run:

```bash
cargo test test_ls_shows_primary_current_dirty_and_detached_statuses -- --nocapture
```

Expected: compile failure until `WorktreeInfo` and output formatting are implemented, or assertion failure because current output lacks labels.

### Task 3: Implement accurate worktree identity

**Files:**
- Modify: `src/git.rs`

- [ ] **Step 1: Extend `WorktreeInfo`**

Add fields:

```rust
pub is_current: bool,
pub is_detached: bool,
```

- [ ] **Step 2: Initialize new fields during porcelain parsing**

When creating each `WorktreeInfo`, initialize:

```rust
is_current: false,
is_detached: false,
```

Handle detached porcelain lines:

```rust
} else if line == "detached" {
    if let Some(ref mut wt) = current_worktree {
        wt.is_detached = true;
    }
}
```

- [ ] **Step 3: Enrich entries after parsing**

After pushing the final worktree, add:

```rust
let current_root = self
    .repo_path
    .canonicalize()
    .unwrap_or_else(|_| self.repo_path.clone());

for (index, worktree) in worktrees.iter_mut().enumerate() {
    if index == 0 {
        worktree.is_primary = true;
    }

    let worktree_path = worktree
        .path
        .canonicalize()
        .unwrap_or_else(|_| worktree.path.clone());
    worktree.is_current = worktree_path == current_root;

    if worktree.branch.is_empty() {
        worktree.is_detached = true;
    }
}
```

- [ ] **Step 4: Run unit tests and confirm GREEN for identity**

Run:

```bash
cargo test test_list_worktrees -- --nocapture
```

Expected: all `test_list_worktrees*` tests pass.

### Task 4: Add compact `warp ls` status labels

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Import and initialize process manager in `handle_ls`**

Add inside `handle_ls`:

```rust
use crate::process::ProcessManager;
```

After loading worktrees:

```rust
let mut process_manager = ProcessManager::new();
```

- [ ] **Step 2: Compute labels for each row**

Add private helper methods on `Cli`:

```rust
fn worktree_status_labels(
    worktree: &crate::git::WorktreeInfo,
    is_dirty: bool,
    is_busy: bool,
) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if worktree.is_primary {
        labels.push("primary");
    }
    if worktree.is_current {
        labels.push("current");
    }
    if is_dirty {
        labels.push("dirty");
    }
    if worktree.is_detached {
        labels.push("detached");
    }
    if is_busy {
        labels.push("busy");
    }
    labels
}

fn format_status_labels(labels: &[&str]) -> String {
    if labels.is_empty() {
        String::new()
    } else {
        format!(" [{}]", labels.join(" "))
    }
}
```

- [ ] **Step 3: Update `handle_ls` row output**

Inside the row loop, compute:

```rust
let is_dirty = git_repo
    .has_uncommitted_changes(&worktree.path)
    .unwrap_or(false);
let is_busy = !worktree.is_current
    && process_manager
        .has_processes_in_directory(&worktree.path)
        .unwrap_or(false);
let labels = Self::worktree_status_labels(worktree, is_dirty, is_busy);
let label_display = Self::format_status_labels(&labels);
```

Then print:

```rust
println!(
    "{}  {}{} {}",
    status_icon,
    branch_display,
    label_display,
    worktree.path.display()
);
```

- [ ] **Step 4: Extend debug output**

Add debug lines:

```rust
println!("     Current: {}", worktree.is_current);
println!("     Detached: {}", worktree.is_detached);
println!("     Dirty: {}", is_dirty);
println!("     Busy: {}", is_busy);
```

- [ ] **Step 5: Run CLI integration test and confirm GREEN**

Run:

```bash
cargo test test_ls_shows_primary_current_dirty_and_detached_statuses -- --nocapture
```

Expected: test passes and stdout contains compact labels.

### Task 5: Verify full focused behavior

**Files:**
- No code edits unless verification exposes a defect.

- [ ] **Step 1: Format and lint whitespace**

Run:

```bash
cargo fmt
git diff --check
```

Expected: both commands exit successfully.

- [ ] **Step 2: Run focused test suite**

Run:

```bash
cargo test git_tests -- --nocapture
cargo test cli_surface_tests -- --nocapture
```

Expected: all focused tests pass.

- [ ] **Step 3: Smoke test local `warp ls`**

Run:

```bash
cargo run -- ls
```

Expected: the main checkout line contains `[primary]`, the issue worktree line contains `[current]`, and the output remains compact.
