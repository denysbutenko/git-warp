use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands;
use crate::commands::cleanup::CleanupMode;

#[derive(Parser)]
#[command(
    name = "warp",
    about = "High-performance Git worktree manager with Copy-on-Write speed",
    long_about = "Git-Warp combines instantaneous Copy-on-Write worktree creation with rich terminal integration and advanced features.",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Branch name (used when no subcommand is provided)
    pub branch: Option<String>,

    /// Enable debug logging
    #[arg(long, short, global = true)]
    pub debug: bool,

    /// Show what would be done without executing
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Terminal mode: tab, window, current, inplace, echo
    #[arg(long, global = true, value_enum)]
    pub terminal: Option<crate::terminal::TerminalMode>,

    /// Auto-confirm operations
    #[arg(long, short = 'y', global = true)]
    pub auto_confirm: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create or switch to a worktree
    Switch {
        /// Branch name
        branch: Option<String>,
        /// Custom worktree path
        #[arg(long)]
        path: Option<String>,
        /// Switch to the most recent agent branch
        #[arg(long)]
        latest: bool,
        /// Switch to the most recent waiting agent branch
        #[arg(long)]
        waiting: bool,
        /// Force traditional worktree (skip CoW)
        #[arg(long)]
        no_cow: bool,
    },

    /// List all worktrees
    #[command(alias = "list")]
    Ls {
        /// Show debug information
        #[arg(long)]
        debug: bool,
        /// Interactive mode
        #[arg(long, short)]
        interactive: bool,
    },

    /// Clean up worktrees
    Cleanup {
        /// Cleanup mode: all, merged, remoteless, interactive
        #[arg(long, value_enum, default_value_t = CleanupMode::Merged)]
        mode: CleanupMode,
        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,
        /// Kill processes in worktrees being removed
        #[arg(long)]
        kill: bool,
        /// Don't kill processes (override config)
        #[arg(long, conflicts_with = "kill")]
        no_kill: bool,
        /// Interactive mode
        #[arg(long, short)]
        interactive: bool,
    },

    /// Configure git-warp settings
    Config {
        /// Show current configuration
        #[arg(long, conflicts_with_all = ["edit", "interactive"])]
        show: bool,
        /// Open the configuration file in your editor
        #[arg(long, conflicts_with = "interactive")]
        edit: bool,
        /// Launch the interactive config editor TUI
        #[arg(long, short, conflicts_with_all = ["show", "edit"])]
        interactive: bool,
    },

    /// Live agent monitoring dashboard
    Agents,

    /// Check Git-Warp setup and print next steps
    Doctor,

    /// Validate release metadata and smoke checks
    ReleaseCheck {
        /// Expected release version, for example v0.3.0
        #[arg(long)]
        version: Option<String>,
        /// Only validate version, changelog, release notes, install docs, and install script
        #[arg(long)]
        metadata_only: bool,
    },

    /// Install agent hooks
    HooksInstall {
        /// Installation level: user, project, console
        #[arg(long, value_enum)]
        level: Option<crate::hooks::HookInstallLevel>,
        /// Runtime: claude, codex, all
        #[arg(long, default_value = "claude")]
        runtime: String,
    },

    /// Remove agent hooks
    HooksRemove {
        /// Installation level: user, project
        #[arg(long, value_enum)]
        level: Option<crate::hooks::HookRemoveLevel>,
        /// Runtime: claude, codex, all
        #[arg(long, default_value = "claude")]
        runtime: String,
    },

    /// Show installed hooks status
    HooksStatus {
        /// Runtime: claude, codex, all
        #[arg(long, default_value = "claude")]
        runtime: String,
    },

    /// Generate shell configuration
    ShellConfig {
        /// Shell type: bash, zsh, fish, powershell
        shell: Option<String>,
    },

    /// Internal shell completion helper
    #[command(name = "__complete", hide = true)]
    Complete {
        /// Completion target
        target: String,
        /// Current token prefix
        prefix: Option<String>,
    },

    /// Internal hook callback used by installed Claude/Codex hooks (#189)
    #[command(name = "__hook-status", hide = true)]
    HookStatus {
        /// Runtime: claude or codex
        #[arg(long)]
        runtime: String,
        /// Status value to record in the live status file
        #[arg(long)]
        status: String,
    },
}

impl Cli {
    pub fn run(&self) -> Result<()> {
        match &self.command {
            Some(command) => self.dispatch(command),
            None => {
                if let Some(branch) = &self.branch {
                    // Dynamic branch command - same as switch
                    commands::switch::run(self, Some(branch), None, false, false, false)
                } else {
                    commands::switch::run_default(self)
                }
            }
        }
    }

    fn dispatch(&self, command: &Commands) -> Result<()> {
        match command {
            Commands::Switch {
                branch,
                path,
                latest,
                waiting,
                no_cow,
            } => commands::switch::run(
                self,
                branch.as_deref(),
                path.as_deref(),
                *latest,
                *waiting,
                *no_cow,
            ),
            Commands::Ls { debug, interactive } => commands::ls::run(self, *debug, *interactive),
            Commands::Cleanup {
                mode,
                force,
                kill,
                no_kill,
                interactive,
            } => commands::cleanup::run(self, *mode, *force, *kill, *no_kill, *interactive),
            Commands::Config {
                show,
                edit,
                interactive,
            } => commands::config::run(self, *show, *edit, *interactive),
            Commands::Agents => commands::agents::run(self),
            Commands::Doctor => commands::doctor::run(self),
            Commands::ReleaseCheck {
                version,
                metadata_only,
            } => crate::release::run_release_check(crate::release::ReleaseCheckOptions {
                version: version.clone(),
                metadata_only: *metadata_only,
            }),
            Commands::HooksInstall { level, runtime } => {
                commands::hooks::install(self, *level, runtime)
            }
            Commands::HooksRemove { level, runtime } => {
                commands::hooks::remove(self, *level, runtime)
            }
            Commands::HooksStatus { runtime } => commands::hooks::status(self, runtime),
            Commands::ShellConfig { shell } => commands::shell_config::run(self, shell.as_deref()),
            Commands::Complete { target, prefix } => {
                commands::complete::run(self, target, prefix.as_deref())
            }
            Commands::HookStatus { runtime, status } => {
                commands::hooks::hook_status(self, runtime, status)
            }
        }
    }
}
