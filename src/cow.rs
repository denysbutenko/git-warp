use crate::error::{GitWarpError, Result};
use std::path::{Path, PathBuf};

/// Check if Copy-on-Write is supported for the given path.
///
/// Resolves to the nearest existing ancestor first so callers can probe a
/// not-yet-created worktree path without the underlying filesystem syscalls
/// failing with `ENOENT`.
pub fn is_cow_supported<P: AsRef<Path>>(path: P) -> Result<bool> {
    let probe = nearest_existing_ancestor(path.as_ref());

    #[cfg(target_os = "macos")]
    {
        is_apfs(&probe)
    }

    #[cfg(target_os = "linux")]
    {
        linux::is_supported(&probe)
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
// Part of the whole-tree `clone_directory` API, which the `warp` bin no longer
// calls (it uses the untracked overlay); still exercised by tests/benches.
#[allow(dead_code)]
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
#[allow(dead_code)] // Whole-tree clone helper; bin uses the untracked overlay.
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
#[allow(dead_code)]
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
        clone_directory_apfs(src, dest, exclude.as_deref())
    }

    #[cfg(target_os = "linux")]
    {
        clone_directory_reflink(src, dest, exclude.as_deref())
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
        cp_clone(src, dst)
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

#[cfg(target_os = "macos")]
fn is_apfs<P: AsRef<Path>>(path: P) -> Result<bool> {
    use nix::sys::statfs::statfs;

    let statfs =
        statfs(path.as_ref()).map_err(|e| anyhow::anyhow!("Failed to check filesystem: {}", e))?;

    // APFS filesystem type name
    let fs_type = statfs.filesystem_type_name();
    Ok(fs_type == "apfs")
}

#[cfg(target_os = "linux")]
mod linux {
    use crate::error::Result;
    use std::collections::HashMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    // FICLONE = _IOW(0x94, 9, int): clones the source fd (passed as the int
    // arg) into the destination fd. Available since Linux 4.5 on filesystems
    // that implement reflink (btrfs, xfs with reflink=1, bcachefs, etc.).
    nix::ioctl_write_int!(ficlone, 0x94, 9);

    static CACHE: OnceLock<Mutex<HashMap<u64, bool>>> = OnceLock::new();

    fn cache() -> &'static Mutex<HashMap<u64, bool>> {
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn is_supported(path: &Path) -> Result<bool> {
        let temp_dir: PathBuf = if path.is_dir() {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(Path::new(".")).to_path_buf()
        };

        let dev = match fs::metadata(&temp_dir) {
            Ok(meta) => meta.dev(),
            Err(_) => return Ok(false),
        };

        if let Some(&cached) = cache().lock().unwrap().get(&dev) {
            return Ok(cached);
        }

        let result = probe(&temp_dir);
        cache().lock().unwrap().insert(dev, result);
        Ok(result)
    }

    fn probe(temp_dir: &Path) -> bool {
        let mut src = match tempfile::Builder::new()
            .prefix(".git-warp-reflink-")
            .tempfile_in(temp_dir)
        {
            Ok(f) => f,
            Err(_) => return false,
        };
        if src.write_all(b"x").is_err() {
            return false;
        }
        let dst = match tempfile::Builder::new()
            .prefix(".git-warp-reflink-")
            .tempfile_in(temp_dir)
        {
            Ok(f) => f,
            Err(_) => return false,
        };

        clone_file_fd(src.as_file(), dst.as_file()).is_ok()
    }

    fn clone_file_fd(src: &File, dst: &File) -> nix::Result<nix::libc::c_int> {
        // SAFETY: caller passes open regular files on the same filesystem.
        // FICLONE takes the destination fd as the ioctl target and the source
        // fd (cast to the ioctl integer arg type) as its argument.
        unsafe { ficlone(dst.as_raw_fd(), src.as_raw_fd() as nix::libc::c_ulong) }
    }

    /// Overlay a single entry (file, dir, or symlink) from `src` to `dst`
    /// without touching `dst`'s siblings. `dst` must not already exist; its
    /// parent is created by the caller.
    pub(super) fn clone_entry(src: &Path, dst: &Path) -> Result<()> {
        clone_tree(src, dst, None)
    }

    // Whole-tree reflink clone; reached only through the lib's `clone_directory`
    // (tests/benches), not the `warp` bin, which overlays untracked files.
    #[allow(dead_code)]
    pub(super) fn clone_directory(src: &Path, dst: &Path, exclude: Option<&Path>) -> Result<()> {
        if dst.exists() {
            fs::remove_dir_all(dst)?;
        }
        // Canonicalize so the caller-supplied `exclude` (a canonical top-level
        // child of `src`) matches the paths produced while walking the tree.
        let src_root = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
        clone_tree(&src_root, dst, exclude)
    }

    fn clone_tree(src: &Path, dst: &Path, exclude: Option<&Path>) -> Result<()> {
        let meta = fs::symlink_metadata(src).map_err(|e| {
            anyhow::anyhow!("Failed to stat {} for reflink clone: {}", src.display(), e)
        })?;
        let file_type = meta.file_type();

        if file_type.is_symlink() {
            let target = fs::read_link(src).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to read symlink {} for reflink clone: {}",
                    src.display(),
                    e
                )
            })?;
            std::os::unix::fs::symlink(&target, dst).map_err(|e| {
                anyhow::anyhow!(
                    "Failed to recreate symlink {} -> {}: {}",
                    dst.display(),
                    target.display(),
                    e
                )
            })?;
            return Ok(());
        }

        if file_type.is_dir() {
            fs::create_dir(dst).map_err(|e| {
                anyhow::anyhow!("Failed to create directory {}: {}", dst.display(), e)
            })?;
            for entry in fs::read_dir(src)
                .map_err(|e| anyhow::anyhow!("Failed to read directory {}: {}", src.display(), e))?
            {
                let entry = entry.map_err(|e| {
                    anyhow::anyhow!("Failed to iterate directory {}: {}", src.display(), e)
                })?;
                if exclude == Some(entry.path().as_path()) {
                    continue;
                }
                let name = entry.file_name();
                clone_tree(&entry.path(), &dst.join(&name), exclude)?;
            }
            fs::set_permissions(dst, fs::Permissions::from_mode(meta.mode() & 0o7777)).map_err(
                |e| {
                    anyhow::anyhow!(
                        "Failed to set permissions on {} after clone: {}",
                        dst.display(),
                        e
                    )
                },
            )?;
            return Ok(());
        }

        if file_type.is_file() {
            return clone_regular_file(src, dst, meta.mode());
        }

        Err(anyhow::anyhow!(
            "Unsupported file type for reflink clone at {}",
            src.display()
        ))
    }

    fn clone_regular_file(src: &Path, dst: &Path, mode: u32) -> Result<()> {
        let src_file = File::open(src)
            .map_err(|e| anyhow::anyhow!("Failed to open {} for clone: {}", src.display(), e))?;
        let dst_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode & 0o7777)
            .open(dst)
            .map_err(|e| anyhow::anyhow!("Failed to create {} for clone: {}", dst.display(), e))?;

        clone_file_fd(&src_file, &dst_file).map_err(|e| {
            anyhow::anyhow!(
                "FICLONE failed for {} -> {}: {}",
                src.display(),
                dst.display(),
                e
            )
        })?;
        Ok(())
    }
}

