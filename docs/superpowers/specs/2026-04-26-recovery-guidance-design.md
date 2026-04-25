# Recovery Guidance Design

## Source Of Truth

GitHub issue #8, "[P1] Turn common failures into actionable recovery guidance", asks Git-Warp to explain what users should do next for frequent failures. The requested cases are: not in a Git repository, branch/worktree already in use, terminal automation failure, hooks missing, processes blocking cleanup, and Copy-on-Write unavailable. Normal output should stay concise, while raw internals should remain a debug concern.

## Brainstorming

### Options Considered

1. Add a global rich error formatter.
   - This would cover hard failures consistently, but it touches the executable boundary and risks changing every command's error shape at once.

2. Add focused hints at the failure sites that already print user-facing output.
   - This is smaller and testable with the current integration harness.
   - It keeps command-specific advice near the command that knows the correct fallback.

3. Expand `warp doctor` only.
   - Helpful for setup, but it would not improve live failures during `switch` and `cleanup`.

Recommended approach: option 2, plus a small shared helper for hard "not in repo" errors. It gives immediate recovery guidance where users see the failure without turning normal output into a troubleshooting manual.

## Design

Add short, command-oriented hints to the existing CLI surfaces:

- Not in a Git repository:
  - Return `Not in a Git repository. Run this command inside a Git repository, or use \`cd <repo>\` first.`
  - Apply this to `switch`, `ls`, `cleanup`, `agents`, and related repo-bound commands through a helper.
- Branch/worktree already in use:
  - When `warp switch --path <existing-path>` finds a different branch at that path, keep the warning but add `Use a different --path or run \`warp ls\` to inspect worktrees.`
- Terminal automation failure:
  - Keep the existing incomplete switch summary and `cd` fallback.
  - Add `Retry with \`--terminal echo\` to print manual commands instead.`
- Hooks missing:
  - `warp doctor` already reports `warp hooks-install --level user --runtime all`; keep that as the setup hint.
- Processes blocking cleanup:
  - Keep the skip behavior, but add explicit alternatives: `warp cleanup --kill ...`, `--force`, or stop the process manually.
- Copy-on-Write unavailable:
  - `warp doctor` should say Git-Warp will fall back to traditional worktree creation and suggest `warp switch --no-cow <branch>` when users want to skip CoW checks.

## Error Handling

Hints should not change exit codes. Hard errors remain hard errors; recoverable switch/cleanup warnings stay in normal output. The hints must be concise and should not dump raw command output unless existing debug logging is enabled.

## Testing

Use focused integration tests:

- `cli_surface_tests`: repo-bound command outside a Git repo prints the new "cd repo" guidance.
- `terminal_switch_tests`: existing worktree branch mismatch prints the branch/worktree hint.
- `terminal_switch_tests`: terminal handoff failure prints the `--terminal echo` hint.
- `cli_surface_tests`: `warp doctor` outside a repo prints the hooks hint and the `--no-cow` fallback.

Run focused tests, formatting, and diff whitespace checks before committing.
