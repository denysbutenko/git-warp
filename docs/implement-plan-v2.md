### Project: Git-Warp v0.1.0 - A High-Performance, UX-Focused Worktree Manager

**Mission:** To combine the instantaneous, Copy-on-Write worktree creation of `coworktree` with the rich user experience, terminal integration, and advanced features of `autowt`. The result will be a single, statically-linked binary written in Rust.

### Core Dependencies (The Toolbox)

This is the curated list of Rust crates we will use to build the project. Versions are based on current stable releases.

| Crate | Version | Purpose |
| :--- | :--- | :--- |
| **CLI & TUI** | | |
| `clap` | `~4.5.4` | Powerful, ergonomic, and fast command-line argument parsing. |
| `ratatui` | `~0.26.2` | A modern library for building rich Textual User Interfaces (TUIs). |
| `crossterm` | `~0.27.0` | Cross-platform terminal manipulation library, used as the backend for `ratatui`. |
| **Core Logic** | | |
| `gix` | `~0.62.0` | A high-performance, pure-Rust implementation of Git (git-oxide) for all repository operations. |
| `nix` | `~0.28.0` | Safe wrappers for low-level Unix/Linux/macOS syscalls (specifically `clonefile` for CoW). |
| `rayon` | `~1.10.0` | Data-parallelism library for high-performance, parallel file processing (for path rewriting). |
| `ignore` | `~0.4.22` | Fast `.gitignore` parsing and file traversal. |
| `sysinfo` | `~0.30.12` | Cross-platform library for inspecting and managing system processes. |
| `notify` | `~6.1.1` | Cross-platform filesystem event notification for live agent monitoring. |
| **Configuration & Data** | | |
| `figment` | `~0.10.19` | A flexible, layered configuration library supporting files, environment variables, and defaults. |
| `serde` | `~1.0.203` | The standard for efficient, generic serialization and deserialization in Rust. |
| `serde_json` | `~1.0.117` | JSON support for Serde, used for Claude hooks management. |
| `toml` | `~0.8.13` | TOML support for Serde, used for our configuration files. |
| **Utilities** | | |
| `anyhow` | `~1.0.86` | Provides `anyhow::Result` for flexible, context-rich error handling in the application. |
| `thiserror` | `~1.0.61` | For creating custom, boilerplate-free error types within our library modules. |
| `log` | `~0.4.21` | A standard logging facade. |
| `env_logger` | `~0.11.3` | A logger implementation that can be configured via environment variables and CLI flags. |

---

### The Unified Implementation Plan for v0.1.0

This plan is structured into six modules, which can be seen as sequential development phases.

#### Module 1: Project Foundation & CLI Structure

**Goal:** Establish the project's skeleton, dependencies, and a complete, non-functional CLI interface.

1.  **Project Setup:**
    *   Initialize the Rust project: `cargo new git-warp --bin`.
    *   Populate `Cargo.toml` with all the dependencies listed above.
    *   Create the module structure in `src/`: `main.rs`, `cli.rs`, `config.rs`, `git.rs`, `cow.rs`, `rewrite.rs`, `process.rs`, `terminal.rs`, `tui.rs`, `hooks.rs`, `error.rs`.

2.  **CLI Definition (`cli.rs`):**
    *   Using `clap`'s derive macro, define the entire command hierarchy inspired by `autowt`.
    *   **Main Command:** `warp` (with dynamic branch name fallback).
    *   **Subcommands:** `switch`, `ls`, `cleanup`, `config`, `agents`, `hooks-install`, `shell-config`.
    *   Define all flags: `--terminal`, `--init`, `--kill`, `--dry-run`, `--user`, `--project`, `--debug`, etc.
    *   Initially, each command handler will simply print a "Not yet implemented" message.

3.  **Logging and Error Handling:**
    *   In `main.rs`, set up `env_logger` to be controlled by the `--debug` flag.
    *   Define a custom `Result` type for the application: `pub type Result<T> = anyhow::Result<T>;`.

#### Module 2: The Core Engine - Git & Copy-on-Write

**Goal:** Implement the high-performance worktree creation, making the tool's core promise a reality.

1.  **Git Abstraction (`git.rs`):**
    *   Create a `GitRepository` struct that wraps `gix`.
    *   Implement functions: `find_root()`, `list_worktrees()`, `create_worktree_and_branch()`, `remove_worktree()`, `prune_worktrees()`, `delete_branch()`, `analyze_branches_for_cleanup()`.

