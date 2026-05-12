use crate::error::Result;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const GIT_WARP_HOOK_PREFIX: &str = "agent_status_";

const EXPECTED_EVENTS: &[&str] = &[
    "UserPromptSubmit",
    "Stop",
    "PreToolUse",
    "PostToolUse",
    "SubagentStop",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookRuntime {
    Claude,
    Codex,
}

impl HookRuntime {
    fn parse_many(runtime: &str) -> Result<Vec<Self>> {
        match runtime {
            "claude" => Ok(vec![Self::Claude]),
            "codex" => Ok(vec![Self::Codex]),
            "all" => Ok(vec![Self::Claude, Self::Codex]),
            _ => Err(anyhow::anyhow!(
                "Invalid runtime. Use: claude, codex, or all"
            )),
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    fn install_arg(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    fn status_root_dir(&self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Codex => ".codex",
        }
    }

    fn user_settings_path(&self) -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;

        Ok(match self {
            Self::Claude => home.join(".claude").join("settings.json"),
            Self::Codex => home.join(".codex").join("hooks.json"),
        })
    }

    fn project_settings_path(&self) -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;

        Ok(match self {
            Self::Claude => current_dir.join(".claude").join("settings.json"),
            Self::Codex => current_dir.join(".codex").join("hooks.json"),
        })
    }

    fn wraps_hooks_at_root(&self) -> bool {
        matches!(self, Self::Claude)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookScope {
    User,
    Project,
}

impl HookScope {
    fn label(&self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Project => "Project",
        }
    }

    fn install_arg(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookState {
    NotConfigured,
    Missing,
    Partial,
    Conflicting,
    Complete,
}

#[derive(Clone, Debug)]
pub struct HookDiagnosis {
    pub runtime: HookRuntime,
    pub scope: HookScope,
    pub path: PathBuf,
    pub state: HookState,
    pub present_events: Vec<String>,
    pub missing_events: Vec<String>,
    pub conflicting_events: Vec<String>,
    pub parse_error: Option<String>,
}

impl HookDiagnosis {
    pub fn install_command(&self) -> String {
        format!(
            "warp hooks-install --level {} --runtime {}",
            self.scope.install_arg(),
            self.runtime.install_arg()
        )
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self.state, HookState::Complete)
    }
}

pub struct HooksManager;

impl HooksManager {
    pub fn install_hooks(level: Option<&str>, runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        match level {
            Some("console") | None => {
                for (index, runtime) in runtimes.iter().enumerate() {
                    if index > 0 {
                        println!();
                    }
                    println!("Add this to your {} hook config:", runtime.display_name());
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&Self::get_hooks_config(*runtime))?
                    );
                }
                Ok(())
            }
            Some("user") => {
                for runtime in runtimes {
                    let settings_path = runtime.user_settings_path()?;
                    Self::merge_hooks_into_settings(settings_path, runtime)?;
                }
                Ok(())
            }
            Some("project") => {
                for runtime in runtimes {
                    let settings_path = runtime.project_settings_path()?;
                    Self::merge_hooks_into_settings(settings_path, runtime)?;
                }
                Ok(())
            }
            _ => {
                println!("Invalid level. Use: user, project, or console");
                Ok(())
            }
        }
    }

    pub fn remove_hooks(level: &str, runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        match level {
            "user" => {
                for runtime in runtimes {
                    Self::remove_hooks_from_settings(runtime.user_settings_path()?, runtime)?;
                }
                Ok(())
            }
            "project" => {
                for runtime in runtimes {
                    Self::remove_hooks_from_settings(runtime.project_settings_path()?, runtime)?;
                }
                Ok(())
            }
            _ => {
                println!("Invalid level. Use: user or project");
                Ok(())
            }
        }
    }

    pub fn show_hooks_status(runtime: &str) -> Result<()> {
        let runtimes = HookRuntime::parse_many(runtime)?;

        println!("🔧 Git-Warp Agent Integration Status");
        println!("====================================");
        println!("Expected events: {}", EXPECTED_EVENTS.join(", "));

        let mut repair_steps: Vec<String> = Vec::new();
        let mut any_complete = false;

        for (index, runtime) in runtimes.iter().enumerate() {
            if index > 0 {
                println!();
            }

            println!("\n{}:", runtime.display_name());

            for scope in [HookScope::User, HookScope::Project] {
                let diagnosis = Self::diagnose_scope(*runtime, scope);
                Self::print_diagnosis(&diagnosis);
                if diagnosis.is_healthy() {
                    any_complete = true;
                } else {
                    repair_steps.push(diagnosis.install_command());
                }
            }
        }

        println!("\n📖 Repair guidance:");
        if repair_steps.is_empty() {
            println!("   All checked scopes look healthy. No action needed.");
        } else {
            for step in &repair_steps {
                println!("   {step}");
            }
            if !any_complete {
                println!(
                    "   warp hooks-install --level user --runtime all  # bootstrap both runtimes at user level"
                );
            }
        }

        Ok(())
    }

    pub fn diagnose(runtime: &str) -> Result<Vec<HookDiagnosis>> {
        let runtimes = HookRuntime::parse_many(runtime)?;
        let mut out = Vec::with_capacity(runtimes.len() * 2);
        for runtime in runtimes {
            for scope in [HookScope::User, HookScope::Project] {
                out.push(Self::diagnose_scope(runtime, scope));
            }
        }
        Ok(out)
    }

    fn diagnose_scope(runtime: HookRuntime, scope: HookScope) -> HookDiagnosis {
        let path = match scope {
            HookScope::User => runtime.user_settings_path(),
            HookScope::Project => runtime.project_settings_path(),
        };

        let path = match path {
            Ok(p) => p,
            Err(error) => {
                return HookDiagnosis {
                    runtime,
                    scope,
                    path: PathBuf::new(),
                    state: HookState::NotConfigured,
                    present_events: Vec::new(),
                    missing_events: EXPECTED_EVENTS.iter().map(|s| s.to_string()).collect(),
                    conflicting_events: Vec::new(),
                    parse_error: Some(error.to_string()),
                };
            }
        };

        Self::diagnose_path(&path, runtime, scope)
    }

    pub fn diagnose_path(path: &Path, runtime: HookRuntime, scope: HookScope) -> HookDiagnosis {
        let mut diagnosis = HookDiagnosis {
            runtime,
            scope,
            path: path.to_path_buf(),
            state: HookState::NotConfigured,
            present_events: Vec::new(),
            missing_events: EXPECTED_EVENTS.iter().map(|s| s.to_string()).collect(),
            conflicting_events: Vec::new(),
            parse_error: None,
        };

        if !path.exists() {
            return diagnosis;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(error) => {
                diagnosis.parse_error = Some(format!("read error: {error}"));
                return diagnosis;
            }
        };

        let settings: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(error) => {
                diagnosis.parse_error = Some(format!("invalid JSON: {error}"));
                return diagnosis;
            }
        };

        let hooks = match Self::hooks_object(&settings, runtime) {
            Ok(h) => h,
            Err(_) => {
                return diagnosis;
            }
        };

        let mut present: Vec<String> = Vec::new();
        let mut conflicting: Vec<String> = Vec::new();
        for event in EXPECTED_EVENTS {
            let entries = hooks.get(*event).and_then(Value::as_array);
            let warp_entries = entries
                .map(|arr| arr.iter().filter(|h| Self::is_git_warp_hook(h)).count())
                .unwrap_or(0);

            if warp_entries == 0 {
                continue;
            }

            present.push((*event).to_string());
            if warp_entries > 1 {
                conflicting.push((*event).to_string());
            }
        }

        let missing: Vec<String> = EXPECTED_EVENTS
            .iter()
            .filter(|e| !present.contains(&(**e).to_string()))
            .map(|e| (*e).to_string())
            .collect();

        diagnosis.state = if present.is_empty() {
            HookState::Missing
        } else if !conflicting.is_empty() {
            HookState::Conflicting
        } else if missing.is_empty() {
            HookState::Complete
        } else {
            HookState::Partial
        };

        diagnosis.present_events = present;
        diagnosis.missing_events = missing;
        diagnosis.conflicting_events = conflicting;
        diagnosis
    }

    fn print_diagnosis(d: &HookDiagnosis) {
        let scope_label = d.scope.label();
        let path_label = if d.path.as_os_str().is_empty() {
            "<unresolved>".to_string()
        } else {
            d.path.display().to_string()
        };

        match d.state {
            HookState::Complete => {
                println!(
                    "  ✅ {scope_label} ({path}): all {n} hooks installed",
                    path = path_label,
                    n = EXPECTED_EVENTS.len()
                );
            }
            HookState::Partial => {
                println!(
                    "  ⚠️  {scope_label} ({path}): partial — {p}/{t} hooks installed; missing: {missing}",
                    path = path_label,
                    p = d.present_events.len(),
                    t = EXPECTED_EVENTS.len(),
                    missing = d.missing_events.join(", ")
                );
                println!("     Repair: {}", d.install_command());
            }
            HookState::Conflicting => {
                println!(
                    "  ⚠️  {scope_label} ({path}): conflicting duplicate git-warp entries on: {events}",
                    path = path_label,
                    events = d.conflicting_events.join(", ")
                );
                println!(
                    "     Repair: {}  # rewrites a single canonical entry per event",
                    d.install_command()
                );
            }
            HookState::Missing => {
                println!(
                    "  ❌ {scope_label} ({path}): file present but no git-warp hooks installed",
                    path = path_label
                );
                println!("     Repair: {}", d.install_command());
            }
            HookState::NotConfigured => {
                if let Some(err) = &d.parse_error {
                    println!(
                        "  ❌ {scope_label} ({path}): unreadable — {err}",
                        path = path_label
                    );
                } else {
                    println!(
                        "  ❌ {scope_label} ({path}): not configured",
                        path = path_label
                    );
                }
                println!("     Repair: {}", d.install_command());
            }
        }
    }

    fn get_hooks_config(runtime: HookRuntime) -> Value {
        let hooks = json!({
            "UserPromptSubmit": [Self::build_hook_entry(runtime, "processing", "agent_status_userpromptsubmit")],
            "Stop": [Self::build_hook_entry(runtime, "waiting", "agent_status_stop")],
            "PreToolUse": [Self::build_hook_entry(runtime, "working", "agent_status_pretooluse")],
            "PostToolUse": [Self::build_hook_entry(runtime, "processing", "agent_status_posttooluse")],
            "SubagentStop": [Self::build_hook_entry(runtime, "subagent_complete", "agent_status_subagent_stop")]
        });

        if runtime.wraps_hooks_at_root() {
            json!({ "hooks": hooks })
        } else {
            hooks
        }
    }

    fn build_hook_entry(runtime: HookRuntime, status: &str, hook_id: &str) -> Value {
        let status_root = runtime.status_root_dir();
        let command = format!(
            "ROOT=$(git rev-parse --show-toplevel 2>/dev/null || pwd) && mkdir -p \"$ROOT/{status_root}/git-warp\" && echo \"{{\\\"status\\\":\\\"{status}\\\",\\\"last_activity\\\":\\\"$(date -Iseconds)\\\"}}\" > \"$ROOT/{status_root}/git-warp/status\""
        );

        json!({
            "hooks": [{
                "type": "command",
                "command": command
            }],
            "git_warp_hook_id": hook_id
        })
    }

    fn merge_hooks_into_settings(settings_path: PathBuf, runtime: HookRuntime) -> Result<()> {
        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut settings: Value = if settings_path.exists() {
            let content = fs::read_to_string(&settings_path)?;
            serde_json::from_str(&content)?
        } else {
            json!({})
        };

        let hooks_config = Self::get_hooks_config(runtime);
        let hooks_to_merge = Self::hooks_object(&hooks_config, runtime)?;
        let settings_hooks = Self::hooks_object_mut(&mut settings, runtime);

        for (hook_type, new_entries) in hooks_to_merge {
            let entry = settings_hooks
                .entry(hook_type.clone())
                .or_insert_with(|| Value::Array(Vec::new()));

            if !entry.is_array() {
                *entry = Value::Array(Vec::new());
            }

            let entry_array = entry.as_array_mut().expect("array ensured");
            entry_array.retain(|hook| !Self::is_git_warp_hook(hook));

            if let Some(new_entries) = new_entries.as_array() {
                entry_array.extend(new_entries.iter().cloned());
            }
        }

        let content = serde_json::to_string_pretty(&settings)?;
        crate::fs_atomic::write_atomic(&settings_path, content.as_bytes())?;

        println!(
            "{} hooks installed to: {}",
            runtime.display_name(),
            settings_path.display()
        );
        Ok(())
    }

    fn remove_hooks_from_settings(settings_path: PathBuf, runtime: HookRuntime) -> Result<()> {
        if !settings_path.exists() {
            println!("Settings file not found: {}", settings_path.display());
            return Ok(());
        }

        let content = fs::read_to_string(&settings_path)?;
        let mut settings: Value = serde_json::from_str(&content)?;

        let hooks = Self::hooks_object_mut(&mut settings, runtime);
        for hook_array in hooks.values_mut() {
            if let Some(array) = hook_array.as_array_mut() {
                array.retain(|hook| !Self::is_git_warp_hook(hook));
            }
        }

        let content = serde_json::to_string_pretty(&settings)?;
        crate::fs_atomic::write_atomic(&settings_path, content.as_bytes())?;

        println!(
            "{} hooks removed from: {}",
            runtime.display_name(),
            settings_path.display()
        );
        Ok(())
    }

    fn hooks_object(settings: &Value, runtime: HookRuntime) -> Result<&Map<String, Value>> {
        let container = if runtime.wraps_hooks_at_root() {
            settings
                .get("hooks")
                .ok_or_else(|| anyhow::anyhow!("Missing hooks section"))?
        } else {
            settings
        };

        container
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Hooks config is not a JSON object"))
    }

    fn hooks_object_mut(settings: &mut Value, runtime: HookRuntime) -> &mut Map<String, Value> {
        if !settings.is_object() {
            *settings = json!({});
        }

        let root = settings.as_object_mut().expect("object ensured");
        if runtime.wraps_hooks_at_root() {
            let hooks = root.entry("hooks".to_string()).or_insert_with(|| json!({}));

            if !hooks.is_object() {
                *hooks = json!({});
            }

            hooks.as_object_mut().expect("object ensured")
        } else {
            root
        }
    }

    fn is_git_warp_hook(hook: &Value) -> bool {
        hook.get("git_warp_hook_id")
            .and_then(|id| id.as_str())
            .unwrap_or("")
            .starts_with(GIT_WARP_HOOK_PREFIX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_hooks_config_generation() {
        let config = HooksManager::get_hooks_config(HookRuntime::Claude);
        assert!(config.get("hooks").is_some());

        let hooks = &config["hooks"];
        assert!(hooks.get("UserPromptSubmit").is_some());
        assert!(
            hooks["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(".claude/git-warp/status")
        );
    }

    #[test]
    fn test_codex_hooks_config_generation() {
        let config = HooksManager::get_hooks_config(HookRuntime::Codex);
        assert!(config.get("hooks").is_none());
        assert!(config.get("PreToolUse").is_some());
        assert!(
            config["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(".codex/git-warp/status")
        );
    }

    #[test]
    fn test_codex_merge_preserves_existing_hooks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hooks_path = temp_dir.path().join("hooks.json");

        fs::write(
            &hooks_path,
            serde_json::to_string_pretty(&json!({
                "SessionStart": [{
                    "type": "command",
                    "command": "mempalace-start"
                }],
                "PreToolUse": [{
                    "type": "command",
                    "command": "custom-pre-tool"
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        HooksManager::merge_hooks_into_settings(hooks_path.clone(), HookRuntime::Codex).unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&hooks_path).unwrap()).unwrap();
        assert_eq!(settings["SessionStart"].as_array().unwrap().len(), 1);
        assert_eq!(settings["PreToolUse"].as_array().unwrap().len(), 2);
        assert!(
            settings["PreToolUse"]
                .as_array()
                .unwrap()
                .iter()
                .any(HooksManager::is_git_warp_hook)
        );
    }

    #[test]
    fn test_diagnose_missing_when_file_absent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("settings.json");

        let d = HooksManager::diagnose_path(&path, HookRuntime::Claude, HookScope::User);

        assert!(matches!(d.state, HookState::NotConfigured));
        assert_eq!(d.present_events.len(), 0);
        assert_eq!(d.missing_events.len(), EXPECTED_EVENTS.len());
        assert_eq!(
            d.install_command(),
            "warp hooks-install --level user --runtime claude"
        );
    }

    #[test]
    fn test_diagnose_partial_when_some_events_present() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        // Pre-existing settings with only Stop wired up.
        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "hooks": {
                    "Stop": [{
                        "git_warp_hook_id": "agent_status_stop",
                        "hooks": [{ "type": "command", "command": "x" }]
                    }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let d =
            HooksManager::diagnose_path(&settings_path, HookRuntime::Claude, HookScope::Project);

        assert!(matches!(d.state, HookState::Partial));
        assert_eq!(d.present_events, vec!["Stop"]);
        assert!(d.missing_events.contains(&"PreToolUse".to_string()));
        assert!(d.missing_events.contains(&"UserPromptSubmit".to_string()));
        assert_eq!(
            d.install_command(),
            "warp hooks-install --level project --runtime claude"
        );
    }

    #[test]
    fn test_diagnose_complete_after_install_claude() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        HooksManager::merge_hooks_into_settings(settings_path.clone(), HookRuntime::Claude)
            .unwrap();

        let d = HooksManager::diagnose_path(&settings_path, HookRuntime::Claude, HookScope::User);

        assert!(
            matches!(d.state, HookState::Complete),
            "expected Complete, got {:?}",
            d.state
        );
        assert_eq!(d.present_events.len(), EXPECTED_EVENTS.len());
        assert!(d.missing_events.is_empty());
        assert!(d.is_healthy());
    }

    #[test]
    fn test_diagnose_complete_after_install_codex() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hooks_path = temp_dir.path().join("hooks.json");

        HooksManager::merge_hooks_into_settings(hooks_path.clone(), HookRuntime::Codex).unwrap();

        let d = HooksManager::diagnose_path(&hooks_path, HookRuntime::Codex, HookScope::Project);

        assert!(matches!(d.state, HookState::Complete));
        assert!(d.missing_events.is_empty());
    }

    #[test]
    fn test_diagnose_missing_when_file_has_no_warp_hooks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "hooks": {
                    "Stop": [{ "type": "command", "command": "unrelated" }]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let d = HooksManager::diagnose_path(&settings_path, HookRuntime::Claude, HookScope::User);

        assert!(matches!(d.state, HookState::Missing));
        assert!(d.present_events.is_empty());
        assert_eq!(d.missing_events.len(), EXPECTED_EVENTS.len());
    }

    #[test]
    fn test_diagnose_conflicting_when_duplicate_warp_entries() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "hooks": {
                    "Stop": [
                        {
                            "git_warp_hook_id": "agent_status_stop",
                            "hooks": [{ "type": "command", "command": "a" }]
                        },
                        {
                            "git_warp_hook_id": "agent_status_stop_old",
                            "hooks": [{ "type": "command", "command": "b" }]
                        }
                    ]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let d = HooksManager::diagnose_path(&settings_path, HookRuntime::Claude, HookScope::User);

        assert!(matches!(d.state, HookState::Conflicting));
        assert_eq!(d.conflicting_events, vec!["Stop"]);
    }

    #[test]
    fn test_diagnose_invalid_json_reports_parse_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        fs::write(&settings_path, "{not json").unwrap();

        let d = HooksManager::diagnose_path(&settings_path, HookRuntime::Codex, HookScope::Project);

        assert!(matches!(d.state, HookState::NotConfigured));
        assert!(d.parse_error.is_some());
    }

    #[test]
    fn test_install_command_uses_scope_and_runtime_args() {
        let d = HookDiagnosis {
            runtime: HookRuntime::Codex,
            scope: HookScope::User,
            path: PathBuf::from("/tmp/x"),
            state: HookState::Missing,
            present_events: Vec::new(),
            missing_events: Vec::new(),
            conflicting_events: Vec::new(),
            parse_error: None,
        };
        assert_eq!(
            d.install_command(),
            "warp hooks-install --level user --runtime codex"
        );
    }

    #[test]
    fn test_claude_remove_preserves_non_git_warp_hooks() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");

        fs::write(
            &settings_path,
            serde_json::to_string_pretty(&json!({
                "hooks": {
                    "Stop": [
                        {
                            "git_warp_hook_id": "agent_status_stop",
                            "hooks": [{
                                "type": "command",
                                "command": "git-warp-stop"
                            }]
                        },
                        {
                            "type": "command",
                            "command": "custom-stop"
                        }
                    ]
                }
            }))
            .unwrap(),
        )
        .unwrap();

        HooksManager::remove_hooks_from_settings(settings_path.clone(), HookRuntime::Claude)
            .unwrap();

        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(
            settings["hooks"]["Stop"][0]["command"].as_str().unwrap(),
            "custom-stop"
        );
    }
}
