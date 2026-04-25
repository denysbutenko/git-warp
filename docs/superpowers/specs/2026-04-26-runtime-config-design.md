# Runtime Config Behavior Design

Issue #7 asks Git-Warp to make documented configuration values affect command behavior, not only serialization and display. The broad config surface already loads and displays many values, and recent work has wired `terminal_mode`, `terminal.app`, and custom worktree paths into `warp switch`. The remaining high-value gap is terminal launch behavior: `terminal.auto_activate` and `terminal.init_commands` exist in config but do not change the generated terminal handoff.

## Approach

Use the existing `warp switch` handoff path as the single integration point. `Cli::record_terminal_handoff` will pass the loaded `TerminalConfig` into `TerminalManager`, and terminal backends will build launch commands from a shared launch-options value. This keeps CLI precedence intact: explicit `--terminal` still chooses the mode, config still supplies the terminal app and launch options, and custom `--path` continues to override configured worktree base paths.

## Runtime Behavior

- `terminal.init_commands` runs after the generated `cd <worktree>` command for shell-printing modes, current mode, and macOS AppleScript tab/window handoff.
- Echo and inplace modes print shell commands that can be evaluated by shell integrations.
- iTerm2 and Terminal.app AppleScript writes the `cd` command first, then each configured init command.
- `terminal.auto_activate = true` adds an AppleScript `activate` command for iTerm2 and Terminal.app. `false` omits it.
- Warp URI handoff keeps using the URI launch path for tab/window creation; init commands are not appended to the URI because Git-Warp's current Warp URI action only carries a path.

## Config Visibility

`warp config --show` will include `terminal.init_commands`, using `[]` when no commands are configured. README and the user guide will document that init commands run after switching into the worktree and that auto-activation affects macOS AppleScript terminal backends.

## Tests

Integration coverage should prove the actual CLI path:

- `warp switch` with config `terminal_mode = "echo"` prints configured init commands.
- Terminal.app AppleScript includes `activate` when `auto_activate = true`.
- Terminal.app AppleScript omits `activate` when `auto_activate = false`.
