use super::model::{ConfigEditorModel, ConfigQuitOutcome, ConfigStatusKind, ConfigStatusMsg};
use super::spec::ConfigFieldKind;
use super::view::draw_config_editor;
use crate::tui::terminal::{TuiTerminalGuard, combine_errors};
use crate::{config::ConfigManager, error::Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{Terminal as RatatuiTerminal, backend::CrosstermBackend};
use std::io;

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
