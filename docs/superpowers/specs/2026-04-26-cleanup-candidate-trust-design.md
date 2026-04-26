# Cleanup Candidate Trust Design

## Goal

Make `warp cleanup` explain and remove the same branches in non-interactive and interactive flows, using the repository's actual cleanup base branch instead of guessing common branch names.

## Design

Cleanup analysis resolves a single base branch from live repository state. It prefers `origin/HEAD` when available, then falls back to the primary worktree branch, then the configured `git.default_branch` only when neither live signal is available. Protected branches remain excluded from cleanup, but they no longer drive merged/identical decisions.

CLI cleanup builds one filtered candidate list for the chosen mode. The list includes plain reason labels such as `merged`, `identical`, `no remote`, `clean`, or `dirty`, so the user can see why a branch is eligible or why a branch is skipped. Interactive cleanup receives that already-filtered candidate list, so the TUI cannot show a branch that the CLI flow would later discard.

## Testing

Add regression coverage for a repository whose primary branch is not `main`, proving a merged branch is recognized against that real base branch. Add unit coverage that cleanup row text exposes the same reason labels used by CLI and TUI rendering.
