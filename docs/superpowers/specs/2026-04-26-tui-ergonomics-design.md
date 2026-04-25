# TUI Ergonomics Design

GitHub issue #9, "[P2] Improve TUI legibility and keyboard ergonomics", asks the cleanup and agents TUIs to become easier to scan and operate on first use. The current screens work, but they lean on compact symbols, footer-only controls, and inline cleanup rendering that is hard to test.

## Approaches Considered

1. Full TUI redesign with new layout primitives.
   - This could produce a larger visual jump, but it would be high-risk for a small issue and hard to verify without snapshot infrastructure.
2. Model-first semantic polish.
   - Add explicit labels, status descriptions, and reusable row text builders, then let existing ratatui screens render those improved model fields.
   - This is the recommended approach because existing tests already cover TUI models.
3. Documentation-only guidance.
   - This would explain controls, but it would not improve first-use operation inside the actual screens.

## Design

Agents dashboard rows will keep their compact status symbol, but every row will also carry a plain-language state label such as `working`, `waiting`, or `recent`. The row text should show runtime, state, location, agent, and relative time so users do not need a legend to decode symbols. Details will also include the plain state. The empty state will explicitly say that there are no sessions to show, mention the 7-day recent-history window, and point to hook installation for live monitoring.

Cleanup TUI row rendering will move from anonymous inline string formatting into a small `CleanupRow` model. Each row will expose a checkbox, branch, reason, remote label, dirty label, and a combined display line. The cleanup screen will use text-backed status words (`merged`, `identical`, `remote`, `no remote`, `dirty`) instead of emoji-only meaning. The title and footer will name the important actions directly: `Space` toggles a row, `a` toggles all rows, `Enter` confirms, and `q/Esc` cancels.

Keyboard behavior will add two low-cost affordances where the current screens are stiff: `j/k` will mirror down/up navigation for agents, switcher, and cleanup; cleanup will support `a` to select or clear all candidates; cleanup will support `Esc` as cancel. These keys supplement the existing controls and do not remove current behavior.

## Boundaries

This change will not introduce terminal screenshot tests, new dependencies, async event handling, or a new TUI architecture. Verification should stay focused on unit tests for model semantics plus the existing TUI-adjacent test suite.

## Testing

Add failing unit tests first for:

- agents dashboard rows exposing text status labels and clearer empty-state copy,
- session detail lines including plain state,
- cleanup row display explaining candidate reason, remote status, and dirty state,
- cleanup selection helper deciding whether `a` selects all or clears all.

Then update the TUI rendering and keyboard handling until those tests pass, followed by `cargo fmt --check`, focused unit tests, and `git diff --check`.
