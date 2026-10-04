use super::*;
use crate::config::{Config, ConfigManager};
use std::path::PathBuf;

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
    let mut model =
        ConfigEditorModel::from_config_with_env(manager.get().clone(), path.clone(), Vec::new());
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
