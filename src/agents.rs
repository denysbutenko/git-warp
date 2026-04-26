use crate::error::Result;
use chrono::{DateTime, Duration, Local};
use ignore::WalkBuilder;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const DEFAULT_HISTORY_LIMIT: usize = 100;

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
    max_history_sessions: usize,
}

#[derive(Debug, Clone)]
struct SessionFileCandidate {
    runtime: AgentRuntime,
    path: PathBuf,
    modified: DateTime<Local>,
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

fn normalize_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn is_path_within_root(path: &Path, root: &Path) -> bool {
    let path = normalize_path(path);
    let root = normalize_path(root);
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

fn session_file_candidates_under(
    root: &Path,
    runtime: AgentRuntime,
    cutoff: DateTime<Local>,
) -> Vec<SessionFileCandidate> {
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

            let modified = fs::metadata(entry.path())
                .ok()
                .and_then(|meta| meta.modified().ok())
                .map(DateTime::<Local>::from)?;
            if modified < cutoff {
                return None;
            }

            Some(SessionFileCandidate {
                runtime,
                path: entry.path().to_path_buf(),
                modified,
            })
        })
        .collect()
}

impl AgentDiscovery {
    pub fn new(monitored_paths: Vec<PathBuf>) -> Self {
        Self::with_max_history_sessions(monitored_paths, DEFAULT_HISTORY_LIMIT)
    }

    pub fn with_max_history_sessions(
        monitored_paths: Vec<PathBuf>,
        max_history_sessions: usize,
    ) -> Self {
        Self {
            monitored_paths,
            max_history_sessions: max_history_sessions.max(1),
        }
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

    fn load_recent_history_sessions(
        &self,
        now: DateTime<Local>,
    ) -> Result<Vec<AgentSessionSummary>> {
        let Some(home_dir) = dirs::home_dir() else {
            return Ok(Vec::new());
        };

        let cutoff = now - Duration::days(7);
        let mut candidates = Vec::new();
        candidates.extend(session_file_candidates_under(
            &home_dir.join(".codex").join("sessions"),
            AgentRuntime::Codex,
            cutoff,
        ));
        candidates.extend(session_file_candidates_under(
            &home_dir.join(".claude").join("projects"),
            AgentRuntime::Claude,
            cutoff,
        ));
        candidates.sort_by(|a, b| {
            b.modified
                .cmp(&a.modified)
                .then_with(|| b.path.cmp(&a.path))
        });

        let mut sessions = Vec::new();
        for candidate in candidates {
            let summary = match candidate.runtime {
                AgentRuntime::Codex => {
                    parse_codex_session_file_at_path(&candidate.path, candidate.modified)
                }
                AgentRuntime::Claude => {
                    parse_claude_session_file_at_path(&candidate.path, candidate.modified)
                }
            };

            if let Some(session) = summary.filter(|session| self.keep_session(session, now)) {
                sessions.push(session);
                if sessions.len() >= self.max_history_sessions {
                    break;
                }
            }
        }

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
        .unwrap_or_else(|| Path::new("."));

    Ok(Some(AgentSessionSummary {
        runtime,
        session_id: None,
        cwd: normalize_path(cwd),
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
    if value.get("type").and_then(|v| v.as_str()) != Some("session_meta") {
        return None;
    }
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
        cwd: normalize_path(Path::new(cwd)),
        branch: payload
            .get("git")
            .and_then(|git| git.get("branch"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| {
                payload
                    .get("gitBranch")
                    .or_else(|| payload.get("branch"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            }),
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

fn parse_codex_session_file_at_path(
    path: &Path,
    fallback_last_activity: DateTime<Local>,
) -> Option<AgentSessionSummary> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut session = None;

    for line in reader.lines().map_while(|line| line.ok()) {
        if let Some(summary) = parse_codex_session_meta_line(&line) {
            session = Some(summary);
            break;
        }
    }

    let mut session = session?;
    session.last_activity = last_jsonl_timestamp(path).unwrap_or(fallback_last_activity);
    Some(session)
}

fn parse_claude_session_file_at_path(
    path: &Path,
    fallback_last_activity: DateTime<Local>,
) -> Option<AgentSessionSummary> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let mut session = None;

    for line in reader.lines().map_while(|line| line.ok()) {
        if let Some(summary) = parse_claude_session_event_line(&line) {
            session = Some(summary);
            break;
        }
    }

    let mut session = session?;
    session.last_activity = last_jsonl_timestamp(path).unwrap_or(fallback_last_activity);
    Some(session)
}

fn last_jsonl_timestamp(path: &Path) -> Option<DateTime<Local>> {
    let line = last_non_empty_line(path)?;
    let value: Value = serde_json::from_str(&line).ok()?;
    value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .and_then(parse_timestamp)
}

fn last_non_empty_line(path: &Path) -> Option<String> {
    const CHUNK_SIZE: u64 = 8 * 1024;
    const MAX_TAIL_BYTES: usize = 64 * 1024;

    let mut file = File::open(path).ok()?;
    let mut cursor = file.seek(SeekFrom::End(0)).ok()?;
    let mut bytes = Vec::new();

    while cursor > 0 && bytes.len() < MAX_TAIL_BYTES {
        let read_size = CHUNK_SIZE.min(cursor) as usize;
        cursor -= read_size as u64;
        file.seek(SeekFrom::Start(cursor)).ok()?;

        let mut chunk = vec![0; read_size];
        file.read_exact(&mut chunk).ok()?;
        chunk.extend_from_slice(&bytes);
        bytes = chunk;

        if bytes.iter().filter(|byte| **byte == b'\n').count() >= 2 {
            break;
        }
    }

    let text = String::from_utf8_lossy(&bytes);
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(str::to_string)
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
        .then_with(|| b.session_id.cmp(&a.session_id))
        .then_with(|| b.branch.cmp(&a.branch))
        .then_with(|| b.agent_label.cmp(&a.agent_label))
        .then_with(|| b.cwd.cmp(&a.cwd))
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
        cwd: normalize_path(Path::new(cwd)),
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
