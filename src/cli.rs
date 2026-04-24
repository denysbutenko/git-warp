use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use log::info;
use std::path::{Path, PathBuf};
use std::process::Command;

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
}

impl Cli {
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
                    // No command or branch - show help
                    let mut cmd = Self::command();
                    cmd.print_help()?;
                    Ok(())
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
            Commands::HooksInstall { level, runtime } => {
                self.handle_hooks_install(level.as_deref(), runtime)
            }
            Commands::HooksRemove { level, runtime } => {
                self.handle_hooks_remove(level.as_deref(), runtime)
            }
            Commands::HooksStatus { runtime } => self.handle_hooks_status(runtime),
            Commands::ShellConfig { shell } => self.handle_shell_config(shell.as_deref()),
        }
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
        use crate::terminal::{TerminalManager, TerminalMode};

        // Find the Git repository
        let git_repo =
            GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a Git repository"))?;
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

        let mut worktree_created = false;

        // Check if worktree already exists
        if worktree_path.exists() {
            println!("📁 Worktree already exists at: {}", worktree_path.display());
        } else {
            println!("🚀 Creating worktree for branch '{}'", branch);

            // Choose creation method based on CoW support and user preference
            let use_cow =
                !no_cow && config.use_cow && cow::is_cow_supported(&worktree_path).unwrap_or(false);

            if use_cow {
                println!("⚡ Using Copy-on-Write for instant creation...");

                // Create worktree using traditional method first
                git_repo.create_worktree_and_branch(&branch, &worktree_path, None)?;

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
                        git_repo.create_worktree_and_branch(&branch, &worktree_path, None)?;
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
                            log::warn!("Failed to checkout branch in CoW worktree: {}", error);
                        }
                    }
                }
            } else {
                println!("📦 Using traditional Git worktree creation...");
                git_repo.create_worktree_and_branch(&branch, &worktree_path, None)?;
            }

            println!("✅ Worktree created successfully!");
            worktree_created = true;
        }

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

        // Handle terminal switching
        let terminal_mode = if let Some(mode_str) = &self.terminal {
            TerminalMode::from_str(mode_str).unwrap_or(TerminalMode::Tab)
        } else {
            TerminalMode::from_str(&config.terminal_mode).unwrap_or(TerminalMode::Tab)
        };
        let is_current_mode = matches!(terminal_mode, TerminalMode::Current);

        if is_current_mode {
            println!("🔄 Switched to worktree: {}", worktree_path.display());
        }

        let terminal_manager = TerminalManager;
        match terminal_manager.switch_to_worktree_with_app(
            &worktree_path,
            terminal_mode,
            None,
            Some(config.terminal.app.as_str()),
        ) {
            Ok(()) => {
                if !is_current_mode {
                    println!("🔄 Switched to worktree: {}", worktree_path.display());
                }
            }
            Err(e) => {
                log::warn!("Terminal switching failed: {}", e);
                println!("📍 Worktree created at: {}", worktree_path.display());
                println!("💡 Run: cd '{}'", worktree_path.display());
            }
        }

        Ok(())
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
        use chrono::Local;

        let discovery = AgentDiscovery::new(Self::agent_monitored_paths(git_repo)?);
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

        let git_repo =
            GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a Git repository"))?;

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
        use crate::git::GitRepository;
        use crate::process::ProcessManager;

        info!("Cleaning up worktrees with mode: {}", mode);

        let git_repo =
            GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a Git repository"))?;
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
        let branch_statuses = git_repo.analyze_branches_for_cleanup(&worktrees)?;

        if branch_statuses.is_empty() {
            println!("✨ No worktrees to clean up");
            return Ok(());
        }

        let mut candidates = Vec::new();

        // Filter based on mode
        for status in &branch_statuses {
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

            if should_include && (!status.has_uncommitted_changes || force) {
                candidates.push(status);
            }
        }

        if candidates.is_empty() {
            println!("✨ No worktrees match cleanup criteria for mode: {}", mode);
            return Ok(());
        }

        // Show what would be cleaned up
        println!("🧹 Cleanup candidates:");
        for candidate in &candidates {
            let uncommitted = if candidate.has_uncommitted_changes {
                " (⚠️  uncommitted)"
            } else {
                ""
            };
            let merged = if candidate.is_merged { " [merged]" } else { "" };
            println!(
                "  • {} at {}{}{}",
                candidate.branch,
                candidate.path.display(),
                merged,
                uncommitted
            );
        }

        if interactive {
            use crate::tui::CleanupTui;

            println!("\n🤖 Starting interactive cleanup...");
            let cleanup_tui = CleanupTui::new();
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

    fn handle_agents(&self) -> Result<()> {
        use crate::agents::AgentDiscovery;
        use crate::git::GitRepository;
        use crate::tui::AgentsDashboard;

        info!("Starting agents dashboard");
        if self.dry_run {
            println!("Would start agents dashboard");
            return Ok(());
        }

        let git_repo =
            GitRepository::find().map_err(|_| anyhow::anyhow!("Not in a Git repository"))?;
        let dashboard =
            AgentsDashboard::new(AgentDiscovery::new(Self::agent_monitored_paths(&git_repo)?));
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
            }
            "zsh" => {
                println!("# Add to ~/.zshrc");
                println!(
                    "{}",
                    r#"warp_cd() { eval "$(warp --terminal echo "$@")"; }"#
                );
            }
            "fish" => {
                println!("# Add to ~/.config/fish/config.fish");
                println!("function warp_cd");
                println!("    eval (warp --terminal echo $argv)");
                println!("end");
            }
            other => {
                return Err(anyhow::anyhow!(
                    "Unsupported shell '{other}'. Supported shells: bash, zsh, fish"
                ));
            }
        }
        Ok(())
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
