use anyhow::{Result, anyhow};
use log::info;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::Cli;

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

fn create_worktree_for_source_with_recovery(
    git_repo: &crate::git::GitRepository,
    branch: &str,
    worktree_path: &Path,
    source: &crate::git::BranchSource,
) -> Result<()> {
    git_repo
        .create_worktree_for_source(branch, worktree_path, source)
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}. Use a different branch name or run `warp ls` to inspect existing worktrees."
            )
        })
}

/// CoW-overlay the primary worktree's untracked & ignored files onto a
/// freshly created linked worktree at `dest`.
///
/// Copies only what `git worktree add` cannot reproduce — build output and
/// local files such as `node_modules`, `.env`, and caches — and never
/// touches `.git`, tracked files, the worktree-storage directory, or names
/// listed in `exclude`. Absolute paths baked into the copied files are
/// rewritten to point at `dest`. Best-effort: a failure leaves the worktree
/// usable, just without the overlaid files.
fn cow_overlay_untracked(
    git_repo: &crate::git::GitRepository,
    dest: &Path,
    exclude: &[String],
) -> Result<()> {
    use crate::cow;
    use crate::rewrite::PathRewriter;

    let worktrees = git_repo.list_worktrees()?;
    let Some(primary) = worktrees.iter().find(|wt| wt.is_primary) else {
        return Ok(());
    };
    let primary_path = primary.path.clone();

    let entries = crate::git::list_untracked_and_ignored(&primary_path)?;
    if entries.is_empty() {
        return Ok(());
    }

    // The worktree-storage directory is the destination's parent: in a
    // nested layout (`worktrees_path` inside the repo) every sibling
    // worktree lives under it. Skipping any entry that resolves to the
    // destination's ancestors *or* into that storage directory keeps whole
    // worktrees from being copied back into `dest`. In the default sibling
    // layout the storage dir lives outside the primary, so nothing matches.
    let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    let storage = dest_canon.parent().map(std::path::Path::to_path_buf);

    let mut copied = Vec::new();
    for rel in entries {
        if !cow::should_overlay_entry(&rel, exclude) {
            continue;
        }
        let src = primary_path.join(&rel);
        // `symlink_metadata`, not `exists`: keep dangling untracked symlinks
        // (e.g. `.env` -> a not-yet-present target) that the worktree wants.
        if std::fs::symlink_metadata(&src).is_err() {
            continue;
        }
        let src_canon = src.canonicalize().unwrap_or_else(|_| src.clone());
        if dest_canon.starts_with(&src_canon)
            || storage
                .as_deref()
                .is_some_and(|base| src_canon.starts_with(base))
        {
            continue;
        }
        // `overlay_into` skips paths git already checked out and returns the
        // freshly created paths (for scoped rewriting), cleaning up on a
        // partial clone so no stale, un-rewritten copy is left behind.
        match cow::overlay_into(&src, &dest.join(&rel)) {
            Ok(mut created) => copied.append(&mut created),
            Err(e) => log::warn!("CoW overlay of {} failed: {}", rel.display(), e),
        }
    }

    if !copied.is_empty() {
        let rewriter = PathRewriter::new(&primary_path, dest);
        if let Err(e) = rewriter.rewrite_paths_under(&copied) {
            log::warn!("Path rewriting failed: {}", e);
        }
    }

    Ok(())
}

fn source_announcement(branch: &str, source: &crate::git::BranchSource) -> String {
    match source {
        crate::git::BranchSource::ExistingWorktree { path } => format!(
            "🔁 Reusing existing worktree for branch '{}' at {}",
            branch,
            path.display()
        ),
        crate::git::BranchSource::LocalBranch => {
            format!("🌱 Creating worktree for local branch '{}'", branch)
        }
        crate::git::BranchSource::RemoteBranch { remote_ref } => format!(
            "🌐 Creating worktree from remote branch '{}' (new local '{}' tracking it)",
            remote_ref, branch
        ),
        crate::git::BranchSource::CommitIsh { sha } => format!(
            "🔖 Creating worktree at commit '{}' (new local branch '{}')",
            &sha[..7],
            branch
        ),
        crate::git::BranchSource::NewBranch => {
            format!("✨ Creating new branch '{}' from HEAD", branch)
        }
    }
}

/// Decide whether a switch should use CoW. Kept out of `run` so the dry-run
/// preview and the real run stay in lockstep.
fn compute_use_cow(no_cow: bool, cfg: &crate::config::Config, path: &Path) -> bool {
    !no_cow && cfg.use_cow && crate::cow::is_cow_supported(path).unwrap_or(false)
}

fn dry_run_source_label(branch: &str, source: &crate::git::BranchSource) -> String {
    match source {
        crate::git::BranchSource::ExistingWorktree { path } => {
            format!("Source: existing worktree at {}", path.display())
        }
        crate::git::BranchSource::LocalBranch => {
            format!("Source: local branch '{}'", branch)
        }
        crate::git::BranchSource::RemoteBranch { remote_ref } => format!(
            "Source: remote branch '{}' (would create local '{}' tracking it)",
            remote_ref, branch
        ),
        crate::git::BranchSource::CommitIsh { sha } => {
            format!(
                "Source: commit-ish '{}' (would create local branch '{}')",
                &sha[..7],
                branch
            )
        }
        crate::git::BranchSource::NewBranch => {
            format!("Source: new branch '{}' from HEAD", branch)
        }
    }
}

