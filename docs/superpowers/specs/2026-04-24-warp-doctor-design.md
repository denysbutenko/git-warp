# Warp Doctor Design

## Issue

GitHub issue #6 asks for a first-run onboarding command, such as `warp doctor`, that lets a new user run one command and understand what is configured and what to do next.

## Design

Add a read-only `warp doctor` command to the existing CLI. The command should avoid changing config, hooks, worktrees, or branches. It should inspect the current machine and repository state, print short status lines for each onboarding area, then end with a tailored next-step checklist.

The report covers:

- Repository detection: show the current repo root when inside a Git repository, or explain that worktree commands need to run inside one.
- Worktree base path: show the configured worktree base path when set, otherwise show the default path Git-Warp would use for the current repository.
- Copy-on-Write support: check the worktree base path with the existing `cow::is_cow_supported` helper and show whether Git-Warp can use CoW there.
- Terminal support: show the configured terminal app and default terminal mode, then note whether the selected app is usable directly or will rely on auto/fallback behavior.
- Hooks status: check Claude and Codex user/project hook config files for Git-Warp hooks and summarize whether live agent monitoring is already wired.
- Config path: show the config file path and whether it exists.

## Output

The command prints:

1. A heading: `Git-Warp Doctor`.
2. A `Checks` section with concise pass/warn/info status rows.
3. A `Next steps` section that only includes actions relevant to the current state.

Example next steps:

- `Run warp config --edit` when the config file is missing.
- `Run warp hooks-install --level user --runtime all` when no user hooks are installed.
- `Run warp switch <branch>` inside a Git repository after setup is ready.
- `Run this command inside a Git repository` when repo detection fails.

## Testing

Add focused CLI integration coverage for:

- `warp doctor` outside a Git repository prints config path, a repo warning, and a next step to run inside a repository.
- `warp doctor` inside a Git repository prints repository and worktree path checks.
- `warp doctor --help` exposes the new command.

The implementation should be intentionally narrow: no interactive prompts, no config mutation, no hook installation, and no TUI.
