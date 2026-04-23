use chrono::{DateTime, Local, TimeZone};
use git_warp::agents::{
    AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
    merge_session_summaries, parse_claude_session_event_line, parse_codex_session_meta_line,
    parse_live_status_file, sort_session_summaries,
};
use std::fs;
use std::path::PathBuf;

fn sample_summary(
    runtime: AgentRuntime,
    session_id: Option<&str>,
    cwd: &str,
    state: AgentSessionState,
    last_activity_hour: u32,
    is_live: bool,
    source: AgentSessionSource,
) -> AgentSessionSummary {
    AgentSessionSummary {
        runtime,
        session_id: session_id.map(str::to_string),
        cwd: PathBuf::from(cwd),
        branch: Some("feat".to_string()),
        agent_label: "agent".to_string(),
        state,
        last_activity: Local
            .with_ymd_and_hms(2026, 4, 23, last_activity_hour, 0, 0)
            .unwrap(),
        is_live,
        source,
    }
}

#[test]
fn test_parse_live_status_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    let status_path = temp_dir.path().join("status");
    fs::write(
        &status_path,
        r#"{"status":"working","last_activity":"2026-04-23T10:15:00+07:00"}"#,
    )
    .unwrap();

    let summary = parse_live_status_file(AgentRuntime::Codex, &status_path)
        .unwrap()
        .expect("status file should parse");

    assert_eq!(summary.runtime, AgentRuntime::Codex);
    assert_eq!(summary.state, AgentSessionState::Working);
    assert!(summary.is_live);
}

#[test]
fn test_parse_live_status_file_falls_back_to_modified_time() {
    let temp_dir = tempfile::tempdir().unwrap();
    let status_path = temp_dir.path().join("status");
    fs::write(
        &status_path,
        r#"{"status":"working","last_activity":"not-a-time"}"#,
    )
    .unwrap();

    let summary = parse_live_status_file(AgentRuntime::Codex, &status_path)
        .unwrap()
        .expect("status file should parse");
    let modified = DateTime::<Local>::from(fs::metadata(&status_path).unwrap().modified().unwrap());

    assert_eq!(summary.last_activity, modified);
}

#[test]
fn test_parse_live_status_file_malformed_json_returns_none() {
    let temp_dir = tempfile::tempdir().unwrap();
    let status_path = temp_dir.path().join("status");

    for content in ["", "{", r#"{"status":"working""#] {
        fs::write(&status_path, content).unwrap();

        assert_eq!(
            parse_live_status_file(AgentRuntime::Codex, &status_path).unwrap(),
            None
        );
    }
}

#[test]
fn test_parse_codex_session_meta_line() {
    let line = r#"{"timestamp":"2026-04-23T05:35:14.983Z","type":"session_meta","payload":{"id":"019db8d5-cf8c-7c10-ab48-7f495e8dc54b","timestamp":"2026-04-23T05:35:13.308Z","cwd":"/tmp/repo/.worktrees/feat","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker"}}"#;

    let summary = parse_codex_session_meta_line(line).expect("codex session should parse");

    assert_eq!(summary.runtime, AgentRuntime::Codex);
    assert_eq!(
        summary.session_id.as_deref(),
        Some("019db8d5-cf8c-7c10-ab48-7f495e8dc54b")
    );
    assert_eq!(summary.agent_label, "Parfit (worker)");
    assert_eq!(summary.state, AgentSessionState::Recent);
}

#[test]
fn test_parse_claude_session_event_line() {
    let line = r#"{"type":"user","timestamp":"2026-04-23T04:10:00.000Z","cwd":"/tmp/repo","sessionId":"claude-session-1","gitBranch":"main"}"#;

    let summary = parse_claude_session_event_line(line).expect("claude event should parse");

    assert_eq!(summary.runtime, AgentRuntime::Claude);
    assert_eq!(summary.session_id.as_deref(), Some("claude-session-1"));
    assert_eq!(summary.branch.as_deref(), Some("main"));
    assert_eq!(
        summary.last_activity.timestamp(),
        DateTime::parse_from_rfc3339("2026-04-23T04:10:00Z")
            .unwrap()
            .with_timezone(&Local)
            .timestamp()
    );
}

#[test]
fn test_merge_prefers_live_state_and_newest_timestamp() {
    let merged = merge_session_summaries(vec![
        sample_summary(
            AgentRuntime::Codex,
            Some("session-1"),
            "/repo/.worktrees/feat",
            AgentSessionState::Recent,
            9,
            false,
            AgentSessionSource::SessionStore,
        ),
        sample_summary(
            AgentRuntime::Codex,
            Some("session-1"),
            "/repo/.worktrees/feat",
            AgentSessionState::Working,
            10,
            true,
            AgentSessionSource::LiveStatus,
        ),
    ]);

    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].state, AgentSessionState::Working);
    assert!(merged[0].is_live);
    assert_eq!(merged[0].source, AgentSessionSource::Merged);
}

#[test]
fn test_sort_session_summaries_keeps_live_rows_first() {
    let mut items = vec![
        sample_summary(
            AgentRuntime::Claude,
            Some("claude-1"),
            "/repo",
            AgentSessionState::Recent,
            11,
            false,
            AgentSessionSource::SessionStore,
        ),
        sample_summary(
            AgentRuntime::Codex,
            Some("codex-1"),
            "/repo/.worktrees/feat",
            AgentSessionState::Working,
            10,
            true,
            AgentSessionSource::LiveStatus,
        ),
    ];

    sort_session_summaries(&mut items);

    assert_eq!(items[0].session_id.as_deref(), Some("codex-1"));
    assert_eq!(items[1].session_id.as_deref(), Some("claude-1"));
}

#[test]
fn test_discover_for_repo_filters_by_worktree_and_recency() {
    let discovery = AgentDiscovery::new(vec![
        PathBuf::from("/repo"),
        PathBuf::from("/repo/.worktrees/feat"),
    ]);

    let kept = discovery.keep_session(
        &sample_summary(
            AgentRuntime::Codex,
            Some("inside"),
            "/repo/.worktrees/feat",
            AgentSessionState::Recent,
            10,
            false,
            AgentSessionSource::SessionStore,
        ),
        Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap(),
    );

    let rejected = discovery.keep_session(
        &AgentSessionSummary {
            last_activity: Local.with_ymd_and_hms(2026, 4, 10, 12, 0, 0).unwrap(),
            cwd: PathBuf::from("/other/repo"),
            ..sample_summary(
                AgentRuntime::Claude,
                Some("outside"),
                "/other/repo",
                AgentSessionState::Recent,
                9,
                false,
                AgentSessionSource::SessionStore,
            )
        },
        Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap(),
    );

    assert!(kept);
    assert!(!rejected);
}
