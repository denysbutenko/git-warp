use crate::config::GitConfig;
use crate::error::{GitWarpError, Result};
use gix::Repository;
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

pub struct GitRepository {
    repo: Repository,
    repo_path: PathBuf,
}

fn is_protected_branch(branch: &str, protected_branches: &[String]) -> bool {
    protected_branches
        .iter()
        .any(|protected_branch| protected_branch.trim() == branch)
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
            .args(&["worktree", "list", "--porcelain"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list worktrees: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git worktree list failed").into());
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
            } else if line == "detached" {
                if let Some(ref mut wt) = current_worktree {
                    wt.is_detached = true;
                }
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

    /// Create a new worktree and branch
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
            cmd.args(&["worktree", "add"])
                .arg(worktree_path)
                .arg(branch_name)
                .current_dir(&self.repo_path);

            let output = cmd
                .output()
                .map_err(|e| anyhow::anyhow!("Failed to create worktree: {}", e))?;

            if !output.status.success() {
                let error = String::from_utf8_lossy(&output.stderr);
                return Err(anyhow::anyhow!("Failed to create worktree: {}", error).into());
            }
        } else {
            // Create new branch and worktree
            let mut cmd = Command::new("git");
            cmd.args(&["worktree", "add", "-b", branch_name])
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
                return Err(
                    anyhow::anyhow!("Failed to create worktree and branch: {}", error).into(),
                );
            }
        }

        Ok(())
    }

    /// Remove a worktree
    pub fn remove_worktree<P: AsRef<Path>>(&self, worktree_path: P) -> Result<()> {
        use std::process::Command;

        let worktree_path = worktree_path.as_ref();

        // Remove the worktree using git
        let output = Command::new("git")
            .args(&["worktree", "remove"])
            .arg(worktree_path)
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to remove worktree: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to remove worktree: {}", error).into());
        }

        Ok(())
    }

    /// Delete a local branch
    pub fn delete_branch(&self, branch_name: &str, force: bool) -> Result<()> {
        use std::process::Command;

        let delete_flag = if force { "-D" } else { "-d" };

        let output = Command::new("git")
            .args(&["branch", delete_flag, branch_name])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to delete branch: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(
                anyhow::anyhow!("Failed to delete branch {}: {}", branch_name, error).into(),
            );
        }

        Ok(())
    }

    /// Prune worktrees (clean up stale references)
    pub fn prune_worktrees(&self) -> Result<()> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["worktree", "prune"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to prune worktrees: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to prune worktrees: {}", error).into());
        }

        Ok(())
    }

    /// Analyze branches for cleanup
    pub fn analyze_branches_for_cleanup(
        &self,
        worktrees: &[WorktreeInfo],
    ) -> Result<Vec<BranchStatus>> {
        let config = GitConfig::default();
        self.analyze_branches_for_cleanup_with_config(worktrees, &config)
    }

    /// Analyze branches for cleanup using an explicit protected branch list
    pub fn analyze_branches_for_cleanup_with_protected_branches(
        &self,
        worktrees: &[WorktreeInfo],
        protected_branches: &[String],
    ) -> Result<Vec<BranchStatus>> {
        self.analyze_branches_for_cleanup_with_options(
            worktrees,
            protected_branches,
            &GitConfig::default().default_branch,
        )
    }

    /// Analyze branches for cleanup using full git configuration
    pub fn analyze_branches_for_cleanup_with_config(
        &self,
        worktrees: &[WorktreeInfo],
        config: &GitConfig,
    ) -> Result<Vec<BranchStatus>> {
        self.analyze_branches_for_cleanup_with_options(
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

    fn analyze_branches_for_cleanup_with_options(
        &self,
        worktrees: &[WorktreeInfo],
        protected_branches: &[String],
        configured_default_branch: &str,
    ) -> Result<Vec<BranchStatus>> {
        use std::process::Command;

        let mut branch_statuses = Vec::new();
        let cleanup_base_branch = self.cleanup_base_branch(worktrees, configured_default_branch)?;

        for worktree in worktrees {
            if worktree.is_primary
                || worktree.branch.is_empty()
                || worktree.branch == cleanup_base_branch
                || is_protected_branch(&worktree.branch, protected_branches)
            {
                continue;
            }

            let branch = &worktree.branch;
            let path = &worktree.path;

            // Check if branch has a remote
            let has_remote = {
                let output = Command::new("git")
                    .args(&["config", &format!("branch.{}.remote", branch)])
                    .current_dir(&self.repo_path)
                    .output()
                    .map_err(|e| anyhow::anyhow!("Failed to check remote: {}", e))?;

                output.status.success() && !output.stdout.is_empty()
            };

            // Check if branch is merged to the repo's actual cleanup base branch
            let is_merged = Command::new("git")
                .args(["merge-base", "--is-ancestor", branch, &cleanup_base_branch])
                .current_dir(&self.repo_path)
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);

            // Check if branch is identical to the repo's actual cleanup base branch
            let is_identical = {
                let output = Command::new("git")
                    .args(["diff", "--quiet", &cleanup_base_branch, branch])
                    .current_dir(&self.repo_path)
                    .output();

                output.map(|o| o.status.success()).unwrap_or(false)
            };

            // Check for uncommitted changes
            let has_uncommitted_changes = {
                let output = Command::new("git")
                    .args(&["status", "--porcelain"])
                    .current_dir(path)
                    .output();

                output.map(|o| !o.stdout.is_empty()).unwrap_or(false)
            };

            branch_statuses.push(BranchStatus {
                branch: branch.clone(),
                path: path.clone(),
                has_remote,
                is_merged,
                is_identical,
                has_uncommitted_changes,
            });
        }

        Ok(branch_statuses)
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
            .args(&["fetch", "--all", "--prune"])
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
            .args(&[
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
            return Err(anyhow::anyhow!("Failed to list local branches: {}", error).into());
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
    pub fn get_head_commit(&self) -> Result<String> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["rev-parse", "HEAD"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to get HEAD commit: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to get HEAD commit: {}", error).into());
        }

        let commit_hash = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Ok(commit_hash)
    }

    /// Get the default worktree path for a branch
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
            Some(path) => self.repo_path.join(path),
            None => self.repo_path.join("../worktrees"),
        };

        base_path.join(sanitized_branch)
    }

    /// Get the main branch name (main or master)
    pub fn get_main_branch(&self) -> Result<String> {
        use std::process::Command;

        // Try to get the default branch from remote
        let output = Command::new("git")
            .args(&["symbolic-ref", "refs/remotes/origin/HEAD"])
            .current_dir(&self.repo_path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let branch_ref = String::from_utf8_lossy(&output.stdout);
                if let Some(branch) = branch_ref.trim().strip_prefix("refs/remotes/origin/") {
                    return Ok(branch.to_string());
                }
            }
        }

        // Fallback: check if main exists, otherwise use master
        if self.branch_exists("main")? {
            Ok("main".to_string())
        } else {
            Ok("master".to_string())
        }
    }

    /// Check if a directory has uncommitted changes
    pub fn has_uncommitted_changes<P: AsRef<Path>>(&self, path: P) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(path.as_ref())
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check git status: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Git status failed: {}", error).into());
        }

        Ok(!output.stdout.is_empty())
    }

    /// Check if a branch is merged into a target branch
    pub fn is_branch_merged(&self, branch: &str, target_branch: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(&["merge-base", "--is-ancestor", branch, target_branch])
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
            .args(&["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure git
        Command::new("git")
            .args(&["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Create initial commit
        std::fs::write(repo_path.join("test.txt"), "test").unwrap();
        Command::new("git")
            .args(&["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(&["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test opening the repository
        let git_repo = GitRepository::open(repo_path);
        assert!(git_repo.is_ok());
    }
}
