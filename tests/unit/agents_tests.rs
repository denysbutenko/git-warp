use chrono::{DateTime, Local, TimeZone};
use git_warp::agents::{
    AgentDiscovery, AgentRuntime, AgentSessionSource, AgentSessionState, AgentSessionSummary,
    merge_session_summaries, parse_claude_session_event_line, parse_codex_session_meta_line,
    parse_live_status_file, sort_session_summaries,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static HOME_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

struct HomeOverride {
    original_home: Option<std::ffi::OsString>,
}

impl HomeOverride {
    fn set(temp_home: &PathBuf) -> Self {
        let original_home = env::var_os("HOME");
        unsafe {
            env::set_var("HOME", temp_home);
        }
        Self { original_home }
    }
}

impl Drop for HomeOverride {
    fn drop(&mut self) {
        match self.original_home.take() {
            Some(value) => unsafe {
                env::set_var("HOME", value);
            },
            None => unsafe {
                env::remove_var("HOME");
            },
        }
    }
}

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

fn home_guard() -> std::sync::MutexGuard<'static, ()> {
    HOME_GUARD.get_or_init(|| Mutex::new(())).lock().unwrap()
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
fn test_merge_keeps_recent_session_rows_with_distinct_ids() {
    let merged = merge_session_summaries(vec![
        AgentSessionSummary {
            session_id: Some("session-1".to_string()),
            branch: Some("feat".to_string()),
            agent_label: "Parfit (worker)".to_string(),
            state: AgentSessionState::Recent,
            last_activity: Local.with_ymd_and_hms(2026, 4, 23, 9, 0, 0).unwrap(),
            is_live: false,
            source: AgentSessionSource::SessionStore,
            ..sample_summary(
                AgentRuntime::Codex,
                Some("session-1"),
                "/repo/.worktrees/feat",
                AgentSessionState::Recent,
                9,
                false,
                AgentSessionSource::SessionStore,
            )
        },
        AgentSessionSummary {
            session_id: Some("session-2".to_string()),
            branch: Some("feat-2".to_string()),
            agent_label: "Parfit (worker)".to_string(),
            state: AgentSessionState::Recent,
            last_activity: Local.with_ymd_and_hms(2026, 4, 23, 10, 0, 0).unwrap(),
            is_live: false,
            source: AgentSessionSource::SessionStore,
            ..sample_summary(
                AgentRuntime::Codex,
                Some("session-2"),
                "/repo/.worktrees/feat",
                AgentSessionState::Recent,
                10,
                false,
                AgentSessionSource::SessionStore,
            )
        },
    ]);

    assert_eq!(merged.len(), 2);
    assert!(
        merged
            .iter()
            .any(|item| item.session_id.as_deref() == Some("session-1"))
    );
    assert!(
        merged
            .iter()
            .any(|item| item.session_id.as_deref() == Some("session-2"))
    );
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

#[test]
fn test_discover_keeps_distinct_recent_history_rows_without_live_status() {
    let _guard = home_guard();
    let temp_home = tempfile::tempdir().unwrap();
    let repo_root = tempfile::tempdir().unwrap();
    let worktree_root = repo_root.path().join(".worktrees").join("feat");
    let codex_sessions = temp_home.path().join(".codex").join("sessions");

    fs::create_dir_all(&codex_sessions).unwrap();

    fs::write(
        codex_sessions.join("sessions.jsonl"),
        format!(
            r#"{{"timestamp":"2026-04-22T09:00:00.000Z","type":"session_meta","payload":{{"id":"session-1","timestamp":"2026-04-22T09:00:00.000Z","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","gitBranch":"feat"}}}}
{{"timestamp":"2026-04-22T10:00:00.000Z","type":"session_meta","payload":{{"id":"session-2","timestamp":"2026-04-22T10:00:00.000Z","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","gitBranch":"feat-2"}}}}"#,
            worktree_root.display(),
            worktree_root.display()
        ),
    )
    .unwrap();

    let _home_override = HomeOverride::set(&temp_home.path().to_path_buf());

    let discovery =
        AgentDiscovery::new(vec![repo_root.path().to_path_buf(), worktree_root.clone()]);
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = discovery.discover(now).unwrap();

    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|session| {
        session.session_id.as_deref() == Some("session-1")
            && session.branch.as_deref() == Some("feat")
            && session.source == AgentSessionSource::SessionStore
    }));
    assert!(sessions.iter().any(|session| {
        session.session_id.as_deref() == Some("session-2")
            && session.branch.as_deref() == Some("feat-2")
            && session.source == AgentSessionSource::SessionStore
    }));
}

