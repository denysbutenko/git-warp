use chrono::{Local, TimeZone};
use git_warp::agents::{
    AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
};
use git_warp::git::{BranchStatus, WorktreeInfo};
use git_warp::tui::{
    AgentsDashboard, WorktreeRuntimeStatus, build_cleanup_rows, build_dashboard_model,
    build_worktree_switch_model, next_bulk_selection_state, session_detail_lines,
};
use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

fn sample_summary(
    runtime: AgentRuntime,
    session_id: &str,
    cwd: &str,
    state: AgentSessionState,
    last_activity_hour: u32,
    is_live: bool,
    source: AgentSessionSource,
) -> AgentSessionSummary {
    AgentSessionSummary {
        runtime,
        session_id: Some(session_id.to_string()),
        cwd: PathBuf::from(cwd),
        branch: Some("feat/agents".to_string()),
        agent_label: "Parfit (worker)".to_string(),
        state,
        last_activity: Local
            .with_ymd_and_hms(2026, 4, 23, last_activity_hour, 0, 0)
            .unwrap(),
        is_live,
        source,
    }
}

#[test]
fn test_build_dashboard_model_empty_state() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();

    let model = build_dashboard_model(&[], now);

    assert!(model.rows.is_empty());
    assert_eq!(
        model.empty_state_lines,
        vec![
            "No agent sessions to show for this repository.".to_string(),
            "Recent Claude/Codex sessions appear here for 7 days.".to_string(),
            "Hint: run `warp hooks-install --runtime all --level user` to enable live monitoring."
                .to_string(),
        ]
    );
}

#[test]
fn test_build_dashboard_model_orders_live_sessions_before_recent_sessions() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![
        sample_summary(
            AgentRuntime::Codex,
            "recent-newest",
            "/repo/.worktrees/recent-newest",
            AgentSessionState::Recent,
            11,
            false,
            AgentSessionSource::SessionStore,
        ),
        sample_summary(
            AgentRuntime::Claude,
            "live-older",
            "/repo/.worktrees/live-older",
            AgentSessionState::Working,
            9,
            true,
            AgentSessionSource::LiveStatus,
        ),
        sample_summary(
            AgentRuntime::Codex,
            "recent-older",
            "/repo/.worktrees/recent-older",
            AgentSessionState::Recent,
            8,
            false,
            AgentSessionSource::SessionStore,
        ),
        sample_summary(
            AgentRuntime::Codex,
            "live-newest",
            "/repo/.worktrees/live-newest",
            AgentSessionState::Processing,
            10,
            true,
            AgentSessionSource::Merged,
        ),
    ];

    let model = build_dashboard_model(&sessions, now);
    let ordered_ids: Vec<_> = model
        .rows
        .iter()
        .map(|row| row.session.session_id.as_deref())
        .collect();

    assert_eq!(
        ordered_ids,
        vec![
            Some("live-newest"),
            Some("live-older"),
            Some("recent-newest"),
            Some("recent-older"),
        ]
    );
}

#[test]
fn test_session_detail_lines_include_expected_fields() {
    let summary = sample_summary(
        AgentRuntime::Codex,
        "session-123",
        "/repo/.worktrees/agents",
        AgentSessionState::Working,
        11,
        true,
        AgentSessionSource::Merged,
    );

    let lines = session_detail_lines(&summary);
    let cwd_line = lines
        .iter()
        .find(|line| line.starts_with("CWD: "))
        .expect("CWD line should be present");

    assert_eq!(
        PathBuf::from(cwd_line.trim_start_matches("CWD: ")),
        summary.cwd
    );
    assert!(lines.iter().any(|line| line == "Agent: Parfit (worker)"));
    assert!(lines.iter().any(|line| line == "Session ID: session-123"));
    assert!(lines.iter().any(|line| line == "Runtime: Codex"));
    assert!(lines.iter().any(|line| line == "Branch: feat/agents"));
    assert!(lines.iter().any(|line| line == "State: working"));
    assert!(lines.iter().any(|line| line == "Presence: live"));
    assert!(
        lines
            .iter()
            .any(|line| line == &format!("Last Activity: {}", summary.last_activity.to_rfc3339()))
    );
    assert!(lines.iter().any(|line| line == "Source: Merged"));
}

#[test]
fn test_build_dashboard_model_renders_future_timestamps_explicitly() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![sample_summary(
        AgentRuntime::Codex,
        "future-session",
        "/repo/.worktrees/future",
        AgentSessionState::Recent,
        12,
        false,
        AgentSessionSource::SessionStore,
    )];

    let model = build_dashboard_model(&sessions, now - chrono::Duration::minutes(5));

    assert_eq!(model.rows.len(), 1);
    assert_eq!(model.rows[0].relative_time, "in 5m");
}

#[test]
fn test_build_dashboard_model_exposes_plain_state_labels() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![sample_summary(
        AgentRuntime::Codex,
        "waiting-session",
        "/repo/.worktrees/waiting",
        AgentSessionState::Waiting,
        11,
        true,
        AgentSessionSource::LiveStatus,
    )];

    let model = build_dashboard_model(&sessions, now);

    assert_eq!(model.rows.len(), 1);
    assert_eq!(model.rows[0].state_symbol, "!");
    assert_eq!(model.rows[0].state_label, "waiting");
}

#[test]
fn test_agents_dashboard_accepts_discovery() {
    let discovery = AgentDiscovery::new(vec![PathBuf::from("/repo")]);
    let _dashboard = AgentsDashboard::new(discovery);
}

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
    assert_eq!(model.rows[0].badges, vec!["primary", "current", "dirty"]);
    assert_eq!(model.rows[1].branch_label, "(detached HEAD: abcdef01)");
    assert_eq!(model.rows[1].badges, vec!["detached", "occupied"]);
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
fn test_next_bulk_selection_state_selects_all_unless_all_are_selected() {
    assert!(next_bulk_selection_state(&[false, false]));
    assert!(next_bulk_selection_state(&[true, false]));
    assert!(!next_bulk_selection_state(&[true, true]));
    assert!(!next_bulk_selection_state(&[]));
}
