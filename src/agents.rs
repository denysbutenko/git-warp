use crate::error::Result;
use chrono::{DateTime, Duration, Local};
use ignore::WalkBuilder;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum AgentRuntime {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum AgentSessionState {
    Working,
    Processing,
    Waiting,
    Completed,
    Recent,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
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

#[derive(Debug, Clone)]
pub struct AgentDiscovery {
    monitored_paths: Vec<PathBuf>,
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

fn status_file_path(root: &Path, runtime: AgentRuntime) -> PathBuf {
    match runtime {
        AgentRuntime::Claude => root.join(".claude").join("git-warp").join("status"),
        AgentRuntime::Codex => root.join(".codex").join("git-warp").join("status"),
    }
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

fn is_fallback_label(runtime: AgentRuntime, label: &str) -> bool {
    let label = label.trim();
    label.is_empty()
        || matches!(
            (runtime, label),
            (AgentRuntime::Claude, "Claude") | (AgentRuntime::Codex, "Codex")
        )
}

fn preferred_group<'a>(items: &'a [AgentSessionSummary]) -> Vec<&'a AgentSessionSummary> {
    let mut refs: Vec<&AgentSessionSummary> = items.iter().collect();
    refs.sort_by(|a, b| {
        b.is_live
            .cmp(&a.is_live)
            .then_with(|| b.last_activity.cmp(&a.last_activity))
            .then_with(|| a.cwd.cmp(&b.cwd))
    });
    refs
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum AgentSessionKey {
    SessionId {
        runtime: AgentRuntime,
        session_id: String,
    },
    Cwd {
        runtime: AgentRuntime,
        cwd: PathBuf,
    },
}

fn session_key(session: &AgentSessionSummary) -> AgentSessionKey {
    match &session.session_id {
        Some(session_id) => AgentSessionKey::SessionId {
            runtime: session.runtime,
            session_id: session_id.clone(),
        },
        None => AgentSessionKey::Cwd {
            runtime: session.runtime,
            cwd: session.cwd.clone(),
        },
    }
}

fn jsonl_files_under(root: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_exclude(false)
        .git_global(false)
        .build()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type()?;
            if !file_type.is_file() {
                return None;
            }
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                return None;
            }
            Some(entry.path().to_path_buf())
        })
        .collect()
}

