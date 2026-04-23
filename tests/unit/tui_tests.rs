use chrono::{Local, TimeZone};
use git_warp::agents::{
    AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
};
use git_warp::tui::{AgentsDashboard, build_dashboard_model, session_detail_lines};
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
            "No live or recent Claude/Codex sessions found for this repo in the last 7 days."
                .to_string(),
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

    assert_eq!(
        lines,
        vec![
            "Agent: Parfit (worker)".to_string(),
            "CWD: /repo/.worktrees/agents".to_string(),
            "Session ID: session-123".to_string(),
            "Runtime: Codex".to_string(),
            "Branch: feat/agents".to_string(),
            "Presence: live".to_string(),
            format!("Last Activity: {}", summary.last_activity.to_rfc3339()),
            "Source: Merged".to_string(),
        ]
    );
}

#[test]
fn test_agents_dashboard_accepts_discovery() {
    let discovery = AgentDiscovery::new(vec![PathBuf::from("/repo")]);
    let _dashboard = AgentsDashboard::new(discovery);
}
