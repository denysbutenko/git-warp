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
        is_linux_reflink_supported(&probe)
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

    #[cfg(target_os = "macos")]
    {
        clone_directory_apfs(src, dest)
    }

    #[cfg(target_os = "linux")]
    {
        clone_directory_reflink(src, dest)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = dest;
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
fn is_linux_reflink_supported<P: AsRef<Path>>(path: P) -> Result<bool> {
    use std::fs;
    use std::process::Command;

    let path = path.as_ref();

    // Attempt to create a temp file in the target directory to test reflink
    let temp_dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    let test_file = temp_dir.join(".reflink_test_src");
    let test_dest = temp_dir.join(".reflink_test_dest");

    // Clean up if they somehow exist
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_file(&test_dest);

    // Create a small source file
    if fs::write(&test_file, "reflink test").is_err() {
        return Ok(false);
    }

    // Attempt reflink
    let status = Command::new("cp")
        .arg("--reflink=always")
        .arg(&test_file)
        .arg(&test_dest)
        .status();

    // Clean up
    let _ = fs::remove_file(&test_file);
    let _ = fs::remove_file(&test_dest);

    match status {
        Ok(s) => Ok(s.success()),
        Err(_) => Ok(false),
    }
}

#[cfg(target_os = "macos")]
fn clone_directory_apfs<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
    use std::process::Command;

    // Ensure we're on APFS
    if !is_apfs(&src)? {
        return Err(GitWarpError::CoWNotSupported.into());
    }

    // Remove destination if it exists
    if dest.as_ref().exists() {
        std::fs::remove_dir_all(&dest)?;
    }

    // Use cp with APFS clone flags
    let output = Command::new("cp")
        .arg("-c") // Clone files (CoW) if possible
        .arg("-R") // Recursive
        .arg(src.as_ref())
        .arg(dest.as_ref())
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
fn clone_directory_reflink<P: AsRef<Path>, Q: AsRef<Path>>(src: P, dest: Q) -> Result<()> {
    use std::process::Command;

    // Remove destination if it exists
    if dest.as_ref().exists() {
        std::fs::remove_dir_all(&dest)?;
    }

    // Use cp with Linux reflink flags
    let output = Command::new("cp")
        .arg("--reflink=always") // Force reflink (CoW)
        .arg("-R") // Recursive
        .arg(src.as_ref())
        .arg(dest.as_ref())
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to execute cp command: {}", e))?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!(
            "Failed to clone directory with CoW (reflink): {}",
            error
        ));
    }

    Ok(())
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
}
