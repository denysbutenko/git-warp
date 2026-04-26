# Git-Warp

Fast, safety-focused Git worktree management with terminal handoff, cleanup
helpers, and optional Claude/Codex session visibility.

Git-Warp is a Rust CLI for creating, switching, listing, and cleaning Git
worktrees. On macOS/APFS it can use Copy-on-Write cloning for faster worktree
creation, and it falls back to normal `git worktree` creation when CoW is not
available.

## What It Does Today

- Creates or switches to branch worktrees with `warp switch <branch>` or the
  short form `warp <branch>`.
- Opens the selected worktree in a terminal tab/window, starts a shell in the
  current terminal, or prints the target path/command.
- Lists worktrees with primary/current/dirty/detached/busy state.
- Cleans up eligible worktrees with dry-run, interactive selection, process
  checks, protected branches, and optional process termination.
- Checks local setup with `warp doctor`.
- Installs, removes, and reports Claude/Codex hooks for live session tracking.
- Shows a TUI dashboard for live hook data and recent local agent sessions.
- Generates shell completion snippets for Bash, Zsh, and Fish.

## Install

```bash
git clone https://github.com/denysbutenko/git-warp
cd git-warp
cargo build --release
cargo install --path .
warp --help
```

Run the setup check before using it in another repository:

```bash
warp doctor
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

Protected branches default to `main`, `master`, and `develop`. Cleanup also
checks for dirty worktrees and running processes before removing anything.

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

Environment overrides use the `GIT_WARP_` prefix:

```bash
export GIT_WARP_TERMINAL_MODE=window
export GIT_WARP_USE_COW=false
export GIT_WARP_AUTO_CONFIRM=true
export GIT_WARP_WORKTREES_PATH=/custom/worktrees
```

`terminal.init_commands` run after Git-Warp changes into the worktree for
terminal handoff modes that print or send shell commands.

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

Useful manual checks:

```bash
cargo run -- --help
cargo run -- doctor
cargo run -- --dry-run switch docs-check
cargo run -- --dry-run cleanup --mode merged
```

## Documentation

- [User Guide](docs/user-guide.md)
- [Technical Overview](docs/technical-overview.md)
- [Documentation Index](docs/README.md)
- [Original autowt reference](docs/autowt.txt)
- [Original coworktree reference](docs/coworktree.txt)

## Status

Git-Warp is usable for local worktree workflows, but it is still evolving. Prefer
`warp doctor`, `--dry-run`, and focused command help (`warp <command> --help`)
when setting it up on a new machine or repository.

## License

MIT.
