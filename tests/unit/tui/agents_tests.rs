use chrono::{Local, TimeZone};
use git_warp::agents::{
    AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
};
use git_warp::tui::{
    AgentPresenceFilter, AgentRuntimeFilter, AgentsDashboard, DashboardFilters,
    build_dashboard_model, build_dashboard_model_filtered_windowed, build_dashboard_model_windowed,
    is_stale_session, session_detail_lines,
};
use std::path::PathBuf;

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
fn test_build_dashboard_model_windowed_only_formats_visible_rows() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = (0..200)
        .map(|index| {
            let mut summary = sample_summary(
                AgentRuntime::Codex,
                &format!("session-{index:03}"),
                &format!("/repo/.worktrees/session-{index:03}"),
                AgentSessionState::Recent,
                11,
                false,
                AgentSessionSource::SessionStore,
            );
            summary.branch = Some(format!("session-{index:03}"));
            summary
        })
        .collect::<Vec<_>>();

    let model = build_dashboard_model_windowed(&sessions, now, 120, 12);

    assert_eq!(model.total_rows, 200);
    assert_eq!(model.start_index, 114);
    assert_eq!(model.rows.len(), 12);
    assert_eq!(
        model.rows[6].session.session_id.as_deref(),
        Some("session-120")
    );
}

#[test]
fn test_agents_dashboard_accepts_discovery() {
    let discovery = AgentDiscovery::new(vec![PathBuf::from("/repo")]);
    let _dashboard = AgentsDashboard::new(discovery);
}

fn dashboard_filter_sample(
    runtime: AgentRuntime,
    session_id: &str,
    cwd: &str,
    is_live: bool,
    last_activity_hour: u32,
) -> AgentSessionSummary {
    let state = if is_live {
        AgentSessionState::Working
    } else {
        AgentSessionState::Recent
    };
    let source = if is_live {
        AgentSessionSource::LiveStatus
    } else {
        AgentSessionSource::SessionStore
    };
    sample_summary(
        runtime,
        session_id,
        cwd,
        state,
        last_activity_hour,
        is_live,
        source,
    )
}

#[test]
fn test_is_stale_session_true_when_recent_and_older_than_a_day() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let mut session = dashboard_filter_sample(
        AgentRuntime::Codex,
        "stale",
        "/repo/.worktrees/stale",
        false,
        11,
    );
    session.last_activity = now - chrono::Duration::hours(48);

    assert!(is_stale_session(&session, now));
}

#[test]
fn test_is_stale_session_false_when_live_even_if_old() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let mut session = dashboard_filter_sample(
        AgentRuntime::Claude,
        "live-old",
        "/repo/.worktrees/live-old",
        true,
        11,
    );
    session.last_activity = now - chrono::Duration::hours(72);

    assert!(!is_stale_session(&session, now));
}

#[test]
fn test_is_stale_session_false_when_recent_but_fresh() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let mut session = dashboard_filter_sample(
        AgentRuntime::Codex,
        "fresh",
        "/repo/.worktrees/fresh",
        false,
        11,
    );
    session.last_activity = now - chrono::Duration::hours(2);

    assert!(!is_stale_session(&session, now));
}

#[test]
fn test_build_dashboard_model_marks_stale_rows() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let mut stale = dashboard_filter_sample(
        AgentRuntime::Codex,
        "stale",
        "/repo/.worktrees/stale",
        false,
        11,
    );
    stale.last_activity = now - chrono::Duration::hours(48);
    let fresh = dashboard_filter_sample(
        AgentRuntime::Codex,
        "fresh",
        "/repo/.worktrees/fresh",
        false,
        11,
    );

    let model = build_dashboard_model(&[stale, fresh], now);

    let stale_row = model
        .rows
        .iter()
        .find(|row| row.session.session_id.as_deref() == Some("stale"))
        .expect("stale row");
    let fresh_row = model
        .rows
        .iter()
        .find(|row| row.session.session_id.as_deref() == Some("fresh"))
        .expect("fresh row");
    assert!(stale_row.is_stale);
    assert!(!fresh_row.is_stale);
}

#[test]
fn test_build_dashboard_model_filtered_filters_runtime_codex() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![
        dashboard_filter_sample(
            AgentRuntime::Claude,
            "claude-1",
            "/repo/.worktrees/claude-1",
            true,
            11,
        ),
        dashboard_filter_sample(
            AgentRuntime::Codex,
            "codex-1",
            "/repo/.worktrees/codex-1",
            true,
            10,
        ),
    ];

    let filters = DashboardFilters {
        runtime: AgentRuntimeFilter::Codex,
        presence: AgentPresenceFilter::All,
    };
    let model = build_dashboard_model_filtered_windowed(&sessions, now, 0, 10, filters);

    assert_eq!(model.total_unfiltered, 2);
    assert_eq!(model.total_rows, 1);
    assert_eq!(model.rows[0].session.session_id.as_deref(), Some("codex-1"));
}

#[test]
fn test_build_dashboard_model_filtered_filters_presence_live() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![
        dashboard_filter_sample(
            AgentRuntime::Codex,
            "live-one",
            "/repo/.worktrees/live-one",
            true,
            11,
        ),
        dashboard_filter_sample(
            AgentRuntime::Codex,
            "recent-one",
            "/repo/.worktrees/recent-one",
            false,
            10,
        ),
    ];

    let filters = DashboardFilters {
        runtime: AgentRuntimeFilter::All,
        presence: AgentPresenceFilter::Live,
    };
    let model = build_dashboard_model_filtered_windowed(&sessions, now, 0, 10, filters);

    assert_eq!(model.rows.len(), 1);
    assert_eq!(
        model.rows[0].session.session_id.as_deref(),
        Some("live-one")
    );
}

#[test]
fn test_build_dashboard_model_filter_summary_text_describes_active_filters() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![dashboard_filter_sample(
        AgentRuntime::Codex,
        "codex-1",
        "/repo/.worktrees/codex-1",
        true,
        11,
    )];

    let filters = DashboardFilters {
        runtime: AgentRuntimeFilter::Codex,
        presence: AgentPresenceFilter::Live,
    };
    let model = build_dashboard_model_filtered_windowed(&sessions, now, 0, 10, filters);

    assert_eq!(model.filter_summary, "Filter: Codex · live");
}

#[test]
fn test_build_dashboard_model_filter_summary_empty_when_no_filters_active() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![dashboard_filter_sample(
        AgentRuntime::Codex,
        "codex-1",
        "/repo/.worktrees/codex-1",
        true,
        11,
    )];

    let model =
        build_dashboard_model_filtered_windowed(&sessions, now, 0, 10, DashboardFilters::default());

    assert!(model.filter_summary.is_empty());
}

#[test]
fn test_build_dashboard_model_filtered_empty_state_explains_filter() {
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = vec![dashboard_filter_sample(
        AgentRuntime::Claude,
        "claude-1",
        "/repo/.worktrees/claude-1",
        true,
        11,
    )];

    let filters = DashboardFilters {
        runtime: AgentRuntimeFilter::Codex,
        presence: AgentPresenceFilter::All,
    };
    let model = build_dashboard_model_filtered_windowed(&sessions, now, 0, 10, filters);

    assert!(model.rows.is_empty());
    assert_eq!(model.total_unfiltered, 1);
    assert!(
        model
            .empty_state_lines
            .iter()
            .any(|line| line.contains("filter")),
        "expected empty state to mention the active filter, got {:?}",
        model.empty_state_lines
    );
}
