use super::branches::is_protected_branch;
use super::{
    BranchStatus, CleanupAnalysis, CleanupSkip, CleanupSkipReason, GitRepository, WorktreeInfo,
};
use crate::config::GitConfig;
use crate::error::Result;
use rayon::prelude::*;
use std::path::Path;

fn cleanup_skip_reason(
    worktree: &WorktreeInfo,
    cleanup_base_branch: &str,
    protected_branches: &[String],
) -> Option<CleanupSkipReason> {
    if worktree.is_primary {
        return Some(CleanupSkipReason::PrimaryWorktree);
    }
    if worktree.is_detached {
        return Some(CleanupSkipReason::DetachedHead);
    }
    if worktree.branch.is_empty() {
        return Some(CleanupSkipReason::EmptyBranch);
    }
    if worktree.branch == cleanup_base_branch {
        return Some(CleanupSkipReason::BaseBranch);
    }
    if is_protected_branch(&worktree.branch, protected_branches) {
        return Some(CleanupSkipReason::ProtectedBranch);
    }
    None
}

impl GitRepository {
    /// Analyze branches for cleanup
    #[allow(dead_code)] // Default-config wrapper used by tests/benches.
    pub fn analyze_branches_for_cleanup(
        &self,
        worktrees: &[WorktreeInfo],
    ) -> Result<Vec<BranchStatus>> {
        let config = GitConfig::default();
        self.analyze_branches_for_cleanup_with_config(worktrees, &config)
    }

    /// Analyze branches for cleanup using an explicit protected branch list
    #[allow(dead_code)] // Convenience wrapper used by tests; bin uses *_with_config.
    pub fn analyze_branches_for_cleanup_with_protected_branches(
        &self,
        worktrees: &[WorktreeInfo],
        protected_branches: &[String],
    ) -> Result<Vec<BranchStatus>> {
        Ok(self
            .analyze_worktrees_for_cleanup_with_options(
                worktrees,
                protected_branches,
                &GitConfig::default().default_branch,
            )?
            .candidates)
    }

    /// Analyze branches for cleanup using full git configuration
    pub fn analyze_branches_for_cleanup_with_config(
        &self,
        worktrees: &[WorktreeInfo],
        config: &GitConfig,
    ) -> Result<Vec<BranchStatus>> {
        Ok(self
            .analyze_worktrees_for_cleanup_with_config(worktrees, config)?
            .candidates)
    }

    /// Analyze worktrees for cleanup, returning both eligible candidates and
    /// skipped worktrees with the reason each was filtered out.
    pub fn analyze_worktrees_for_cleanup_with_config(
        &self,
        worktrees: &[WorktreeInfo],
        config: &GitConfig,
    ) -> Result<CleanupAnalysis> {
        self.analyze_worktrees_for_cleanup_with_options(
            worktrees,
            &config.protected_branches,
            &config.default_branch,
        )
    }

    /// Resolve the branch cleanup compares candidates against
    pub fn cleanup_base_branch(
        &self,
        worktrees: &[WorktreeInfo],
        configured_default_branch: &str,
    ) -> Result<String> {
        if let Some(remote_default) = self.remote_default_branch()? {
            return Ok(remote_default);
        }

        if let Some(primary_branch) = worktrees
            .iter()
            .find(|worktree| worktree.is_primary && !worktree.branch.trim().is_empty())
            .map(|worktree| worktree.branch.clone())
        {
            return Ok(primary_branch);
        }

        let configured_default_branch = configured_default_branch.trim();
        if !configured_default_branch.is_empty() {
            return Ok(configured_default_branch.to_string());
        }

        self.get_main_branch()
    }

    fn analyze_worktrees_for_cleanup_with_options(
        &self,
        worktrees: &[WorktreeInfo],
        protected_branches: &[String],
        configured_default_branch: &str,
    ) -> Result<CleanupAnalysis> {
        use std::process::Command;

        enum CleanupEntry {
            Candidate(BranchStatus),
            Skip(CleanupSkip),
        }

        let cleanup_base_branch = self.cleanup_base_branch(worktrees, configured_default_branch)?;
        let remotes = self.load_branch_remotes()?;
        let repo_path: &Path = &self.repo_path;
        let cleanup_base_branch_ref: &str = &cleanup_base_branch;
        let remotes_ref = &remotes;

        let entries: Vec<CleanupEntry> = worktrees
            .par_iter()
            .map(|worktree| {
                if let Some(reason) =
                    cleanup_skip_reason(worktree, cleanup_base_branch_ref, protected_branches)
                {
                    let branch_label = if worktree.branch.is_empty() {
                        if worktree.is_detached {
                            format!(
                                "(detached {})",
                                worktree.head.chars().take(7).collect::<String>()
                            )
                        } else {
                            "(no branch)".to_string()
                        }
                    } else {
                        worktree.branch.clone()
                    };
                    return CleanupEntry::Skip(CleanupSkip {
                        branch_label,
                        path: worktree.path.clone(),
                        reason,
                    });
                }

                let branch = &worktree.branch;
                let path = &worktree.path;

                let has_remote = remotes_ref.contains(branch);

                let is_merged = Command::new("git")
                    .args([
                        "merge-base",
                        "--is-ancestor",
                        "--end-of-options",
                        branch,
                        cleanup_base_branch_ref,
                    ])
                    .current_dir(repo_path)
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);

                let is_identical = Command::new("git")
                    .args([
                        "diff",
                        "--quiet",
                        "--end-of-options",
                        cleanup_base_branch_ref,
                        branch,
                        "--",
                    ])
                    .current_dir(repo_path)
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                let has_uncommitted_changes = Command::new("git")
                    .args(["status", "--porcelain"])
                    .current_dir(path)
                    .output()
                    .map(|o| !o.stdout.is_empty())
                    .unwrap_or(false);

                CleanupEntry::Candidate(BranchStatus {
                    branch: branch.clone(),
                    path: path.clone(),
                    has_remote,
                    is_merged,
                    is_identical,
                    has_uncommitted_changes,
                })
            })
            .collect();

        let mut candidates = Vec::new();
        let mut skipped = Vec::new();
        for entry in entries {
            match entry {
                CleanupEntry::Candidate(c) => candidates.push(c),
                CleanupEntry::Skip(s) => skipped.push(s),
            }
        }

        Ok(CleanupAnalysis {
            candidates,
            skipped,
            base_branch: cleanup_base_branch,
        })
    }
}
