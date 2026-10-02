use anyhow::Result;
use log::info;
use std::path::PathBuf;

use crate::cli::Cli;

mod overlay;
mod report;
mod switcher;
mod terminal;
mod worktree_ops;

use overlay::cow_overlay_untracked;
use report::SwitchOutcomeReport;
use switcher::{
    collect_local_only_branches, collect_worktree_runtime_statuses, print_switcher_preview,
    run_switcher_batch_remove, run_switcher_remove, run_switcher_target,
};
use terminal::{record_branch_checkout, record_terminal_handoff, resolve_terminal_mode};
use worktree_ops::{
    compute_use_cow, create_worktree_for_source_with_recovery, dry_run_source_label,
    source_announcement,
};

use super::util::{agent_monitored_paths, not_in_git_repo_error};

pub fn run_default(cli: &Cli) -> Result<()> {
    use crate::agents::AgentDiscovery;
    use crate::config::ConfigManager;
    use crate::git::GitRepository;
    use crate::process::ProcessManager;
    use crate::tui::{WarpTui, WorktreeSwitchAction, build_worktree_switch_model_with_metadata};

    info!("Starting default worktree switcher");

    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();
    let protected_branches = config.git.protected_branches.clone();
    let worktrees = git_repo.list_worktrees()?;
    let statuses = collect_worktree_runtime_statuses(&git_repo, &worktrees, ProcessManager::new());
    let local_only_branches = collect_local_only_branches(&git_repo, &worktrees);
    let model = build_worktree_switch_model_with_metadata(
        &worktrees,
        &statuses,
        &protected_branches,
        &local_only_branches,
    );

    if cli.dry_run {
        print_switcher_preview(&model);
        return Ok(());
    }

    let agent_config = &config.agent;
    let discovery = AgentDiscovery::with_max_history_sessions(
        agent_monitored_paths(&git_repo)?,
        agent_config.max_activities,
    );
    let mut warp_tui = WarpTui::new(
        model,
        discovery,
        agent_config.refresh_rate,
        agent_config.enabled,
    );

    match warp_tui.run()? {
        Some(WorktreeSwitchAction::Switch(target)) => run_switcher_target(cli, target),
        Some(WorktreeSwitchAction::Remove(target)) => run_switcher_remove(target),
        Some(WorktreeSwitchAction::RemoveMany(batch)) => run_switcher_batch_remove(batch),
        None => {
            println!("No worktree selected");
            Ok(())
        }
    }
}

pub fn run(
    cli: &Cli,
    branch: Option<&str>,
    path: Option<&str>,
    latest: bool,
    waiting: bool,
    no_cow: bool,
) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::git::GitRepository;
    use crate::post_create::{PostCreateSetupStatus, run_post_create_setup};

    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let branch = resolve_switch_branch(&git_repo, branch, latest, waiting)?;

    info!("Switching to branch: {}", branch);

    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();

    let worktrees_for_classification = git_repo.list_worktrees()?;
    let branch_source = git_repo.classify_branch_source(&branch, &worktrees_for_classification)?;

    let worktree_path = if let Some(path) = path {
        PathBuf::from(path)
    } else if let crate::git::BranchSource::ExistingWorktree {
        path: existing_path,
    } = &branch_source
    {
        existing_path.clone()
    } else {
        git_repo.get_worktree_path_with_base(&branch, config.worktrees_path.as_deref())
    };

    if cli.dry_run {
        println!(
            "Would switch to branch '{}' at path: {}",
            branch,
            worktree_path.display()
        );
        println!("{}", dry_run_source_label(&branch, &branch_source));
        if worktree_path.exists() {
            println!("Would reuse existing worktree");
        } else if compute_use_cow(no_cow, config, &worktree_path) {
            println!("Would use Copy-on-Write for fast worktree creation");
        } else {
            println!("Would use traditional Git worktree creation");
        }
        return Ok(());
    }

    let mut report = SwitchOutcomeReport::new(worktree_path.clone());
    let mut worktree_created = false;
    // The CoW path no longer re-checks-out the branch (the linked worktree is
    // created on the right branch up front), so no checkout warning arises.
    let checkout_warning: Option<String> = None;

    if worktree_path.exists() {
        println!("📁 Worktree already exists at: {}", worktree_path.display());
        report.skipped("Worktree creation", "already existed");
    } else {
        println!("{}", source_announcement(&branch, &branch_source));

        let use_cow = compute_use_cow(no_cow, config, &worktree_path);

        if use_cow {
            println!("⚡ Using Copy-on-Write for instant setup...");

            // Create the real linked worktree first. `git worktree add`
            // produces the correct `.git` gitfile and checks out the branch;
            // CoW only needs to add the files git can't reproduce.
            create_worktree_for_source_with_recovery(
                &git_repo,
                &branch,
                &worktree_path,
                &branch_source,
            )?;

            // Overlay the primary worktree's untracked & ignored files
            // (node_modules, .env, local caches) via CoW — never `.git` or
            // tracked files. If the overlay does nothing the worktree is
            // still a fully valid linked worktree.
            if let Err(e) = cow_overlay_untracked(&git_repo, &worktree_path, &config.cow.exclude) {
                log::warn!("CoW overlay skipped ({e}); worktree is still usable");
            }
        } else {
            println!("📦 Using traditional Git worktree creation...");
            create_worktree_for_source_with_recovery(
                &git_repo,
                &branch,
                &worktree_path,
                &branch_source,
            )?;
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

    record_branch_checkout(&mut report, &worktree_path, &branch, checkout_warning);

    match run_post_create_setup(
        &worktree_path,
        worktree_created,
        config.post_create.auto_install,
    ) {
        PostCreateSetupStatus::Installed(manager) => {
            println!(
                "📦 Detected {} repo, ran `{}`",
                manager.binary(),
                manager.install_label()
            );
        }
        PostCreateSetupStatus::Warned { manager, reason } => {
            println!(
                "⚠️  Detected {} repo but `{}` failed: {}",
                manager.binary(),
                manager.install_label(),
                reason
            );
        }
        PostCreateSetupStatus::SkippedExistingWorktree
        | PostCreateSetupStatus::SkippedDisabled
        | PostCreateSetupStatus::SkippedNoLockfile => {}
    }

    let terminal_mode = resolve_terminal_mode(cli, &config.terminal_mode)?;

    record_terminal_handoff(
        &mut report,
        &worktree_path,
        terminal_mode,
        config.terminal.app.as_str(),
        &config.terminal,
    );
    report.finish();

    Ok(())
}

fn resolve_switch_branch(
    git_repo: &crate::git::GitRepository,
    branch: Option<&str>,
    latest: bool,
    waiting: bool,
) -> Result<String> {
    let selector_count = usize::from(branch.is_some()) + usize::from(latest) + usize::from(waiting);
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
        agent_monitored_paths(git_repo)?,
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
