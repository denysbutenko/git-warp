use std::path::PathBuf;

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
