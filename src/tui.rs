use crate::{
    agents::{
        AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
        sort_session_summaries,
    },
    config::GitConfig,
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
    io,
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};

const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

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
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DashboardModel {
    pub rows: Vec<DashboardRow>,
    pub total_rows: usize,
    pub start_index: usize,
    pub empty_state_lines: Vec<String>,
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
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum WorktreeSwitchAction {
    Switch(WorktreeSwitchTarget),
    Remove(WorktreeRemovalTarget),
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
        if !row.removal_blockers.is_empty() {
            return None;
        }

        Some(WorktreeRemovalTarget {
            branch: row.target.branch.clone()?,
            path: row.target.path.clone(),
        })
    }
}

pub struct TuiApp {
    should_quit: bool,
    selected_index: usize,
    last_refresh: Instant,
    discovery: AgentDiscovery,
    sessions: Vec<AgentSessionSummary>,
}

impl TuiApp {
    pub fn new(discovery: AgentDiscovery) -> Self {
        Self {
            should_quit: false,
            selected_index: 0,
            last_refresh: Instant::now() - REFRESH_INTERVAL,
            discovery,
            sessions: Vec::new(),
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
            if self.last_refresh.elapsed() >= REFRESH_INTERVAL {
                self.refresh_sessions()?;
            }

            terminal.draw(|f| self.draw_agents_dashboard(f, Local::now()))?;

            // Non-blocking event check
            let timeout = Duration::from_millis(100);
            if poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') => {
                            self.should_quit = true;
                        }
                        KeyCode::Esc => {
                            self.should_quit = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.selected_index < self.sessions.len().saturating_sub(1) {
                                self.selected_index += 1;
                            }
                        }
                        KeyCode::Char('r') => {
                            self.refresh_sessions()?;
                        }
                        _ => {}
                    }
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
        if self.sessions.is_empty() {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(self.sessions.len() - 1);
        }
        self.last_refresh = Instant::now();
        Ok(())
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
            .split(f.size());

        // Header
        let header = Paragraph::new(format!("Warp Agents ({})", self.sessions.len()))
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(header, chunks[0]);

        if self.sessions.is_empty() {
            let model = build_dashboard_model(&[], now);
            let empty_state = Paragraph::new(model.empty_state_lines.join("\n\n"))
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
            let model = build_dashboard_model_windowed(
                &self.sessions,
                now,
                self.selected_index,
                visible_capacity,
            );

            let session_items: Vec<ListItem> = model
                .rows
                .iter()
                .map(|row| {
                    let text = format!(
                        "{} {:<6} {:<10} {:<18} {:<18} {}",
                        row.state_symbol,
                        row.runtime_label,
                        truncate_label(row.state_label, 10),
                        truncate_label(&row.location_label, 18),
                        truncate_label(&row.agent_label, 18),
                        row.relative_time
                    );
                    let style = Style::default().fg(session_state_color(row.session.state));
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

            if let Some(selected_session) = self.sessions.get(self.selected_index) {
                let details = Paragraph::new(session_detail_lines(selected_session).join("\n"))
                    .block(Block::default().title("Details").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White))
                    .wrap(Wrap { trim: false });
                f.render_widget(details, content_chunks[1]);
            }
        }

        // Help
        let help_text = "↑↓/jk: Navigate | r: Refresh | q/Esc: Quit";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[2]);
    }
}

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

