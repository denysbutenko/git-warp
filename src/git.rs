use crate::config::GitConfig;
use crate::error::{GitWarpError, Result};
use gix::Repository;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    pub branch: String,
    pub head: String,
    pub is_primary: bool,
    pub is_current: bool,
    pub is_detached: bool,
}

#[derive(Debug, Clone)]
pub struct BranchStatus {
    pub branch: String,
    pub path: PathBuf,
    pub has_remote: bool,
    pub is_merged: bool,
    pub is_identical: bool,
    pub has_uncommitted_changes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupSkipReason {
    PrimaryWorktree,
    BaseBranch,
    ProtectedBranch,
    DetachedHead,
    EmptyBranch,
}

impl CleanupSkipReason {
    pub fn label(&self) -> &'static str {
        match self {
            CleanupSkipReason::PrimaryWorktree => "primary worktree",
            CleanupSkipReason::BaseBranch => "base branch",
            CleanupSkipReason::ProtectedBranch => "protected branch",
            CleanupSkipReason::DetachedHead => "detached HEAD",
            CleanupSkipReason::EmptyBranch => "no branch checked out",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CleanupSkip {
    pub branch_label: String,
    pub path: PathBuf,
    pub reason: CleanupSkipReason,
}

#[derive(Debug, Clone)]
pub struct CleanupAnalysis {
    pub candidates: Vec<BranchStatus>,
    pub skipped: Vec<CleanupSkip>,
    pub base_branch: String,
}

/// Where the branch for `warp switch <branch>` comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchSource {
    /// Branch is already checked out in another worktree, which will be reused.
    ExistingWorktree { path: PathBuf },
    /// Local branch exists, no worktree yet.
    LocalBranch,
    /// Branch only exists on the remote; a local tracking branch will be created.
    RemoteBranch { remote_ref: String },
    /// Target is a commit-ish (tag, SHA) that isn't a local or remote branch;
    /// a local branch will be created at this commit.
    CommitIsh { sha: String },
    /// Neither local nor remote branch exists; a brand new branch will be created from HEAD.
    NewBranch,
}

pub struct GitRepository {
    #[allow(dead_code)] // Held to keep gix repository state alive; commands shell out to `git`.
    repo: Repository,
    repo_path: PathBuf,
}

fn is_protected_branch(branch: &str, protected_branches: &[String]) -> bool {
    protected_branches
        .iter()
        .any(|protected_branch| protected_branch.trim() == branch)
}

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
    /// Find and open the Git repository
    pub fn find() -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let repo = gix::discover(current_dir).map_err(|_| GitWarpError::NotInGitRepository)?;

        let repo_path = repo
            .work_dir()
            .ok_or(GitWarpError::NotInGitRepository)?
            .to_path_buf();

        Ok(Self { repo, repo_path })
    }

    /// Open a specific Git repository
    #[allow(dead_code)] // Public constructor used by inline tests/embedders.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo_path = path.as_ref().to_path_buf();
        let repo = gix::open(&repo_path).map_err(|_| GitWarpError::NotInGitRepository)?;

