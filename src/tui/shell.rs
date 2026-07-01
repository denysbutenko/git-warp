use crate::agents::AgentDiscovery;
use crate::error::Result;
use crate::tui::ViewOutcome;
use crate::tui::agents::{AgentsView, clamp_refresh_interval};
use crate::tui::switcher::{WorktreeSwitchAction, WorktreeSwitchModel, WorktreeSwitchView};
use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use chrono::Local;
use crossterm::event::{self, Event, KeyEventKind, poll};
use ratatui::{Terminal as RatatuiTerminal, backend::CrosstermBackend};
use std::{io, time::Duration};

/// Which view the unified `warp` TUI is currently showing.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum View {
    Worktrees,
    Agents,
}

/// Decide the next view (and any user-facing notice) when `Tab` is pressed.
/// Toggling into the agents view is blocked when agent monitoring is disabled.
fn resolve_toggle(current: View, agents_enabled: bool) -> (View, Option<&'static str>) {
    match current {
        View::Worktrees if agents_enabled => (View::Agents, None),
        View::Worktrees => (
            View::Worktrees,
            Some("Agent monitoring disabled (agent.enabled=false)"),
        ),
        View::Agents => (View::Worktrees, None),
    }
}

/// The bare `warp` TUI: a worktree switcher that can toggle to the agents
/// dashboard and back within a single terminal session.
pub struct WarpTui {
    worktrees: WorktreeSwitchView,
    agents: AgentsView,
    agents_enabled: bool,
    view: View,
}

impl WarpTui {
    pub fn new(
        model: WorktreeSwitchModel,
        discovery: AgentDiscovery,
        refresh_rate_ms: u64,
        agents_enabled: bool,
    ) -> Self {
        Self {
            worktrees: WorktreeSwitchView::new(model),
            agents: AgentsView::with_refresh_interval(
                discovery,
                clamp_refresh_interval(refresh_rate_ms),
            ),
            agents_enabled,
            view: View::Worktrees,
        }
    }

    pub fn run(&mut self) -> Result<Option<WorktreeSwitchAction>> {
        let mut terminal_guard = TuiTerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = RatatuiTerminal::new(backend)?;

        let run_result = self.run_app(&mut terminal);
        let cleanup_result = terminal_guard.restore();
        let cursor_result: Result<()> = terminal.show_cursor().map_err(Into::into);
        drop(terminal);

        match run_result {
            Err(err) => {
                let mut follow_on_errors = Vec::new();
                if let Err(cleanup_err) = cleanup_result {
                    follow_on_errors.push(cleanup_err);
                }
                if let Err(cursor_err) = cursor_result {
                    follow_on_errors.push(cursor_err);
                }
                if follow_on_errors.is_empty() {
                    Err(err)
                } else {
                    Err(combine_errors(err, follow_on_errors))
                }
            }
            Ok(action) => {
                cleanup_result?;
                cursor_result?;
                Ok(action)
            }
        }
    }

    fn toggle_view(&mut self) {
        let (next, notice) = resolve_toggle(self.view, self.agents_enabled);
        self.view = next;
        if let Some(message) = notice {
            self.worktrees.set_notice(message.to_string());
        }
    }

    fn run_app(
        &mut self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<WorktreeSwitchAction>> {
        loop {
            if matches!(self.view, View::Agents) {
                self.agents.maybe_refresh()?;
            }

            terminal.draw(|f| match self.view {
                View::Worktrees => self.worktrees.draw(f),
                View::Agents => self.agents.draw(f, Local::now()),
            })?;

            let timeout = Duration::from_millis(100);
            if poll(timeout)?
                && let Event::Key(key) = event::read()?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                let outcome = match self.view {
                    View::Worktrees => self.worktrees.handle_key(key.code),
                    View::Agents => self.agents.handle_key(key.code),
                };
                match outcome {
                    ViewOutcome::Action(action) => return Ok(Some(action)),
                    ViewOutcome::Quit => return Ok(None),
                    ViewOutcome::ToggleView => self.toggle_view(),
                    ViewOutcome::Consumed => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_from_worktrees_opens_agents_when_enabled() {
        let (next, notice) = resolve_toggle(View::Worktrees, true);
        assert_eq!(next, View::Agents);
        assert!(notice.is_none());
    }

    #[test]
    fn toggle_from_worktrees_blocked_when_disabled() {
        let (next, notice) = resolve_toggle(View::Worktrees, false);
        assert_eq!(next, View::Worktrees);
        assert!(notice.is_some());
    }

    #[test]
    fn toggle_from_agents_returns_to_worktrees() {
        let (next, notice) = resolve_toggle(View::Agents, false);
        assert_eq!(next, View::Worktrees);
        assert!(notice.is_none());
    }
}
