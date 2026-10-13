use super::{BranchSource, GitRepository, WorktreeInfo};
use crate::error::Result;

pub(super) fn is_protected_branch(branch: &str, protected_branches: &[String]) -> bool {
    protected_branches
        .iter()
        .any(|protected_branch| protected_branch.trim() == branch)
}

/// Sanitize a branch name for use as a filesystem directory name.
///
/// Replaces characters that are invalid or reserved on filesystems (especially Windows Win32):
/// `/`, `\`, `<`, `>`, `:`, `"`, `|`, `?`, `*`, and ASCII control characters.
/// Trims leading slashes, trailing dots and spaces, and provides a fallback name ("worktree")
/// if the resulting string is empty.
pub fn sanitize_branch_name(branch_name: &str) -> String {
    let trimmed = branch_name.trim_matches('/');
    let sanitized: String = trimmed
        .chars()
        .map(|c| match c {
            '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*' => '-',
            c if (c as u32) < 32 => '-',
            c => c,
        })
        .collect();

    let clean = sanitized.trim_end_matches(['.', ' ']);
    if clean.is_empty() {
        "worktree".to_string()
    } else {
        clean.to_string()
    }
}

impl GitRepository {
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
            .args(["rev-parse", "--verify", "--quiet", "--end-of-options"])
            .arg(format!("{}^{{commit}}", name))
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

    /// Check if a branch exists
    pub fn branch_exists(&self, branch_name: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args(["show-ref", "--verify", "--quiet", "--end-of-options"])
            .arg(format!("refs/heads/{}", branch_name))
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check branch existence: {}", e))?;

        Ok(output.status.success())
    }

    /// Delete a local branch
    pub fn delete_branch(&self, branch_name: &str, force: bool) -> Result<()> {
        use std::process::Command;

        let delete_flag = if force { "-D" } else { "-d" };

        let output = Command::new("git")
            .args(["branch", delete_flag, "--", branch_name])
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

    /// Check if a branch is merged into a target branch
    #[allow(dead_code)] // Public helper kept for tests/embedders.
    pub fn is_branch_merged(&self, branch: &str, target_branch: &str) -> Result<bool> {
        use std::process::Command;

        let output = Command::new("git")
            .args([
                "merge-base",
                "--is-ancestor",
                "--end-of-options",
                branch,
                target_branch,
            ])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to check merge status: {}", e))?;

        Ok(output.status.success())
    }

    /// List refs (local branches, remote-tracking branches, tags) that `warp switch`
    /// can resolve, matching a prefix for shell completion. Remote-tracking branches
    /// are exposed by their trailing name (`origin/foo` → `foo`) so users can complete
    /// a fetched-but-not-checked-out branch and let `warp switch` create the local
    /// tracking branch. Duplicates (same name local and on a remote) collapse into
    /// one suggestion; the symbolic `refs/remotes/<remote>/HEAD` alias is skipped.
    pub fn list_switchable_refs_matching_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        use std::collections::BTreeSet;
        use std::process::Command;

        let output = Command::new("git")
            .args([
                "for-each-ref",
                "--format=%(refname)",
                "refs/heads",
                "refs/remotes",
                "refs/tags",
            ])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list refs: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to list refs: {}", error));
        }

        let mut refs: BTreeSet<String> = BTreeSet::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let name = if let Some(rest) = line.strip_prefix("refs/heads/") {
                rest
            } else if let Some(rest) = line.strip_prefix("refs/remotes/") {
                let Some((_, branch)) = rest.split_once('/') else {
                    continue;
                };
                if branch == "HEAD" {
                    continue;
                }
                branch
            } else if let Some(rest) = line.strip_prefix("refs/tags/") {
                rest
            } else {
                continue;
            };

            if name.starts_with(prefix) {
                refs.insert(name.to_string());
            }
        }

        Ok(refs.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::GitRepository;
    use std::process::Command;
    use tempfile::tempdir;

    /// A user-controlled ref name that begins with `-` (e.g. `--upload-pack=…`)
    /// used to reach git as a positional and be mis-parsed as an option. Git's
    /// own ref-format rules forbid creating such a branch, so we cannot round-
    /// trip through a real branch here — instead assert the guard turns a
    /// leading-dash argument into a benign "unknown revision" outcome rather
    /// than "unknown option `--upload-pack=…`" (which would prove the argument
    /// was still being option-parsed by git).
    #[test]
    fn test_leading_dash_ref_is_not_parsed_as_git_option() {
        let temp = tempdir().unwrap();
        let repo_path = temp.path();

        for cmd in [
            &["init", "-q"][..],
            &["config", "user.email", "t@e.com"][..],
            &["config", "user.name", "T"][..],
        ] {
            Command::new("git")
                .args(cmd)
                .current_dir(repo_path)
                .output()
                .unwrap();
        }
        std::fs::write(repo_path.join("f.txt"), "x").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let git_repo = GitRepository::open(repo_path).unwrap();

        // A name git recognises as an option but that isn't a valid ref. With
        // no `--end-of-options` / `--` guard, `git rev-parse` / `show-ref` /
        // `merge-base` would fail with "unknown option" and the helper would
        // bubble that up as `Err`. With the guard, git parses the argument as
        // a ref, finds nothing, and the helper returns a clean negative.
        let evil = "--upload-pack=/tmp/git-warp-should-never-run";

        assert!(
            !git_repo
                .branch_exists(evil)
                .expect("branch_exists must not surface an option-parse error"),
            "branch_exists must return false for an unknown ref"
        );

        assert!(
            git_repo
                .resolve_commit_ish(evil)
                .expect("resolve_commit_ish must not surface an option-parse error")
                .is_none(),
            "resolve_commit_ish must return None for an unknown ref, not error"
        );

        assert!(
            !git_repo
                .is_branch_merged(evil, "HEAD")
                .expect("is_branch_merged must not surface an option-parse error"),
            "is_branch_merged must return false for an unknown ref"
        );

        // `delete_branch` on a non-existent branch legitimately errors ("branch
        // not found"), but the message must be about the ref, not about an
        // unknown option like `--upload-pack=…`.
        let delete_err = git_repo
            .delete_branch(evil, true)
            .expect_err("delete_branch on a missing ref should error")
            .to_string();
        assert!(
            !delete_err.contains("unknown option") && !delete_err.contains("unknown switch"),
            "delete_branch must not let `{evil}` reach git as an option: {delete_err}"
        );
    }
}