fn load_session_file_lines(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

impl AgentDiscovery {
    pub fn new(monitored_paths: Vec<PathBuf>) -> Self {
        Self { monitored_paths }
    }

    pub fn keep_session(&self, session: &AgentSessionSummary, now: DateTime<Local>) -> bool {
        if now.signed_duration_since(session.last_activity) > Duration::days(7) {
            return false;
        }

        self.monitored_paths
            .iter()
            .any(|root| is_path_within_root(&session.cwd, root))
    }

    pub fn discover(&self, now: DateTime<Local>) -> Result<Vec<AgentSessionSummary>> {
        let live_sessions = merge_session_summaries(self.load_live_statuses()?);
        let recent_history = self.load_recent_history_sessions(now)?;
        let merged_history = merge_session_summaries(recent_history);
        let mut merged = merge_live_sessions(live_sessions, merged_history);
        sort_session_summaries(&mut merged);
        Ok(merged)
    }

    pub fn load_live_statuses(&self) -> Result<Vec<AgentSessionSummary>> {
        let mut sessions = Vec::new();

        for root in &self.monitored_paths {
            for runtime in [AgentRuntime::Claude, AgentRuntime::Codex] {
                let status_path = status_file_path(root, runtime);
                if let Some(summary) = parse_live_status_file(runtime, &status_path)? {
                    sessions.push(summary);
                }
            }
        }

        Ok(sessions)
    }

    pub fn load_codex_sessions(&self) -> Result<Vec<AgentSessionSummary>> {
        let Some(home_dir) = dirs::home_dir() else {
            return Ok(Vec::new());
        };
        let sessions_root = home_dir.join(".codex").join("sessions");

        let mut sessions = Vec::new();
        for file_path in jsonl_files_under(&sessions_root) {
            let Some(content) = load_session_file_lines(&file_path) else {
                continue;
            };
            if let Some(summary) = parse_codex_session_file(&content) {
                sessions.push(summary);
            }
        }

        Ok(sessions)
    }

    pub fn load_claude_sessions(&self) -> Result<Vec<AgentSessionSummary>> {
        let Some(home_dir) = dirs::home_dir() else {
            return Ok(Vec::new());
        };
        let sessions_root = home_dir.join(".claude").join("projects");

        let mut sessions = Vec::new();
        for file_path in jsonl_files_under(&sessions_root) {
            let Some(content) = load_session_file_lines(&file_path) else {
                continue;
            };
            sessions.extend(content.lines().filter_map(parse_claude_session_event_line));
        }

        Ok(sessions)
    }

    fn load_recent_history_sessions(
        &self,
        now: DateTime<Local>,
    ) -> Result<Vec<AgentSessionSummary>> {
        let mut sessions = Vec::new();

        sessions.extend(
            self.load_codex_sessions()?
                .into_iter()
                .filter(|session| self.keep_session(session, now)),
        );
        sessions.extend(
            self.load_claude_sessions()?
                .into_iter()
                .filter(|session| self.keep_session(session, now)),
        );

        Ok(sessions)
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

    let value: Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
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
        branch: payload
            .get("gitBranch")
            .or_else(|| payload.get("branch"))
            .and_then(|v| v.as_str())
            .map(str::to_string),
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

fn parse_codex_session_file(content: &str) -> Option<AgentSessionSummary> {
    let mut session = None;
    let mut newest_event_timestamp = None;

    for line in content.lines() {
        let value: Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(timestamp) = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_timestamp)
        {
            newest_event_timestamp = Some(match newest_event_timestamp {
                Some(current) if current >= timestamp => current,
                _ => timestamp,
            });
        }

        if value.get("type").and_then(|v| v.as_str()) == Some("session_meta") {
            session = parse_codex_session_meta_line(line);
        }
    }

    let mut session = session?;
    if let Some(timestamp) = newest_event_timestamp {
        session.last_activity = timestamp;
    }

    Some(session)
}

pub fn merge_session_summaries(items: Vec<AgentSessionSummary>) -> Vec<AgentSessionSummary> {
    let mut grouped: HashMap<AgentSessionKey, Vec<AgentSessionSummary>> = HashMap::new();

    for item in items {
        grouped.entry(session_key(&item)).or_default().push(item);
    }

    let mut merged = Vec::with_capacity(grouped.len());

    for mut group in grouped.into_values() {
        if group.len() == 1 {
            merged.push(group.pop().unwrap());
            continue;
        }

        let ordered = preferred_group(&group);
        merged.push(merge_session_group(ordered));
    }

    merged
}

fn merge_session_group(items: Vec<&AgentSessionSummary>) -> AgentSessionSummary {
    let mut selected = items[0].clone();
    let newest_timestamp = items
        .iter()
        .map(|item| item.last_activity)
        .max()
        .unwrap_or(selected.last_activity);
    let live_item = items.iter().copied().find(|item| item.is_live);
    let session_store_item = items
        .iter()
        .copied()
        .find(|item| matches!(item.source, AgentSessionSource::SessionStore));

    if let Some(item) = live_item {
        selected.state = item.state;
        selected.is_live = true;
    }

    if selected.branch.is_none()
        || is_fallback_label(selected.runtime, selected.agent_label.as_str())
    {
        if let Some(branch) = items
            .iter()
            .filter_map(|item| item.branch.clone())
            .find(|branch| !branch.is_empty())
        {
            selected.branch = Some(branch);
        }
    }

    if is_fallback_label(selected.runtime, &selected.agent_label)
        && session_store_item
            .map(|item| !is_fallback_label(selected.runtime, &item.agent_label))
            .unwrap_or(false)
    {
        if let Some(label) = items
            .iter()
            .map(|item| item.agent_label.as_str())
            .find(|label| !is_fallback_label(selected.runtime, label))
        {
            selected.agent_label = label.to_string();
        }
    }

    if selected.session_id.is_none() {
        selected.session_id = items.iter().find_map(|item| item.session_id.clone());
    }

    selected.last_activity = newest_timestamp;
    selected.source = AgentSessionSource::Merged;
    selected
}

fn merge_live_sessions(
    live_sessions: Vec<AgentSessionSummary>,
    history_sessions: Vec<AgentSessionSummary>,
) -> Vec<AgentSessionSummary> {
    let mut history_by_cwd: HashMap<(AgentRuntime, PathBuf), Vec<AgentSessionSummary>> =
        HashMap::new();

    for session in history_sessions {
        history_by_cwd
            .entry((session.runtime, session.cwd.clone()))
            .or_default()
            .push(session);
    }

    let mut merged = Vec::new();

    for live in live_sessions {
        let key = (live.runtime, live.cwd.clone());
        match history_by_cwd.remove(&key) {
            Some(history_group) if history_group.len() == 1 => {
                merged.push(merge_session_group(vec![&live, &history_group[0]]));
            }
            Some(mut history_group) => {
                history_group.sort_by(history_merge_order);
                let newest_history = history_group.remove(0);
                merged.push(merge_session_group(vec![&live, &newest_history]));
                merged.extend(history_group);
            }
            None => merged.push(live),
        }
    }

    for history_group in history_by_cwd.into_values() {
        merged.extend(history_group);
    }

    merged
}

fn history_merge_order(a: &AgentSessionSummary, b: &AgentSessionSummary) -> std::cmp::Ordering {
    b.last_activity
        .cmp(&a.last_activity)
        .then_with(|| a.session_id.cmp(&b.session_id))
        .then_with(|| a.branch.cmp(&b.branch))
        .then_with(|| a.agent_label.cmp(&b.agent_label))
        .then_with(|| a.cwd.cmp(&b.cwd))
}

pub fn sort_session_summaries(items: &mut [AgentSessionSummary]) {
    items.sort_by(|a, b| {
        b.is_live
            .cmp(&a.is_live)
            .then_with(|| b.last_activity.cmp(&a.last_activity))
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.cwd.cmp(&b.cwd))
    });
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
