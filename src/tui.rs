use crate::error::Result;
use chrono::Timelike;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, poll},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, ListState, Paragraph, Row, Table},
};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

pub struct TuiApp {
    should_quit: bool,
    selected_index: usize,
    last_update: Instant,
}

#[derive(Debug, Clone)]
pub struct AgentActivity {
    pub timestamp: String,
    pub agent_name: String,
    pub activity: String,
    pub file_path: Option<PathBuf>,
    pub status: AgentStatus,
}

#[derive(Debug, Clone)]
pub enum AgentStatus {
    Active,
    Waiting,
    Completed,
    Error,
}

impl AgentStatus {
    pub fn color(&self) -> Color {
        match self {
            AgentStatus::Active => Color::Green,
            AgentStatus::Waiting => Color::Yellow,
            AgentStatus::Completed => Color::Blue,
            AgentStatus::Error => Color::Red,
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            AgentStatus::Active => "🔄",
            AgentStatus::Waiting => "⏳",
            AgentStatus::Completed => "✅",
            AgentStatus::Error => "❌",
        }
    }
}

impl TuiApp {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            selected_index: 0,
            last_update: Instant::now(),
        }
    }

    pub fn get_selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn set_selected_index(&mut self, index: usize) {
        self.selected_index = index;
    }

    pub fn get_last_update(&self) -> Instant {
        self.last_update
    }

    pub fn set_last_update(&mut self, time: Instant) {
        self.last_update = time;
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = RatatuiTerminal::new(backend)?;

        let res = self.run_app(&mut terminal);

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        res
    }

    fn run_app(
        &mut self,
        terminal: &mut RatatuiTerminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        // Mock agent data for demo - in real implementation this would come from file watchers
        let mut activities = vec![
            AgentActivity {
                timestamp: "14:32:15".to_string(),
                agent_name: "Claude-Code".to_string(),
                activity: "Analyzing code structure".to_string(),
                file_path: Some(PathBuf::from("/project/src/main.rs")),
                status: AgentStatus::Active,
            },
            AgentActivity {
                timestamp: "14:31:42".to_string(),
                agent_name: "Claude-Code".to_string(),
                activity: "Refactoring function".to_string(),
                file_path: Some(PathBuf::from("/project/src/utils.rs")),
                status: AgentStatus::Completed,
            },
            AgentActivity {
                timestamp: "14:30:18".to_string(),
                agent_name: "Claude-Code".to_string(),
                activity: "Waiting for user input".to_string(),
                file_path: None,
                status: AgentStatus::Waiting,
            },
        ];

        loop {
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
                        KeyCode::Up => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if self.selected_index < activities.len().saturating_sub(1) {
                                self.selected_index += 1;
                            }
                        }
                        KeyCode::Char('r') => {
                            // Simulate refresh - add new activity
                            activities.insert(
                                0,
                                AgentActivity {
                                    timestamp: format!(
                                        "{:02}:{:02}:{:02}",
                                        chrono::Local::now().hour(),
                                        chrono::Local::now().minute(),
                                        chrono::Local::now().second()
                                    ),
                                    agent_name: "Claude-Code".to_string(),
                                    activity: "Processing new request".to_string(),
                                    file_path: Some(PathBuf::from("/project/src/new_module.rs")),
                                    status: AgentStatus::Active,
                                },
                            );
                            self.selected_index = 0;
                        }
                        _ => {}
                    }
                }
            }

            // Update UI
            terminal.draw(|f| self.draw_agents_dashboard(f, &activities))?;

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn draw_agents_dashboard(&self, f: &mut Frame, activities: &[AgentActivity]) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Min(8),    // Main content
                Constraint::Length(5), // Stats
                Constraint::Length(3), // Help
            ])
            .split(f.size());

        // Header
        let header = Paragraph::new("🤖 Agent Activity Monitor")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        f.render_widget(header, chunks[0]);

        // Main activity list
        let activity_items: Vec<ListItem> = activities
            .iter()
            .enumerate()
            .map(|(i, activity)| {
                let style = if i == self.selected_index {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                let file_info = if let Some(path) = &activity.file_path {
                    format!(
                        " ({})",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    )
                } else {
                    String::new()
                };

                let content = format!(
                    "{} {} [{}] {}{}!",
                    activity.status.symbol(),
                    activity.timestamp,
                    activity.agent_name,
                    activity.activity,
                    file_info
                );

                ListItem::new(Line::from(Span::styled(
                    content,
                    style.fg(activity.status.color()),
                )))
            })
            .collect();

        let activities_list = List::new(activity_items)
            .block(
                Block::default()
                    .title("Recent Activity")
                    .borders(Borders::ALL),
            )
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ");

        let mut list_state = ListState::default();
        list_state.select(Some(self.selected_index));
        f.render_stateful_widget(activities_list, chunks[1], &mut list_state);

        // Stats section
        let stats_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ])
            .split(chunks[2]);

        let active_count = activities
            .iter()
            .filter(|a| matches!(a.status, AgentStatus::Active))
            .count();
        let total_count = activities.len();
        let completed_count = activities
            .iter()
            .filter(|a| matches!(a.status, AgentStatus::Completed))
            .count();

        // Active agents gauge
        let active_ratio = if total_count > 0 {
            active_count as f64 / total_count as f64
        } else {
            0.0
        };
        let active_gauge = Gauge::default()
            .block(Block::default().title("Active").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Green))
            .ratio(active_ratio)
            .label(format!("{}/{}", active_count, total_count));
        f.render_widget(active_gauge, stats_chunks[0]);

        // Completion rate
        let completion_ratio = if total_count > 0 {
            completed_count as f64 / total_count as f64
        } else {
            0.0
        };
        let completion_gauge = Gauge::default()
            .block(Block::default().title("Completed").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Blue))
            .ratio(completion_ratio)
            .label(format!("{:.1}%", completion_ratio * 100.0));
        f.render_widget(completion_gauge, stats_chunks[1]);

        // Uptime
        let uptime = self.last_update.elapsed().as_secs();
        let uptime_display = Paragraph::new(format!("{}m {}s", uptime / 60, uptime % 60))
            .block(Block::default().title("Uptime").borders(Borders::ALL))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(uptime_display, stats_chunks[2]);

        // Help
        let help_text = "↑↓: Navigate | r: Refresh | q: Quit | Esc: Exit";
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help, chunks[3]);
    }
}

