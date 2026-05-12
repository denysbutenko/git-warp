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
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
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
    let tmp_name = format!(
        "{}.tmp.{}.{}",
        file_name.to_string_lossy(),
        process::id(),
        nanos
    );
    let tmp_path = parent.join(tmp_name);

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp_path)?;

    if let Err(e) = file.write_all(contents).and_then(|_| file.sync_all()) {
        drop(file);
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    drop(file);

    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