        Ok(Self { repo, repo_path })
    }

    /// Get the repository root path
    pub fn root_path(&self) -> &Path {
        &self.repo_path
    }

    /// List all worktrees
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        use std::process::Command;

        // Use git command to list worktrees since gix doesn't have full worktree support yet
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list worktrees: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git worktree list failed"));
        }

        let output_str = String::from_utf8_lossy(&output.stdout);
        let mut worktrees = Vec::new();
        let mut current_worktree: Option<WorktreeInfo> = None;

        for line in output_str.lines() {
            if line.starts_with("worktree ") {
                // Save previous worktree if exists
                if let Some(wt) = current_worktree.take() {
                    worktrees.push(wt);
                }

                let path = line.strip_prefix("worktree ").unwrap_or("");
                current_worktree = Some(WorktreeInfo {
                    path: PathBuf::from(path),
                    branch: String::new(),
                    head: String::new(),
                    is_primary: false,
                    is_current: false,
                    is_detached: false,
                });
            } else if line.starts_with("HEAD ") {
                if let Some(ref mut wt) = current_worktree {
                    wt.head = line.strip_prefix("HEAD ").unwrap_or("").to_string();
                }
            } else if line.starts_with("branch refs/heads/") {
                if let Some(ref mut wt) = current_worktree {
                    wt.branch = line
                        .strip_prefix("branch refs/heads/")
                        .unwrap_or("")
                        .to_string();
                }
            } else if line == "bare" {
                if let Some(ref mut wt) = current_worktree {
                    wt.is_primary = true;
                }
            } else if line == "detached"
                && let Some(ref mut wt) = current_worktree
            {
                wt.is_detached = true;
            }
        }

        // Add the last worktree
        if let Some(wt) = current_worktree {
            worktrees.push(wt);
        }

        let current_root = self
            .repo_path
            .canonicalize()
            .unwrap_or_else(|_| self.repo_path.clone());

        if let Some(first_worktree) = worktrees.first_mut() {
            first_worktree.is_primary = true;
        }

        for worktree in &mut worktrees {
            let worktree_path = worktree
                .path
                .canonicalize()
                .unwrap_or_else(|_| worktree.path.clone());
            worktree.is_current = worktree_path == current_root;

            if worktree.branch.is_empty() {
                worktree.is_detached = true;
            }
        }

        Ok(worktrees)
    }

    /// Classify how a branch should be materialized for `warp switch`.
    pub fn classify_branch_source(
        &self,
        branch_name: &str,
        worktrees: &[WorktreeInfo],
    ) -> Result<BranchSource> {
        if let Some(existing) = worktrees.iter().find(|wt| wt.branch == branch_name) {
            return Ok(BranchSource::ExistingWorktree {
                path: existing.path.clone(),
            });
        }
        if self.branch_exists(branch_name)? {
            return Ok(BranchSource::LocalBranch);
        }
        if let Some(remote_ref) = self.find_remote_branch_ref(branch_name)? {
            return Ok(BranchSource::RemoteBranch { remote_ref });
        }
        if let Some(sha) = self.resolve_commit_ish(branch_name)? {
            return Ok(BranchSource::CommitIsh { sha });
        }
        Ok(BranchSource::NewBranch)
    }

    /// Resolve a name to a commit SHA if it is a valid commit-ish (tag, SHA, etc.)
    pub fn resolve_commit_ish(&self, name: &str) -> Result<Option<String>> {
        use std::process::Command;

        let output = Command::new("git")
            .args([
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{}^{{commit}}", name),
            ])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to resolve commit-ish: {}", e))?;

        if output.status.success() {
            Ok(Some(
                String::from_utf8_lossy(&output.stdout).trim().to_string(),
            ))
        } else {
            Ok(None)
        }
    }

    /// Locate the first remote that carries `branch_name`, returning the short ref
    /// (e.g. `origin/feature`). `origin` is preferred when present.
    pub fn find_remote_branch_ref(&self, branch_name: &str) -> Result<Option<String>> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["for-each-ref", "--format=%(refname:short)", "refs/remotes"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list remote branches: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to list remote branches: {}", error));
        }

        let mut origin_match: Option<String> = None;
        let mut other_match: Option<String> = None;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.ends_with("/HEAD") {
                continue;
            }
            let Some((remote, branch)) = trimmed.split_once('/') else {
                continue;
            };
            if branch != branch_name {
                continue;
            }
            if remote == "origin" {
                origin_match = Some(trimmed.to_string());
                break;
            }
            if other_match.is_none() {
                other_match = Some(trimmed.to_string());
            }
        }

        Ok(origin_match.or(other_match))
    }

    /// Return true if a remote tracks a branch with this name on any remote.
    pub fn remote_branch_exists(&self, branch_name: &str) -> Result<bool> {
        Ok(self.find_remote_branch_ref(branch_name)?.is_some())
    }

    /// Return true if at least one remote is configured for this repository.
    pub fn has_remotes(&self) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["remote"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list remotes: {}", e))?;

        if !output.status.success() {
            return Ok(false);
        }

        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| !line.trim().is_empty()))
    }

    /// Create a new worktree and branch using a precomputed `BranchSource` classification.
    pub fn create_worktree_for_source<P: AsRef<Path>>(
        &self,
        branch_name: &str,
        worktree_path: P,
        source: &BranchSource,
    ) -> Result<()> {
        use std::process::Command;

        let worktree_path = worktree_path.as_ref();

        match source {
            BranchSource::ExistingWorktree { .. } => Err(anyhow::anyhow!(
                "Cannot create worktree for branch '{branch_name}': it is already checked out in another worktree"
            )),
            BranchSource::LocalBranch => {
                let output = Command::new("git")
                    .args(["worktree", "add"])
                    .arg(worktree_path)
                    .arg(branch_name)
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| anyhow::anyhow!("Failed to create worktree: {}", e))?;

                if !output.status.success() {
                    let error = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow::anyhow!("Failed to create worktree: {}", error));
                }
                Ok(())
            }
            BranchSource::RemoteBranch { remote_ref } => {
                let output = Command::new("git")
                    .args(["worktree", "add", "-b", branch_name])
                    .arg(worktree_path)
                    .arg(remote_ref)
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to create worktree from remote branch: {}", e)
                    })?;

                if !output.status.success() {
                    let error = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow::anyhow!(
                        "Failed to create worktree from remote branch '{remote_ref}': {}",
                        error
                    ));
                }
                Ok(())
            }
            BranchSource::CommitIsh { sha } => {
                let output = Command::new("git")
                    .args(["worktree", "add", "-b", branch_name])
                    .arg(worktree_path)
                    .arg(sha)
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| {
                        anyhow::anyhow!("Failed to create worktree from commit-ish: {}", e)
                    })?;

                if !output.status.success() {
                    let error = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow::anyhow!(
                        "Failed to create worktree from commit-ish: {}",
                        error
                    ));
                }
                Ok(())
            }
            BranchSource::NewBranch => {
                let output = Command::new("git")
                    .args(["worktree", "add", "-b", branch_name])
                    .arg(worktree_path)
                    .arg("HEAD")
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| anyhow::anyhow!("Failed to create worktree and branch: {}", e))?;

                if !output.status.success() {
                    let error = String::from_utf8_lossy(&output.stderr);
                    return Err(anyhow::anyhow!(
                        "Failed to create worktree and branch: {}",
                        error
                    ));
                }
                Ok(())
            }
        }
    }

    /// Create a new worktree and branch
    #[allow(dead_code)] // Used by integration tests/benches; bin path uses other helpers today.
    pub fn create_worktree_and_branch<P: AsRef<Path>>(
        &self,
        branch_name: &str,
        worktree_path: P,
        from_commit: Option<&str>,
    ) -> Result<()> {
        use std::process::Command;

        let worktree_path = worktree_path.as_ref();

        // Check if branch already exists
        if self.branch_exists(branch_name)? {
            // Create worktree from existing branch
            let mut cmd = Command::new("git");
            cmd.args(["worktree", "add"])
                .arg(worktree_path)
                .arg(branch_name)
                .current_dir(&self.repo_path);

            let output = cmd
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to create worktree: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to create worktree: {}", error));
            }
        } else if let Some(remote_ref) = self.find_remote_branch_ref(branch_name)? {
            // Track remote branch as new local branch
            let mut cmd = Command::new("git");
            cmd.args(["worktree", "add", "-b", branch_name])
                .arg(worktree_path)
                .arg(&remote_ref)
                .current_dir(&self.repo_path);

            let output = cmd.output().map_err(|e| {
                anyhow::anyhow!("Failed to create worktree from remote branch: {}", e)
            })?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!(
                    "Failed to create worktree from remote branch '{remote_ref}': {}",
                    error
                ));
            }
        } else {
            // Create new branch and worktree
            let mut cmd = Command::new("git");
            cmd.args(["worktree", "add", "-b", branch_name])
                .arg(worktree_path);

            if let Some(commit) = from_commit {
                cmd.arg(commit);
            } else {
                cmd.arg("HEAD");
            }

            cmd.current_dir(&self.repo_path);

            let output = cmd
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to create worktree and branch: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!(
                    "Failed to create worktree and branch: {}",
                    error
                ));
            }
        }

        Ok(())
    }

    /// Remove a worktree. Pass `force = true` to override git's refusal when
    /// the worktree has uncommitted changes or is locked.
    pub fn remove_worktree<P: AsRef<Path>>(&self, worktree_path: P, force: bool) -> Result<()> {
        use std::process::Command;

        let worktree_path = worktree_path.as_ref();

        let mut cmd = Command::new("git");
        cmd.args(["worktree", "remove"]);
        if force {
            cmd.arg("--force");
        }
        cmd.arg(worktree_path).current_dir(&self.repo_path);

        let output = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to remove worktree: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to remove worktree: {}", error));
        }

        Ok(())
    }

    /// Delete a local branch
    pub fn delete_branch(&self, branch_name: &str, force: bool) -> Result<()> {
        use std::process::Command;

        let delete_flag = if force { "-D" } else { "-d" };

        let output = Command::new("git")
            .args(["branch", delete_flag, branch_name])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to delete branch: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!(
                "Failed to delete branch {}: {}",
                branch_name,
                error
            ));
        }

        Ok(())
    }

    /// Prune worktrees (clean up stale references)
    pub fn prune_worktrees(&self) -> Result<()> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to prune worktrees: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to prune worktrees: {}", error));
        }

        Ok(())
    }

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
                        branch,
                        cleanup_base_branch_ref,
                    ])
                    .current_dir(repo_path)
                    .output()
                    .map(|output| output.status.success())
                    .unwrap_or(false);

                let is_identical = Command::new("git")
                    .args(["diff", "--quiet", cleanup_base_branch_ref, branch])
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

    fn load_branch_remotes(&self) -> Result<HashSet<String>> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["config", "--get-regexp", r"^branch\..*\.remote$"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to load branch remotes: {}", e))?;

        if !output.status.success() {
            return Ok(HashSet::new());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut branches = HashSet::new();
        for line in stdout.lines() {
            let key = match line.split_once(' ') {
                Some((key, _value)) => key,
                None => continue,
            };
            let branch = key
                .strip_prefix("branch.")
                .and_then(|rest| rest.strip_suffix(".remote"));
            if let Some(branch) = branch
                && !branch.is_empty()
            {
                branches.insert(branch.to_string());
            }
        }
        Ok(branches)
    }

    fn remote_default_branch(&self) -> Result<Option<String>> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to inspect remote default branch: {}", e))?;

        if !output.status.success() {
            return Ok(None);
        }

        let branch_ref = String::from_utf8_lossy(&output.stdout);
        Ok(branch_ref
            .trim()
            .strip_prefix("refs/remotes/origin/")
            .map(str::to_string))
    }

    /// Fetch from remote
    pub fn fetch_branches(&self) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["fetch", "--all", "--prune"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to fetch: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            log::warn!("Git fetch failed: {}", error);
            return Ok(false);
        }

        Ok(true)
    }

    /// Check if a branch exists
    pub fn branch_exists(&self, branch_name: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", branch_name),
            ])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check branch existence: {}", e))?;

        Ok(output.status.success())
    }

    /// List local branches matching a prefix for shell completion
    pub fn list_local_branches_matching_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["for-each-ref", "--format=%(refname:short)", "refs/heads"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list local branches: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to list local branches: {}", error));
        }

        let mut branches: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|branch| branch.starts_with(prefix))
            .map(str::to_string)
            .collect();
        branches.sort();
        branches.dedup();

        Ok(branches)
    }

    /// Get the current HEAD commit
    #[allow(dead_code)] // Public helper kept for tests/embedders.
    pub fn get_head_commit(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to get HEAD commit: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to get HEAD commit: {}", error));
        }

        let commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(commit_hash)
    }

    /// Get the default worktree path for a branch
    #[allow(dead_code)] // Convenience wrapper used by tests; bin path passes a base.
    pub fn get_worktree_path(&self, branch_name: &str) -> PathBuf {
        self.get_worktree_path_with_base(branch_name, None)
    }

    /// Get the worktree path for a branch using an optional custom base path
    pub fn get_worktree_path_with_base(
        &self,
        branch_name: &str,
        worktrees_path: Option<&Path>,
    ) -> PathBuf {
        let sanitized_branch = branch_name.trim_matches('/').replace(['/', '\\'], "-");

        let base_path = match worktrees_path {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => self.primary_worktree_root().join(path),
            None => self.primary_worktree_root().join("../worktrees"),
        };

        base_path.join(sanitized_branch)
    }

    fn primary_worktree_root(&self) -> PathBuf {
        self.list_worktrees()
            .ok()
            .and_then(|worktrees| {
                worktrees
                    .into_iter()
                    .find(|worktree| worktree.is_primary)
                    .map(|worktree| worktree.path)
            })
            .unwrap_or_else(|| self.repo_path.clone())
    }

    /// Get the main branch name. Prefers `origin/HEAD`, falls back to `main`.
    pub fn get_main_branch(&self) -> Result<String> {
        use std::process::Command;

        // Try to get the default branch from remote
        let output = Command::new("git")
            .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(&self.repo_path)
            .output();

        if let Ok(output) = output
            && output.status.success()
        {
            let branch_ref = String::from_utf8_lossy(&output.stdout);
            if let Some(branch) = branch_ref.trim().strip_prefix("refs/remotes/origin/") {
                return Ok(branch.to_string());
            }
        }

        Ok("main".to_string())
    }

    /// Check if a directory has uncommitted changes
    pub fn has_uncommitted_changes<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(path.as_ref())
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check git status: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Git status failed: {}", error));
        }

        Ok(!output.stdout.is_empty())
    }

    /// Check if a branch is merged into a target branch
    #[allow(dead_code)] // Public helper kept for tests/embedders.
    pub fn is_branch_merged(&self, branch: &str, target_branch: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["merge-base", "--is-ancestor", branch, target_branch])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check merge status: {}", e))?;

        Ok(output.status.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_git_repo_operations() {
        // Create a temporary git repository for testing
        let temp_dir = tempdir().unwrap();
        let repo_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure git
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("test.txt"), "test").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test opening the repository
        let git_repo = GitRepository::open(repo_path);
        assert!(git_repo.is_ok());
    }
}
