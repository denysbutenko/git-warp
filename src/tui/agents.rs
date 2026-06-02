use crate::{
    agents::{
        AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
        sort_session_summaries,
    },
    error::Result,
};
use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use chrono::{DateTime, Duration as ChronoDuration, Local};
use crossterm::event::{self, Event, KeyCode, poll};
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
    time::{Duration, Instant},
};

/// Minimum allowed agent dashboard refresh interval. Faster than this and the
/// dashboard churns the file system reading session state.
const MIN_REFRESH_INTERVAL: Duration = Duration::from_millis(250);
/// Fallback when the caller does not pass a configured refresh rate.
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);

pub fn clamp_refresh_interval(refresh_rate_ms: u64) -> Duration {
    Duration::from_millis(refresh_rate_ms).max(MIN_REFRESH_INTERVAL)
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            format!("Filter: {}", parts.join(" \u{00b7} "))
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
                "Warp Agents ({}/{}) â€” {}",
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
            "â†‘â†“/jk: Navigate | r: Refresh | t: Runtime | p: Presence | c: Clear | q/Esc: Quit";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[2]);
    }
}

#[allow(dead_code)] // Convenience wrappers used by unit tests via the library crate.
pub fn build_dashboard_model(
    sessions: &[AgentSessionSummary],
    now: DateTime<Local>,
) -> DashboardModel {
    let mut ordered_sessions = sessions.to_vec();
    sort_session_summaries(&mut ordered_sessions);
    build_dashboard_model_windowed(&ordered_sessions, now, 0, ordered_sessions.len().max(1))
}

#[allow(dead_code)]
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
        AgentSessionState::Starting => "â€”‹",
        AgentSessionState::Working => "●",
        AgentSessionState::Processing => "—",
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

pub fn truncate_label(value: &str, max_chars: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;

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
}
