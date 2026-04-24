# Warp Doctor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a read-only `warp doctor` onboarding command that checks setup state and prints tailored next steps.

**Architecture:** Keep the CLI entrypoint in `src/cli.rs`, with a small `handle_doctor` command handler and helper methods for status rows and next-step collection. Reuse existing `ConfigManager`, `GitRepository`, `cow`, terminal config, and hook config conventions instead of adding new persistent state.

**Tech Stack:** Rust, clap, anyhow, existing Git-Warp integration tests using `CARGO_BIN_EXE_warp`.

---

### Task 1: Add CLI Coverage First

**Files:**
- Modify: `tests/integration/cli_surface_tests.rs`

- [ ] **Step 1: Add a help test for `doctor`**

Add:

```rust
#[test]
fn test_root_help_shows_doctor_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_warp"))
        .arg("--help")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("Check Git-Warp setup and print next steps"));
}
```

- [ ] **Step 2: Add outside-repo doctor test**

Add:

```rust
#[test]
fn test_doctor_outside_repo_prints_recovery_guidance() {
    let temp_dir = tempdir().unwrap();
    let home_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Git-Warp Doctor"));
    assert!(stdout.contains("Config file"));
    assert!(stdout.contains("Git repository"));
    assert!(stdout.contains("Run this command inside a Git repository"));
}
```

- [ ] **Step 3: Add inside-repo doctor test**

Add:

```rust
#[test]
fn test_doctor_inside_repo_prints_repo_and_worktree_checks() {
    let temp_dir = setup_test_repo();
    let home_dir = tempdir().unwrap();

    let output = warp_command(temp_dir.path())
        .env("HOME", home_dir.path())
        .env("XDG_CONFIG_HOME", home_dir.path().join(".config"))
        .arg("doctor")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("Git-Warp Doctor"));
    assert!(stdout.contains("Git repository"));
    assert!(stdout.contains(temp_dir.path().to_string_lossy().as_ref()));
    assert!(stdout.contains("Worktree base path"));
    assert!(stdout.contains("Next steps"));
}
```

- [ ] **Step 4: Run tests and verify failure**

Run: `cargo test --test mod cli_surface_tests::test_root_help_shows_doctor_command cli_surface_tests::test_doctor_outside_repo_prints_recovery_guidance cli_surface_tests::test_doctor_inside_repo_prints_repo_and_worktree_checks -- --nocapture`

Expected: failure because `doctor` is not a known command yet.

### Task 2: Implement `warp doctor`

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Add `Doctor` to `Commands`**

Add this enum variant near the other user-facing commands:

```rust
    /// Check Git-Warp setup and print next steps
    Doctor,
```

Wire it in `handle_command`:

```rust
            Commands::Doctor => self.handle_doctor(),
```

- [ ] **Step 2: Implement the doctor handler**

Add helper methods on `Cli`:

