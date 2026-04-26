use anyhow::Result;
use clap::{Parser, Subcommand};
use log::info;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

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
    #[arg(long, global = true)]
    pub terminal: Option<String>,

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
    },

    /// Clean up worktrees
    Cleanup {
        /// Cleanup mode: all, merged, remoteless, interactive
        #[arg(long, default_value = "merged")]
        mode: String,
        /// Force removal even with uncommitted changes
        #[arg(long)]
        force: bool,
        /// Kill processes in worktrees being removed
        #[arg(long)]
        kill: bool,
        /// Don't kill processes (override config)
        #[arg(long)]
        no_kill: bool,
        /// Interactive mode
        #[arg(long, short)]
        interactive: bool,
    },

    /// Configure git-warp settings
    Config {
        /// Show current configuration
        #[arg(long)]
        show: bool,
        /// Open the configuration file in your editor
        #[arg(long)]
        edit: bool,
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
        #[arg(long)]
        level: Option<String>,
        /// Runtime: claude, codex, all
        #[arg(long, default_value = "claude")]
        runtime: String,
    },

    /// Remove agent hooks
    HooksRemove {
        /// Installation level: user, project
        #[arg(long)]
        level: Option<String>,
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
        /// Shell type: bash, zsh, fish
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchStepStatus {
    Done,
    Skipped,
    Warning,
}

struct SwitchStep {
    label: &'static str,
    status: SwitchStepStatus,
    detail: String,
}

impl SwitchStep {
    fn print(&self) {
        let icon = match self.status {
            SwitchStepStatus::Done => "✅",
            SwitchStepStatus::Skipped => "↪️ ",
            SwitchStepStatus::Warning => "⚠️ ",
        };

        println!("{} {}: {}", icon, self.label, self.detail);
    }
}

struct SwitchOutcomeReport {
    worktree_path: PathBuf,
    steps: Vec<SwitchStep>,
}

impl SwitchOutcomeReport {
    fn new(worktree_path: PathBuf) -> Self {
        Self {
            worktree_path,
            steps: Vec::new(),
        }
    }

    fn done(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Done, detail);
    }

    fn skipped(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Skipped, detail);
    }

    fn warned(&mut self, label: &'static str, detail: impl Into<String>) {
        self.push(label, SwitchStepStatus::Warning, detail);
    }

    fn push(&mut self, label: &'static str, status: SwitchStepStatus, detail: impl Into<String>) {
        let step = SwitchStep {
            label,
            status,
            detail: detail.into(),
        };
        step.print();
        self.steps.push(step);
    }

    fn has_warnings(&self) -> bool {
        self.steps
            .iter()
            .any(|step| step.status == SwitchStepStatus::Warning)
    }

    fn finish(&self) {
        if self.has_warnings() {
            println!("⚠️  Switch incomplete: {}", self.worktree_path.display());
            println!("💡 Run: cd '{}'", self.worktree_path.display());
        } else {
            println!("✅ Switch complete: {}", self.worktree_path.display());
        }
    }
}

impl Cli {
    fn not_in_git_repo_error() -> anyhow::Error {
        anyhow::anyhow!(
            "Not in a Git repository. Run this command inside a Git repository, or use `cd <repo>` first."
        )
    }

    fn create_worktree_with_recovery(
        git_repo: &crate::git::GitRepository,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<()> {
        git_repo
            .create_worktree_and_branch(branch, worktree_path, None)
            .map_err(|error| {
                anyhow::anyhow!(
                    "{error}. Use a different branch name or run `warp ls` to inspect existing worktrees."
                )
            })
    }

    pub fn run(&self) -> Result<()> {
        if self.debug {
            unsafe {
                std::env::set_var("RUST_LOG", "debug");
            }
        }

        match &self.command {
            Some(command) => self.handle_command(command),
            None => {
                if let Some(branch) = &self.branch {
                    // Dynamic branch command - same as switch
                    self.handle_switch(Some(branch), None, false, false, false)
                } else {
                    self.handle_default_switcher()
                }
            }
        }
    }

    fn handle_command(&self, command: &Commands) -> Result<()> {
        match command {
            Commands::Switch {
                branch,
                path,
                latest,
                waiting,
                no_cow,
            } => self.handle_switch(
                branch.as_deref(),
                path.as_deref(),
                *latest,
                *waiting,
                *no_cow,
            ),
            Commands::Ls { debug } => self.handle_ls(*debug),
            Commands::Cleanup {
                mode,
                force,
                kill,
                no_kill,
                interactive,
            } => self.handle_cleanup(mode, *force, *kill, *no_kill, *interactive),
            Commands::Config { show, edit } => self.handle_config(*show, *edit),
            Commands::Agents => self.handle_agents(),
            Commands::Doctor => self.handle_doctor(),
            Commands::ReleaseCheck {
                version,
                metadata_only,
            } => crate::release::run_release_check(crate::release::ReleaseCheckOptions {
                version: version.clone(),
                metadata_only: *metadata_only,
            }),
            Commands::HooksInstall { level, runtime } => {
                self.handle_hooks_install(level.as_deref(), runtime)
            }
            Commands::HooksRemove { level, runtime } => {
                self.handle_hooks_remove(level.as_deref(), runtime)
            }
            Commands::HooksStatus { runtime } => self.handle_hooks_status(runtime),
            Commands::ShellConfig { shell } => self.handle_shell_config(shell.as_deref()),
            Commands::Complete { target, prefix } => {
                self.handle_complete(target, prefix.as_deref())
            }
        }
    }

    fn handle_default_switcher(&self) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::git::GitRepository;
        use crate::process::ProcessManager;
        use crate::tui::{
            WorktreeSwitchAction, WorktreeSwitchTui,
            build_worktree_switch_model_with_protected_branches,
        };

        info!("Starting default worktree switcher");

        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;
        let config_manager = ConfigManager::new()?;
        let protected_branches = config_manager.get().git.protected_branches.clone();
        let worktrees = git_repo.list_worktrees()?;
        let statuses =
            Self::collect_worktree_runtime_statuses(&git_repo, &worktrees, ProcessManager::new());
        let model = build_worktree_switch_model_with_protected_branches(
            &worktrees,
            &statuses,
            &protected_branches,
        );

        if self.dry_run {
            Self::print_switcher_preview(&model);
            return Ok(());
        }

        let switcher = WorktreeSwitchTui::new(model);
        match switcher.run()? {
            Some(WorktreeSwitchAction::Switch(target)) => self.handle_switcher_target(target),
            Some(WorktreeSwitchAction::Remove(target)) => self.handle_switcher_remove(target),
            None => {
                println!("No worktree selected");
                Ok(())
            }
        }
    }

