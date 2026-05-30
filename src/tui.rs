use crate::{
    agents::{
        AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
        sort_session_summaries,
    },
    config::{Config, ConfigManager, GitConfig},
    error::Result,
    git::{BranchStatus, WorktreeInfo},
};
use chrono::{DateTime, Duration as ChronoDuration, Local};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, poll},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

/// Minimum allowed agent dashboard refresh interval. Faster than this and the
/// dashboard churns the file system reading session state.
const MIN_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
/// Fallback when the caller does not pass a configured refresh rate.
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

fn clamp_refresh_interval(refresh_rate_ms: u64) -> Duration {
    Duration::from_millis(refresh_rate_ms).max(MIN_REFRESH_INTERVAL)
}

struct TuiTerminalGuard {
    active: bool,
}

impl TuiTerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(err) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
            return Err(rollback_terminal_entry(
                err.into(),
                disable_raw_mode,
                || {
                    let mut rollback_stdout = io::stdout();
                    execute!(rollback_stdout, LeaveAlternateScreen, DisableMouseCapture)
                },
            ));
        }

        Ok(Self { active: true })
    }

    fn restore(&mut self) -> Result<()> {
        let (active, result) = terminal_cleanup_attempt(self.active, disable_raw_mode, || {
            let mut stdout = io::stdout();
            execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)
        });
        self.active = active;
        result
    }
}

impl Drop for TuiTerminalGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }

        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen, DisableMouseCapture);
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DashboardRow {
    pub session: AgentSessionSummary,
    pub state_symbol: &'static str,
    pub state_label: &'static str,
    pub runtime_label: &'static str,
    pub location_label: String,
    pub agent_label: String,
    pub relative_time: String,
    pub is_stale: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DashboardModel {
    pub rows: Vec<DashboardRow>,
    pub total_rows: usize,
    pub total_unfiltered: usize,
    pub start_index: usize,
    pub empty_state_lines: Vec<String>,
    pub filter_summary: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum AgentRuntimeFilter {
    #[default]
    All,
    Claude,
    Codex,
}

impl AgentRuntimeFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Claude,
            Self::Claude => Self::Codex,
            Self::Codex => Self::All,
        }
    }

    fn matches(self, runtime: AgentRuntime) -> bool {
        match self {
            Self::All => true,
            Self::Claude => runtime == AgentRuntime::Claude,
            Self::Codex => runtime == AgentRuntime::Codex,
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Claude => Some("Claude"),
            Self::Codex => Some("Codex"),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub enum AgentPresenceFilter {
    #[default]
    All,
    Live,
    Recent,
}

impl AgentPresenceFilter {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Live,
            Self::Live => Self::Recent,
            Self::Recent => Self::All,
        }
    }

    fn matches(self, session: &AgentSessionSummary, now: DateTime<Local>) -> bool {
        match self {
            Self::All => true,
            Self::Live => session.is_live,
            Self::Recent => !session.is_live && !is_stale_session(session, now),
        }
    }

    fn label(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Live => Some("live"),
            Self::Recent => Some("recent"),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct DashboardFilters {
    pub runtime: AgentRuntimeFilter,
    pub presence: AgentPresenceFilter,
}

impl DashboardFilters {
    pub fn is_active(&self) -> bool {
        self.runtime != AgentRuntimeFilter::All || self.presence != AgentPresenceFilter::All
    }

    pub fn summary(&self) -> String {
        let parts: Vec<&'static str> = [self.runtime.label(), self.presence.label()]
            .into_iter()
            .flatten()
            .collect();
        if parts.is_empty() {
            String::new()
        } else {
            format!("Filter: {}", parts.join(" · "))
        }
    }
}

const STALE_THRESHOLD_HOURS: i64 = 24;

pub fn is_stale_session(session: &AgentSessionSummary, now: DateTime<Local>) -> bool {
    if session.is_live {
        return false;
    }
    now.signed_duration_since(session.last_activity) > ChronoDuration::hours(STALE_THRESHOLD_HOURS)
}

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CleanupRow {
    pub branch: String,
    pub path_label: String,
    pub reason_label: &'static str,
    pub remote_label: &'static str,
    pub dirty_label: &'static str,
    pub display_line: String,
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

fn is_soft_blocker(blocker: WorktreeRemovalBlock) -> bool {
    matches!(blocker, WorktreeRemovalBlock::Dirty)
}

fn has_hard_blocker(blockers: &[WorktreeRemovalBlock]) -> bool {
    blockers.iter().any(|blocker| !is_soft_blocker(*blocker))
}

pub struct TuiApp {
    should_quit: bool,
    selected_index: usize,
    last_refresh: Instant,
    refresh_interval: Duration,
    discovery: AgentDiscovery,
    sessions: Vec<AgentSessionSummary>,
    filters: DashboardFilters,
}

impl TuiApp {
    pub fn with_refresh_interval(discovery: AgentDiscovery, refresh_interval: Duration) -> Self {
        let refresh_interval = refresh_interval.max(MIN_REFRESH_INTERVAL);
        Self {
            should_quit: false,
            selected_index: 0,
            last_refresh: Instant::now() - refresh_interval,
            refresh_interval,
            discovery,
            sessions: Vec::new(),
            filters: DashboardFilters::default(),
        }
    }

    pub fn run(&mut self) -> Result<()> {
        let mut terminal_guard = TuiTerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = RatatuiTerminal::new(backend)?;

        let run_result = self
            .refresh_sessions()
            .and_then(|_| self.run_app(&mut terminal));
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
            Ok(()) => {
                cleanup_result?;
                cursor_result
            }
        }
    }

    fn run_app(
        &mut self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        loop {
            if self.last_refresh.elapsed() >= self.refresh_interval {
                self.refresh_sessions()?;
            }

            terminal.draw(|f| self.draw_agents_dashboard(f, Local::now()))?;

            // Non-blocking event check
            let timeout = Duration::from_millis(100);
            if poll(timeout)?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('q') => {
                        self.should_quit = true;
                    }
                    KeyCode::Esc => {
                        self.should_quit = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') if self.selected_index > 0 => {
                        self.selected_index -= 1;
                    }
                    KeyCode::Down | KeyCode::Char('j')
                        if self.selected_index < self.filtered_count().saturating_sub(1) =>
                    {
                        self.selected_index += 1;
                    }
                    KeyCode::Char('r') => {
                        self.refresh_sessions()?;
                    }
                    KeyCode::Char('t') => {
                        self.filters.runtime = self.filters.runtime.next();
                        self.selected_index = 0;
                    }
                    KeyCode::Char('p') => {
                        self.filters.presence = self.filters.presence.next();
                        self.selected_index = 0;
                    }
                    KeyCode::Char('c') => {
                        self.filters = DashboardFilters::default();
                        self.selected_index = 0;
                    }
                    _ => {}
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn refresh_sessions(&mut self) -> Result<()> {
        self.sessions = self.discovery.discover(Local::now())?;
        let filtered = self.filtered_count();
        if filtered == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(filtered - 1);
        }
        self.last_refresh = Instant::now();
        Ok(())
    }

    fn filtered_count(&self) -> usize {
        let now = Local::now();
        self.sessions
            .iter()
            .filter(|session| {
                self.filters.runtime.matches(session.runtime)
                    && self.filters.presence.matches(session, now)
            })
            .count()
    }

    fn draw_agents_dashboard(&self, f: &mut Frame, now: DateTime<Local>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(8),    // Main content
                Constraint::Length(3), // Help
            ])
            .split(f.area());

        let preview_model = build_dashboard_model_filtered_windowed(
            &self.sessions,
            now,
            self.selected_index,
            1,
            self.filters,
        );
        let header_text = if self.filters.is_active() {
            format!(
                "Warp Agents ({}/{}) — {}",
                preview_model.total_rows,
                preview_model.total_unfiltered,
                preview_model.filter_summary
            )
        } else {
            format!("Warp Agents ({})", preview_model.total_unfiltered)
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

        if preview_model.total_rows == 0 {
            let empty_model =
                build_dashboard_model_filtered_windowed(&self.sessions, now, 0, 1, self.filters);
            let empty_state = Paragraph::new(empty_model.empty_state_lines.join("\n\n"))
                .block(Block::default().title("No Sessions").borders(Borders::ALL))
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray))
                .wrap(Wrap { trim: false });
            f.render_widget(empty_state, chunks[1]);
        } else {
            let content_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
                .split(chunks[1]);
            let visible_capacity = content_chunks[0].height.saturating_sub(2).max(1) as usize;
            let model = build_dashboard_model_filtered_windowed(
                &self.sessions,
                now,
                self.selected_index,
                visible_capacity,
                self.filters,
            );

            let session_items: Vec<ListItem> = model
                .rows
                .iter()
                .map(|row| {
                    let stale_marker = if row.is_stale { "~" } else { " " };
                    let text = format!(
                        "{}{} {:<6} {:<10} {:<18} {:<18} {}",
                        stale_marker,
                        row.state_symbol,
                        row.runtime_label,
                        truncate_label(row.state_label, 10),
                        truncate_label(&row.location_label, 18),
                        truncate_label(&row.agent_label, 18),
                        row.relative_time
                    );
                    let style = if row.is_stale {
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM)
                    } else {
                        Style::default().fg(session_state_color(row.session.state))
                    };
                    ListItem::new(Line::from(text)).style(style)
                })
                .collect();

            let sessions_list = List::new(session_items)
                .block(Block::default().title("Sessions").borders(Borders::ALL))
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol(">> ");
            let mut list_state = ListState::default();
            list_state.select(Some(self.selected_index.saturating_sub(model.start_index)));
            f.render_stateful_widget(sessions_list, content_chunks[0], &mut list_state);

            if let Some(selected_row) = model
                .rows
                .get(self.selected_index.saturating_sub(model.start_index))
            {
                let details =
                    Paragraph::new(session_detail_lines(&selected_row.session).join("\n"))
                        .block(Block::default().title("Details").borders(Borders::ALL))
                        .style(Style::default().fg(Color::White))
                        .wrap(Wrap { trim: false });
                f.render_widget(details, content_chunks[1]);
            }
        }

        // Help
        let help_text =
            "↑↓/jk: Navigate | r: Refresh | t: Runtime | p: Presence | c: Clear | q/Esc: Quit";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[2]);
    }
}

