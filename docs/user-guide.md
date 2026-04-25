# Git-Warp User Guide

Git-Warp is a Rust CLI for working with Git worktrees. It focuses on fast
worktree creation, terminal handoff, safe cleanup, and optional Claude/Codex
session visibility.

## Table of Contents

1. [Installation](#installation)
2. [Quick Start](#quick-start)
3. [Switching Worktrees](#switching-worktrees)
4. [Listing Worktrees](#listing-worktrees)
5. [Cleanup](#cleanup)
6. [Agent Session Dashboard](#agent-session-dashboard)
7. [Configuration](#configuration)
8. [Troubleshooting](#troubleshooting)
9. [Best Practices](#best-practices)
10. [Advanced Scenarios](#advanced-scenarios)

## Installation

### Prerequisites

- Rust, latest stable toolchain.
- Git.
- macOS/APFS for Copy-on-Write acceleration. Other platforms and filesystems use
  traditional Git worktree creation.

### Build from Source

```bash
git clone https://github.com/denysbutenko/git-warp
cd git-warp
cargo build --release
cargo install --path .
warp --version
```

### Check Setup

```bash
warp doctor
```

`warp doctor` checks repository detection, config path, worktree base path, CoW
support, terminal mode, and hook setup.

## Quick Start

Run these commands from inside a Git repository:

```bash
# Create or switch to a worktree for a branch
warp switch feature/amazing-new-feature

# Short form
warp feature/amazing-new-feature

# List worktrees
warp ls

# Preview cleanup
warp --dry-run cleanup --mode merged

# Pick cleanup candidates interactively
warp cleanup --interactive
```

## Switching Worktrees

```bash
# Existing or new branch
warp switch feature/user-authentication

# Custom path
warp switch feature/ui-redesign --path /tmp/ui-redesign

# Skip Copy-on-Write checks and use normal Git worktree creation
warp switch testing-branch --no-cow

# Jump to agent-related branches when local session data is available
warp switch --latest
warp switch --waiting
```

Terminal handoff modes:

```bash
warp --terminal tab switch feature-branch
warp --terminal window switch feature-branch
warp --terminal current switch feature-branch
warp --terminal inplace switch feature-branch
warp --terminal echo switch feature-branch
```

## Listing Worktrees

```bash
warp ls
warp ls --debug
```

The list output marks useful state such as primary, current, dirty, detached,
and busy worktrees. `--debug` includes additional details for diagnostics.

## Cleanup

Git-Warp analyzes worktrees before removal. Protected branches default to
`main`, `master`, and `develop`.

```bash
warp cleanup --mode merged
warp cleanup --mode remoteless
warp cleanup --mode all
warp cleanup --interactive
warp --dry-run cleanup --mode all
```

Process and dirty-worktree handling:

```bash
# Terminate blocking processes before removal
warp cleanup --mode merged --kill

# Ignore safety blocks only when you have checked the candidate
warp cleanup --mode merged --force

# Override config that would otherwise kill processes
warp cleanup --mode merged --no-kill
```

Use `--dry-run` first when you are unsure what a cleanup mode will select.

## Agent Session Dashboard

The `agents` command opens a TUI dashboard for live hook records and recent local
Claude/Codex session history scoped to the current repository and its worktrees.

```bash
warp hooks-install --level user --runtime all
warp hooks-status
warp agents
```

Use `--level project` to install hooks only for the current project:

```bash
warp hooks-install --level project --runtime all
```

If no hook records or readable session history exists, the dashboard shows an
empty state with setup guidance.

## Configuration

Show the effective config:

```bash
warp config --show
```

Create or open the config file in `$VISUAL` or `$EDITOR`:

```bash
warp config --edit
```

Default config path:

```text
~/.config/git-warp/config.toml
```

Example:

```toml
terminal_mode = "tab"
use_cow = true
auto_confirm = false
# worktrees_path = "/custom/path/to/worktrees"

[git]
default_branch = "main"
protected_branches = ["main", "master", "develop"]
auto_fetch = true
auto_prune = true

[process]
check_processes = true
auto_kill = false
kill_timeout = 5

[terminal]
app = "auto"
auto_activate = true
init_commands = []

[agent]
enabled = true
refresh_rate = 1000
max_activities = 100
claude_hooks = true
```

Environment variables override config file values:

```bash
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_USE_COW=false
export GIT_WARP_AUTO_CONFIRM=false
export GIT_WARP_WORKTREES_PATH=/Users/me/dev/worktrees
```

Command-line options have the highest priority:

```bash
warp --terminal window --auto-confirm switch feature-branch
```

## Troubleshooting

### Not in a Git Repository

Run Git-Warp from inside a Git repository:

```bash
cd /path/to/repo
warp doctor
```

### CoW Is Not Available

Git-Warp falls back to normal Git worktree creation when CoW is unsupported.
You can skip CoW checks explicitly:

```bash
warp switch --no-cow branch-name
```

### Terminal Handoff Fails

Use `echo` mode to get a plain path without terminal automation:

```bash
warp --terminal echo switch branch-name
```

Then manually `cd` into the printed path.

### Cleanup Is Blocked

Preview candidates and inspect blockers:

```bash
warp --dry-run cleanup --mode merged
warp ls --debug
```

If processes are the blocker, either stop them manually or use `--kill`. Use
`--force` only when you are intentionally bypassing dirty/process safety.

### Config Does Not Open

`warp config --edit` needs `$VISUAL` or `$EDITOR`.

```bash
export EDITOR=vim
warp config --edit
```

## Best Practices

- Run `warp doctor` after installation or config changes.
- Prefer `warp --dry-run cleanup ...` before destructive cleanup.
- Keep protected branch config aligned with your repo conventions.
- Use short branch names that still identify the task.
- Use `warp --terminal echo switch <branch>` in scripts or automation where
  opening a terminal tab would be surprising.

## Advanced Scenarios

### Large Repositories

Use a stable worktree location on a fast disk:

```bash
export GIT_WARP_WORKTREES_PATH=/fast-ssd/worktrees
warp switch feature/large-repo-change
```

### Team Defaults

Share conservative settings through shell profile or onboarding docs:

```bash
export GIT_WARP_AUTO_CONFIRM=false
export GIT_WARP_TERMINAL_MODE=window
```

### CI or Non-Interactive Scripts

Use terminal output modes that do not launch UI:

```bash
export GIT_WARP_AUTO_CONFIRM=true
export GIT_WARP_USE_COW=false
export GIT_WARP_TERMINAL_MODE=echo
```

Prefer `warp --help` and `warp <command> --help` as the source of truth for the
current command surface.
