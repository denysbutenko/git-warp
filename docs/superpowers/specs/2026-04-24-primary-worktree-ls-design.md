# Primary Worktree Detection And `warp ls` Status Design

Source of truth: GitHub issue #3, fetched on 2026-04-24 from `denysbutenko/git-warp`.

## Problem

`warp ls` currently trusts the `bare` line from `git worktree list --porcelain` to identify the primary worktree. Normal non-bare repositories do not emit that line, so the main checkout is displayed like a linked worktree. The list also omits important state, which forces users to run follow-up commands before deciding whether to switch, clean up, or avoid a worktree.

## Goals

- Mark the main checkout as primary in a normal non-bare repository.
- Mark the worktree that contains the current process as current.
- Show compact `warp ls` labels for high-signal state: primary, current, dirty, detached, and process-occupied.
- Keep cleanup behavior safer by ensuring primary worktrees are excluded from branch cleanup analysis.
- Preserve the existing command shape: `warp ls`, `warp list`, and `warp ls --debug`.

## Non-Goals

- Redesign `warp ls` into a table or TUI.
- Add JSON output.
- Change cleanup modes or branch deletion policy beyond fixing primary detection input.
- Add expensive process details to every `ls` line.

## Design

`GitRepository::list_worktrees` remains the source for structural worktree facts. It will parse porcelain output as it does today, then normalize paths and enrich each entry with:

- `is_primary`: true for the first porcelain worktree entry, and also true if Git emits `bare`.
- `is_current`: true when the worktree path matches the repository root discovered from the current directory.
- `is_detached`: true when Git emits `detached` or when no branch name is present.

`warp ls` will layer volatile status on top of those facts:

- `dirty`: from `git status --porcelain` for that worktree.
- `busy`: from `ProcessManager`, but only for non-current worktrees so the shell running `warp ls` does not make the current line noisy.

Output stays line-oriented and compact:

```text
🏠  main [primary] /repo
🌿  feature/demo [current dirty] /repo/.worktrees/feature-demo
🌿  (detached: abc12345) [detached] /repo/.worktrees/detached
```

`--debug` continues to show HEAD and primary state, and will also show current and detached state for troubleshooting.

## Testing

- Unit coverage in `tests/unit/git_tests.rs` will verify that a normal one-worktree repository marks its main checkout as both primary and current.
- Unit coverage will verify that when the command is run from a linked worktree, the first/main checkout remains primary while the linked checkout is current.
- Integration coverage in `tests/integration/cli_surface_tests.rs` will verify that `warp ls` prints compact labels for primary, current, dirty, and detached worktrees.

## Risks

- Process detection can be noisy because the current shell is itself a process in the current worktree. The design avoids this by only showing `busy` for non-current worktrees.
- `git worktree list --porcelain` ordering is relied on for primary detection. This matches Git's worktree listing behavior and is also backed by preserving the existing `bare` marker handling.