2.  **CoW Abstraction (`cow.rs`):**
    *   Implement `is_cow_supported()` which checks the filesystem type.
    *   Implement `clone_directory(src, dest)`. This function will contain the platform-specific logic:
        *   `#[cfg(target_os = "macos")]`: Use `nix::sys::stat::clonefile`.
        *   `#[cfg(target_os = "linux")]`: Placeholder for future `overlayfs` implementation.
        *   The function will return a `Result` to indicate success, failure, or "unsupported".

3.  **Implement `create` and `switch` Commands:**
    *   This is the heart of the merge. The command logic will be:
        1.  Find the repository root.
        2.  Determine the target worktree path.
        3.  Attempt a CoW clone via `cow::clone_directory`.
        4.  **If CoW succeeds:** The directory is now instantly ready.
        5.  **If CoW is unsupported or fails:** Fall back to creating a standard worktree directory and checking out files with `gix`.
        6.  In both cases, use `gix` to create the new branch inside the worktree and register it with the main repository.

#### Module 3: Post-Creation Hook - Path Rewriting

**Goal:** Ensure CoW-created worktrees are immediately functional by fixing broken absolute paths in build tools and virtual environments.

1.  **Path Rewriter (`rewrite.rs`):**
    *   Create a `PathRewriter` struct that holds the source and destination paths.
    *   Use the `ignore::WalkBuilder` to traverse the new worktree directory, automatically respecting `.gitignore`.
    *   Use `rayon` to process the file walk in parallel.
    *   For each file that is not binary, read its contents and perform a find-and-replace of the source path with the destination path.
2.  **Integration:** Call the path rewriter immediately after a successful CoW clone in the `create` command logic.

#### Module 4: Advanced Features - Cleanup, Config & Hooks

**Goal:** Port the sophisticated management and configuration features from `autowt`.

1.  **Process Management (`process.rs`):**
    *   Use `sysinfo::System` to get a list of all processes.
    *   Implement `find_processes_in_directory(path)` by iterating through processes and checking their `cwd()`.
    *   Implement `terminate_processes(pids)` with a graceful SIGINT -> SIGKILL escalation.
2.  **Implement `cleanup` Command:**
    *   Use `gix` to analyze branches (merged, remoteless).
    *   For each worktree to be removed, call the process management module to find and terminate running processes.
    *   Remove the directory and then use `gix` to clean up the Git metadata.
3.  **Configuration (`config.rs`):**
    *   Define the complete `Config` struct using `serde::Deserialize`.
    *   Use `figment` to create the loader that merges defaults -> global file (`~/.config/git-warp/config.toml`) -> project file (`.git-warp.toml`) -> environment variables (`GITWARP_*`) -> CLI flags.
4.  **Implement `hooks-install` Command (`hooks.rs`):**
    *   Use `serde_json` to read, modify, and write the Claude `settings.json` file. This is a direct logic port from `autowt`.

#### Module 5: The User Experience - Terminal & TUIs

**Goal:** Build the interactive and automated user interfaces that make the tool a joy to use.

1.  **Terminal Integration (`terminal.rs`):**
    *   Define a `Terminal` trait with methods like `open_tab(path)`, `open_window(path)`, etc.
    *   Create platform-specific implementations. On macOS, this will use `std::process::Command` to call `osascript`.
    *   The `switch` command will use this module to automatically open the new worktree.
2.  **TUI Framework (`tui.rs`):**
    *   Set up the main TUI application loop using `ratatui` and `crossterm`.
3.  **Implement All TUIs:**
    *   **`warp agents`:** Build the live agent dashboard. Use the `notify` crate in a separate thread to watch the status file and send updates to the TUI thread via a channel to trigger re-renders.
    *   **`warp cleanup --interactive`:** Build the interactive worktree selector for cleanup.
    *   **`warp config`:** Build the interactive configuration editor.
    *   **`warp hooks-install` (interactive):** Build the TUI for selecting the hook installation level.

#### Module 6: Finalization & Distribution

**Goal:** Prepare the project for release with robust testing, documentation, and automated builds.

1.  **Comprehensive Testing:**
    *   Write unit tests for each module.
    *   Write integration tests that create real Git repositories in a temporary directory and run `git-warp` commands against them.
2.  **Documentation:**
    *   Create a detailed `README.md` explaining the "why," the features, installation, and usage.
    *   Implement the `warp shell-config` command to print helper functions for various shells.
3.  **CI/CD Pipeline (GitHub Actions):**
    *   **Test:** On every push, run `cargo test`, `cargo fmt --check`, and `cargo clippy -- -D warnings`.
    *   **Release:** When a Git tag is pushed, use `cargo-dist` or a similar tool to build binaries for macOS (aarch64/x86_64), Linux (x86_64), and Windows, then attach them to a GitHub Release.
