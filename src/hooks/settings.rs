use super::HooksManager;
use super::types::{GIT_WARP_HOOK_PREFIX, HookRuntime};
use crate::error::{GitWarpError, Result};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

impl HooksManager {
    pub(super) fn get_hooks_config(runtime: HookRuntime) -> Value {
        let hooks = json!({
            "SessionStart": [Self::build_hook_entry(runtime, "starting", "agent_status_sessionstart")],
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
        // Single executable invocation parses identically under cmd.exe,
        // pwsh, bash, and dash — no shell-specific quoting, no GNU/BSD
        // `date` divergence (#189).
        let command = format!(
            "warp __hook-status --runtime {} --status {}",
            runtime.install_arg(),
            status,
        );

        json!({
            "hooks": [{
                "type": "command",
                "command": command
            }],
            "git_warp_hook_id": hook_id
        })
    }

    pub(super) fn merge_hooks_into_settings(
        settings_path: PathBuf,
        runtime: HookRuntime,
    ) -> Result<()> {
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
        let settings_hooks = Self::hooks_object_mut(&mut settings, &settings_path, runtime)?;

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

    pub(super) fn remove_hooks_from_settings(
        settings_path: PathBuf,
        runtime: HookRuntime,
    ) -> Result<()> {
        if !settings_path.exists() {
            println!("Settings file not found: {}", settings_path.display());
            return Ok(());
        }

        let content = fs::read_to_string(&settings_path)?;
        let mut settings: Value = serde_json::from_str(&content)?;

        let hooks = Self::hooks_object_mut(&mut settings, &settings_path, runtime)?;
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

    pub(super) fn hooks_object(
        settings: &Value,
        runtime: HookRuntime,
    ) -> Result<&Map<String, Value>> {
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

    fn hooks_object_mut<'a>(
        settings: &'a mut Value,
        settings_path: &Path,
        runtime: HookRuntime,
    ) -> Result<&'a mut Map<String, Value>> {
        if !settings.is_object() {
            return Err(GitWarpError::ConfigError {
                message: format!(
                    "Refusing to overwrite non-object JSON root in {} (found {}). \
                     Expected a JSON object at the top level; fix the file manually and re-run.",
                    settings_path.display(),
                    value_kind(settings)
                ),
            }
            .into());
        }

        let root = settings.as_object_mut().expect("object ensured");
        if runtime.wraps_hooks_at_root() {
            let hooks = root.entry("hooks".to_string()).or_insert_with(|| json!({}));

            if !hooks.is_object() {
                return Err(GitWarpError::ConfigError {
                    message: format!(
                        "Refusing to overwrite non-object /hooks in {} (found {}). \
                         Expected a JSON object at /hooks; fix the file manually and re-run.",
                        settings_path.display(),
                        value_kind(hooks)
                    ),
                }
                .into());
            }

            Ok(hooks.as_object_mut().expect("object ensured"))
        } else {
            Ok(root)
        }
    }

    pub(super) fn is_git_warp_hook(hook: &Value) -> bool {
        hook.get("git_warp_hook_id")
            .and_then(|id| id.as_str())
            .unwrap_or("")
            .starts_with(GIT_WARP_HOOK_PREFIX)
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::super::HooksManager;
    use super::super::types::HookRuntime;
    use serde_json::{Value, json};
    use std::fs;

    #[test]
    fn test_claude_hooks_config_generation() {
        let config = HooksManager::get_hooks_config(HookRuntime::Claude);
        assert!(config.get("hooks").is_some());

        let hooks = &config["hooks"];
        assert!(hooks.get("SessionStart").is_some());
        assert!(hooks.get("UserPromptSubmit").is_some());
        assert_eq!(
            hooks["Stop"][0]["hooks"][0]["command"].as_str().unwrap(),
            "warp __hook-status --runtime claude --status waiting"
        );
    }

    #[test]
    fn test_codex_hooks_config_generation() {
        let config = HooksManager::get_hooks_config(HookRuntime::Codex);
        assert!(config.get("hooks").is_none());
        assert!(config.get("SessionStart").is_some());
        assert!(config.get("PreToolUse").is_some());
        assert_eq!(
            config["Stop"][0]["hooks"][0]["command"].as_str().unwrap(),
            "warp __hook-status --runtime codex --status waiting"
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
        assert_eq!(settings["SessionStart"].as_array().unwrap().len(), 2);
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
    fn test_claude_merge_errors_on_non_object_root_array() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        let original = "[\n  \"do-not-touch\"\n]\n";
        fs::write(&settings_path, original).unwrap();

        let err =
            HooksManager::merge_hooks_into_settings(settings_path.clone(), HookRuntime::Claude)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("non-object JSON root"), "message: {msg}");
        assert!(msg.contains("array"), "message: {msg}");
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
    }

    #[test]
    fn test_codex_merge_errors_on_non_object_root_string() {
        let temp_dir = tempfile::tempdir().unwrap();
        let hooks_path = temp_dir.path().join("hooks.json");
        let original = "\"disabled\"";
        fs::write(&hooks_path, original).unwrap();

        let err = HooksManager::merge_hooks_into_settings(hooks_path.clone(), HookRuntime::Codex)
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("non-object JSON root"), "message: {msg}");
        assert!(msg.contains("string"), "message: {msg}");
        assert_eq!(fs::read_to_string(&hooks_path).unwrap(), original);
    }

    #[test]
    fn test_claude_merge_errors_on_non_object_hooks_field() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        let original = serde_json::to_string_pretty(&json!({
            "hooks": [],
            "keepMe": true
        }))
        .unwrap();
        fs::write(&settings_path, &original).unwrap();

        let err =
            HooksManager::merge_hooks_into_settings(settings_path.clone(), HookRuntime::Claude)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("non-object /hooks"), "message: {msg}");
        assert!(msg.contains("array"), "message: {msg}");
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
    }

    #[test]
    fn test_claude_remove_errors_on_non_object_root() {
        let temp_dir = tempfile::tempdir().unwrap();
        let settings_path = temp_dir.path().join("settings.json");
        let original = "null";
        fs::write(&settings_path, original).unwrap();

        let err =
            HooksManager::remove_hooks_from_settings(settings_path.clone(), HookRuntime::Claude)
                .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("non-object JSON root"), "message: {msg}");
        assert!(msg.contains("null"), "message: {msg}");
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
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
