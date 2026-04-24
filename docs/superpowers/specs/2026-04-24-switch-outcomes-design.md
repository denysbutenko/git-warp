# Switch Outcome Reporting Design

## Source Of Truth

GitHub issue #5, "[P1] Report switch/create outcomes as verified steps", requires `warp switch` to report the worktree creation, branch checkout, terminal handoff, and manual fallback as explicit outcomes. The final message must be downgraded when part of the flow only partially succeeds, and failure states must point users to the next command.

## Brainstorming

### Options Considered

1. Keep the existing inline `println!` calls and add a few more messages.
   - Fastest, but it keeps success and warning logic spread through `handle_switch`.
   - Harder to test because the final state is implicit in message ordering.

2. Add a small switch outcome reporter in `src/cli.rs`.
   - Tracks each step as `done`, `skipped`, or `warning`.
   - Keeps the happy path short while making partial failures explicit.
   - Fits the current CLI structure without broader architecture churn.

3. Push outcome state into `TerminalManager` and `GitRepository`.
   - Gives stronger typed boundaries, but issue #5 is about the user-facing switch flow.
   - Higher risk and touches stable shared APIs without enough payoff.

Recommended approach: option 2. It gives verified, testable output while staying scoped to the switch command.

## Design

`handle_switch` will build an outcome report while it performs the existing steps:

- Worktree creation:
  - Existing worktree: report that creation was skipped because the path already exists.
  - New worktree: report creation as completed only after the worktree path exists.
- Branch checkout:
  - Traditional worktree creation: report checkout as completed because `git worktree add` completed successfully.
  - CoW enhancement: after cloning and path rewriting, run `git checkout <branch>`. A checkout failure must be captured as a warning outcome, not only a log entry.
  - Existing worktree: verify the branch by running `git -C <worktree> branch --show-current`; report a warning if it does not match the requested branch.
- Terminal handoff:
  - Successful terminal switch: report handoff as completed.
  - Terminal failure: report handoff as warning and include the error text.
- Manual fallback:
  - Hidden on the fully successful path.
  - Printed when any warning outcome exists, with `cd '<worktree>'` as the next command.

The output should stay compact on the happy path. Partial failures should show an "Incomplete switch" summary with explicit step lines and the fallback command.

## Error Handling

Hard failures that prevent worktree creation still return an error as they do today. Recoverable post-create setup warnings and terminal handoff failures do not fail the command; they degrade the final summary and give the manual recovery command.

## Testing

Add integration coverage around `warp switch` using the existing fake command strategy:

- Existing worktree with mismatched requested branch reports skipped creation, checkout warning, terminal handoff, fallback command, and incomplete summary.
- Terminal handoff failure reports creation and checkout as completed, handoff warning, fallback command, and incomplete summary.

Run focused integration tests, formatting, and diff whitespace checks before publishing.
