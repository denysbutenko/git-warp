use crate::error::Result;
use serde_json::{Map, Value, json};
use std::fs;
use std::path::PathBuf;

const GIT_WARP_HOOK_PREFIX: &str = "agent_status_";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HookRuntime {
    Claude,
    Codex,
}

impl HookRuntime {
    fn parse_many(runtime: &str) -> Result<Vec<Self>> {
        match runtime {
            "claude" => Ok(vec![Self::Claude]),
            "codex" => Ok(vec![Self::Codex]),
            "all" => Ok(vec![Self::Claude, Self::Codex]),
            _ => Err(anyhow::anyhow!("Invalid runtime. Use: claude, codex, or all").into()),
        }
    }

    fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
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

        for (index, runtime) in runtimes.iter().enumerate() {
            if index > 0 {
                println!();
            }

            println!("{}:", runtime.display_name());

            match runtime.user_settings_path() {
                Ok(path) => {
                    if path.exists() {
                        println!("✅ User config: {}", path.display());
                        Self::show_hooks_for_path(&path, *runtime)?;
                    } else {
                        println!("❌ User config: Not found");
                    }
                }
                Err(_) => println!("❌ User config: Unable to locate"),
            }

            match runtime.project_settings_path() {
                Ok(path) => {
                    if path.exists() {
                        println!("✅ Project config: {}", path.display());
                        Self::show_hooks_for_path(&path, *runtime)?;
                    } else {
                        println!("❌ Project config: Not found");
                    }
                }
                Err(_) => println!("❌ Project config: Unable to locate"),
            }
        }

        println!("\n📖 Integration Guide:");
        println!("   warp hooks-install --level user --runtime codex");
        println!("   warp hooks-install --level project --runtime claude");
        println!("   warp hooks-install --level user --runtime all");

        Ok(())
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
        fs::write(&settings_path, content)?;

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
        fs::write(&settings_path, content)?;

        println!(
            "{} hooks removed from: {}",
            runtime.display_name(),
            settings_path.display()
        );
        Ok(())
    }

    fn show_hooks_for_path(path: &PathBuf, runtime: HookRuntime) -> Result<()> {
        if !path.exists() {
            println!("  No settings file found");
            return Ok(());
        }

        let content = fs::read_to_string(path)?;
        let settings: Value = serde_json::from_str(&content)?;
        let hooks = match Self::hooks_object(&settings, runtime) {
            Ok(hooks) => hooks,
            Err(_) => {
                println!("  No git-warp hooks installed");
                return Ok(());
            }
        };

        let mut found_hooks = false;
        for (hook_type, hook_array) in hooks {
            if let Some(array) = hook_array.as_array() {
                let git_warp_hooks: Vec<_> = array
                    .iter()
                    .filter(|hook| Self::is_git_warp_hook(hook))
                    .collect();

                if !git_warp_hooks.is_empty() {
                    if !found_hooks {
                        println!("  ✓ Hooks installed:");
                        found_hooks = true;
                    }
                    println!(
                        "    {}: {} git-warp hook(s)",
                        hook_type,
                        git_warp_hooks.len()
                    );
                }
            }
        }

        if !found_hooks {
            println!("  No git-warp hooks installed");
        }

        Ok(())
    }

    fn hooks_object<'a>(
        settings: &'a Value,
        runtime: HookRuntime,
    ) -> Result<&'a Map<String, Value>> {
        let container = if runtime.wraps_hooks_at_root() {
            settings
                .get("hooks")
                .ok_or_else(|| anyhow::anyhow!("Missing hooks section"))?
        } else {
            settings
        };

        container
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("Hooks config is not a JSON object").into())
    }

    fn hooks_object_mut<'a>(
        settings: &'a mut Value,
        runtime: HookRuntime,
    ) -> &'a mut Map<String, Value> {
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
                .any(|entry| HooksManager::is_git_warp_hook(entry))
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
