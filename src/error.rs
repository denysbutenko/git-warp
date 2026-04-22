use thiserror::Error;

pub type Result<T> = anyhow::Result<T>;

#[derive(Error, Debug)]
pub enum GitWarpError {
    #[error("Not in a git repository")]
    NotInGitRepository,

    #[error("Branch '{branch}' already exists")]
    BranchAlreadyExists { branch: String },

    #[error("Worktree '{path}' already exists")]
    WorktreeAlreadyExists { path: String },

    #[error("Branch '{branch}' not found")]
    BranchNotFound { branch: String },

    #[error("Worktree '{path}' not found")]
    WorktreeNotFound { path: String },

    #[error("Copy-on-Write is not supported on this filesystem")]
    CoWNotSupported,

    #[error("Failed to create worktree: {reason}")]
    WorktreeCreationFailed { reason: String },

    #[error("Terminal integration not supported on this platform")]
    TerminalNotSupported,

    #[error("No processes found in directory '{path}'")]
    NoProcessesFound { path: String },

    #[error("Failed to terminate processes: {reason}")]
    ProcessTerminationFailed { reason: String },

    #[error("Configuration error: {message}")]
    ConfigError { message: String },
}
