use crate::tui::agents::truncate_label;
use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use crate::{error::Result, git::BranchStatus};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use std::io;

#[allow(dead_code)] // Row metadata is asserted in unit tests via `display_line` only.
pub struct CleanupRow {
    pub branch: String,
    pub path_label: String,
    pub reason_label: &'static str,
    pub remote_label: &'static str,
    pub dirty_label: &'static str,
    pub display_line: String,
}

pub struct CleanupTui {
    candidates: Option<Vec<BranchStatus>>,
}

impl Default for CleanupTui {
    fn default() -> Self {
        Self::new()
    }
}

impl CleanupTui {
    pub fn new() -> Self {
        Self { candidates: None }
    }

    pub fn with_candidates(candidates: Vec<BranchStatus>) -> Self {
        Self {
            candidates: Some(candidates),
        }
    }

    pub fn run(&self) -> Result<Vec<String>> {
        use crate::config::ConfigManager;
        use crate::git::GitRepository;

        let git_repo =
            GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a git repository"))?;
        let config_manager = ConfigManager::new()?;
        let config = config_manager.get().git.clone();
        let worktrees = git_repo.list_worktrees()?;
        let branch_statuses = match &self.candidates {
            Some(candidates) => candidates.clone(),
            None => git_repo.analyze_branches_for_cleanup_with_config(&worktrees, &config)?,
        };

        if branch_statuses.is_empty() {
            println!("âœ¨ No worktrees found that can be cleaned up!");
            return Ok(vec![]);
        }

        let mut terminal_guard = TuiTerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = RatatuiTerminal::new(backend)?;

        let run_result = self.interactive_select(&mut terminal, &branch_statuses);
        let cleanup_result = terminal_guard.restore();
        let cursor_result: Result<()> = terminal.show_cursor().map_err(Into::into);
        drop(terminal);

        let (confirmed, selected_branches) = match run_result {
            Err(err) => {
                let mut follow_on_errors = Vec::new();
                if let Err(cleanup_err) = cleanup_result {
                    follow_on_errors.push(cleanup_err);
                }
                if let Err(cursor_err) = cursor_result {
                    follow_on_errors.push(cursor_err);
                }
                return if follow_on_errors.is_empty() {
                    Err(err)
                } else {
                    Err(combine_errors(err, follow_on_errors))
                };
            }
            Ok(result) => {
                cleanup_result?;
                cursor_result?;
                result
            }
        };

        if !confirmed {
            return Ok(vec![]);
        }

        let selected: Vec<String> = branch_statuses
            .iter()
            .enumerate()
            .filter_map(|(i, status)| {
                if selected_branches[i] {
                    Some(status.branch.clone())
                } else {
                    None
                }
            })
            .collect();

        Ok(selected)
    }

    fn interactive_select(
        &self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
        branch_statuses: &[BranchStatus],
    ) -> Result<(bool, Vec<bool>)> {
        let mut selected_index = 0usize;
        let mut selected_branches: Vec<bool> = vec![false; branch_statuses.len()];
        let mut should_quit = false;
        let mut confirmed = false;

        loop {
            terminal.draw(|f| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .margin(1)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(0),
                        Constraint::Length(4),
                    ])
                    .split(f.area());

                let header = Paragraph::new(format!(
                    "Worktree cleanup ({} candidates)",
                    branch_statuses.len()
                ))
                    .style(Style::default().fg(Color::Yellow))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(header, chunks[0]);

                let rows = build_cleanup_rows(branch_statuses, &selected_branches);
                let items: Vec<ListItem> = rows
                    .iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let style = if i == selected_index {
                            Style::default().bg(Color::Blue).fg(Color::White)
                        } else {
                            Style::default()
                        };

                        ListItem::new(row.display_line.clone()).style(style)
                    })
                    .collect();

                let list = List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Space toggles a branch; Enter removes selected branches"),
                );
                f.render_widget(list, chunks[1]);

                let selected_count = selected_branches.iter().filter(|&&x| x).count();
                let footer_text = format!(
                    "â†‘â†“/jk: Navigate | Space: Toggle | a: Toggle all | Enter: Confirm ({} selected) | q/Esc: Cancel",
                    selected_count
                );
                let footer = Paragraph::new(footer_text)
                    .style(Style::default().fg(Color::Gray))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title("Controls"));
                f.render_widget(footer, chunks[2]);
            })?;

            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        should_quit = true;
                        break;
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected_index = selected_index.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if selected_index < branch_statuses.len() - 1 =>
                    {
                        selected_index += 1;
                    }
                    KeyCode::Char(' ') => {
                        selected_branches[selected_index] = !selected_branches[selected_index];
                    }
                    KeyCode::Char('a') => {
                        let select_all = next_bulk_selection_state(&selected_branches);
                        selected_branches.fill(select_all);
                    }
                    KeyCode::Enter => {
                        confirmed = true;
                        break;
                    }
                    _ => {}
                }
            }
        }

        if should_quit {
            confirmed = false;
        }

        Ok((confirmed, selected_branches))
    }
}

pub fn build_cleanup_rows(statuses: &[BranchStatus], selected: &[bool]) -> Vec<CleanupRow> {
    statuses
        .iter()
        .enumerate()
        .map(|(index, status)| {
            let checked = if selected.get(index).copied().unwrap_or(false) {
                "[x]"
            } else {
                "[ ]"
            };
            let reason_label = cleanup_reason_label(status);
            let remote_label = if status.has_remote {
                "remote"
            } else {
                "no remote"
            };
            let dirty_label = if status.has_uncommitted_changes {
                "dirty"
            } else {
                "clean"
            };
            let path_label = status.path.display().to_string();
            let display_line = format!(
                "{checked} {:<28} {:<9} {:<9} {:<5} {}",
                truncate_label(&status.branch, 28),
                reason_label,
                remote_label,
                dirty_label,
                path_label
            );

            CleanupRow {
                branch: status.branch.clone(),
                path_label,
                reason_label,
                remote_label,
                dirty_label,
                display_line,
            }
        })
        .collect()
}

pub fn next_bulk_selection_state(selected: &[bool]) -> bool {
    !selected.is_empty() && !selected.iter().all(|is_selected| *is_selected)
}

pub fn cleanup_reason_label(status: &BranchStatus) -> &'static str {
    if status.is_merged {
        "merged"
    } else if status.is_identical {
        "identical"
    } else if !status.has_remote {
        "no remote"
    } else {
        "candidate"
    }
}

/// Same as [`cleanup_reason_label`] but disambiguates the generic `candidate`
/// fallback when a branch is included only because the caller passed
/// `--mode all` or `--mode interactive`.
pub fn cleanup_reason_label_for_mode(status: &BranchStatus, mode: &str) -> &'static str {
    let base = cleanup_reason_label(status);
    if base == "candidate" && matches!(mode, "all" | "interactive") {
        "all-mode"
    } else {
        base
    }
}
