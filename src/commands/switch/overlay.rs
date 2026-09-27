use anyhow::Result;
use std::path::Path;

use crate::git::GitRepository;

/// CoW-overlay the primary worktree's untracked & ignored files onto a
/// freshly created linked worktree at `dest`.
///
/// Copies only what `git worktree add` cannot reproduce — build output and
/// local files such as `node_modules`, `.env`, and caches — and never
/// touches `.git`, tracked files, the worktree-storage directory, or names
/// listed in `exclude`. Absolute paths baked into the copied files are
/// rewritten to point at `dest`. Best-effort: a failure leaves the worktree
/// usable, just without the overlaid files.
pub(super) fn cow_overlay_untracked(
    git_repo: &GitRepository,
    dest: &Path,
    exclude: &[String],
) -> Result<()> {
    use crate::cow;
    use crate::rewrite::PathRewriter;

    let worktrees = git_repo.list_worktrees()?;
    let Some(primary) = worktrees.iter().find(|wt| wt.is_primary) else {
        return Ok(());
    };
    let primary_path = primary.path.clone();

    let entries = crate::git::list_untracked_and_ignored(&primary_path)?;
    if entries.is_empty() {
        return Ok(());
    }

    // The worktree-storage directory is the destination's parent: in a
    // nested layout (`worktrees_path` inside the repo) every sibling
    // worktree lives under it. Skipping any entry that resolves to the
    // destination's ancestors *or* into that storage directory keeps whole
    // worktrees from being copied back into `dest`. In the default sibling
    // layout the storage dir lives outside the primary, so nothing matches.
    let dest_canon = dest.canonicalize().unwrap_or_else(|_| dest.to_path_buf());
    let storage = dest_canon.parent().map(std::path::Path::to_path_buf);

    let mut copied = Vec::new();
    for rel in entries {
        if !cow::should_overlay_entry(&rel, exclude) {
            continue;
        }
        let src = primary_path.join(&rel);
        // `symlink_metadata`, not `exists`: keep dangling untracked symlinks
        // (e.g. `.env` -> a not-yet-present target) that the worktree wants.
        if std::fs::symlink_metadata(&src).is_err() {
            continue;
        }
        let src_canon = src.canonicalize().unwrap_or_else(|_| src.clone());
        if dest_canon.starts_with(&src_canon)
            || storage
                .as_deref()
                .is_some_and(|base| src_canon.starts_with(base))
        {
            continue;
        }
        // `overlay_into` skips paths git already checked out and returns the
        // freshly created paths (for scoped rewriting), cleaning up on a
        // partial clone so no stale, un-rewritten copy is left behind.
        match cow::overlay_into(&src, &dest.join(&rel)) {
            Ok(mut created) => copied.append(&mut created),
            Err(e) => log::warn!("CoW overlay of {} failed: {}", rel.display(), e),
        }
    }

    if !copied.is_empty() {
        let rewriter = PathRewriter::new(&primary_path, dest);
        if let Err(e) = rewriter.rewrite_paths_under(&copied) {
            log::warn!("Path rewriting failed: {}", e);
        }
    }

    Ok(())
}