    fn collect_worktree_runtime_statuses(
        git_repo: &crate::git::GitRepository,
        worktrees: &[crate::git::WorktreeInfo],
        mut process_manager: crate::process::ProcessManager,
    ) -> Vec<crate::tui::WorktreeRuntimeStatus> {
        let current_dir = std::env::current_dir().unwrap_or_else(|_| git_repo.root_path().into());
        let current_dir =
            std::fs::canonicalize(&current_dir).unwrap_or_else(|_| current_dir.clone());
        let worktree_paths = worktrees
            .iter()
            .map(|worktree| {
                std::fs::canonicalize(&worktree.path).unwrap_or_else(|_| worktree.path.clone())
            })
            .collect::<Vec<_>>();
        let current_worktree_index = worktree_paths
            .iter()
            .enumerate()
            .filter(|(_, path)| current_dir.starts_with(path))
            .max_by_key(|(_, path)| path.components().count())
            .map(|(index, _)| index);

        worktrees
            .iter()
            .enumerate()
            .map(|(index, worktree)| crate::tui::WorktreeRuntimeStatus {
                path: worktree.path.clone(),
                is_current: current_worktree_index == Some(index),
                is_dirty: git_repo
                    .has_uncommitted_changes(&worktree.path)
                    .unwrap_or(false),
                is_occupied: process_manager
                    .has_processes_in_directory(&worktree.path)
                    .unwrap_or(false),
                last_touched: worktree_last_touched(&worktree.path),
            })
            .collect()
    }

    fn print_switcher_preview(model: &crate::tui::WorktreeSwitchModel) {
        println!(
            "Would open interactive worktree switcher with {} worktrees:",
            model.rows.len()
        );

        for row in &model.rows {
            let badges = if row.badges.is_empty() {
                String::new()
            } else {
                format!(" [{}]", row.badges.join(", "))
            };
            println!("  - {}{} {}", row.branch_label, badges, row.path_label);
        }
    }

    fn handle_switcher_target(&self, target: crate::tui::WorktreeSwitchTarget) -> Result<()> {
        if let Some(branch) = target.branch.as_deref() {
            let path = target.path.to_string_lossy().into_owned();
            self.handle_switch(Some(branch), Some(path.as_str()), false, false, false)
        } else {
            self.handle_existing_worktree_jump(&target.path)
        }
    }

    fn handle_switcher_remove(&self, target: crate::tui::WorktreeRemovalTarget) -> Result<()> {
        use crate::git::GitRepository;

        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;

        println!(
            "Removing worktree for branch '{}': {}",
            target.branch,
            target.path.display()
        );
        git_repo.remove_worktree(&target.path)?;

        match git_repo.delete_branch(&target.branch, false) {
            Ok(()) => {
                println!("Removed worktree and branch: {}", target.branch);
            }
            Err(err) => {
                println!(
                    "Removed worktree but kept branch '{}': {}",
                    target.branch, err
                );
            }
        }

        if let Err(err) = git_repo.prune_worktrees() {
            log::warn!("Failed to prune worktrees: {}", err);
        }

        Ok(())
    }

    fn handle_existing_worktree_jump(&self, worktree_path: &Path) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::terminal::TerminalMode;

        let config_manager = ConfigManager::new()?;
        let config = config_manager.get();
        let terminal_mode = if let Some(mode_str) = &self.terminal {
            TerminalMode::from_str(mode_str).unwrap_or(TerminalMode::Tab)
        } else {
            TerminalMode::from_str(&config.terminal_mode).unwrap_or(TerminalMode::Tab)
        };

        let mut report = SwitchOutcomeReport::new(worktree_path.to_path_buf());
        report.skipped("Worktree creation", "already existed");
        self.record_terminal_handoff(
            &mut report,
            worktree_path,
            terminal_mode,
            config.terminal.app.as_str(),
            &config.terminal,
        );
        report.finish();

