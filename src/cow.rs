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
fn resolve_for_containment(path: &Path) -> PathBuf {
    let ancestor = nearest_existing_ancestor(path);
    let base = ancestor.canonicalize().unwrap_or(ancestor.clone());
    match path.strip_prefix(&ancestor) {
        Ok(tail) => base.join(tail),
        Err(_) => base,
    }
}

/// Clone a directory using Copy-on-Write
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
