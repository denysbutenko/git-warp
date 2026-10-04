#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSectionId {
    General,
    Git,
    Process,
    Terminal,
    Agent,
    PostCreate,
}

impl ConfigSectionId {
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Git => "Git",
            Self::Process => "Process",
            Self::Terminal => "Terminal",
            Self::Agent => "Agent",
            Self::PostCreate => "Post-create",
        }
    }

    pub fn all() -> &'static [ConfigSectionId] {
        &[
            Self::General,
            Self::Git,
            Self::Process,
            Self::Terminal,
            Self::Agent,
            Self::PostCreate,
        ]
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ConfigFieldId {
    TerminalMode,
    WorktreesPath,
    UseCow,
    AutoConfirm,
    GitDefaultBranch,
    GitProtectedBranches,
    GitAutoFetch,
    GitAutoPrune,
    ProcessCheckProcesses,
    ProcessAutoKill,
    ProcessKillTimeout,
    TerminalApp,
    TerminalAutoActivate,
    TerminalInitCommands,
    AgentEnabled,
    AgentRefreshRate,
    AgentMaxActivities,
    PostCreateAutoInstall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigFieldKind {
    Bool,
    Choice { allowed: &'static [&'static str] },
    FreeText,
    OptionPath,
    U64 { min: u64, max: u64 },
    Usize { min: usize, max: usize },
    ReadOnlyList,
}

#[derive(Debug, Clone)]
pub struct ConfigFieldSpec {
    pub id: ConfigFieldId,
    pub section: ConfigSectionId,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: ConfigFieldKind,
}

pub fn config_field_specs() -> Vec<ConfigFieldSpec> {
    use ConfigFieldId::*;
    use ConfigFieldKind::*;
    use ConfigSectionId::*;
    vec![
        ConfigFieldSpec {
            id: TerminalMode,
            section: General,
            label: "terminal_mode",
            help: "Default terminal launch mode for worktree switches.",
            kind: Choice {
                allowed: &["tab", "window", "current", "inplace", "echo"],
            },
        },
        ConfigFieldSpec {
            id: WorktreesPath,
            section: General,
            label: "worktrees_path",
            help: "Optional override for the worktree base directory. Empty value clears it.",
            kind: OptionPath,
        },
        ConfigFieldSpec {
            id: UseCow,
            section: General,
            label: "use_cow",
            help: "Use copy-on-write when the filesystem supports it.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: AutoConfirm,
            section: General,
            label: "auto_confirm",
            help: "Skip confirmation prompts on destructive operations.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: GitDefaultBranch,
            section: Git,
            label: "default_branch",
            help: "Default base branch name.",
            kind: FreeText,
        },
        ConfigFieldSpec {
            id: GitProtectedBranches,
            section: Git,
            label: "protected_branches",
            help: "Branches cleanup must never remove. Edit the config file to change.",
            kind: ReadOnlyList,
        },
        ConfigFieldSpec {
            id: GitAutoFetch,
            section: Git,
            label: "auto_fetch",
            help: "Run `git fetch` before operations that need fresh refs.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: GitAutoPrune,
            section: Git,
            label: "auto_prune",
            help: "Prune stale remote-tracking branches during fetch.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessCheckProcesses,
            section: Process,
            label: "check_processes",
            help: "Scan for processes rooted in a worktree before cleanup.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessAutoKill,
            section: Process,
            label: "auto_kill",
            help: "Send termination signals to worktree-rooted processes without prompting.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: ProcessKillTimeout,
            section: Process,
            label: "kill_timeout",
            help: "Grace period (seconds) before escalating SIGTERM to SIGKILL.",
            kind: U64 { min: 1, max: 300 },
        },
        ConfigFieldSpec {
            id: TerminalApp,
            section: Terminal,
            label: "app",
            help: "Preferred terminal application for worktree switches.",
            kind: Choice {
                allowed: &["auto", "iterm2", "terminal", "warp"],
            },
        },
        ConfigFieldSpec {
            id: TerminalAutoActivate,
            section: Terminal,
            label: "auto_activate",
            help: "Activate the newly created tab or window.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: TerminalInitCommands,
            section: Terminal,
            label: "init_commands",
            help: "Commands run when entering a worktree. Edit the config file to change.",
            kind: ReadOnlyList,
        },
        ConfigFieldSpec {
            id: AgentEnabled,
            section: Agent,
            label: "enabled",
            help: "Enable the agents dashboard.",
            kind: Bool,
        },
        ConfigFieldSpec {
            id: AgentRefreshRate,
            section: Agent,
            label: "refresh_rate",
            help: "Agents dashboard refresh interval (milliseconds).",
            kind: U64 {
                min: 250,
                max: 60_000,
            },
        },
        ConfigFieldSpec {
            id: AgentMaxActivities,
            section: Agent,
            label: "max_activities",
            help: "Maximum number of recent agent activities to track.",
            kind: Usize {
                min: 1,
                max: 10_000,
            },
        },
        ConfigFieldSpec {
            id: PostCreateAutoInstall,
            section: PostCreate,
            label: "auto_install",
            help: "Run the matching package-manager install when a JS lockfile is present.",
            kind: Bool,
        },
    ]
}