        Ok(())
    }

    fn handle_switch(
        &self,
        branch: Option<&str>,
        path: Option<&str>,
        latest: bool,
        waiting: bool,
        no_cow: bool,
    ) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::cow;
        use crate::git::GitRepository;
        use crate::post_create::{PostCreateSetupStatus, run_post_create_setup};
        use crate::rewrite::PathRewriter;
        use crate::terminal::TerminalMode;

        // Find the Git repository
        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;
        let branch = self.resolve_switch_branch(&git_repo, branch, latest, waiting)?;

        info!("Switching to branch: {}", branch);

        let config_manager = ConfigManager::new()?;
        let config = config_manager.get();

        // Determine worktree path
        let worktree_path = if let Some(path) = path {
            PathBuf::from(path)
        } else {
            git_repo.get_worktree_path_with_base(&branch, config.worktrees_path.as_deref())
        };

        if self.dry_run {
            println!(
                "Would switch to branch '{}' at path: {}",
                branch,
                worktree_path.display()
            );
            if !no_cow && cow::is_cow_supported(&worktree_path).unwrap_or(false) {
                println!("Would use Copy-on-Write for fast worktree creation");
            } else {
                println!("Would use traditional Git worktree creation");
            }
            return Ok(());
        }

        let mut report = SwitchOutcomeReport::new(worktree_path.clone());
        let mut worktree_created = false;
        let mut checkout_warning = None;

        // Check if worktree already exists
        if worktree_path.exists() {
            println!("📁 Worktree already exists at: {}", worktree_path.display());
            report.skipped("Worktree creation", "already existed");
        } else {
            println!("🚀 Creating worktree for branch '{}'", branch);

            // Choose creation method based on CoW support and user preference
            let use_cow =
                !no_cow && config.use_cow && cow::is_cow_supported(&worktree_path).unwrap_or(false);

            if use_cow {
                println!("⚡ Using Copy-on-Write for instant creation...");

                // Create worktree using traditional method first
                Self::create_worktree_with_recovery(&git_repo, &branch, &worktree_path)?;

                // If we have existing worktrees, try CoW enhancement
                let worktrees = git_repo.list_worktrees()?;
                if let Some(main_worktree) = worktrees.iter().find(|wt| wt.is_primary) {
                    // Remove the traditionally created worktree files
                    if worktree_path.exists() {
                        std::fs::remove_dir_all(&worktree_path)?;
                    }

                    // Clone using CoW
                    if let Err(e) = cow::clone_directory(&main_worktree.path, &worktree_path) {
                        log::warn!("CoW failed, falling back to traditional method: {}", e);
                        // Recreate using traditional method
                        Self::create_worktree_with_recovery(&git_repo, &branch, &worktree_path)?;
                    } else {
                        // Rewrite paths in the CoW copy
                        let rewriter = PathRewriter::new(&main_worktree.path, &worktree_path);
                        if let Err(e) = rewriter.rewrite_paths() {
                            log::warn!("Path rewriting failed: {}", e);
                        }

                        // Switch to the correct branch
                        let output = Command::new("git")
                            .args(["checkout", branch.as_str()])
                            .current_dir(&worktree_path)
                            .output()?;

                        if !output.status.success() {
                            let error = String::from_utf8_lossy(&output.stderr);
                            let error = error.trim().to_string();
                            log::warn!("Failed to checkout branch in CoW worktree: {}", error);
                            checkout_warning = Some(error);
                        }
                    }
                }
            } else {
                println!("📦 Using traditional Git worktree creation...");
                Self::create_worktree_with_recovery(&git_repo, &branch, &worktree_path)?;
            }

            if worktree_path.exists() {
                report.done("Worktree creation", "created");
            } else {
                report.warned(
                    "Worktree creation",
                    format!(
                        "path was not found after creation: {}",
                        worktree_path.display()
                    ),
                );
            }
            worktree_created = true;
        }

        Self::record_branch_checkout(&mut report, &worktree_path, &branch, checkout_warning);

        match run_post_create_setup(&worktree_path, worktree_created) {
            PostCreateSetupStatus::Installed => {
                println!("📦 Detected pnpm repo, ran `pnpm install`");
            }
            PostCreateSetupStatus::Warned(reason) => {
                println!(
                    "⚠️  Detected pnpm repo but `pnpm install` failed: {}",
                    reason
                );
            }
            PostCreateSetupStatus::SkippedExistingWorktree
            | PostCreateSetupStatus::SkippedNonPnpmRepo => {}
        }

        let terminal_mode = if let Some(mode_str) = &self.terminal {
            TerminalMode::from_str(mode_str).unwrap_or(TerminalMode::Tab)
        } else {
            TerminalMode::from_str(&config.terminal_mode).unwrap_or(TerminalMode::Tab)
        };

        self.record_terminal_handoff(
            &mut report,
            &worktree_path,
            terminal_mode,
            config.terminal.app.as_str(),
            &config.terminal,
        );
        report.finish();

        Ok(())
    }

    fn record_branch_checkout(
        report: &mut SwitchOutcomeReport,
        worktree_path: &Path,
        branch: &str,
        checkout_warning: Option<String>,
    ) {
        match Self::current_branch_at_path(worktree_path) {
            Ok(current_branch) if current_branch == branch && checkout_warning.is_none() => {
                report.done("Branch checkout", branch);
            }
            Ok(current_branch) if current_branch == branch => {
                report.warned(
                    "Branch checkout",
                    format!(
                        "checkout reported warning for {}: {}",
                        branch,
                        checkout_warning.unwrap_or_default()
                    ),
                );
            }
            Ok(current_branch) => {
                let found = if current_branch.is_empty() {
                    "detached HEAD".to_string()
                } else {
                    current_branch
                };
                let detail = match checkout_warning {
                    Some(warning) if !warning.is_empty() => {
                        format!("expected {branch}, found {found}; checkout failed: {warning}")
                    }
                    _ => format!(
                        "expected {branch}, found {found}. Use a different --path or run `warp ls` to inspect worktrees."
                    ),
                };
                report.warned("Branch checkout", detail);
            }
            Err(error) => {
                let detail = match checkout_warning {
                    Some(warning) if !warning.is_empty() => {
                        format!("could not verify {branch}: {error}; checkout failed: {warning}")
                    }
                    _ => format!("could not verify {branch}: {error}"),
                };
                report.warned("Branch checkout", detail);
            }
        }
    }

    fn current_branch_at_path(worktree_path: &Path) -> Result<String> {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(worktree_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to verify worktree branch: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Failed to verify worktree branch: {}",
                error.trim()
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn record_terminal_handoff(
        &self,
        report: &mut SwitchOutcomeReport,
        worktree_path: &Path,
        terminal_mode: crate::terminal::TerminalMode,
        terminal_app: &str,
        terminal_config: &crate::config::TerminalConfig,
    ) {
        use crate::terminal::{TerminalLaunchOptions, TerminalManager};

        let terminal_manager = TerminalManager;
        let launch_options = TerminalLaunchOptions {
            auto_activate: terminal_config.auto_activate,
            init_commands: terminal_config.init_commands.clone(),
        };

        match terminal_manager.switch_to_worktree_with_options(
            &worktree_path,
            terminal_mode,
            None,
            Some(terminal_app),
            &launch_options,
        ) {
            Ok(()) => {
                report.done(
                    "Terminal handoff",
                    Self::terminal_handoff_success_detail(terminal_mode),
                );
            }
            Err(e) => {
                log::warn!("Terminal switching failed: {}", e);
                report.warned(
                    "Terminal handoff",
                    format!(
                        "failed: {e}. Retry with `--terminal echo` to print manual commands instead."
                    ),
                );
            }
        }
    }

    fn terminal_handoff_success_detail(
        terminal_mode: crate::terminal::TerminalMode,
    ) -> &'static str {
        use crate::terminal::TerminalMode;

        match terminal_mode {
            TerminalMode::Tab => "opened tab",
            TerminalMode::Window => "opened window",
            TerminalMode::InPlace => "printed cd command",
            TerminalMode::Echo => "printed manual commands",
            TerminalMode::Current => "started current-terminal shell",
        }
    }

    fn resolve_switch_branch(
        &self,
        git_repo: &crate::git::GitRepository,
        branch: Option<&str>,
        latest: bool,
        waiting: bool,
    ) -> Result<String> {
        let selector_count =
            usize::from(branch.is_some()) + usize::from(latest) + usize::from(waiting);
        if selector_count != 1 {
            return Err(anyhow::anyhow!(
                "Specify exactly one of [BRANCH], --latest, or --waiting"
            ));
        }

        if let Some(branch) = branch {
            return Ok(branch.to_string());
        }

        use crate::agents::{AgentDiscovery, AgentSessionState};
        use crate::config::ConfigManager;
        use chrono::Local;

        let config_manager = ConfigManager::new()?;
        let discovery = AgentDiscovery::with_max_history_sessions(
            Self::agent_monitored_paths(git_repo)?,
            config_manager.get().agent.max_activities,
        );
        let sessions = discovery.discover(Local::now())?;

        let branch = if waiting {
            sessions
                .into_iter()
                .find(|session| {
                    session.state == AgentSessionState::Waiting
                        && session
                            .branch
                            .as_ref()
                            .is_some_and(|branch| !branch.is_empty())
                })
                .and_then(|session| session.branch)
        } else {
            sessions
                .into_iter()
                .find(|session| {
                    session.state != AgentSessionState::Completed
                        && session
                            .branch
                            .as_ref()
                            .is_some_and(|branch| !branch.is_empty())
                })
                .and_then(|session| session.branch)
        };

        branch.ok_or_else(|| {
            if waiting {
                anyhow::anyhow!("No waiting agent branches were found for this repository")
            } else {
                anyhow::anyhow!("No recent agent branches were found for this repository")
            }
        })
    }

    fn agent_monitored_paths(git_repo: &crate::git::GitRepository) -> Result<Vec<PathBuf>> {
        let mut monitored_paths = vec![git_repo.root_path().to_path_buf()];
        monitored_paths.extend(
            git_repo
                .list_worktrees()?
                .into_iter()
                .map(|worktree| worktree.path),
        );
        monitored_paths.sort();
        monitored_paths.dedup();
        Ok(monitored_paths)
    }

    fn handle_ls(&self, debug: bool) -> Result<()> {
        use crate::git::GitRepository;
        use crate::process::ProcessManager;

        info!("Listing worktrees");

        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;

        if self.dry_run {
            println!("Would list all worktrees");
            return Ok(());
        }

        let worktrees = git_repo.list_worktrees()?;
        let mut process_manager = ProcessManager::new();

        if worktrees.is_empty() {
            println!("📭 No worktrees found");
            return Ok(());
        }

        println!("📁 Git Worktrees:");
        println!();

        for (i, worktree) in worktrees.iter().enumerate() {
            let status_icon = if worktree.is_primary { "🏠" } else { "🌿" };
            let branch_display = if worktree.is_detached {
                let short_head: String = worktree.head.chars().take(8).collect();
                format!("(detached: {})", short_head)
            } else {
                worktree.branch.clone()
            };
            let is_dirty = git_repo
                .has_uncommitted_changes(&worktree.path)
                .unwrap_or(false);
            let is_busy = !worktree.is_current
                && process_manager
                    .has_processes_in_directory(&worktree.path)
                    .unwrap_or(false);
            let labels = Self::worktree_status_labels(worktree, is_dirty, is_busy);
            let label_display = Self::format_status_labels(&labels);

            println!(
                "{}  {}{} {}",
                status_icon,
                branch_display,
                label_display,
                worktree.path.display()
            );

            if debug {
                println!("     HEAD: {}", worktree.head);
                println!("     Primary: {}", worktree.is_primary);
                println!("     Current: {}", worktree.is_current);
                println!("     Detached: {}", worktree.is_detached);
                println!("     Dirty: {}", is_dirty);
                println!("     Busy: {}", is_busy);
                if i < worktrees.len() - 1 {
                    println!();
                }
            }
        }

        println!();
        println!("📊 Total: {} worktrees", worktrees.len());

        Ok(())
    }

    fn worktree_status_labels(
        worktree: &crate::git::WorktreeInfo,
        is_dirty: bool,
        is_busy: bool,
    ) -> Vec<&'static str> {
        let mut labels = Vec::new();
        if worktree.is_primary {
            labels.push("primary");
        }
        if worktree.is_current {
            labels.push("current");
        }
        if is_dirty {
            labels.push("dirty");
        }
        if worktree.is_detached {
            labels.push("detached");
        }
        if is_busy {
            labels.push("busy");
        }
        labels
    }

    fn format_status_labels(labels: &[&str]) -> String {
        if labels.is_empty() {
            String::new()
        } else {
            format!(" [{}]", labels.join(" "))
        }
    }

    fn handle_cleanup(
        &self,
        mode: &str,
        force: bool,
        kill: bool,
        no_kill: bool,
        interactive: bool,
    ) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::git::GitRepository;
        use crate::process::ProcessManager;

        info!("Cleaning up worktrees with mode: {}", mode);

        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;
        let config_manager = ConfigManager::new()?;
        let git_config = config_manager.get().git.clone();
        let mut process_manager = ProcessManager::new();

        if self.dry_run {
            println!("Would cleanup worktrees with mode: {}", mode);
            return Ok(());
        }

        // Fetch latest changes for accurate analysis
        println!("🔄 Fetching latest changes...");
        if !git_repo.fetch_branches()? {
            println!("⚠️  Fetch failed, analysis may be outdated");
        }

        let worktrees = git_repo.list_worktrees()?;
        let branch_statuses =
            git_repo.analyze_branches_for_cleanup_with_config(&worktrees, &git_config)?;

        if branch_statuses.is_empty() {
            println!("✨ No worktrees to clean up");
            return Ok(());
        }

        let mut candidates = Vec::new();
        let mut blocked = Vec::new();

        // Filter based on mode
        for status in branch_statuses {
            let should_include = match mode {
                "all" => true,
                "merged" => status.is_merged,
                "remoteless" => !status.has_remote,
                "interactive" => true, // Will be filtered in interactive mode
                _ => {
                    println!("❌ Unknown cleanup mode: {}", mode);
                    return Ok(());
                }
            };

            if should_include {
                if status.has_uncommitted_changes && !force {
                    blocked.push(status);
                } else {
                    candidates.push(status);
                }
            }
        }

        if !blocked.is_empty() {
            println!("🚧 Skipped cleanup branches:");
            for branch in &blocked {
                println!(
                    "  • {} at {} [{}; dirty; use --force to include]",
                    branch.branch,
                    branch.path.display(),
                    crate::tui::cleanup_reason_label(branch)
                );
            }
            println!();
        }

        if candidates.is_empty() {
            println!("✨ No worktrees match cleanup criteria for mode: {}", mode);
            return Ok(());
        }

        // Show what would be cleaned up
        println!("🧹 Cleanup candidates:");
        for candidate in &candidates {
            let remote = if candidate.has_remote {
                "remote"
            } else {
                "no remote"
            };
            let dirty = if candidate.has_uncommitted_changes {
                "dirty"
            } else {
                "clean"
            };
            println!(
                "  • {} at {} [{}; {}; {}]",
                candidate.branch,
                candidate.path.display(),
                crate::tui::cleanup_reason_label(candidate),
                remote,
                dirty
            );
        }

        if interactive {
            use crate::tui::CleanupTui;

            println!("\n🤖 Starting interactive cleanup...");
            let cleanup_tui = CleanupTui::with_candidates(candidates.clone());
            let selected_branches = cleanup_tui.run()?;

            if selected_branches.is_empty() {
                println!("❌ No branches selected for cleanup");
                return Ok(());
            }

            // Update candidates to only include selected branches
            candidates.retain(|c| selected_branches.contains(&c.branch));

            if candidates.is_empty() {
                println!("✨ No matching candidates found");
                return Ok(());
            }

            println!(
                "✅ Selected {} branches for cleanup",
                selected_branches.len()
            );
        }

        // Confirm unless auto-confirmed
        if !self.auto_confirm {
            print!("\n❓ Proceed with cleanup? [y/N]: ");
            use std::io::{self, Write};
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            if !input.trim().to_lowercase().starts_with('y') {
                println!("❌ Cleanup cancelled");
                return Ok(());
            }
        }

        // Perform cleanup
        let mut cleaned = 0;
        let mut failed = 0;

        for candidate in candidates {
            println!("🗑️  Removing worktree: {}", candidate.branch);

            // Handle process management
            if kill && !no_kill {
                println!("🔍 Checking for processes in worktree...");
                match process_manager.find_processes_in_directory(&candidate.path) {
                    Ok(processes) if !processes.is_empty() => {
                        println!("⚠️  Found {} processes in worktree", processes.len());
                        if !process_manager.terminate_processes(&processes, self.auto_confirm)? {
                            println!("❌ Failed to terminate processes, skipping worktree");
                            failed += 1;
                            continue;
                        }
                    }
                    Ok(_) => {
                        println!("✅ No processes found in worktree");
                    }
                    Err(e) => {
                        println!("⚠️  Failed to check processes: {}", e);
                    }
                }
            } else if !no_kill {
                // Default behavior - check for processes but don't auto-kill
                match process_manager.has_processes_in_directory(&candidate.path) {
                    Ok(true) => {
                        if force {
                            println!(
                                "⚠️  Processes found but --force specified, continuing anyway"
                            );
                        } else {
                            println!(
                                "❌ Processes found in worktree, use --kill to terminate them or --force to ignore"
                            );
                            println!(
                                "💡 Run `warp cleanup --mode {mode} --kill` to terminate them, use `--force` to ignore them, or stop the process manually."
                            );
                            failed += 1;
                            continue;
                        }
                    }
                    Ok(false) => {
                        println!("✅ No processes found in worktree");
                    }
                    Err(e) => {
                        println!("⚠️  Failed to check processes: {}", e);
                    }
                }
            }

            // Remove worktree
            match git_repo.remove_worktree(&candidate.path) {
                Ok(()) => {
                    // Try to delete the branch if it's safe
                    if candidate.is_merged || force {
                        match git_repo.delete_branch(&candidate.branch, force) {
                            Ok(()) => {
                                println!("✅ Removed worktree and branch: {}", candidate.branch)
                            }
                            Err(e) => {
                                println!(
                                    "⚠️  Removed worktree but failed to delete branch {}: {}",
                                    candidate.branch, e
                                );
                            }
                        }
                    } else {
                        println!("✅ Removed worktree: {} (branch kept)", candidate.branch);
                    }
                    cleaned += 1;
                }
                Err(e) => {
                    println!("❌ Failed to remove worktree {}: {}", candidate.branch, e);
                    failed += 1;
                }
            }
        }

        // Prune stale worktree references
        if let Err(e) = git_repo.prune_worktrees() {
            log::warn!("Failed to prune worktrees: {}", e);
        }

        println!();
        println!(
            "📊 Cleanup complete: {} removed, {} failed",
            cleaned, failed
        );

        Ok(())
    }

    fn handle_config(&self, show: bool, edit: bool) -> Result<()> {
        use crate::config::ConfigManager;

        info!("Config command");
        if self.dry_run {
            println!("Would manage configuration");
            return Ok(());
        }

        let config_manager = ConfigManager::new()?;

        if show {
            // Show current configuration
            println!("📋 Current Git-Warp Configuration:");
            println!("Config file: {}", config_manager.config_path().display());
            println!();

            let config = config_manager.get();

            println!("🖥️  Terminal Settings:");
            println!("  Mode: {}", config.terminal_mode);
            println!("  Use CoW: {}", config.use_cow);
            println!("  Auto-confirm: {}", config.auto_confirm);
            if let Some(path) = &config.worktrees_path {
                println!("  Worktrees path: {}", path.display());
            }
            println!();

            println!("🔧 Git Settings:");
            println!("  Default branch: {}", config.git.default_branch);
            println!("  Protected branches: {:?}", config.git.protected_branches);
            println!("  Auto-fetch: {}", config.git.auto_fetch);
            println!("  Auto-prune: {}", config.git.auto_prune);
            println!();

            println!("⚙️  Process Settings:");
            println!("  Check processes: {}", config.process.check_processes);
            println!("  Auto-kill: {}", config.process.auto_kill);
            println!("  Kill timeout: {}s", config.process.kill_timeout);
            println!();

            println!("🖥️  Terminal Integration:");
            println!("  App: {}", config.terminal.app);
            println!("  Auto-activate: {}", config.terminal.auto_activate);
            println!("  Init commands: {:?}", config.terminal.init_commands);
            println!();

            println!("🤖 Agent Settings:");
            println!("  Enabled: {}", config.agent.enabled);
            println!("  Refresh rate: {}ms", config.agent.refresh_rate);
            println!("  Max activities: {}", config.agent.max_activities);
            println!("  Claude hooks: {}", config.agent.claude_hooks);
        } else if edit {
            if !config_manager.config_exists() {
                config_manager.create_default_config()?;
                println!(
                    "✅ Created default configuration at: {}",
                    config_manager.config_path().display()
                );
            }

            Self::open_in_editor(config_manager.config_path())?;
        } else {
            // Show help for config command
            println!("⚙️  Configuration Management");
            println!();
            println!("Usage:");
            println!("  warp config --show     Show current configuration");
            println!("  warp config --edit     Open configuration in your editor");
            println!();
            println!("Configuration file location:");
            println!("  {}", config_manager.config_path().display());
            println!();
            println!("Environment variables (GIT_WARP_ prefix):");
            println!("  GIT_WARP_TERMINAL_MODE=tab|window|current|inplace|echo");
            println!("  GIT_WARP_USE_COW=true|false");
            println!("  GIT_WARP_AUTO_CONFIRM=true|false");
            println!("  GIT_WARP_WORKTREES_PATH=/custom/path");
        }

        Ok(())
    }

    fn handle_doctor(&self) -> Result<()> {
        use crate::config::ConfigManager;
        use crate::cow;
        use crate::git::GitRepository;

        let config_manager = ConfigManager::new()?;
        let config = config_manager.get();
        let repo = GitRepository::find().ok();
        let mut next_steps = Vec::new();

        println!("🩺 Git-Warp Doctor");
        println!("==================");
        println!();
        println!("Checks:");

        if config_manager.config_exists() {
            Self::doctor_ok(
                "Config file",
                format!("found at {}", config_manager.config_path().display()),
            );
        } else {
            Self::doctor_warn(
                "Config file",
                format!("missing at {}", config_manager.config_path().display()),
            );
            next_steps
                .push("Run `warp config --edit` to create and review your config.".to_string());
        }

        let worktree_base = if let Some(repo) = &repo {
            Self::doctor_ok("Git repository", repo.root_path().display().to_string());
            let sample_worktree =
                repo.get_worktree_path_with_base("doctor-check", config.worktrees_path.as_deref());
            let base = sample_worktree
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| sample_worktree.clone());
            Self::doctor_info("Worktree base path", base.display().to_string());
            base
        } else {
            Self::doctor_warn("Git repository", "not detected from current directory");
            next_steps.push(
                "Run this command inside a Git repository before creating worktrees.".to_string(),
            );
            config
                .worktrees_path
                .clone()
                .unwrap_or_else(|| PathBuf::from(".worktrees"))
        };

        let cow_check_path = Self::nearest_existing_parent(&worktree_base);
        let cow_supported = cow::is_cow_supported(&cow_check_path);
        match &cow_supported {
            Ok(true) => Self::doctor_ok(
                "Copy-on-Write",
                format!(
                    "available on filesystem containing {}",
                    worktree_base.display()
                ),
            ),
            Ok(false) => Self::doctor_info(
                "Copy-on-Write",
                "not available on this filesystem; Git-Warp will use git worktree add",
            ),
            Err(error) => {
                Self::doctor_warn("Copy-on-Write", format!("could not check support: {error}"))
            }
        }

        Self::doctor_info(
            "Terminal",
            format!("mode {}, app {}", config.terminal_mode, config.terminal.app),
        );

        if repo.is_none() || !matches!(cow_supported, Ok(true)) {
            next_steps.push(
                "Run `warp switch --no-cow <branch>` to skip CoW checks for a switch.".to_string(),
            );
        }

        let hooks_installed = Self::doctor_hooks_installed();
        if hooks_installed {
            Self::doctor_ok("Agent hooks", "git-warp hooks found for Claude or Codex");
        } else {
            Self::doctor_warn("Agent hooks", "no user or project git-warp hooks found");
            next_steps.push(
                "Run `warp hooks-install --level user --runtime all` to enable live agent monitoring."
                    .to_string(),
            );
        }

        if repo.is_some() && config_manager.config_exists() && hooks_installed {
            next_steps.push("Run `warp switch <branch>` to create or open a worktree.".to_string());
        }

        println!();
        println!("Next steps:");
        if next_steps.is_empty() {
            println!("  - No immediate setup steps found.");
        } else {
            for step in next_steps {
                println!("  - {step}");
            }
        }

        Ok(())
    }

    fn doctor_ok(label: &str, detail: impl AsRef<str>) {
        println!("  ✅ {label}: {}", detail.as_ref());
    }

    fn doctor_warn(label: &str, detail: impl AsRef<str>) {
        println!("  ⚠️  {label}: {}", detail.as_ref());
    }

    fn doctor_info(label: &str, detail: impl AsRef<str>) {
        println!("  ℹ️  {label}: {}", detail.as_ref());
    }

    fn nearest_existing_parent(path: &Path) -> PathBuf {
        let mut candidate = path.to_path_buf();
        while !candidate.exists() {
            if !candidate.pop() {
                return PathBuf::from(".");
            }
        }
        candidate
    }

    fn doctor_hooks_installed() -> bool {
        let mut paths = Vec::new();

        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".claude").join("settings.json"));
            paths.push(home.join(".codex").join("hooks.json"));
        }

        if let Ok(current_dir) = std::env::current_dir() {
            paths.push(current_dir.join(".claude").join("settings.json"));
            paths.push(current_dir.join(".codex").join("hooks.json"));
        }

        paths.into_iter().any(|path| {
            std::fs::read_to_string(path)
                .map(|content| content.contains("\"git_warp_hook_id\""))
                .unwrap_or(false)
        })
    }

    fn handle_agents(&self) -> Result<()> {
        use crate::agents::AgentDiscovery;
        use crate::config::ConfigManager;
        use crate::git::GitRepository;
        use crate::tui::AgentsDashboard;

        info!("Starting agents dashboard");
        if self.dry_run {
            println!("Would start agents dashboard");
            return Ok(());
        }

        let git_repo = GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;
        let config_manager = ConfigManager::new()?;
        let dashboard = AgentsDashboard::new(AgentDiscovery::with_max_history_sessions(
            Self::agent_monitored_paths(&git_repo)?,
            config_manager.get().agent.max_activities,
        ));
        dashboard.run()
    }

    fn handle_hooks_install(&self, level: Option<&str>, runtime: &str) -> Result<()> {
        use crate::hooks::HooksManager;

        info!(
            "Installing hooks at level: {:?}, runtime: {}",
            level, runtime
        );
        if self.dry_run {
            println!(
                "Would install Git-Warp hooks at level: {:?}, runtime: {}",
                level.unwrap_or("console"),
                runtime
            );
            return Ok(());
        }

        HooksManager::install_hooks(level, runtime)
    }

    fn handle_hooks_remove(&self, level: Option<&str>, runtime: &str) -> Result<()> {
        use crate::hooks::HooksManager;

        info!("Removing hooks at level: {:?}, runtime: {}", level, runtime);
        if self.dry_run {
            println!(
                "Would remove Git-Warp hooks at level: {:?}, runtime: {}",
                level.unwrap_or("user"),
                runtime
            );
            return Ok(());
        }

        let level = level.unwrap_or("user");
        HooksManager::remove_hooks(level, runtime)
    }

    fn handle_hooks_status(&self, runtime: &str) -> Result<()> {
        use crate::hooks::HooksManager;

        info!("Checking hooks status for runtime: {}", runtime);
        HooksManager::show_hooks_status(runtime)
    }

    fn handle_shell_config(&self, shell: Option<&str>) -> Result<()> {
        info!("Generating shell config for: {:?}", shell);
        let detected_shell = shell
            .map(str::to_string)
            .or_else(|| {
                std::env::var("SHELL").ok().and_then(|value| {
                    value
                        .rsplit('/')
                        .next()
                        .map(str::to_string)
                        .filter(|value| !value.is_empty())
                })
            })
            .unwrap_or_else(|| "bash".to_string());

        match detected_shell.as_str() {
            "bash" => {
                println!("# Add to ~/.bashrc");
                println!(
                    "{}",
                    r#"warp_cd() { eval "$(warp --terminal echo "$@")"; }"#
                );
                println!(
                    "{}",
                    r#"_warp_completion() {
    local cur prev commands branches
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    commands="switch ls list cleanup config agents doctor hooks-install hooks-remove hooks-status shell-config"

    if [[ "$prev" == "switch" ]]; then
        branches="$(warp __complete branches "$cur" 2>/dev/null)"
        COMPREPLY=($(compgen -W "$branches" -- "$cur"))
    elif [[ $COMP_CWORD -eq 1 ]]; then
        branches="$(warp __complete branches "$cur" 2>/dev/null)"
        COMPREPLY=($(compgen -W "$commands $branches" -- "$cur"))
    fi
}
complete -F _warp_completion warp"#
                );
            }
            "zsh" => {
                println!("# Add to ~/.zshrc");
                println!(
                    "{}",
                    r#"warp_cd() { eval "$(warp --terminal echo "$@")"; }"#
                );
                println!(
                    "{}",
                    r#"_warp_branch_completions() {
    local -a branches
    branches=("${(@f)$(warp __complete branches "$PREFIX" 2>/dev/null)}")
    compadd -- "${branches[@]}"
}

_warp_completion() {
    local -a commands
    commands=(switch ls list cleanup config agents doctor hooks-install hooks-remove hooks-status shell-config)

    if (( CURRENT == 2 )); then
        compadd -- "${commands[@]}"
        _warp_branch_completions
    elif [[ ${words[2]} == switch && CURRENT == 3 ]]; then
        _warp_branch_completions
    fi
}
compdef _warp_completion warp"#
                );
            }
            "fish" => {
                println!("# Add to ~/.config/fish/config.fish");
                println!("function warp_cd");
                println!("    eval (warp --terminal echo $argv)");
                println!("end");
                println!(
                    "{}",
                    r#"complete -c warp -n '__fish_use_subcommand' -a 'switch ls list cleanup config agents doctor hooks-install hooks-remove hooks-status shell-config'
complete -c warp -n '__fish_use_subcommand' -f -a '(warp __complete branches (commandline -ct) 2>/dev/null)'
complete -c warp -n '__fish_seen_subcommand_from switch' -f -a '(warp __complete branches (commandline -ct) 2>/dev/null)'"#
                );
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported shell '{other}'. Supported shells: bash, zsh, fish"
                ));
            }
        }
        Ok(())
    }

    fn handle_complete(&self, target: &str, prefix: Option<&str>) -> Result<()> {
        match target {
            "branches" => {
                let git_repo =
                    crate::git::GitRepository::find().map_err(|_| Self::not_in_git_repo_error())?;
                for branch in git_repo.list_local_branches_matching_prefix(prefix.unwrap_or(""))? {
                    println!("{branch}");
                }
                Ok(())
            }
            other => Err(anyhow::anyhow!(
                "Unsupported completion target '{other}'. Supported targets: branches"
            )),
        }
    }

    fn open_in_editor(path: &Path) -> Result<()> {
        let editor = std::env::var("VISUAL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                std::env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            });

        if let Some(editor) = editor {
            let mut parts = editor.split_whitespace();
            let program = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("Invalid editor command"))?;
            let status = Command::new(program)
                .args(parts)
                .arg(path)
                .status()
                .map_err(|err| anyhow::anyhow!("Failed to launch editor '{}': {}", editor, err))?;

            if status.success() {
                return Ok(());
            }

            return Err(anyhow::anyhow!(
                "Editor '{}' exited with status {:?}",
                editor,
                status.code()
            ));
        }

        #[cfg(target_os = "macos")]
        {
            let status = Command::new("open")
                .args(["-t"])
                .arg(path)
                .status()
                .map_err(|err| anyhow::anyhow!("Failed to open config file: {}", err))?;

            if status.success() {
                return Ok(());
            }
        }

        Err(anyhow::anyhow!(
            "No editor configured. Set $VISUAL or $EDITOR to use `warp config --edit`"
        ))
    }
}

fn worktree_last_touched(path: &Path) -> Option<SystemTime> {
    let metadata = std::fs::metadata(path).ok()?;

    [metadata.modified().ok(), metadata.created().ok()]
        .into_iter()
        .flatten()
        .max()
}