```rust
    fn handle_doctor(&self) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::cow;
        use crate::git::GitRepository;

        let config_manager = ConfigManager::new()?;
        let config = config_manager.get();
        let repo = GitRepository::find().ok();
        let mut next_steps = Vec::new();

        println!("🩺 Git-Warp Doctor");
        println!("==================");
        println!();
        println!("Checks:");

        if config_manager.config_exists() {
            Self::doctor_ok(
                "Config file",
                format!("found at {}", config_manager.config_path().display()),
            );
        } else {
            Self::doctor_warn(
                "Config file",
                format!("missing at {}", config_manager.config_path().display()),
            );
            next_steps.push("Run `warp config --edit` to create and review your config.".to_string());
        }

        let worktree_base = if let Some(repo) = &repo {
            Self::doctor_ok("Git repository", repo.root_path().display().to_string());
            let base = repo.get_worktree_path_with_base(
                "doctor-check",
                config.worktrees_path.as_deref(),
            );
            Self::doctor_info("Worktree base path", base.parent().unwrap_or(&base).display().to_string());
            base.parent().map(PathBuf::from).unwrap_or(base)
        } else {
            Self::doctor_warn("Git repository", "not detected from current directory");
            next_steps.push("Run this command inside a Git repository before creating worktrees.".to_string());
            config
                .worktrees_path
                .clone()
                .unwrap_or_else(|| PathBuf::from(".worktrees"))
        };

        match cow::is_cow_supported(&worktree_base) {
            Ok(true) => Self::doctor_ok("Copy-on-Write", format!("available for {}", worktree_base.display())),
            Ok(false) => Self::doctor_info("Copy-on-Write", "not available here; Git-Warp will use git worktree add"),
            Err(error) => Self::doctor_warn("Copy-on-Write", format!("could not check support: {error}")),
        }

        Self::doctor_info(
            "Terminal",
            format!("mode {}, app {}", config.terminal_mode, config.terminal.app),
        );

        let hooks_installed = Self::doctor_hooks_installed();
        if hooks_installed {
            Self::doctor_ok("Agent hooks", "git-warp hooks found for Claude or Codex");
        } else {
            Self::doctor_warn("Agent hooks", "no user or project git-warp hooks found");
            next_steps.push("Run `warp hooks-install --level user --runtime all` to enable live agent monitoring.".to_string());
        }

        if repo.is_some() && config_manager.config_exists() && hooks_installed {
            next_steps.push("Run `warp switch <branch>` to create or open a worktree.".to_string());
        }

        println!();
        println!("Next steps:");
        for step in next_steps {
            println!("  - {step}");
        }

        Ok(())
    }
```

- [ ] **Step 3: Add formatting helpers**

Add:

```rust
    fn doctor_ok(label: &str, detail: impl AsRef<str>) {
        println!("  ✅ {label}: {}", detail.as_ref());
    }

    fn doctor_warn(label: &str, detail: impl AsRef<str>) {
        println!("  ⚠️  {label}: {}", detail.as_ref());
    }

    fn doctor_info(label: &str, detail: impl AsRef<str>) {
        println!("  ℹ️  {label}: {}", detail.as_ref());
    }
```

- [ ] **Step 4: Add hook detection helper**

Add a minimal read-only helper that checks known user/project hook files for the existing Git-Warp hook IDs:

```rust
    fn doctor_hooks_installed() -> bool {
        let mut paths = Vec::new();
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".claude").join("settings.json"));
            paths.push(home.join(".codex").join("hooks.json"));
        }
        if let Ok(current_dir) = std::env::current_dir() {
            paths.push(current_dir.join(".claude").join("settings.json"));
            paths.push(current_dir.join(".codex").join("hooks.json"));
        }

        paths.into_iter().any(|path| {
            std::fs::read_to_string(path)
                .map(|content| content.contains("\"git_warp_hook_id\""))
                .unwrap_or(false)
        })
    }
```

- [ ] **Step 5: Run focused tests**

Run: `cargo test --test mod cli_surface_tests::test_root_help_shows_doctor_command cli_surface_tests::test_doctor_outside_repo_prints_recovery_guidance cli_surface_tests::test_doctor_inside_repo_prints_repo_and_worktree_checks -- --nocapture`

Expected: all three tests pass.

### Task 3: Verify and Publish

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Document the command**

Add `warp doctor` to the quick start and command reference as the first setup check.

- [ ] **Step 2: Run verification**

Run:

```bash
cargo fmt --check
git diff --check
cargo test --test mod cli_surface_tests -- --nocapture
cargo run -- doctor
```

- [ ] **Step 3: Commit and push**

Run:

```bash
git status --short
git add src/cli.rs tests/integration/cli_surface_tests.rs README.md docs/superpowers/specs/2026-04-24-warp-doctor-design.md docs/superpowers/plans/2026-04-24-warp-doctor.md
git commit -m "feat: add warp doctor onboarding command"
git push -u origin 6-warp-doctor
```