#[allow(dead_code)] // Convenience wrapper used by tui_tests integration tests.
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

#[allow(dead_code)] // Convenience wrapper used by tui_tests integration tests.
pub fn build_dashboard_model(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
) -> DashboardModel {
    let mut ordered_sessions = sessions.to_vec();
    sort_session_summaries(&mut ordered_sessions);
    build_dashboard_model_windowed(&ordered_sessions, now, 0, ordered_sessions.len().max(1))
}

#[allow(dead_code)] // Convenience wrapper used by tui_tests integration tests.
pub fn build_dashboard_model_windowed(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
    selected_index: usize,
    visible_capacity: usize,
) -> DashboardModel {
    build_dashboard_model_filtered_windowed(
        sessions,
        now,
        selected_index,
        visible_capacity,
        DashboardFilters::default(),
    )
}

pub fn build_dashboard_model_filtered_windowed(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
    selected_index: usize,
    visible_capacity: usize,
    filters: DashboardFilters,
) -> DashboardModel {
    let total_unfiltered = sessions.len();
    let filtered: Vec<AgentSessionSummary> = sessions
        .iter()
        .filter(|session| {
            filters.runtime.matches(session.runtime) && filters.presence.matches(session, now)
        })
        .cloned()
        .collect();

    let total_rows = filtered.len();
    let visible_capacity = visible_capacity.max(1).min(total_rows.max(1));
    let selected_index = selected_index.min(total_rows.saturating_sub(1));
    let start_index = dashboard_window_start(total_rows, selected_index, visible_capacity);
    let end_index = start_index.saturating_add(visible_capacity).min(total_rows);

    let rows = filtered
        .get(start_index..end_index)
        .unwrap_or_default()
        .iter()
        .cloned()
        .map(|session| DashboardRow {
            state_symbol: session_state_symbol(session.state),
            state_label: session_state_label(session.state),
            runtime_label: runtime_label(session.runtime),
            location_label: session_location_label(&session),
            agent_label: session.agent_label.clone(),
            relative_time: relative_time_label(session.last_activity, now),
            is_stale: is_stale_session(&session, now),
            session,
        })
        .collect::<Vec<_>>();

    let empty_state_lines = if rows.is_empty() {
        if filters.is_active() && total_unfiltered > 0 {
            vec![
                format!(
                    "No sessions match the active filter ({}/{} hidden).",
                    total_unfiltered, total_unfiltered
                ),
                "Press `t` to cycle runtime, `p` for presence, or `c` to clear.".to_string(),
            ]
        } else {
            vec![
                "No agent sessions to show for this repository.".to_string(),
                "Recent Claude/Codex sessions appear here for 7 days.".to_string(),
                "Hint: run `warp hooks-install --runtime all --level user` to enable live monitoring."
                    .to_string(),
            ]
        }
    } else {
        Vec::new()
    };

    DashboardModel {
        rows,
        total_rows,
        total_unfiltered,
        start_index,
        empty_state_lines,
        filter_summary: filters.summary(),
    }
}

fn dashboard_window_start(
    total_rows: usize,
    selected_index: usize,
    visible_capacity: usize,
) -> usize {
    if total_rows <= visible_capacity {
        return 0;
    }

    let half_window = visible_capacity / 2;
    selected_index
        .saturating_sub(half_window)
        .min(total_rows.saturating_sub(visible_capacity))
}

pub fn session_detail_lines(session: &AgentSessionSummary) -> Vec<String> {
    vec![
        format!("Agent: {}", session.agent_label),
        format!("CWD: {}", session.cwd.display()),
        format!(
            "Session ID: {}",
            session.session_id.as_deref().unwrap_or("-")
        ),
        format!("Runtime: {}", runtime_label(session.runtime)),
        format!("Branch: {}", session.branch.as_deref().unwrap_or("-")),
        format!("State: {}", session_state_label(session.state)),
        format!(
            "Presence: {}",
            if session.is_live { "live" } else { "recent" }
        ),
        format!("Last Activity: {}", session.last_activity.to_rfc3339()),
        format!("Source: {}", source_label(session.source)),
    ]
}

