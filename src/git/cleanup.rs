use super::branches::is_protected_branch;
use super::{
    BranchStatus, CleanupAnalysis, CleanupSkip, CleanupSkipReason, GitRepository, WorktreeInfo,
};
use crate::config::GitConfig;
use crate::error::Result;
use rayon::prelude::*;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn git_output(repo_path: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn git_stdout(repo_path: &Path, args: &[&str]) -> Option<String> {
    git_output(repo_path, args).map(|stdout| String::from_utf8_lossy(&stdout).trim().to_string())
}

fn resolve_commit(repo_path: &Path, rev: &str) -> Option<String> {
    git_stdout(
        repo_path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
    )
}

/// Commits a branch must be merged into to count as merged, and the branches
/// among `branches` they come from. Each branch counts locally and on
/// `origin`, which a fetch moves as soon as a PR merges while the local branch
/// waits for a pull.
fn merge_targets(repo_path: &Path, branches: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut targets = Vec::new();
    let mut found = Vec::new();
    for branch in branches {
        for rev in [
            format!("refs/heads/{branch}"),
            format!("refs/remotes/origin/{branch}"),
        ] {
            let Some(oid) = resolve_commit(repo_path, &rev) else {
                continue;
            };
            if !found.iter().any(|name| name == branch) {
                found.push(branch.to_string());
            }
            if !targets.contains(&oid) {
                targets.push(oid);
            }
        }
    }
    (targets, found)
}

/// Whitespace-preserving patch ids of the patches `git <diff_args>` prints,
/// with `input` on its stdin. The patches go straight into `git patch-id`
/// through a pipe, so a long history of large diffs never sits in memory.
fn patch_ids(repo_path: &Path, diff_args: &[&str], input: Option<&[u8]>) -> Option<Vec<String>> {
    let mut diff = Command::new("git")
        .args(diff_args)
        .current_dir(repo_path)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let patches = diff.stdout.take()?;
    let diff_stdin = diff.stdin.take();
    let ids = Command::new("git")
        .args(["patch-id", "--verbatim"])
        .current_dir(repo_path)
        .stdin(patches)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|patch_id| {
            // Feed stdin from another thread so full pipes cannot deadlock.
            std::thread::scope(|scope| {
                if let (Some(mut diff_stdin), Some(input)) = (diff_stdin, input) {
                    scope.spawn(move || diff_stdin.write_all(input));
                }
                patch_id.wait_with_output()
            })
        });
    let diff_succeeded = diff.wait().is_ok_and(|status| status.success());
    let ids = ids
        .ok()
        .filter(|output| diff_succeeded && output.status.success())?;
    Some(
        String::from_utf8_lossy(&ids.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(str::to_string))
            .collect(),
    )
}

/// Whether every change on the `branch` commit is already in `target`.
///
/// Ancestry covers fast-forward and merge-commit merges. Squash and rebase
/// merges leave the branch's own commits off `target`, so patch ids of the
/// commits `target` gained since they forked are compared with the branch's
/// whole diff (a squash merge) and with each branch commit (a rebase merge).
/// A match means `target` got exactly these changes, even if later commits
/// reworked them. The ids keep whitespace, so a whitespace-only commit made
/// after the merge still counts as unmerged work. They do ignore line
/// numbers: the same edit made independently at another spot of a file with
/// repeated blocks would match too.
///
/// A squash commit whose diff has other context lines, because the PR was
/// rebased onto nearby changes, does not match and the branch is kept. Any
/// failure, such as a pathspec list too long for one command, also keeps it.
///
/// Both arguments are commit ids, so none can be parsed as an option.
fn is_merged_into(repo_path: &Path, branch: &str, target: &str) -> bool {
    let is_ancestor = Command::new("git")
        .args(["merge-base", "--is-ancestor", branch, target])
        .current_dir(repo_path)
        .output()
        .is_ok_and(|output| output.status.success());
    if is_ancestor {
        return true;
    }

    let Some(merge_base) = git_stdout(repo_path, &["merge-base", branch, target]) else {
        return false;
    };
    let diff = ["diff-tree", "-r", "--no-renames"];
    let Some(branch_id) = patch_ids(
        repo_path,
        &[&diff[..], &["-p", &merge_base, branch]].concat(),
        None,
    )
    .and_then(|ids| ids.into_iter().next()) else {
        // No net change: nothing to find on `target`.
        return false;
    };
    let Some(paths) = git_output(
        repo_path,
        &[&diff[..], &["--name-only", "-z", &merge_base, branch]].concat(),
    ) else {
        return false;
    };
    let paths = String::from_utf8_lossy(&paths);
    let paths: Vec<&str> = paths.split('\0').filter(|path| !path.is_empty()).collect();

    // Only commits touching the branch's paths can match, and only those
    // paths of each commit matter.
    let range = format!("{merge_base}..{target}");
    let rev_list = [
        &[
            "--literal-pathspecs",
            "rev-list",
            "--no-merges",
            &range,
            "--",
        ][..],
        &paths,
    ]
    .concat();
    let Some(commits) = git_output(repo_path, &rev_list) else {
        return false;
    };
    let target_patches = [
        &["--literal-pathspecs"][..],
        &diff,
        &["--stdin", "-p", "--"],
        &paths,
    ]
    .concat();
    let Some(target_ids) = patch_ids(repo_path, &target_patches, Some(&commits)) else {
        return false;
    };
    if target_ids.contains(&branch_id) {
        return true;
    }

    // A rebase merge copies each branch commit onto `target` instead. A merge
    // commit on the branch can carry changes no copied commit has, so a
    // branch with merges relies on the whole-diff check alone.
    let branch_range = format!("{merge_base}..{branch}");
    let has_merges = git_stdout(
        repo_path,
        &["rev-list", "--min-parents=2", "--count", &branch_range],
    )
    .is_none_or(|count| count != "0");
    if has_merges {
        return false;
    }
    let Some(branch_commits) = git_output(repo_path, &["rev-list", &branch_range]) else {
        return false;
    };
    let Some(branch_ids) = patch_ids(
        repo_path,
        &[&diff[..], &["--stdin", "-p"]].concat(),
        Some(&branch_commits),
    ) else {
        return false;
    };
    !branch_ids.is_empty() && branch_ids.iter().all(|id| target_ids.contains(id))
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
        enum CleanupEntry {
            Candidate(BranchStatus),
            Skip(CleanupSkip),
        }

        let cleanup_base_branch = self.cleanup_base_branch(worktrees, configured_default_branch)?;
        let remotes = self.load_branch_remotes()?;
        let repo_path: &Path = &self.repo_path;
        let cleanup_base_branch_ref: &str = &cleanup_base_branch;
        let remotes_ref = &remotes;
        // PRs can land in any long-lived branch, not only the base: gitflow
        // repos merge into `develop` while `origin/HEAD` names `main`.
        // Candidates are never the base or protected, so none is checked
        // against itself.
        let mut merge_branches = vec![cleanup_base_branch_ref];
        for protected in protected_branches {
            let protected = protected.trim();
            if !protected.is_empty() && !merge_branches.contains(&protected) {
                merge_branches.push(protected);
            }
        }
        let (merge_targets, merge_branches) = merge_targets(repo_path, &merge_branches);
        let merge_targets_ref = &merge_targets;

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

                let head =
                    resolve_commit(repo_path, &format!("refs/heads/{branch}")).unwrap_or_default();
                let is_merged = !head.is_empty()
                    && merge_targets_ref
                        .iter()
                        .any(|target| is_merged_into(repo_path, &head, target));

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
                    head,
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
            merge_branches,
        })
    }
}
