use crate::error::{GitWarpError, Result};
use std::path::Path;

pub(super) fn is_cow_supported<P: AsRef<Path>>(path: P) -> Result<bool> {
    use nix::sys::statfs::statfs;

    let statfs =
        statfs(path.as_ref()).map_err(|e| anyhow::anyhow!("Failed to check filesystem: {}", e))?;

    // APFS filesystem type name
    let fs_type = statfs.filesystem_type_name();
    Ok(fs_type == "apfs")
}

pub(super) fn clone_directory(src: &Path, dest: &Path, exclude: Option<&Path>) -> Result<()> {
    // Ensure we're on APFS
    if !is_cow_supported(src)? {
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

pub(super) fn clone_entry(src: &Path, dst: &Path) -> Result<()> {
    cp_clone(src, dst)
}

/// Recursively CoW-clone `from` to `to`.
///
/// One `clonefile(2)` call clones a whole directory tree in the kernel, which
/// is far faster than `cp -c -R` cloning file by file (2s vs 28s for a
/// 127k-file `node_modules`). `cp` stays as the fallback for what `clonefile`
/// refuses: an existing destination, another volume, a non-APFS filesystem.
fn cp_clone(from: &Path, to: &Path) -> Result<()> {
    use std::process::Command;

    let dest_existed = std::fs::symlink_metadata(to).is_ok();
    match clonefile_tree(from, to) {
        Ok(()) => return Ok(()),
        Err(e) => {
            log::debug!(
                "clonefile {} -> {} failed ({e}); falling back to cp -c -R",
                from.display(),
                to.display()
            );
            // A failed directory clone can leave a partial tree behind, and
            // `cp -R` into an existing directory would nest inside it.
            if !dest_existed {
                let _ = super::remove_path(to);
            }
        }
    }

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

/// Clone `from` (file, directory tree, or symlink) to the absent path `to`
/// with a single `clonefile(2)` call.
pub(super) fn clonefile_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::{CString, c_char, c_int};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        // <sys/clonefile.h>, exported by libSystem.
        fn clonefile(src: *const c_char, dst: *const c_char, flags: u32) -> c_int;
    }
    const CLONE_NOFOLLOW: u32 = 0x0001;

    let src = CString::new(from.as_os_str().as_bytes())?;
    let dst = CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both arguments are NUL-terminated strings that outlive the call.
    if unsafe { clonefile(src.as_ptr(), dst.as_ptr(), CLONE_NOFOLLOW) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::clonefile_tree;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn clonefile_tree_clones_a_directory_in_one_call() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("node_modules");
        fs::create_dir_all(src.join("pkg/lib")).unwrap();
        fs::write(src.join("pkg/lib/index.js"), "module.exports = 1;\n").unwrap();
        std::os::unix::fs::symlink("pkg", src.join("alias")).unwrap();
        let dst = temp.path().join("clone");

        clonefile_tree(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(dst.join("pkg/lib/index.js")).unwrap(),
            "module.exports = 1;\n"
        );
        assert_eq!(
            fs::read_link(dst.join("alias")).unwrap(),
            PathBuf::from("pkg")
        );
    }

    #[test]
    fn clonefile_tree_refuses_an_existing_destination() {
        let temp = tempdir().unwrap();
        let src = temp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        let dst = temp.path().join("dst");
        fs::create_dir_all(&dst).unwrap();

        let err = clonefile_tree(&src, &dst).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }
}
