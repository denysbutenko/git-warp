use crate::error::{GitWarpError, Result};
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(test)]
mod tests;

/// Check if Copy-on-Write is supported for the given path.
///
/// Resolves to the nearest existing ancestor first so callers can probe a
/// not-yet-created worktree path without the underlying filesystem syscalls
/// failing with `ENOENT`.
pub fn is_cow_supported<P: AsRef<Path>>(path: P) -> Result<bool> {
    let probe = nearest_existing_ancestor(path.as_ref());

    #[cfg(target_os = "macos")]
    {
        macos::is_cow_supported(&probe)
    }

    #[cfg(target_os = "linux")]
    {
        linux::is_cow_supported(&probe)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = probe;
        Ok(false)
    }
}

fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut candidate = path.to_path_buf();
    while !candidate.exists() {
        if !candidate.pop() {
            return PathBuf::from(".");
        }
    }
    candidate
}

/// When `dest` lives inside `src`, return the direct child of `src` that lies on
/// the path to `dest`. Cloning `src` into a location beneath itself would copy
/// that child (and everything under it, including `dest`) back into the new
/// tree, recursing without bound — this is exactly what happens when worktrees
/// are stored under `<repo>/.worktrees/<branch>` and the whole repo is CoW
/// cloned. Callers exclude the returned child from the clone.
///
/// Returns `None` when `dest` is not inside `src` (the default git-warp layout,
/// where worktrees live in a sibling `../worktrees` directory), so the fast
/// whole-tree clone path is preserved.
#[allow(dead_code)] // Whole-tree `clone_directory` helper; exercised by tests.
fn nested_dest_exclusion(src: &Path, dest: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let src = src.canonicalize().ok()?;
    let dest = resolve_for_containment(dest);
    let relative = dest.strip_prefix(&src).ok()?;
    match relative.components().next()? {
        Component::Normal(name) => Some(src.join(name)),
        _ => None,
    }
}

/// Resolve `path` to an absolute, symlink-free form for containment checks,
/// tolerating a not-yet-created leaf by canonicalizing the nearest existing
/// ancestor and re-appending the missing tail components.
#[allow(dead_code)] // Whole-tree `clone_directory` helper; exercised by tests.
fn resolve_for_containment(path: &Path) -> PathBuf {
    let ancestor = nearest_existing_ancestor(path);
    let base = ancestor.canonicalize().unwrap_or(ancestor.clone());
    match path.strip_prefix(&ancestor) {
        Ok(tail) => base.join(tail),
        Err(_) => base,
    }
}

/// Clone an entire directory tree using Copy-on-Write.
///
/// The `warp` bin creates worktrees via `git worktree add` plus [`clone_entry`]
/// overlay and no longer calls this; it remains part of the public library
/// surface and is exercised by the integration tests and benchmarks.
#[allow(dead_code)] // Public library API; exercised by tests and benches.
pub fn clone_directory<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
    let src = src.as_ref();
    let dest = dest.as_ref();

    if !src.exists() {
        return Err(GitWarpError::WorktreeNotFound {
            path: src.display().to_string(),
        }
        .into());
    }

    // Guard against cloning `src` into a location beneath itself (e.g. worktrees
    // stored under `<repo>/.worktrees/<branch>`). Copying the whole tree would
    // otherwise recurse into the destination and every sibling worktree,
    // exploding without bound.
    let exclude = nested_dest_exclusion(src, dest);

    #[cfg(target_os = "macos")]
    {
        macos::clone_directory(src, dest, exclude.as_deref())
    }

    #[cfg(target_os = "linux")]
    {
        linux::clone_directory(src, dest, exclude.as_deref())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (dest, exclude);
        Err(GitWarpError::CoWNotSupported.into())
    }
}

/// Whether a relative untracked/ignored entry should be CoW-overlaid onto a
/// freshly created worktree. Always skips `.git`, and skips any entry whose
/// path contains a component listed in `exclude` (so `target` also drops
/// `crates/foo/target`). Matching is name-based; callers list bare directory
/// or file names, not globs.
pub fn should_overlay_entry(rel: &Path, exclude: &[String]) -> bool {
    use std::path::Component;

    for component in rel.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if name == ".git" {
                return false;
            }
            if exclude.iter().any(|token| token.trim_matches('/') == name) {
                return false;
            }
        }
    }
    true
}

/// CoW-clone a single filesystem entry (regular file, directory, or symlink)
/// from `src` to `dst`, creating `dst`'s parent directories as needed.
///
/// Unlike [`clone_directory`], this overlays into an existing tree: it never
/// removes `dst`'s siblings, so it is safe to layer untracked/ignored files on
/// top of a worktree that `git worktree add` already populated.
pub fn clone_entry(src: &Path, dst: &Path) -> Result<()> {
    // `symlink_metadata` (not `exists`) so a *dangling* symlink still counts as
    // present — both backends recreate the link verbatim without resolving it.
    if std::fs::symlink_metadata(src).is_err() {
        return Err(GitWarpError::WorktreeNotFound {
            path: src.display().to_string(),
        }
        .into());
    }

    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("Failed to create {}: {}", parent.display(), e))?;
    }

    #[cfg(target_os = "macos")]
    {
        macos::clone_entry(src, dst)
    }

    #[cfg(target_os = "linux")]
    {
        linux::clone_entry(src, dst)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = dst;
        Err(GitWarpError::CoWNotSupported.into())
    }
}

/// Overlay `src` onto `dst`, copying only paths that do not already exist in
/// `dst`, and return the freshly created destination paths (so callers can
/// scope path-rewriting to exactly what was added).
///
/// - `dst` absent → the whole entry is CoW-cloned in one shot ([`clone_entry`]).
/// - both directories → children are merged recursively, so a directory that is
///   fully untracked on the primary's branch (reported by git as one collapsed
///   entry) but partly *tracked* on the new branch still contributes its
///   untracked children instead of being skipped wholesale.
/// - `dst` already a file/symlink → left untouched (never clobber a checkout).
///
/// On a clone failure the partially written destination is removed, so a failed
/// overlay leaves the worktree *without* those files rather than with a stale,
/// half-written, un-rewritten copy.
pub fn overlay_into(src: &Path, dst: &Path) -> Result<Vec<PathBuf>> {
    let src_meta = std::fs::symlink_metadata(src)
        .map_err(|e| anyhow::anyhow!("Failed to stat {}: {}", src.display(), e))?;

    match std::fs::symlink_metadata(dst) {
        // Nothing there yet: clone the whole entry, cleaning up on failure.
        Err(_) => match clone_entry(src, dst) {
            Ok(()) => Ok(vec![dst.to_path_buf()]),
            Err(e) => {
                let _ = remove_path(dst);
                Err(e)
            }
        },
        // Directory-into-directory: merge only the missing children.
        Ok(dst_meta) if src_meta.is_dir() && dst_meta.is_dir() => {
            let mut created = Vec::new();
            for entry in std::fs::read_dir(src)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", src.display(), e))?
            {
                let entry = entry.map_err(|e| {
                    anyhow::anyhow!("Failed to read entry in {}: {}", src.display(), e)
                })?;
                created.extend(overlay_into(&entry.path(), &dst.join(entry.file_name()))?);
            }
            Ok(created)
        }
        // `dst` already exists as a tracked file/symlink: leave it be.
        Ok(_) => Ok(Vec::new()),
    }
}

/// Best-effort recursive removal of whatever kind of node `path` is.
fn remove_path(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(_) => Ok(()),
    }
}
