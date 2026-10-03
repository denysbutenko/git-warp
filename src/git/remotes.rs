use super::GitRepository;
use crate::error::Result;
use std::collections::HashSet;

impl GitRepository {
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

    pub(super) fn load_branch_remotes(&self) -> Result<HashSet<String>> {
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

    pub(super) fn remote_default_branch(&self) -> Result<Option<String>> {
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
}
