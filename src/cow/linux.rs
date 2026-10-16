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

pub(super) fn is_cow_supported(path: &Path) -> Result<bool> {
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
        fs::create_dir(dst)
            .map_err(|e| anyhow::anyhow!("Failed to create directory {}: {}", dst.display(), e))?;
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
