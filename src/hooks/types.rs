use crate::error::Result;
use clap::ValueEnum;
use std::fmt;
use std::path::PathBuf;

pub(super) const GIT_WARP_HOOK_PREFIX: &str = "agent_status_";

pub(super) const EXPECTED_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "Stop",
    "PreToolUse",
    "PostToolUse",
    "SubagentStop",
];

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum HookInstallLevel {
    User,
    Project,
    Console,
}

impl HookInstallLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            HookInstallLevel::User => "user",
            HookInstallLevel::Project => "project",
            HookInstallLevel::Console => "console",
        }
    }
}

impl fmt::Display for HookInstallLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum HookRemoveLevel {
    User,
    Project,
}

impl HookRemoveLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            HookRemoveLevel::User => "user",
            HookRemoveLevel::Project => "project",
        }
    }
}

impl fmt::Display for HookRemoveLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookRuntime {
    Claude,
    Codex,
}

impl HookRuntime {
    pub(super) fn parse_many(runtime: &str) -> Result<Vec<Self>> {
        match runtime {
            "claude" => Ok(vec![Self::Claude]),
            "codex" => Ok(vec![Self::Codex]),
            "all" => Ok(vec![Self::Claude, Self::Codex]),
            _ => Err(anyhow::anyhow!(
                "Invalid runtime. Use: claude, codex, or all"
            )),
        }
    }

    pub(super) fn parse_single(runtime: &str) -> Result<Self> {
        match runtime {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err(anyhow::anyhow!("Invalid runtime. Use: claude or codex")),
        }
    }

    pub(super) fn display_name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    pub(super) fn install_arg(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub(super) fn status_root_dir(&self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Codex => ".codex",
        }
    }

    pub(super) fn user_settings_path(&self) -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;

        Ok(match self {
            Self::Claude => home.join(".claude").join("settings.json"),
            Self::Codex => home.join(".codex").join("hooks.json"),
        })
    }

    pub(super) fn project_settings_path(&self) -> Result<PathBuf> {
        let current_dir = std::env::current_dir()?;

        Ok(match self {
            Self::Claude => current_dir.join(".claude").join("settings.json"),
            Self::Codex => current_dir.join(".codex").join("hooks.json"),
        })
    }

    pub(super) fn wraps_hooks_at_root(&self) -> bool {
        matches!(self, Self::Claude)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookScope {
    User,
    Project,
}

impl HookScope {
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Project => "Project",
        }
    }

    pub(super) fn install_arg(&self) -> &'static str {
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
