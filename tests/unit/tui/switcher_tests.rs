use git_warp::git::WorktreeInfo;
use git_warp::tui::{
    WorktreeRemovalBlock, WorktreeRuntimeStatus, build_worktree_switch_model,
    build_worktree_switch_model_with_metadata, build_worktree_switch_model_with_protected_branches,
    build_worktree_switch_rows,
};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

#[test]
fn test_build_worktree_switch_model_marks_state_and_detached_rows() {
    let worktrees = vec![
        WorktreeInfo {
            path: PathBuf::from("/repo"),
            branch: "main".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: true,
            is_current: true,
            is_detached: false,
        },
        WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/detached"),
            branch: String::new(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: true,
        },
    ];
    let statuses = vec![
        WorktreeRuntimeStatus {
            path: PathBuf::from("/repo"),
            is_current: true,
            is_dirty: true,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: PathBuf::from("/repo/.worktrees/detached"),
            is_current: false,
            is_dirty: false,
            is_occupied: true,
            last_touched: None,
        },
    ];

    let model = build_worktree_switch_model(&worktrees, &statuses);

    assert_eq!(model.rows.len(), 2);
    assert_eq!(model.rows[0].branch_label, "main");
    assert_eq!(
        model.rows[0].badges,
        vec!["primary", "protected", "current", "dirty"]
    );
    assert_eq!(model.rows[1].branch_label, "(detached HEAD: abcdef01)");
    assert_eq!(model.rows[1].badges, vec!["detached", "occupied"]);
}

#[test]
fn test_build_worktree_switch_model_marks_local_only_branches() {
    let worktrees = vec![
        WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/tracked"),
            branch: "tracked".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/local-only"),
            branch: "local-only".to_string(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
    ];
    let statuses = vec![];
    let local_only_branches = vec!["local-only".to_string()];

    let model =
        build_worktree_switch_model_with_metadata(&worktrees, &statuses, &[], &local_only_branches);

    let tracked_row = model
        .rows
        .iter()
        .find(|row| row.branch_label == "tracked")
        .expect("tracked row");
    assert!(!tracked_row.badges.iter().any(|badge| badge == "local-only"));

    let local_only_row = model
        .rows
        .iter()
        .find(|row| row.branch_label == "local-only")
        .expect("local-only row");
    assert!(
        local_only_row
            .badges
            .iter()
            .any(|badge| badge == "local-only")
    );
}

#[test]
fn test_build_worktree_switch_model_orders_recently_touched_worktrees_first() {
    let older_path = PathBuf::from("/repo/.worktrees/older");
    let newer_path = PathBuf::from("/repo/.worktrees/newer");
    let middle_path = PathBuf::from("/repo/.worktrees/middle");
    let worktrees = vec![
        WorktreeInfo {
            path: older_path.clone(),
            branch: "older".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: newer_path.clone(),
            branch: "newer".to_string(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: middle_path.clone(),
            branch: "middle".to_string(),
            head: "fedcba9876543210".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
    ];
    let statuses = vec![
        WorktreeRuntimeStatus {
            path: older_path,
            is_current: false,
            is_dirty: false,
            is_occupied: false,
            last_touched: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(10)),
        },
        WorktreeRuntimeStatus {
            path: newer_path,
            is_current: false,
            is_dirty: false,
            is_occupied: false,
            last_touched: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(30)),
        },
        WorktreeRuntimeStatus {
            path: middle_path,
            is_current: false,
            is_dirty: false,
            is_occupied: false,
            last_touched: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(20)),
        },
    ];

    let model = build_worktree_switch_model(&worktrees, &statuses);
    let branch_labels: Vec<_> = model
        .rows
        .iter()
        .map(|row| row.branch_label.as_str())
        .collect();

    assert_eq!(branch_labels, vec!["newer", "middle", "older"]);
}

