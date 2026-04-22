### Project Name Idea: **Git-Warp**

This name evokes speed, instantaneous action, and moving between different contexts (worktrees). It's short, memorable, and hints at the "sci-fi" level of speed that Copy-on-Write provides. The CLI command could be `warp`.

### Versioning and Iteration Strategy

We will follow **Semantic Versioning (SemVer)** starting from `v0.x.y`. This signifies that the project is in its initial development phase, and the API/CLI is not yet stable. Each major feature set will correspond to a minor version bump (e.g., `v0.1.0`, `v0.2.0`).

*   **v0.1.0:** Foundation & Core CoW Logic
*   **v0.2.0:** Full CLI Parity & Advanced Features
*   **v0.3.0:** TUI & Interactive Experience
*   **v0.4.0:** Expanded Platform Support & Polish
*   **v1.0.0:** Stable release with a complete feature set.

---

## The "Git-Warp" Project Plan

This plan outlines the major phases to merge the concepts of `autowt` and `coworktree` into a single, high-performance, user-friendly Rust application.

### Phase 0: Foundation & Project Setup (v0.1.0)

**Goal:** Establish the Rust project structure, dependencies, and a basic, non-functional CLI shell. This phase is about getting the scaffolding right.

1.  **Project Initialization:**
    *   `cargo new git-warp --bin`
    *   Choose a license (e.g., MIT, like the original projects).
    *   Set up the initial `README.md` and `.gitignore`.

2.  **Core Dependencies:**
    *   **CLI:** `clap` (with the `derive` feature) for argument parsing.
    *   **Git:** `gix` (git-oxide) for a modern, high-performance, and safe pure-Rust Git implementation.
    *   **System Calls:** `nix` for low-level syscalls like `clonefile` on macOS.
    *   **Error Handling:** `anyhow` for simple, context-rich error handling in the application layer.
    *   **Logging:** `log` and `env_logger` for structured logging.

3.  **CLI Structure Definition:**
    *   Using `clap`, define the full command hierarchy from `autowt`:
        *   `warp <BRANCH>` (dynamic branch command)
        *   `warp switch <BRANCH>` (explicit switch command)
        *   `warp ls`
        *   `warp cleanup`
        *   `warp config`
        *   `warp agents`
        *   `warp hooks-install`
    *   At this stage, these commands will only parse arguments and print a "Not yet implemented" message.

4.  **Logging and Configuration Stubs:**
    *   Set up a basic logger that can be controlled by a `--debug` flag.
    *   Create a `config.rs` module with a stub `Config` struct.

**Deliverable for v0.1.0:** A compilable Rust binary that recognizes all the target commands and flags but performs no actions. This validates the project structure and dependency choices.

---

### Phase 1: Core Functionality - The CoW Engine (v0.2.0)

**Goal:** Implement the core value proposition: blazingly fast worktree creation using CoW. This phase ports the essential logic from `coworktree` and makes the tool functional for basic use cases.

1.  **CoW Abstraction Layer:**
    *   Create a `cow.rs` module.
    *   Implement `clone_directory(src, dest)` function.
    *   Inside, use `#[cfg(target_os = "macos")]` to gate the `nix::sys::stat::clonefile` implementation.
    *   For other OSes (`#[cfg(not(target_os = "macos"))]`), implement a fallback that simply returns an "unsupported" error for now.

2.  **Git Operations Module:**
    *   Create a `git.rs` module to wrap `gix` operations.
    *   Implement functions for:
        *   Finding the repo root.
        *   Creating a new branch.
        *   Listing worktrees.
        *   Adding a worktree reference (`git worktree add`).
        *   Removing a worktree reference (`git worktree remove`).
        *   Deleting a local branch.

3.  **Implement Core Commands:**
    *   **`warp <BRANCH>` / `warp create <BRANCH>`:**
        *   Check for CoW support. If available, use the `cow::clone_directory` function.
        *   If CoW fails or is unavailable, fall back to a standard `gix` worktree creation.
        *   After creating the directory, use `gix` to create the new branch inside the new worktree and register it with the main repo.
    *   **`warp list`:** Use `gix` to list all worktrees. Enhance the output to show which are CoW-managed.
    *   **`warp remove <BRANCH>`:**
        *   Use `gix` to find the worktree path.
        *   Remove the worktree directory (`std::fs::remove_dir_all`).
        *   Use `gix` to run `worktree prune` and optionally delete the branch.

