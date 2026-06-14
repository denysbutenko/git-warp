use thiserror::Error;

pub type Result<T> = anyhow::Result<T>;

#[derive(Error, Debug)]
pub enum GitWarpError {
    #[error("Not in a git repository")]
    NotInGitRepository,

    #[error("Worktree '{path}' not found")]
    WorktreeNotFound { path: String },

    // Constructed only on the macOS APFS path and the non-CoW fallback; never
    // on Linux, where the reflink probe reports support via a bool instead.
    #[cfg_attr(target_os = "linux", allow(dead_code))]
    #[error("Copy-on-Write is not supported on this filesystem")]
    CoWNotSupported,

    #[error("Terminal integration not supported on this platform")]
    TerminalNotSupported,

    #[error("Configuration error: {message}")]
    ConfigError { message: String },
}