fn session_state_symbol(state: AgentSessionState) -> &'static str {
    match state {
        AgentSessionState::Starting => "â—‹",
        AgentSessionState::Working => "●",
        AgentSessionState::Processing => "◔",
        AgentSessionState::Waiting => "!",
        AgentSessionState::Completed => "✓",
        AgentSessionState::Recent => "○",
        AgentSessionState::Unknown => "?",
    }
}

fn session_state_label(state: AgentSessionState) -> &'static str {
    match state {
        AgentSessionState::Starting => "starting",
        AgentSessionState::Working => "working",
        AgentSessionState::Processing => "processing",
        AgentSessionState::Waiting => "waiting",
        AgentSessionState::Completed => "complete",
        AgentSessionState::Recent => "recent",
        AgentSessionState::Unknown => "unknown",
    }
}

fn session_state_color(state: AgentSessionState) -> Color {
    match state {
        AgentSessionState::Starting => Color::White,
        AgentSessionState::Working => Color::Green,
        AgentSessionState::Processing => Color::Cyan,
        AgentSessionState::Waiting => Color::Yellow,
        AgentSessionState::Completed => Color::Blue,
        AgentSessionState::Recent => Color::Gray,
        AgentSessionState::Unknown => Color::Red,
    }
}

fn runtime_label(runtime: AgentRuntime) -> &'static str {
    match runtime {
        AgentRuntime::Claude => "Claude",
        AgentRuntime::Codex => "Codex",
    }
}

fn source_label(source: AgentSessionSource) -> &'static str {
    match source {
        AgentSessionSource::LiveStatus => "LiveStatus",
        AgentSessionSource::SessionStore => "SessionStore",
        AgentSessionSource::Merged => "Merged",
    }
}

fn session_location_label(session: &AgentSessionSummary) -> String {
    session
        .branch
        .clone()
        .filter(|branch| !branch.trim().is_empty())
        .unwrap_or_else(|| {
            session
                .cwd
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| session.cwd.display().to_string())
        })
}

fn relative_time_label(last_activity: DateTime<Local>, now: DateTime<Local>) -> String {
    let delta = now.signed_duration_since(last_activity);
    if delta < ChronoDuration::zero() {
        let future_delta = last_activity.signed_duration_since(now);
        if future_delta < ChronoDuration::minutes(1) {
            "in <1m".to_string()
        } else if future_delta < ChronoDuration::hours(1) {
            format!("in {}m", future_delta.num_minutes())
        } else if future_delta < ChronoDuration::days(1) {
            format!("in {}h", future_delta.num_hours())
        } else {
            format!("in {}d", future_delta.num_days())
        }
    } else if delta < ChronoDuration::minutes(1) {
        "just now".to_string()
    } else if delta < ChronoDuration::hours(1) {
        format!("{}m ago", delta.num_minutes())
    } else if delta < ChronoDuration::days(1) {
        format!("{}h ago", delta.num_hours())
    } else {
        format!("{}d ago", delta.num_days())
    }
}

fn truncate_label(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let candidate: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() && max_chars > 3 {
        format!(
            "{}...",
            candidate.chars().take(max_chars - 3).collect::<String>()
        )
    } else {
        candidate
    }
}

fn combine_errors(
    primary: anyhow::Error,
    additional: impl IntoIterator<Item = anyhow::Error>,
) -> anyhow::Error {
    let mut message = primary.to_string();
    for err in additional {
        message.push_str("; ");
        message.push_str(&err.to_string());
    }

    anyhow::anyhow!(message)
}

fn terminal_cleanup_attempt<FDisable, FCleanup>(
    active: bool,
    disable_raw: FDisable,
    cleanup_terminal: FCleanup,
) -> (bool, Result<()>)
where
    FDisable: FnOnce() -> io::Result<()>,
    FCleanup: FnOnce() -> io::Result<()>,
{
    if !active {
        return (false, Ok(()));
    }

    let mut errors = Vec::new();

    if let Err(err) = disable_raw() {
        errors.push(anyhow::Error::new(err));
    }

    if let Err(err) = cleanup_terminal() {
        errors.push(anyhow::Error::new(err));
    }

    if errors.is_empty() {
        (false, Ok(()))
    } else {
        let first = errors.remove(0);
        (true, Err(combine_errors(first, errors)))
    }
}

fn rollback_terminal_entry<FDisable, FCleanup>(
    primary: anyhow::Error,
    disable_raw: FDisable,
    cleanup_terminal: FCleanup,
) -> anyhow::Error
where
    FDisable: FnOnce() -> io::Result<()>,
    FCleanup: FnOnce() -> io::Result<()>,
{
    let (_, rollback_result) = terminal_cleanup_attempt(true, disable_raw, cleanup_terminal);
    match rollback_result {
        Ok(()) => primary,
        Err(rollback_error) => combine_errors(primary, [rollback_error]),
    }
}

pub struct AgentsDashboard {
    discovery: AgentDiscovery,
    refresh_interval: Duration,
}

impl AgentsDashboard {
    #[allow(dead_code)] // Default constructor used by integration tests and embedders.
    pub fn new(discovery: AgentDiscovery) -> Self {
        Self {
            discovery,
            refresh_interval: DEFAULT_REFRESH_INTERVAL,
        }
    }

    pub fn with_refresh_rate(discovery: AgentDiscovery, refresh_rate_ms: u64) -> Self {
        Self {
            discovery,
            refresh_interval: clamp_refresh_interval(refresh_rate_ms),
        }
    }

    pub fn run(&self) -> Result<()> {
        let mut app = TuiApp::with_refresh_interval(self.discovery.clone(), self.refresh_interval);
        app.run()
    }
}

