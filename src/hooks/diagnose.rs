use super::HooksManager;
use super::types::{EXPECTED_EVENTS, HookDiagnosis, HookRuntime, HookScope, HookState};
use crate::error::Result;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

impl HooksManager {
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

    pub(super) fn diagnose_scope(runtime: HookRuntime, scope: HookScope) -> HookDiagnosis {
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

    pub(super) fn print_diagnosis(d: &HookDiagnosis) {
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
}

#[cfg(test)]
mod tests {
    use super::super::HooksManager;
    use super::super::types::{EXPECTED_EVENTS, HookDiagnosis, HookRuntime, HookScope, HookState};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

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
}
