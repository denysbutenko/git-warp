pub mod terminal;
pub mod agents;
pub mod switcher;
pub mod cleanup;
pub mod config_editor;

#[allow(unused_imports)]
// Re-exports for public API consistency
pub use terminal::{TuiTerminalGuard, combine_errors};
#[allow(unused_imports)]
pub use agents::{
    AgentsDashboard, DashboardModel, DashboardRow, DashboardFilters,
    AgentRuntimeFilter, AgentPresenceFilter, build_dashboard_model,
    build_dashboard_model_windowed, build_dashboard_model_filtered_windowed,
    session_detail_lines, is_stale_session, truncate_label,
};
#[allow(unused_imports)]
pub use switcher::{
    WorktreeSwitchTui, WorktreeSwitchModel, WorktreeSwitchRow,
    WorktreeSwitchDisplayRow, WorktreeSwitchTarget, WorktreeRemovalTarget,
    WorktreeRemovalSkip, WorktreeBatchRemoval, WorktreeSwitchAction,
    WorktreeRemovalBlock, WorktreeRuntimeStatus, build_worktree_switch_model,
    build_worktree_switch_model_with_protected_branches,
    build_worktree_switch_model_with_metadata, build_worktree_switch_rows,
};
#[allow(unused_imports)]
pub use cleanup::{
    CleanupTui, CleanupRow, build_cleanup_rows, next_bulk_selection_state,
    cleanup_reason_label, cleanup_reason_label_for_mode,
};
#[allow(unused_imports)]
pub use config_editor::{
    ConfigTui, ConfigEditorModel, ConfigSectionId, ConfigFieldId,
    ConfigFieldKind, ConfigFieldSpec, ConfigStatusKind, ConfigStatusMsg,
    ConfigQuitOutcome, config_field_specs, detect_env_overrides,
    render_field_value, apply_field_value, draw_config_editor,
};

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
