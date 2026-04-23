use crate::error::Result;
use chrono::{DateTime, Local};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentRuntime {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentSessionState {
    Working,
    Processing,
    Waiting,
    Completed,
    Recent,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AgentSessionSource {
    LiveStatus,
    SessionStore,
    Merged,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AgentSessionSummary {
    pub runtime: AgentRuntime,
    pub session_id: Option<String>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
    pub agent_label: String,
    pub state: AgentSessionState,
    pub last_activity: DateTime<Local>,
    pub is_live: bool,
    pub source: AgentSessionSource,
}

fn parse_timestamp(value: &str) -> Option<DateTime<Local>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Local))
}

fn map_status(value: &str) -> AgentSessionState {
    match value {
        "working" => AgentSessionState::Working,
        "processing" => AgentSessionState::Processing,
        "waiting" => AgentSessionState::Waiting,
        "subagent_complete" => AgentSessionState::Completed,
        _ => AgentSessionState::Unknown,
    }
}

pub fn parse_live_status_file(
    runtime: AgentRuntime,
    status_path: &Path,
) -> Result<Option<AgentSessionSummary>> {
    let content = match fs::read_to_string(status_path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };

    let value: Value = serde_json::from_str(&content)?;
    let last_activity = value
        .get("last_activity")
        .and_then(|v| v.as_str())
        .and_then(parse_timestamp)
        .unwrap_or_else(|| {
            fs::metadata(status_path)
                .ok()
                .and_then(|meta| meta.modified().ok())
                .map(DateTime::<Local>::from)
                .unwrap_or_else(Local::now)
        });

    let cwd = status_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    Ok(Some(AgentSessionSummary {
        runtime,
        session_id: None,
        cwd,
        branch: None,
        agent_label: match runtime {
            AgentRuntime::Claude => "Claude".to_string(),
            AgentRuntime::Codex => "Codex".to_string(),
        },
        state: value
            .get("status")
            .and_then(|v| v.as_str())
            .map(map_status)
            .unwrap_or(AgentSessionState::Unknown),
        last_activity,
        is_live: true,
        source: AgentSessionSource::LiveStatus,
    }))
}

pub fn parse_codex_session_meta_line(line: &str) -> Option<AgentSessionSummary> {
    let value: Value = serde_json::from_str(line).ok()?;
    let payload = value.get("payload")?;
    let cwd = payload.get("cwd")?.as_str()?;
    let nickname = payload.get("agent_nickname").and_then(|v| v.as_str());
    let role = payload.get("agent_role").and_then(|v| v.as_str());
    let agent_label = match (nickname, role) {
        (Some(nickname), Some(role)) => format!("{nickname} ({role})"),
        (Some(nickname), None) => nickname.to_string(),
        _ => "Codex".to_string(),
    };

    Some(AgentSessionSummary {
        runtime: AgentRuntime::Codex,
        session_id: payload
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        cwd: PathBuf::from(cwd),
        branch: None,
        agent_label,
        state: AgentSessionState::Recent,
        last_activity: payload
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_timestamp)?,
        is_live: false,
        source: AgentSessionSource::SessionStore,
    })
}

pub fn parse_claude_session_event_line(line: &str) -> Option<AgentSessionSummary> {
    let value: Value = serde_json::from_str(line).ok()?;
    let cwd = value.get("cwd")?.as_str()?;

    Some(AgentSessionSummary {
        runtime: AgentRuntime::Claude,
        session_id: value
            .get("sessionId")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        cwd: PathBuf::from(cwd),
        branch: value
            .get("gitBranch")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        agent_label: "Claude".to_string(),
        state: AgentSessionState::Recent,
        last_activity: value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_timestamp)?,
        is_live: false,
        source: AgentSessionSource::SessionStore,
    })
}
