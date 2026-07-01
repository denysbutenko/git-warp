pub mod agents;
pub mod cleanup;
pub mod config_editor;
pub mod shell;
pub mod switcher;
pub mod terminal;

#[allow(unused_imports)]
pub use agents::{
    AgentPresenceFilter, AgentRuntimeFilter, AgentsDashboard, DashboardFilters, DashboardModel,
    DashboardRow, build_dashboard_model, build_dashboard_model_filtered_windowed,
    build_dashboard_model_windowed, is_stale_session, session_detail_lines, truncate_label,
};
#[allow(unused_imports)]
pub use cleanup::{
    CleanupRow, CleanupTui, build_cleanup_rows, cleanup_reason_label,
    cleanup_reason_label_for_mode, next_bulk_selection_state,
};
#[allow(unused_imports)]
pub use config_editor::{
    ConfigEditorModel, ConfigFieldId, ConfigFieldKind, ConfigFieldSpec, ConfigQuitOutcome,
    ConfigSectionId, ConfigStatusKind, ConfigStatusMsg, ConfigTui, apply_field_value,
    config_field_specs, detect_env_overrides, draw_config_editor, render_field_value,
};
pub use shell::WarpTui;
#[allow(unused_imports)]
pub use switcher::{
    WorktreeBatchRemoval, WorktreeRemovalBlock, WorktreeRemovalSkip, WorktreeRemovalTarget,
    WorktreeRuntimeStatus, WorktreeSwitchAction, WorktreeSwitchDisplayRow, WorktreeSwitchModel,
    WorktreeSwitchRow, WorktreeSwitchTarget, build_worktree_switch_model,
    build_worktree_switch_model_with_metadata, build_worktree_switch_model_with_protected_branches,
    build_worktree_switch_rows,
};
#[allow(unused_imports)]
// Re-exports for public API consistency
pub use terminal::{TuiTerminalGuard, combine_errors};

/// Result of feeding one key press to a TUI view controller. The shell that
/// owns the terminal loop interprets this to switch views, quit, or act.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ViewOutcome {
    /// Key handled; stay in the current view.
    Consumed,
    /// Switch to the other view.
    ToggleView,
    /// Quit the TUI.
    Quit,
    /// The worktree switcher produced a terminal action.
    Action(switcher::WorktreeSwitchAction),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::AgentDiscovery;
    use std::path::PathBuf;

    #[test]
    fn test_tui_creation() {
        let _dashboard =
            AgentsDashboard::new(AgentDiscovery::new(vec![PathBuf::from("/tmp/repo")]));
        let _cleanup_tui = CleanupTui::new();
        let _config_tui = ConfigTui::new();
    }
}
