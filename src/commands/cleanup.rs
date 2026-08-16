use anyhow::Result;
use clap::ValueEnum;
use log::info;
use std::fmt;

use crate::cli::Cli;

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum CleanupMode {
    All,
    Merged,
    Remoteless,
    Interactive,
}

impl CleanupMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CleanupMode::All => "all",
            CleanupMode::Merged => "merged",
            CleanupMode::Remoteless => "remoteless",
            CleanupMode::Interactive => "interactive",
        }
    }
}

impl fmt::Display for CleanupMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Resolve the effective kill behavior for cleanup. `--no-kill` always wins,
/// matching the precedence established in #67 — explicit opt-out trumps any
/// config-driven auto-kill.
fn resolve_cleanup_kill(flag_kill: bool, flag_no_kill: bool, config_auto_kill: bool) -> bool {
    if flag_no_kill {
        return false;
    }
    flag_kill || config_auto_kill
}

pub fn run(
    cli: &Cli,
    mode: CleanupMode,
    force: bool,
    kill: bool,
    no_kill: bool,
    interactive: bool,
) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::git::GitRepository;
    use crate::process::ProcessManager;

    use super::util::not_in_git_repo_error;

    info!("Cleaning up worktrees with mode: {}", mode);

    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let config_manager = ConfigManager::new()?;
    let config = config_manager.get().clone();
    let git_config = config.git.clone();
    let effective_kill = resolve_cleanup_kill(kill, no_kill, config.process.auto_kill);
    let check_processes = config.process.check_processes;
    let kill_timeout = std::time::Duration::from_secs(config.process.kill_timeout);
    let mut process_manager = ProcessManager::new();

    if cli.dry_run {
        println!("🔎 Dry run: previewing cleanup with mode: {}", mode);
    } else if config.git.auto_fetch {
        // Fetch latest changes for accurate analysis
        println!("🔄 Fetching latest changes...");
        if !git_repo.fetch_branches()? {
            println!("⚠️  Fetch failed, analysis may be outdated");
        }
    } else {
        println!("ℹ️  Skipping fetch (git.auto_fetch=false); analysis uses local refs only.");
    }

    let worktrees = git_repo.list_worktrees()?;
    let analysis = git_repo.analyze_worktrees_for_cleanup_with_config(&worktrees, &git_config)?;

    println!("🧭 Cleanup base branch: {}", analysis.base_branch);

    if !analysis.skipped.is_empty() {
        println!("🛡️  Skipped (not eligible for cleanup):");
        for skip in &analysis.skipped {
            println!(
                "  • {} at {} [{}]",
                skip.branch_label,
                skip.path.display(),
                skip.reason.label()
            );
        }
        println!();
    }

    if analysis.candidates.is_empty() {
        println!("✨ No worktrees to clean up");
        return Ok(());
    }

    let mut candidates = Vec::new();
    let mut blocked = Vec::new();
    let mut mode_excluded = Vec::new();

    for status in analysis.candidates {
        let matches_mode = match mode {
            CleanupMode::All | CleanupMode::Interactive => true,
            CleanupMode::Merged => status.is_merged,
            CleanupMode::Remoteless => !status.has_remote,
        };

        if !matches_mode {
            mode_excluded.push(status);
            continue;
        }

        if status.has_uncommitted_changes && !force {
            blocked.push(status);
        } else {
            candidates.push(status);
        }
    }

    if !mode_excluded.is_empty() {
        println!("➖ Excluded by mode `{}`:", mode);
        for status in &mode_excluded {
            println!(
                "  • {} at {} [{}; {}; {}]",
                status.branch,
                status.path.display(),
                crate::tui::cleanup_reason_label_for_mode(status, mode.as_str()),
                if status.has_remote {
                    "remote"
                } else {
                    "no remote"
                },
                if status.has_uncommitted_changes {
                    "dirty"
                } else {
                    "clean"
                }
            );
        }
        println!();
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
    if check_processes {
        process_manager.refresh();
    }
    let mut process_busy = Vec::new();
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
        let busy = if check_processes {
            match process_manager.has_processes_in_directory(&candidate.path) {
                Ok(true) => {
                    process_busy.push(candidate.branch.clone());
                    "; process-busy"
                }
                Ok(false) => "",
                Err(_) => "",
            }
        } else {
            ""
        };
        println!(
            "  • {} at {} [{}; {}; {}{}]",
            candidate.branch,
            candidate.path.display(),
            crate::tui::cleanup_reason_label_for_mode(candidate, mode.as_str()),
            remote,
            dirty,
            busy,
        );
    }
    if !process_busy.is_empty() && !force && !effective_kill {
        println!(
            "💡 {} candidate(s) have running processes. Use --kill to terminate or --force to ignore.",
            process_busy.len()
        );
    }

    if cli.dry_run {
        println!("\n🔎 Dry run complete: no worktrees were removed.");
        return Ok(());
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
    if !cli.auto_confirm {
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

    if check_processes {
        process_manager.refresh();
    }
    for candidate in candidates {
        println!("🗑️  Removing worktree: {}", candidate.branch);

        // Handle process management
        if effective_kill && check_processes {
            println!("🔍 Checking for processes in worktree...");
            match process_manager.find_processes_in_directory(&candidate.path) {
                Ok(processes) if !processes.is_empty() => {
                    println!("⚠️  Found {} processes in worktree", processes.len());
                    if !process_manager.terminate_processes(
                        &processes,
                        cli.auto_confirm,
                        kill_timeout,
                    )? {
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
        } else if !no_kill && check_processes {
            // Default behavior - check for processes but don't auto-kill
            match process_manager.has_processes_in_directory(&candidate.path) {
                Ok(true) => {
                    if force {
                        println!("⚠️  Processes found but --force specified, continuing anyway");
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
        match git_repo.remove_worktree(&candidate.path, force) {
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
    if config.git.auto_prune
        && let Err(e) = git_repo.prune_worktrees()
    {
        log::warn!("Failed to prune worktrees: {}", e);
    }

    println!();
    println!(
        "📊 Cleanup complete: {} removed, {} failed",
        cleaned, failed
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cleanup_kill_no_kill_flag_always_wins() {
        assert!(!resolve_cleanup_kill(true, true, true));
        assert!(!resolve_cleanup_kill(false, true, true));
        assert!(!resolve_cleanup_kill(true, true, false));
    }

    #[test]
    fn resolve_cleanup_kill_kill_flag_or_config_enables() {
        assert!(resolve_cleanup_kill(true, false, false));
        assert!(resolve_cleanup_kill(false, false, true));
        assert!(resolve_cleanup_kill(true, false, true));
    }

    #[test]
    fn resolve_cleanup_kill_default_is_off() {
        assert!(!resolve_cleanup_kill(false, false, false));
    }
}
