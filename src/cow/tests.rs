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

    let excluded = nested_dest_exclusion(&src, &dest).expect("dest inside src must be excluded");

    assert_eq!(excluded, src.canonicalize().unwrap().join(".worktrees"));
}

#[test]
fn nested_dest_exclusion_flags_direct_child() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("repo");
    fs::create_dir_all(&src).unwrap();
    let dest = src.join("feature-x");

    let excluded = nested_dest_exclusion(&src, &dest).expect("direct child dest must be excluded");

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

#[cfg(target_os = "macos")]
#[test]
fn clonefile_tree_clones_a_directory_in_one_call() {
    let temp = tempdir().unwrap();
    let src = temp.path().join("node_modules");
    fs::create_dir_all(src.join("pkg/lib")).unwrap();
    fs::write(src.join("pkg/lib/index.js"), "module.exports = 1;\n").unwrap();
    std::os::unix::fs::symlink("pkg", src.join("alias")).unwrap();
    let dst = temp.path().join("clone");

    macos::clonefile_tree(&src, &dst).unwrap();

    assert_eq!(
        fs::read_to_string(dst.join("pkg/lib/index.js")).unwrap(),
        "module.exports = 1;\n"
    );
    assert_eq!(
        fs::read_link(dst.join("alias")).unwrap(),
        PathBuf::from("pkg")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn clonefile_tree_refuses_an_existing_destination() {
    let temp = tempdir().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();
    let dst = temp.path().join("dst");
    fs::create_dir_all(&dst).unwrap();

    let err = macos::clonefile_tree(&src, &dst).unwrap_err();

    assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
}