#[test]
fn test_discover_merges_live_row_with_newest_of_multiple_history_rows() {
    let _guard = home_guard();
    let temp_home = tempfile::tempdir().unwrap();
    let repo_root = tempfile::tempdir().unwrap();
    let worktree_root = repo_root.path().join(".worktrees").join("feat");
    let live_status = worktree_root.join(".codex").join("git-warp").join("status");
    let codex_sessions = temp_home.path().join(".codex").join("sessions");

    fs::create_dir_all(&live_status.parent().unwrap()).unwrap();
    fs::create_dir_all(&codex_sessions).unwrap();

    fs::write(
        &live_status,
        r#"{"status":"working","last_activity":"2026-04-23T10:00:00+00:00"}"#,
    )
    .unwrap();
    fs::write(
        codex_sessions.join("sessions.jsonl"),
        format!(
            r#"{{"timestamp":"2026-04-22T09:00:00.000Z","type":"session_meta","payload":{{"id":"session-1","timestamp":"2026-04-22T09:00:00.000Z","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","gitBranch":"feat"}}}}
{{"timestamp":"2026-04-22T10:00:00.000Z","type":"session_meta","payload":{{"id":"session-2","timestamp":"2026-04-22T10:00:00.000Z","cwd":"{}","originator":"codex-tui","agent_nickname":"Parfit","agent_role":"worker","gitBranch":"feat-2"}}}}"#,
            worktree_root.display(),
            worktree_root.display()
        ),
    )
    .unwrap();

    let _home_override = HomeOverride::set(&temp_home.path().to_path_buf());

    let discovery =
        AgentDiscovery::new(vec![repo_root.path().to_path_buf(), worktree_root.clone()]);
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = discovery.discover(now).unwrap();

    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|session| {
        session.is_live
            && session.source == AgentSessionSource::Merged
            && session.session_id.as_deref() == Some("session-2")
            && session.branch.as_deref() == Some("feat-2")
    }));
    assert!(sessions.iter().any(|session| {
        !session.is_live
            && session.source == AgentSessionSource::SessionStore
            && session.session_id.as_deref() == Some("session-1")
            && session.branch.as_deref() == Some("feat")
    }));
}

#[test]
fn test_discover_keeps_live_row_even_when_older_than_cutoff() {
    let _guard = home_guard();
    let temp_home = tempfile::tempdir().unwrap();
    let repo_root = tempfile::tempdir().unwrap();
    let worktree_root = repo_root.path().join(".worktrees").join("feat");
    let live_status = worktree_root.join(".codex").join("git-warp").join("status");

    fs::create_dir_all(&live_status.parent().unwrap()).unwrap();
    fs::write(
        &live_status,
        r#"{"status":"working","last_activity":"2026-04-10T10:00:00+00:00"}"#,
    )
    .unwrap();

    let _home_override = HomeOverride::set(&temp_home.path().to_path_buf());

    let discovery =
        AgentDiscovery::new(vec![repo_root.path().to_path_buf(), worktree_root.clone()]);
    let now = Local.with_ymd_and_hms(2026, 4, 23, 12, 0, 0).unwrap();
    let sessions = discovery.discover(now).unwrap();

    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.state, AgentSessionState::Working);
    assert!(session.is_live);
    assert_eq!(session.source, AgentSessionSource::LiveStatus);
    assert_eq!(session.branch.as_deref(), None);
    assert_eq!(session.session_id.as_deref(), None);
    assert_eq!(session.agent_label, "Codex");
}