pub struct WorktreeSwitchTui {
    model: WorktreeSwitchModel,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum PendingWorktreeRemoval {
    Single(WorktreeRemovalTarget),
    Batch(WorktreeBatchRemoval),
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
        let mut selected_index = 0;
        let mut selected_indices = BTreeSet::new();
        let mut pending_remove: Option<PendingWorktreeRemoval> = None;
        let mut notice: Option<String> = None;

        loop {
            let selected_indices_list = selected_indices.iter().copied().collect::<Vec<_>>();
            terminal.draw(|f| {
                draw_worktree_switcher(
                    f,
                    &self.model,
                    selected_index,
                    &selected_indices_list,
                    pending_remove.as_ref(),
                    notice.as_deref(),
                )
            })?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if let Some(removal) = pending_remove.clone() {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                            return Ok(Some(match removal {
                                PendingWorktreeRemoval::Single(target) => {
                                    WorktreeSwitchAction::Remove(target)
                                }
                                PendingWorktreeRemoval::Batch(batch) => {
                                    WorktreeSwitchAction::RemoveMany(batch)
                                }
                            }));
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            pending_remove = None;
                            notice = Some("Removal cancelled".to_string());
                        }
                        KeyCode::Char('q') => return Ok(None),
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                    KeyCode::Up | KeyCode::Char('k') => {
                        selected_index = selected_index.saturating_sub(1);
                        notice = None;
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected_index =
                            (selected_index + 1).min(self.model.rows.len().saturating_sub(1));
                        notice = None;
                    }
                    KeyCode::Char(' ') => {
                        if !selected_indices.insert(selected_index) {
                            selected_indices.remove(&selected_index);
                        }
                        notice = Some(format!(
                            "{} worktree{} selected",
                            selected_indices.len(),
                            if selected_indices.len() == 1 { "" } else { "s" }
                        ));
                    }
                    KeyCode::Char('a') => {
                        if selected_indices.len() == self.model.rows.len() {
                            selected_indices.clear();
                        } else {
                            selected_indices = (0..self.model.rows.len()).collect();
                        }
                        notice = Some(format!(
                            "{} worktree{} selected",
                            selected_indices.len(),
                            if selected_indices.len() == 1 { "" } else { "s" }
                        ));
                    }
                    KeyCode::Enter => {
                        return Ok(self
                            .model
                            .target_at(selected_index)
                            .map(WorktreeSwitchAction::Switch));
                    }
                    KeyCode::Char('d') | KeyCode::Delete => {
                        if !selected_indices.is_empty() {
                            let selected_indices_list =
                                selected_indices.iter().copied().collect::<Vec<_>>();
                            if let Some(batch) = self.model.batch_removal_at(&selected_indices_list)
                            {
                                if batch.targets.is_empty() {
                                    let skipped = batch
                                        .skipped
                                        .iter()
                                        .map(|skip| {
                                            format!("{} ({})", skip.branch_label, skip.reason)
                                        })
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    notice = Some(format!(
                                        "No selected worktrees can be removed: {skipped}"
                                    ));
                                } else {
                                    notice = Some(batch_removal_notice(&batch));
                                    pending_remove = Some(PendingWorktreeRemoval::Batch(batch));
                                }
                            } else {
                                notice = Some("No worktrees selected".to_string());
                            }
                        } else if let Some(removal) = self.model.removal_at(selected_index) {
                            notice = Some(if removal.force {
                                format!(
                                    "⚠️  Worktree '{}' is dirty — uncommitted changes will be lost. Force remove? y/N",
                                    removal.branch
                                )
                            } else {
                                format!(
                                    "Remove '{}' and delete its local branch? y/N",
                                    removal.branch
                                )
                            });
                            pending_remove = Some(PendingWorktreeRemoval::Single(removal));
                        } else if let Some(row) = self.model.rows.get(selected_index) {
                            let reason = if row.removal_blockers.is_empty() {
                                "no local branch".to_string()
                            } else {
                                removal_blocker_summary(&row.removal_blockers)
                            };
                            notice =
                                Some(format!("Cannot remove '{}': {}", row.branch_label, reason));
                        }
                    }
                    _ => {}
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
        parts.push(
            "⚠️  dirty worktrees will be force-removed; uncommitted changes lost".to_string(),
        );
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
        use ratatui::{Terminal, backend::CrosstermBackend};
        use std::io;

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
            println!("✨ No worktrees found that can be cleaned up!");
            return Ok(vec![]);
        }

        let mut terminal_guard = TuiTerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

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
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
        branch_statuses: &[BranchStatus],
    ) -> Result<(bool, Vec<bool>)> {
        use crossterm::event::{self, Event, KeyCode, KeyEventKind};
        use ratatui::{
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Style},
            widgets::{Block, Borders, List, ListItem, Paragraph},
        };

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
                    "↑↓/jk: Navigate | Space: Toggle | a: Toggle all | Enter: Confirm ({} selected) | q/Esc: Cancel",
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ConfigSectionId {
    General,
    Git,
    Process,
    Terminal,
    Agent,
    PostCreate,
}

impl ConfigSectionId {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Git => "Git",
            Self::Process => "Process",
            Self::Terminal => "Terminal",
            Self::Agent => "Agent",
            Self::PostCreate => "Post-create",
        }
    }

    pub fn all() -> &'static [ConfigSectionId] {
        &[
            Self::General,
            Self::Git,
            Self::Process,
            Self::Terminal,
            Self::Agent,
            Self::PostCreate,
        ]
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ConfigFieldId {
    TerminalMode,
    WorktreesPath,
    UseCow,
    AutoConfirm,
    GitDefaultBranch,
    GitProtectedBranches,
    GitAutoFetch,
    GitAutoPrune,
    ProcessCheckProcesses,
    ProcessAutoKill,
    ProcessKillTimeout,
    TerminalApp,
    TerminalAutoActivate,
    TerminalInitCommands,
    AgentEnabled,
    AgentRefreshRate,
    AgentMaxActivities,
    PostCreateAutoInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFieldKind {
    Bool,
    Choice { allowed: &'static [&'static str] },
    FreeText,
    OptionPath,
    U64 { min: u64, max: u64 },
    Usize { min: usize, max: usize },
    ReadOnlyList,
}

#[derive(Debug, Clone)]
pub struct ConfigFieldSpec {
    pub id: ConfigFieldId,
    pub section: ConfigSectionId,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: ConfigFieldKind,
}

fn config_field_specs() -> Vec<ConfigFieldSpec> {
    use ConfigFieldId::*;
    use ConfigFieldKind::*;
    use ConfigSectionId::*;
    vec![
        ConfigFieldSpec {
            id: TerminalMode,
            section: General,
            label: "terminal_mode",
            help: "Default terminal launch mode for worktree switches.",
            kind: Choice {
                allowed: &["tab", "window", "current", "inplace", "echo"],
            },
        },
        ConfigFieldSpec {
            id: WorktreesPath,
            section: General,
            label: "worktrees_path",
            help: "Optional override for the worktree base directory. Empty value clears it.",
            kind: OptionPath,
        },
        ConfigFieldSpec {
            id: UseCow,
            section: General,
            label: "use_cow",
            help: "Use copy-on-write when the filesystem supports it.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: AutoConfirm,
            section: General,
            label: "auto_confirm",
            help: "Skip confirmation prompts on destructive operations.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: GitDefaultBranch,
            section: Git,
            label: "default_branch",
            help: "Default base branch name.",
            kind: FreeText,
        },
        ConfigFieldSpec {
            id: GitProtectedBranches,
            section: Git,
            label: "protected_branches",
            help: "Branches cleanup must never remove. Edit the config file to change.",
            kind: ReadOnlyList,
        },
        ConfigFieldSpec {
            id: GitAutoFetch,
            section: Git,
            label: "auto_fetch",
            help: "Run `git fetch` before operations that need fresh refs.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: GitAutoPrune,
            section: Git,
            label: "auto_prune",
            help: "Prune stale remote-tracking branches during fetch.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessCheckProcesses,
            section: Process,
            label: "check_processes",
            help: "Scan for processes rooted in a worktree before cleanup.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessAutoKill,
            section: Process,
            label: "auto_kill",
            help: "Send termination signals to worktree-rooted processes without prompting.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessKillTimeout,
            section: Process,
            label: "kill_timeout",
            help: "Grace period (seconds) before escalating SIGTERM to SIGKILL.",
            kind: U64 { min: 1, max: 300 },
        },
        ConfigFieldSpec {
            id: TerminalApp,
            section: Terminal,
            label: "app",
            help: "Preferred terminal application for worktree switches.",
            kind: Choice {
                allowed: &["auto", "iterm2", "terminal", "warp"],
            },
        },
        ConfigFieldSpec {
            id: TerminalAutoActivate,
            section: Terminal,
            label: "auto_activate",
            help: "Activate the newly created tab or window.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: TerminalInitCommands,
            section: Terminal,
            label: "init_commands",
            help: "Commands run when entering a worktree. Edit the config file to change.",
            kind: ReadOnlyList,
        },
        ConfigFieldSpec {
            id: AgentEnabled,
            section: Agent,
            label: "enabled",
            help: "Enable the agents dashboard.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: AgentRefreshRate,
            section: Agent,
            label: "refresh_rate",
            help: "Agents dashboard refresh interval (milliseconds).",
            kind: U64 {
                min: 250,
                max: 60_000,
            },
        },
        ConfigFieldSpec {
            id: AgentMaxActivities,
            section: Agent,
            label: "max_activities",
            help: "Maximum number of recent agent activities to track.",
            kind: Usize {
                min: 1,
                max: 10_000,
            },
        },
        ConfigFieldSpec {
            id: PostCreateAutoInstall,
            section: PostCreate,
            label: "auto_install",
            help: "Run the matching package-manager install when a JS lockfile is present.",
            kind: Bool,
        },
    ]
}

