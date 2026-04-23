use chrono::{DateTime, Local};
use git_warp::agents::{
    AgentRuntime, AgentSessionState, parse_claude_session_event_line,
    parse_codex_session_meta_line, parse_live_status_file,
};
use std::fs;
use std::thread::sleep;
use std::time::Duration;

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
    fs::write(&status_path, r#"{"status":"working","last_activity":"not-a-time"}"#).unwrap();

    sleep(Duration::from_millis(1100));

    let summary = parse_live_status_file(AgentRuntime::Codex, &status_path)
        .unwrap()
        .expect("status file should parse");
    let modified = DateTime::<Local>::from(fs::metadata(&status_path).unwrap().modified().unwrap());

    assert_eq!(summary.last_activity, modified);
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
