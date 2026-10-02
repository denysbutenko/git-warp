use anyhow::Result;
use std::path::Path;

use crate::config::Config;
use crate::git::{BranchSource, GitRepository};

pub(super) fn create_worktree_for_source_with_recovery(
    git_repo: &GitRepository,
    branch: &str,
    worktree_path: &Path,
    source: &BranchSource,
) -> Result<()> {
    git_repo
        .create_worktree_for_source(branch, worktree_path, source)
        .map_err(|error| {
            anyhow::anyhow!(
                "{error}. Use a different branch name or run `warp ls` to inspect existing worktrees."
            )
        })
}

pub(super) fn source_announcement(branch: &str, source: &BranchSource) -> String {
    match source {
        BranchSource::ExistingWorktree { path } => format!(
            "🔁 Reusing existing worktree for branch '{}' at {}",
            branch,
            path.display()
        ),
        BranchSource::LocalBranch => {
            format!("🌱 Creating worktree for local branch '{}'", branch)
        }
        BranchSource::RemoteBranch { remote_ref } => format!(
            "🌐 Creating worktree from remote branch '{}' (new local '{}' tracking it)",
            remote_ref, branch
        ),
        BranchSource::CommitIsh { sha } => format!(
            "🔖 Creating worktree at commit '{}' (new local branch '{}')",
            &sha[..7],
            branch
        ),
        BranchSource::NewBranch => {
            format!("✨ Creating new branch '{}' from HEAD", branch)
        }
    }
}

pub(super) fn dry_run_source_label(branch: &str, source: &BranchSource) -> String {
    match source {
        BranchSource::ExistingWorktree { path } => {
            format!("Source: existing worktree at {}", path.display())
        }
        BranchSource::LocalBranch => {
            format!("Source: local branch '{}'", branch)
        }
        BranchSource::RemoteBranch { remote_ref } => format!(
            "Source: remote branch '{}' (would create local '{}' tracking it)",
            remote_ref, branch
        ),
        BranchSource::CommitIsh { sha } => {
            format!(
                "Source: commit-ish '{}' (would create local branch '{}')",
                &sha[..7],
                branch
            )
        }
        BranchSource::NewBranch => {
            format!("Source: new branch '{}' from HEAD", branch)
        }
    }
}

/// Decide whether a switch should use CoW. Kept out of `run` so the dry-run
/// preview and the real run stay in lockstep.
pub(super) fn compute_use_cow(no_cow: bool, cfg: &Config, path: &Path) -> bool {
    !no_cow && cfg.use_cow && crate::cow::is_cow_supported(path).unwrap_or(false)
}