#[derive(Debug, Clone)]
struct ConfigEditBuffer {
    field_id: ConfigFieldId,
    value: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfigStatusKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfigStatusMsg {
    pub kind: ConfigStatusKind,
    pub text: String,
}

/// Outcome reported by `ConfigEditorModel::request_quit`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ConfigQuitOutcome {
    /// No unsaved changes — caller may exit immediately.
    Clean,
    /// Working copy diverges from disk — caller must confirm discard.
    NeedsConfirm,
}

#[derive(Debug, Clone)]
pub struct ConfigEditorModel {
    fields: Vec<ConfigFieldSpec>,
    sections: Vec<ConfigSectionId>,
    section_idx: usize,
    field_idx_in_section: usize,
    working: Config,
    original: Config,
    edit_buffer: Option<ConfigEditBuffer>,
    status: Option<ConfigStatusMsg>,
    config_path: PathBuf,
    env_overrides: Vec<String>,
}

impl ConfigEditorModel {
    pub fn from_config(config: Config, config_path: PathBuf) -> Self {
        Self::from_config_with_env(config, config_path, detect_env_overrides())
    }

    pub fn from_config_with_env(
        config: Config,
        config_path: PathBuf,
        env_overrides: Vec<String>,
    ) -> Self {
        let fields = config_field_specs();
        let sections = ConfigSectionId::all().to_vec();
        Self {
            fields,
            sections,
            section_idx: 0,
            field_idx_in_section: 0,
            working: config.clone(),
            original: config,
            edit_buffer: None,
            status: None,
            config_path,
            env_overrides,
        }
    }

    pub fn current_section(&self) -> ConfigSectionId {
        self.sections[self.section_idx]
    }

    pub fn section_idx(&self) -> usize {
        self.section_idx
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn env_overrides(&self) -> &[String] {
        &self.env_overrides
    }

    pub fn status(&self) -> Option<&ConfigStatusMsg> {
        self.status.as_ref()
    }

    pub fn editing(&self) -> bool {
        self.edit_buffer.is_some()
    }

    pub fn edit_buffer_value(&self) -> Option<&str> {
        self.edit_buffer.as_ref().map(|buf| buf.value.as_str())
    }

    #[cfg(test)]
    pub fn working_config(&self) -> &Config {
        &self.working
    }

    pub fn is_dirty(&self) -> bool {
        self.working != self.original
    }

    pub fn fields_in_section(&self, section: ConfigSectionId) -> Vec<&ConfigFieldSpec> {
        self.fields
            .iter()
            .filter(|f| f.section == section)
            .collect()
    }

    pub fn current_field(&self) -> &ConfigFieldSpec {
        let section = self.current_section();
        let fields = self.fields_in_section(section);
        let idx = self
            .field_idx_in_section
            .min(fields.len().saturating_sub(1));
        fields[idx]
    }

    pub fn field_value_display(&self, field: &ConfigFieldSpec) -> String {
        render_field_value(&self.working, field)
    }

    pub fn move_up(&mut self) {
        if self.editing() {
            return;
        }
        self.field_idx_in_section = self.field_idx_in_section.saturating_sub(1);
        self.status = None;
    }

    pub fn move_down(&mut self) {
        if self.editing() {
            return;
        }
        let last = self
            .fields_in_section(self.current_section())
            .len()
            .saturating_sub(1);
        if self.field_idx_in_section < last {
            self.field_idx_in_section += 1;
        }
        self.status = None;
    }

    pub fn next_section(&mut self) {
        if self.editing() {
            return;
        }
        self.section_idx = (self.section_idx + 1) % self.sections.len();
        self.field_idx_in_section = 0;
        self.status = None;
    }

    pub fn prev_section(&mut self) {
        if self.editing() {
            return;
        }
        self.section_idx = if self.section_idx == 0 {
            self.sections.len() - 1
        } else {
            self.section_idx - 1
        };
        self.field_idx_in_section = 0;
        self.status = None;
    }

    pub fn toggle(&mut self) -> bool {
        if self.editing() {
            return false;
        }
        let field = self.current_field().clone();
        if !matches!(field.kind, ConfigFieldKind::Bool) {
            return false;
        }
        match field.id {
            ConfigFieldId::UseCow => self.working.use_cow = !self.working.use_cow,
            ConfigFieldId::AutoConfirm => self.working.auto_confirm = !self.working.auto_confirm,
            ConfigFieldId::GitAutoFetch => {
                self.working.git.auto_fetch = !self.working.git.auto_fetch
            }
            ConfigFieldId::GitAutoPrune => {
                self.working.git.auto_prune = !self.working.git.auto_prune
            }
            ConfigFieldId::ProcessCheckProcesses => {
                self.working.process.check_processes = !self.working.process.check_processes
            }
            ConfigFieldId::ProcessAutoKill => {
                self.working.process.auto_kill = !self.working.process.auto_kill
            }
            ConfigFieldId::TerminalAutoActivate => {
                self.working.terminal.auto_activate = !self.working.terminal.auto_activate
            }
            ConfigFieldId::AgentEnabled => self.working.agent.enabled = !self.working.agent.enabled,
            ConfigFieldId::PostCreateAutoInstall => {
                self.working.post_create.auto_install = !self.working.post_create.auto_install
            }
            _ => return false,
        }
        self.status = None;
        true
    }

    pub fn begin_edit(&mut self) -> bool {
        if self.editing() {
            return false;
        }
        let field = self.current_field().clone();
        let editable = matches!(
            field.kind,
            ConfigFieldKind::Choice { .. }
                | ConfigFieldKind::FreeText
                | ConfigFieldKind::OptionPath
                | ConfigFieldKind::U64 { .. }
                | ConfigFieldKind::Usize { .. }
        );
        if !editable {
            self.status = Some(ConfigStatusMsg {
                kind: ConfigStatusKind::Info,
                text: "Field not editable from the TUI. Edit the config file directly.".into(),
            });
            return false;
        }
        let prefill = match field.kind {
            ConfigFieldKind::OptionPath => match field.id {
                ConfigFieldId::WorktreesPath => self
                    .working
                    .worktrees_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                _ => String::new(),
            },
            _ => self.field_value_display(&field),
        };
        self.edit_buffer = Some(ConfigEditBuffer {
            field_id: field.id,
            value: prefill,
        });
        self.status = None;
        true
    }

    pub fn edit_push_char(&mut self, ch: char) {
        if let Some(buf) = self.edit_buffer.as_mut() {
            buf.value.push(ch);
        }
    }

    pub fn edit_pop_char(&mut self) {
        if let Some(buf) = self.edit_buffer.as_mut() {
            buf.value.pop();
        }
    }

    pub fn cancel_edit(&mut self) {
        self.edit_buffer = None;
        self.status = None;
    }

    pub fn commit_edit(&mut self) -> bool {
        let Some(buf) = self.edit_buffer.clone() else {
            return false;
        };
        let field = self
            .fields
            .iter()
            .find(|f| f.id == buf.field_id)
            .cloned()
            .expect("edit buffer references known field");
        match apply_field_value(&mut self.working, &field, &buf.value) {
            Ok(()) => {
                self.edit_buffer = None;
                self.status = None;
                true
            }
            Err(err) => {
                self.status = Some(ConfigStatusMsg {
                    kind: ConfigStatusKind::Error,
                    text: err,
                });
                false
            }
        }
    }

    pub fn revert(&mut self) {
        self.working = self.original.clone();
        self.edit_buffer = None;
        self.status = Some(ConfigStatusMsg {
            kind: ConfigStatusKind::Info,
            text: "Reverted to last saved values.".into(),
        });
    }

    pub fn save(&mut self, manager: &mut ConfigManager) -> Result<()> {
        manager.config = self.working.clone();
        manager.save_current()?;
        self.original = self.working.clone();
        self.status = Some(ConfigStatusMsg {
            kind: ConfigStatusKind::Success,
            text: format!("Saved to {}", self.config_path.display()),
        });
        Ok(())
    }

    pub fn request_quit(&self) -> ConfigQuitOutcome {
        if self.is_dirty() {
            ConfigQuitOutcome::NeedsConfirm
        } else {
            ConfigQuitOutcome::Clean
        }
    }
}

fn detect_env_overrides() -> Vec<String> {
    std::env::vars()
        .filter_map(|(k, _)| {
            if k.starts_with("GIT_WARP_") {
                Some(k)
            } else {
                None
            }
        })
        .collect()
}

fn render_field_value(config: &Config, field: &ConfigFieldSpec) -> String {
    use ConfigFieldId::*;
    match field.id {
        TerminalMode => config.terminal_mode.clone(),
        WorktreesPath => config
            .worktrees_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into()),
        UseCow => config.use_cow.to_string(),
        AutoConfirm => config.auto_confirm.to_string(),
        GitDefaultBranch => config.git.default_branch.clone(),
        GitProtectedBranches => render_list(&config.git.protected_branches),
        GitAutoFetch => config.git.auto_fetch.to_string(),
        GitAutoPrune => config.git.auto_prune.to_string(),
        ProcessCheckProcesses => config.process.check_processes.to_string(),
        ProcessAutoKill => config.process.auto_kill.to_string(),
        ProcessKillTimeout => config.process.kill_timeout.to_string(),
        TerminalApp => config.terminal.app.clone(),
        TerminalAutoActivate => config.terminal.auto_activate.to_string(),
        TerminalInitCommands => render_list(&config.terminal.init_commands),
        AgentEnabled => config.agent.enabled.to_string(),
        AgentRefreshRate => config.agent.refresh_rate.to_string(),
        AgentMaxActivities => config.agent.max_activities.to_string(),
        PostCreateAutoInstall => config.post_create.auto_install.to_string(),
    }
}

