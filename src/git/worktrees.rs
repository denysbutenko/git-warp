use super::branches::sanitize_branch_name;
use super::paths::{bytes_to_path, git_compatible_path};
use super::{BranchSource, GitRepository, WorktreeInfo};
use crate::error::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

impl GitRepository {
    /// List all worktrees
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>> {
        let output = Command::new("git")
            .args(["worktree", "list", "--porcelain", "-z"])
            .current_dir(&self.repo_path)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to list worktrees: {}", e))?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Git worktree list failed"));
        }

        // With `-z`, each porcelain field is terminated by NUL and records are
        // separated by an empty field (`\0\0`). Paths in the `worktree <path>`
        // field arrive as raw bytes, so parse on the byte stream and use
        // `bytes_to_path` for the path — otherwise a legal newline in a path
        // would split one record across two, and non-UTF-8 bytes would round
        // to U+FFFD and mis-target later `remove_worktree` / `switch` calls.
        let mut worktrees = Vec::new();
        let mut current_worktree: Option<WorktreeInfo> = None;

        for field in output.stdout.split(|&b| b == 0) {
            if field.is_empty() {
                if let Some(wt) = current_worktree.take() {
                    worktrees.push(wt);
                }
                continue;
            }

            if let Some(rest) = field.strip_prefix(b"worktree ") {
                if let Some(wt) = current_worktree.take() {
                    worktrees.push(wt);
                }
                current_worktree = Some(WorktreeInfo {
                    path: bytes_to_path(rest),
                    branch: String::new(),
                    head: String::new(),
                    is_primary: false,
                    is_current: false,
                    is_detached: false,
                });
            } else if let Some(rest) = field.strip_prefix(b"HEAD ") {
                if let Some(ref mut wt) = current_worktree {
                    wt.head = String::from_utf8_lossy(rest).into_owned();
                }
            } else if let Some(rest) = field.strip_prefix(b"branch refs/heads/") {
                if let Some(ref mut wt) = current_worktree {
                    wt.branch = String::from_utf8_lossy(rest).into_owned();
                }
            } else if field == b"bare" {
                if let Some(ref mut wt) = current_worktree {
                    wt.is_primary = true;
                }
            } else if field == b"detached"
                && let Some(ref mut wt) = current_worktree
            {
                wt.is_detached = true;
            }
        }

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

    /// Create a new worktree and branch using a precomputed `BranchSource` classification.
    pub fn create_worktree_for_source<P: AsRef<Path>>(
        &self,
        branch_name: &str,
        worktree_path: P,
        source: &BranchSource,
    ) -> Result<()> {
        let worktree_path = worktree_path.as_ref();
        let git_worktree_path = git_compatible_path(worktree_path);

        match source {
            BranchSource::ExistingWorktree { .. } => Err(anyhow::anyhow!(
                "Cannot create worktree for branch '{branch_name}': it is already checked out in another worktree"
            )),
            BranchSource::LocalBranch => {
                let output = Command::new("git")
                    .args(["worktree", "add", "--"])
                    .arg(&git_worktree_path)
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
                    .args(["worktree", "add", "-b", branch_name, "--"])
                    .arg(&git_worktree_path)
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
                    .args(["worktree", "add", "-b", branch_name, "--"])
                    .arg(&git_worktree_path)
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
                    .args(["worktree", "add", "-b", branch_name, "--"])
                    .arg(&git_worktree_path)
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
        let worktree_path = worktree_path.as_ref();
        let git_worktree_path = git_compatible_path(worktree_path);

        // Check if branch already exists
        if self.branch_exists(branch_name)? {
            // Create worktree from existing branch
            let mut cmd = Command::new("git");
            cmd.args(["worktree", "add", "--"])
                .arg(&git_worktree_path)
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
            cmd.args(["worktree", "add", "-b", branch_name, "--"])
                .arg(&git_worktree_path)
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
            cmd.args(["worktree", "add", "-b", branch_name, "--"])
                .arg(&git_worktree_path);

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
        let worktree_path = worktree_path.as_ref();
        let git_worktree_path = git_compatible_path(worktree_path);

        let mut cmd = Command::new("git");
        cmd.args(["worktree", "remove"]);
        if force {
            cmd.arg("--force");
        }
        cmd.arg("--")
            .arg(git_worktree_path)
            .current_dir(&self.repo_path);

        let output = cmd
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to remove worktree: {}", e))?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to remove worktree: {}", error));
        }

        Ok(())
    }

    /// Prune worktrees (clean up stale references)
    pub fn prune_worktrees(&self) -> Result<()> {
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
        let sanitized_branch = sanitize_branch_name(branch_name);

        let base_path = match worktrees_path {
            Some(path) if path.is_absolute() => path.to_path_buf(),
            Some(path) => self.primary_worktree_root().join(path),
            None => self.primary_worktree_root().join("../worktrees"),
        };

        base_path.join(sanitized_branch)
    }

    pub(super) fn primary_worktree_root(&self) -> PathBuf {
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
}

