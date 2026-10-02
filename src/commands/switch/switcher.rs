use anyhow::{Result, anyhow};
use std::path::Path;

use crate::cli::Cli;
use crate::config::ConfigManager;
use crate::git::{GitRepository, WorktreeInfo};
use crate::process::ProcessManager;
use crate::tui::{
    WorktreeBatchRemoval, WorktreeRemovalTarget, WorktreeRuntimeStatus, WorktreeSwitchModel,
    WorktreeSwitchTarget,
};

use super::report::SwitchOutcomeReport;
use super::terminal::{record_terminal_handoff, resolve_terminal_mode};

use super::super::util::{abbreviate_path, not_in_git_repo_error, worktree_last_touched};

pub(super) fn collect_local_only_branches(
    git_repo: &GitRepository,
    worktrees: &[WorktreeInfo],
) -> Vec<String> {
    if !git_repo.has_remotes().unwrap_or(false) {
        return Vec::new();
    }
    worktrees
        .iter()
        .filter(|wt| !wt.branch.trim().is_empty() && !wt.is_detached)
        .filter_map(|wt| match git_repo.remote_branch_exists(&wt.branch) {
            Ok(false) => Some(wt.branch.clone()),
            _ => None,
        })
        .collect()
}

pub(super) fn collect_worktree_runtime_statuses(
    git_repo: &GitRepository,
    worktrees: &[WorktreeInfo],
    mut process_manager: ProcessManager,
) -> Vec<WorktreeRuntimeStatus> {
    process_manager.refresh();
    let current_dir = std::env::current_dir().unwrap_or_else(|_| git_repo.root_path().into());
    let current_dir = std::fs::canonicalize(&current_dir).unwrap_or_else(|_| current_dir.clone());
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
        .map(|(index, worktree)| WorktreeRuntimeStatus {
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

pub(super) fn print_switcher_preview(model: &WorktreeSwitchModel) {
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

pub(super) fn run_switcher_target(cli: &Cli, target: WorktreeSwitchTarget) -> Result<()> {
    if let Some(branch) = target.branch.as_deref() {
        let path = target.path.to_string_lossy().into_owned();
        super::run(cli, Some(branch), Some(path.as_str()), false, false, false)
    } else {
        run_existing_worktree_jump(cli, &target.path)
    }
}

pub(super) fn run_switcher_remove(target: WorktreeRemovalTarget) -> Result<()> {
    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let auto_prune = ConfigManager::new()?.get().git.auto_prune;

    let force_note = if target.force { " (forced)" } else { "" };
    git_repo.remove_worktree(&target.path, target.force)?;

    match git_repo.delete_branch(&target.branch, target.force) {
        Ok(()) => {
            println!(
                "🗑️  Removed worktree and branch '{}'{} ({})",
                target.branch,
                force_note,
                abbreviate_path(&target.path)
            );
        }
        Err(err) => {
            println!(
                "⚠️  Removed worktree '{}'{} but kept branch: {}",
                target.branch, force_note, err
            );
        }
    }

    if auto_prune && let Err(err) = git_repo.prune_worktrees() {
        log::warn!("Failed to prune worktrees: {}", err);
    }

    Ok(())
}

pub(super) fn run_switcher_batch_remove(batch: WorktreeBatchRemoval) -> Result<()> {
    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let auto_prune = ConfigManager::new()?.get().git.auto_prune;

    println!(
        "Batch removing {} selected worktree{}",
        batch.targets.len(),
        if batch.targets.len() == 1 { "" } else { "s" }
    );

    if !batch.skipped.is_empty() {
        println!(
            "Skipped {} worktree{}:",
            batch.skipped.len(),
            if batch.skipped.len() == 1 { "" } else { "s" }
        );
        for skipped in &batch.skipped {
            println!(
                "  - {} {} ({})",
                skipped.branch_label,
                skipped.path.display(),
                skipped.reason
            );
        }
    }

    if batch.targets.is_empty() {
        println!(
            "Batch removal complete: 0 removed, {} skipped, 0 failed",
            batch.skipped.len()
        );
        return Ok(());
    }

    let mut removed = 0;
    let mut failed = 0;

    for target in batch.targets {
        let force_note = if target.force { " (forced)" } else { "" };

        match git_repo.remove_worktree(&target.path, target.force) {
            Ok(()) => {
                removed += 1;
                match git_repo.delete_branch(&target.branch, target.force) {
                    Ok(()) => {
                        println!(
                            "🗑️  Removed worktree and branch '{}'{} ({})",
                            target.branch,
                            force_note,
                            abbreviate_path(&target.path)
                        );
                    }
                    Err(err) => {
                        println!(
                            "⚠️  Removed worktree '{}'{} but kept branch: {}",
                            target.branch, force_note, err
                        );
                    }
                }
            }
            Err(err) => {
                failed += 1;
                println!("❌ Failed to remove worktree '{}': {}", target.branch, err);
            }
        }
    }

    if auto_prune && let Err(err) = git_repo.prune_worktrees() {
        log::warn!("Failed to prune worktrees: {}", err);
    }

    println!(
        "Batch removal complete: {} removed, {} skipped, {} failed",
        removed,
        batch.skipped.len(),
        failed
    );

    if failed > 0 {
        return Err(anyhow!(
            "Batch removal finished with {failed} failed removal(s) (removed {removed})"
        ));
    }

    Ok(())
}

pub(super) fn run_existing_worktree_jump(cli: &Cli, worktree_path: &Path) -> Result<()> {
    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();
    let terminal_mode = resolve_terminal_mode(cli, &config.terminal_mode)?;

    let mut report = SwitchOutcomeReport::new(worktree_path.to_path_buf());
    report.skipped("Worktree creation", "already existed");
    record_terminal_handoff(
        &mut report,
        worktree_path,
        terminal_mode,
        config.terminal.app.as_str(),
        &config.terminal,
    );
    report.finish();

    Ok(())
}