#[test]
fn test_worktree_switch_model_returns_selected_target() {
    let worktrees = vec![WorktreeInfo {
        path: PathBuf::from("/repo/.worktrees/feature"),
        branch: "feature/default-picker".to_string(),
        head: "0123456789abcdef".to_string(),
        is_primary: false,
        is_current: false,
        is_detached: false,
    }];

    let model = build_worktree_switch_model(&worktrees, &[]);
    let target = model
        .target_at(0)
        .expect("selected row should have a target");

    assert_eq!(target.branch.as_deref(), Some("feature/default-picker"));
    assert_eq!(target.path, PathBuf::from("/repo/.worktrees/feature"));
    assert!(model.target_at(1).is_none());
}

#[test]
fn test_worktree_switch_model_allows_safe_branch_removal() {
    let worktree_path = PathBuf::from("/repo/.worktrees/feature");
    let worktrees = vec![WorktreeInfo {
        path: worktree_path.clone(),
        branch: "feature/default-picker".to_string(),
        head: "0123456789abcdef".to_string(),
        is_primary: false,
        is_current: false,
        is_detached: false,
    }];
    let statuses = vec![WorktreeRuntimeStatus {
        path: worktree_path.clone(),
        is_current: false,
        is_dirty: false,
        is_occupied: false,
        last_touched: None,
    }];

    let model = build_worktree_switch_model(&worktrees, &statuses);
    let removal = model
        .removal_at(0)
        .expect("clean branch worktree should be removable");

    assert_eq!(
        model.rows[0].removal_blockers,
        Vec::<WorktreeRemovalBlock>::new()
    );
    assert_eq!(removal.branch, "feature/default-picker");
    assert_eq!(removal.path, worktree_path);
    assert!(model.removal_at(1).is_none());
}