#[cfg(target_os = "macos")]
#[allow(dead_code)] // Whole-tree clone path; bin overlays untracked files instead.
fn clone_directory_apfs(src: &Path, dest: &Path, exclude: Option<&Path>) -> Result<()> {
    // Ensure we're on APFS
    if !is_apfs(src)? {
        return Err(GitWarpError::CoWNotSupported.into());
    }

    // Remove destination if it exists
    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }

    // Fast path: nothing to exclude, clone the whole tree in one `cp` call.
    let Some(excluded) = exclude else {
        return cp_clone(src, dest);
    };

    // The destination lives inside the source, so clone each top-level child
    // individually and skip the one that leads to the destination (the worktree
    // storage directory). This preserves CoW speed (`cp -c -R` per child clones
    // recursively) while never copying the destination back into itself.
    let src_root = src.canonicalize().unwrap_or_else(|_| src.to_path_buf());
    std::fs::create_dir_all(dest)
        .map_err(|e| anyhow::anyhow!("Failed to create {}: {}", dest.display(), e))?;
    if let Ok(meta) = std::fs::metadata(&src_root) {
        let _ = std::fs::set_permissions(dest, meta.permissions());
    }

    for entry in std::fs::read_dir(&src_root)
        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", src_root.display(), e))?
    {
        let entry = entry.map_err(|e| {
            anyhow::anyhow!("Failed to read entry in {}: {}", src_root.display(), e)
        })?;
        if entry.path() == excluded {
            continue;
        }
        cp_clone(&entry.path(), &dest.join(entry.file_name()))?;
    }

    Ok(())
}