/// List a worktree's untracked and ignored entries as paths relative to
/// `repo_dir`, with whole untracked/ignored directories collapsed to a single
/// entry (git's default reporting granularity).
///
/// This drives the CoW overlay: `git worktree add` reproduces tracked files but
/// not the build output and local files (`node_modules`, `.env`, caches, venvs)
/// that make a worktree usable, so those are the only entries worth cloning.
/// `.git` is never reported by `git status`, so it is inherently excluded.
pub fn list_untracked_and_ignored(repo_dir: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "-z", "--ignored"])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to list untracked files: {}", e))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    // Porcelain v1 records are `XY <path>`; `-z` NUL-separates records and
    // leaves paths *unquoted, as raw bytes*. Parse on the byte stream and
    // rebuild each path via the OS encoding so non-UTF-8 filenames (legal on
    // Linux) survive intact rather than being mangled by lossy UTF-8 decoding.
    let mut entries = Vec::new();
    for record in output.stdout.split(|&byte| byte == 0) {
        // Shortest meaningful record is `?? x` (2 status bytes, a space, ≥1 path
        // byte). `XY` is always ASCII.
        if record.len() < 4 {
            continue;
        }
        let is_untracked = record[0] == b'?' && record[1] == b'?';
        let is_ignored = record[0] == b'!' && record[1] == b'!';
        if !is_untracked && !is_ignored {
            continue;
        }
        let mut path_bytes = &record[3..];
        // git appends a trailing '/' to collapsed directory entries.
        if path_bytes.last() == Some(&b'/') {
            path_bytes = &path_bytes[..path_bytes.len() - 1];
        }
        if path_bytes.is_empty() {
            continue;
        }
        let path = bytes_to_path(path_bytes);
        if path == Path::new(".git") {
            continue;
        }
        entries.push(path);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::super::GitRepository;
    use super::list_untracked_and_ignored;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::tempdir;

    #[test]
    fn test_list_untracked_and_ignored_collapses_dirs_and_skips_tracked() {
        let temp = tempdir().unwrap();
        let repo = temp.path();

        Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@e.com"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(repo)
            .output()
            .unwrap();

        std::fs::write(repo.join(".gitignore"), "node_modules/\n*.log\n").unwrap();
        std::fs::write(repo.join("tracked.txt"), "x").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "init"])
            .current_dir(repo)
            .output()
            .unwrap();

        // ignored directory (collapses to a single entry), ignored file, and an
        // untracked file.
        std::fs::create_dir_all(repo.join("node_modules/pkg")).unwrap();
        std::fs::write(repo.join("node_modules/pkg/a.js"), "y").unwrap();
        std::fs::write(repo.join("debug.log"), "z").unwrap();
        std::fs::write(repo.join(".env"), "SECRET=1").unwrap();

        let entries = list_untracked_and_ignored(repo).unwrap();

        assert!(
            entries.contains(&PathBuf::from("node_modules")),
            "ignored dir collapses to one entry: {entries:?}"
        );
        assert!(entries.contains(&PathBuf::from("debug.log")), "{entries:?}");
        assert!(entries.contains(&PathBuf::from(".env")), "{entries:?}");
        assert!(
            !entries.contains(&PathBuf::from("tracked.txt")),
            "tracked files must not be overlaid: {entries:?}"
        );
        assert!(
            !entries
                .iter()
                .any(|e| e != Path::new("node_modules") && e.starts_with("node_modules")),
            "whole ignored dir must not be expanded to files: {entries:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_list_worktrees_roundtrips_newline_in_path() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir(&repo).unwrap();

        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.email", "t@e.com"][..],
            &["config", "user.name", "T"][..],
            &["commit", "--allow-empty", "-q", "-m", "init"][..],
        ] {
            let out = Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        }

        // A directory whose name contains a literal newline — legal on Unix,
        // and the exact case the pre-`-z` parser split across two bogus rows.
        let odd_dir = temp.path().join("odd\nname");
        std::fs::create_dir(&odd_dir).unwrap();
        let wt_path = odd_dir.join("wt");

        let out = Command::new("git")
            .args(["worktree", "add", "-b", "newline-test"])
            .arg(&wt_path)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git worktree add: {}",
            String::from_utf8_lossy(&out.stderr)
        );

        let repo_obj = GitRepository::open(&repo).unwrap();
        let worktrees = repo_obj.list_worktrees().unwrap();

        let matched: Vec<_> = worktrees
            .iter()
            .filter(|wt| wt.branch == "newline-test")
            .collect();
        assert_eq!(
            matched.len(),
            1,
            "expected one newline-test worktree, got: {worktrees:?}"
        );
        // Compare canonicalized paths — `git worktree add` records the
        // real path, which may differ from the input on macOS (`/private` prefix).
        let want = wt_path.canonicalize().unwrap();
        let got = matched[0].path.canonicalize().unwrap();
        assert_eq!(got, want, "worktree path must round-trip newline exactly");
    }
}
