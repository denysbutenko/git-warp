//! Crash-safe file writes via temp-file + atomic rename.
//!
//! `std::fs::write` truncates before writing, so a crash or power loss
//! between truncate and write leaves the destination empty. Use
//! [`write_atomic`] when the target is user-owned configuration that may
//! contain unrelated state we must not lose.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `contents` to `path` atomically: write to a sibling temp file,
/// `sync_all`, then rename over the target. On any failure before the
/// rename, the original file is left untouched and the temp is removed.
///
/// An existing target keeps its permission bits, so a `0600` config holding
/// a token does not come back world-readable; a new file gets the default
/// mode.
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let existing_permissions = fs::metadata(path).ok().map(|meta| meta.permissions());
    let tmp_path = sibling_temp_path(path)?;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)?;

    let written = file
        .write_all(contents)
        .and_then(|()| match existing_permissions {
            Some(permissions) => fs::set_permissions(&tmp_path, permissions),
            None => Ok(()),
        });
    if let Err(e) = written.and_then(|()| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    drop(file);

    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }

    #[cfg(unix)]
    {
        let parent = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        // Fsync the parent directory to ensure the rename is persisted.
        // We open it read-only as that is sufficient for fsync on most Unix systems.
        // ENOTSUP/EINVAL are expected on filesystems that do not support directory
        // fsync (per #102); surface anything else so a durable write is
        // distinguishable from a best-effort one that silently skipped the fsync.
        match fs::File::open(parent) {
            Ok(dir) => {
                if let Err(e) = dir.sync_all() {
                    if is_fsync_unsupported(&e) {
                        log::debug!(
                            "parent directory fsync unsupported for {}: {e}",
                            parent.display()
                        );
                    } else {
                        log::warn!(
                            "parent directory fsync failed for {}: {e}",
                            parent.display()
                        );
                    }
                }
            }
            Err(e) => {
                if is_fsync_unsupported(&e) {
                    log::debug!(
                        "could not open parent directory for fsync at {}: {e}",
                        parent.display()
                    );
                } else {
                    log::warn!(
                        "could not open parent directory for fsync at {}: {e}",
                        parent.display()
                    );
                }
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn is_fsync_unsupported(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::Unsupported | io::ErrorKind::InvalidInput
    )
}

/// Replace an existing file's contents via a sibling temp file and rename,
/// keeping the file's permission bits, without fsync.
///
/// For files inside a just-created worktree: a crash mid-switch discards the
/// worktree anyway, while `sync_all` (`F_FULLFSYNC` on macOS) costs
/// milliseconds per file across hundreds of rewrites. Keeping the mode
/// matters because the rewritten files include executable `.bin` shims.
pub fn replace_contents_unsynced(path: &Path, contents: &[u8]) -> io::Result<()> {
    let permissions = fs::metadata(path)?.permissions();
    let tmp_path = sibling_temp_path(path)?;

    let result = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)
        .and_then(|mut file| file.write_all(contents))
        .and_then(|()| fs::set_permissions(&tmp_path, permissions))
        .and_then(|()| fs::rename(&tmp_path, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp_path);
    }
    result
}

fn sibling_temp_path(path: &Path) -> io::Result<std::path::PathBuf> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(parent.join(format!(
        "{}.tmp.{}.{}",
        file_name.to_string_lossy(),
        process::id(),
        nanos
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    #[test]
    fn replace_contents_unsynced_keeps_mode_and_leaves_no_temp() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("shim");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o750)).unwrap();

        replace_contents_unsynced(&target, b"new").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o750
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn write_atomic_full_cycle() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("full_cycle.txt");
        write_atomic(&target, b"content").expect("Write should succeed");
        assert_eq!(fs::read_to_string(&target).unwrap(), "content");
    }

    #[test]
    fn writes_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");

        write_atomic(&target, b"{\"a\":1}").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, b"old").unwrap();

        write_atomic(&target, b"new bytes").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new bytes");
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_keeps_the_existing_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("config.toml");
        fs::write(&target, b"token = \"secret\"").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&target, b"token = \"rotated\"").unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"token = \"rotated\"");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn leaves_no_temp_sibling_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");

        write_atomic(&target, b"payload").unwrap();

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(entries.len(), 1, "unexpected entries: {:?}", entries);
        assert_eq!(entries[0], "settings.json");
    }

    #[cfg(unix)]
    #[test]
    fn failure_preserves_original_and_cleans_temp() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path().join("locked");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("settings.json");
        fs::write(&target, b"original").unwrap();

        let mut perms = fs::metadata(&parent).unwrap().permissions();
        perms.set_mode(0o500); // read+execute, no write
        fs::set_permissions(&parent, perms).unwrap();

        let err = write_atomic(&target, b"new").expect_err("expected write failure");
        assert!(
            matches!(err.kind(), io::ErrorKind::PermissionDenied),
            "unexpected error: {err:?}"
        );

        // Restore perms so we can read and clean up.
        let mut perms = fs::metadata(&parent).unwrap().permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&parent, perms).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"original");

        let leftovers: Vec<_> = fs::read_dir(&parent)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != "settings.json")
            .collect();
        assert!(leftovers.is_empty(), "stale temp files: {:?}", leftovers);
    }
}
