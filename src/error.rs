use thiserror::Error;

pub type Result<T> = anyhow::Result<T>;

#[derive(Error, Debug)]
pub enum GitWarpError {
    #[error("Not in a git repository")]
    NotInGitRepository,

    #[error("Worktree '{path}' not found")]
    WorktreeNotFound { path: String },

    // Constructed by the lib's whole-tree `clone_directory` (APFS path) and the
    // non-CoW fallback, exercised through tests/benches. The `warp` bin reaches
    // CoW only via the untracked overlay, which never constructs this, so the
    // bin build would otherwise flag it as unconstructed.
    #[allow(dead_code)]
    #[error("Copy-on-Write is not supported on this filesystem")]
    CoWNotSupported,

    #[error("Terminal integration not supported on this platform")]
    TerminalNotSupported,

    #[error("Configuration error: {message}")]
    ConfigError { message: String },
}
