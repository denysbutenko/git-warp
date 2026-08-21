use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub(crate) fn not_in_git_repo_error() -> anyhow::Error {
    anyhow::anyhow!(
        "Not in a Git repository. Run this command inside a Git repository, or use `cd <repo>` first."
    )
}

/// Render a path with the user's home directory collapsed to `~` for
/// terser, less noisy output.
pub(crate) fn abbreviate_path(path: &Path) -> String {
    abbreviate_path_with_home(path, dirs::home_dir().as_deref())
}

pub(crate) fn abbreviate_path_with_home(path: &Path, home: Option<&Path>) -> String {
    let display = path.display().to_string();
    if let Some(home) = home {
        let home = home.display().to_string();
        if let Some(rest) = display.strip_prefix(&home)
            && (rest.is_empty() || rest.starts_with(std::path::is_separator))
        {
            return format!("~{}", rest);
        }
    }
    display
}

pub(crate) fn agent_monitored_paths(git_repo: &crate::git::GitRepository) -> Result<Vec<PathBuf>> {
    let mut monitored_paths = vec![git_repo.root_path().to_path_buf()];
    monitored_paths.extend(
        git_repo
            .list_worktrees()?
            .into_iter()
            .map(|worktree| worktree.path),
    );
    monitored_paths.sort();
    monitored_paths.dedup();
    Ok(monitored_paths)
}

pub(crate) fn worktree_last_touched(path: &Path) -> Option<SystemTime> {
    let metadata = std::fs::metadata(path).ok()?;

    [metadata.modified().ok(), metadata.created().ok()]
        .into_iter()
        .flatten()
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abbreviate_path_collapses_home_prefix() {
        assert_eq!(
            abbreviate_path_with_home(Path::new("/tmp/alice/repo"), Some(Path::new("/tmp/alice")),),
            "~/repo",
        );
    }

    #[test]
    fn abbreviate_path_equal_to_home_returns_tilde() {
        assert_eq!(
            abbreviate_path_with_home(Path::new("/tmp/alice"), Some(Path::new("/tmp/alice")),),
            "~",
        );
    }

    #[test]
    fn abbreviate_path_preserves_home_sibling() {
        // Regression for #236: a sibling that shares the home dir as a byte
        // prefix (e.g. `/tmp/alice-scratch/repo` when home is `/tmp/alice`)
        // must not be abbreviated to `~-scratch/repo`.
        assert_eq!(
            abbreviate_path_with_home(
                Path::new("/tmp/alice-scratch/repo"),
                Some(Path::new("/tmp/alice")),
            ),
            "/tmp/alice-scratch/repo",
        );
    }

    #[test]
    fn abbreviate_path_without_home_returns_display() {
        assert_eq!(
            abbreviate_path_with_home(Path::new("/tmp/alice/repo"), None),
            "/tmp/alice/repo",
        );
    }
}
