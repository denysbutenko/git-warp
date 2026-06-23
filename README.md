# Git-Warp

Fast, safety-focused Git worktree management with terminal handoff, cleanup
helpers, and optional Claude/Codex session visibility.

Git-Warp is a Rust CLI for creating, switching, listing, and cleaning Git
worktrees. On macOS/APFS and Linux filesystems that implement the FICLONE
ioctl (btrfs, xfs with `reflink=1`, bcachefs, OCFS2, etc.) it can use
Copy-on-Write cloning for faster worktree creation, and it falls back to
normal `git worktree` creation when CoW is not available.

## What It Does Today

- Creates or switches to branch worktrees with `warp switch <branch>` or the
  short form `warp <branch>`, classifying the target as existing worktree,
  local branch, remote branch, or new branch.
- Opens the selected worktree in a terminal tab/window, starts a shell in the
  current terminal, or prints the target path/command.
- Lists worktrees with primary/current/dirty/detached/busy state, ordered for
  fast scanning in busy repositories.
- Bare `warp` opens an interactive switcher with multi-select and batch removal.
- Cleans up eligible worktrees with dry-run, interactive selection, process
  checks, protected branches, optional process termination, and per-worktree
  eligibility reasons.
- Checks local setup with `warp doctor`, including detection of multiple `warp`
  binaries on `PATH`.
- Installs, removes, and reports Claude/Codex hooks with per-runtime and
  per-scope `Complete` / `Partial` / `Missing` / `Conflicting` diagnostics and
  the exact repair command.
- Shows a TUI dashboard for live hook data and recent local agent sessions.
- Generates shell completion snippets for Bash, Zsh, and Fish.
- Validates release metadata before tagging with `warp release-check`.

## Install

One command, no Rust/Cargo required:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | sh
```

Then verify the install:

```bash
warp --version
warp doctor
```

The installer downloads a prebuilt binary for macOS/Linux and installs `warp` to
`~/.local/bin`. If your shell cannot find `warp`, add that directory to `PATH`.

More install options, including upgrade and uninstall flows: [Install Git-Warp](docs/install.md).

Cargo is still available as a fallback:

```bash
curl -fsSL https://raw.githubusercontent.com/denysbutenko/git-warp/main/install.sh | GIT_WARP_INSTALL_METHOD=cargo sh
```

Build from source when contributing or testing local changes:

```bash
git clone https://github.com/denysbutenko/git-warp
cd git-warp
cargo build --release
cargo install --path .
warp --help
```

## Quick Start

From inside any Git repository:

```bash
# Create or switch to a branch worktree
warp switch feature/new-ui

# Short form for the same flow
warp feature/new-ui

# List known worktrees and their state
warp ls

# Preview cleanup before deleting anything
warp --dry-run cleanup --mode merged

# Clean up merged worktrees interactively
warp cleanup --interactive
```

## Terminal Modes

```bash
warp --terminal tab switch feature/branch      # new tab
warp --terminal window switch feature/branch   # new window
warp --terminal current switch feature/branch  # shell in this terminal
warp --terminal inplace switch feature/branch  # print a cd command
warp --terminal echo switch feature/branch     # print the target path
```

Configure the macOS terminal app with `terminal.app = "auto"`, `"terminal"`,
`"iterm2"`, or `"warp"`.

## Cleanup

```bash
warp cleanup --mode merged
warp cleanup --mode remoteless
warp cleanup --mode all
warp cleanup --interactive
warp --dry-run cleanup --mode all
warp cleanup --mode merged --kill
warp cleanup --mode merged --force
```

Protected branches default to `main` and `develop`. Cleanup also checks for
dirty worktrees and running processes before removing anything.

## Agent Session Dashboard

The agent dashboard is optional. It shows live hook records when hooks are
installed, plus recent local Claude/Codex session history for the current
repository and its worktrees.

```bash
warp hooks-install --level user --runtime all
warp hooks-status
warp agents
```

Use `--level project` if hooks should be written only for the current project.

## Configuration

Show the effective configuration:

```bash
warp config --show
```

Create or open the config file in `$VISUAL` or `$EDITOR`:

```bash
warp config --edit
```

Default path:

```text
~/.config/git-warp/config.toml
```

Example:

```toml
terminal_mode = "tab"
use_cow = true
auto_confirm = false
# worktrees_path = "/Users/me/dev/worktrees"

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

`[git].auto_fetch`, `[git].auto_prune`, `[process].check_processes`,
`[process].auto_kill`, `[process].kill_timeout`, `[agent].enabled`, and
`[agent].refresh_rate` now drive runtime behavior — see `docs/user-guide.md`
for the per-key semantics. `[process].auto_kill` defers to `--kill` /
`--no-kill` when those flags are present (`--no-kill` always wins).

Environment overrides use the `GIT_WARP_` prefix. Top-level keys map
directly; nested sections use `__` (double underscore) as the separator
(`GIT_WARP_<section>__<field>`):

```bash
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_USE_COW=false
export GIT_WARP_AUTO_CONFIRM=true
export GIT_WARP_WORKTREES_PATH=/custom/worktrees

export GIT_WARP_GIT__DEFAULT_BRANCH=develop
export GIT_WARP_PROCESS__AUTO_KILL=true
export GIT_WARP_POST_CREATE__AUTO_INSTALL=false
```

`terminal.init_commands` run after Git-Warp changes into the worktree for
terminal handoff modes that print or send shell commands.

`post_create.auto_install` controls whether Git-Warp runs the matching
`<manager> install` after creating a new worktree. Lockfile detection order is
`pnpm-lock.yaml` → `yarn.lock` → `bun.lock` → `bun.lockb` → `package-lock.json` → `Cargo.toml`; the first
match wins. Set `auto_install = false` (or
`GIT_WARP_POST_CREATE__AUTO_INSTALL=false`) to skip the install step entirely.

## Shell Integration

```bash
warp shell-config bash >> ~/.bashrc
warp shell-config zsh >> ~/.zshrc
warp shell-config fish >> ~/.config/fish/config.fish
```

## Development

```bash
cargo fmt --check
cargo test --test mod cli_surface_tests -- --nocapture
cargo test --test mod terminal_switch_tests -- --nocapture
cargo test --test mod tui_tests -- --nocapture
git diff --check
```

Before tagging a release, run the release validation flow:

```bash
warp release-check --version v0.3.0
```

Useful manual checks:

```bash
cargo run -- --help
cargo run -- doctor
cargo run -- --dry-run switch docs-check
cargo run -- --dry-run cleanup --mode merged
```

## Documentation

- [Install Git-Warp](docs/install.md)
- [Release Check](docs/release-check.md)
- [User Guide](docs/user-guide.md)
- [Technical Overview](docs/technical-overview.md)
- [Documentation Index](docs/README.md)
- [Changelog](CHANGELOG.md)
- [Release Notes (v0.3.0)](docs/releases/v0.3.0.md)
- [Release Notes (v0.2.0)](docs/releases/v0.2.0.md)

## Status

Git-Warp is usable for local worktree workflows, but it is still evolving. Prefer
`warp doctor`, `--dry-run`, and focused command help (`warp <command> --help`)
when setting it up on a new machine or repository.

## License

MIT.