fn render_list(items: &[String]) -> String {
    if items.is_empty() {
        "(empty)".into()
    } else {
        format!("[{}]", items.join(", "))
    }
}

fn apply_field_value(
    config: &mut Config,
    field: &ConfigFieldSpec,
    raw: &str,
) -> std::result::Result<(), String> {
    use ConfigFieldId::*;
    let trimmed = raw.trim();
    match &field.kind {
        ConfigFieldKind::Bool | ConfigFieldKind::ReadOnlyList => {
            Err("Field not editable from the TUI.".into())
        }
        ConfigFieldKind::Choice { allowed } => {
            if !allowed.contains(&trimmed) {
                return Err(format!("Allowed values: {}", allowed.join(", ")));
            }
            match field.id {
                TerminalMode => config.terminal_mode = trimmed.to_string(),
                TerminalApp => config.terminal.app = trimmed.to_string(),
                _ => unreachable!("Choice kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::FreeText => {
            if trimmed.is_empty() {
                return Err("Value must not be empty.".into());
            }
            match field.id {
                GitDefaultBranch => config.git.default_branch = trimmed.to_string(),
                _ => unreachable!("FreeText kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::OptionPath => {
            match field.id {
                WorktreesPath => {
                    config.worktrees_path = if trimmed.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(trimmed))
                    };
                }
                _ => unreachable!("OptionPath kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::U64 { min, max } => {
            let parsed: u64 = trimmed
                .parse()
                .map_err(|_| format!("Enter an integer in [{min}, {max}]."))?;
            if parsed < *min || parsed > *max {
                return Err(format!("Value must be in [{min}, {max}]."));
            }
            match field.id {
                ProcessKillTimeout => config.process.kill_timeout = parsed,
                AgentRefreshRate => config.agent.refresh_rate = parsed,
                _ => unreachable!("U64 kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::Usize { min, max } => {
            let parsed: usize = trimmed
                .parse()
                .map_err(|_| format!("Enter an integer in [{min}, {max}]."))?;
            if parsed < *min || parsed > *max {
                return Err(format!("Value must be in [{min}, {max}]."));
            }
            match field.id {
                AgentMaxActivities => config.agent.max_activities = parsed,
                _ => unreachable!("Usize kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
    }
}

pub struct ConfigTui;

impl Default for ConfigTui {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigTui {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<()> {
        let mut manager = ConfigManager::new()?;
        let config_path = manager.config_path().clone();
        let working = manager.get().clone();
        let mut model = ConfigEditorModel::from_config(working, config_path);

        let mut terminal_guard = TuiTerminalGuard::enter()?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = RatatuiTerminal::new(backend)?;

        let run_result = self.run_loop(&mut terminal, &mut model, &mut manager);
        let cleanup_result = terminal_guard.restore();
        let cursor_result: Result<()> = terminal.show_cursor().map_err(Into::into);
        drop(terminal);

        match run_result {
            Err(err) => {
                let mut follow_on = Vec::new();
                if let Err(e) = cleanup_result {
                    follow_on.push(e);
                }
                if let Err(e) = cursor_result {
                    follow_on.push(e);
                }
                if follow_on.is_empty() {
                    Err(err)
                } else {
                    Err(combine_errors(err, follow_on))
                }
            }
            Ok(()) => {
                cleanup_result?;
                cursor_result?;
                Ok(())
            }
        }
    }

    fn run_loop(
        &self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
        model: &mut ConfigEditorModel,
        manager: &mut ConfigManager,
    ) -> Result<()> {
        let mut pending_quit = false;
        loop {
            terminal.draw(|f| draw_config_editor(f, model, pending_quit))?;

            if let Event::Key(key) = event::read()?
                && key.kind == KeyEventKind::Press
            {
                if pending_quit {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => return Ok(()),
                        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                            pending_quit = false;
                        }
                        _ => {}
                    }
                    continue;
                }

                if model.editing() {
                    match key.code {
                        KeyCode::Esc => model.cancel_edit(),
                        KeyCode::Enter => {
                            model.commit_edit();
                        }
                        KeyCode::Backspace => model.edit_pop_char(),
                        KeyCode::Char(c) => model.edit_push_char(c),
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => match model.request_quit() {
                        ConfigQuitOutcome::Clean => return Ok(()),
                        ConfigQuitOutcome::NeedsConfirm => pending_quit = true,
                    },
                    KeyCode::Up | KeyCode::Char('k') => model.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => model.move_down(),
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => model.next_section(),
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => model.prev_section(),
                    KeyCode::Char(' ') => {
                        model.toggle();
                    }
                    KeyCode::Enter | KeyCode::Char('e') => {
                        let field = model.current_field().clone();
                        if matches!(field.kind, ConfigFieldKind::Bool) {
                            model.toggle();
                        } else {
                            model.begin_edit();
                        }
                    }
                    KeyCode::Char('s') => {
                        if let Err(err) = model.save(manager) {
                            model.status = Some(ConfigStatusMsg {
                                kind: ConfigStatusKind::Error,
                                text: format!("Save failed: {err}"),
                            });
                        }
                    }
                    KeyCode::Char('r') => model.revert(),
                    _ => {}
                }
            }
        }
    }
}

fn draw_config_editor(f: &mut Frame, model: &ConfigEditorModel, pending_quit: bool) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(f.area());

    let dirty_tag = if model.is_dirty() { " [unsaved]" } else { "" };
    let header_text = format!(
        "Git-Warp config editor — {}{}",
        model.config_path().display(),
        dirty_tag
    );
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, outer[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(outer[1]);

    let section_items: Vec<ListItem> = model
        .sections
        .iter()
        .enumerate()
        .map(|(idx, section)| {
            let marker = if idx == model.section_idx() {
                "▸ "
            } else {
                "  "
            };
            let line = format!("{}{}", marker, section.label());
            let style = if idx == model.section_idx() {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(line).style(style)
        })
        .collect();
    let sections = List::new(section_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Sections (Tab)"),
    );
    f.render_widget(sections, body[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(5)])
        .split(body[1]);

    let current_section = model.current_section();
    let fields = model.fields_in_section(current_section);
    let cursor = model
        .field_idx_in_section
        .min(fields.len().saturating_sub(1));
    let field_items: Vec<ListItem> = fields
        .iter()
        .enumerate()
        .map(|(idx, field)| {
            let value = if model.editing() && idx == cursor {
                let buf = model.edit_buffer_value().unwrap_or("");
                format!("> {} = {}_", field.label, buf)
            } else {
                let cursor_mark = if idx == cursor { "▸" } else { " " };
                format!(
                    "{} {} = {}",
                    cursor_mark,
                    field.label,
                    model.field_value_display(field)
                )
            };
            let style = if idx == cursor {
                Style::default().bg(Color::Blue).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(value).style(style)
        })
        .collect();
    let fields_block = List::new(field_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(current_section.label()),
    );
    f.render_widget(fields_block, right[0]);

    let current_field = fields[cursor];
    let mut help_lines = vec![Line::from(current_field.help.to_string())];
    if let ConfigFieldKind::Choice { allowed } = current_field.kind {
        help_lines.push(Line::from(format!("Allowed: {}", allowed.join(", "))));
    }
    if let ConfigFieldKind::U64 { min, max } = current_field.kind {
        help_lines.push(Line::from(format!("Range: {min}..={max}")));
    }
    if let ConfigFieldKind::Usize { min, max } = current_field.kind {
        help_lines.push(Line::from(format!("Range: {min}..={max}")));
    }
    let help = Paragraph::new(help_lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, right[1]);

    let status_text = if pending_quit {
        "Discard unsaved changes? y/N".to_string()
    } else if let Some(status) = model.status() {
        status.text.clone()
    } else if !model.env_overrides().is_empty() {
        format!(
            "Heads up: {} GIT_WARP_* env var(s) set — they shadow saved values at runtime.",
            model.env_overrides().len()
        )
    } else {
        String::new()
    };
    let status_color = if pending_quit {
        Color::Yellow
    } else if let Some(status) = model.status() {
        match status.kind {
            ConfigStatusKind::Success => Color::Green,
            ConfigStatusKind::Error => Color::Red,
            ConfigStatusKind::Info => Color::Cyan,
        }
    } else {
        Color::Gray
    };
    let status = Paragraph::new(status_text)
        .style(Style::default().fg(status_color))
        .block(Block::default().borders(Borders::ALL).title("Status"));
    f.render_widget(status, outer[2]);

    let footer_text = if pending_quit {
        "y discard and quit  n keep editing".to_string()
    } else if model.editing() {
        "Enter save field  Esc cancel  Backspace delete".to_string()
    } else {
        "↑↓/jk move  Tab section  Space toggle  Enter/e edit  s save  r revert  q quit".to_string()
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(footer, outer[3]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io;

    #[test]
    fn test_tui_creation() {
        let _dashboard =
            AgentsDashboard::new(AgentDiscovery::new(vec![PathBuf::from("/tmp/repo")]));
        let _cleanup_tui = CleanupTui::new();
        let _config_tui = ConfigTui::new();
    }

    #[test]
    fn clamp_refresh_interval_enforces_minimum() {
        assert_eq!(clamp_refresh_interval(0), MIN_REFRESH_INTERVAL);
        assert_eq!(clamp_refresh_interval(50), MIN_REFRESH_INTERVAL);
        assert_eq!(clamp_refresh_interval(249), MIN_REFRESH_INTERVAL);
    }

    #[test]
    fn clamp_refresh_interval_preserves_configured() {
        assert_eq!(clamp_refresh_interval(250), Duration::from_millis(250));
        assert_eq!(clamp_refresh_interval(1000), Duration::from_millis(1000));
        assert_eq!(clamp_refresh_interval(5000), Duration::from_millis(5000));
    }

    #[test]
    fn test_build_dashboard_model_empty_state() {
        let model =
            build_dashboard_model(&[], Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap());

        assert!(model.rows.is_empty());
        assert_eq!(model.empty_state_lines.len(), 3);
    }

    #[test]
    fn test_session_detail_lines() {
        let session = AgentSessionSummary {
            runtime: AgentRuntime::Codex,
            session_id: Some("session-123".to_string()),
            cwd: PathBuf::from("/tmp/repo/.worktrees/agents"),
            branch: Some("feat/agents".to_string()),
            agent_label: "Parfit (worker)".to_string(),
            state: AgentSessionState::Working,
            last_activity: Local.with_ymd_and_hms(2026, 4, 23, 11, 0, 0).unwrap(),
            is_live: true,
            source: AgentSessionSource::Merged,
        };

        let lines = session_detail_lines(&session);

        assert!(lines.iter().any(|line| line == "Runtime: Codex"));
        assert!(lines.iter().any(|line| line == "Source: Merged"));
    }

    #[test]
    fn test_terminal_cleanup_attempt_keeps_guard_active_on_failure() {
        let (active, result) =
            terminal_cleanup_attempt(true, || Err(io::Error::other("disable failed")), || Ok(()));

        assert!(active);
        let message = result.expect_err("cleanup should fail").to_string();
        assert!(message.contains("disable failed"));
    }

    #[test]
    fn test_terminal_cleanup_attempt_keeps_guard_active_when_cleanup_fails() {
        let (active, result) =
            terminal_cleanup_attempt(true, || Ok(()), || Err(io::Error::other("cleanup failed")));

        assert!(active);
        let message = result.expect_err("cleanup should fail").to_string();
        assert!(message.contains("cleanup failed"));
    }

    #[test]
    fn test_terminal_cleanup_attempt_deactivates_guard_on_success() {
        let (active, result) = terminal_cleanup_attempt(true, || Ok(()), || Ok(()));

        assert!(!active);
        assert!(result.is_ok());
    }

    #[test]
    fn test_rollback_terminal_entry_combines_primary_and_cleanup_failures() {
        let error = rollback_terminal_entry(
            anyhow::anyhow!("enter failed"),
            || Err(io::Error::other("disable failed")),
            || Err(io::Error::other("leave failed")),
        );

        let message = error.to_string();
        assert!(message.contains("enter failed"));
        assert!(message.contains("disable failed"));
        assert!(message.contains("leave failed"));
    }

    fn fresh_model() -> ConfigEditorModel {
        ConfigEditorModel::from_config_with_env(
            Config::default(),
            PathBuf::from("/tmp/git-warp/config.toml"),
            Vec::new(),
        )
    }

    fn position_on_field(model: &mut ConfigEditorModel, target: ConfigFieldId) {
        let target_field = config_field_specs()
            .into_iter()
            .find(|f| f.id == target)
            .expect("known field");
        // navigate to the right section
        while model.current_section() != target_field.section {
            model.next_section();
        }
        // navigate within the section
        let mut found = false;
        for (idx, field) in model
            .fields_in_section(target_field.section)
            .iter()
            .enumerate()
        {
            if field.id == target {
                model.field_idx_in_section = idx;
                found = true;
                break;
            }
        }
        assert!(found, "target field not present in its section");
    }

    #[test]
    fn config_editor_starts_clean_and_lists_all_sections() {
        let model = fresh_model();
        assert!(!model.is_dirty());
        assert_eq!(model.sections.len(), ConfigSectionId::all().len());
        assert_eq!(model.current_section(), ConfigSectionId::General);
    }

    #[test]
    fn config_editor_toggle_flips_bool_and_marks_dirty() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::UseCow);
        let before = model.working_config().use_cow;
        assert!(model.toggle());
        assert_eq!(model.working_config().use_cow, !before);
        assert!(model.is_dirty());
    }

    #[test]
    fn config_editor_toggle_is_noop_on_non_bool() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::ProcessKillTimeout);
        assert!(!model.toggle());
        assert!(!model.is_dirty());
    }

    #[test]
    fn config_editor_commit_edit_validates_choice() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::TerminalMode);
        assert!(model.begin_edit());
        // overwrite the prefilled value
        for _ in 0..32 {
            model.edit_pop_char();
        }
        for ch in "garbage".chars() {
            model.edit_push_char(ch);
        }
        assert!(!model.commit_edit());
        let status = model.status().expect("status set on validation error");
        assert_eq!(status.kind, ConfigStatusKind::Error);
        assert!(model.editing(), "stays in edit mode on validation failure");

        // recover with a valid value
        for _ in 0..32 {
            model.edit_pop_char();
        }
        for ch in "window".chars() {
            model.edit_push_char(ch);
        }
        assert!(model.commit_edit());
        assert_eq!(model.working_config().terminal_mode, "window");
        assert!(model.is_dirty());
    }

    #[test]
    fn config_editor_commit_edit_validates_u64_range() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::AgentRefreshRate);
        assert!(model.begin_edit());
        for _ in 0..32 {
            model.edit_pop_char();
        }
        for ch in "10".chars() {
            // below min of 250
            model.edit_push_char(ch);
        }
        assert!(!model.commit_edit());
        assert_eq!(
            model.status().map(|s| s.kind),
            Some(ConfigStatusKind::Error)
        );

        for _ in 0..32 {
            model.edit_pop_char();
        }
        for ch in "1500".chars() {
            model.edit_push_char(ch);
        }
        assert!(model.commit_edit());
        assert_eq!(model.working_config().agent.refresh_rate, 1500);
    }

    #[test]
    fn config_editor_option_path_empty_clears_value() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::WorktreesPath);
        // First set it to something
        assert!(model.begin_edit());
        for ch in "/tmp/wt".chars() {
            model.edit_push_char(ch);
        }
        assert!(model.commit_edit());
        assert_eq!(
            model.working_config().worktrees_path,
            Some(PathBuf::from("/tmp/wt"))
        );

        // Then clear it
        assert!(model.begin_edit());
        for _ in 0..64 {
            model.edit_pop_char();
        }
        assert!(model.commit_edit());
        assert_eq!(model.working_config().worktrees_path, None);
    }

    #[test]
    fn config_editor_begin_edit_blocks_on_read_only_list() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::GitProtectedBranches);
        assert!(!model.begin_edit());
        assert!(!model.editing());
        let status = model.status().expect("info status set");
        assert_eq!(status.kind, ConfigStatusKind::Info);
    }

    #[test]
    fn config_editor_revert_restores_original_and_clears_dirty() {
        let mut model = fresh_model();
        position_on_field(&mut model, ConfigFieldId::AutoConfirm);
        assert!(model.toggle());
        assert!(model.is_dirty());
        model.revert();
        assert!(!model.is_dirty());
        assert_eq!(model.status().map(|s| s.kind), Some(ConfigStatusKind::Info));
    }

    #[test]
    fn config_editor_save_persists_changes_and_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.toml");
        let mut manager = ConfigManager {
            config: Config::default(),
            config_path: path.clone(),
        };
        let mut model = ConfigEditorModel::from_config_with_env(
            manager.get().clone(),
            path.clone(),
            Vec::new(),
        );
        position_on_field(&mut model, ConfigFieldId::ProcessKillTimeout);
        assert!(model.begin_edit());
        for _ in 0..32 {
            model.edit_pop_char();
        }
        for ch in "42".chars() {
            model.edit_push_char(ch);
        }
        assert!(model.commit_edit());
        assert!(model.is_dirty());

        model.save(&mut manager).expect("save");
        assert!(!model.is_dirty());
        let raw = std::fs::read_to_string(&path).expect("written");
        assert!(raw.contains("kill_timeout = 42"));
        assert_eq!(
            model.status().map(|s| s.kind),
            Some(ConfigStatusKind::Success)
        );
    }

    #[test]
    fn config_editor_request_quit_signals_dirty_state() {
        let mut model = fresh_model();
        assert_eq!(model.request_quit(), ConfigQuitOutcome::Clean);
        position_on_field(&mut model, ConfigFieldId::UseCow);
        model.toggle();
        assert_eq!(model.request_quit(), ConfigQuitOutcome::NeedsConfirm);
    }

    #[test]
    fn config_editor_section_navigation_wraps() {
        let mut model = fresh_model();
        let total = model.sections.len();
        for _ in 0..total {
            model.next_section();
        }
        assert_eq!(model.current_section(), ConfigSectionId::General);
        model.prev_section();
        assert_eq!(
            model.current_section(),
            *ConfigSectionId::all().last().unwrap()
        );
    }
}
