use super::GitRepository;
use crate::error::Result;
use std::path::Path;

impl GitRepository {
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
}
