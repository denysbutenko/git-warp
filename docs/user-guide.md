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

- Git.
- `curl` or `wget` and `tar` for the one-command installer.
- macOS/APFS or Linux with Btrfs/XFS/OCFS2 for Copy-on-Write acceleration. Other
  platforms and filesystems use traditional Git worktree creation.
- Rust, latest stable toolchain, only when building from source or using the
  Cargo fallback.

### Quick Install

Install Git-Warp with one command. Rust/Cargo is not required:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh
```

Then verify the install:

```bash
warp --version
warp doctor
```

The installer detects macOS/Linux and Intel/Apple Silicon architectures. It
installs to `~/.local/bin` by default. If your shell cannot find `warp`, add
that directory to `PATH`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

For more install options, see [Install Git-Warp](install.md).

Install into another writable directory:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_INSTALL_DIR=/usr/local/bin sh
```

Use Cargo explicitly when you want to build during installation:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_INSTALL_METHOD=cargo sh
```

### Build from Source

Use this path when contributing or testing local changes:

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
support, terminal mode, install layout (lists detected `warp` binaries and
warns when more than one is on `PATH`), and hook setup. Hook output names each
runtime/scope and reports `Complete`, `Partial`, `Missing`, or `Conflicting`
with the exact `warp hooks-install --level <scope> --runtime <runtime>` repair
command.

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

`warp switch` classifies the target as existing worktree, local branch, remote
branch, or new branch and prints a labeled status line. When a branch only
exists on a remote, Git-Warp creates a local tracking branch from that remote
ref instead of branching from `HEAD`. Running bare `warp` opens an interactive
switcher with a `local-only` badge for branches with no matching remote ref;
`Space` and `a` toggle multi-select for batch worktree removal. Press `Tab` to
switch the surface to the [Agent Session Dashboard](#agent-session-dashboard)
and `Tab` again to return.

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
and busy worktrees. Rows are ordered current → primary → dirty → busy →
detached → clean, with a one-line summary header and distinct row icons so
busy repos stay scannable. The `[primary current dirty detached busy]` label
tokens are preserved for script consumers. `--debug` includes additional
details for diagnostics.

## Cleanup

Git-Warp analyzes worktrees before removal. Protected branches default to
`main` and `develop`.

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

Both `warp cleanup` and `warp --dry-run cleanup` print:

- the resolved base branch,
- worktrees skipped from cleanup (primary, base branch, protected, detached, no
  branch checked out) with structured reasons,
- mode-excluded entries,
- candidate tags (`merged` / `identical` / `no remote` / `all-mode`),
- a `process-busy` flag from a pre-confirm process preview.

`--force` can also remove dirty worktrees when you intentionally bypass safety
checks.

## Agent Session Dashboard

The `agents` command opens a TUI dashboard for live hook records and recent local
Claude/Codex session history scoped to the current repository and its worktrees.

The dashboard has two entry points: `warp agents` opens it directly, and
pressing `Tab` inside the [bare `warp` switcher](#switching-worktrees) toggles
between the switcher and the dashboard. `Tab` again returns to the switcher.
When `agent.enabled=false` in the config, `Tab` shows a notice instead of
switching.

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

Launch the interactive editor (in-process TUI; no external editor needed):

```bash
warp config --interactive
# or the short alias
warp config -i
```

The interactive editor lists every section on the left and the fields in the
focused section on the right. `Tab` cycles sections, `Space` toggles booleans,
`Enter`/`e` edits string and numeric fields, `s` saves to disk, `r` reverts
to the last saved state, and `q` quits (with a confirmation prompt if there
are unsaved changes). List-valued fields (`git.protected_branches`,
`terminal.init_commands`) render read-only — edit the TOML file directly to
change them.

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
protected_branches = ["main", "develop"]
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

[post_create]
auto_install = true

[agent]
enabled = true
refresh_rate = 1000
max_activities = 100
```

`[git].auto_fetch` controls whether `warp cleanup` runs `git fetch --all --prune`
before analysis. `[git].auto_prune` gates the `git worktree prune` call that
follows worktree removal. `[process].check_processes` skips the "process-busy"
preview and per-candidate process check when `false`; `[process].auto_kill`
makes `warp cleanup` behave as if `--kill` was passed when neither `--kill` nor
`--no-kill` is on the command line (`--no-kill` still wins).
`[process].kill_timeout` is the SIGTERM grace period before Git-Warp escalates
to SIGKILL. `[agent].enabled = false` short-circuits `warp agents` with a
disabled banner; `[agent].refresh_rate` sets the dashboard poll interval in
milliseconds (clamped to ≥ 250 ms).

`post_create.auto_install` runs the matching `<manager> install` after Git-Warp
creates a new worktree. Lockfile detection order is `pnpm-lock.yaml` →
`yarn.lock` → `bun.lock` → `bun.lockb` → `package-lock.json` → `Cargo.toml`; the first match wins. Set
`auto_install = false` to skip the install step entirely.

Environment variables override config file values. Top-level keys map
directly; nested sections use `__` (double underscore) as the path
separator (`GIT_WARP_<section>__<field>`):

```bash
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_USE_COW=false
export GIT_WARP_AUTO_CONFIRM=false
export GIT_WARP_WORKTREES_PATH=/Users/me/dev/worktrees

export GIT_WARP_GIT__DEFAULT_BRANCH=develop
export GIT_WARP_GIT__AUTO_FETCH=false
export GIT_WARP_PROCESS__AUTO_KILL=true
export GIT_WARP_TERMINAL__APP=iterm2
export GIT_WARP_AGENT__REFRESH_RATE=2000
export GIT_WARP_POST_CREATE__AUTO_INSTALL=false
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

### Skip Auto-Install After Worktree Creation

If you do not want Git-Warp to run `<manager> install` after creating a
worktree (for example, when the install step is slow or managed elsewhere),
set:

```toml
[post_create]
auto_install = false
```

in `~/.config/git-warp/config.toml`, or export
`GIT_WARP_POST_CREATE__AUTO_INSTALL=false`.

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