pub fn run_default(cli: &Cli) -> Result<()> {
    use crate::agents::AgentDiscovery;
    use crate::config::ConfigManager;
    use crate::git::GitRepository;
    use crate::process::ProcessManager;
    use crate::tui::{WarpTui, WorktreeSwitchAction, build_worktree_switch_model_with_metadata};

    use super::util::{agent_monitored_paths, not_in_git_repo_error};

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

fn collect_local_only_branches(
    git_repo: &crate::git::GitRepository,
    worktrees: &[crate::git::WorktreeInfo],
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

fn collect_worktree_runtime_statuses(
    git_repo: &crate::git::GitRepository,
    worktrees: &[crate::git::WorktreeInfo],
    mut process_manager: crate::process::ProcessManager,
) -> Vec<crate::tui::WorktreeRuntimeStatus> {
    use super::util::worktree_last_touched;

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

fn run_switcher_target(cli: &Cli, target: crate::tui::WorktreeSwitchTarget) -> Result<()> {
    if let Some(branch) = target.branch.as_deref() {
        let path = target.path.to_string_lossy().into_owned();
        run(cli, Some(branch), Some(path.as_str()), false, false, false)
    } else {
        run_existing_worktree_jump(cli, &target.path)
    }
}

fn run_switcher_remove(target: crate::tui::WorktreeRemovalTarget) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::git::GitRepository;

    use super::util::{abbreviate_path, not_in_git_repo_error};

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

fn run_switcher_batch_remove(batch: crate::tui::WorktreeBatchRemoval) -> Result<()> {
    use crate::config::ConfigManager;
    use crate::git::GitRepository;

    use super::util::{abbreviate_path, not_in_git_repo_error};

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

fn run_existing_worktree_jump(cli: &Cli, worktree_path: &Path) -> Result<()> {
    use crate::config::ConfigManager;

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

    use super::util::not_in_git_repo_error;

    // Find the Git repository
    let git_repo = GitRepository::find().map_err(|_| not_in_git_repo_error())?;
    let branch = resolve_switch_branch(&git_repo, branch, latest, waiting)?;

    info!("Switching to branch: {}", branch);

    let config_manager = ConfigManager::new()?;
    let config = config_manager.get();

    let worktrees_for_classification = git_repo.list_worktrees()?;
    let branch_source = git_repo.classify_branch_source(&branch, &worktrees_for_classification)?;

    // Determine worktree path
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

    // Check if worktree already exists
    if worktree_path.exists() {
        println!("📁 Worktree already exists at: {}", worktree_path.display());
        report.skipped("Worktree creation", "already existed");
    } else {
        println!("{}", source_announcement(&branch, &branch_source));

        // Choose creation method based on CoW support and user preference
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

fn resolve_terminal_mode(cli: &Cli, config_mode: &str) -> Result<crate::terminal::TerminalMode> {
    use crate::terminal::TerminalMode;
    if let Some(mode) = cli.terminal {
        return Ok(mode);
    }
    TerminalMode::from_str(config_mode).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid terminal_mode '{}' in config. Supported modes: {}",
            config_mode,
            TerminalMode::SUPPORTED.join(", "),
        )
    })
}

fn record_branch_checkout(
    report: &mut SwitchOutcomeReport,
    worktree_path: &Path,
    branch: &str,
    checkout_warning: Option<String>,
) {
    match current_branch_at_path(worktree_path) {
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
    report: &mut SwitchOutcomeReport,
    worktree_path: &Path,
    terminal_mode: crate::terminal::TerminalMode,
    terminal_app: &str,
    terminal_config: &crate::config::TerminalConfig,
) {
    use crate::git::GitRepository;
    use crate::terminal::{TerminalLaunchOptions, TerminalManager};

    let branch = current_branch_at_path(worktree_path).unwrap_or_else(|_| "unknown".to_string());
    let repo = GitRepository::find()
        .map(|r| r.repo_name())
        .unwrap_or_else(|_| "unknown".to_string());

    let terminal_manager = TerminalManager;
    let launch_options = TerminalLaunchOptions {
        auto_activate: terminal_config.auto_activate,
        init_commands: terminal_config.init_commands.clone(),
        branch: Some(branch),
        repo: Some(repo),
    };

    match terminal_manager.switch_to_worktree_with_options(
        worktree_path,
        terminal_mode,
        None,
        Some(terminal_app),
        &launch_options,
    ) {
        Ok(()) => {
            report.done(
                "Terminal handoff",
                terminal_handoff_success_detail(terminal_mode),
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

fn terminal_handoff_success_detail(terminal_mode: crate::terminal::TerminalMode) -> &'static str {
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
    git_repo: &crate::git::GitRepository,
    branch: Option<&str>,
    latest: bool,
    waiting: bool,
) -> Result<String> {
    use super::util::agent_monitored_paths;

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