pub fn build_worktree_switch_model_with_protected_branches(
    worktrees: &[WorktreeInfo],
    statuses: &[WorktreeRuntimeStatus],
    protected_branches: &[String],
) -> WorktreeSwitchModel {
    let mut rows = worktrees
        .iter()
        .enumerate()
        .map(|(index, worktree)| {
            let status = statuses.iter().find(|status| status.path == worktree.path);
            let is_detached = worktree.branch.trim().is_empty() || worktree.is_detached;
            let is_protected = is_protected_branch(&worktree.branch, protected_branches);
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
            "Run `warp switch <branch>` to create one.".to_string(),
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

pub fn build_dashboard_model(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
) -> DashboardModel {
    let mut ordered_sessions = sessions.to_vec();
    sort_session_summaries(&mut ordered_sessions);
    build_dashboard_model_windowed(&ordered_sessions, now, 0, ordered_sessions.len().max(1))
}

pub fn build_dashboard_model_windowed(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
    selected_index: usize,
    visible_capacity: usize,
) -> DashboardModel {
    let total_rows = sessions.len();
    let visible_capacity = visible_capacity.max(1).min(total_rows.max(1));
    let selected_index = selected_index.min(total_rows.saturating_sub(1));
    let start_index = dashboard_window_start(total_rows, selected_index, visible_capacity);
    let end_index = start_index.saturating_add(visible_capacity).min(total_rows);

    let rows = sessions
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
            session,
        })
        .collect::<Vec<_>>();

    let empty_state_lines = if rows.is_empty() {
        vec![
            "No agent sessions to show for this repository.".to_string(),
            "Recent Claude/Codex sessions appear here for 7 days.".to_string(),
            "Hint: run `warp hooks-install --runtime all --level user` to enable live monitoring."
                .to_string(),
        ]
    } else {
        Vec::new()
    };

    DashboardModel {
        rows,
        total_rows,
        start_index,
        empty_state_lines,
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
}

impl AgentsDashboard {
    pub fn new(discovery: AgentDiscovery) -> Self {
        Self { discovery }
    }

    pub fn run(&self) -> Result<()> {
        let mut app = TuiApp::new(self.discovery.clone());
        app.run()
    }

    /// Start monitoring agents in a specific worktree
    pub fn monitor_worktree(&self, worktree_path: PathBuf) -> Result<()> {
        let mut app = TuiApp::new(AgentDiscovery::new(vec![worktree_path]));
        app.run()
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
        let mut selected_index = 0;
        let mut pending_remove: Option<WorktreeRemovalTarget> = None;
        let mut notice: Option<String> = None;

        loop {
            terminal.draw(|f| {
                draw_worktree_switcher(
                    f,
                    &self.model,
                    selected_index,
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
                            return Ok(Some(WorktreeSwitchAction::Remove(removal)));
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
                    KeyCode::Enter => {
                        return Ok(self
                            .model
                            .target_at(selected_index)
                            .map(WorktreeSwitchAction::Switch));
                    }
                    KeyCode::Char('d') | KeyCode::Delete => {
                        if let Some(removal) = self.model.removal_at(selected_index) {
                            notice = Some(format!(
                                "Remove '{}' and delete its local branch? y/N",
                                removal.branch
                            ));
                            pending_remove = Some(removal);
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
    pending_remove: Option<&WorktreeRemovalTarget>,
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
        .split(f.size());

    let header = Paragraph::new(format!("Warp Worktrees ({})", model.rows.len()))
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    let items = model
        .rows
        .iter()
        .map(|row| {
            let badges = if row.badges.is_empty() {
                String::new()
            } else {
                format!(" [{}]", row.badges.join(", "))
            };
            ListItem::new(Line::from(format!(
                "{:<30} {:<28} {}",
                truncate_label(&row.branch_label, 30),
                truncate_label(&badges, 28),
                row.path_label
            )))
        })
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
            format!("confirm remove {} (y/N)", removal.branch)
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
            "Branch: {}\nPath: {}\nStatus: {}\nRemove: {}{}",
            branch,
            row.target.path.display(),
            status,
            removal_status,
            notice_line
        ))
        .block(Block::default().title("Details").borders(Borders::ALL))
        .style(Style::default().fg(Color::White))
        .wrap(Wrap { trim: false });
        f.render_widget(details, chunks[2]);
    }

    let help = Paragraph::new("↑↓/jk: Navigate | Enter: Switch | d/Del: Remove | q/Esc: Quit")
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Help"));
    f.render_widget(help, chunks[3]);
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

pub struct CleanupTui {
    candidates: Option<Vec<BranchStatus>>,
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
        use crossterm::{
            event::{self, Event, KeyCode, KeyEventKind},
            execute,
            terminal::{
                EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
            },
        };
        use ratatui::{
            Terminal,
            backend::CrosstermBackend,
            layout::{Alignment, Constraint, Direction, Layout},
            style::{Color, Style},
            widgets::{Block, Borders, List, ListItem, Paragraph},
        };
        use std::io;

        // Get the git repository and worktrees
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

        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut selected_index = 0;
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
                    .split(f.size());

                // Header
                let header = Paragraph::new(format!(
                    "Worktree cleanup ({} candidates)",
                    branch_statuses.len()
                ))
                    .style(Style::default().fg(Color::Yellow))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(header, chunks[0]);

                // Branch list
                let rows = build_cleanup_rows(&branch_statuses, &selected_branches);
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

                // Footer with controls
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

            // Handle input
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            should_quit = true;
                            break;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if selected_index > 0 {
                                selected_index -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if selected_index < branch_statuses.len() - 1 {
                                selected_index += 1;
                            }
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
        }

        // Cleanup terminal
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        if should_quit || !confirmed {
            return Ok(vec![]);
        }

        // Return selected branches
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
}

pub struct ConfigTui;

impl ConfigTui {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<()> {
        println!("⚙️ Configuration Editor");
        println!("=======================");
        println!("📝 Interactive config editor coming in v0.3.1");
        println!("💡 For now, use: warp config --show");
        println!("💡 Edit config file at: ~/.config/git-warp/config.toml");

        // Show current TUI for demonstration
        let mut app = TuiApp::new(AgentDiscovery::new(Vec::new()));
        app.run()
    }
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
}