#[test]
fn test_worktree_switch_model_blocks_removal_for_risky_rows() {
    let primary_path = PathBuf::from("/repo");
    let detached_path = PathBuf::from("/repo/.worktrees/detached");
    let dirty_path = PathBuf::from("/repo/.worktrees/dirty");
    let occupied_path = PathBuf::from("/repo/.worktrees/occupied");
    let worktrees = vec![
        WorktreeInfo {
            path: primary_path.clone(),
            branch: "main".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: true,
            is_current: true,
            is_detached: false,
        },
        WorktreeInfo {
            path: detached_path.clone(),
            branch: String::new(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: true,
        },
        WorktreeInfo {
            path: dirty_path.clone(),
            branch: "dirty".to_string(),
            head: "fedcba9876543210".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: occupied_path.clone(),
            branch: "occupied".to_string(),
            head: "9876543210fedcba".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
    ];
    let statuses = vec![
        WorktreeRuntimeStatus {
            path: primary_path,
            is_current: true,
            is_dirty: false,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: detached_path,
            is_current: false,
            is_dirty: false,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: dirty_path,
            is_current: false,
            is_dirty: true,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: occupied_path,
            is_current: false,
            is_dirty: false,
            is_occupied: true,
            last_touched: None,
        },
    ];

    let model = build_worktree_switch_model(&worktrees, &statuses);
    let blockers: Vec<_> = model
        .rows
        .iter()
        .map(|row| row.removal_blockers.clone())
        .collect();

    assert_eq!(
        blockers,
        vec![
            vec![
                WorktreeRemovalBlock::Primary,
                WorktreeRemovalBlock::Protected,
                WorktreeRemovalBlock::Current
            ],
            vec![WorktreeRemovalBlock::Detached],
            vec![WorktreeRemovalBlock::Dirty],
            vec![WorktreeRemovalBlock::Occupied],
        ]
    );
    assert!(model.removal_at(0).is_none());
    assert!(model.removal_at(1).is_none());
    let dirty_target = model
        .removal_at(2)
        .expect("dirty worktree should be force-removable");
    assert_eq!(dirty_target.branch, "dirty");
    assert!(
        dirty_target.force,
        "dirty worktree must request force removal"
    );
    assert!(model.removal_at(3).is_none());
}

#[test]
fn test_worktree_switch_model_blocks_removal_for_protected_branches() {
    let worktree_path = PathBuf::from("/repo/.worktrees/develop");
    let worktrees = vec![WorktreeInfo {
        path: worktree_path.clone(),
        branch: "develop".to_string(),
        head: "0123456789abcdef".to_string(),
        is_primary: false,
        is_current: false,
        is_detached: false,
    }];
    let statuses = vec![WorktreeRuntimeStatus {
        path: worktree_path,
        is_current: false,
        is_dirty: false,
        is_occupied: false,
        last_touched: None,
    }];

    let model = build_worktree_switch_model_with_protected_branches(
        &worktrees,
        &statuses,
        &["develop".to_string()],
    );

    assert_eq!(model.rows[0].badges, vec!["protected"]);
    assert_eq!(
        model.rows[0].removal_blockers,
        vec![WorktreeRemovalBlock::Protected]
    );
    assert!(model.removal_at(0).is_none());
}

#[test]
fn test_worktree_switch_model_builds_batch_removal_plan_with_skips() {
    let safe_path = PathBuf::from("/repo/.worktrees/safe");
    let dirty_path = PathBuf::from("/repo/.worktrees/dirty");
    let primary_path = PathBuf::from("/repo");
    let worktrees = vec![
        WorktreeInfo {
            path: safe_path.clone(),
            branch: "safe".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: dirty_path.clone(),
            branch: "dirty".to_string(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: primary_path.clone(),
            branch: "main".to_string(),
            head: "fedcba9876543210".to_string(),
            is_primary: true,
            is_current: true,
            is_detached: false,
        },
    ];
    let statuses = vec![
        WorktreeRuntimeStatus {
            path: safe_path.clone(),
            is_current: false,
            is_dirty: false,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: dirty_path.clone(),
            is_current: false,
            is_dirty: true,
            is_occupied: false,
            last_touched: None,
        },
        WorktreeRuntimeStatus {
            path: primary_path.clone(),
            is_current: true,
            is_dirty: false,
            is_occupied: false,
            last_touched: None,
        },
    ];

    let model = build_worktree_switch_model(&worktrees, &statuses);
    let batch = model
        .batch_removal_at(&[0, 1, 2, 2, 99])
        .expect("selected rows should build a batch plan");

    assert_eq!(batch.targets.len(), 2);
    assert_eq!(batch.targets[0].branch, "safe");
    assert_eq!(batch.targets[0].path, safe_path);
    assert!(!batch.targets[0].force);
    assert_eq!(batch.targets[1].branch, "dirty");
    assert_eq!(batch.targets[1].path, dirty_path);
    assert!(
        batch.targets[1].force,
        "dirty worktree must be queued with force"
    );
    assert_eq!(batch.skipped.len(), 1);
    assert_eq!(batch.skipped[0].branch_label, "main");
    assert_eq!(batch.skipped[0].path, primary_path);
    assert_eq!(batch.skipped[0].reason, "primary, protected, current");
    assert!(model.batch_removal_at(&[]).is_none());
}

#[test]
fn test_build_worktree_switch_rows_marks_selected_entries() {
    let worktrees = vec![
        WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/one"),
            branch: "one".to_string(),
            head: "0123456789abcdef".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
        WorktreeInfo {
            path: PathBuf::from("/repo/.worktrees/two"),
            branch: "two".to_string(),
            head: "abcdef0123456789".to_string(),
            is_primary: false,
            is_current: false,
            is_detached: false,
        },
    ];

    let model = build_worktree_switch_model(&worktrees, &[]);
    let rows = build_worktree_switch_rows(&model, &[1]);

    assert_eq!(rows.len(), 2);
    assert!(rows[0].display_line.starts_with("[ ]"));
    assert!(rows[1].display_line.starts_with("[x]"));
    assert!(rows[1].display_line.contains("two"));
    assert!(rows[1].display_line.contains("/repo/.worktrees/two"));
}