4.  **Path Rewriting:**
    *   Port the path rewriting logic from `coworktree`.
    *   Use the `ignore` crate to parse `.gitignore` files efficiently.
    *   Use the `rayon` crate to walk the directory and rewrite files in parallel, making it extremely fast.

**Deliverable for v0.2.0:** A functional command-line tool that can create, list, and remove worktrees, using CoW on macOS for near-instant creation. This is the MVP.

---

### Phase 2: Advanced Features & UX (v0.3.0)

**Goal:** Port the sophisticated developer experience features from `autowt` to make the tool powerful and seamless.

1.  **Layered Configuration System:**
    *   Use the `figment` crate to build a cascading config system.
    *   Layers: Default values -> Global file (`~/.config/git-warp/config.toml`) -> Project file (`.git-warp.toml`) -> Environment variables (`GITWARP_...`) -> CLI flags.
    *   Implement `warp config --show` to display the final merged configuration.

2.  **Process Management:**
    *   Use the `sysinfo` crate, which is cross-platform.
    *   Implement `find_processes_in_directory` and `terminate_processes` for the `cleanup` command.
    *   Integrate this into the `warp cleanup` logic, respecting the `--kill` and `--no-kill` flags and config values.

3.  **Terminal Integration:**
    *   This is the most complex part of this phase.
    *   Create a `terminal.rs` module with a `Terminal` trait.
    *   Implement platform-specific structs: `ITerm2`, `AppleTerminal`, etc.
    *   On macOS, use `std::process::Command` to execute `osascript` with the required AppleScript strings.
    *   Implement the `warp switch` command, which orchestrates opening/switching tabs or windows.
    *   Implement the `--terminal=echo` and `--terminal=inplace` modes.

4.  **Agent & Hooks Support:**
    *   Implement `warp hooks-install`. This will involve parsing and merging JSON for the Claude `settings.json` file. The `serde_json` crate will be essential here.
    *   Implement a file watcher using the `notify` crate to monitor the agent status file. This will be used by the `warp agents` TUI in the next phase.

**Deliverable for v0.3.0:** A feature-rich tool that rivals `autowt` in functionality, combining `autowt`'s UX with `coworktree`'s performance.

---

### Phase 3: TUI & Interactive Experience (v0.4.0)

**Goal:** Build the rich Textual User Interfaces (TUIs) from `autowt` for an even better interactive experience.

1.  **TUI Framework Setup:**
    *   Integrate `ratatui` as the TUI library.
    *   Use `crossterm` as the terminal backend for cross-platform support.
    *   Structure a TUI application loop.

2.  **Implement TUIs:**
    *   **`warp agents`:** Build the live agent monitoring dashboard. It will use the file watcher from Phase 2 to get live updates and re-render the `ratatui` UI.
    *   **`warp cleanup --interactive`:** Create the interactive worktree selector.
    *   **`warp config`:** Build the interactive configuration editor TUI.
    *   **`warp hooks-install` (interactive):** Create the TUI for interactively choosing the hook installation level.

**Deliverable for v0.4.0:** The tool now has full feature parity with `autowt`, including its polished interactive TUIs, but built on a high-performance Rust core.

---

### Phase 4: Broaden Support & Mature (v0.5.0 and beyond)

**Goal:** Make the tool robust, well-documented, and available on more platforms.

1.  **Linux CoW Support:**
    *   Research and implement the `overlayfs` backend for Linux. This will be a significant feature requiring careful implementation of mounting and unmounting logic. This will likely involve more `nix` syscalls.

2.  **Documentation & CI/CD:**
    *   Set up GitHub Actions for automated testing, linting (`clippy`), and formatting (`rustfmt`).
    *   Write comprehensive user documentation and guides.
    *   Generate and publish API documentation using `cargo doc`.

3.  **Distribution:**
    *   Use `cargo-dist` or custom GitHub Actions to build and release binaries for macOS (x86_64, aarch64), Linux (x86_64, aarch64), and Windows.
    *   Create a Homebrew tap for easy installation on macOS.

By following this phased plan, you can incrementally build a powerful, best-of-both-worlds tool while ensuring each version delivers a concrete set of valuable features.
