use super::spec::{ConfigFieldId, ConfigFieldKind, ConfigFieldSpec};
use crate::config::Config;
use std::path::PathBuf;

pub fn detect_env_overrides() -> Vec<String> {
    std::env::vars()
        .filter_map(|(k, _)| {
            if k.starts_with("GIT_WARP_") {
                Some(k)
            } else {
                None
            }
        })
        .collect()
}

pub fn render_field_value(config: &Config, field: &ConfigFieldSpec) -> String {
    use ConfigFieldId::*;
    match field.id {
        TerminalMode => config.terminal_mode.clone(),
        WorktreesPath => config
            .worktrees_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into()),
        UseCow => config.use_cow.to_string(),
        AutoConfirm => config.auto_confirm.to_string(),
        GitDefaultBranch => config.git.default_branch.clone(),
        GitProtectedBranches => render_list(&config.git.protected_branches),
        GitAutoFetch => config.git.auto_fetch.to_string(),
        GitAutoPrune => config.git.auto_prune.to_string(),
        ProcessCheckProcesses => config.process.check_processes.to_string(),
        ProcessAutoKill => config.process.auto_kill.to_string(),
        ProcessKillTimeout => config.process.kill_timeout.to_string(),
        TerminalApp => config.terminal.app.clone(),
        TerminalAutoActivate => config.terminal.auto_activate.to_string(),
        TerminalInitCommands => render_list(&config.terminal.init_commands),
        AgentEnabled => config.agent.enabled.to_string(),
        AgentRefreshRate => config.agent.refresh_rate.to_string(),
        AgentMaxActivities => config.agent.max_activities.to_string(),
        PostCreateAutoInstall => config.post_create.auto_install.to_string(),
    }
}

fn render_list(items: &[String]) -> String {
    if items.is_empty() {
        "(empty)".into()
    } else {
        format!("[{}]", items.join(", "))
    }
}

pub fn apply_field_value(
    config: &mut Config,
    field: &ConfigFieldSpec,
    raw: &str,
) -> std::result::Result<(), String> {
    use ConfigFieldId::*;
    let trimmed = raw.trim();
    match &field.kind {
        ConfigFieldKind::Bool | ConfigFieldKind::ReadOnlyList => {
            Err("Field not editable from the TUI.".into())
        }
        ConfigFieldKind::Choice { allowed } => {
            if !allowed.contains(&trimmed) {
                return Err(format!("Allowed values: {}", allowed.join(", ")));
            }
            match field.id {
                TerminalMode => config.terminal_mode = trimmed.to_string(),
                TerminalApp => config.terminal.app = trimmed.to_string(),
                _ => unreachable!("Choice kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::FreeText => {
            if trimmed.is_empty() {
                return Err("Value must not be empty.".into());
            }
            match field.id {
                GitDefaultBranch => config.git.default_branch = trimmed.to_string(),
                _ => unreachable!("FreeText kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::OptionPath => {
            match field.id {
                WorktreesPath => {
                    config.worktrees_path = if trimmed.is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(trimmed))
                    };
                }
                _ => unreachable!("OptionPath kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::U64 { min, max } => {
            let parsed: u64 = trimmed
                .parse()
                .map_err(|_| format!("Enter an integer in [{min}, {max}]."))?;
            if parsed < *min || parsed > *max {
                return Err(format!("Value must be in [{min}, {max}]."));
            }
            match field.id {
                ProcessKillTimeout => config.process.kill_timeout = parsed,
                AgentRefreshRate => config.agent.refresh_rate = parsed,
                _ => unreachable!("U64 kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
        ConfigFieldKind::Usize { min, max } => {
            let parsed: usize = trimmed
                .parse()
                .map_err(|_| format!("Enter an integer in [{min}, {max}]."))?;
            if parsed < *min || parsed > *max {
                return Err(format!("Value must be in [{min}, {max}]."));
            }
            match field.id {
                AgentMaxActivities => config.agent.max_activities = parsed,
                _ => unreachable!("Usize kind on unknown field {:?}", field.id),
            }
            Ok(())
        }
    }
}
