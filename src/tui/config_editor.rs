use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use crate::{
    config::{Config, ConfigManager},
    error::Result,
};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    Frame, Terminal as RatatuiTerminal,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

pub fn config_field_specs() -> Vec<ConfigFieldSpec> {
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
    /// No unsaved changes â€” caller may exit immediately.
    Clean,
    /// Working copy diverges from disk â€” caller must confirm discard.
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

pub fn detect_env_overrides() -> Vec<String> {
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

pub fn render_field_value(config: &Config, field: &ConfigFieldSpec) -> String {
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

pub fn apply_field_value(
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

pub fn draw_config_editor(f: &mut Frame, model: &ConfigEditorModel, pending_quit: bool) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(f.size());

    let dirty_tag = if model.is_dirty() { " [unsaved]" } else { "" };
    let header_text = format!(
        "Git-Warp config editor â€” {}{}",
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
                "â–¸ "
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
                let cursor_mark = if idx == cursor { "â–¸" } else { " " };
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
            "Heads up: {} GIT_WARP_* env var(s) set â€” they shadow saved values at runtime.",
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
        "â†‘â†“/jk move  Tab section  Space toggle  Enter/e edit  s save  r revert  q quit"
            .to_string()
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Center);
    f.render_widget(footer, outer[3]);
}

#[cfg(test)]
mod tests {
    use super::*;

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
