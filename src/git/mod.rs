use crate::error::{GitWarpError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

mod branches;
mod cleanup;
mod head;
mod paths;
mod remotes;
mod types;
mod worktrees;

#[allow(unused_imports)]
pub use branches::sanitize_branch_name;
pub use types::{
    BranchSource, BranchStatus, CleanupAnalysis, CleanupSkip, CleanupSkipReason, WorktreeInfo,
};
pub use worktrees::list_untracked_and_ignored;

pub struct GitRepository {
    repo_path: PathBuf,
}

impl GitRepository {
    /// Find and open the Git repository
    pub fn find() -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        Self::resolve_toplevel(&current_dir)
    }

    /// Open a specific Git repository
    #[allow(dead_code)] // Public constructor used by inline tests/embedders.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::resolve_toplevel(path.as_ref())
    }

    fn resolve_toplevel(start_dir: &Path) -> Result<Self> {
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(start_dir)
            .output()
            .map_err(|_| GitWarpError::NotInGitRepository)?;

        if !output.status.success() {
            return Err(GitWarpError::NotInGitRepository.into());
        }

        let toplevel = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if toplevel.is_empty() {
            return Err(GitWarpError::NotInGitRepository.into());
        }

        Ok(Self {
            repo_path: PathBuf::from(toplevel),
        })
    }

    /// Get the repository root path
    pub fn root_path(&self) -> &Path {
        &self.repo_path
    }

    /// Get the repository name (base name of the root path)
    pub fn repo_name(&self) -> String {
        self.repo_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::GitRepository;
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
