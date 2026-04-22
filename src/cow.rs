use crate::error::{GitWarpError, Result};
use std::path::Path;

/// Check if Copy-on-Write is supported for the given path
pub fn is_cow_supported<P: AsRef<Path>>(path: P) -> Result<bool> {
    #[cfg(target_os = "macos")]
    {
        // Check if the path is on an APFS filesystem
        is_apfs(path)
    }

    #[cfg(target_os = "linux")]
    {
        // TODO: Check for overlayfs support
        Ok(false)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(false)
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

    #[cfg(target_os = "macos")]
    {
        clone_directory_apfs(src, dest)
    }

    #[cfg(target_os = "linux")]
    {
        // TODO: Implement overlayfs cloning
        Err(GitWarpError::CoWNotSupported.into())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
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
        return Err(anyhow::anyhow!("Failed to clone directory with CoW: {}", error).into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    #[cfg(target_os = "macos")]
    fn test_cow_support_check() {
        let result = is_cow_supported(".");
        assert!(result.is_ok());
        // Result depends on whether we're on APFS
    }

    #[test]
    #[cfg(target_os = "macos")]
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
