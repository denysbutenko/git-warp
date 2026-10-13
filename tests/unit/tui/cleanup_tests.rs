use git_warp::git::BranchStatus;
use git_warp::tui::{build_cleanup_rows, cleanup_reason_label_for_mode, next_bulk_selection_state};
use std::path::PathBuf;

#[test]
fn test_build_cleanup_rows_explains_candidate_status_with_text() {
    let rows = build_cleanup_rows(
        &[BranchStatus {
            branch: "feature/old".to_string(),
            path: PathBuf::from("/repo/.worktrees/feature-old"),
            has_remote: true,
            is_merged: true,
            is_identical: false,
            has_uncommitted_changes: true,
        }],
        &[true],
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].branch, "feature/old");
    assert_eq!(rows[0].reason_label, "merged");
    assert_eq!(rows[0].remote_label, "remote");
    assert_eq!(rows[0].dirty_label, "dirty");
    assert!(rows[0].display_line.contains("[x]"));
    assert!(rows[0].display_line.contains("merged"));
    assert!(rows[0].display_line.contains("remote"));
    assert!(rows[0].display_line.contains("dirty"));
    assert!(
        rows[0]
            .display_line
            .contains("/repo/.worktrees/feature-old")
    );
}

#[test]
fn test_build_cleanup_rows_explains_remoteless_candidates() {
    let rows = build_cleanup_rows(
        &[BranchStatus {
            branch: "feature/local-only".to_string(),
            path: PathBuf::from("/repo/.worktrees/local-only"),
            has_remote: false,
            is_merged: false,
            is_identical: false,
            has_uncommitted_changes: false,
        }],
        &[false],
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason_label, "no remote");
    assert_eq!(rows[0].remote_label, "no remote");
    assert_eq!(rows[0].dirty_label, "clean");
    assert!(rows[0].display_line.contains("no remote"));
}

#[test]
fn test_cleanup_reason_label_for_mode_disambiguates_all_mode_fallback() {
    let plain_candidate = BranchStatus {
        branch: "feature/active".to_string(),
        path: PathBuf::from("/repo/.worktrees/active"),
        has_remote: true,
        is_merged: false,
        is_identical: false,
        has_uncommitted_changes: false,
    };
    assert_eq!(
        cleanup_reason_label_for_mode(&plain_candidate, "all"),
        "all-mode"
    );
    assert_eq!(
        cleanup_reason_label_for_mode(&plain_candidate, "interactive"),
        "all-mode"
    );
    assert_eq!(
        cleanup_reason_label_for_mode(&plain_candidate, "merged"),
        "candidate"
    );

    let merged = BranchStatus {
        is_merged: true,
        ..plain_candidate.clone()
    };
    assert_eq!(cleanup_reason_label_for_mode(&merged, "all"), "merged");
}

#[test]
fn test_next_bulk_selection_state_selects_all_unless_all_are_selected() {
    assert!(next_bulk_selection_state(&[false, false]));
    assert!(next_bulk_selection_state(&[true, false]));
    assert!(!next_bulk_selection_state(&[true, true]));
    assert!(!next_bulk_selection_state(&[]));
}