pub struct AgentsDashboard;

impl AgentsDashboard {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<()> {
        let mut app = TuiApp::new();
        app.run()
    }

    /// Start monitoring agents in a specific worktree
    pub fn monitor_worktree(&self, worktree_path: PathBuf) -> Result<()> {
        println!(
            "🔍 Starting agent monitoring for: {}",
            worktree_path.display()
        );

        // TODO: In real implementation, set up file watchers here
        // let (tx, rx) = mpsc::channel();
        // let mut watcher: RecommendedWatcher = Watcher::new_immediate(move |res| {
        //     tx.send(res).unwrap();
        // })?;
        // watcher.watch(&worktree_path, RecursiveMode::Recursive)?;

        let mut app = TuiApp::new();
        app.run()
    }
}

pub struct CleanupTui;

impl CleanupTui {
    pub fn new() -> Self {
        Self
    }

    pub fn run(&self) -> Result<Vec<String>> {
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
        let worktrees = git_repo.list_worktrees()?;
        let branch_statuses = git_repo.analyze_branches_for_cleanup(&worktrees)?;

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

        let result = loop {
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
                let header = Paragraph::new("🧹 Interactive Worktree Cleanup")
                    .style(Style::default().fg(Color::Yellow))
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL));
                f.render_widget(header, chunks[0]);

                // Branch list
                let items: Vec<ListItem> = branch_statuses
                    .iter()
                    .enumerate()
                    .map(|(i, status)| {
                        let checkbox = if selected_branches[i] {
                            "☑️"
                        } else {
                            "☐"
                        };
                        let merged_indicator = if status.is_merged { "✅" } else { "🔄" };
                        let style = if i == selected_index {
                            Style::default().bg(Color::Blue).fg(Color::White)
                        } else {
                            Style::default()
                        };

                        ListItem::new(format!(
                            "{} {} {} - {}{}",
                            checkbox,
                            merged_indicator,
                            status.branch,
                            if status.has_remote {
                                "with remote"
                            } else {
                                "no remote"
                            },
                            if status.has_uncommitted_changes {
                                " (uncommitted)"
                            } else {
                                ""
                            }
                        ))
                        .style(style)
                    })
                    .collect();

                let list = List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Select branches to clean up (Space to select, Enter to confirm)"),
                );
                f.render_widget(list, chunks[1]);

                // Footer with controls
                let selected_count = selected_branches.iter().filter(|&&x| x).count();
                let footer_text = format!(
                    "↑↓: Navigate | Space: Select | Enter: Confirm ({} selected) | q: Quit",
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
                        KeyCode::Char('q') => {
                            should_quit = true;
                            break;
                        }
                        KeyCode::Up => {
                            if selected_index > 0 {
                                selected_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if selected_index < branch_statuses.len() - 1 {
                                selected_index += 1;
                            }
                        }
                        KeyCode::Char(' ') => {
                            selected_branches[selected_index] = !selected_branches[selected_index];
                        }
                        KeyCode::Enter => {
                            confirmed = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
        };

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
        let mut app = TuiApp::new();
        app.run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tui_creation() {
        let dashboard = AgentsDashboard::new();
        // Just test that we can create the TUI components
        let _cleanup_tui = CleanupTui::new();
        let _config_tui = ConfigTui::new();
    }

    #[test]
    fn test_agent_status() {
        assert_eq!(AgentStatus::Active.symbol(), "🔄");
        assert_eq!(AgentStatus::Waiting.color(), Color::Yellow);
        assert_eq!(AgentStatus::Completed.symbol(), "✅");
        assert_eq!(AgentStatus::Error.color(), Color::Red);
    }

    #[test]
    fn test_agent_activity() {
        let activity = AgentActivity {
            timestamp: "12:34:56".to_string(),
            agent_name: "TestAgent".to_string(),
            activity: "Testing".to_string(),
            file_path: Some(PathBuf::from("/test/file.rs")),
            status: AgentStatus::Active,
        };

        assert_eq!(activity.agent_name, "TestAgent");
        assert!(activity.file_path.is_some());
    }
}
