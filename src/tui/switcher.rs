use super::ViewOutcome;
use crate::tui::agents::truncate_label;
use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use crate::{config::GitConfig, error::Result, git::WorktreeInfo};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{collections::BTreeSet, io, path::PathBuf, time::SystemTime};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeRuntimeStatus {
    pub path: PathBuf,
    pub is_current: bool,
    pub is_dirty: bool,
    pub is_occupied: bool,
    pub last_touched: Option<SystemTime>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeSwitchTarget {
    pub branch: Option<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeRemovalTarget {
    pub branch: String,
    pub path: PathBuf,
    pub force: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeRemovalSkip {
    pub branch_label: String,
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeBatchRemoval {
    pub targets: Vec<WorktreeRemovalTarget>,
    pub skipped: Vec<WorktreeRemovalSkip>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorktreeSwitchAction {
    Switch(WorktreeSwitchTarget),
    Remove(WorktreeRemovalTarget),
    RemoveMany(WorktreeBatchRemoval),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum WorktreeRemovalBlock {
    Primary,
    Protected,
    Detached,
    Current,
    Dirty,
    Occupied,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeSwitchRow {
    pub branch_label: String,
    pub path_label: String,
    pub badges: Vec<String>,
    pub target: WorktreeSwitchTarget,
    pub removal_blockers: Vec<WorktreeRemovalBlock>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeSwitchDisplayRow {
    pub display_line: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WorktreeSwitchModel {
    pub rows: Vec<WorktreeSwitchRow>,
    pub empty_state_lines: Vec<String>,
}

impl WorktreeSwitchModel {
    pub fn target_at(&self, index: usize) -> Option<WorktreeSwitchTarget> {
        self.rows.get(index).map(|row| row.target.clone())
    }

    pub fn removal_at(&self, index: usize) -> Option<WorktreeRemovalTarget> {
        let row = self.rows.get(index)?;
        if has_hard_blocker(&row.removal_blockers) {
            return None;
        }

        let force = row
            .removal_blockers
            .iter()
            .any(|blocker| is_soft_blocker(*blocker));

        Some(WorktreeRemovalTarget {
            branch: row.target.branch.clone()?,
            path: row.target.path.clone(),
            force,
        })
    }

    pub fn batch_removal_at(&self, indices: &[usize]) -> Option<WorktreeBatchRemoval> {
        let unique_indices = indices.iter().copied().collect::<BTreeSet<_>>();
        if unique_indices.is_empty() {
            return None;
        }

        let mut targets = Vec::new();
        let mut skipped = Vec::new();

        for index in unique_indices {
            let Some(row) = self.rows.get(index) else {
                continue;
            };

            if let Some(target) = self.removal_at(index) {
                targets.push(target);
            } else {
                let reason = if row.removal_blockers.is_empty() {
                    "no local branch".to_string()
                } else {
                    removal_blocker_summary(&row.removal_blockers)
                };
                skipped.push(WorktreeRemovalSkip {
                    branch_label: row.branch_label.clone(),
                    path: row.target.path.clone(),
                    reason,
                });
            }
        }

        if targets.is_empty() && skipped.is_empty() {
            return None;
        }

        Some(WorktreeBatchRemoval { targets, skipped })
    }
}

pub fn is_soft_blocker(blocker: WorktreeRemovalBlock) -> bool {
    matches!(blocker, WorktreeRemovalBlock::Dirty)
}

pub fn has_hard_blocker(blockers: &[WorktreeRemovalBlock]) -> bool {
    blockers.iter().any(|blocker| !is_soft_blocker(*blocker))
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum PendingWorktreeRemoval {
    Single(WorktreeRemovalTarget),
    Batch(WorktreeBatchRemoval),
}

/// Terminal-free controller for the worktree switcher view. Owns selection and
/// pending-removal state and turns key presses into `ViewOutcome`s. The shell
/// (or `WorktreeSwitchTui`) owns the terminal and the event loop.
pub struct WorktreeSwitchView {
    model: WorktreeSwitchModel,
    selected_index: usize,
    selected_indices: BTreeSet<usize>,
    pending_remove: Option<PendingWorktreeRemoval>,
    notice: Option<String>,
}

impl WorktreeSwitchView {
    pub fn new(model: WorktreeSwitchModel) -> Self {
        Self {
            model,
            selected_index: 0,
            selected_indices: BTreeSet::new(),
            pending_remove: None,
            notice: None,
        }
    }

    #[allow(dead_code)] // Part of the controller's public API; used by the shell in later tasks.
    pub fn set_notice(&mut self, message: String) {
        self.notice = Some(message);
    }

    pub fn draw(&self, f: &mut Frame) {
        let selected_indices_list = self.selected_indices.iter().copied().collect::<Vec<_>>();
        draw_worktree_switcher(
            f,
            &self.model,
            self.selected_index,
            &selected_indices_list,
            self.pending_remove.as_ref(),
            self.notice.as_deref(),
        );
    }

    pub fn handle_key(&mut self, code: KeyCode) -> ViewOutcome {
        if let Some(removal) = self.pending_remove.clone() {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    return ViewOutcome::Action(match removal {
                        PendingWorktreeRemoval::Single(target) => {
                            WorktreeSwitchAction::Remove(target)
                        }
                        PendingWorktreeRemoval::Batch(batch) => {
                            WorktreeSwitchAction::RemoveMany(batch)
                        }
                    });
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.pending_remove = None;
                    self.notice = Some("Removal cancelled".to_string());
                }
                KeyCode::Char('q') => return ViewOutcome::Quit,
                _ => {}
            }
            return ViewOutcome::Consumed;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Esc => return ViewOutcome::Quit,
            KeyCode::Tab => return ViewOutcome::ToggleView,
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = self.selected_index.saturating_sub(1);
                self.notice = None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_index =
                    (self.selected_index + 1).min(self.model.rows.len().saturating_sub(1));
                self.notice = None;
            }
            KeyCode::Char(' ') => {
                if !self.selected_indices.insert(self.selected_index) {
                    self.selected_indices.remove(&self.selected_index);
                }
                self.notice = Some(format!(
                    "{} worktree{} selected",
                    self.selected_indices.len(),
                    if self.selected_indices.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
            KeyCode::Char('a') => {
                if self.selected_indices.len() == self.model.rows.len() {
                    self.selected_indices.clear();
                } else {
                    self.selected_indices = (0..self.model.rows.len()).collect();
                }
                self.notice = Some(format!(
                    "{} worktree{} selected",
                    self.selected_indices.len(),
                    if self.selected_indices.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
            KeyCode::Enter => {
                return self
                    .model
                    .target_at(self.selected_index)
                    .map(WorktreeSwitchAction::Switch)
                    .map(ViewOutcome::Action)
                    .unwrap_or(ViewOutcome::Consumed);
            }
            KeyCode::Char('d') | KeyCode::Delete => {
                if !self.selected_indices.is_empty() {
                    let selected_indices_list =
                        self.selected_indices.iter().copied().collect::<Vec<_>>();
                    if let Some(batch) = self.model.batch_removal_at(&selected_indices_list) {
                        if batch.targets.is_empty() {
                            let skipped = batch
                                .skipped
                                .iter()
                                .map(|skip| format!("{} ({})", skip.branch_label, skip.reason))
                                .collect::<Vec<_>>()
                                .join(", ");
                            self.notice =
                                Some(format!("No selected worktrees can be removed: {skipped}"));
                        } else {
                            self.notice = Some(batch_removal_notice(&batch));
                            self.pending_remove = Some(PendingWorktreeRemoval::Batch(batch));
                        }
                    } else {
                        self.notice = Some("No worktrees selected".to_string());
                    }
                } else if let Some(removal) = self.model.removal_at(self.selected_index) {
                    self.notice = Some(if removal.force {
                        format!(
                            "⚠  Worktree '{}' is dirty — uncommitted changes will be lost. Force remove? y/N",
                            removal.branch
                        )
                    } else {
                        format!(
                            "Remove '{}' and delete its local branch? y/N",
                            removal.branch
                        )
                    });
                    self.pending_remove = Some(PendingWorktreeRemoval::Single(removal));
                } else if let Some(row) = self.model.rows.get(self.selected_index) {
                    let reason = if row.removal_blockers.is_empty() {
                        "no local branch".to_string()
                    } else {
                        removal_blocker_summary(&row.removal_blockers)
                    };
                    self.notice = Some(format!("Cannot remove '{}': {}", row.branch_label, reason));
                }
            }
            _ => {}
        }
        ViewOutcome::Consumed
    }
}

pub struct WorktreeSwitchTui {
    model: WorktreeSwitchModel,
}

impl WorktreeSwitchTui {
    pub fn new(model: WorktreeSwitchModel) -> Self {
        Self { model }
    }

    pub fn run(&self) -> Result<Option<WorktreeSwitchAction>> {
        if self.model.rows.is_empty() {
            for line in &self.model.empty_state_lines {
                println!("{line}");
            }
            return Ok(None);
        }

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
            Ok(target) => {
                cleanup_result?;
                cursor_result?;
                Ok(target)
            }
        }
    }

    fn run_app(
        &self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<Option<WorktreeSwitchAction>> {
        let mut view = WorktreeSwitchView::new(self.model.clone());

        loop {
            terminal.draw(|f| view.draw(f))?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match view.handle_key(key.code) {
                    ViewOutcome::Action(action) => return Ok(Some(action)),
                    ViewOutcome::Quit => return Ok(None),
                    ViewOutcome::ToggleView | ViewOutcome::Consumed => {}
                }
            }
        }
    }
}

fn draw_worktree_switcher(
    f: &mut Frame,
    model: &WorktreeSwitchModel,
    selected_index: usize,
    selected_indices: &[usize],
    pending_remove: Option<&PendingWorktreeRemoval>,
    notice: Option<&str>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(7),
            Constraint::Length(3),
        ])
        .split(f.area());

    let selected_count = selected_indices.len();
    let header_text = if selected_count == 0 {
        format!("Warp Worktrees ({})", model.rows.len())
    } else {
        format!(
            "Warp Worktrees ({}; {} selected)",
            model.rows.len(),
            selected_count
        )
    };
    let header = Paragraph::new(header_text)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let items = build_worktree_switch_rows(model, selected_indices)
        .into_iter()
        .map(|row| ListItem::new(Line::from(row.display_line)))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(Block::default().title("Branches").borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    let mut list_state = ListState::default();
    list_state.select(Some(selected_index));
    f.render_stateful_widget(list, chunks[1], &mut list_state);

    if let Some(row) = model.rows.get(selected_index) {
        let branch = row.target.branch.as_deref().unwrap_or("-");
        let status = if row.badges.is_empty() {
            "-".to_string()
        } else {
            row.badges.join(", ")
        };
        let removal_status = if let Some(removal) = pending_remove {
            match removal {
                PendingWorktreeRemoval::Single(removal) => {
                    format!("confirm remove {} (y/N)", removal.branch)
                }
                PendingWorktreeRemoval::Batch(batch) => {
                    format!(
                        "confirm batch remove {} worktree{} (y/N)",
                        batch.targets.len(),
                        if batch.targets.len() == 1 { "" } else { "s" }
                    )
                }
            }
        } else if row.removal_blockers.is_empty() && row.target.branch.is_some() {
            "available".to_string()
        } else if row.removal_blockers.is_empty() {
            "blocked: no local branch".to_string()
        } else {
            format!(
                "blocked: {}",
                removal_blocker_summary(&row.removal_blockers)
            )
        };
        let notice_line = notice
            .map(|message| format!("\nNote: {message}"))
            .unwrap_or_default();
        let details = Paragraph::new(format!(
            "Branch: {}\nPath: {}\nStatus: {}\nRemove: {}\nSelected: {}{}",
            branch,
            row.target.path.display(),
            status,
            removal_status,
            selected_count,
            notice_line
        ))
        .block(Block::default().title("Details").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
        f.render_widget(details, chunks[2]);
    }

    let help = Paragraph::new(
        "↑↓/jk: Navigate | Space: Select | a: All | Enter: Switch | d/Del: Remove selected/current | q/Esc: Quit",
    )
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[3]);
}

fn batch_removal_notice(batch: &WorktreeBatchRemoval) -> String {
    let mut parts = vec![format!(
        "Remove {} selected worktree{}",
        batch.targets.len(),
        if batch.targets.len() == 1 { "" } else { "s" }
    )];

    let targets = batch
        .targets
        .iter()
        .map(|target| {
            if target.force {
                format!(
                    "{} ({}) [dirty — force]",
                    target.branch,
                    target.path.display()
                )
            } else {
                format!("{} ({})", target.branch, target.path.display())
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if !targets.is_empty() {
        parts.push(format!("targets: {targets}"));
    }

    if batch.targets.iter().any(|target| target.force) {
        parts
            .push("⚠  dirty worktrees will be force-removed; uncommitted changes lost".to_string());
    }

    if !batch.skipped.is_empty() {
        let skipped = batch
            .skipped
            .iter()
            .map(|skip| format!("{} ({})", skip.branch_label, skip.reason))
            .collect::<Vec<_>>()
            .join("; ");
        parts.push(format!("skipped: {skipped}"));
    }

    format!("{}? y/N", parts.join("; "))
}

#[allow(dead_code)] // Convenience wrapper used by unit tests via the library crate.
pub fn build_worktree_switch_model(
    worktrees: &[WorktreeInfo],
    statuses: &[WorktreeRuntimeStatus],
) -> WorktreeSwitchModel {
    build_worktree_switch_model_with_protected_branches(
        worktrees,
        statuses,
        &GitConfig::default().protected_branches,
    )
}

#[allow(dead_code)] // Convenience wrapper used by tui_tests integration tests.
pub fn build_worktree_switch_model_with_protected_branches(
    worktrees: &[WorktreeInfo],
    statuses: &[WorktreeRuntimeStatus],
    protected_branches: &[String],
) -> WorktreeSwitchModel {
    build_worktree_switch_model_with_metadata(worktrees, statuses, protected_branches, &[])
}

pub fn build_worktree_switch_model_with_metadata(
    worktrees: &[WorktreeInfo],
    statuses: &[WorktreeRuntimeStatus],
    protected_branches: &[String],
    local_only_branches: &[String],
) -> WorktreeSwitchModel {
    let mut rows = worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| {
            let status = statuses.iter().find(|status| status.path == worktree.path);
            let is_detached = worktree.branch.trim().is_empty() || worktree.is_detached;
            let is_protected = is_protected_branch(&worktree.branch, protected_branches);
            let is_local_only = !is_detached
                && !worktree.branch.is_empty()
                && local_only_branches
                    .iter()
                    .any(|name| name == &worktree.branch);
            let removal_blockers = worktree_removal_blockers(worktree, status, protected_branches);
            let mut badges = Vec::new();

            if worktree.is_primary {
                badges.push("primary".to_string());
            }
            if is_protected {
                badges.push("protected".to_string());
            }
            if is_detached {
                badges.push("detached".to_string());
            }
            if is_local_only {
                badges.push("local-only".to_string());
            }
            if status.is_some_and(|status| status.is_current) {
                badges.push("current".to_string());
            }
            if status.is_some_and(|status| status.is_dirty) {
                badges.push("dirty".to_string());
            }
            if status.is_some_and(|status| status.is_occupied) {
                badges.push("occupied".to_string());
            }

            (
                index,
                status.and_then(|status| status.last_touched),
                WorktreeSwitchRow {
                    branch_label: worktree_branch_label(worktree),
                    path_label: worktree.path.display().to_string(),
                    badges,
                    target: WorktreeSwitchTarget {
                        branch: (!is_detached).then(|| worktree.branch.clone()),
                        path: worktree.path.clone(),
                    },
                    removal_blockers,
                },
            )
        })
        .collect::<Vec<_>>();

    rows.sort_by(|(left_index, left_time, _), (right_index, right_time, _)| {
        right_time
            .cmp(left_time)
            .then_with(|| left_index.cmp(right_index))
    });
    let rows = rows.into_iter().map(|(_, _, row)| row).collect::<Vec<_>>();

    let empty_state_lines = if rows.is_empty() {
        vec![
            "No Git worktrees found for this repository.".to_string(),
            "Run `warp switch <branch>` to create one (local or remote branches both work)."
                .to_string(),
        ]
    } else {
        Vec::new()
    };

    WorktreeSwitchModel {
        rows,
        empty_state_lines,
    }
}

fn worktree_branch_label(worktree: &WorktreeInfo) -> String {
    if worktree.branch.trim().is_empty() {
        let head = worktree.head.chars().take(8).collect::<String>();
        let head = if head.is_empty() {
            "unknown".to_string()
        } else {
            head
        };
        format!("(detached HEAD: {head})")
    } else {
        worktree.branch.clone()
    }
}

fn worktree_removal_blockers(
    worktree: &WorktreeInfo,
    status: Option<&WorktreeRuntimeStatus>,
    protected_branches: &[String],
) -> Vec<WorktreeRemovalBlock> {
    let mut blockers = Vec::new();
    let is_detached = worktree.branch.trim().is_empty() || worktree.is_detached;

    if worktree.is_primary {
        blockers.push(WorktreeRemovalBlock::Primary);
    }
    if is_protected_branch(&worktree.branch, protected_branches) {
        blockers.push(WorktreeRemovalBlock::Protected);
    }
    if is_detached {
        blockers.push(WorktreeRemovalBlock::Detached);
    }
    if worktree.is_current || status.is_some_and(|status| status.is_current) {
        blockers.push(WorktreeRemovalBlock::Current);
    }
    if status.is_some_and(|status| status.is_dirty) {
        blockers.push(WorktreeRemovalBlock::Dirty);
    }
    if status.is_some_and(|status| status.is_occupied) {
        blockers.push(WorktreeRemovalBlock::Occupied);
    }

    blockers
}

fn is_protected_branch(branch: &str, protected_branches: &[String]) -> bool {
    protected_branches
        .iter()
        .any(|protected_branch| protected_branch.trim() == branch)
}

fn removal_blocker_label(blocker: WorktreeRemovalBlock) -> &'static str {
    match blocker {
        WorktreeRemovalBlock::Primary => "primary",
        WorktreeRemovalBlock::Protected => "protected",
        WorktreeRemovalBlock::Detached => "detached",
        WorktreeRemovalBlock::Current => "current",
        WorktreeRemovalBlock::Dirty => "dirty",
        WorktreeRemovalBlock::Occupied => "occupied",
    }
}

fn removal_blocker_summary(blockers: &[WorktreeRemovalBlock]) -> String {
    blockers
        .iter()
        .map(|blocker| removal_blocker_label(*blocker))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn build_worktree_switch_rows(
    model: &WorktreeSwitchModel,
    selected_indices: &[usize],
) -> Vec<WorktreeSwitchDisplayRow> {
    let selected_indices = selected_indices.iter().copied().collect::<BTreeSet<_>>();

    model
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let checked = if selected_indices.contains(&index) {
                "[x]"
            } else {
                "[ ]"
            };
            let badges = if row.badges.is_empty() {
                String::new()
            } else {
                format!(" [{}]", row.badges.join(", "))
            };
            let display_line = format!(
                "{checked} {:<30} {:<28} {}",
                truncate_label(&row.branch_label, 30),
                truncate_label(&badges, 28),
                row.path_label
            );

            WorktreeSwitchDisplayRow { display_line }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_row_model() -> WorktreeSwitchModel {
        WorktreeSwitchModel {
            rows: vec![WorktreeSwitchRow {
                branch_label: "feature".to_string(),
                path_label: "/tmp/wt".to_string(),
                badges: vec![],
                target: WorktreeSwitchTarget {
                    branch: Some("feature".to_string()),
                    path: std::path::PathBuf::from("/tmp/wt"),
                },
                removal_blockers: vec![],
            }],
            empty_state_lines: vec![],
        }
    }

    #[test]
    fn switch_view_tab_requests_toggle() {
        let mut view = WorktreeSwitchView::new(single_row_model());
        assert_eq!(
            view.handle_key(KeyCode::Tab),
            super::super::ViewOutcome::ToggleView
        );
    }

    #[test]
    fn switch_view_quit_on_q_and_esc() {
        let mut view = WorktreeSwitchView::new(single_row_model());
        assert_eq!(
            view.handle_key(KeyCode::Char('q')),
            super::super::ViewOutcome::Quit
        );
        assert_eq!(
            view.handle_key(KeyCode::Esc),
            super::super::ViewOutcome::Quit
        );
    }

    #[test]
    fn switch_view_enter_returns_switch_action() {
        let mut view = WorktreeSwitchView::new(single_row_model());
        match view.handle_key(KeyCode::Enter) {
            super::super::ViewOutcome::Action(WorktreeSwitchAction::Switch(target)) => {
                assert_eq!(target.branch.as_deref(), Some("feature"));
            }
            other => panic!("expected Switch action, got {other:?}"),
        }
    }

    #[test]
    fn switch_view_tab_swallowed_during_pending_removal() {
        let mut view = WorktreeSwitchView::new(single_row_model());
        view.pending_remove = Some(PendingWorktreeRemoval::Single(WorktreeRemovalTarget {
            branch: "feature".to_string(),
            path: std::path::PathBuf::from("/tmp/wt"),
            force: false,
        }));
        assert_eq!(
            view.handle_key(KeyCode::Tab),
            super::super::ViewOutcome::Consumed
        );
    }
}