/// Recursively CoW-clone `from` to `to` via `cp -c -R` (APFS clonefile).
#[cfg(target_os = "macos")]
fn cp_clone(from: &Path, to: &Path) -> Result<()> {
    use std::process::Command;

    let output = Command::new("cp")
        .arg("-c") // Clone files (CoW) if possible
        .arg("-R") // Recursive
        .arg(from)
        .arg(to)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute cp command: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Failed to clone directory with CoW: {}",
            error
        ));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
#[allow(dead_code)] // Whole-tree clone path; bin overlays untracked files instead.
fn clone_directory_reflink(src: &Path, dest: &Path, exclude: Option<&Path>) -> Result<()> {
    linux::clone_directory(src, dest, exclude)
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_cow_support_check() {
        let result = is_cow_supported(".");
        assert!(result.is_ok());
    }

    #[test]
    fn should_overlay_entry_always_skips_dot_git() {
        assert!(!should_overlay_entry(Path::new(".git"), &[]));
        assert!(!should_overlay_entry(Path::new(".git/config"), &[]));
    }

    #[test]
    fn should_overlay_entry_honors_exclude_by_component() {
        let exclude = vec!["target".to_string(), ".next".to_string()];
        assert!(!should_overlay_entry(Path::new("target"), &exclude));
        assert!(!should_overlay_entry(
            Path::new("target/debug/foo"),
            &exclude
        ));
        // nested build dir with the same leaf name is also skipped
        assert!(!should_overlay_entry(
            Path::new("crates/inner/target"),
            &exclude
        ));
        assert!(!should_overlay_entry(Path::new(".next/cache"), &exclude));
        // trailing-slash tokens still match
        let exclude_slash = vec!["dist/".to_string()];
        assert!(!should_overlay_entry(Path::new("dist"), &exclude_slash));
    }

    #[test]
    fn should_overlay_entry_keeps_normal_untracked() {
        let exclude = vec!["target".to_string()];
        assert!(should_overlay_entry(Path::new("node_modules"), &exclude));
        assert!(should_overlay_entry(Path::new(".env"), &exclude));
        assert!(should_overlay_entry(
            Path::new("node_modules/lodash/index.js"),
            &exclude
        ));
    }

    #[test]
    fn overlay_into_merges_missing_children_without_clobbering_tracked() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("primary");
        let dest = tmp.path().join("worktree");
        // Primary's `assets/` is fully untracked here (git would collapse it to
        // one entry): a local note plus generated data.
        fs::create_dir_all(src.join("assets")).unwrap();
        fs::write(src.join("assets/logo.png"), b"local-logo").unwrap();
        fs::write(src.join("assets/notes.txt"), b"local-notes").unwrap();
        // On the NEW branch `assets/` is tracked, so `git worktree add` already
        // laid down a checked-out file there.
        fs::create_dir_all(dest.join("assets")).unwrap();
        fs::write(dest.join("assets/style.css"), b"tracked").unwrap();

        if !is_cow_supported(&src).unwrap_or(false) {
            return;
        }

        let created = overlay_into(&src.join("assets"), &dest.join("assets")).unwrap();

        // Untracked children were overlaid...
        assert_eq!(
            fs::read(dest.join("assets/logo.png")).unwrap(),
            b"local-logo"
        );
        assert_eq!(
            fs::read(dest.join("assets/notes.txt")).unwrap(),
            b"local-notes"
        );
        // ...the tracked checkout survived untouched...
        assert_eq!(fs::read(dest.join("assets/style.css")).unwrap(), b"tracked");
        // ...and only the freshly created paths are returned for rewriting.
        assert!(created.iter().any(|p| p.ends_with("assets/logo.png")));
        assert!(created.iter().any(|p| p.ends_with("assets/notes.txt")));
        assert!(!created.iter().any(|p| p.ends_with("assets/style.css")));
    }

    #[test]
    fn overlay_into_clones_whole_entry_when_dest_absent() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("primary");
        let dest = tmp.path().join("worktree");
        fs::create_dir_all(src.join("node_modules/pkg")).unwrap();
        fs::write(src.join("node_modules/pkg/i.js"), b"dep").unwrap();
        fs::create_dir_all(&dest).unwrap();

        if !is_cow_supported(&src).unwrap_or(false) {
            return;
        }

        let created = overlay_into(&src.join("node_modules"), &dest.join("node_modules")).unwrap();
        assert_eq!(
            fs::read(dest.join("node_modules/pkg/i.js")).unwrap(),
            b"dep"
        );
        assert_eq!(created, vec![dest.join("node_modules")]);
    }

    #[cfg(unix)]
    #[test]
    fn overlay_into_recreates_dangling_symlink() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("primary");
        let dest = tmp.path().join("worktree");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dest).unwrap();
        // A symlink whose target does not exist — `Path::exists()` would say no.
        std::os::unix::fs::symlink("/nonexistent/target", src.join("link")).unwrap();

        if !is_cow_supported(&src).unwrap_or(false) {
            return;
        }

        overlay_into(&src.join("link"), &dest.join("link")).unwrap();
        let meta = fs::symlink_metadata(dest.join("link")).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "dangling symlink must survive"
        );
        assert_eq!(
            fs::read_link(dest.join("link")).unwrap(),
            Path::new("/nonexistent/target")
        );
    }

    #[test]
    fn clone_entry_overlays_without_touching_siblings() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("primary");
        let dest = tmp.path().join("worktree");
        fs::create_dir_all(src.join("node_modules/pkg")).unwrap();
        fs::write(src.join("node_modules/pkg/index.js"), b"module").unwrap();
        // `dest` already exists (as if `git worktree add` created it) and has a
        // sibling that must survive the overlay.
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("tracked.txt"), b"checked out").unwrap();

        if !is_cow_supported(&src).unwrap_or(false) {
            return;
        }

        clone_entry(&src.join("node_modules"), &dest.join("node_modules"))
            .expect("overlay must succeed");

        assert_eq!(
            fs::read(dest.join("node_modules/pkg/index.js")).unwrap(),
            b"module"
        );
        // The pre-existing tracked file is untouched.
        assert_eq!(fs::read(dest.join("tracked.txt")).unwrap(), b"checked out");
    }

    #[test]
    fn nested_dest_exclusion_flags_worktree_storage_child() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("repo");
        fs::create_dir_all(src.join(".worktrees")).unwrap();
        let dest = src.join(".worktrees").join("feature-x");

        let excluded =
            nested_dest_exclusion(&src, &dest).expect("dest inside src must be excluded");

        assert_eq!(excluded, src.canonicalize().unwrap().join(".worktrees"));
    }

    #[test]
    fn nested_dest_exclusion_flags_direct_child() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("repo");
        fs::create_dir_all(&src).unwrap();
        let dest = src.join("feature-x");

        let excluded =
            nested_dest_exclusion(&src, &dest).expect("direct child dest must be excluded");

        assert_eq!(excluded, src.canonicalize().unwrap().join("feature-x"));
    }

    #[test]
    fn nested_dest_exclusion_none_for_external_dest() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("repo");
        fs::create_dir_all(&src).unwrap();
        // Sibling directory outside the source tree — the default git-warp layout.
        let dest = tmp.path().join("worktrees").join("feature-x");

        assert!(nested_dest_exclusion(&src, &dest).is_none());
    }

    #[test]
    fn clone_directory_skips_nested_destination() {
        let tmp = tempdir().unwrap();
        let src = tmp.path().join("repo");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::create_dir_all(src.join(".worktrees")).unwrap();
        fs::write(src.join("a.txt"), b"a").unwrap();
        fs::write(src.join("sub/inner.txt"), b"inner").unwrap();
        fs::write(src.join(".worktrees/keep.txt"), b"keep").unwrap();

        // Destination lives inside the source's worktree-storage directory —
        // the layout that previously caused an unbounded recursive copy.
        let dest = src.join(".worktrees").join("new-worktree");

        if !is_cow_supported(&src).unwrap_or(false) {
            return;
        }

        clone_directory(&src, &dest).expect("clone must succeed");

        // Regular files outside the storage dir are cloned.
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"a");
        assert_eq!(fs::read(dest.join("sub/inner.txt")).unwrap(), b"inner");
        // The worktree-storage dir (which contains the destination) is excluded,
        // so nothing recurses back into the new tree.
        assert!(
            !dest.join(".worktrees").exists(),
            "worktree storage dir must be excluded from the clone"
        );
    }

    #[test]
    fn test_cow_support_resolves_missing_path() {
        let temp_dir = tempdir().unwrap();
        let missing = temp_dir.path().join("does-not-exist-yet");
        assert!(!missing.exists());

        let probed = is_cow_supported(&missing).expect("probe should not error");
        let parent = is_cow_supported(temp_dir.path()).expect("probe should not error");
        assert_eq!(
            probed, parent,
            "missing child must yield same CoW verdict as its existing parent",
        );
    }

    #[test]
    fn test_cow_clone() {
        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dest_dir = temp_dir.path().join("dest");

        // Create source directory with content
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("test.txt"), "Hello, World!").unwrap();

        // Only test if CoW is supported
        if is_cow_supported(&src_dir).unwrap_or(false) {
            let result = clone_directory(&src_dir, &dest_dir);
            assert!(result.is_ok());

            // Verify content was copied
            assert!(dest_dir.join("test.txt").exists());
            let content = fs::read_to_string(dest_dir.join("test.txt")).unwrap();
            assert_eq!(content, "Hello, World!");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_clone_tree_via_ficlone() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempdir().unwrap();
        let src_dir = temp_dir.path().join("src");
        let dest_dir = temp_dir.path().join("dest");

        if !is_cow_supported(temp_dir.path()).unwrap_or(false) {
            return;
        }

        fs::create_dir(&src_dir).unwrap();
        fs::create_dir(src_dir.join("nested")).unwrap();
        fs::write(src_dir.join("a.txt"), b"hello").unwrap();
        fs::write(src_dir.join("nested/b.bin"), b"world").unwrap();
        fs::set_permissions(src_dir.join("a.txt"), fs::Permissions::from_mode(0o640)).unwrap();
        std::os::unix::fs::symlink("a.txt", src_dir.join("link")).unwrap();

        clone_directory(&src_dir, &dest_dir).expect("ficlone clone should succeed");

        assert_eq!(fs::read(dest_dir.join("a.txt")).unwrap(), b"hello");
        assert_eq!(fs::read(dest_dir.join("nested/b.bin")).unwrap(), b"world");
        assert!(
            fs::symlink_metadata(dest_dir.join("link"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "symlink must be preserved verbatim, not dereferenced"
        );
        assert_eq!(
            fs::read_link(dest_dir.join("link")).unwrap(),
            Path::new("a.txt")
        );
        assert_eq!(
            fs::metadata(dest_dir.join("a.txt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_linux_probe_leaves_no_trace() {
        use std::sync::Arc;
        use std::thread;

        let dir = Arc::new(tempdir().unwrap());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let dir = Arc::clone(&dir);
            handles.push(thread::spawn(move || {
                let _ = is_cow_supported(dir.path());
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        for entry in fs::read_dir(dir.path()).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.starts_with(".git-warp-reflink-") && !name.starts_with(".reflink_test_"),
                "probe leaked file: {name}"
            );
        }
    }
}
