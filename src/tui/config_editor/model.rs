use super::apply::{apply_field_value, detect_env_overrides, render_field_value};
use super::spec::{
    ConfigFieldId, ConfigFieldKind, ConfigFieldSpec, ConfigSectionId, config_field_specs,
};
use crate::{
    config::{Config, ConfigManager},
    error::Result,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(super) struct ConfigEditBuffer {
    pub(super) field_id: ConfigFieldId,
    pub(super) value: String,
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
    pub(super) fields: Vec<ConfigFieldSpec>,
    pub(super) sections: Vec<ConfigSectionId>,
    pub(super) section_idx: usize,
    pub(super) field_idx_in_section: usize,
    pub(super) working: Config,
    pub(super) original: Config,
    pub(super) edit_buffer: Option<ConfigEditBuffer>,
    pub(super) status: Option<ConfigStatusMsg>,
    pub(super) config_path: PathBuf,
    pub(super) env_overrides: Vec<String>,
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
